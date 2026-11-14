//! edge-cases YAML 用例探针（定位 where/parse/content 断言失败根因，调试用）。
use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{Clock, FixtureTransport, HttpResponse, MemoryCookieStore, SystemClock};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use rsl_core::{compile, script};

const SEARCH_HTML: &str = r#"<!doctype html><html><body>
<ul class="list">
  <li>书名甲<span class="price">12元</span><a class="t" href="/b/1">链接</a></li>
  <li class="hidden-row">隐藏行（无 a.t，where 过滤掉）</li>
  <li>书名乙<span class="price">8元</span><a class="t" href="/b/2">链接</a></li>
  <li>书名丙<span class="price">15元</span><a class="t" href="/b/3">链接</a></li>
</ul>
</body></html>"#;

fn resp(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
        body: body.as_bytes().to_vec(),
        final_url: String::new(),
        ms: 1,
    }
}

#[test]
fn probe_edge_search() {
    let text = r#"{
  "rsl": "2.0", "id": "edge.example.com", "name": "边缘站", "type": "novel",
  "baseUrl": "https://edge.example.com",
  "requires": { "features": [], "crypto": [] },
  "stages": {
    "search": {
      "request": { "url": "https://edge.example.com/s", "params": { "q": "var:kw" } },
      "out": {
        "list": "css:ul.list > li",
        "where": { "has": "css:a.t" },
        "map": {
          "name": { "$": "text", "transform": [{ "trim": true }] },
          "url": { "$": "css:a.t", "attr": "href", "transform": [{ "absolute": true }] },
          "price": { "$": "css:.price", "optional": true, "default": "" }
        },
        "emptyOk": true
      }
    },
    "detail": {
      "request": { "url": "var:book.url" },
      "out": { "name": "css:h1", "tocUrl": { "$": "css:a.read", "attr": "href", "transform": [{ "absolute": true }], "optional": true } }
    },
    "toc": {
      "request": { "url": "var:book.tocUrl" },
      "out": { "list": "css:#list li", "map": {
        "title": { "$": "css:a", "transform": [{ "trim": true }] },
        "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] }
      } }
    },
    "content": {
      "request": { "url": "var:chapter.url" },
      "out": { "content": { "$": "css:div#content", "paragraphs": { "split": "br" } } }
    }
  }
}"#;
    let out = compile(text);
    if !out.ok() {
        for d in out.errors() {
            eprintln!("CERR: {d}");
        }
        panic!("编译失败");
    }
    let src = out.source.unwrap();
    script::setup_engine(&src);
    let transport = FixtureTransport {
        responses: vec![resp(SEARCH_HTML)],
        idx: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "search", &mut cx, &RunInputs { kw: Some("书".into()), page: Some(1), ..Default::default() }, &mut reuse, &HostCaps::default());
    match r {
        Ok(s) => println!("SEARCH OK: {}", serde_json::to_string_pretty(&s.out).unwrap_or_default()),
        Err(d) => println!("SEARCH ERR: {d}"),
    }
    for w in cx.warnings.borrow().iter() {
        println!("WARN: {w}");
    }
}

const CONTENT_HTML: &str = r#"<!doctype html><html><body><div id="content"><p>第一段足够长的正文内容。</p><p>第二段同样足够长的正文内容。</p><p>第三段足够长的正文内容。</p></div></body></html>"#;

#[test]
fn probe_edge_content() {
    let text = r#"{
  "rsl": "2.0", "id": "edge.example.com", "name": "边缘站", "type": "novel",
  "baseUrl": "https://edge.example.com",
  "requires": { "features": [], "crypto": [] },
  "stages": {
    "search": {
      "request": { "url": "https://edge.example.com/s", "params": { "q": "var:kw" } },
      "out": { "list": "css:ul.list > li", "map": {
        "name": { "$": "text", "transform": [{ "trim": true }] },
        "url": { "$": "css:a.t", "attr": "href", "transform": [{ "absolute": true }] }
      }, "emptyOk": true }
    },
    "detail": {
      "request": { "url": "var:book.url" },
      "out": { "name": "css:h1", "tocUrl": { "$": "css:a.read", "attr": "href", "transform": [{ "absolute": true }], "optional": true } }
    },
    "toc": {
      "request": { "url": "var:book.tocUrl" },
      "out": { "list": "css:#list li", "map": {
        "title": { "$": "css:a", "transform": [{ "trim": true }] },
        "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] }
      } }
    },
    "content": {
      "request": { "url": "var:chapter.url" },
      "out": { "content": { "$": "css:div#content", "paragraphs": { "split": "auto", "minLength": 2 }, "transform": [{ "trim": true }] } }
    }
  }
}"#;
    let out = compile(text);
    let src = out.source.expect("编译通过");
    script::setup_engine(&src);
    let transport = FixtureTransport {
        responses: vec![resp(CONTENT_HTML)],
        idx: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    let mut chapter = std::collections::BTreeMap::new();
    chapter.insert("url".to_string(), serde_json::json!("https://edge.example.com/c/1"));
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "content", &mut cx, &RunInputs { chapter, ..Default::default() }, &mut reuse, &HostCaps::default());
    match r {
        Ok(s) => println!("CONTENT OUT: {}", serde_json::to_string(&s.out).unwrap_or_default()),
        Err(d) => println!("CONTENT ERR: {d}"),
    }
}
