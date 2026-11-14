//! TASK-BOOK-3 F1：`Diagnostic.fix` 确定性修复建议。
//! 核心验收（V3-D8/R5）：**有效性测试——应用 fix 后编译通过**；假修比不修更糟。

use rsl_core::analyze::nearest_match;
use rsl_core::compile;
use rsl_core::error::{Code, Diagnostic};

/// 应用 fix：把源文档中首个 `find` 精确替换为 `replacement`（语义与 LSP code action 一致）。
fn apply_fix(text: &str, d: &Diagnostic) -> String {
    let fx = d.fix.as_ref().expect("fix 必须存在");
    match text.find(&fx.find) {
        Some(i) => format!(
            "{}{}{}",
            &text[..i],
            fx.replacement,
            &text[i + fx.find.len()..]
        ),
        None => panic!("fix.find `{}` 必须在源文档中出现（确定性）", fx.find),
    }
}

fn broken_selector_source() -> String {
    r#"{
  "rsl": "2.0", "id": "fix.example.com", "name": "修复站", "type": "novel",
  "baseUrl": "https://fix.example.com",
  "requires": { "features": [], "crypto": [] },
  "stages": {
    "search": {
      "request": { "url": "https://fix.example.com/search", "params": { "q": "var:kw" } },
      "out": {
        "list": "cs:ul.list li",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
          "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
        },
        "emptyOk": true
      }
    },
    "detail": {
      "request": { "url": "var:book.url" },
      "out": { "name": "css:h1", "tocUrl": { "$": "css:a.read", "attr": "href", "optional": true } }
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
}"#.to_string()
}

fn broken_ref_source() -> String {
    r#"{
  "rsl": "2.0", "id": "fix2.example.com", "name": "引用站", "type": "novel",
  "baseUrl": "https://fix2.example.com",
  "requires": { "features": [], "crypto": [] },
  "stages": {
    "search": {
      "request": { "url": "https://fix2.example.com/search", "params": { "q": "var:kw" } },
      "out": {
        "list": "css:ul.list li",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
          "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
        },
        "emptyOk": true
      }
    },
    "detail": {
      "request": { "url": "var:book.urll" },
      "out": { "name": "css:h1", "tocUrl": { "$": "css:a.read", "attr": "href", "optional": true } }
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
}"#.to_string()
}

/// E_SELECTOR：未知命名空间前缀 → 最近似命名空间 fix；应用后编译通过。
#[test]
fn f1_fix_e_selector_validity() {
    let text = broken_selector_source();
    let out = compile(&text);
    let errs = out.errors();
    let d = errs
        .iter()
        .find(|d| d.code == Code::ESelector)
        .expect("cs:li 必须产生 E_SELECTOR");
    let fx = d.fix.as_ref().expect("E_SELECTOR 前缀笔误必须带 fix");
    assert_eq!(fx.find, "cs:");
    assert_eq!(fx.replacement, "css:");
    assert!(fx.message.contains("css"));
    // 有效性：应用后该路径不再报错，整源编译通过
    let fixed = apply_fix(&text, d);
    let out2 = compile(&fixed);
    assert!(
        out2.ok(),
        "应用 fix 后必须编译通过，剩余诊断：{:?}",
        out2.errors().iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
    assert!(
        !out2
            .errors()
            .iter()
            .any(|d| d.code == Code::ESelector && d.path == d.path),
        "E_SELECTOR 必须消失"
    );
}

/// E_REF：var: 笔误 → 最近似可见路径 fix；应用后编译通过。
#[test]
fn f1_fix_e_ref_validity() {
    let text = broken_ref_source();
    let out = compile(&text);
    let errs = out.errors();
    let d = errs
        .iter()
        .find(|d| d.code == Code::ERef)
        .expect("var:book.urll 必须产生 E_REF");
    let fx = d.fix.as_ref().expect("最近似 id 存在时必须带 fix");
    assert_eq!(fx.find, "var:book.urll");
    assert_eq!(fx.replacement, "var:book.url");
    let fixed = apply_fix(&text, d);
    let out2 = compile(&fixed);
    assert!(
        out2.ok(),
        "应用 fix 后必须编译通过，剩余诊断：{:?}",
        out2.errors().iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
    assert!(!out2.errors().iter().any(|d| d.code == Code::ERef));
}

/// 无最近似候选时不产 fix（假修比不修更糟，V3-D8）。
#[test]
fn f1_no_fix_when_no_near_match() {
    // `zzz:` 与 18 个命名空间距离全超阈值 → 无 fix
    let text = broken_selector_source().replace("cs:ul.list li", "zzqq11:x");
    let out = compile(&text);
    let errs = out.errors();
    let d = errs
        .iter()
        .find(|d| d.code == Code::ESelector)
        .expect("未知前缀仍报 E_SELECTOR");
    assert!(d.fix.is_none(), "距离超阈必须不产 fix，got {:?}", d.fix);
}

/// 新引用（非笔误）不产 fix：var:unknown 完全无候选。
#[test]
fn f1_no_fix_for_genuinely_new_ref() {
    let text = broken_ref_source().replace("var:book.urll", "var:totally.new.thing");
    let out = compile(&text);
    let errs = out.errors();
    let d = errs
        .iter()
        .find(|d| d.code == Code::ERef)
        .expect("未知引用仍报 E_REF");
    assert!(d.fix.is_none(), "无候选时 fix 必须为 None");
}

/// 序列化兼容（D8 回归）：无 fix 字段的旧诊断 JSON 可反序列化；带 fix 往返一致。
#[test]
fn f1_serialization_backward_compatible() {
    // derived serde 枚举格式（Code::ERef → "ERef"，Severity::Error → "Error"）
    let old = r#"{"code":"ERef","severity":"Error","path":"stages.search","message":"m","hint":"h"}"#;
    let d: Diagnostic = serde_json::from_str(old).expect("旧格式必须可解析");
    assert!(d.fix.is_none());
    let with_fix = Diagnostic::new(Code::ERef, "p", "m", "h").fix("a", "b", "c");
    let s = serde_json::to_string(&with_fix).unwrap_or_default();
    assert!(s.contains("\"fix\""));
    let d2: Diagnostic = serde_json::from_str(&s).expect("新格式往返");
    let fx = d2.fix.expect("fix 必须往返");
    assert_eq!((fx.find.as_str(), fx.replacement.as_str()), ("a", "b"));
}

/// nearest_match 阈值行为：近距命中、远距拒绝、自身不参与。
#[test]
fn f1_nearest_match_thresholds() {
    let cands: Vec<String> = vec!["url".into(), "tocUrl".into(), "name".into()];
    assert_eq!(nearest_match("urll", &cands), Some("url".into()));
    // 自身相等不返回（调用方只在未命中时调用）
    assert_eq!(nearest_match("url", &cands), None);
    // 全部超阈 → None
    assert_eq!(nearest_match("zzzzzzzz", &cands), None);
}
