//! 选择器探针（调试用）。
use rsl_core::selector::select_css;

#[test]
fn probe_css() {
    let html = r#"<!doctype html><html><body>
<div id="chapter-list">
  <ul>
    <li><a href="/c/1">第一章 风起</a></li>
    <li><a href="/c/2">第二章 云涌</a></li>
  </ul>
</div>
</body></html>"#;
    let d = rsl_core::exec::value::HtmlDoc::parse(html);
    let r1 = select_css(&d.html, None, "div#chapter-list");
    println!("div#chapter-list -> {:?}", r1.as_ref().map(|v| v.len()));
    let r2 = select_css(&d.html, None, "div#chapter-list > ul > li");
    println!("li -> {:?}", r2.as_ref().map(|v| v.len()));
    let r3 = select_css(&d.html, None, "li");
    println!("bare li -> {:?}", r3.as_ref().map(|v| v.len()));
    assert!(r2.unwrap().len() == 2, "应命中 2 个 li");
}
