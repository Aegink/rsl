//! TASK-BOOK-3 A1–A4 / B1–B3：交互登录会话 + 机械重登。
//! 全部走 FixtureTransport + FakeSessionPort，不联网（S§17 合规：example.com 虚构域）。

use rsl_core::exec::ctx::ExecCx;
use rsl_core::error::Code;
use rsl_core::ports::{
    FakeSessionPort, FixtureTransport, HttpResponse, LoginSessionPort, SessionPoll, SessionResult,
    SystemClock,
};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use rsl_core::{compile, script};

fn resp_json(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: body.as_bytes().to_vec(),
        final_url: String::new(),
        ms: 1,
    }
}

const CHECK_OUT: &str = r#"{"user": null}"#;
const CHECK_IN: &str = r#"{"user": {"id": 42, "name": "tester"}}"#;
const SUBMIT_OK: &str = r#"{"success": true, "token": "t-abc"}"#;
const SEARCH_JSON: &str = r#"{"list": [{"name": "书A", "url": "/b/1"}]}"#;

/// 带 login 的完整源。webview/steps/check 分片以逗号拼接（JSON 恒合法）。
fn login_source(webview: Option<&str>, relogin: Option<i64>, steps: Option<&str>) -> String {
    let mut parts: Vec<String> = vec![
        r#""type": "form""#.into(),
        r#""check": {
      "request": { "url": "https://sess.example.com/user/info", "response": "json" },
      "ok": "json:$.user.id"
    }"#.into(),
    ];
    if let Some(wv) = webview {
        parts.push(wv.to_string());
    }
    if let Some(m) = relogin {
        parts.push(format!(r#""relogin": {{ "max": {m} }}"#));
    }
    if let Some(st) = steps {
        parts.push(st.to_string());
    }
    let login_block = parts.join(",
    ");
    let tpl = r#"{{
  "rsl": "2.0", "id": "sess.example.com", "name": "会话站", "type": "novel",
  "baseUrl": "https://sess.example.com",
  "requires": {{ "features": [], "crypto": [] }},
  "http": {{ "headers": {{ "User-Agent": "rsl-test/1.0" }} }},
  "login": {{
    @@LOGIN@@
  }},
  "stages": {{
    "search": {{
      "requiresLogin": true,
      "request": {{ "url": "https://sess.example.com/search", "params": {{ "q": "var:kw" }} }},
      "out": {{
        "list": "json:$.list[*]",
        "map": {{
          "name": {{ "$": "json:$.name" }},
          "url": {{ "$": "json:$.url" }}
        }},
        "emptyOk": true
      }}
    }},
    "detail": {{
      "request": {{ "url": "var:book.url" }},
      "out": {{ "name": "css:h1", "tocUrl": {{ "$": "css:a.read", "attr": "href", "optional": true }} }}
    }},
    "toc": {{
      "request": {{ "url": "var:book.tocUrl" }},
      "out": {{ "list": "css:#list li", "map": {{
        "title": {{ "$": "css:a", "transform": [{{ "trim": true }}] }},
        "url": {{ "$": "css:a", "attr": "href", "transform": [{{ "absolute": true }}] }}
      }} }}
    }},
    "content": {{
      "request": {{ "url": "var:chapter.url" }},
      "out": {{ "content": {{ "$": "css:div#content", "paragraphs": {{ "split": "br" }} }} }}
    }}
  }}
}}"#;
    tpl.replace("@@LOGIN@@", &login_block).replace("{{", "{").replace("}}", "}")
}

fn webview_block() -> &'static str {
    r#""webview": { "entry": "https://sess.example.com/login", "success": "css:div.user-avatar" }"#
}

fn form_steps() -> &'static str {
    r#""fields": [ { "key": "username", "label": "账号" }, { "key": "password", "label": "密码", "secret": true, "default": "pw-demo" } ],
    "submit": { "request": { "url": "https://sess.example.com/login", "method": "POST", "body": { "data": { "u": "var:login.username", "p": "var:login.password" } }, "response": "json" }, "ok": "json:$.success" }
"#
}

