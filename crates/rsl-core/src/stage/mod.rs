//! T3.6–T3.12 阶段编排：五阶段契约、四种分页、登录推进、缓存分层、预算、多源合并、trace。

pub mod debug;
pub mod login;
pub mod merge;

use crate::error::{Code, Diagnostic};
use crate::exec::ctx::{render_template_with_host, ExecCx};
use crate::exec::eval::{finalize_val, Eval};
use crate::exec::ctx::eval_param;
use crate::exec::hosts::{CtxHost, ExprHostImpl};
use crate::exec::value::{HtmlDoc, Val};
use crate::http::{decode_response, encode_param, HttpClient, RateLimiter};
use crate::ir::*;
use crate::ports::{HttpRequest, HttpResponse};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// 阶段输出。
#[derive(Debug, Clone)]
pub struct StageResult {
    pub out: Value,
    /// E_CANCELLED 时的部分结果标记（S§12.5）。
    pub partial: bool,
}

/// 宿主能力（hostStages 支持矩阵，U7）。
#[derive(Debug, Clone, Default)]
pub struct HostCaps {
    pub host_stages: Vec<String>,
    pub webview: bool,
}

/// 运行输入（宿主注入，S§5.4 显式注入，无隐式提升）。
#[derive(Debug, Clone, Default)]
pub struct RunInputs {
    pub kw: Option<String>,
    pub page: Option<i64>,
    /// detail/toc/content：选中条目字段。
    pub book: BTreeMap<String, Value>,
    pub chapter: BTreeMap<String, Value>,
    /// explore：分组与筛选。
    pub group: Option<Value>,
    pub filter: BTreeMap<String, Value>,
    /// 登录字段值（login.<key>）。
    pub login_values: BTreeMap<String, String>,
    /// hostStages 入参。
    pub extra: BTreeMap<String, Value>,
}

/// 上一次响应复用通道（reuseDetailHtml，S§6.5.2）。
#[derive(Default)]
pub struct ResponseReuse {
    pub detail_url: Option<String>,
    pub detail_body: Option<Vec<u8>>,
}

/// 运行一个阶段。不联网能力由注入的 transport 决定（L2 fixture / L3 真实）。
#[allow(clippy::too_many_arguments)]
pub fn run_stage(
    src: &CompiledSource,
    stage_name: &str,
    cx: &mut ExecCx,
    inputs: &RunInputs,
    reuse: &mut ResponseReuse,
    caps: &HostCaps,
) -> Result<StageResult, Diagnostic> {
    let is_host = src.host_stages.contains_key(stage_name);
    // hostStages：宿主未实现 → E_HOST_STAGE（只禁用该入口，U3）
    if is_host && !caps.host_stages.iter().any(|s| s == stage_name) {
        return Err(Diagnostic::new(
            Code::EHostStage,
            format!("hostStages.{stage_name}"),
            format!("宿主未实现 {stage_name} 阶段"),
            "宿主 engine.capabilities.hostStages 未含该 id；仅该入口禁用，五阶段不受影响",
        ));
    }
    let stage = match src.stage(stage_name) {
        Some(s) => s,
        None => {
            return Err(Diagnostic::new(
                Code::EEnum,
                stage_name,
                format!("阶段 `{stage_name}` 不存在"),
                "合法阶段：search explore detail toc content 或 hostStages.<id>",
            ))
        }
    };
    *cx.stage.borrow_mut() = stage_name.to_string();
    cx.cancelled_hit.set(false);

    // requiresLogin → check（T3.9 / S§4.5.3）
    if stage.requires_login || (is_host && stage.requires_login) {
        // B2：带机械重登包装（relogin.max=0 时行为与原 ensure_login 逐字节一致）
        login::ensure_login_with_relogin(src, cx, inputs)?;
    } else if src.login.is_some() && login::login_needed_for(src, stage_name) {
        // 五阶段中声明了 token:/cookie: 依赖的阶段在 check 失败时 → E_LOGIN_REQUIRED
        login::ensure_login(src, cx, inputs)?;
    }

    // 缓存块预载（T3.10）
    prefetch_cache(src, cx, inputs)?;

    // 注入上下文变量
    inject_vars(src, stage_name, cx, inputs);

    // let 求值（有序，T3.5）
    eval_lets(src, stage, cx, inputs)?;

    // 分页循环
    let collected = paginate_loop(src, stage, stage_name, cx, inputs, reuse, caps)?;

    // out 汇总：列表型用 collect_items；对象型直接取
    let out = match &collected {
        Paginated::Pages(_) => {
            let items = collect_items(cx, stage, stage_name, pages_roots(&collected))?;
            let mut m = serde_json::Map::new();
            m.insert("list".into(), Value::Array(items));
            Value::Object(m)
        }
        Paginated::Object(v) => v.clone(),
    };

    // verify（T3.6）
    if let Some(verify) = &stage.out.verify {
        let text = out.to_string();
        if let Some(contains) = &verify.contains {
            let rendered = render_template_with_host(
                contains,
                &ExprHostImpl { cx, doc_val: Val::Empty },
                "out.verify",
            )?;
            let needle = rendered.as_str().unwrap_or_default().to_string();
            if !needle.is_empty() && !text.contains(&needle) {
                return Err(Diagnostic::new(
                    Code::EVerify,
                    "out.verify",
                    format!("结果不含关键词 `{needle}`"),
                    "疑似风控页；先看 sample，再补 headers 或降频",
                ));
            }
        }
        if let Some(min) = verify.min_items {
            let n = out.get("list").and_then(|l| l.as_array()).map(|a| a.len()).unwrap_or(0);
            if (n as i64) < min {
                return Err(Diagnostic::new(
                    Code::EVerify,
                    "out.verify.minItems",
                    format!("结果 {} 条少于下限 {min}", n),
                    "确认分页/筛选正确",
                ));
            }
        }
    }

    // emptyOk（T3.6 / S§3.8）
    if let Some(list) = out.get("list").and_then(|l| l.as_array()) {
        if list.is_empty() && !stage.out.empty_ok {
            let mut d = Diagnostic::new(
                Code::EEmpty,
                format!("stages.{stage_name}.out.list"),
                "列表为空",
                "接口正常但无结果；确认 var:kw 已注入，或把 emptyOk 设为 true",
            );
            d.stage = Some(stage_name.to_string());
            return Err(d);
        }
    }

    // after 钩子（T4.6）
    if let Some(hook) = &stage.script.after {
        let engine = crate::script::get_engine();
        if let Some(e) = engine {
            let host = CtxHost { cx };
            let _ = e.call_ctx(&hook.fn_name, &[&out], &host, &format!("stages.{stage_name}.script.after"))?;
        }
    }

    Ok(StageResult { out, partial: cx.cancelled_hit.get() })
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "num",
        Value::String(_) => "str",
        Value::Array(_) => "json[]",
        Value::Object(_) => "json",
    }
}

