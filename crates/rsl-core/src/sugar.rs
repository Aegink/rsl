//! T1.2 `sugar`：字符串选择器 → 对象；`${}` 插值解析为模板段（S§3.3 / S§4.1）。
//! 模板骨架错误（未闭合 / 空 / 嵌套 / 未知命名空间）→ E_TEMPLATE；
//! 变量未定义仍 E_REF，表达式非法仍 E_EXPR_SYNTAX（裁决 U4 / G-04）。

use crate::error::{Code, Diagnostic};
use serde::{Deserialize, Serialize};

/// `${ns:path}` 模板段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Seg {
    Lit(String),
    Tmpl { ns: String, expr: String },
}

/// RSL 全部 18 个命名空间（S§3.2，含 cache:）。用于报错清单与模板 ns 校验。
pub const NAMESPACES: &[&str] = &[
    "css:", "xpath:", "json:", "re:", "text", "owntext", "html", "tag",
    "attr:", "const:", "var:", "let:", "session:", "token:", "cookie:",
    "cache:", "lib:", "expr:",
];

/// 无参命名空间（不带冒号）。
pub const BARE_NS: &[&str] = &["text", "owntext", "html", "tag"];

/// 选择器字符串的命名空间是否合法。返回 Err(msg) 表示非法。
pub fn check_ns(s: &str) -> Result<(), &'static str> {
    if BARE_NS.contains(&s) {
        return Ok(());
    }
    for p in NAMESPACES {
        if let Some(stripped) = s.strip_prefix(p) {
            let param_ns = matches!(*p, "attr:" | "const:" | "var:" | "let:" | "session:" | "token:" | "cookie:" | "cache:" | "lib:" | "expr:");
            if param_ns && stripped.is_empty() {
                return Err("命名空间参数为空");
            }
            return Ok(());
        }
    }
    Err("选择器缺命名空间")
}

/// 解析 `${...}` 模板。整个字符串没有 `${` 时返回单一 Lit。
pub fn parse_template(s: &str, path: &str) -> Result<Vec<Seg>, Diagnostic> {
    let ch: Vec<char> = s.chars().collect();
    let mut out: Vec<Seg> = vec![];
    let mut lit = String::new();
    let mut i = 0usize;
    while i < ch.len() {
        let c = ch[i];
        if c == '$' && i + 1 < ch.len() && ch[i + 1] == '{' {
            // 找配对的 `}`（不允许嵌套）
            let mut j = i + 2;
            let mut closed = false;
            while j < ch.len() {
                if ch[j] == '}' {
                    closed = true;
                    break;
                }
                if ch[j] == '$' && j + 1 < ch.len() && ch[j + 1] == '{' {
                    break; // 嵌套
                }
                j += 1;
            }
            if !closed {
                let inner: String = ch[i..ch.len().min(j + 1)].iter().collect();
                return Err(Diagnostic::new(
                    Code::ETemplate,
                    path,
                    format!("`${{` 未闭合：{inner}"),
                    "补上 }；若本意是字面量 $，不要写 ${",
                ));
            }
            if ch[j] != '}' {
                return Err(Diagnostic::new(
                    Code::ETemplate,
                    path,
                    "模板嵌套不允许",
                    "需要嵌套就先用 let 算好中间值",
                ));
            }
            let inner: String = ch[i + 2..j].iter().collect();
            if inner.trim().is_empty() {
                return Err(Diagnostic::new(
                    Code::ETemplate,
                    path,
                    "`${}` 为空",
                    "直接写字面量，或补全命名空间路径",
                ));
            }
            let (ns, expr) = split_ns(&inner, path)?;
            if !lit.is_empty() {
                out.push(Seg::Lit(std::mem::take(&mut lit)));
            }
            out.push(Seg::Tmpl { ns, expr });
            i = j + 1;
            continue;
        }
        lit.push(c);
        i += 1;
    }
    if !lit.is_empty() || out.is_empty() {
        out.push(Seg::Lit(lit));
    }
    Ok(out)
}

