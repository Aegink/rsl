//! T4.4 boa 引擎实现：pure / ctx 两档隔离 + ctx 十接口 + libSteps 计量 + 确定性。
//! pure 上下文不提供 Date/fetch/setTimeout（S§7.3：无时间无随机无 IO）。

use super::{CtxApi, ScriptEngine};
use crate::error::{Code, Diagnostic};
use boa_engine::js_string;
use boa_engine::{Context, JsValue, NativeFunction};
use serde_json::Value;
use std::cell::Cell;

pub struct BoaEngine {
    pure_src: String,
    ctx_src: String,
    steps: Cell<i64>,
    step_limit: i64,
}

impl BoaEngine {
    pub fn new(pure_src: &str, ctx_src: &str, step_limit: i64) -> Self {
        BoaEngine {
            pure_src: pure_src.to_string(),
            ctx_src: ctx_src.to_string(),
            steps: Cell::new(0),
            step_limit,
        }
    }

    fn check_budget(&self, path: &str) -> Result<(), Diagnostic> {
        self.steps.set(self.steps.get() + 1);
        if self.steps.get() > self.step_limit {
            return Err(Diagnostic::new(
                Code::EBudget,
                path,
                format!("libSteps {} 超上限 {}", self.steps.get(), self.step_limit),
                "提高 limits.libSteps 或简化脚本",
            ));
        }
        Ok(())
    }
}

fn js_err(src: &str, e: &boa_engine::JsError) -> Diagnostic {
    let msg = e.to_string();
    Diagnostic::new(
        Code::EScript,
        src,
        format!("JS 异常：{msg}"),
        format!("定位到 {src}；检查异常栈"),
    )
}