/// 请求参数求值（var:/let:/session:/expr: 无需响应上下文；其余走 eval_param）。
fn eval_request_param(
    cx: &mut ExecCx,
    stage: &str,
    p: &ParamExpr,
    path: &str,
) -> Result<Value, Diagnostic> {
    if let ParamExpr::Selector(sel) = p {
        let mut e = make_eval(cx, stage, Val::Empty, None);
        let r = e.eval_selector(sel)?;
        return Ok(finalize_val(r));
    }
    eval_param(p, &ExprHostImpl { cx, doc_val: Val::Empty }, path)
}

fn stage_name_of(path: &str) -> String {
    // "stages.search.request.params.q" → "search"
    path.split('.')
        .nth(1)
        .unwrap_or("search")
        .to_string()
}

fn pages_roots(p: &Paginated) -> Vec<(Val, std::boxed::Box<HtmlDoc>)> {
    match p {
        Paginated::Pages(v) => v.clone(),
        Paginated::Object(_) => vec![],
    }
}

fn make_eval<'a, 'c>(
    cx: &'a mut ExecCx<'c>,
    stage: &str,
    origin: Val,
    doc: Option<&'a HtmlDoc>,
) -> Eval<'a, 'c> {
    Eval {
        cx,
        doc,
        origin,
        path: format!("stages.{stage}"),
        stage: stage.to_string(),
        keep_nodes: false,
        cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
    }
}

fn make_eval_keep<'a, 'c>(
    cx: &'a mut ExecCx<'c>,
    stage: &str,
    origin: Val,
    doc: Option<&'a HtmlDoc>,
) -> Eval<'a, 'c> {
    let mut e = make_eval(cx, stage, origin, doc);
    e.keep_nodes = true;
    e
}

fn collected_list(_cx: &ExecCx, _stage: &StageIR, _name: &str, _roots: &[(Val, Option<HtmlDoc>)]) -> Result<Vec<Value>, Diagnostic> {
    Ok(vec![])
}

fn collect_items(
    cx: &mut ExecCx,
    stage: &StageIR,
    stage_name: &str,
    roots: Vec<(Val, std::boxed::Box<HtmlDoc>)>,
) -> Result<Vec<Value>, Diagnostic> {
    let mut items = vec![];
    for (page_index, (root, doc_box)) in roots.into_iter().enumerate() {
        let doc_ref: Option<&HtmlDoc> = Some(doc_box.as_ref());
        let mut e = make_eval_keep(cx, stage_name, root, doc_ref);
        e.path = format!("stages.{stage_name}.out.list");
        if let Some(list_sel) = &stage.out.list {
            let lv = match e.eval_selector(list_sel) {
                Ok(v) => v,
                Err(d) if d.code == Code::ENoMatch => {
                    // 首页必填列表命中 0 → E_NO_MATCH（改版）；emptyOk 才视为合法空
                    if !stage.out.empty_ok && page_index == 0 {
                        return Err(d);
                    }
                    Val::Empty
                }
                Err(d) => return Err(d),
            };
            let entries: Vec<Val> = match lv {
                Val::NodeList(ids) => ids.into_iter().map(Val::Node).collect(),
                Val::Json(Value::Array(a)) => a.into_iter().map(Val::Json).collect(),
                Val::StrList(l) => l.into_iter().map(|s| Val::Str(s)).collect(),
                Val::Empty => vec![],
                other => vec![other],
            };
            let cap = cx.src.limits.max_nodes as usize;
            for (i, item) in entries.into_iter().enumerate() {
                if i >= cap {
                    break;
                }
                let mut pending_warns: Vec<Diagnostic> = vec![];
                let mut ie = make_eval(cx, stage_name, item, doc_ref);
                let mut obj = serde_json::Map::new();
                for (k, fsel) in &stage.out.map {
                    match ie.eval_selector(fsel) {
                        Ok(r) => {
                            obj.insert(k.clone(), finalize_val(r));
                        }
                        Err(d) if d.code == Code::ENoMatch => {
                            // 逐条目降级（S§11.3）：该字段取 default/null，条目保留
                            if !fsel.optional {
                                pending_warns.push(
                                    Diagnostic::new(
                                        Code::WDegraded,
                                        format!("stages.{stage_name}.out.map.{k}"),
                                        format!("字段 {k} 在条目上未命中，已降级"),
                                        "卷标记行无正文字段属正常；真实缺失请修选择器或加 optional",
                                    )
                                    .rule(&fsel.raw)
                                    .degraded(true),
                                );
                            }
                            obj.insert(k.clone(), fsel.default.clone().map(|x| x.to_json()).unwrap_or(Value::Null));
                        }
                        Err(d) => return Err(d),
                    }
                }
                drop(ie);
                for w in pending_warns {
                    cx.warn(w);
                }
                items.push(serde_json::json!({ "map": obj }));
            }
        }
    }
    Ok(items)
}

/// 注入上下文变量（S§5.3 内置变量）。
fn inject_vars(src: &CompiledSource, stage_name: &str, cx: &mut ExecCx, inputs: &RunInputs) {
    if let Some(kw) = &inputs.kw {
        cx.vars.insert("kw".into(), Value::String(kw.clone()));
    }
    if let Some(p) = inputs.page {
        cx.vars.insert("page".into(), json!(p));
    }
    if let Some(g) = &inputs.group {
        cx.vars.insert("group".into(), g.clone());
    }
    for (k, v) in &inputs.filter {
        cx.vars.insert(format!("filter.{k}"), v.clone());
    }
    for (k, v) in &inputs.book {
        cx.vars.insert(format!("book.{k}"), v.clone());
    }
    for (k, v) in &inputs.chapter {
        cx.vars.insert(format!("chapter.{k}"), v.clone());
    }
    for (k, v) in &inputs.extra {
        cx.vars.insert(k.clone(), v.clone());
    }
    // cookie.initial 预置
    for (name, value) in &src.cookie_initial {
        if cx.get_cookie(name).is_none() {
            cx.set_cookie(name, value);
        }
    }
    // enumerate 阶段：var:pages = 枚举值拼接（joinWith 语义，S§4.7.1）
    if let Some(pg) = src.stage(stage_name).and_then(|st| st.paginate.as_ref()) {
        if let PaginateKind::Enumerate { values, join_with, .. } = &pg.kind {
            let sep = join_with.clone().unwrap_or_default();
            let joined = values
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(&sep);
            cx.vars.insert("pages".into(), Value::String(joined));
        }
    }
    // scriptResult
    let has_script = src
        .stage(stage_name)
        .map(|s| !s.script.before.is_none() || !s.script.on_response.is_none() || !s.script.after.is_none())
        .unwrap_or(false);
    if has_script {
        cx.vars.insert("scriptResult".into(), Value::Null);
    }
}

