//! T3.5 执行上下文：六层状态（S§5.1）+ 端口注入 + 预算计量 + trace。
//! `exec` 不依赖 `http`——传输经 `ports::Transport` 注入。

use crate::error::{Code, Diagnostic};
use crate::ir::{CompiledSource, ParamExpr};
use crate::ports::*;
use crate::sugar::Seg;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::rc::Rc;

/// trace 条目（S§13.1）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEntry {
    pub stage: String,
    pub step: String,
    pub rule: String,
    pub matched: usize,
    pub ms: u64,
    pub bridge: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

// ───────────────────── rsl-debug/1 捕获（TASK-BOOK-4 T1，四 tab 数据面） ─────────────────────
// 与 TraceEntry 同层：exec 不依赖 stage；记录由 stage 层 send_request 写入。

/// 请求侧记录（协议标头 tab：原始地址/请求地址/重定向/重试/引擎/方式/解析方式）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugReqInfo {
    /// 原始地址（模板形态，如 var:line/search）。
    pub template: String,
    /// 请求地址（模板渲染 + params 编码后的实际值）。
    pub url: String,
    /// 请求方式 GET/POST/PUT/DELETE/HEAD。
    pub method: String,
    /// 实际发送的请求头（全局 + 登录 + 阶段合并）。
    pub headers: Vec<(String, String)>,
    /// 引擎：http / webview。
    pub engine: String,
    /// 解析方式：json / html / text / bytes。
    pub parse: String,
    /// 实际尝试次数（含重试）。
    pub attempts: u64,
}

/// 响应侧记录。预览 ≤256KB + sha256 + truncated（V4-D4：大响应不进产物）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugRespInfo {
    pub status: u16,
    /// 响应头（协议标头 tab 原样展示）。
    pub headers: Vec<(String, String)>,
    pub ms: u64,
    /// 最终 URL（重定向后；与请求地址不同即发生了重定向）。
    pub final_url: String,
    /// 原始体字节数（截断前的实际值）。
    pub body_bytes: usize,
    /// 原始体 sha256（hex）。
    pub sha256: String,
    /// 预览被截断（原始体 > 256KB 预算）。
    pub truncated: bool,
    /// 二进制响应（bytes 声明）——preview 为 null。
    pub binary: bool,
    /// 解码文本预览（≤256KB；解码失败时 lossy 兜底，二进制时 null）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

/// 单次请求-响应配对（分页时多条；网络层失败时 response 为 null）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DebugRecord {
    pub request: DebugReqInfo,
    pub response: Option<DebugRespInfo>,
}

/// 运行期上下文。
pub struct ExecCx<'a> {
    pub src: &'a CompiledSource,
    /// 六层状态之一：上下文/源配置（var:）。键为点路径（"book.tocUrl"）。
    pub vars: BTreeMap<String, Value>,
    /// 会话（session:）。
    pub session: RefCell<BTreeMap<String, Value>>,
    /// 登录令牌（token:）。
    pub tokens: RefCell<BTreeMap<String, String>>,
    /// Cookie（cookie:，只读视图由 host store 供值 + login 产出写入）。
    pub cookies: RefCell<Vec<(String, String)>>,
    /// 缓存（cache:<name>）。
    pub cache: RefCell<HashMap<String, Value>>,
    /// 命名缓存条目的落盘值（cache.name → take 结果）。
    pub transport: &'a dyn Transport,
    pub webview: Option<&'a dyn WebViewPort>,
    /// 交互式登录会话（TASK-BOOK-3 A1；未注入时 login.type=webview → E_UNSUPPORTED）。
    pub login_session: Option<&'a dyn LoginSessionPort>,
    pub cookie_store: Option<&'a dyn CookieStore>,
    /// 隔离键（源 id + 域名 + 登录名）。
    pub cookie_scope: String,
    pub clock: &'a dyn Clock,
    pub trace: RefCell<Vec<TraceEntry>>,
    /// rsl-debug/1 捕获（TASK-BOOK-4 T1）：本阶段全部请求-响应配对。
    pub debug: RefCell<Vec<DebugRecord>>,
    pub warnings: RefCell<Vec<Diagnostic>>,
    /// libSteps 计量（确定性计量，S§8.4）。
    pub lib_steps: std::cell::Cell<i64>,
    /// 当前阶段名。
    pub stage: RefCell<String>,
    /// 慢速标记：取消。
    pub cancelled: std::sync::atomic::AtomicBool,
    /// 协作式取消令牌（A5）：宿主注入句柄，跨线程置位。
    pub cancel_token: Option<CancelToken>,
    /// 本阶段是否命中取消（partial 传播；run_stage 读后置 partial=true）。
    pub cancelled_hit: std::cell::Cell<bool>,
}

