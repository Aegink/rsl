//! M1 验收测试（任务书 §5.1 + COMPILER §7）：
//! ① 4 示例 0 错误；② 15 个编译期负例全报对；③ bincode 往返；④ expr 27 用例。

use rsl_core::analyze::compile_str;

/// 可编译的最小合法源骨架。
const BASE: &str = r#"{
  "rsl": "2.0", "id": "neg.example.com", "name": "负例站", "type": "novel",
  "baseUrl": "https://neg.example.com",
  "requires": { "features": ["js"], "crypto": [] },
  "lib": { "pure": "function cleanTitle(s){ return String(s||'').trim(); }", "ctx": "" },
  "stages": {
    "search": {
      "request": { "url": "https://neg.example.com/search", "params": { "q": "var:kw" } },
      "out": { "list": "css:ul.result > li", "map": {
        "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
        "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
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

/// 在骨架上打补丁（简单 JSON 文本替换，专供负例）。
fn patch(from: &str, to: &str) -> String {
    assert!(BASE.contains(from), "patch 源不存在: {from}");
    BASE.replace(from, to)
}

/// JSON 级 patch：解析骨架 → 修改 → 序列化（不受文本替换的括号问题影响）。
fn patch_json(f: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut v: serde_json::Value = serde_json::from_str(BASE).unwrap();
    f(&mut v);
    serde_json::to_string(&v).unwrap()
}

fn codes(text: &str) -> Vec<String> {
    compile_str(text)
        .diagnostics
        .iter()
        .filter(|d| !d.code.is_warning())
        .map(|d| d.code.as_str().to_string())
        .collect()
}

fn expect_code(text: &str, code: &str) {
    let cs = codes(text);
    assert!(
        cs.iter().any(|c| c == code),
        "期望 {code}，实际 {:?}\n—— 诊断 ——\n{}",
        cs,
        compile_str(text)
            .diagnostics
            .iter()
            .map(|d| format!("{d}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn example_path(name: &str) -> String {
    format!("{}/../../examples/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn examples_compile_clean() {
    for f in [
        example_path("novel-html.rsl.json"),
        example_path("comic-api.rsl.json"),
        example_path("video-login.rsl.json"),
        example_path("aggregate-merge.rsl.json"),
    ] {
        let text = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("读取 {f} 失败: {e}"));
        let out = compile_str(&text);
        assert!(
            out.ok(),
            "{f} 应 0 错误编译，实际：{:?}",
            out.errors().iter().map(|d| format!("{d}")).collect::<Vec<_>>()
        );
    }
}

// ─────────────── 15 个编译期负例（任务书 §5.1） ───────────────

#[test]
fn neg_01_selector_missing_ns() {
    let t = patch_json(|v| {
        v["stages"]["search"]["out"]["map"]["name"] = serde_json::json!("li.title");
    });
    expect_code(&t, "E_SELECTOR");
}

#[test]
fn neg_02_regex_on_list() {
    // D5 裁决：显式列表（split 产物 str[]）上的 regex → E_TYPE（"node[]/list 上做 regex"）
    let t = patch_json(|v| {
        v["stages"]["content"]["out"]["content"]["transform"] = serde_json::json!([
            { "split": "\n" },
            { "regex": { "regex": "x" } }
        ]);
    });
    expect_code(&t, "E_TYPE");
}

#[test]
fn neg_03_ref_typo_book() {
    expect_code(&patch("var:book.tocUrl", "var:book.urll"), "E_REF");
}

#[test]
fn neg_04_regex_backref() {
    expect_code(&patch("\"css:a.title\"", "\"re:(a)\\\\1\""), "E_REGEX_UNSUPPORTED");
}

#[test]
fn neg_05_ctx_in_per_item() {
    let t = patch_json(|v| {
        v["lib"]["ctx"] = serde_json::json!("function ctxFetch(ctx, s){ return ctx.http.get('https://x'); }");
        v["stages"]["toc"]["out"]["map"]["updateTime"] = serde_json::json!({
            "$": "text",
            "transform": [{ "js": { "fn": "lib:ctxFetch", "ctx": true } }],
            "optional": true
        });
    });
    expect_code(&t, "E_COST");
}

#[test]
fn neg_06_md5_undeclared() {
    let t = patch_json(|v| {
        v["stages"]["search"]["request"]["params"]["sign"] = serde_json::json!("expr:md5(var:kw)");
    });
    expect_code(&t, "E_UNSUPPORTED");
}

#[test]
fn neg_07_lib_selector_calls_ctx_fn() {
    let t = patch_json(|v| {
        v["lib"]["ctx"] = serde_json::json!("function fetcher(ctx){ return 1; }");
        v["stages"]["search"]["out"]["map"]["name"]["$"] = serde_json::json!("lib:fetcher");
        v["stages"]["search"]["out"]["map"]["name"]["transform"] = serde_json::Value::Null;
    });
    expect_code(&t, "E_SCRIPT");
}

#[test]
fn neg_08_missing_toc_stage() {
    let t = BASE.replace(
        "    \"toc\": {",
        "    \"tocs\": {\"request\": {\"url\":\"var:book.url\"}, \"out\": {\"list\": \"css:li\", \"map\": {\"title\":\"css:a\",\"url\":{\"$\":\"css:a\",\"attr\":\"href\"}}},",
    );
    // stages 缺 toc（把 toc 改名）→ 同时也触发未知阶段键 E_ENUM
    let t = t.replace("\"tocs\": {", "\"tocs\": {");
    expect_code(&t, "E_SCHEMA");
}

#[test]
fn neg_09_login_cycle() {
    let login = r#""login": {
      "type": "form",
      "steps": [
        { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
          "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                      "ok": { "$": "json:$.code", "eq": 0 } },
          "next": { "to": "sms" } },
        { "id": "sms", "fields": [{ "key": "code", "label": "短信码", "secret": true }],
          "submit": { "request": { "url": "https://x/api/verify", "method": "POST", "body": { "type": "json", "data": { "code": "var:login.code" } } },
                      "ok": { "$": "json:$.code", "eq": 0 } },
          "next": { "to": "cred" } }
      ]
    },"#;
    let t = BASE.replace("\"stages\":", &format!("{login}\n  \"stages\":"));
    let cs = codes(&t);
    assert!(cs.iter().any(|c| c == "E_SCHEMA"), "期望 E_SCHEMA，实际 {cs:?}");
    // 环路径 hint
    let out = compile_str(&t);
    let cyc = out
        .diagnostics
        .iter()
        .find(|d| d.code == rsl_core::error::Code::ESchema)
        .unwrap();
    assert!(cyc.message.contains("cred"), "环路径应含 cred：{}", cyc.message);
}

#[test]
fn neg_10_next_to_missing() {
    let login = r#""login": {
      "type": "form",
      "steps": [
        { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
          "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                      "ok": { "$": "json:$.code", "eq": 0 } },
          "next": { "to": "snp" } }
      ]
    },"#;
    let t = BASE.replace("\"stages\":", &format!("{login}\n  \"stages\":"));
    expect_code(&t, "E_REF");
}

#[test]
fn neg_11_duplicate_step_id() {
    let login = r#""login": {
      "type": "form",
      "steps": [
        { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
          "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                      "ok": { "$": "json:$.code", "eq": 0 } } },
        { "id": "cred", "fields": [{ "key": "code", "label": "码", "secret": true }],
          "submit": { "request": { "url": "https://x/api/verify", "method": "POST", "body": { "type": "json", "data": { "code": "var:login.code" } } },
                      "ok": { "$": "json:$.code", "eq": 0 } } }
      ]
    },"#;
    let t = BASE.replace("\"stages\":", &format!("{login}\n  \"stages\":"));
    expect_code(&t, "E_SCHEMA");
}

#[test]
fn neg_12_direct_vs_request() {
    let t = patch_json(|v| {
        v["stages"]["content"]["direct"] = serde_json::json!(true);
    });
    expect_code(&t, "E_SCHEMA");
}

#[test]
fn neg_13_pagevar_enumerate() {
    let t = patch_json(|v| {
        v["stages"]["search"]["paginate"] = serde_json::json!({
            "kind": "enumerate", "enumerate": [1, 2], "into": "params.q", "pageVar": "page"
        });
    });
    expect_code(&t, "E_SCHEMA");
}

#[test]
fn neg_14_onresponse_ctx_http() {
    let t = patch(
        "\"request\": { \"url\": \"var:chapter.url\" },",
        "\"request\": { \"url\": \"var:chapter.url\" },\n      \"script\": { \"onResponse\": { \"fn\": \"lib:fixBody\", \"ctx\": true } },",
    );
    let t = t.replace(
        "\"ctx\": \"\"",
        "\"ctx\": \"function fixBody(ctx, body){ return ctx.http.get('https://x'); }\"",
    );
    expect_code(&t, "E_SCRIPT");
}

#[test]
fn neg_15_merge_key_missing() {
    let t = patch(
        "\"requires\": { \"features\": [\"js\"], \"crypto\": [] },",
        "\"requires\": { \"features\": [\"js\"], \"crypto\": [] },\n  \"merge\": { \"key\": [\"name\", \"authro\"] },",
    );
    expect_code(&t, "E_REF");
}

// ─────────────── 加载期 ───────────────

#[test]
fn version_mismatch() {
    let t = BASE.replace("\"rsl\": \"2.0\"", "\"rsl\": \"3.0\"");
    expect_code(&t, "E_VERSION_UNSUPPORTED");
}

// ─────────────── T1.13 bincode 往返 ───────────────

#[test]
fn bincode_roundtrip() {
    let text = std::fs::read_to_string(example_path("novel-html.rsl.json")).unwrap();
    let out = compile_str(&text);
    let src = out.source.expect("编译通过");
    let bytes = src.to_bincode().expect("bincode 序列化");
    let back = rsl_core::ir::CompiledSource::from_bincode(&bytes).expect("bincode 反序列化");
    assert_eq!(back.id, src.id);
    assert_eq!(back.stages.len(), src.stages.len());
    assert_eq!(back.limits.timeout_ms, src.limits.timeout_ms);
    assert_eq!(back.merge.key, src.merge.key);
    // 选择器逐字段等价
    let a = serde_json::to_string(&src.stages["toc"]).unwrap();
    let b = serde_json::to_string(&back.stages["toc"]).unwrap();
    assert_eq!(a, b);
}