/// 统一跑法：编译 → FixtureTransport（responses 顺序回放）→ 可选会话端口 → run_stage。
fn run_with(
    source: &str,
    stage: &str,
    responses: Vec<HttpResponse>,
    port: Option<&'static FakeSessionPort>,
) -> (Result<rsl_core::stage::StageResult, rsl_core::error::Diagnostic>, ExecCx<'static>) {
    let out = compile(source);
    assert!(out.source.is_some(), "fixture 源必须可编译：{:?}", out.errors().iter().map(|d| format!("{} {}", d.code.as_str(), d.path)).collect::<Vec<_>>());
    let src: &'static rsl_core::ir::CompiledSource = Box::leak(Box::new(out.source.unwrap()));
    script::setup_engine(src);
    let ft = FixtureTransport { responses, idx: Default::default() };
    let ft: &'static FixtureTransport = Box::leak(Box::new(ft));
    let clock: &'static SystemClock = Box::leak(Box::new(SystemClock));
    let mut cx = ExecCx::new(src, ft, clock);
    if let Some(p) = port {
        cx.login_session = Some(p as &dyn LoginSessionPort);
    }
    let mut reuse = ResponseReuse::default();
    let inputs = RunInputs {
        kw: Some("斗".into()),
        login_values: [("username".to_string(), "demo-user".to_string())].into_iter().collect(),
        ..Default::default()
    };
    let r = run_stage(src, stage, &mut cx, &inputs, &mut reuse, &HostCaps::default());
    (r, cx)
}

// ── A1：端口单测（begin 记录；poll 脚本出队）──
#[test]
fn a1_fake_session_port_script() {
    let port = FakeSessionPort::new(vec![
        SessionPoll::Pending,
        SessionPoll::Done(SessionResult {
            cookies: vec![("sid".into(), "s-1".into())],
            ua: Some("ua-x".into()),
        }),
    ]);
    port.begin("https://sess.example.com/login").unwrap();
    assert_eq!(port.began.lock().unwrap().len(), 1);
    assert!(matches!(port.poll(), SessionPoll::Pending));
    match port.poll() {
        SessionPoll::Done(r) => {
            assert_eq!(r.cookies, vec![("sid".to_string(), "s-1".to_string())]);
            assert_eq!(r.ua.as_deref(), Some("ua-x"));
        }
        _ => panic!("应为 Done"),
    }
    // 脚本耗尽 → Pending（不会 panic）
    assert!(matches!(port.poll(), SessionPoll::Pending));
}

// ── A2：契约正反例 ──
#[test]
fn a2_webview_contract_positive() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()));
    let out = compile(&src);
    assert!(
        out.source.is_some(),
        "webview 契约正例必须编译：{:?}",
        out.errors().iter().map(|d| format!("{} {}", d.code.as_str(), d.path)).collect::<Vec<_>>()
    );
}

#[test]
fn a2_webview_contract_missing_entry() {
    let bad = login_source(Some(r#""webview": { "success": "css:div.user" }"#.into()), None, Some(form_steps()));
    let out = compile(&bad);
    assert!(out.errors().iter().any(|d| d.code == Code::ESchema && d.path.contains("login.webview.entry")));
}

#[test]
fn a2_webview_contract_missing_success() {
    let bad = login_source(Some(r#""webview": { "entry": "https://sess.example.com/login" }"#.into()), None, Some(form_steps()));
    let out = compile(&bad);
    assert!(out.errors().iter().any(|d| d.code == Code::ESchema && d.path.contains("login.webview.success")));
}

#[test]
fn a2_type_webview_enum_ok() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()))
        .replace("\"type\": \"form\"", "\"type\": \"webview\"");
    let out = compile(&src);
    assert!(out.source.is_some(), "type=webview 是合法枚举");
}

// ── B1：relogin 契约 ──
#[test]
fn b1_relogin_max_ok() {
    let src = login_source(None, Some(2), Some(form_steps()));
    assert!(compile(&src).source.is_some(), "max=2 是上限内");
}

#[test]
fn b1_relogin_max_over_limit() {
    let src = login_source(None, Some(3), Some(form_steps()));
    let out = compile(&src);
    assert!(out
        .errors()
        .iter()
        .any(|d| d.code == Code::ESchema && d.path == "login.relogin.max"), "max>2 → E_SCHEMA");
}

// ── A3：webview 会话执行路径（Fake 端口脚本）──
#[test]
fn a3_webview_session_recovers() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()))
        .replace("\"type\": \"form\"", "\"type\": \"webview\"");
    let port = FakeSessionPort::new(vec![
        SessionPoll::Pending,
        SessionPoll::Done(SessionResult {
            cookies: vec![("sid".into(), "sess-42".into())],
            ua: Some("rsl-test/1.0".into()), // 与 http.headers 一致（A4 无警告）
        }),
    ]);
    let (r, cx) = run_with(
        &src,
        "search",
        vec![
            resp_json(CHECK_OUT),  // check 首败
            resp_json(CHECK_IN),   // 会话后 check 复查
            resp_json(SEARCH_JSON), // 阶段本体
        ],
        Some(Box::leak(Box::new(port))),
    );
    assert!(r.is_ok(), "会话回捕后阶段必须成功：{:?}", r.err().map(|d| format!("{} {}", d.code.as_str(), d.message)));
    // 回捕 cookie 入隔离键（cookie_scope = 源 id）
    assert_eq!(cx.get_cookie("sid"), Some("sess-42".to_string()));
    // A4：UA 一致 → 无 W_DEGRADED
    let warns = cx.warnings.borrow();
    assert!(!warns.iter().any(|w| w.code == Code::WDegraded));
}