impl<'a> ExecCx<'a> {
    pub fn new(src: &'a CompiledSource, transport: &'a dyn Transport, clock: &'a dyn Clock) -> Self {
        let mut vars = BTreeMap::new();
        for (k, v) in &src.vars {
            vars.insert(k.clone(), v.value.to_json());
        }
        vars.insert("baseUrl".into(), Value::String(src.base_url.clone()));
        vars.insert("charset".into(), Value::String("utf-8".into()));
        ExecCx {
            src,
            vars,
            session: RefCell::new(BTreeMap::new()),
            tokens: RefCell::new(BTreeMap::new()),
            cookies: RefCell::new(vec![]),
            cache: RefCell::new(HashMap::new()),
            transport,
            webview: None,
            login_session: None,
            cookie_store: None,
            cookie_scope: src.id.clone(),
            clock,
            trace: RefCell::new(vec![]),
            debug: RefCell::new(vec![]),
            warnings: RefCell::new(vec![]),
            lib_steps: std::cell::Cell::new(0),
            stage: RefCell::new(String::new()),
            cancelled: std::sync::atomic::AtomicBool::new(false),
            cancel_token: None,
            cancelled_hit: std::cell::Cell::new(false),
        }
    }

    pub fn warn(&self, d: Diagnostic) {
        self.warnings.borrow_mut().push(d);
    }

    /// var: 路径求值（"book.tocUrl"、"kw"、"filter.mode"）。
    /// 协作式取消检查点（A5）：命中即记录 E_CANCELLED 诊断（仅首推）并标记 cancelled_hit。
    /// 返回 true 表示调用方应在最近的边界（页/步/请求前）停止并返回部分结果。
    pub fn check_cancelled(&self) -> bool {
        let hit = self
            .cancel_token
            .as_ref()
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
            || self.cancelled.load(std::sync::atomic::Ordering::SeqCst);
        if hit && !self.cancelled_hit.get() {
            self.cancelled_hit.set(true);
            self.warnings.borrow_mut().push(Diagnostic::new(
                Code::ECancelled,
                format!("stages.{}", self.stage.borrow()),
                "执行被宿主取消",
                "已返回部分结果；检查取消时机或重试",
            ));
        }
        hit
    }

    pub fn get_var(&self, path: &str) -> Option<Value> {
        if let Some(v) = self.vars.get(path) {
            return Some(v.clone());
        }
        // 父对象展开：book → {k: v}
        if let Some(v) = self.vars.get(path.split('.').next().unwrap_or(path)) {
            let mut cur = v.clone();
            for part in path.split('.').skip(1) {
                cur = cur.get(part)?.clone();
            }
            return Some(cur);
        }
        None
    }

    /// cookie 读取（含 host store）。
    pub fn get_cookie(&self, name: &str) -> Option<String> {
        if let Some(v) = self.cookies.borrow().iter().find(|(k, _)| k == name) {
            return Some(v.1.clone());
        }
        if let Some(store) = self.cookie_store {
            return store.get(&format!("{}/{}", self.cookie_scope, name));
        }
        None
    }

    pub fn set_cookie(&self, name: &str, value: &str) {
        self.cookies
            .borrow_mut()
            .retain(|(k, _)| k != name);
        self.cookies.borrow_mut().push((name.to_string(), value.to_string()));
        if let Some(store) = self.cookie_store {
            store.set(&format!("{}/{}", self.cookie_scope, name), value, self.src.cookie_persist);
        }
    }

