//! C2 验收（TASK-BOOK-2 §3.3 C2 / RFC 9535）：手写子集 + jsonpath-rust 兼容层。

use rsl_core::selector::jsonpath;
use serde_json::{json, Value};

fn got(v: &Value, path: &str) -> Vec<Value> {
    let segs = jsonpath::parse(path).unwrap_or_else(|| panic!("解析失败: {path}"));
    jsonpath::eval(v, &segs)
}

#[test]
fn subset_unchanged() {
    // 手写子集语义保持（D8 回归：七猫/测试源全绿）
    let doc = json!({"data": {"book": {"title": "书"}}, "arr": [1, 2, 3]});
    assert_eq!(got(&doc, "$.data.book.title"), vec![json!("书")]);
    assert_eq!(got(&doc, "$.arr[1]"), vec![json!(2)]);
    assert_eq!(got(&doc, "$.arr[-1]"), vec![json!(3)]);
    assert_eq!(got(&doc, "$.arr[*]"), vec![json!(1), json!(2), json!(3)]);
    assert_eq!(got(&doc, "$.arr[0:2]"), vec![json!(1), json!(2)]);
    assert_eq!(got(&doc, "$..title"), vec![json!("书")]);
}

#[test]
fn rfc9535_shorthand_filter() {
    // 任务书 D6 验收：[?@.vip==false] 简写可用
    let doc = json!([
        {"name": "a", "vip": false},
        {"name": "b", "vip": true},
        {"name": "c", "vip": false}
    ]);
    let r = got(&doc, "$[?@.vip==false].name");
    assert_eq!(r, vec![json!("a"), json!("c")]);
    // 空格变体
    let r = got(&doc, "$[? @.vip == false ]");
    assert!(r.len() >= 2, "简写宽松空格: {r:?}");
}

#[test]
fn rfc9535_numeric_compare() {
    let doc = json!([
        {"n": 1}, {"n": 5}, {"n": 10}
    ]);
    let r = got(&doc, "$[?@.n > 3]");
    assert_eq!(r, vec![json!({"n": 5}), json!({"n": 10})]);
    let r = got(&doc, "$[?@.n <= 1]");
    assert_eq!(r, vec![json!({"n": 1})]);
}

#[test]
fn rfc9535_nested_filter_key() {
    // 手写子集只支持单键 filter；嵌套路径 a.b 交 RFC 引擎（语义修正）
    let doc = json!([
        {"meta": {"vip": false}, "id": 1},
        {"meta": {"vip": true}, "id": 2}
    ]);
    let r = got(&doc, "$[?@.meta.vip==false].id");
    assert_eq!(r, vec![json!(1)]);
    let r = got(&doc, "$[?(@.meta.vip==false)].id");
    assert_eq!(r, vec![json!(1)]);
}

#[test]
fn rfc9535_union_and_functions() {
    let doc = json!({"arr": [10, 20, 30, 40]});
    // 多选择器 union
    let r = got(&doc, "$.arr[1,3]");
    assert_eq!(r, vec![json!(20), json!(40)]);
    // 深层 miss → 空
    let r = got(&doc, "$.arr[?@.x].y");
    assert!(r.is_empty());
}

#[test]
fn bad_paths_still_none() {
    // 两个引擎都不认的路径 → None（编译期 E_SELECTOR 不变，V2-D3）
    assert!(jsonpath::parse("[unclosed").is_none());
    assert!(jsonpath::parse(".bad,,name").is_none());
}

#[test]
fn is_list_path_covers_rfc_filter() {
    assert!(jsonpath::is_list_path("$[?@.x==1]"));
    assert!(jsonpath::is_list_path("$.a[*]"));
    assert!(!jsonpath::is_list_path("$.a.b"));
}
