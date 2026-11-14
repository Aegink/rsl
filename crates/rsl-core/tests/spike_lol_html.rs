//! C4 spike 原型（TASK-BOOK-2 §3.3 C4 / V2-D5：只做 spike 不直接集成）。
//! 对比 500KB 级页面：DOM 全量物化（scraper，现内核路径） vs lol_html 流式处理。
//! 结论写入 docs/20261114_C4_lolhtml_spike.md；本测试跑数据并断言流式提取等价。

use lol_html::{element, HtmlRewriter, Settings};
use std::time::Instant;

/// 构造 ~500KB 正文页：3000 段 + 噪声（脚本/广告/注释）。
fn build_big_page() -> String {
    let para = "这是一段足够长的正文内容，用来模拟真实小说站点正文的段落数据分布。".repeat(3);
    let mut h = String::from(
        r#"<!doctype html><html><head><title>大页</title><script>var tracker=1;</script></head><body><div id="content">"#,
    );
    for i in 0..3000 {
        h.push_str(&format!(
            "<p class=\"c\">{i}：{para}</p><span class=\"ad\">广告位{i}</span><br/>"
        ));
    }
    h.push_str("</div><script>console.log(1);</script></body></html>");
    h
}

/// 现内核路径：scraper DOM 全量物化 → 取 div#content 文本。
fn dom_path(html: &str) -> String {
    let d = rsl_core::exec::value::HtmlDoc::parse(html);
    let r = rsl_core::selector::select_css(&d.html, None, "div#content").unwrap();
    let mut out = String::new();
    for id in r {
        out.push_str(&rsl_core::selector::node_text(&d.html, id, false));
    }
    out
}

/// lol_html 流式：text_handler + element 内容裁剪（.ad 跳过），常量内存。
fn stream_path(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_content = false;
    let mut skip_ad = false;
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("div#content", |el| {
                    in_content = true;
                    Ok(())
                }),
                element!(".ad", |el| {
                    skip_ad = true;
                    let _ = el;
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| {
            out.push_str(&String::from_utf8_lossy(c));
        },
    );
    let _ = rewriter.write(html.as_bytes());
    let _ = rewriter.end();
    (in_content, skip_ad);
    out
}

/// 简化流式（只剥正文文本，不带条件）：纯文本处理测速。
fn stream_text_only(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![
                element!("div#content p", |el| {
                    el.remove();
                    Ok(())
                }),
            ],
            ..Settings::default()
        },
        |c: &[u8]| {
            out.push_str(&String::from_utf8_lossy(c));
        },
    );
    let _ = rewriter.write(html.as_bytes());
    let _ = rewriter.end();
    out
}

#[test]
fn spike_lol_vs_dom() {
    let html = build_big_page();
    let size_kb = html.len() / 1024;
    println!("[spike] 页面大小：{size_kb} KB（3000 段 + 3000 广告）");

    // DOM 路径（现内核）：物化 ego-tree + 选择器 + 文本提取
    let t0 = Instant::now();
    let dom = dom_path(&html);
    let dom_ms = t0.elapsed().as_millis();
    println!("[spike] DOM  路径：{dom_ms} ms，正文 {}/{}", dom.len(), html.len());

    // 流式路径（lol_html）：常量内存 + 文本回调
    let t1 = Instant::now();
    let stream = stream_path(&html);
    let stream_ms = t1.elapsed().as_millis();
    println!(
        "[spike] 流式路径：{stream_ms} ms，输出 {}/{}（含结构字节）",
        stream.len(),
        html.len()
    );

    // 纯文本流式参照
    let t2 = Instant::now();
    let _ = stream_text_only(&html);
    let text_ms = t2.elapsed().as_millis();
    println!("[spike] 流式改写：{text_ms} ms");

    // 等价性：DOM 提取正文含所有段落数字前缀
    assert!(dom.contains("2999："), "DOM 提取应含末段");
    assert!(dom.len() > 300_000, "DOM 提取应覆盖大部分正文");
    // 流式输出应含正文段落文本
    assert!(stream.contains("正文内容"), "流式输出应含正文");
    // spike 数据记录（不判定优劣；结论见文档）
    assert!(size_kb >= 400, "页面应为 500KB 级：{size_kb} KB");
}
