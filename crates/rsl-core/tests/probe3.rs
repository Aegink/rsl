//! 探针 3：content 文档选择 + 文本节点聚合。
use rsl_core::selector::select_css;

#[test]
fn probe_content() {
    let html = r#"<!doctype html><html><body>
<h1 class="chapter-title">第一章 风起</h1>
<div id="content">第一段正文。&nbsp;缩进测试。<br/>第二段正文。<br/><span class="ad">广告位</span></div>
<a id="next-page" href="/c/1_2">下一页</a>
</body></html>"#;
    let d = rsl_core::exec::value::HtmlDoc::parse(html);
    let div = select_css(&d.html, None, "div#content").unwrap();
    println!("div#content = {}", div.len());
    let h1 = select_css(&d.html, None, "h1.chapter-title").unwrap();
    println!("h1.chapter-title = {}", h1.len());
    assert!(div.len() == 1 && h1.len() == 1);
    let t = rsl_core::selector::node_text(&d.html, div[0], false);
    println!("node_text = {:?}", t);
    let paras = rsl_core::exec::eval::split_paragraphs(&t, "br", vec![]);
    println!("paras = {:?}", paras);
}