/// let 求值（有序，T3.5；S§6.1.1）。
fn eval_lets(src: &CompiledSource, stage: &StageIR, cx: &mut ExecCx, _inputs: &RunInputs) -> Result<(), Diagnostic> {
    let path0 = format!("stages.{}.let", stage.name);
    for (name, le) in &stage.lets {
        let path = format!("{path0}.{name}");
        let v = match le {
            LetIR::Expr(e) => {
                let host = ExprHostImpl { cx, doc_val: Val::Empty };
                crate::expr::eval::eval(e, &host, &path)?.to_json()
            }
            LetIR::Selector(sel) => {
                let mut e = make_eval(cx, &stage.name, Val::Empty, None);
                let r = e.eval_selector(sel)?;
                finalize_val(r)
            }
            LetIR::Lib(f) => {
                let engine = crate::script::get_engine();
                match engine {
                    Some(e) => e.call_pure(f, &[&Value::Null], &path)?,
                    None => Value::Null,
                }
            }
            LetIR::Request { req, take } => {
                let resp = send_request(src, cx, req, &path)?;
                let doc = decode_to_doc(cx, req, &resp, &path)?;
                let root = doc_to_val(&doc);
                match take {
                    Some(t) => {
                        let doc_ref: Option<&HtmlDoc> = match &*doc {
                            crate::exec::value::Doc::Html(h) => Some(h),
                            _ => None,
                        };
                        let mut e = make_eval(cx, &stage.name, root, doc_ref);
                        let r = e.eval_selector(t)?;
                        finalize_val(r)
                    }
                    None => root.to_json(),
                }
            }
            LetIR::Parallel { items, limit } => {
                // 受 maxParallel 与 http.rate 双重约束（S§6.1.1）；阻塞串行实现保序（S§12.2）
                let mut out = vec![];
                for (i, req) in items.iter().enumerate().take((*limit).max(1)) {
                    let resp = send_request(src, cx, req, &format!("{path}.parallel[{i}]"))?;
                    out.push(Value::String(String::from_utf8_lossy(&resp.body).to_string()));
                }
                Value::Array(out)
            }
        };
        cx.session.borrow_mut().insert(name.clone(), v);
    }
    Ok(())
}

/// 解码响应为 Doc（六步判定已由 decode_response 处理；bytes 保留原始字节）。
pub fn decode_to_doc(
    cx: &ExecCx,
    req: &RequestIR,
    resp: &HttpResponse,
    path: &str,
) -> Result<std::boxed::Box<crate::exec::value::Doc>, Diagnostic> {
    let _ = cx;
    match req.response {
        RespKind::Bytes => Ok(Box::new(crate::exec::value::Doc::Bytes(resp.body.clone()))),
        RespKind::Json => {
            let text = decode_response(&resp.body, req.charset.as_deref(), resp.header("content-type"), false, path)?;
            let cleaned = strip_noise(&text);
            let v: Value = serde_json::from_str(&cleaned).map_err(|e| {
                Diagnostic::new(
                    Code::EScript,
                    path,
                    format!("响应不是合法 JSON：{e}"),
                    "response 改 html/text，或用 script.onResponse 预清洗",
                )
            })?;
            Ok(Box::new(crate::exec::value::Doc::Json(v)))
        }
        RespKind::Text => {
            let text = decode_response(&resp.body, req.charset.as_deref(), resp.header("content-type"), false, path)?;
            Ok(Box::new(crate::exec::value::Doc::Text(text)))
        }
        RespKind::Html => {
            let text = decode_response(&resp.body, req.charset.as_deref(), resp.header("content-type"), false, path)?;
            Ok(Box::new(crate::exec::value::Doc::Html(HtmlDoc::parse(&text))))
        }
    }
}

fn strip_noise(s: &str) -> String {
    let t = s.trim();
    if let (Some(i), Some(j)) = (t.find('{'), t.rfind('}')) {
        if i < j && !t.starts_with('{') {
            return t[i..=j].to_string();
        }
    }
    t.to_string()
}

pub fn doc_to_val(doc: &crate::exec::value::Doc) -> Val {
    match doc {
        crate::exec::value::Doc::Json(v) => Val::Json(v.clone()),
        crate::exec::value::Doc::Text(t) => Val::Str(t.clone()),
        crate::exec::value::Doc::Bytes(b) => Val::Bytes(b.clone()),
        crate::exec::value::Doc::Html(_) => Val::Empty, // html 以 doc 引用传递
    }
}

