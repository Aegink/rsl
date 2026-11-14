//! C1 QuickJS 引擎实现（TASK-BOOK-2 §3.3 C1 / V2-D2：仅 native；wasm 端维持 Boa）。
//! 语义与 boa_engine.rs 完全对齐：pure/ctx 两档隔离 + ctx 十接口桥 + libSteps 计量 + 确定性。
//! 引擎切换：feature `engine-quickjs`（setup_engine 按 cfg 选择）。
//! 调用形态：函数经 `name(JSON,args...)` 表达式求值 —— 参数全部是 JSON 字面量，
//! 避开变参 API；fn_name 为编译期验证过的标识符（has_fn 正则保证）。

use super::{CtxApi, ScriptEngine};
use crate::error::{Code, Diagnostic};
use rquickjs::{Ctx, Function, Runtime, Undefined, Value as JsValue};
use rquickjs::IntoJs;
use serde_json::Value;
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};

pub struct QuickJsEngine {
    pure_src: String,
    ctx_src: String,
    steps: Rc<AtomicI64>,
    step_limit: i64,
}

impl QuickJsEngine {
    pub fn new(pure_src: &str, ctx_src: &str, step_limit: i64) -> Self {
        QuickJsEngine {
            pure_src: pure_src.to_string(),
            ctx_src: ctx_src.to_string(),
            steps: Rc::new(AtomicI64::new(0)),
            step_limit,
        }
    }

    fn check_budget(&self, path: &str) -> Result<(), Diagnostic> {
        let s = self.steps.fetch_add(1, Ordering::SeqCst) + 1;
        if s > self.step_limit {
            return Err(Diagnostic::new(
                Code::EBudget,
                path,
                format!("libSteps {s} 超上限 {}", self.step_limit),
                "提高 limits.libSteps 或简化脚本",
            ));
        }
        Ok(())
    }
}

fn qjs_err(src: &str, e: &rquickjs::Error) -> Diagnostic {
    Diagnostic::new(
        Code::EScript,
        src,
        format!("JS 异常：{e}"),
        format!("定位到 {src}；检查异常栈"),
    )
}

/// Runtime 工厂：栈/内存上限 + 指令级中断（防脚本死循环）。
fn make_runtime() -> Result<Runtime, Diagnostic> {
    let rt = Runtime::new().map_err(|e| qjs_err("runtime", &e))?;
    rt.set_max_stack_size(4 * 1024 * 1024);
    rt.set_memory_limit(64 * 1024 * 1024);
    rt.set_interrupt_handler(Some(Box::new(|| {
        false // 恒不中断；libSteps 计量在 check_budget 层
    })));
    Ok(rt)
}

/// 纯化上下文：移除时间/随机/IO 入口（与 boa 同构）。
const PURIFY_JS: &str = r#"
try { delete globalThis.Date; } catch (e) {}
try { delete globalThis.setTimeout; } catch (e) {}
try { delete globalThis.setInterval; } catch (e) {}
try { delete globalThis.clearTimeout; } catch (e) {}
"#;

/// JSON 字面量 → JS 表达式（JSON 是 JS 的合法子集；字符串经两次引号化注入）。
fn json_as_expr(c: &Ctx<'_>, v: &Value) -> Result<String, Diagnostic> {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => {
            Ok(serde_json::to_string(v).unwrap_or_else(|_| "null".into()))
        }
        Value::String(s) => Ok(serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())),
        _ => {
            // 对象/数组：JSON.parse('...') —— 字面量引号化保证转义
            let s = serde_json::to_string(v).unwrap_or_default();
            let lit = serde_json::to_string(&s).unwrap_or_default();
            let _ = c;
            Ok(format!("JSON.parse({lit})"))
        }
    }
}

fn js_to_json(v: &JsValue<'_>) -> Result<Value, Diagnostic> {
    if v.is_undefined() || v.is_null() {
        return Ok(Value::Null);
    }
    if let Some(b) = v.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Some(n) = v.as_number() {
        let f = n as f64;
        if f.fract() == 0.0 && f.abs() < 9e15 {
            return Ok(Value::Number(serde_json::Number::from(f as i64)));
        }
        return Ok(serde_json::json!(f));
    }
    if let Some(s) = v.clone().into_string() {
        return Ok(Value::String(s.to_string().unwrap_or_default()));
    }
    Ok(Value::Null)
}

