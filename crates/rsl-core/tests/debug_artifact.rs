//! rsl-debug/1 调试产物单测（TASK-BOOK-4 T1）：fixture 重放不联网，
//! 断言四数据面（请求/响应/解析/诊断）+ 分页多请求 + skip 阶段 request=null + 预览截断。

use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{Clock, FixtureTransport, MemoryCookieStore, SystemClock};
use rsl_core::stage::debug::build_artifact;
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use rsl_core::{compile, script};

const SEARCH_HTML: &str = r#"<!doctype html><html><body>
<ul class="result">
  <li><a class="title" href="/book/1">斗破苍穹</a><span class="author">天蚕土豆</span></li>
  <li><a class="title" href="/book/2">凡人修仙传</a><span class="author">忘语</span></li>
</ul>
</body></html>"#;

const SOURCE_JSON: &str = r#"{
  "rsl": "2.0", "id": "debug.example.com", "name": "调试夹具站", "type": "novel",
  "baseUrl": "https://debug.example.com",
  "http": { "headers": { "User-Agent": "rsl-debug-test/1.0" }, "retry": { "times": 0, "backoff": "none" } },
  "stages": {
    "search": {
      "request": { "url": "var:line/search", "params": { "q": "var:kw", "page": "var:page" }, "response": "html" },
      "out": {
        "list": "css:ul.result > li",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
          "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
        },
        "emptyOk": true
      },
      "paginate": { "kind": "page", "pageVar": "page", "max": 2 }
    },
    "detail": {
      "request": { "url": "https://debug.example.com/book/1", "skip": true },
      "out": {
        "name": { "$": "const:纯常量阶段", "optional": true },
        "note": { "$": "const:skip 请求不发送", "optional": true },
        "tocUrl": { "$": "const:https://debug.example.com/toc", "optional": true }
      }
    },
    "toc": {
      "request": { "url": "var:book.tocUrl" },
      "out": {
        "list": "css:ul.chapters > li",
        "map": {
          "title": { "$": "css:a", "transform": [{ "trim": true }] },
          "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] }
        },
        "emptyOk": true
      }
    },
    "content": {
      "request": { "url": "var:chapter.url" },
      "out": {
        "content": { "$": "css:div#content", "optional": true }
      }
    }
  },
  "vars": { "line": { "value": "https://debug.example.com", "label": "线路", "options": ["https://debug.example.com"], "persist": true } }
}"#;

fn resp(body: &str) -> rsl_core::ports::HttpResponse {
    rsl_core::ports::HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
        body: body.as_bytes().to_vec(),
        final_url: String::new(),
        ms: 1,
    }
}

struct App {
    transport: FixtureTransport,
    cookies: MemoryCookieStore,
}

fn build_app(bodies: Vec<&str>) -> App {
    App {
        transport: FixtureTransport {
            responses: bodies.into_iter().map(resp).collect(),
            idx: std::sync::atomic::AtomicUsize::new(0),
        },
        cookies: MemoryCookieStore::default(),
    }
}

/// 运行并组装产物（与 CLI/serve 同一条组装路径）。
fn run_debug(
    app: &App,
    stage: &str,
    inputs: RunInputs,
) -> (serde_json::Value, Result<rsl_core::stage::StageResult, rsl_core::error::Diagnostic>) {
    let out = compile(SOURCE_JSON);
    let src = match out.source {
        Some(s) => s,
        None => {
            for d in out.errors() {
                eprintln!("{d}");
            }
            panic!("编译失败");
        }
    };
    script::setup_engine(&src);
    let mut cx = ExecCx::new(&src, &app.transport, &SystemClock);
    cx.cookie_store = Some(&app.cookies);
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, stage, &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let artifact = build_artifact(&src, "test.json", stage, &cx, &inputs, &r, 42);
    (artifact, r)
}