#[test]
fn a3_no_port_e_unsupported() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()))
        .replace("\"type\": \"form\"", "\"type\": \"webview\"");
    let (r, _cx) = run_with(
        &src,
        "search",
        vec![resp_json(CHECK_OUT)],
        None, // 未注入端口
    );
    let d = r.expect_err("无端口必须 E_UNSUPPORTED");
    assert_eq!(d.code, Code::EUnsupported);
    assert!(d.path.contains("login.webview"));
}

// ── A4：UA 不一致 → W_DEGRADED（cookie 照常入隔离键，V3-D2）──
#[test]
fn a4_ua_mismatch_w_degraded() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()))
        .replace("\"type\": \"form\"", "\"type\": \"webview\"");
    let port = FakeSessionPort::new(vec![SessionPoll::Done(SessionResult {
        cookies: vec![("sid".into(), "s-9".into())],
        ua: Some("Mozilla/5.0 (WebView-Tachiyomi)".into()), // 与 rsl-test/1.0 不一致
    })]);
    let (r, cx) = run_with(
        &src,
        "search",
        vec![resp_json(CHECK_OUT), resp_json(CHECK_IN), resp_json(SEARCH_JSON)],
        Some(Box::leak(Box::new(port))),
    );
    assert!(r.is_ok(), "UA 不一致是警告不是失败（V3-D2）");
    assert_eq!(cx.get_cookie("sid"), Some("s-9".to_string()), "cookie 照常入隔离键");
    let warns = cx.warnings.borrow();
    assert!(
        warns.iter().any(|w| w.code == Code::WDegraded && w.path.contains("login.webview")),
        "必须出现 W_DEGRADED：{:?}",
        warns.iter().map(|w| w.code.as_str()).collect::<Vec<_>>()
    );
}