fn json_to_js(c: &mut Context, v: &Value) -> Result<JsValue, Diagnostic> {
    match v {
        Value::Null => Ok(JsValue::null()),
        Value::Bool(b) => Ok(JsValue::from(*b)),
        Value::Number(n) => Ok(JsValue::from(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Ok(JsValue::from(js_string!(s.clone()))),
        _ => {
            let s = serde_json::to_string(v).unwrap_or_default();
            c.eval(boa_engine::Source::from_bytes(&format!(
                "JSON.parse({})",
                serde_json::to_string(&s).unwrap_or_default()
            )))
            .map_err(|e| js_err("json", &e))
        }
    }
}

fn js_to_json(c: &mut Context, v: JsValue) -> Result<Value, Diagnostic> {
    if v.is_undefined() || v.is_null() {
        return Ok(Value::Null);
    }
    if let Some(b) = v.as_boolean() {
        return Ok(Value::Bool(b));
    }
    if let Some(n) = v.as_number() {
        let f = n as f64;
        if f.fract() == 0.0 && f.abs() < 9e15 {
            return Ok(Value::Number(serde_json::Number::from(f as i64)));
        }
        return Ok(serde_json::json!(f));
    }
    if let Some(s) = v.as_string() {
        return Ok(Value::String(s.to_std_string_escaped()));
    }
    // 对象/数组 → JSON.stringify
    let stringify = c
        .eval(boa_engine::Source::from_bytes("JSON.stringify"))
        .map_err(|e| js_err("json", &e))?;
    if let Some(f) = stringify.as_callable() {
        let s = f
            .call(&JsValue::undefined(), &[v], c)
            .map_err(|e| js_err("json", &e))?;
        if let Some(st) = s.as_string() {
            let raw = st.to_std_string_escaped();
            return serde_json::from_str(&raw).map_err(|e| {
                Diagnostic::new(
                    Code::EScript,
                    "$",
                    format!("JS 返回值 JSON 化失败：{e}"),
                    "函数应返回原始值或可序列化对象",
                )
            });
        }
    }
    Ok(Value::Null)
}

impl ScriptEngine for BoaEngine {
    fn call_pure(&self, fn_name: &str, args: &[&Value], path: &str) -> Result<Value, Diagnostic> {
        self.check_budget(path)?;
        let mut c = Context::default();
        // 确定性：移除时间/随机/IO 入口
        for name in ["Date", "fetch", "XMLHttpRequest", "setTimeout", "setInterval"] {
            let _ = c.eval(boa_engine::Source::from_bytes(&format!(
                "try {{ delete globalThis.{name}; }} catch (e) {{}}"
            )));
        }
        c.eval(boa_engine::Source::from_bytes(&self.pure_src))
            .map_err(|e| js_err("lib.pure", &e))?;
        let get_fn = format!("(typeof {fn_name} === 'function' ? {fn_name} : null)");
        let f = c
            .eval(boa_engine::Source::from_bytes(&get_fn))
            .map_err(|e| js_err("lib.pure", &e))?;
        if f.is_undefined() {
            return Err(Diagnostic::new(
                Code::EScript,
                path,
                format!("lib.pure 函数 `{fn_name}` 不存在"),
                "在 lib.pure 里定义它",
            ));
        }
        let mut argv = vec![];
        for a in args {
            argv.push(json_to_js(&mut c, a)?);
        }
        let result = match f.as_callable() {
            Some(callable) => callable
                .call(&JsValue::undefined(), &argv, &mut c)
                .map_err(|e| js_err(&format!("lib.pure.{fn_name}"), &e))?,
            None => {
                let e = boa_engine::JsError::from_native(
                    boa_engine::JsNativeError::typ().with_message(format!("{fn_name} 不可调用")),
                );
                return Err(js_err(&format!("lib.pure.{fn_name}"), &e));
            }
        };
        js_to_json(&mut c, result)
    }

    fn call_ctx(
        &self,
        fn_name: &str,
        args: &[&Value],
        host: &dyn CtxApi,
        path: &str,
    ) -> Result<Value, Diagnostic> {
        self.check_budget(path)?;
        // 安全：桥指针只在 call_ctx 同步作用域内被 JS 回调读取，返回前清空
        BRIDGE_HOST.with(|b| {
            let erased: *const (dyn CtxApi + 'static) =
                unsafe { std::mem::transmute(host as *const dyn CtxApi) };
            *b.borrow_mut() = Some(erased);
        });
        BRIDGE_ERR.with(|b| b.borrow_mut().clear());
        let mut c = Context::default();
        for name in ["Date", "fetch", "XMLHttpRequest", "setTimeout", "setInterval"] {
            let _ = c.eval(boa_engine::Source::from_bytes(&format!(
                "try {{ delete globalThis.{name}; }} catch (e) {{}}"
            )));
        }
        bind_ctx_globals(&mut c)?;
        c.eval(boa_engine::Source::from_bytes(&self.ctx_src))
            .map_err(|e| js_err("lib.ctx", &e))?;
        let get_fn = format!("(typeof {fn_name} === 'function' ? {fn_name} : null)");
        let f = c
            .eval(boa_engine::Source::from_bytes(&get_fn))
            .map_err(|e| js_err("lib.ctx", &e))?;
        if f.is_undefined() {
            return Err(Diagnostic::new(
                Code::EScript,
                path,
                format!("lib.ctx 函数 `{fn_name}` 不存在"),
                "在 lib.ctx 里定义它",
            ));
        }
        let mut argv = vec![];
        let ctx_js = c
            .eval(boa_engine::Source::from_bytes("globalThis.__ctx"))
            .map_err(|e| js_err("ctx", &e))?;
        argv.push(ctx_js);
        for a in args {
            argv.push(json_to_js(&mut c, a)?);
        }
        let result = match f.as_callable() {
            Some(callable) => callable
                .call(&JsValue::undefined(), &argv, &mut c)
                .map_err(|e| js_err(&format!("lib.ctx.{fn_name}"), &e))?,
            None => {
                let e = boa_engine::JsError::from_native(
                    boa_engine::JsNativeError::typ().with_message(format!("{fn_name} 不可调用")),
                );
                return Err(js_err(&format!("lib.ctx.{fn_name}"), &e));
            }
        };
        // 桥里记录的错误优先抛出
        let bridge_err = BRIDGE_ERR.with(|b| b.borrow().first().cloned());
        BRIDGE_HOST.with(|b| *b.borrow_mut() = None);
        if let Some(d) = bridge_err {
            return Err(d);
        }
        js_to_json(&mut c, result)
    }

    fn has_fn(&self, name: &str) -> bool {
        let pat = format!("function {name}\\s*\\(");
        regex::Regex::new(&pat)
            .map(|r| r.is_match(&self.pure_src) || r.is_match(&self.ctx_src))
            .unwrap_or(false)
    }

    fn steps_used(&self) -> i64 {
        self.steps.get()
    }
}

thread_local! {
    static BRIDGE_HOST: std::cell::RefCell<Option<*const dyn CtxApi>> = const { std::cell::RefCell::new(None) };
    static BRIDGE_ERR: std::cell::RefCell<Vec<Diagnostic>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// host 未注入时的兜底宿主（A4：读空、写丢弃、IO 报错，永不 panic）。
struct NoHost;

static NO_HOST: NoHost = NoHost;

impl CtxApi for NoHost {
    fn var(&self, _name: &str) -> Option<Value> { None }
    fn config(&self, _key: &str) -> Option<Value> { None }
    fn http_get(&self, _url: &str, _headers: &[(String, String)]) -> Result<Value, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn http_post(&self, _url: &str, _body: &str, _headers: &[(String, String)]) -> Result<Value, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn cache_get(&self, _k: &str) -> Option<Value> { None }
    fn cache_set(&self, _k: &str, _v: Value, _ttl: i64) {}
    fn session_get(&self, _k: &str) -> Option<Value> { None }
    fn session_set(&self, _k: &str, _v: Value) {}
    fn cookie_get(&self, _k: &str) -> Option<String> { None }
    fn cookie_set(&self, _k: &str, _v: &str) {}
    fn cookie_remove(&self, _k: &str) {}
    fn log(&self, _msg: &str) {}
    fn crypto_digest(&self, _alg: &str, _data: &str) -> Result<String, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn crypto_symmetric(&self, _alg: &str, _mode: &str, _key: &str, _iv: &str, _data: &str) -> Result<Value, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn now(&self) -> Result<i64, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn rand(&self) -> Result<f64, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
}

/// ctx 接口桥：全局函数 → BRIDGE_HOST 调用。
fn bind_ctx_globals(c: &mut Context) -> Result<(), Diagnostic> {
    fn with_host<T>(f: impl FnOnce(&dyn CtxApi) -> T) -> T {
        BRIDGE_HOST.with(|b| {
            match b.borrow().as_ref() {
                Some(h) => {
                    let h: &(dyn CtxApi + 'static) = unsafe { &**h };
                    f(h)
                }
                None => f(&NO_HOST),
            }
        })
    }

    fn push_err(d: Diagnostic) -> JsValue {
        BRIDGE_ERR.with(|b| b.borrow_mut().push(d));
        JsValue::from(js_string!("__RSL_ERR__"))
    }

    fn arg_str(args: &[JsValue], i: usize) -> String {
        args.get(i)
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default()
    }

    let reg = |c: &mut Context, name: &'static str, f: NativeFunction| -> Result<(), Diagnostic> {
        c.register_global_callable(js_string!(name), 0, f)
            .map_err(|e| js_err("ctx", &e))
    };

    reg(c, "__ctx_var", NativeFunction::from_copy_closure(|_t, args, _c| {
        let name = arg_str(args, 0);
        Ok(with_host(|h| h.var(&name))
            .and_then(|v| json_to_js(_c, &v).ok())
            .unwrap_or(JsValue::undefined()))
    }))?;
    reg(c, "__ctx_config", NativeFunction::from_copy_closure(|_t, args, _c| {
        let key = arg_str(args, 0);
        Ok(with_host(|h| h.config(&key))
            .and_then(|v| json_to_js(_c, &v).ok())
            .unwrap_or(JsValue::undefined()))
    }))?;
    reg(c, "__ctx_http_get", NativeFunction::from_copy_closure(|_t, args, _c| {
        let url = arg_str(args, 0);
        match with_host(|h| h.http_get(&url, &[])) {
            Ok(v) => Ok(json_to_js(_c, &v).unwrap_or(JsValue::undefined())),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    reg(c, "__ctx_http_post", NativeFunction::from_copy_closure(|_t, args, _c| {
        let url = arg_str(args, 0);
        let body = arg_str(args, 1);
        match with_host(|h| h.http_post(&url, &body, &[])) {
            Ok(v) => Ok(json_to_js(_c, &v).unwrap_or(JsValue::undefined())),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    reg(c, "__ctx_cache_get", NativeFunction::from_copy_closure(|_t, args, _c| {
        let k = arg_str(args, 0);
        Ok(with_host(|h| h.cache_get(&k))
            .and_then(|v| json_to_js(_c, &v).ok())
            .unwrap_or(JsValue::undefined()))
    }))?;
    reg(c, "__ctx_cache_set", NativeFunction::from_copy_closure(|_t, args, c| {
        let k = arg_str(args, 0);
        let v = js_to_json(c, args.get(1).cloned().unwrap_or(JsValue::undefined())).unwrap_or(Value::Null);
        with_host(|h| h.cache_set(&k, v, 3600));
        Ok(JsValue::undefined())
    }))?;
    reg(c, "__ctx_session_get", NativeFunction::from_copy_closure(|_t, args, _c| {
        let k = arg_str(args, 0);
        Ok(with_host(|h| h.session_get(&k))
            .and_then(|v| json_to_js(_c, &v).ok())
            .unwrap_or(JsValue::undefined()))
    }))?;
    reg(c, "__ctx_session_set", NativeFunction::from_copy_closure(|_t, args, c| {
        let k = arg_str(args, 0);
        let v = js_to_json(c, args.get(1).cloned().unwrap_or(JsValue::undefined())).unwrap_or(Value::Null);
        with_host(|h| h.session_set(&k, v));
        Ok(JsValue::undefined())
    }))?;
    reg(c, "__ctx_cookie_get", NativeFunction::from_copy_closure(|_t, args, _c| {
        let k = arg_str(args, 0);
        Ok(match with_host(|h| h.cookie_get(&k)) {
            Some(v) => JsValue::from(js_string!(v)),
            None => JsValue::from(js_string!("")),
        })
    }))?;
    reg(c, "__ctx_cookie_set", NativeFunction::from_copy_closure(|_t, args, _c| {
        let k = arg_str(args, 0);
        let v = arg_str(args, 1);
        with_host(|h| h.cookie_set(&k, &v));
        Ok(JsValue::undefined())
    }))?;
    reg(c, "__ctx_cookie_remove", NativeFunction::from_copy_closure(|_t, args, _c| {
        let k = arg_str(args, 0);
        with_host(|h| h.cookie_remove(&k));
        Ok(JsValue::undefined())
    }))?;
    reg(c, "__ctx_log", NativeFunction::from_copy_closure(|_t, args, _c| {
        let msg = arg_str(args, 0);
        with_host(|h| h.log(&msg));
        Ok(JsValue::undefined())
    }))?;
    reg(c, "__ctx_crypto", NativeFunction::from_copy_closure(|_t, args, _c| {
        let alg = arg_str(args, 0);
        let data = arg_str(args, 1);
        match with_host(|h| h.crypto_digest(&alg, &data)) {
            Ok(v) => Ok(JsValue::from(js_string!(v))),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    reg(c, "__ctx_crypto_sym", NativeFunction::from_copy_closure(|_t, args, _c| {
        let alg = arg_str(args, 0);
        let mode = arg_str(args, 1);
        let key = arg_str(args, 2);
        let iv = arg_str(args, 3);
        let data = arg_str(args, 4);
        match with_host(|h| h.crypto_symmetric(&alg, &mode, &key, &iv, &data)) {
            Ok(v) => Ok(json_to_js(_c, &v).unwrap_or(JsValue::undefined())),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    reg(c, "__ctx_now", NativeFunction::from_copy_closure(|_t, _args, _c| {
        match with_host(|h| h.now()) {
            Ok(v) => Ok(JsValue::from(v as f64)),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    reg(c, "__ctx_rand", NativeFunction::from_copy_closure(|_t, _args, _c| {
        match with_host(|h| h.rand()) {
            Ok(v) => Ok(JsValue::from(v)),
            Err(d) => Ok(push_err(d)),
        }
    }))?;
    c.eval(boa_engine::Source::from_bytes(
        r#"
globalThis.__ctx = {
  var: (n) => __ctx_var(n),
  config: (k) => __ctx_config(k),
  http: {
    get: (u, o) => __ctx_http_get(u, o),
    post: (u, b, o) => __ctx_http_post(u, b, o),
    json: (u, o) => { var t = __ctx_http_get(u, o); try { return JSON.parse(t); } catch (e) { return t; } },
  },
  cache: { get: (k) => __ctx_cache_get(k), set: (k, v, t) => __ctx_cache_set(k, v, t) },
  session: { get: (k) => __ctx_session_get(k), set: (k, v) => __ctx_session_set(k, v) },
  cookie: { get: (k) => __ctx_cookie_get(k), set: (k, v) => __ctx_cookie_set(k, v), remove: (k) => __ctx_cookie_remove(k) },
  log: (m) => __ctx_log(m),
  crypto: {
    md5: (s) => __ctx_crypto('md5', s),
    sha1: (s) => __ctx_crypto('sha1', s),
    sha256: (s) => __ctx_crypto('sha256', s),
    hmac: (alg, k, s) => __ctx_crypto('hmac-' + alg, k + '|' + s),
    sym: (alg, mode, key, iv, data) => __ctx_crypto_sym(alg, mode, key, iv, data),
  },
  now: () => __ctx_now(),
  rand: () => __ctx_rand(),
};
"#,
    ))
    .map_err(|e| js_err("ctx", &e))?;
    Ok(())
}

/// libSteps 消耗查询（供测试）。
pub fn engine_steps(e: &BoaEngine) -> i64 {
    e.steps.get()
}
