//! T3.9 登录执行：单步/多步推进、check 失效检测、隔离键、token/cookie 产出。

use super::{RunInputs, send_request, decode_to_doc, doc_to_val};
use crate::error::{Code, Diagnostic};
use crate::exec::eval::Eval;
use crate::exec::value::Val;
use crate::exec::ctx::ExecCx;
use crate::ir::{CompiledSource, LoginIR};
use crate::ports::{SessionPoll, sleep_ms};
use serde_json::Value;

/// 阶段是否需要登录（requiresLogin 或声明了 requiresLogin 的 hostStages）。
pub fn login_needed_for(src: &CompiledSource, _stage: &str) -> bool {
    // 五阶段默认不强制；hostStages.requiresLogin 在 run_stage 判断
    let _ = src;
    false
}

/// 确保登录态：check 通过或已登录则跳过；失效时按 A3 走 webview 会话（声明了才可），
/// 否则 E_LOGIN_REQUIRED（宿主据此弹登录 UI）。语义与既有版本一致——本函数只加增量分支。
pub fn ensure_login(src: &CompiledSource, cx: &mut ExecCx, inputs: &RunInputs) -> Result<(), Diagnostic> {
    let Some(l) = &src.login else {
        return Err(Diagnostic::new(
            Code::ELoginRequired,
            "login",
            "源未声明 login，但该阶段需要登录",
            "宿主引导用户在支持登录的源上操作",
        ));
    };
    // check 缓存 TTL（loginCheckTtlMs，默认 300000）
    if login_state_valid(cx) {
        return Ok(());
    }
    if let (Some(req), Some(ok)) = (&l.check_req, &l.check_ok) {
        if eval_check(src, cx, req, ok)? {
            mark_login_valid(cx);
            return Ok(());
        }
        // ── A3：webview 交互会话（声明了 login.webview 才可；未注入端口 → E_UNSUPPORTED）──
        if let Some(wv) = &l.webview {
            run_webview_session(src, cx, wv)?;
            // 回捕后重跑 check 一次
            if eval_check(src, cx, req, ok)? {
                mark_login_valid(cx);
                return Ok(());
            }
            return Err(Diagnostic::new(
                Code::ELoginRequired,
                "login.check",
                "webview 会话完成后 check 仍未通过",
                "success 判定与 check 口径需一致；隔离键含登录名",
            )
            .rule(&ok.raw));
        }
        Err(Diagnostic::new(
            Code::ELoginRequired,
            "login.check",
            "check 未通过（登录态失效）",
            "先用宿主登录界面完成登录；注意隔离键含登录名",
        )
        .rule(&ok.raw))
    } else {
        // 无 check：按 token/cookie 存在性判断
        let has_token = cx.tokens.borrow().values().next().is_some()
            || cx.get_cookie("sessionid").is_some();
        if has_token {
            Ok(())
        } else {
            Err(Diagnostic::new(
                Code::ELoginRequired,
                "login",
                "未登录且源未声明 check",
                "宿主先完成 login.steps 再调用",
            ))
        }
    }
}

/// check 判定求值（ensure_login 与 webview 会话复用）。
fn eval_check(
    src: &CompiledSource,
    cx: &mut ExecCx,
    req: &crate::ir::RequestIR,
    ok: &crate::ir::SelectorIR,
) -> Result<bool, Diagnostic> {
    let resp = send_request(src, cx, req, "login.check.request")?;
    let doc = decode_to_doc(cx, req, &resp, "login.check")?;
    let root = doc_to_val(&doc);
    let mut e = Eval {
        cx,
        doc: None,
        origin: root,
        path: "login.check.ok".into(),
        stage: "login".into(),
        keep_nodes: false,
        cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
    };
    let r = match e.eval_selector(ok) {
        Ok(r) => r,
        // check 语义：ok 未命中 = 未登录（而非错误）；契约提示"选一个未登录时不存在的字段"
        Err(d) if d.code == Code::ENoMatch => return Ok(false),
        Err(d) => return Err(d),
    };
    Ok(match r {
        Val::Bool(b) => b,
        other => !other.is_empty_val(),
    })
}