/// 请求构建 + 发送（T2.1–T2.8）。
pub fn send_request(
    src: &CompiledSource,
    cx: &mut ExecCx,
    req: &RequestIR,
    path: &str,
) -> Result<HttpResponse, Diagnostic> {
    // 宿主取消（A5）：请求前边界 —— 分页中途取消保留已收页，本请求不发
    if cx.check_cancelled() {
        return Err(Diagnostic::new(
            Code::ECancelled,
            path,
            "请求前已取消",
            "已收页为部分结果；检查取消时机或重试",
        ));
    }
    if req.skip {
        // 不发起请求（§4.2.1 ①）：返回空响应
        return Ok(HttpResponse {
            status: 200,
            headers: vec![],
            body: vec![],
            final_url: String::new(),
            ms: 0,
        });
    }
    // URL
    let url = render_request_template(cx, req.url.as_ref().unwrap_or(&vec![crate::sugar::Seg::Lit(src.base_url.clone())]), &format!("{path}.url"))?;
    let url = url.as_str().unwrap_or_default().to_string();
    if url.is_empty() {
        return Err(Diagnostic::new(Code::ERef, &format!("{path}.url"), "请求 URL 为空", "检查 var:/expr: 引用"));
    }
    // headers：全局 + 阶段 + login
    let mut headers: Vec<(String, String)> = vec![];
    for (k, segs) in &src.http.headers {
        let v = render_template_with_host(segs, &ExprHostImpl { cx, doc_val: Val::Empty }, &format!("{path}.headers.{k}"))?;
        headers.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
    }
    // login.headers
    if let Some(l) = &src.login {
        for (k, segs) in &l.headers {
            let v = render_template_with_host(segs, &ExprHostImpl { cx, doc_val: Val::Empty }, &format!("{path}.headers.{k}"))?;
            headers.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
        }
    }
    for (k, segs) in &req.headers {
        let v = render_template_with_host(segs, &ExprHostImpl { cx, doc_val: Val::Empty }, &format!("{path}.headers.{k}"))?;
        headers.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
    }
    // DNS 固定解析时补 Host
    for (host, ip) in &src.http.dns {
        if url.contains(ip) && !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
            headers.push(("Host".into(), host.clone()));
        }
    }
    // params
    let encode_cs = req.encode_params.clone();
    let mut query = vec![];
    for (k, p) in &req.params {
        let v = eval_request_param(cx, &stage_name_of(path), p, &format!("{path}.params.{k}"))?;
        let vs = match &v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        query.push(format!(
            "{}={}",
            encode_param(k, encode_cs.as_deref()),
            encode_param(&vs, encode_cs.as_deref())
        ));
    }
    let mut full_url = url;
    if !query.is_empty() {
        let sep = if full_url.contains('?') { "&" } else { "?" };
        full_url = format!("{full_url}{sep}{}", query.join("&"));
    }
    // body
    let mut body_bytes: Option<Vec<u8>> = None;
    if let Some(b) = &req.body {
        let body_cs = req.encode_body.clone();
        match b.kind.as_str() {
            "json" => {
                let mut obj = serde_json::Map::new();
                for (k, p) in &b.data {
                    let v = eval_request_param(cx, &stage_name_of(path), p, &format!("{path}.body.data.{k}"))?;
                    obj.insert(k.clone(), v);
                }
                let s = serde_json::Value::Object(obj).to_string();
                let bytes = match body_cs.as_deref() {
                    Some("gbk") | Some("gb2312") => {
                        let (enc, _, _) = encoding_rs::GBK.encode(&s);
                        enc.into_owned()
                    }
                    _ => s.into_bytes(),
                };
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                    headers.push(("Content-Type".into(), "application/json".into()));
                }
                body_bytes = Some(bytes);
            }
            "form" => {
                let mut pairs = vec![];
                for (k, p) in &b.data {
                    let v = eval_request_param(cx, &stage_name_of(path), p, &format!("{path}.body.data.{k}"))?;
                    let vs = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    pairs.push(format!(
                        "{}={}",
                        encode_param(k, body_cs.as_deref()),
                        encode_param(&vs, body_cs.as_deref())
                    ));
                }
                if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                    headers.push(("Content-Type".into(), "application/x-www-form-urlencoded".into()));
                }
                body_bytes = Some(pairs.join("&").into_bytes());
            }
            "raw" => {
                if let Some(r) = &b.raw {
                    let v = crate::exec::ctx::eval_param(r, &ExprHostImpl { cx, doc_val: Val::Empty }, &format!("{path}.body.raw"))?;
                    body_bytes = Some(v.as_str().unwrap_or_default().as_bytes().to_vec());
                }
            }
            _ => {}
        }
    }
    // engine / webview
    let timeout = req.timeout.unwrap_or(src.limits.timeout_ms);
    let http_req = HttpRequest {
        method: req.method.clone(),
        url: full_url.clone(),
        headers,
        body: body_bytes,
        timeout_ms: timeout,
        max_bytes: src.limits.max_bytes,
    };
    // rsl-debug/1 捕获（T1）：请求侧信息在发送前成形；响应侧由各分支写入。
    let template = debug::request_template(req, &src.base_url);
    let detect = src.requires.features.iter().any(|f| f == "detect");
    let push_record = |cx: &ExecCx, response: Option<crate::exec::ctx::DebugRespInfo>, attempts: u64| {
        cx.debug.borrow_mut().push(crate::exec::ctx::DebugRecord {
            request: debug_req_for_capture(&template, &http_req, req, attempts),
            response,
        });
    };
    match req.engine.as_str() {
        "webview" => {
            let port = cx.webview.ok_or_else(|| crate::webview::unsupported(&format!("{path}.engine")))?;
            let r = crate::webview::render(port, &http_req.url, &http_req.headers, req.web_js.as_ref(), req.sniff.as_deref(), timeout)?;
            // sniff 产出 var:sniffed
            if !r.sniffed.is_empty() {
                cx.vars.insert("sniffed".into(), json!(r.sniffed));
            }
            let resp = HttpResponse {
                status: 200,
                headers: vec![],
                body: r.body.into_bytes(),
                final_url: http_req.url.clone(),
                ms: 0,
            };
            push_record(&cx, Some(debug::capture_response(req, &resp, detect, path)), 1);
            Ok(resp)
        }
        _ => {
            // auto：先 HTTP（autoUpgradeIf 命中升级一次的逻辑由调用方在得到 HTML 后处理）
            let retry = req.retry.clone().unwrap_or_else(|| src.http.retry.clone());
            let mut client = HttpClient {
                transport: cx.transport,
                http: &src.http,
                rate: RateLimiter::from_ir(&src.http),
                last_attempts: 0,
            };
            let resp = match client.send(&http_req, &retry, &src.id) {
                Ok(r) => r,
                Err(d) => {
                    // 网络层失败：协议标头保留请求侧真实数据，响应缺失（诚实降级）
                    push_record(&cx, None, client.last_attempts);
                    return Err(d);
                }
            };
            // HTTP 错误态也捕获（协议标头 tab 要能看到真实状态码，T7）
            push_record(&cx, Some(debug::capture_response(req, &resp, detect, path)), client.last_attempts);
            if resp.status == 0 || (resp.status >= 400) {
                return Err(Diagnostic::new(
                    Code::EHttp,
                    path,
                    format!("HTTP {}", resp.status),
                    format!("补 Referer/UA 或先登录；retry.on = {:?}，未重试", retry.on),
                )
                .rule(&full_url)
                .retryable(retry.on.contains(&(resp.status as i64))));
            }
            Ok(resp)
        }
    }
}

/// 捕获用请求侧信息（send_request 内部；engine 分派前调用）。
fn debug_req_for_capture(
    template: &str,
    http_req: &HttpRequest,
    req: &RequestIR,
    attempts: u64,
) -> crate::exec::ctx::DebugReqInfo {
    let parse = match req.response {
        RespKind::Json => "json",
        RespKind::Bytes => "bytes",
        RespKind::Text => "text",
        RespKind::Html => "html",
    };
    let engine = match req.engine.as_str() {
        "webview" => "webview",
        _ => "http",
    };
    crate::exec::ctx::DebugReqInfo {
        template: template.to_string(),
        url: http_req.url.clone(),
        method: req.method.clone(),
        headers: http_req.headers.clone(),
        engine: engine.to_string(),
        parse: parse.to_string(),
        attempts,
    }
}

/// 请求模板渲染（支持 expr）。
pub fn render_request_template(cx: &mut ExecCx, segs: &[crate::sugar::Seg], path: &str) -> Result<Value, Diagnostic> {
    render_template_with_host(segs, &ExprHostImpl { cx, doc_val: Val::Empty }, path)
}