/// `${inner}` 内部必须是 `ns:rest`；ns 必须是已知命名空间（S§4.1：未知命名空间 → E_TEMPLATE）。
fn split_ns(inner: &str, path: &str) -> Result<(String, String), Diagnostic> {
    if let Some(pos) = inner.find(':') {
        let ns = &inner[..pos];
        let known = NAMESPACES
            .iter()
            .any(|p| p.trim_end_matches(':') == ns)
            || BARE_NS.contains(&ns);
        if !known {
            return Err(Diagnostic::new(
                Code::ETemplate,
                path,
                format!("`${{{inner}}}` 的命名空间 `{ns}:` 不认识"),
                format!("合法命名空间：{}", NAMESPACES.join(" ")),
            ));
        }
        let rest = inner[pos + 1..].to_string();
        if rest.is_empty() && ns != "text" && ns != "owntext" && ns != "html" && ns != "tag" {
            return Err(Diagnostic::new(
                Code::ETemplate,
                path,
                format!("`${{{inner}}}` 命名空间参数为空"),
                "写全路径，如 var:kw",
            ));
        }
        Ok((ns.to_string(), rest))
    } else {
        Err(Diagnostic::new(
            Code::ETemplate,
            path,
            format!("`${{{inner}}}` 缺少命名空间"),
            "命名空间写全：${expr:page} / ${token:auth}（ERRORS §2.13）",
        ))
    }
}

/// 字符串是否为"选择器字符串"（有命名空间前缀或本身就是无参命名空间）。
pub fn is_selector_string(s: &str) -> bool {
    check_ns(s).is_ok()
}

/// 值字符串糖（D11 裁决）：开头允许 `var:x`/`let:x`/`session:x`/`token:x`/`cookie:x`/`cache:x`
/// 命名空间引用（路径读到非 [A-Za-z0-9_.] 字符为止），其余为字面量 + `${}` 模板。
/// 例："var:line/search" → [Tmpl{var,line}, Lit"/search"]；"https://x?q=${var:kw}" → [Lit, Tmpl]。
pub fn parse_value_string(s: &str, path: &str) -> Result<Vec<Seg>, Diagnostic> {
    const LEAD_NS: &[&str] = &["var", "let", "session", "token", "cookie", "cache"];
    let mut out: Vec<Seg> = vec![];
    let mut rest = s;
    // 1) 开头的裸命名空间引用（可连续）
    loop {
        if rest.is_empty() {
            break;
        }
        let mut matched = false;
        for ns in LEAD_NS {
            let prefix = format!("{ns}:");
            if let Some(r) = rest.strip_prefix(&prefix) {
                let pathstr: String = r
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                    .collect();
                if pathstr.is_empty() {
                    return Err(Diagnostic::new(
                        Code::ETemplate,
                        path,
                        format!("`{prefix}` 后缺少路径"),
                        "写全变量路径",
                    ));
                }
                let consumed = prefix.len() + pathstr.len();
                out.push(Seg::Tmpl { ns: ns.to_string(), expr: pathstr });
                rest = &rest[consumed..];
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
    // 2) 剩余文本走 ${} 模板扫描（含纯字面量）
    let tail = parse_template(rest, path)?;
    out.extend(tail);
    // 3) 相邻 Lit 合并 + 空兜底
    let mut merged: Vec<Seg> = vec![];
    for seg in out {
        let is_lit = matches!(merged.last(), Some(Seg::Lit(_))) && matches!(&seg, Seg::Lit(_));
        if is_lit {
            if let (Some(Seg::Lit(prev)), Seg::Lit(cur)) = (merged.last_mut(), seg) {
                prev.push_str(&cur);
            }
        } else {
            merged.push(seg);
        }
    }
    if merged.is_empty() {
        merged.push(Seg::Lit(String::new()));
    }
    Ok(merged)
}
