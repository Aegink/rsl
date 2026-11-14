//! T1.8 `lib` 签名与档位扫描：`function NAME(` 轻量扫描；ctx 首参必须是 `ctx`；
//! `lib:` 选择器命中 ctx 档 → E_SCRIPT。

use crate::error::{Code, Diagnostic};
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct LibScan {
    pub pure_fns: Vec<String>,
    pub ctx_fns: Vec<String>,
}

impl LibScan {
    pub fn is_pure(&self, name: &str) -> bool {
        self.pure_fns.iter().any(|f| f == name)
    }
    pub fn is_ctx(&self, name: &str) -> bool {
        self.ctx_fns.iter().any(|f| f == name)
    }
    pub fn exists(&self, name: &str) -> bool {
        self.is_pure(name) || self.is_ctx(name)
    }
    pub fn tier_of(&self, name: &str) -> Option<crate::ir::Tier> {
        use crate::ir::Tier;
        if self.is_pure(name) {
            Some(Tier::Pure)
        } else if self.is_ctx(name) {
            Some(Tier::Ctx)
        } else {
            None
        }
    }
}

fn scan(src: &str, tier_is_ctx: bool, path: &str) -> Result<Vec<String>, Diagnostic> {
    let mut names = vec![];
    if src.trim().is_empty() {
        return Ok(names);
    }
    let re = match Regex::new(r"(?m)function\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*\(([^)]*)\)") {
        Ok(r) => r,
        Err(_) => return Ok(names),
    };
    for cap in re.captures_iter(src) {
        let name = cap[1].to_string();
        if tier_is_ctx {
            let params = cap[2].trim();
            let first = params.split(',').next().unwrap_or("").trim();
            if first != "ctx" {
                return Err(Diagnostic::new(
                    Code::EScript,
                    path,
                    format!("lib.ctx 函数 `{name}` 的第一个参数必须是 `ctx`"),
                    "签名写 function name(ctx, ...)；ctx 函数只能从 script 钩子或 {\"ctx\":true} 调用",
                ));
            }
        }
        names.push(name);
    }
    Ok(names)
}

/// 扫描 pure/ctx 两档。返回 (scan, 诊断)。
pub fn scan_lib(
    pure: Option<&str>,
    ctx: Option<&str>,
) -> Result<LibScan, Diagnostic> {
    let mut s = LibScan::default();
    s.pure_fns = scan(pure.unwrap_or(""), false, "lib.pure")?;
    s.ctx_fns = scan(ctx.unwrap_or(""), true, "lib.ctx")?;
    Ok(s)
}

/// 提取某个函数的源码（从 `function NAME(` 到配对大括号），供 onResponse 的 ctx.http 扫描用。
pub fn fn_source(src: &str, name: &str) -> Option<String> {
    let needle = format!("function {name}(");
    let start = src.find(&needle)?;
    let rest = &src[start..];
    // 找第一个 `{`，然后按深度取到配对 `}`
    let open = rest.find('{')?;
    let mut depth = 0usize;
    let mut end = rest.len();
    for (i, c) in rest[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    Some(rest[..end].to_string())
}

/// onResponse 钩子的 `ctx.http` 递归扫描（COMPILER §3.4.1）。
/// 返回true = 命中（请求递归）。
pub fn scan_ctx_http(ctx_src: &str, entry: &str, depth: usize) -> bool {
    if depth > 8 {
        return false;
    }
    let Some(body) = fn_source(ctx_src, entry) else {
        return false;
    };
    if body.contains("ctx.http") {
        return true;
    }
    // 可到达的同档函数
    let Ok(re) = regex::Regex::new(r"\b([A-Za-z_$][A-Za-z0-9_$]*)\s*\(") else {
        return false;
    };
    for cap in re.captures_iter(&body) {
        let callee = cap[1].to_string();
        if callee != entry && fn_source(ctx_src, &callee).is_some() {
            if scan_ctx_http(ctx_src, &callee, depth + 1) {
                return true;
            }
        }
    }
    false
}