/// 缓存块预载（T3.10：键维度化防串书）。
fn prefetch_cache(src: &CompiledSource, cx: &mut ExecCx, _inputs: &RunInputs) -> Result<(), Diagnostic> {
    let entries = src.cache.clone();
    for entry in entries {
        if cx.cache.borrow().contains_key(&entry.name) {
            continue;
        }
        // 维度化键
        let dim_key = match &entry.key {
            Some(ParamExpr::Expr(e)) => {
                let host = ExprHostImpl { cx, doc_val: Val::Empty };
                crate::expr::eval::eval(e, &host, &format!("cache.{}.key", entry.name))?.to_str()
            }
            Some(ParamExpr::Template(segs)) => {
                render_template_with_host(segs, &ExprHostImpl { cx, doc_val: Val::Empty }, "cache.key")?
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            }
            _ => format!("{}/{}", src.id, entry.name),
        };
        let full_key = format!("cache:{dim_key}");
        // 读缓存层（进程内）
        if let Some(v) = cx.cache.borrow().get(&full_key) {
            cx.cache.borrow_mut().insert(entry.name.clone(), v.clone());
            continue;
        }
        if let Some(req) = &entry.req {
            let resp = send_request(src, cx, req, &format!("cache.{}.request", entry.name))?;
            let doc = decode_to_doc(cx, req, &resp, &format!("cache.{}.take", entry.name))?;
            let root = doc_to_val(&doc);
            if let Some(take) = &entry.take {
                let doc_ref: Option<&HtmlDoc> = match &*doc {
                    crate::exec::value::Doc::Html(h) => Some(h),
                    _ => None,
                };
                let mut e = make_eval(cx, "cache", root, doc_ref);
                let r = e.eval_selector(take)?;
                let v = finalize_val(r);
                cx.cache.borrow_mut().insert(full_key.clone(), v.clone());
                cx.cache.borrow_mut().insert(entry.name.clone(), v);
            }
        }
    }
    Ok(())
}

// ───────────── 分页（T3.7） ─────────────

pub enum Paginated {
    /// 列表型阶段：每页 (root, doc)。
    Pages(Vec<(Val, std::boxed::Box<HtmlDoc>)>),
    /// 对象型阶段（detail/content）：最终 out。
    Object(Value),
}

#[allow(clippy::too_many_arguments)]
fn paginate_loop(
    src: &CompiledSource,
    stage: &StageIR,
    stage_name: &str,
    cx: &mut ExecCx,
    inputs: &RunInputs,
    reuse: &mut ResponseReuse,
    caps: &HostCaps,
) -> Result<Paginated, Diagnostic> {
    let is_list_stage = matches!(stage_name, "search" | "explore" | "toc") || stage_name.starts_with("host");
    // content.direct（T3.6 / S§6.6.4）
    if stage.direct {
        let url = inputs
            .chapter
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let carrier = src.carrier.as_str();
        let item = json!({ "url": url });
        return Ok(Paginated::Object(match carrier {
            "comic" => json!({ "images": [item] }),
            "audio" => json!({ "audio": item }),
            "video" => json!({ "video": item }),
            "file" => json!({ "files": [{ "name": "", "url": url }] }),
            _ => json!({ "content": url }),
        }));
    }
    // 一次性对象型（无分页）：detail/content
    if !is_list_stage && stage.paginate.is_none() {
        let (root, doc) = fetch_stage_root(src, stage, cx, inputs, reuse, caps, stage_name)?;
        // onResponse 已在 fetch_stage_root 内处理；parse 后求 out
        let out = eval_object_out(src, stage, stage_name, cx, root, doc)?;
        // nextPage 自动循环拼接（受 maxPages）
        return Ok(Paginated::Object(out));
    }
    // 列表型 + 分页
    let mut pages: Vec<(Val, std::boxed::Box<HtmlDoc>)> = vec![];
    let mut seen_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut page_no = 0i64;
    let max_pages = stage.paginate.as_ref().and_then(|p| p.max).unwrap_or(src.limits.max_pages);
    let mut next_url: Option<String> = None;
    let mut cursor_val: Option<String> = None;
    let stop = stage.paginate.as_ref().map(|p| (p.stop_empty_path.clone(), p.stop_no_new)).unwrap_or((None, false));
    loop {
        page_no += 1;
        if page_no > max_pages {
            cx.warn(Diagnostic::new(
                Code::WBudgetTruncated,
                "limits.maxPages",
                format!("到 {max_pages} 页上限仍可翻页"),
                "目录确实很长就提高 maxPages；否则查 paginate.stopIf",
            ));
            break;
        }
        // 请求
        let mut req = stage.request.clone().unwrap_or(RequestIR {
            url: Some(vec![crate::sugar::Seg::Lit(format!("{}/", src.base_url))]),
            method: "GET".into(),
            params: vec![],
            body: None,
            headers: vec![],
            charset: None,
            encode_params: None,
            encode_body: None,
            response: RespKind::Html,
            engine: "http".into(),
            web_js: None,
            sniff: None,
            auto_upgrade_if: None,
            skip: false,
            retry: None,
            timeout: None,
        });
        // 分页 kind 注入
        if let Some(p) = &stage.paginate {
            match &p.kind {
                PaginateKind::Page { page_var } => {
                    let pn = inputs.page.unwrap_or(1) + page_no - 1;
                    cx.vars.insert(page_var.clone(), json!(pn));
                }
                PaginateKind::Next { .. } => {
                    if let Some(u) = &next_url {
                        req.url = Some(vec![crate::sugar::Seg::Lit(u.clone())]);
                        req.params = vec![];
                    }
                }
                PaginateKind::Cursor { into, .. } => {
                    if let Some(cv) = &cursor_val {
                        inject_into(&mut req, into, cv);
                    }
                }
                PaginateKind::Enumerate { join_with, into, .. } => {
                    {
                        let iv: &String = into;
                        let value = match join_with {
                            Some(_) => {
                                // 单次请求：值拼接由首循环完成
                                String::new()
                            }
                            None => {
                                let values = stage.paginate.as_ref().map(|p| enumerate_values(&p.kind)).unwrap_or_default();
                                values.get((page_no - 1) as usize).map(|v| v.to_string()).unwrap_or_default()
                            }
                        };
                        inject_into(&mut req, iv, &value);
                    }
                }
            }
        }
        // 宿主取消（A5）：页边界 —— 返回已收集页作为部分结果
        if cx.check_cancelled() {
            return Ok(Paginated::Pages(pages));
        }
        let (root, doc, _resp) = fetch_with_request(src, stage, cx, &req, reuse, caps, stage_name)?;
        // enumerate joinWith：只发一次（在第一页时把拼接值注入）
        // cursor 取值
        if let Some(p) = &stage.paginate {
            if let PaginateKind::Cursor { take, .. } = &p.kind {
                let doc_ref: Option<&HtmlDoc> = Some(doc.as_ref());
                let mut e = make_eval(cx, stage_name, root.clone(), doc_ref);
                let r = e.eval_selector(take)?;
                cursor_val = Some(r.to_text());
                if cursor_val.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                    break;
                }
            }
        }
        let doc_keep = doc;
        let doc_ref: Option<&HtmlDoc> = Some(doc_keep.as_ref());
        // 列表为空 → 该页无条目，停止翻页（S§4.7 stopIf.empty 的默认行为）
        let page_empty = {
            let mut pe = make_eval_keep(cx, stage_name, root.clone(), doc_ref);
            stage
                .out
                .list
                .as_ref()
                .map(|l| pe.eval_selector(l).map(|v| v.is_empty_val()).unwrap_or(true))
                .unwrap_or(false)
        };
        // stopIf.noNew：当前页与已收集结果完全无新增 → 不收录该页，停止（S§4.7）
        if stop.1 {
            let new_keys = page_keys(cx, stage, stage_name, &[(root.clone(), doc_keep.clone())])?;
            let before = seen_keys.len();
            seen_keys.extend(new_keys.iter().cloned());
            if seen_keys.len() == before && page_no > 1 {
                break;
            }
        }
        pages.push((root.clone(), doc_keep.clone()));
        if page_empty {
            break;
        }
        // stopIf.empty
        if let Some(empty_path) = &stop.0 {
            let empty_now = pages.last().map(|(r, d)| {
                if empty_path == "out.list" {
                    let mut e = make_eval(cx, stage_name, r.clone(), Some(d));
                    stage.out.list.as_ref().map(|l| {
                        e.eval_selector(l).map(|v| v.is_empty_val()).unwrap_or(true)
                    }).unwrap_or(true)
                } else {
                    false
                }
            }).unwrap_or(false);
            if empty_now {
                break;
            }
        }
        // next
        if let Some(p) = &stage.paginate {
            if let PaginateKind::Next { sels } = &p.kind {
                let doc_ref: Option<&HtmlDoc> = Some(doc_keep.as_ref());
                let mut e = make_eval(cx, stage_name, root.clone(), doc_ref);
                let mut found = None;
                for s in sels {
                    let r = e.eval_selector(s)?;
                    let t = r.to_text();
                    if !t.trim().is_empty() {
                        found = Some(t);
                        break;
                    }
                }
                match found {
                    Some(u) => {
                        let abs = absolutize_url(cx, &u);
                        if Some(&abs) == next_url.as_ref() {
                            break; // 防死循环
                        }
                        next_url = Some(abs);
                    }
                    None => break,
                }
            }
        }
        // enumerate joinWith：单次请求
        if let Some(p) = &stage.paginate {
            if matches!(p.kind, PaginateKind::Enumerate { join_with: Some(_), .. }) {
                break;
            }
        }
        // enumerate 无 joinWith：值用尽即停
        if let Some(p) = &stage.paginate {
            if let PaginateKind::Enumerate { values, .. } = &p.kind {
                if page_no as usize >= values.len() {
                    break;
                }
            }
        }
        // 未声明分页的列表阶段：单页即停
        if stage.paginate.is_none() {
            break;
        }
        // 宿主取消（S§12.5）
        if cx.cancelled.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(Paginated::Pages(pages));
        }
        let _ = inputs;
    }
    Ok(Paginated::Pages(pages))
}