// ── B2：机械重登（check 首败 → steps 重放 → check 复查 → 成功；trace 记录）──
#[test]
fn b2_relogin_machine_success() {
    let src = login_source(None, Some(1), Some(form_steps()));
    let (r, cx) = run_with(
        &src,
        "search",
        vec![
            resp_json(CHECK_OUT),   // check 首败
            resp_json(SUBMIT_OK),   // 机械重放 submit
            resp_json(CHECK_IN),    // check 复查
            resp_json(SEARCH_JSON), // 阶段本体
        ],
        None,
    );
    assert!(r.is_ok(), "首败→重登→成功：{:?}", r.err().map(|d| format!("{} {}", d.code.as_str(), d.message)));
    let trace = cx.trace.borrow();
    assert!(
        trace.iter().any(|t| t.step.contains("relogin")),
        "trace 必须记录重登：{:?}",
        trace.iter().map(|t| t.step.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn b2_relogin_exhausted() {
    let src = login_source(None, Some(1), Some(form_steps()));
    let (r, _cx) = run_with(
        &src,
        "search",
        vec![
            resp_json(CHECK_OUT), // check 永远失败（last repeats）
            resp_json(SUBMIT_OK),
        ],
        None,
    );
    let d = r.expect_err("max 耗尽仍 E_LOGIN_REQUIRED");
    assert_eq!(d.code, Code::ELoginRequired);
    assert!(d.path == "login.relogin", "path 指向重登：{}", d.path);
}

/// V3-D4：交互型登录（qr）机械重登 → E_CHALLENGE。
#[test]
fn b2_relogin_qr_e_challenge() {
    let src = login_source(None, Some(1), Some(form_steps())).replace("\"type\": \"form\"", "\"type\": \"qr\"");
    let (r, _cx) = run_with(
        &src,
        "search",
        vec![resp_json(CHECK_OUT)],
        None,
    );
    let d = r.expect_err("qr 机械重登必须 E_CHALLENGE");
    assert_eq!(d.code, Code::EChallenge);
}

/// V3-D4：凭据缺失（无值无默认）→ E_CHALLENGE。
#[test]
fn b2_relogin_missing_field_e_challenge() {
    // username 无宿主值也无 default
    let src = login_source(None, Some(1), Some(form_steps()));
    let out = compile(&src);
    let src_static: &'static rsl_core::ir::CompiledSource =
        Box::leak(Box::new(out.source.unwrap()));
    script::setup_engine(src_static);
    let ft = FixtureTransport { responses: vec![resp_json(CHECK_OUT)], idx: Default::default() };
    let clock = SystemClock;
    let mut cx = ExecCx::new(src_static, &ft, &clock);
    let mut reuse = ResponseReuse::default();
    let inputs = RunInputs::default(); // 无 login_values
    let r = run_stage(src_static, "search", &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let d = r.expect_err("凭据缺失必须 E_CHALLENGE");
    assert_eq!(d.code, Code::EChallenge);
}

// ── B3：取消 ──
#[test]
fn b3_relogin_cancel_token() {
    let src = login_source(None, Some(2), Some(form_steps()));
    let out = compile(&src);
    let src_static: &'static rsl_core::ir::CompiledSource =
        Box::leak(Box::new(out.source.unwrap()));
    script::setup_engine(src_static);
    let ft = FixtureTransport { responses: vec![resp_json(CHECK_OUT)], idx: Default::default() };
    let clock = SystemClock;
    let mut cx = ExecCx::new(src_static, &ft, &clock);
    let token = rsl_core::ports::CancelToken::new();
    cx.cancel_token = Some(token.clone());
    let mut reuse = ResponseReuse::default();
    let inputs = RunInputs {
        login_values: [("username".to_string(), "u".to_string())].into_iter().collect(),
        ..Default::default()
    };
    token.cancel(); // 先取消再跑 → 重登循环边界命中
    let r = run_stage(src_static, "search", &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let d = r.expect_err("取消必须终止");
    assert_eq!(d.code, Code::ECancelled);
}

#[test]
fn b3_webview_poll_cancel_token() {
    let src = login_source(Some(webview_block()), None, Some(form_steps()))
        .replace("\"type\": \"form\"", "\"type\": \"webview\"");
    let out = compile(&src);
    let src_static: &'static rsl_core::ir::CompiledSource =
        Box::leak(Box::new(out.source.unwrap()));
    script::setup_engine(src_static);
    let ft = FixtureTransport { responses: vec![resp_json(CHECK_OUT)], idx: Default::default() };
    let clock = SystemClock;
    let mut cx = ExecCx::new(src_static, &ft, &clock);
    let port = FakeSessionPort::new(vec![SessionPoll::Pending, SessionPoll::Pending]);
    cx.login_session = Some(Box::leak(Box::new(port)));
    let token = rsl_core::ports::CancelToken::new();
    cx.cancel_token = Some(token.clone());
    token.cancel();
    let mut reuse = ResponseReuse::default();
    let inputs = RunInputs::default();
    let r = run_stage(src_static, "search", &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let d = r.expect_err("轮询边界取消必须 E_CANCELLED");
    assert_eq!(d.code, Code::ECancelled);
}

/// R§4.5 既有语义回归：无 relogin（max=0）时 check 失败即 E_LOGIN_REQUIRED（一次都不重放）。
#[test]
fn b2_relogin_zero_keeps_legacy_semantics() {
    let src = login_source(None, None, Some(form_steps()));
    let (r, cx) = run_with(
        &src,
        "search",
        vec![resp_json(CHECK_OUT), resp_json(SUBMIT_OK)],
        None,
    );
    let d = r.expect_err("默认 max=0 不自动重登");
    assert_eq!(d.code, Code::ELoginRequired);
    let trace = cx.trace.borrow();
    assert!(!trace.iter().any(|t| t.step.contains("relogin")), "不得有重登 trace");
}
