//! 选择器探针 2（exists + strip + per-item 调试）。
use rsl_core::selector::select_under;

#[test]
fn probe_exists() {
    let html = r#"<html><body><div id="chapter-list"><ul>
    <li><a href="/c/1">第一章 风起</a></li>
    <li class="volume"><span class="vol">第一卷</span></li>
    </ul></div></body></html>"#;
    let d = rsl_core::exec::value::HtmlDoc::parse(html);
    let lis = rsl_core::selector::select_css(&d.html, None, "div#chapter-list > ul > li").unwrap();
    println!("lis = {}", lis.len());
    for (i, li) in lis.iter().enumerate() {
        let r = select_under(&d.html, *li, "li.volume").unwrap();
        println!("li[{i}] li.volume hits = {}", r.len());
    }
    // .volume 作用于 li
    for (i, li) in lis.iter().enumerate() {
        let r = select_under(&d.html, *li, ".volume").unwrap();
        println!("li[{i}] .volume hits = {}", r.len());
    }
    assert!(lis.len() == 2);
}