fn enumerate_values(kind: &PaginateKind) -> Vec<i64> {
    match kind {
        PaginateKind::Enumerate { values, .. } => values.clone(),
        _ => vec![],
    }
}

fn inject_into(req: &mut RequestIR, into: &str, value: &str) {
    if let Some(key) = into.strip_prefix("params.") {
        req.params.retain(|(k, _)| k != key);
        req.params.push((key.to_string(), ParamExpr::Raw(IrValue::Str(value.to_string()))));
    } else if let Some(key) = into.strip_prefix("body.data.") {
        if let Some(b) = &mut req.body {
            b.data.retain(|k, _| k != key);
            b.data.insert(key.to_string(), ParamExpr::Raw(IrValue::Str(value.to_string())));
        }
    }
}

fn absolutize_url(cx: &ExecCx, u: &str) -> String {
    if u.starts_with("http") {
        return u.to_string();
    }
    let base = cx.get_var("url").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    crate::exec::eval::absolutize(&base, u)
}

fn page_keys(cx: &mut ExecCx, stage: &StageIR, stage_name: &str, pages: &[(Val, std::boxed::Box<HtmlDoc>)]) -> Result<Vec<String>, Diagnostic> {
    let mut keys = vec![];
    for (root, doc_box) in pages {
        let doc_ref: Option<&HtmlDoc> = Some(doc_box.as_ref());
        let mut e = make_eval_keep(cx, stage_name, root.clone(), doc_ref);
        if let Some(list_sel) = &stage.out.list {
            let lv = e.eval_selector(list_sel)?;
            let entries: Vec<Val> = match lv {
                Val::NodeList(ids) => ids.into_iter().map(Val::Node).collect(),
                Val::Json(Value::Array(a)) => a.into_iter().map(Val::Json).collect(),
                _ => vec![],
            };
            for item in entries {
                let mut ie = make_eval(cx, stage_name, item, Some(doc_box));
                let url = stage.out.map.iter().find(|(k, _)| k == "url").map(|(_, s)| ie.eval_selector(s)).transpose()?;
                keys.push(url.map(|v| v.to_text()).unwrap_or_default());
            }
        }
    }
    Ok(keys)
}

/// 阶段首请求（含 reuseDetailHtml 与 auto 升级）。
fn fetch_stage_root(
    src: &CompiledSource,
    stage: &StageIR,
    cx: &mut ExecCx,
    inputs: &RunInputs,
    reuse: &mut ResponseReuse,
    caps: &HostCaps,
    stage_name: &str,
) -> Result<(Val, std::boxed::Box<HtmlDoc>), Diagnostic> {
    let req = stage.request.clone().unwrap_or(RequestIR {
        url: Some(vec![crate::sugar::Seg::Lit(src.base_url.clone())]),
        method: "GET".into(),
        params: vec![],
        body: None,
        headers: vec![],
        charset: None,
        encode_params: None,
        encode_body: None,
        response: RespKind::Html,
        engine: "http".into(),
        web_js: None,
        sniff: None,
        auto_upgrade_if: None,
        skip: false,
        retry: None,
        timeout: None,
    });
    let _ = inputs;
    let (root, doc, _resp) = fetch_with_request(src, stage, cx, &req, reuse, caps, stage_name)?;
    // JSON/Text 响应的根必须是解析值（对象型阶段的 json: 选择器以它为作用域）；HTML 根为 Empty
    Ok((root, doc))
}