/// A3：webview 交互登录会话（begin → 同步轮询 → 回捕 cookie → UA 一致性校验）。
fn run_webview_session(
    src: &CompiledSource,
    cx: &mut ExecCx,
    wv: &crate::ir::WebviewLoginIR,
) -> Result<(), Diagnostic> {
    let Some(port) = cx.login_session else {
        return Err(Diagnostic::new(
            Code::EUnsupported,
            "login.webview",
            "源声明了 webview 登录，但宿主未注入 LoginSessionPort",
            "宿主实现 begin/poll 端口并注入 ExecCx.login_session；或改用 form 登录",
        ));
    };
    port.begin(&wv.entry)?;
    loop {
        // B3：轮询边界可取消
        if cx.check_cancelled() {
            return Err(Diagnostic::new(
                Code::ECancelled,
                "login.webview",
                "登录会话等待被取消",
                "已停止轮询；宿主可关闭会话 UI",
            ));
        }
        match port.poll() {
            SessionPoll::Pending => {
                sleep_ms(20);
            }
            SessionPoll::Cancelled => {
                return Err(Diagnostic::new(
                    Code::ECancelled,
                    "login.webview",
                    "宿主会话 UI 被用户关闭",
                    "用户取消登录；阶段终止",
                ));
            }
            SessionPoll::Done(res) => {
                // A4：UA 一致性（V3-D2：不一致 → W_DEGRADED 警告，cookie 照常入隔离键）
                if let Some(ua) = &res.ua {
                    if let Some(http_ua) = configured_ua(src, cx) {
                        if *ua != http_ua {
                            cx.warn(Diagnostic::new(
                                Code::WDegraded,
                                "login.webview",
                                "会话 UA 与 http.headers UA 不一致（Cloudflare 等网关可能拒收 cookie）",
                                "让宿主 WebView 使用与 HTTP 客户端相同的 UA（Tachiyomi 头号失败教训）",
                            ));
                        }
                    }
                }
                // 回捕 cookie 入隔离键（V3-D2：有效性由 success/check 判定把关）
                for (k, v) in &res.cookies {
                    if !k.is_empty() {
                        cx.set_cookie(k, v);
                    }
                }
                return Ok(());
            }
        }
    }
}

/// 配置态 UA（http.headers 里的 User-Agent；渲染 var:/let: 模板）。
fn configured_ua(src: &CompiledSource, cx: &ExecCx) -> Option<String> {
    let (_, segs) = src
        .http
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("user-agent"))?;
    match crate::exec::ctx::render_template(cx, segs) {
        Ok(v) => v.as_str().filter(|s| !s.is_empty()).map(String::from),
        Err(_) => None,
    }
}

/// B2：机械重登包装——check 首败且 relogin.max > 0 时，机器重放 login.steps 后重试。
/// V3-D4：机械重登不含任何交互（webview/qr 型登录、缺失凭据 → E_CHALLENGE 抛宿主）。
pub fn ensure_login_with_relogin(
    src: &CompiledSource,
    cx: &mut ExecCx,
    inputs: &RunInputs,
) -> Result<(), Diagnostic> {
    match ensure_login(src, cx, inputs) {
        Ok(()) => Ok(()),
        Err(d) if d.code == Code::ELoginRequired => {
            let max = src.login.as_ref().map(|l| l.relogin_max).unwrap_or(0);
            let mut tried: i64 = 0;
            loop {
                if tried >= max {
                    return Err(Diagnostic::new(
                        Code::ELoginRequired,
                        "login.relogin",
                        format!("机械重登 {tried} 次后登录态仍失效"),
                        "检查账号有效性；提高 relogin.max 上限为 2（V3-D3 防打站点）",
                    ));
                }
                // B3：重登循环边界可取消
                if cx.check_cancelled() {
                    return Err(Diagnostic::new(
                        Code::ECancelled,
                        "login.relogin",
                        "重登循环被取消",
                        "已停止；宿主可回放 partial 结果",
                    ));
                }
                tried += 1;
                machine_relogin(src, cx, inputs, tried as usize)?;
                match ensure_login(src, cx, inputs) {
                    Ok(()) => return Ok(()),
                    Err(d2) if d2.code == Code::ELoginRequired => continue,
                    Err(d2) => return Err(d2),
                }
            }
        }
        Err(d) => Err(d),
    }
}

/// B2：一次机械重登（steps 全机器重放；交互型 → E_CHALLENGE；trace 记录）。
fn machine_relogin(
    src: &CompiledSource,
    cx: &mut ExecCx,
    inputs: &RunInputs,
    attempt: usize,
) -> Result<(), Diagnostic> {
    let Some(l) = &src.login else { return Ok(()) };
    // V3-D4：交互型登录不机械重放
    if l.webview.is_some() || l.login_type == "qr" || l.login_type == "webview" {
        return Err(Diagnostic::new(
            Code::EChallenge,
            "login.relogin",
            format!("机械重登不支持交互型登录（type={}）", l.login_type),
            "交互步骤由宿主引导用户完成（E_CHALLENGE）；改用 form 型登录才可自动重登",
        ));
    }
    // 凭据齐全性：任一字段既无宿主值又无默认值 → 需要交互 → E_CHALLENGE
    for step in &l.steps {
        for f in &step.fields {
            let has_val = inputs.login_values.contains_key(&f.key)
                || f.default.is_some();
            if !has_val {
                return Err(Diagnostic::new(
                    Code::EChallenge,
                    "login.relogin",
                    format!("字段 `{}` 无存储凭据，机械重登需要交互", f.key),
                    "宿主预存 login 值（隔离键）或给字段声明 default",
                ));
            }
        }
    }
    let t0 = std::time::Instant::now();
    let r = run_login_steps(src, cx, inputs);
    cx.trace.borrow_mut().push(crate::exec::ctx::TraceEntry {
        stage: "login".into(),
        step: format!("relogin[attempt {attempt}]"),
        rule: format!("relogin.max={}", l.relogin_max),
        matched: usize::from(r.is_ok()),
        ms: t0.elapsed().as_millis() as u64,
        bridge: 0,
        sample: None,
    });
    r
}