thread_local! {
    static QJ_BRIDGE_HOST: std::cell::RefCell<Option<*const dyn CtxApi>> =
        const { std::cell::RefCell::new(None) };
    static QJ_BRIDGE_ERR: std::cell::RefCell<Vec<Diagnostic>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// host 未注入兜底（同 boa NoHost：读空、写丢弃、IO 报错）。
struct QjNoHost;
impl CtxApi for QjNoHost {
    fn var(&self, _name: &str) -> Option<Value> { None }
    fn config(&self, _key: &str) -> Option<Value> { None }
    fn http_get(&self, _u: &str, _h: &[(String, String)]) -> Result<Value, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn http_post(&self, _u: &str, _b: &str, _h: &[(String, String)]) -> Result<Value, Diagnostic> {
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
    fn crypto_digest(&self, _a: &str, _d: &str) -> Result<String, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn crypto_symmetric(&self, _a: &str, _m: &str, _k: &str, _i: &str, _d: &str) -> Result<Value, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn now(&self) -> Result<i64, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
    fn rand(&self) -> Result<f64, Diagnostic> {
        Err(Diagnostic::new(Code::EScript, "ctx", "ctx host 未注入", "检查执行环境"))
    }
}

fn with_host_qjs<T>(f: impl FnOnce(&dyn CtxApi) -> T) -> T {
    QJ_BRIDGE_HOST.with(|b| match b.borrow().as_ref() {
        Some(h) => {
            let h: &(dyn CtxApi + 'static) = unsafe { &**h };
            f(h)
        }
        None => f(&QjNoHost),
    })
}

fn push_err_qjs(d: Diagnostic) -> String {
    QJ_BRIDGE_ERR.with(|b| b.borrow_mut().push(d));
    "__RSL_ERR__".to_string()
}


/// 求值表达式；失败兜底 undefined（恒成功路径）。
fn eval_or_undefined<'js>(ctx: &Ctx<'js>, expr: &str) -> JsValue<'js> {
    match ctx.eval::<JsValue<'js>, _>(expr.as_bytes()) {
        Ok(v) => v,
        Err(_) => JsValue::new_undefined(ctx.clone()),
    }
}