/// 发请求 + onResponse + parse + auto 升级；返回 (root_val, doc, resp)。
fn fetch_with_request(
    src: &CompiledSource,
    stage: &StageIR,
    cx: &mut ExecCx,
    req: &RequestIR,
    reuse: &mut ResponseReuse,
    caps: &HostCaps,
    stage_name: &str,
) -> Result<(Val, std::boxed::Box<HtmlDoc>, HttpResponse), Diagnostic> {
    // reuseDetailHtml（T3.6 / S§6.5.2）：本阶段 URL 与 detail 实际 URL 相同 → 不请求
    let rendered_url = if let Some(u) = &req.url {
        render_request_template(cx, u, &format!("stages.{stage_name}.request.url"))?
            .as_str()
            .unwrap_or_default()
            .to_string()
    } else {
        src.base_url.clone()
    };
    let reuse_hit = stage.reuse_detail_html
        && reuse.detail_url.as_deref() == Some(rendered_url.as_str())
        && reuse.detail_body.is_some();
    let resp: HttpResponse = if reuse_hit {
        // 复用不计入 http.rate（没有请求发生）
        HttpResponse {
            status: 200,
            headers: vec![],
            body: reuse.detail_body.clone().unwrap_or_default(),
            final_url: rendered_url.clone(),
            ms: 0,
        }
    } else {
        let r = send_request(src, cx, req, &format!("stages.{stage_name}.request"))?;
        // 记录 detail 响应供复用
        if stage_name == "detail" {
            reuse.detail_url = Some(rendered_url.clone());
            reuse.detail_body = Some(r.body.clone());
        }
        // auto 升级（S§9.3：先 HTTP，命中 autoUpgradeIf 再升级一次）
        if req.engine == "auto" {
            if let (Some(up), Some(port)) = (&req.auto_upgrade_if, cx.webview) {
                if !caps.webview {
                    return Err(crate::webview::unsupported(&format!("stages.{stage_name}.request.engine")));
                }
                let (text, _, _) = encoding_rs::UTF_8.decode(&r.body);
                if let crate::webview::EngineDecision::WebView = crate::webview::decide_auto(Some(up), Some(&text)) {
                    let wr = crate::webview::render(port, &rendered_url, &[], req.web_js.as_ref(), req.sniff.as_deref(), req.timeout.unwrap_or(src.limits.timeout_ms))?;
                    if !wr.sniffed.is_empty() {
                        cx.vars.insert("sniffed".into(), json!(wr.sniffed));
                    }
                    let doc: std::boxed::Box<HtmlDoc> = Box::new(HtmlDoc::parse(&wr.body));
                    let root = Val::Empty;
                    return Ok((root, doc, r));
                }
            }
        }
        r
    };
    // onResponse（解码后、parse 前，T4.6）
    let mut resp = resp;
    if let Some(hook) = &stage.script.on_response {
        let engine = crate::script::get_engine();
        if let Some(e) = engine {
            let (text, _, had_err) = encoding_rs::UTF_8.decode(&resp.body);
            let _ = had_err;
            let host = CtxHost { cx };
            let input = Value::String(text.into_owned());
            let out = e.call_ctx(&hook.fn_name, &[&input], &host, &format!("stages.{stage_name}.script.onResponse"))?;
            // 返回类型必须与输入一致（str 进 str 出）——U2：E_TYPE 单码双时机
            if !out.is_string() {
                return Err(Diagnostic::new(
                    Code::EType,
                    &format!("stages.{stage_name}.script.onResponse"),
                    format!("onResponse 返回 {}，输入是 str", json_type_name(&out)),
                    "返回类型必须与输入一致（str 进 str 出）",
                ));
            }
            resp.body = out.as_str().unwrap_or_default().as_bytes().to_vec();
        }
    }
    // 内置变量注入（S§5.3：var:url / var:response / var:charset 全部阶段可用）
    cx.vars
        .insert("url".into(), Value::String(resp.final_url.clone()));
    cx.vars.insert(
        "response".into(),
        Value::String(String::from_utf8_lossy(&resp.body).to_string()),
    );
    cx.vars.insert(
        "charset".into(),
        Value::String(
            resp.header("content-type")
                .unwrap_or("utf-8")
                .to_string(),
        ),
    );
    let doc = decode_to_doc(cx, req, &resp, &format!("stages.{stage_name}"))?;
    // parse（预处理响应根，T3.5 / S§6.1.2）
    let (root, doc2) = match *doc {
        crate::exec::value::Doc::Html(h) => {
            let h_box = std::boxed::Box::new(h);
            let root = if let Some(p) = &stage.parse {
                let doc_ref: Option<&HtmlDoc> = Some(h_box.as_ref());
                let mut e = make_eval(cx, stage_name, Val::Empty, doc_ref);
                let r = e.eval_selector(p)?;
                match r {
                    Val::Node(id) => {
                        let sub = HtmlDoc::parse_fragment(&crate::selector::node_inner_html(&h_box.html, id));
                        let _ = sub;
                        Val::Empty
                    }
                    Val::Str(s) => {
                        let v: Value = serde_json::from_str(&s).unwrap_or(Value::String(s.clone()));
                        Val::Json(v)
                    }
                    Val::Json(v) => Val::Json(v),
                    _ => Val::Empty,
                }
            } else {
                Val::Empty
            };
            (root, h_box)
        }
        other => {
            let root = match &other {
                crate::exec::value::Doc::Json(v) => Val::Json(v.clone()),
                crate::exec::value::Doc::Text(t) => Val::Str(t.clone()),
                crate::exec::value::Doc::Bytes(b) => Val::Bytes(b.clone()),
                crate::exec::value::Doc::Html(_) => Val::Empty,
            };
            (root, std::boxed::Box::new(HtmlDoc::parse_fragment("")))
        }
    };
    Ok((root, doc2, resp))
}

