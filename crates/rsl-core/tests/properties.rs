//! A3 属性测试（TASK-BOOK-2 §3.1 A3 / §5.2 P1–P8）：
//! 三条铁律 + 模板/expr/jsonpath/xpath/regex/选择器 永不 panic + bincode 往返。

use proptest::prelude::*;
use rsl_core::error::{Code, Severity};
use rsl_core::exec::eval::compile_regex;
use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{Clock, FixtureTransport, HttpResponse, MemoryCookieStore, SystemClock};
use rsl_core::selector::{jsonpath, xpath};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use rsl_core::{compile, expr, sugar};
use std::rc::Rc;

// ───────────────────────── 随机串策略 ─────────────────────────

/// 混合 token 的"语法相邻"随机串：比纯字节串更能触达解析器深处。
fn nasty_string() -> impl Strategy<Value = String> {
    let token = prop::sample::select(vec![
        "css:", "re:", "json:$.", "xpath://", "var:", "let:", "expr:", "lib:", "${", "}",
        "(", ")", "[", "]", "{", "}", "@", ".", "::", "a.title", "div#x", "\\", "\"", "'",
        "|", "?", "*", "==", "false", "1", "\n", "%", "trim", "regex", "attr", "href",
        "e", "0", " ", "-",
    ]);
    prop::collection::vec(token, 0..40).prop_map(|v| v.concat())
}

/// 随机正则片段。
fn nasty_regex() -> impl Strategy<Value = String> {
    let token = prop::sample::select(vec![
        "a", "b", "\\d", "\\w", "(", ")", "[", "]", "{", "}", "*", "+", "?", "|", "^", "$",
        "\\1", "\\2", "(?<n>x)", "(?P<n>y)", "..", "x{2,1}", "[z-a]", "\\", "(?i)", "\\p{L}",
    ]);
    prop::collection::vec(token, 0..14).prop_map(|v| v.concat())
}

// ───────────────────────── mini 源构造 ─────────────────────────

const SEARCH_HTML: &str = r#"<!doctype html><html><body>
<ul class="result">
  <li><a class="title" href="/book/1">斗破苍穹</a></li>
  <li><a class="title" href="/book/2">凡人修仙传</a></li>
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