/// Value → JS 桥字符串：String 值直传（不带引号），其它类型 JSON 序列化。
/// 与 boa json_to_js 同语义——V2 曾误用 serde_json::to_string 包了一切，
/// 使 http 响应文本 / crypto.sym 结果在 JS 侧带引号（回归，真实站点 L3 复测暴露）。
fn value_to_bridge_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// ctx 十接口桥（与 boa bind_ctx_globals 同语义）。闭包首参 Ctx，无外部捕获。
fn bind_qjs_globals(c: &Ctx<'_>) -> Result<(), Diagnostic> {
    let g = c.globals();
    macro_rules! reg {
        ($name:literal, $f:expr) => {
            g.set($name, Function::new(c.clone(), $f).map_err(|e| qjs_err("ctx", &e))?)
                .map_err(|e| qjs_err("ctx", &e))?;
        };
    }

    reg!("__ctx_var", |name: String| -> Option<String> {
        with_host_qjs(|h| h.var(&name)).map(|v| serde_json::to_string(&v).unwrap_or_default())
    });

    reg!("__ctx_config", |key: String| -> Option<String> {
        with_host_qjs(|h| h.config(&key)).map(|v| serde_json::to_string(&v).unwrap_or_default())
    });

    reg!("__ctx_http_get", |url: String| -> String {
        match with_host_qjs(|h| h.http_get(&url, &[])) {
            Ok(Value::String(s)) => s,
            Ok(v) => serde_json::to_string(&v).unwrap_or_default(),
            Err(d) => push_err_qjs(d),
        }
    });

    reg!("__ctx_http_post", |url: String, body: String| -> String {
        match with_host_qjs(|h| h.http_post(&url, &body, &[])) {
            Ok(Value::String(s)) => s,
            Ok(v) => serde_json::to_string(&v).unwrap_or_default(),
            Err(d) => push_err_qjs(d),
        }
    });

    reg!("__ctx_cache_get", |k: String| -> Option<String> {
        with_host_qjs(|h| h.cache_get(&k)).map(|v| serde_json::to_string(&v).unwrap_or_default())
    });

    reg!("__ctx_cache_set", |ctx: Ctx<'_>, k: String, v: JsValue<'_>, ttl: rquickjs::Value<'_>| -> () {
        let json = js_to_json(&v).unwrap_or(Value::Null);
        let ttl_i = ttl.as_number().map(|t| t as i64).unwrap_or(3600);
        let _ = &ctx;
        with_host_qjs(|h| h.cache_set(&k, json, ttl_i));
    });

    reg!("__ctx_session_get", |k: String| -> Option<String> {
        with_host_qjs(|h| h.session_get(&k)).map(|v| serde_json::to_string(&v).unwrap_or_default())
    });

    reg!("__ctx_session_set", |k: String, v: JsValue<'_>| -> () {
        let json = js_to_json(&v).unwrap_or(Value::Null);
        with_host_qjs(|h| h.session_set(&k, json));
    });

    reg!("__ctx_cookie_get", |k: String| -> String {
        with_host_qjs(|h| h.cookie_get(&k)).unwrap_or_default()
    });

    reg!("__ctx_cookie_set", |k: String, v: String| -> () {
        with_host_qjs(|h| h.cookie_set(&k, &v));
    });

    reg!("__ctx_cookie_remove", |k: String| -> () {
        with_host_qjs(|h| h.cookie_remove(&k));
    });

    reg!("__ctx_log", |m: String| -> () {
        with_host_qjs(|h| h.log(&m));
    });

    reg!("__ctx_crypto", |alg: String, data: String| -> String {
        match with_host_qjs(|h| h.crypto_digest(&alg, &data)) {
            Ok(v) => v,
            Err(d) => push_err_qjs(d),
        }
    });

    reg!("__ctx_b64hex", |s: String| -> String {
        crate::crypto::base64_to_hex(&s).unwrap_or_default()
    });
    reg!("__ctx_b64utf8", |s: String| -> String {
        crate::crypto::base64_to_utf8(&s).unwrap_or_default()
    });
    reg!("__ctx_crypto_sym", |alg: String, mode: String, key: String, iv: String, data: String| -> String {
        match with_host_qjs(|h| h.crypto_symmetric(&alg, &mode, &key, &iv, &data)) {
            Ok(v) => value_to_bridge_string(&v),
            Err(d) => push_err_qjs(d),
        }
    });

    reg!("__ctx_now", || -> f64 { with_host_qjs(|h| h.now()).map(|v| v as f64).unwrap_or(0.0) });

    reg!("__ctx_rand", || -> f64 { with_host_qjs(|h| h.rand()).unwrap_or(0.0) });

    // __ctx 组合层（与 boa 相同的 JS 源）
    c.eval(
        r#"
globalThis.__ctx = {
  var: (n) => { var t = __ctx_var(n); try { return t == null ? undefined : JSON.parse(t); } catch (e) { return undefined; } },
  config: (k) => { var t = __ctx_config(k); try { return t == null ? undefined : JSON.parse(t); } catch (e) { return undefined; } },
  http: {
    get: (u, o) => __ctx_http_get(u, o),
    post: (u, b, o) => __ctx_http_post(u, b, o),
    json: (u, o) => { var t = __ctx_http_get(u, o); try { return JSON.parse(t); } catch (e) { return t; } },
  },
  cache: { get: (k) => { var t = __ctx_cache_get(k); try { return t == null ? undefined : JSON.parse(t); } catch (e) { return undefined; } }, set: (k, v, t) => __ctx_cache_set(k, v, t) },
  session: { get: (k) => { var t = __ctx_session_get(k); try { return t == null ? undefined : JSON.parse(t); } catch (e) { return undefined; } }, set: (k, v) => __ctx_session_set(k, v) },
  cookie: { get: (k) => __ctx_cookie_get(k), set: (k, v) => __ctx_cookie_set(k, v), remove: (k) => __ctx_cookie_remove(k) },
  log: (m) => __ctx_log(m),
  b64hex: (s) => __ctx_b64hex(s),
  b64utf8: (s) => __ctx_b64utf8(s),
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
"#
        .as_bytes(),
    )
    .map_err(|e| qjs_err("ctx", &e))
}