/// 对象型 out 求值（detail/content）。
fn eval_object_out(
    src: &CompiledSource,
    stage: &StageIR,
    stage_name: &str,
    cx: &mut ExecCx,
    root: Val,
    doc: std::boxed::Box<HtmlDoc>,
) -> Result<Value, Diagnostic> {
    let mut obj = serde_json::Map::new();
    let doc_ref: Option<&HtmlDoc> = Some(doc.as_ref());
    let mut e = make_eval(cx, stage_name, root.clone(), doc_ref);
    let mut pending_warns2: Vec<Diagnostic> = vec![];
    // detail 字段（title 是章节标题的输出键）
    for (k, sel) in [
        ("title", &stage.out.title),
        ("content", &stage.out.content),
        ("images", &stage.out.images),
        ("audio", &stage.out.audio),
        ("video", &stage.out.video),
        ("files", &stage.out.files),
        ("nextPage", &stage.out.next_page),
        ("payAction", &stage.out.pay_action),
    ] {
        if let Some(s) = sel {
            match e.eval_selector(s) {
                Ok(v) => {
                    obj.insert(k.to_string(), finalize_val(v));
                }
                Err(d) if d.code == Code::ENoMatch => {
                    if !s.optional {
                        pending_warns2.push(
                            Diagnostic::new(
                                Code::WDegraded,
                                format!("stages.{stage_name}.out.{k}"),
                                format!("字段 {k} 未命中，已降级"),
                                "增强字段加 optional:true + default；核心字段请修选择器",
                            )
                            .rule(&s.raw)
                            .degraded(true),
                        );
                    }
                    obj.insert(k.to_string(), s.default.clone().map(|x| x.to_json()).unwrap_or(Value::Null));
                }
                Err(d) => return Err(d),
            }
        }
    }
    // detail 的保留字段（name/author/intro/...）在 out.map 里（模型统一进 map）
    let mut pending_warns: Vec<Diagnostic> = vec![];
    for (k, sel) in &stage.out.map {
        match e.eval_selector(sel) {
            Ok(v) => {
                obj.insert(k.clone(), finalize_val(v));
            }
            Err(d) if d.code == Code::ENoMatch => {
                if !sel.optional {
                    pending_warns.push(
                        Diagnostic::new(
                            Code::WDegraded,
                            format!("stages.{stage_name}.out.{k}"),
                            format!("字段 {k} 未命中，已降级"),
                            "增强字段加 optional:true + default；核心字段请修选择器",
                        )
                        .rule(&sel.raw)
                        .degraded(true),
                    );
                }
                obj.insert(k.clone(), sel.default.clone().map(|x| x.to_json()).unwrap_or(Value::Null));
            }
            Err(d) => return Err(d),
        }
    }
    for w in pending_warns2 {
        pending_warns.push(w);
    }
    // tracks（轨道失败不影响主内容，S§6.6.1）
    if !stage.out.tracks.is_empty() {
        let mut tracks = vec![];
        for t in &stage.out.tracks {
            let mut t_obj = serde_json::Map::new();
            t_obj.insert("kind".into(), Value::String(t.kind.clone()));
            t_obj.insert("format".into(), Value::String(t.format.clone()));
            let content = if let Some(u) = &t.url {
                match e.eval_selector(u) {
                    Ok(v) => Value::String(v.to_text()),
                    Err(_) => Value::Null, // 轨道失败 → 空轨道
                }
            } else if let Some(x) = &t.text {
                match e.eval_selector(x) {
                    Ok(v) => Value::String(v.to_text()),
                    Err(_) => Value::Null,
                }
            } else {
                Value::Null
            };
            if t.url.is_some() {
                t_obj.insert("url".into(), content);
            } else {
                t_obj.insert("text".into(), content);
            }
            tracks.push(Value::Object(t_obj));
        }
        obj.insert("tracks".into(), Value::Array(tracks));
    }
    for w in pending_warns {
        cx.warn(w);
    }
    // toc 的 order/dedupe
    if stage_name == "toc" {
        let list = obj.get("list").cloned().unwrap_or(Value::Array(vec![]));
        obj.insert("list".into(), apply_toc_order(src, stage, list));
    }
    // nextPage 自动循环（受 maxPages，S§10.4）
    apply_next_page(src, stage, cx, stage_name, &mut obj, Some(doc.as_ref()))?;
    Ok(Value::Object(obj))
}

fn apply_toc_order(_src: &CompiledSource, stage: &StageIR, mut list: Value) -> Value {
    if let Value::Array(items) = &mut list {
        match stage.order.unwrap_or(TocOrder::Auto) {
            TocOrder::Desc => items.reverse(),
            TocOrder::Asc => {}
            TocOrder::Auto => {
                // 比较首尾标题的章节序号
                let first = items.first().and_then(|i| i.get("title")).and_then(|t| t.as_str()).map(String::from).unwrap_or_default();
                let last = items.last().and_then(|i| i.get("title")).and_then(|t| t.as_str()).map(String::from).unwrap_or_default();
                let nf = crate::exec::eval::num_chapter(&first).parse::<i64>().ok();
                let nl = crate::exec::eval::num_chapter(&last).parse::<i64>().ok();
                if let (Some(a), Some(b)) = (nf, nl) {
                    if a > b && items.len() > 1 {
                        items.reverse();
                    }
                }
            }
        }
        // dedupe
        if stage.dedupe.unwrap_or(Dedupe::Url) != Dedupe::None {
            let mut seen = std::collections::BTreeSet::new();
            items.retain(|i| {
                let k = match stage.dedupe {
                    Some(Dedupe::Title) => i.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                    _ => i.get("url").and_then(|t| t.as_str()).unwrap_or("").to_string(),
                };
                !k.is_empty() && seen.insert(k)
            });
        }
    }
    list
}

/// nextPage 循环拼接（受 maxPages）。
fn apply_next_page(
    src: &CompiledSource,
    stage: &StageIR,
    cx: &mut ExecCx,
    stage_name: &str,
    obj: &mut serde_json::Map<String, Value>,
    doc: Option<&HtmlDoc>,
) -> Result<(), Diagnostic> {
    let field = if src.carrier == "novel" { "content" } else { return Ok(()) };
    let mut pages = 1i64;
    loop {
        let next_url = obj.get("nextPage").and_then(|v| v.as_str()).map(String::from).unwrap_or_default();

        if next_url.is_empty() || pages >= src.limits.max_pages {
            break;
        }
        // 宿主取消（A5）：nextPage 边界 —— 保留已拼内容
        if cx.check_cancelled() {
            return Ok(());
        }
        let mut req = RequestIR {
            url: Some(vec![crate::sugar::Seg::Lit(next_url.clone())]),
            method: "GET".into(),
            params: vec![],
            body: None,
            headers: vec![],
            charset: None,
            encode_params: None,
            encode_body: None,
            response: RespKind::Html,
            engine: "http".into(),
            web_js: None,
            sniff: None,
            auto_upgrade_if: None,
            skip: false,
            retry: None,
            timeout: None,
        };
        req.charset = stage.request.as_ref().and_then(|r| r.charset.clone());
        let resp = send_request(src, cx, &req, &format!("stages.{stage_name}.nextPage"))?;
        let sub_doc = decode_to_doc(cx, &req, &resp, &format!("stages.{stage_name}.nextPage"))?;
        let sub_html: Option<&HtmlDoc> = match &*sub_doc {
            crate::exec::value::Doc::Html(h) => Some(h),
            _ => None,
        };
        let mut e = make_eval(cx, stage_name, Val::Empty, sub_html);
        if let Some(cs) = &stage.out.content {
            match e.eval_selector(cs) {
                Ok(v) => {
                    let more = v.to_text();
                    let cur = obj.get(field).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let mut merged = cur;
                    if let Value::Array(parts) = v.to_json() {
                        merged.extend(parts);
                    } else {
                        merged.push(Value::String(more));
                    }
                    obj.insert(field.to_string(), Value::Array(merged));
                }
                Err(_) => break,
            }
        }
        // 更新 nextPage
        let mut e2 = make_eval(cx, stage_name, Val::Empty, sub_html);
        let next = match &stage.out.next_page {
            Some(np) => e2.eval_selector(np).map(|v| v.to_text()).unwrap_or_default(),
            None => String::new(),
        };
        obj.insert("nextPage".into(), Value::String(next));
        pages += 1;
    }
    if pages > 1 {
        // 尾页清理 nextPage
        obj.insert("nextPage".into(), Value::String(String::new()));
    }
    let _ = doc;
    Ok(())
}