#[test]
fn artifact_has_all_four_faces() {
    let app = build_app(vec![SEARCH_HTML]);
    let (a, r) = run_debug(
        &app,
        "search",
        RunInputs { kw: Some("斗".into()), ..Default::default() },
    );
    assert!(r.is_ok());
    // 面 0：元信息
    assert_eq!(a["schema"], "rsl-debug/1");
    assert_eq!(a["stage"], "search");
    assert_eq!(a["ok"], true);
    assert_eq!(a["sourceId"], "debug.example.com");
    assert_eq!(a["inputs"]["kw"], "斗");
    // 面 1：请求侧（协议标头）
    assert_eq!(a["request"]["template"], "${var:line}/search", "原始地址=模板形态");
    assert_eq!(a["request"]["method"], "GET");
    assert_eq!(a["request"]["engine"], "http");
    assert_eq!(a["request"]["parse"], "html");
    assert_eq!(a["request"]["attempts"], 1);
    let url = a["request"]["url"].as_str().unwrap();
    assert!(url.starts_with("https://debug.example.com/search"), "请求地址：{url}");
    assert!(url.contains("q=%E6%96%97"), "params 已编码：{url}");
    let headers = a["request"]["headers"].as_array().unwrap();
    assert!(
        headers.iter().any(|h| h[0] == "User-Agent" && h[1] == "rsl-debug-test/1.0"),
        "请求头为实际发送值"
    );
    // 面 2：响应侧（原始数据）
    assert_eq!(a["response"]["status"], 200);
    assert_eq!(a["response"]["binary"], false);
    assert_eq!(a["response"]["truncated"], false);
    let preview = a["response"]["preview"].as_str().unwrap();
    assert!(preview.contains("斗破苍穹"), "预览含真实数据");
    assert_eq!(a["response"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(a["response"]["body_bytes"], SEARCH_HTML.len());
    // 面 3：解析结果
    let list = a["out"]["list"].as_array().unwrap();
    assert_eq!(list.len(), 4, "分页 max=2 × 每页 2 条（夹具重放）");
    assert_eq!(list[0]["map"]["name"], "斗破苍穹");
    // 诊断/trace
    assert!(a["trace"].as_array().unwrap().iter().any(|t| t["stage"] == "search"));
    assert!(a["partial"] == false);
}

#[test]
fn pagination_records_every_request() {
    let app = build_app(vec![SEARCH_HTML, SEARCH_HTML]);
    let (a, r) = run_debug(
        &app,
        "search",
        RunInputs { kw: Some("斗".into()), ..Default::default() },
    );
    assert!(r.is_ok());
    let reqs = a["requests"].as_array().unwrap();
    assert_eq!(reqs.len(), 2, "分页 2 页 → 2 条请求记录");
    for req in reqs {
        assert_eq!(req["request"]["method"], "GET");
        assert!(req["response"]["status"] == 200);
    }
    // page 参数递增可见
    let p1 = reqs[0]["request"]["url"].as_str().unwrap();
    let p2 = reqs[1]["request"]["url"].as_str().unwrap();
    assert!(p1.contains("page=1"), "p1: {p1}");
    assert!(p2.contains("page=2"), "p2: {p2}");
}

#[test]
fn skip_stage_request_null() {
    let app = build_app(vec![]);
    let (a, r) = run_debug(&app, "detail", RunInputs::default());
    assert!(r.is_ok(), "skip 阶段应成功: {:?}", r.err());
    assert!(a["request"].is_null(), "纯 const/skip 阶段 request=null");
    assert!(a["response"].is_null());
    assert!(a["requests"].as_array().unwrap().is_empty());
    assert_eq!(a["out"]["name"], "纯常量阶段");
}

#[test]
fn big_response_truncated() {
    // >256KB 响应：truncated=true 且预览 ≤ 预算
    let big = format!("<html><body><ul class=\"result\"><li><a class=\"title\" href=\"/b\">{}</a></li></ul></body></html>", "字".repeat(200_000));
    assert!(big.len() > 256 * 1024);
    let app = build_app(vec![&big]);
    let (a, r) = run_debug(
        &app,
        "search",
        RunInputs { kw: Some("x".into()), ..Default::default() },
    );
    assert!(r.is_ok());
    assert_eq!(a["response"]["truncated"], true);
    assert!(a["response"]["body_bytes"].as_u64().unwrap() as usize == big.len());
    let preview = a["response"]["preview"].as_str().unwrap();
    assert!(preview.len() <= 256 * 1024, "预览不超预算");
    assert_eq!(a["out"]["list"].as_array().unwrap().len(), 2, "解析不受预览截断影响");
}

#[test]
fn failing_stage_still_yields_artifact() {
    // 网络层失败：request 有记录、response=null、ok=false、error 诊断存在
    struct DeadTransport;
    impl rsl_core::ports::Transport for DeadTransport {
        fn send(&self, _req: &rsl_core::ports::HttpRequest) -> Result<rsl_core::ports::HttpResponse, rsl_core::error::Diagnostic> {
            Err(rsl_core::error::Diagnostic::new(
                rsl_core::error::Code::ENet,
                "$",
                "网络层失败（夹具）",
                "检查连接",
            )
            .retryable(true))
        }
    }
    let cookies = MemoryCookieStore::default();
    let out = compile(SOURCE_JSON);
    let src = match out.source {
        Some(s) => s,
        None => {
            for d in out.errors() {
                eprintln!("{d}");
            }
            panic!("编译失败");
        }
    };
    script::setup_engine(&src);
    let mut cx = ExecCx::new(&src, &DeadTransport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    let inputs = RunInputs { kw: Some("斗".into()), ..Default::default() };
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "search", &mut cx, &inputs, &mut reuse, &HostCaps::default());
    assert!(r.is_err(), "网络失败应 Err");
    let a = build_artifact(&src, "test.json", "search", &cx, &inputs, &r, 5);
    assert_eq!(a["ok"], false);
    assert!(a["out"].is_null());
    assert!(!a["request"].is_null(), "请求侧记录保留");
    assert!(a["response"].is_null(), "无响应");
    assert_eq!(a["requests"].as_array().unwrap().len(), 1);
    assert!(a["requests"][0]["response"].is_null());
    let err = &a["error"];
    assert_eq!(err["code"], "ENet", "error 诊断直通: {err}");
    assert_eq!(err["retryable"], true, "error 诊断直通: {err}");
}

#[test]
fn artifact_json_roundtrip() {
    let app = build_app(vec![SEARCH_HTML]);
    let (a, _) = run_debug(
        &app,
        "search",
        RunInputs { kw: Some("斗".into()), ..Default::default() },
    );
    let s = serde_json::to_string(&a).unwrap();
    let back: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(back["schema"], "rsl-debug/1");
}