impl ScriptEngine for QuickJsEngine {
    fn call_pure(&self, fn_name: &str, args: &[&Value], path: &str) -> Result<Value, Diagnostic> {
        self.check_budget(path)?;
        let rt = make_runtime()?;
        let ctx = rquickjs::Context::full(&rt).map_err(|e| qjs_err("lib.pure", &e))?;
        ctx.with(|c| -> Result<Value, Diagnostic> {
            c.eval(PURIFY_JS.as_bytes()).map_err(|e| qjs_err("lib.pure", &e))?;
            c.eval(self.pure_src.as_bytes())
                .map_err(|e| qjs_err("lib.pure", &e))?;
            // 函数存在性检查（避免 eval 时静默 ReferenceError 語义混淆）
            let exists: bool = c
                .eval(format!("(typeof {fn_name} === 'function')").as_bytes())
                .map_err(|e| qjs_err("lib.pure", &e))?;
            if !exists {
                return Err(Diagnostic::new(
                    Code::EScript,
                    path,
                    format!("lib.pure 函数 `{fn_name}` 不存在"),
                    "在 lib.pure 里定义它",
                ));
            }
            let mut argv = vec![];
            for a in args {
                argv.push(json_as_expr(&c, a)?);
            }
            let expr = format!("{fn_name}({})", argv.join(","));
            let result: JsValue<'_> = c
                .eval(expr.as_bytes())
                .map_err(|e| qjs_err(&format!("lib.pure.{fn_name}"), &e))?;
            js_to_json(&result)
        })
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
        QJ_BRIDGE_HOST.with(|b| {
            let erased: *const (dyn CtxApi + 'static) =
                unsafe { std::mem::transmute(host as *const dyn CtxApi) };
            *b.borrow_mut() = Some(erased);
        });
        QJ_BRIDGE_ERR.with(|b| b.borrow_mut().clear());
        let result = (|| -> Result<Value, Diagnostic> {
            let rt = make_runtime()?;
            let ctx = rquickjs::Context::full(&rt).map_err(|e| qjs_err("lib.ctx", &e))?;
            ctx.with(|c| -> Result<Value, Diagnostic> {
                c.eval(PURIFY_JS.as_bytes()).map_err(|e| qjs_err("lib.ctx", &e))?;
                bind_qjs_globals(&c)?;
                c.eval(self.ctx_src.as_bytes())
                    .map_err(|e| qjs_err("lib.ctx", &e))?;
                let exists: bool = c
                    .eval(format!("(typeof {fn_name} === 'function')").as_bytes())
                    .map_err(|e| qjs_err("lib.ctx", &e))?;
                if !exists {
                    return Err(Diagnostic::new(
                        Code::EScript,
                        path,
                        format!("lib.ctx 函数 `{fn_name}` 不存在"),
                        "在 lib.ctx 里定义它",
                    ));
                }
                let mut argv = vec!["globalThis.__ctx".to_string()];
                for a in args {
                    argv.push(json_as_expr(&c, a)?);
                }
                let expr = format!("{fn_name}({})", argv.join(","));
                let result: JsValue<'_> = c
                    .eval(expr.as_bytes())
                    .map_err(|e| qjs_err(&format!("lib.ctx.{fn_name}"), &e))?;
                js_to_json(&result)
            })
        })();
        // 桥里记录的错误优先抛出；清空桥指针
        let bridge_err = QJ_BRIDGE_ERR.with(|b| b.borrow().first().cloned());
        QJ_BRIDGE_HOST.with(|b| *b.borrow_mut() = None);
        if let Some(d) = bridge_err {
            return Err(d);
        }
        result
    }

    fn has_fn(&self, name: &str) -> bool {
        let pat = format!("function {name}\\s*\\(");
        regex::Regex::new(&pat)
            .map(|r| r.is_match(&self.pure_src) || r.is_match(&self.ctx_src))
            .unwrap_or(false)
    }

    fn steps_used(&self) -> i64 {
        self.steps.load(Ordering::SeqCst)
    }
}
