//! C3 验收（TASK-BOOK-2 §3.3 C3）：scraper 0.29 / selectors 新版 —— CSS4 选择器。
//! `:has()` / `:is()` / `:where()` 全部按 forgiving 处理（R§9.2 css level 4）。

use rsl_core::exec::value::HtmlDoc;
use rsl_core::selector::select_css;

const HTML: &str = r#"<!doctype html><html><body>
<div class="card" id="c1"><h3>含价格</h3><span class="price">12元</span></div>
<div class="card" id="c2"><h3>无价格</h3></div>
<ul>
  <li class="row"><a class="t" href="/1">甲</a></li>
  <li class="row special"><a class="t" href="/2">乙</a></li>
  <li class="row">无链接</li>
</ul>
</body></html>"#;

#[test]
fn css4_has_selector() {
    let d = HtmlDoc::parse(HTML);
    // :has() —— 含 .price 子元素的 card
    let r = select_css(&d.html, None, "div.card:has(> span.price)").unwrap();
    assert_eq!(r.len(), 1, ":has() 应命中 1 张含价格卡片");
    assert_eq!(
        rsl_core::selector::node_attr(&d.html, r[0], "id"),
        Some("c1".into()),
        "命中 id=c1"
    );
    // :has() 嵌套结构选择
    let r = select_css(&d.html, None, "div.card:has(h3)").unwrap();
    assert_eq!(r.len(), 2, ":has(h3) 命中两张卡片");
}

#[test]
fn css4_is_where() {
    let d = HtmlDoc::parse(HTML);
    let r = select_css(&d.html, None, "li:is(.special, .other)").unwrap();
    assert_eq!(r.len(), 1, ":is() 命中 special");
    let r = select_css(&d.html, None, ":where(ul) > li.row > a.t").unwrap();
    assert_eq!(r.len(), 2, ":where() 零特异性参与结构匹配");
}

#[test]
fn css4_forgiving_list() {
    // forgiving-selector-list：不认识的伪类只忽略该条，其余仍生效
    let d = HtmlDoc::parse(HTML);
    let r = select_css(&d.html, None, "li:is(.special, ::unknown-pseudo)").unwrap();
    assert_eq!(r.len(), 1, "forgiving 列表中非法项被忽略");
}

#[test]
fn has_in_where_pipeline() {
    // 编译 + 运行全链路：where.has 用 :has() 语法过滤
    use rsl_core::exec::ctx::ExecCx;
    use rsl_core::ports::{Clock, FixtureTransport, HttpResponse, MemoryCookieStore, SystemClock};
    use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
    use rsl_core::{compile, script};
    let text = r#"{
  "rsl": "2.0", "id": "css4.example.com", "name": "CSS4站", "type": "novel",
  "baseUrl": "https://css4.example.com",
  "requires": { "features": [], "crypto": [], "selectorLevel": { "css": "4" } },
  "stages": {
    "search": {
      "request": { "url": "https://css4.example.com/s", "params": { "q": "var:kw" } },
      "out": { "list": "css:ul > li:has(> a.t)", "map": {
        "name": { "$": "css:a.t", "transform": [{ "trim": true }] },
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
      "out": { "content": { "$": "css:div#content", "paragraphs": { "split": "br" } } }
    }
  }
}"#;
    let out = compile(text);
    assert!(out.ok(), "CSS4 源应编译通过: {:?}", out.errors().iter().map(|d| format!("{d}")).collect::<Vec<_>>());
    let src = out.source.unwrap();
    script::setup_engine(&src);
    let resp = |body: &str| HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
        body: body.as_bytes().to_vec(),
        final_url: String::new(),
        ms: 1,
    };
    let transport = FixtureTransport {
        responses: vec![resp(HTML)],
        idx: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "search", &mut cx, &RunInputs { kw: Some("x".into()), ..Default::default() }, &mut reuse, &HostCaps::default());
    let s = r.expect("阶段应成功");
    let list = s.out.get("list").and_then(|v| v.as_array()).expect("list");
    assert_eq!(list.len(), 2, ":has(> a.t) 运行期过滤 → 2 条（无链接行被滤）");
    assert_eq!(list[0]["map"]["name"], "甲");
    assert_eq!(list[1]["map"]["name"], "乙");
}
