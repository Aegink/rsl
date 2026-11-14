//! TASK-BOOK-3 E1/E2：构建期受控远程——`x-jslib` 内联。
//! V3-D7：远程拉取**只发生在构建期**（`rsl build --inline-jslib`），运行时零远程拉取。
//! 内核不做网络：fetch 经注入闭包（CLI 提供 file:// + https:// 实现，测试注入内存 fixture）。
//! 确定性（E2）：同输入字节一致；sha256 不符默认拒绝，`--force` 显式覆盖并记警告。

use crate::error::{Code, Diagnostic};

/// manifest 条目：每个被内联的远程库。
#[derive(Debug, Clone, serde::Serialize)]
pub struct JslibEntry {
    pub url: String,
    /// 拉取内容的 sha256（hex，64 位）。
    pub sha256: String,
    /// 声明期校验值（源里写了 sha256 字段时）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declared_sha256: Option<String>,
}

/// 内联结果：新源文本 + manifest 条目 + 警告（--force 覆盖 sha 不符等）。
#[derive(Debug)]
pub struct InlineOut {
    pub text: String,
    pub entries: Vec<JslibEntry>,
    pub warnings: Vec<Diagnostic>,
}

/// 对源文本执行 x-jslib 内联。无 x-jslib 声明时原样返回（构建字节不变，E1 回归）。
pub fn inline_jslib(
    text: &str,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>, String>,
    force: bool,
) -> Result<InlineOut, Diagnostic> {
    let mut root: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        Diagnostic::new(Code::EParse, "$", format!("x-jslib 源解析失败：{e}"), "—")
    })?;
    let Some(decls) = root.get("x-jslib").cloned() else {
        return Ok(InlineOut {
            text: text.to_string(),
            entries: vec![],
            warnings: vec![],
        });
    };
    let serde_json::Value::Array(items) = decls else {
        return Err(Diagnostic::new(
            Code::ESchema,
            "x-jslib",
            "x-jslib 必须是数组",
            r#"[{ "url": "https://…/lib.js", "sha256": "<64hex>" }]"#,
        ));
    };
    let existing_fns = existing_lib_fns(&root);
    let mut new_entries: Vec<JslibEntry> = vec![];
    let mut warnings: Vec<Diagnostic> = vec![];
    let mut appended = String::new();
    for (i, it) in items.iter().enumerate() {
        let path = format!("x-jslib[{i}]");
        let Some(url) = it.get("url").and_then(|v| v.as_str()) else {
            return Err(Diagnostic::new(
                Code::ESchema,
                &path,
                "x-jslib 条目缺少 url",
                r#"{ "url": "https://…/lib.js", "sha256": "<64hex>" }"#,
            ));
        };
        let declared = it
            .get("sha256")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());
        let bytes = fetch(url).map_err(|e| {
            Diagnostic::new(
                Code::ENet,
                &path,
                format!("拉取 jsLib 失败：{url}"),
                format!("构建期拉取重试已耗尽：{e}；检查 URL 可达性"),
            )
            .retryable(true)
        })?;
        let sha = crate::crypto::sha256_hex(&bytes);
        if let Some(dec) = &declared {
            if dec != &sha {
                if !force {
                    return Err(Diagnostic::new(
                        Code::EVerify,
                        &path,
                        format!("jsLib 内容与声明 sha256 不符（{url}）"),
                        "更新 sha256 声明；确认可信后可用 build --force 覆盖",
                    )
                    .retryable(false));
                }
                warnings.push(Diagnostic::new(
                    Code::WDegraded,
                    &path,
                    format!("--force 覆盖：jsLib sha256 声明不符（声明 {dec}，实际 {sha}）"),
                    "内容已变化；核对来源可信后更新声明",
                ));
            }
        }
        let js = String::from_utf8_lossy(&bytes).to_string();
        // 重名函数 → E_SCRIPT（E1）
        let mut existing = existing_fns.clone();
        for n in extract_fn_names(&js) {
            if existing.contains(&n) {
                return Err(Diagnostic::new(
                    Code::EScript,
                    &path,
                    format!("jsLib 函数 `{n}` 与 lib 已有函数重名"),
                    "改名 jsLib 函数或移除 lib 中同名函数",
                )
                .rule(url));
            }
            existing.insert(n);
        }
        if !appended.is_empty() {
            appended.push_str("\n\n");
        }
        appended.push_str(js.trim());
        new_entries.push(JslibEntry {
            url: url.to_string(),
            sha256: sha,
            declared_sha256: declared,
        });
    }
    // 写回：lib.pure 追加；移除 x-jslib（运行时零远程拉取断言的载体）
    if !appended.is_empty() {
        let lib = root
            .as_object_mut()
            .ok_or_else(|| Diagnostic::new(Code::ESchema, "$", "源根必须是对象", "—"))?
            .entry("lib")
            .or_insert_with(|| serde_json::json!({}));
        let obj = lib
            .as_object_mut()
            .ok_or_else(|| Diagnostic::new(Code::ESchema, "lib", "lib 必须是对象", "—"))?;
        let cur = obj.get("pure").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let merged = if cur.trim().is_empty() {
            appended
        } else {
            format!("{}\n\n{}", cur.trim_end(), appended)
        };
        obj.insert("pure".into(), serde_json::Value::String(merged));
    }
    root.as_object_mut()
        .ok_or_else(|| Diagnostic::new(Code::ESchema, "$", "源根必须是对象", "—"))?
        .remove("x-jslib");
    let out_text = serde_json::to_string_pretty(&root).map_err(|e| {
        Diagnostic::new(Code::EParse, "$", format!("内联结果序列化失败：{e}"), "—")
    })?;
    Ok(InlineOut {
        text: out_text + "\n",
        entries: new_entries,
        warnings,
    })
}

/// 既有 lib 函数名（pure + ctx），用于重名检测。
fn existing_lib_fns(root: &serde_json::Value) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(lib) = root.get("lib") else { return out };
    for key in ["pure", "ctx"] {
        if let Some(js) = lib.get(key).and_then(|v| v.as_str()) {
            for n in extract_fn_names(js) {
                out.insert(n);
            }
        }
    }
    out
}

/// 提取 JS 里的具名函数（`function NAME(` 形式）。
pub fn extract_fn_names(js: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = js;
    while let Some(p) = rest.find("function") {
        let after = &rest[p + 8..];
        let trimmed = after.trim_start();
        let bytes = trimmed.as_bytes();
        // 匿名函数（function( / function ( ）跳过
        let is_ident_start =
            bytes.first().map(|b| b.is_ascii_alphabetic() || *b == b'_').unwrap_or(false);
        if is_ident_start {
            let name: String = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let after_name = &trimmed[name.len()..];
            if after_name.trim_start().starts_with('(') && !name.is_empty() {
                out.push(name);
            }
        }
        rest = after;
    }
    out
}