fn login_state_valid(cx: &ExecCx) -> bool {
    // 进程内标记：tokens 非空视为已校验（TTL 由宿主管）
    !cx.tokens.borrow().is_empty()
}

fn mark_login_valid(cx: &ExecCx) {
    let _ = cx;
}

/// 登录步骤推进（宿主渲染 fields + 回传值；引擎驱动 next.when）。
pub fn run_login_steps(
    src: &CompiledSource,
    cx: &mut ExecCx,
    inputs: &RunInputs,
) -> Result<(), Diagnostic> {
    let Some(l) = &src.login else {
        return Err(Diagnostic::new(Code::ELoginRequired, "login", "源未声明 login", "—"));
    };
    let mut current: Option<String> = l.steps.first().map(|s| s.id.clone());
    let mut guard = 0usize;
    while let Some(step_id) = current {
        // 宿主取消（A5）：登录步边界
        if cx.check_cancelled() {
            return Err(Diagnostic::new(
                Code::ECancelled,
                "login.steps",
                "登录中途被取消",
                "检查取消时机或重试",
            ));
        }
        guard += 1;
        if guard > 16 {
            return Err(Diagnostic::new(
                Code::ESchema,
                "login.steps",
                "登录步骤推进超过 16 步",
                "检查 next.when/next.to 是否成环",
            ));
        }
        let Some(step) = l.steps.iter().find(|s| s.id == step_id) else {
            break;
        };
        // 注入 login.<field>
        for f in &step.fields {
            if let Some(v) = inputs.login_values.get(&f.key) {
                cx.vars.insert(format!("login.{}", f.key), Value::String(v.clone()));
            } else if let Some(d) = &f.default {
                cx.vars.insert(format!("login.{}", f.key), d.to_json());
            }
        }
        // submit
        if let Some(sub) = &step.submit {
            let resp = send_request(src, cx, &sub.req, &format!("login.steps[{}].submit.request", step.id))?;
            let doc = decode_to_doc(cx, &sub.req, &resp, "login.submit")?;
            let root = doc_to_val(&doc);
            let _doc_ref = Some(doc);
            let mut e = Eval {
                cx,
                doc: None,
                origin: root.clone(),
                path: format!("login.steps[{}].submit.ok", step.id),
                stage: "login".into(),
                keep_nodes: false,
                cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
            };
            let r = e.eval_selector(&sub.ok)?;
            let ok = match r {
                Val::Bool(b) => b,
                other => !other.is_empty_val(),
            };
            if !ok {
                return Err(Diagnostic::new(
                    Code::ELoginRequired,
                    format!("login.steps[{}].submit.ok", step.id),
                    "登录提交未通过 ok 判定",
                    "检查账号/验证码；ok 应选登录成功才存在的字段",
                )
                .rule(&sub.ok.raw));
            }
            // token / cookie 产出
            if let Some(tok) = &sub.token {
                let mut e2 = Eval {
                    cx,
                    doc: None,
                    origin: root.clone(),
                    path: format!("login.steps[{}].submit.token", step.id),
                    stage: "login".into(),
                    keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                if let Ok(v) = e2.eval_selector(tok) {
                    let t = v.to_text();
                    if !t.is_empty() {
                        cx.tokens.borrow_mut().insert("auth".into(), t);
                    }
                }
            }
            if let Some(ck) = &sub.cookie {
                let mut e3 = Eval {
                    cx,
                    doc: None,
                    origin: root.clone(),
                    path: format!("login.steps[{}].submit.cookie", step.id),
                    stage: "login".into(),
                    keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                if let Ok(v) = e3.eval_selector(ck) {
                    let name = extract_cookie_name(&ck.raw);
                    let val = v.to_text();
                    if !name.is_empty() && !val.is_empty() {
                        cx.set_cookie(&name, &val);
                    }
                }
            }
            // next 推进
            if let Some(to) = &step.next_to {
                // next.when 在该步提交成功后求值（S§4.5.2）
                if let Some(when) = &step.next_when {
                    let mut e4 = Eval {
                        cx,
                        doc: None,
                        origin: root,
                        path: format!("login.steps[{}].next.when", step.id),
                        stage: "login".into(),
                        keep_nodes: false,
                    cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                    };
                    let r = e4.eval_selector(when)?;
                    let go = match r {
                        Val::Bool(b) => b,
                        other => !other.is_empty_val(),
                    };
                    current = if go { Some(to.clone()) } else { None };
                } else {
                    current = Some(to.clone());
                }
            } else {
                current = None;
            }
        } else {
            current = None;
        }
        mark_login_valid(cx);
    }
    Ok(())
}

/// 从 {"$":"cookie:sessionid"} 提取 cookie 名。
fn extract_cookie_name(raw: &str) -> String {
    raw.strip_prefix("cookie:").unwrap_or(raw).to_string()
}

/// 引擎版本（供绑定层上报）。
pub fn engine_supports(l: &LoginIR) -> bool {
    !l.steps.is_empty()
}