fn mini_source_str(list_sel: &str, extra_fields: &[String]) -> String {
    let mut s = String::from(
        r#"{
  "rsl": "2.0", "id": "prop.example.com", "name": "属性站", "type": "novel",
  "baseUrl": "https://prop.example.com",
  "requires": { "features": [], "crypto": [] },
  "stages": {
    "search": {
      "request": { "url": "https://prop.example.com/search", "params": { "q": "var:kw" } },
      "out": {
        "list": "#,
    );
    s.push_str(&serde_json_string(list_sel));
    s.push_str(
        r#",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
          "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }"#,
    );
    for (i, sel) in extra_fields.iter().enumerate() {
        s.push_str(&format!(
            ",\n          \"x{}\": {{ \"$\": {}, \"optional\": true, \"default\": \"\" }}",
            i,
            serde_json_string(sel)
        ));
    }
    s.push_str(
        r#"
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
}"#,
    );
    s
}

fn serde_json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// 编译 + 运行 search（若编译通过）。返回 (编译 ok, 运行结果文本摘要)。
fn run_search_strict(list_sel: &str, extra_fields: &[String], strict: bool) -> (bool, String) {
    let text = mini_source_str(list_sel, extra_fields);
    let out = compile(&text);
    if !out.ok() {
        let codes: Vec<String> = out
            .errors()
            .iter()
            .map(|d| d.code.as_str().to_string())
            .collect();
        assert!(
            codes.iter().all(|c| c.starts_with("E_")),
            "失败必须全带 E_ 码: {codes:?}"
        );
        return (false, codes.join(","));
    }
    let src = out.source.expect("编译通过必有 source");
    let transport = FixtureTransport {
        responses: vec![resp(SEARCH_HTML)],
        idx: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    let mut reuse = ResponseReuse::default();
    let inputs = RunInputs {
        kw: Some("斗".into()),
        ..Default::default()
    };
    match run_stage(&src, "search", &mut cx, &inputs, &mut reuse, &HostCaps::default()) {
        Ok(r) => {
            let list = r.out.get("list").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            // 铁律：主结果 name 必须保留（字符串非空，或多命中数组的每个元素非空）
            for item in &list {
                let mv = item.get("map").and_then(|m| m.get("name"));
                if strict {
                    // 铁律 P2：optional 字段缺失不得清空主结果（name/url 必有值）
                    let ok = match mv {
                        Some(serde_json::Value::String(x)) => !x.is_empty(),
                        Some(serde_json::Value::Array(a)) => {
                            !a.is_empty() && a.iter().all(|x| x.as_str().map(|s| !s.is_empty()).unwrap_or(false))
                        }
                        _ => false,
                    };
                    assert!(ok, "主结果 name 被清空: {item}");
                }
            }
            (true, format!("items={}", list.len()))
        }
        Err(d) => {
            let c = d.code.as_str().to_string();
            assert!(
                c.starts_with("E_"),
                "运行失败必须带 E_ 码: {c} {}",
                d.message
            );
            (true, format!("err={c}"))
        }
    }
}

// ───────────────────────── P1：随机选择器永不 panic / 失败必有码 ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn p1_selector_never_panics(sel in nasty_string()) {
        let candidates = [
            sel.clone(),
            format!("css:{sel}"),
            format!("json:$.{sel}"),
            format!("re:{sel}"),
            format!("var:{sel}"),
        ];
        for s in candidates {
            let _ = run_search_strict(&s, &[], false);
        }
    }

    // ───────────────────────── P2：optional 缺失不清空主结果 ─────────────────────────

    #[test]
    fn p2_optional_missing_keeps_primary(miss in 1usize..=4) {
        // 固定 miss 的 optional 字段（选择器必 miss）——数量随机
        let extra: Vec<String> = (0..miss).map(|i| format!("css:.missing-{i}")).collect();
        let (ok, summary) = run_search_strict("css:ul.result > li", &extra, true);
        prop_assert!(ok && summary.starts_with("items=2"), "主结果应保留: {summary}");
    }

    // ───────────────────────── P4：parse_template 永不 panic ─────────────────────────

    #[test]
    fn p4_template_never_panics(s in nasty_string()) {
        match sugar::parse_template(&s, "prop") {
            Ok(_) => {}
            Err(d) => prop_assert_eq!(d.code.as_str(), "E_TEMPLATE"),
        }
    }

    // ───────────────────────── P5：jsonpath/xpath parse 永不 panic ─────────────────────────

    #[test]
    fn p5_jsonpath_xpath_never_panics(s in nasty_string()) {
        let _ = jsonpath::parse(&s);
        let _ = xpath::parse(&s);
    }

    // ───────────────────────── P6：expr parse 永不 panic + 深度有限 ─────────────────────────

    #[test]
    fn p6_expr_parse_never_panics(s in nasty_string()) {
        match expr::parse(&s, "prop") {
            Ok(_) => {}
            Err(d) => {
                let c = d.code.as_str();
                prop_assert!(c.starts_with("E_EXPR") || c == "E_EXPR_LIMIT", "expr 错误码 {c}");
            }
        }
    }

    #[test]
    fn p6b_expr_depth_limited(depth in 33..200usize) {
        let s = "(".repeat(depth) + "1" + &")".repeat(depth);
        prop_assert!(expr::parse(&s, "prop").is_err(), "深度 {depth} 应被拒绝");
    }

    // ───────────────────────── P7：regex check 永不 panic ─────────────────────────

    #[test]
    fn p7_regex_check_never_panics(s in nasty_regex()) {
        match compile_regex(&s, "", "prop") {
            Ok(_) => {}
            Err(d) => {
                let c = d.code.as_str();
                prop_assert!(
                    c == "E_REGEX" || c == "E_REGEX_UNSUPPORTED",
                    "regex 错误码 {c}"
                );
            }
        }
    }
}

// ───────────────────────── P3：severity ↔ code 全表一致 ─────────────────────────

/// 反查表（测试内独立实现，用于检测 error.rs 表漂移）。
fn code_from_str(s: &str) -> Option<Code> {
    Some(match s {
        "E_PARSE" => Code::EParse,
        "E_SCHEMA" => Code::ESchema,
        "E_ENUM" => Code::EEnum,
        "E_REF" => Code::ERef,
        "E_TYPE" => Code::EType,
        "E_REGEX" => Code::ERegex,
        "E_REGEX_UNSUPPORTED" => Code::ERegexUnsupported,
        "E_SELECTOR" => Code::ESelector,
        "E_COST" => Code::ECost,
        "E_EXPR_SYNTAX" => Code::EExprSyntax,
        "E_EXPR_FN" => Code::EExprFn,
        "E_EXPR_LIMIT" => Code::EExprLimit,
        "E_TEMPLATE" => Code::ETemplate,
        "E_UNSUPPORTED" => Code::EUnsupported,
        "E_VERSION_UNSUPPORTED" => Code::EVersionUnsupported,
        "E_NET" => Code::ENet,
        "E_TIMEOUT" => Code::ETimeout,
        "E_CANCELLED" => Code::ECancelled,
        "E_HTTP" => Code::EHttp,
        "E_CHARSET" => Code::ECharset,
        "E_LOGIN_REQUIRED" => Code::ELoginRequired,
        "E_CHALLENGE" => Code::EChallenge,
        "E_DECODE" => Code::EDecode,
        "E_NO_MATCH" => Code::ENoMatch,
        "E_EMPTY" => Code::EEmpty,
        "E_FIELD_MISSING" => Code::EFieldMissing,
        "E_VERIFY" => Code::EVerify,
        "E_PROTECT_LOST" => Code::EProtectLost,
        "E_BUDGET" => Code::EBudget,
        "E_SCRIPT" => Code::EScript,
        "E_HOST_STAGE" => Code::EHostStage,
        "W_UNKNOWN_FIELD" => Code::WUnknownField,
        "W_BUDGET_TRUNCATED" => Code::WBudgetTruncated,
        "W_DEGRADED" => Code::WDegraded,
        "W_DEPRECATED" => Code::WDeprecated,
        "W_ZIP_LENGTH" => Code::WZipLength,
        _ => return None,
    })
}

#[test]
fn p3_severity_table_consistent() {
    let all = Code::all_codes();
    assert_eq!(all.len(), 36, "码表数量 36");
    for s in all {
        let c = code_from_str(s).unwrap_or_else(|| panic!("all_codes 里 {s} 反查失败（表漂移）"));
        // as_str 往返
        assert_eq!(c.as_str(), *s);
        // severity 规则：E_EMPTY → Info；W_* → Warning；其余 → Error
        let expect = if *s == "E_EMPTY" {
            Severity::Info
        } else if s.starts_with("W_") {
            Severity::Warning
        } else {
            Severity::Error
        };
        assert_eq!(c.severity(), expect, "{s} severity 漂移");
        assert_eq!(c.is_warning(), s.starts_with("W_"), "{s} is_warning 漂移");
        // Diagnostic::new 默认 severity 与码一致
        let d = rsl_core::error::Diagnostic::new(c, "p", "m", "h");
        assert_eq!(d.severity, c.severity(), "{s} Diagnostic 默认 severity 漂移");
    }
}

// ───────────────────────── P8：bincode 往返 = 结构相等 ─────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn p8_bincode_roundtrip(
        id in "[a-z]{1,12}",
        title in "[a-zA-Z0-9 ]{0,20}",
        sel in prop::sample::select(vec![
            "css:a.title", "css:ul > li", "text", "json:$.data.name", "css:#list li a",
        ]),
        trims in 0usize..3,
    ) {
        let mut transform = String::new();
        for i in 0..trims {
            if i > 0 {
                transform.push(',');
            }
            transform.push_str("{ \"trim\": true }");
        }
        let text = format!(
            r#"{{
  "rsl": "2.0", "id": "{id}.example.com", "name": "往返站《{title}》", "type": "novel",
  "baseUrl": "https://rt.example.com",
  "requires": {{ "features": [], "crypto": [] }},
  "stages": {{
    "search": {{
      "request": {{ "url": "https://rt.example.com/s", "params": {{ "q": "var:kw" }} }},
      "out": {{
        "list": "css:ul.result > li",
        "map": {{ "name": {{ "$": "{sel}"{tr} }}, "url": {{ "$": "css:a.title", "attr": "href", "transform": [{{ "absolute": true }}] }} }},
        "emptyOk": true
      }}
    }},
    "detail": {{
      "request": {{ "url": "var:book.url" }},
      "out": {{ "name": "css:h1", "tocUrl": {{ "$": "css:a.read", "attr": "href", "transform": [{{ "absolute": true }}], "optional": true }} }}
    }},
    "toc": {{
      "request": {{ "url": "var:book.tocUrl" }},
      "out": {{ "list": "css:#list li", "map": {{ "title": {{ "$": "css:a" }}, "url": {{ "$": "css:a", "attr": "href", "transform": [{{ "absolute": true }}] }} }} }}
    }},
    "content": {{
      "request": {{ "url": "var:chapter.url" }},
      "out": {{ "content": {{ "$": "css:div#content", "paragraphs": {{ "split": "br" }} }} }}
    }}
  }}
}}"#,
            tr = if transform.is_empty() { String::new() } else { format!(", \"transform\": [{transform}]") },
        );
        let out = compile(&text);
        prop_assert!(out.ok(), "随机源应编译通过: {:?}", out.errors().iter().map(|d| format!("{d}")).collect::<Vec<_>>());
        let src = out.source.expect("编译通过");
        let bytes = src.to_bincode().expect("bincode 序列化");
        let back = rsl_core::ir::CompiledSource::from_bincode(&bytes).expect("bincode 反序列化");
        prop_assert_eq!(back.id, src.id);
        prop_assert_eq!(serde_json::to_string(&back.stages).unwrap(), serde_json::to_string(&src.stages).unwrap());
        prop_assert_eq!(serde_json::to_string(&back.limits).unwrap(), serde_json::to_string(&src.limits).unwrap());
    }
}

// 保持 Rc import 不报 unused（引擎样板未来用）
#[allow(unused)]
fn _rc_probe() -> Rc<()> {
    Rc::new(())
}
