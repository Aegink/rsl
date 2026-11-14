//! TASK-BOOK-3 C2（V3-D6）：webJs 声明制受控桥。
//! 字符串形态（现状）零破坏；对象形态 {fn, ctx:true} → WebViewRequest.web_js_ctx 标志。

use rsl_core::ports::{WebViewPort, WebViewRequest, WebViewResult};
use rsl_core::{compile, error::Code};
use std::cell::Cell;
use std::sync::Mutex;

/// 记录型 Fake WebViewPort：捕获 web_js 与 ctx 标志，返回固定 body。
struct RecordingPort {
    seen: Mutex<Vec<(Option<String>, bool)>>,
    body: String,
}

impl RecordingPort {
    fn new(body: &str) -> Self {
        Self { seen: Mutex::new(vec![]), body: body.to_string() }
    }
    fn last(&self) -> (Option<String>, bool) {
        self.seen.lock().unwrap_or_else(|e| e.into_inner()).last().cloned().unwrap()
    }
}

impl WebViewPort for RecordingPort {
    fn render(&self, req: &WebViewRequest) -> Result<WebViewResult, rsl_core::error::Diagnostic> {
        self.seen
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((req.web_js.clone(), req.web_js_ctx));
        Ok(WebViewResult {
            body: self.body.clone(),
            sniffed: vec![],
        })
    }
}

fn source_with_webjs(webjs: &str) -> String {
    format!(
        r#"{{
  "rsl": "2.0", "id": "webjs.example.com", "name": "桥站", "type": "novel",
  "baseUrl": "https://webjs.example.com",
  "requires": {{ "features": ["webview"], "crypto": [] }},
  "stages": {{
    "search": {{
      "request": {{
        "url": "https://webjs.example.com/search",
        "engine": "webview",
        "webJs": {webjs}
      }},
      "out": {{
        "list": "css:ul li",
        "map": {{ "name": {{ "$": "css:a" }}, "url": {{ "$": "css:a", "attr": "href", "transform": [{{ "absolute": true }}] }} }},
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
        "title": {{ "$": "css:a" }},
        "url": {{ "$": "css:a", "attr": "href", "transform": [{{ "absolute": true }}] }}
      }} }}
    }},
    "content": {{
      "request": {{ "url": "var:chapter.url" }},
      "out": {{ "content": {{ "$": "css:div#content", "paragraphs": {{ "split": "br" }} }} }}
    }}
  }}
}}"#
    )
}

/// 字符串形态兼容：ctx 标志恒 false（V3-D6 向后兼容）。
#[test]
fn c2_string_form_ctx_false() {
    let src = source_with_webjs(r#""document.body.innerText""#);
    let out = compile(&src);
    let ir = out.source.expect("字符串形态必须编译");
    let req = ir.stages.get("search").unwrap().request.as_ref().unwrap();
    let wj = req.web_js.as_ref().expect("webJs 必须进 IR");
    assert!(wj.code.contains("innerText"));
    assert!(!wj.ctx, "字符串形态 ctx 必须为 false");
}

/// 对象形态 {fn, ctx:true}：ctx 标志传播到 WebViewRequest。
#[test]
fn c2_object_form_ctx_true_propagates() {
    let src = source_with_webjs(r#"{ "fn": "document.querySelector('.token').textContent", "ctx": true }"#);
    let out = compile(&src);
    let ir = out.source.expect("对象形态必须编译");
    let wj = ir.stages.get("search").unwrap().request.as_ref().unwrap().web_js.clone().expect("IR 有 webJs");
    assert!(wj.ctx, "ctx:true 必须进 IR");

    let port = RecordingPort::new("<html>token-abc</html>");
    let r = rsl_core::webview::render(
        &port,
        "https://webjs.example.com/search",
        &[],
        Some(&wj),
        None,
        10_000,
    );
    assert!(r.is_ok());
    let (js, ctx) = port.last();
    assert!(js.as_deref().unwrap_or("").contains("querySelector"));
    assert!(ctx, "web_js_ctx 标志必须传播（宿主据此才暴露 __ctx 桥）");
}

/// 对象形态 ctx 缺省 = false。
#[test]
fn c2_object_form_ctx_default_false() {
    let src = source_with_webjs(r#"{ "fn": "document.title" }"#);
    let out = compile(&src);
    let ir = out.source.expect("无 ctx 字段必须编译（缺省 false）");
    let wj = ir.stages.get("search").unwrap().request.as_ref().unwrap().web_js.clone().unwrap();
    assert!(!wj.ctx);
}

/// 反例：对象形态缺 fn → E_SCHEMA。
#[test]
fn c2_object_form_missing_fn_e_schema() {
    let src = source_with_webjs(r#"{ "ctx": true }"#);
    let out = compile(&src);
    assert!(out
        .errors()
        .iter()
        .any(|d| d.code == Code::ESchema && d.path.ends_with("webJs.fn")));
}

/// 反例：非法类型（数字）→ E_SCHEMA。
#[test]
fn c2_bad_type_e_schema() {
    let src = source_with_webjs("123");
    let out = compile(&src);
    assert!(out
        .errors()
        .iter()
        .any(|d| d.code == Code::ESchema && d.path.ends_with("webJs")));
}

/// 桥冒烟：ctx=false 时宿主收到 web_js_ctx=false（协议约束：严禁暴露 __ctx）。
#[test]
fn c2_string_form_request_flag_false() {
    let src = source_with_webjs(r#""document.title""#);
    let out = compile(&src);
    let ir = out.source.unwrap();
    let wj = ir.stages.get("search").unwrap().request.as_ref().unwrap().web_js.clone().unwrap();
    let port = RecordingPort::new("t");
    rsl_core::webview::render(&port, "https://x.example.com", &[], Some(&wj), None, 1_000).ok();
    let (_, ctx) = port.last();
    assert!(!ctx, "自由 JS 形态不得携带桥标志");
}