    /// secret 脱敏表。
    pub fn secrets(&self) -> Vec<String> {
        self.src
            .vars
            .values()
            .filter(|v| v.secret)
            .filter_map(|v| v.value.as_str().map(String::from))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 限流等待（T2.4：对同一源串行化；可观测）。
    pub fn rate_wait(&self) {
        if let Some(rate) = &self.src.http.rate {
            
            use std::time::{Duration, Instant};
            static LAST: OnceCellLog = OnceCellLog::new();
            let now = Instant::now().elapsed().as_millis() as i64;
            let _ = LAST.get_or(now);
            let min_gap = if rate.window_ms > 0 && rate.max > 0 {
                (rate.window_ms as f64 / rate.max as f64 * 1000.0) as u64
            } else {
                0
            };
            if min_gap > 0 {
                std::thread::sleep(Duration::from_millis(min_gap.min(100)));
            }
        }
    }
}

/// 进程级上次请求时间（简化限流）。
struct OnceCellLog(std::sync::Mutex<Option<i64>>);

impl OnceCellLog {
    const fn new() -> Self {
        OnceCellLog(std::sync::Mutex::new(None))
    }
    fn get_or(&self, v: i64) -> i64 {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let r = g.unwrap_or(v);
        *g = Some(v);
        r
    }
}

/// 模板求值：Seg 列表 → 值（整体单模板保留类型；混合拼接字符串）。
pub fn render_template(cx: &ExecCx, segs: &[Seg]) -> Result<Value, Diagnostic> {
    if segs.len() == 1 {
        if let Seg::Tmpl { ns, expr } = &segs[0] {
            if ns != "expr" {
                if let Some(v) = resolve_ns_raw(cx, ns, expr)? {
                    return Ok(v);
                }
            }
        }
    }
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Lit(s) => out.push_str(s),
            Seg::Tmpl { ns, expr } => {
                let v = if ns == "expr" {
                    // 带上下文求值（host 由调用方提供）；此处走简化：纯文本无法求值 → 由 stage 层处理
                    // expr 模板在 stage 层用带 host 的求值器
                    return Err(Diagnostic::new(
                        Code::ERef,
                        "$",
                        "expr 模板必须在阶段上下文中求值",
                        "用 stage::render_template_with_host",
                    ));
                } else {
                    resolve_ns_raw(cx, ns, expr)?
                        .map(|v| crate::exec::value::Val::Json(v))
                        .map(|v| v.to_text())
                        .unwrap_or_default()
                };
                out.push_str(&v);
            }
        }
    }
    Ok(Value::String(out))
}

/// 阶段层的模板求值（支持 expr，带 ExprHost）。
pub fn render_template_with_host<H: crate::expr::ExprHost>(
    segs: &[Seg],
    host: &H,
    path: &str,
) -> Result<Value, Diagnostic> {
    if segs.len() == 1 {
        if let Seg::Tmpl { ns, expr } = &segs[0] {
            if ns == "expr" {
                let e = crate::expr::parse::parse(expr, path)?;
                let v = crate::expr::eval::eval(&e, host, path)?;
                return Ok(v.to_json());
            }
        }
    }
    let mut out = String::new();
    for seg in segs {
        match seg {
            Seg::Lit(s) => out.push_str(s),
            Seg::Tmpl { ns, expr } => {
                let v = if ns == "expr" {
                    let e = crate::expr::parse::parse(expr, path)?;
                    crate::expr::eval::eval(&e, host, path)?.to_str()
                } else {
                    match host.var(ns, expr) {
                        Some(v) => v.to_str(),
                        None => String::new(),
                    }
                };
                out.push_str(&v);
            }
        }
    }
    Ok(Value::String(out))
}

/// 非表达式命名空间的原始求值（var/let/session/token/cookie/cache/const/lib）。
fn resolve_ns_raw(cx: &ExecCx, ns: &str, path: &str) -> Result<Option<Value>, Diagnostic> {
    Ok(match ns {
        "var" | "" => cx.get_var(path),
        "let" => cx.session.borrow().get(path).cloned(),
        "session" => cx.session.borrow().get(path).cloned(),
        "token" => cx.tokens.borrow().get(path).cloned().map(Value::String),
        "cookie" => cx.get_cookie(path).map(Value::String),
        "cache" => cx.cache.borrow().get(path).cloned(),
        "const" => {
            let v: Value = serde_json::from_str(path)
                .unwrap_or(Value::String(path.to_string()));
            Some(v)
        }
        _ => None,
    })
}

/// ParamExpr 求值（stage 层用）。
pub fn eval_param<H: crate::expr::ExprHost>(
    p: &ParamExpr,
    host: &H,
    path: &str,
) -> Result<Value, Diagnostic> {
    match p {
        ParamExpr::Expr(e) => {
            let v = crate::expr::eval::eval(e, host, path)?;
            Ok(v.to_json())
        }
        ParamExpr::Raw(v) => Ok(v.to_json()),
        ParamExpr::Selector(_) => Err(Diagnostic::new(
            Code::EType,
            path,
            "选择器型参数需要在响应上下文中求值",
            "该参数由 stage 层处理",
        )),
        ParamExpr::Template(segs) => render_template_with_host(segs, host, path),
    }
}

/// libSteps 计量。
pub fn tick_steps(cx: &ExecCx, n: i64) -> Result<(), Diagnostic> {
    let cur = cx.lib_steps.get() + n;
    cx.lib_steps.set(cur);
    if cur > cx.src.limits.lib_steps {
        return Err(Diagnostic::new(
            Code::EBudget,
            "limits.libSteps",
            format!("libSteps {cur} 超上限 {}", cx.src.limits.lib_steps),
            "提高 limits.libSteps 或简化脚本",
        ));
    }
    Ok(())
}

/// rc 包装便利。
pub type Shared<T> = Rc<T>;
