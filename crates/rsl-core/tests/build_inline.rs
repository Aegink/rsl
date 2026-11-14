//! TASK-BOOK-3 E1/E2：`rsl build --inline-jslib` 构建期受控远程。
//! V3-D7：拉取只发生在构建期；本测试用注入闭包证明内核自身零网络。

use rsl_core::build::{extract_fn_names, inline_jslib};
use std::collections::HashMap;

const LIB_JS: &str = "function jlibDecode(s) {\n  return s.split('').reverse().join('');\n}\n\nfunction jlibTitle(t) {\n  return jlibDecode(t).trim();\n}\n";

fn source_with_jslib(url: &str, sha: Option<&str>) -> String {
    let sha_line = match sha {
        Some(s) => format!(", \"sha256\": \"{}\"", s),
        None => String::new(),
    };
    // 内联后含 lib.pure → js 能力 → requires.features 声明（能力按需付费公理）
    format!(
        r#"{{
  "rsl": "2.0", "id": "inline.example.com", "name": "内联站", "type": "novel",
  "baseUrl": "https://inline.example.com",
  "requires": {{ "features": ["js"], "crypto": [] }},
  "x-jslib": [{{ "url": "{url}"{sha_line} }}],
  "stages": {{
    "search": {{
      "request": {{ "url": "https://inline.example.com/search", "params": {{ "q": "var:kw" }} }},
      "out": {{
        "list": "css:ul.list li",
        "map": {{
          "name": {{ "$": "css:a.title", "transform": [{{ "trim": true }}] }},
          "url": {{ "$": "css:a.title", "attr": "href", "transform": [{{ "absolute": true }}] }}
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
}}
"#
    )
}

/// 内存 fetch：file://maps://x/lib.js → LIB_JS；其余 Err（证明内核不隐式联网）。
fn mem_fetch() -> impl Fn(&str) -> Result<Vec<u8>, String> {
    let mut m: HashMap<String, Vec<u8>> = HashMap::new();
    m.insert("file://libs/lib.js".into(), LIB_JS.as_bytes().to_vec());
    move |url: &str| match m.get(url) {
        Some(v) => Ok(v.clone()),
        None => Err(format!("测试闭包不认识 {url}（内核不得隐式联网）")),
    }
}

fn sha_hex(s: &str) -> String {
    rsl_core::crypto::sha256_hex(s.as_bytes())
}

/// E1 主流程：file:// fixture → 内联进 lib.pure；x-jslib 移除；manifest 记 url+sha256。
#[test]
fn e1_inline_file_fixture() {
    let src = source_with_jslib("file://libs/lib.js", None);
    let out = inline_jslib(&src, &mem_fetch(), false).expect("内联必须成功");
    assert_eq!(out.entries.len(), 1);
    assert_eq!(out.entries[0].url, "file://libs/lib.js");
    assert_eq!(out.entries[0].sha256, sha_hex(LIB_JS));
    // 运行时零远程拉取断言：产物无 x-jslib、无 url 引用
    assert!(!out.text.contains("x-jslib"), "产物不得残留 x-jslib 声明");
    assert!(!out.text.contains("file://libs/lib.js"), "产物不得残留 url 引用");
    // lib.pure 追加成功
    assert!(out.text.contains("function jlibDecode("), "lib.pure 必须含内联函数");
    // 内联产物可编译
    let c = rsl_core::analyze::compile_str(&out.text);
    assert!(c.source.is_some(), "内联产物必须可编译：{:?}", c.errors().iter().map(|d| d.code.as_str()).collect::<Vec<_>>());
}

/// E1：无 x-jslib 的源字节不变（回归）。
#[test]
fn e1_no_decl_byte_unchanged() {
    let src = source_with_jslib("file://libs/lib.js", None)
        .replace("  \"x-jslib\": [{ \"url\": \"file://libs/lib.js\" }],\n", "");
    let out = inline_jslib(&src, &mem_fetch(), false).expect("必须成功");
    assert!(out.entries.is_empty());
    assert_eq!(out.text, src, "无 x-jslib 源必须原样返回");
}

/// E2：sha256 声明匹配 → 通过；不符 → 默认拒绝（E_VERIFY）；--force → 通过 + W_DEGRADED 警告。
#[test]
fn e2_tamper_detection() {
    let good = sha_hex(LIB_JS);
    let src_ok = source_with_jslib("file://libs/lib.js", Some(&good));
    let out = inline_jslib(&src_ok, &mem_fetch(), false).expect("sha 匹配必须通过");
    assert!(out.warnings.is_empty());

    let bad_sha = "0".repeat(64);
    let src_bad = source_with_jslib("file://libs/lib.js", Some(&bad_sha));
    let err = inline_jslib(&src_bad, &mem_fetch(), false).expect_err("sha 不符必须拒绝");
    assert_eq!(err.code, rsl_core::error::Code::EVerify);

    let out2 = inline_jslib(&src_bad, &mem_fetch(), true).expect("--force 必须放行");
    assert_eq!(out2.warnings.len(), 1);
    assert_eq!(out2.warnings[0].code, rsl_core::error::Code::WDegraded);
}

/// E2 确定性：同输入重复内联字节一致（R§14）。
#[test]
fn e2_deterministic() {
    let src = source_with_jslib("file://libs/lib.js", None);
    let a = inline_jslib(&src, &mem_fetch(), false).expect("a");
    let b = inline_jslib(&src, &mem_fetch(), false).expect("b");
    assert_eq!(a.text, b.text, "同输入必须字节一致");
    assert_eq!(a.entries.len(), b.entries.len());
    assert_eq!(a.entries[0].sha256, b.entries[0].sha256);
}

/// E1：重名函数 → E_SCRIPT 拒绝。
#[test]
fn e1_name_collision_rejected() {
    let src = source_with_jslib("file://libs/lib.js", None)
        .replace("  \"requires\"", "  \"lib\": { \"pure\": \"function jlibDecode(x) { return x; }\" },\n  \"requires\"");
    let err = inline_jslib(&src, &mem_fetch(), false).expect_err("重名必须拒绝");
    assert_eq!(err.code, rsl_core::error::Code::EScript);
    assert!(err.message.contains("jlibDecode"));
}

/// E1：拉取失败 → E_NET。
#[test]
fn e1_fetch_failure_e_net() {
    let src = source_with_jslib("https://unreachable.invalid/lib.js", None);
    let err = inline_jslib(&src, &mem_fetch(), false).expect_err("不可达必须失败");
    assert_eq!(err.code, rsl_core::error::Code::ENet);
    assert!(err.retryable == Some(true));
}

/// extract_fn_names：具名函数提取、匿名跳过。
#[test]
fn e1_extract_fn_names() {
    let js = "function a(x){return x} function  b (y){} function(z){return z}\nlet c = function named(w){};";
    let names = extract_fn_names(js);
    assert!(names.contains(&"a".into()), "got {names:?}");
    assert!(names.contains(&"b".into()), "got {names:?}");
    assert!(!names.iter().any(|n| n.is_empty()), "匿名函数不产空名");
}
