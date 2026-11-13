//! T6.5 WASM 端一致性：同一 fixture 经 RslEngine::run_stage 输出与 golden 哈希对比。
//! （wasm32 目标上运行；golden 由 CLI 端 tools/conformance.py golden 生成。）

#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const SOURCE: &str = include_str!("../../../tests/cases/novel-source.rsl.json");
const SEARCH_HTML: &str = include_str!("../../../fixtures/search.html");
const TOC_1200: &str = include_str!("../../../fixtures/toc_1200.html");
const ENUMERATE_JSON: &str = include_str!("../../../fixtures/enumerate.json");
const AGG_SOURCE: &str = include_str!("../../../examples/aggregate-merge.rsl.json");

fn b64(s: &str) -> String {
    // 无 base64 crate 依赖的极简编码（wasm 侧测试专用）
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let data = s.as_bytes();
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        out.push(T[(b[0] >> 2) as usize] as char);
        out.push(T[(((b[0] & 0x03) << 4) | (b[1] >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(T[(((b[1] & 0x0f) << 2) | (b[2] >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(T[(b[2] & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn transport_json(body: &str) -> String {
    format!(
        r#"{{"responses":[{{"url":"fixture://","status":200,"content_type":"text/html; charset=utf-8","body_base64":"{}"}}]}}"#,
        b64(body)
    )
}

#[wasm_bindgen_test]
fn search_matches_cli() {
    let mut eng = rsl_wasm::RslEngine::new(SOURCE, &transport_json(SEARCH_HTML)).unwrap();
    let out = eng.run_stage("search", r#"{"kw":"斗破","page":1}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["ok"].as_bool().unwrap(), "search ok");
    let list = v["out"]["list"].as_array().unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["map"]["name"], "斗破苍穹");
}

#[wasm_bindgen_test]
fn toc_1200_matches_cli() {
    let mut eng = rsl_wasm::RslEngine::new(SOURCE, &transport_json(TOC_1200)).unwrap();
    let out = eng
        .run_stage("toc", r#"{"book":{"tocUrl":"https://example.com/book/1/toc"}}"#)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let list = v["out"]["list"].as_array().unwrap();
    assert_eq!(list.len(), 1200);
}

#[wasm_bindgen_test]
fn enumerate_matches_cli() {
    let mut eng = rsl_wasm::RslEngine::new(AGG_SOURCE, &transport_json(ENUMERATE_JSON)).unwrap();
    let out = eng.run_stage("search", r#"{"kw":"页","page":1}"#).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let list = v["out"]["list"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["map"]["name"], "页 1 内容");
}
