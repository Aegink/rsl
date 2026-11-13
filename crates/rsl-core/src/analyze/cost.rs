//! T1.9 逐章成本白名单（S§6.5.1 / COMPILER §3.6）。
//! `toc.out.map.*` 逐章求值：成本 = 章数 × 字段数 × 跨桥次数。编译期强制白名单。

use crate::error::{Code, Diagnostic};
use crate::ir::{BaseSel, ProtectIR, SelectorIR, StepIR, WhereIR};

/// 逐章允许的选择器命名空间（白名单，逐字对齐 S§6.5.1）。
const ALLOWED_NS: &[&str] = &[
    "css:", "xpath:", "json:", "re:", "text", "owntext", "html", "tag",
    "attr:", "const:", "var:", "let:", "session:",
];

/// 逐章允许的管道步骤（含 zip/protect/font/js(lib.pure)，S§6.5.1 + 裁决 U1）。
const ALLOWED_STEPS: &[&str] = &[
    "trim", "replace", "regex", "match", "prepend", "append", "absolute",
    "urlencode", "urldecode", "base64", "hex", "html", "json", "date",
    "number", "numChapter", "cjkNumber", "t2s", "s2t", "slice", "join",
    "split", "zip", "protect", "font", "unique", "sort", "dropEmpty",
];

pub const COST_HINT: &str = "把该字段的计算挪到 list 选择器的父规则里一次算完，或改用 transform.date";

/// 检查一个逐章选择器子树。`lib_pure` 用于判断 js/lib: 是否 pure 档。
pub fn check_per_item(sel: &SelectorIR, path: &str, lib_pure: &[String]) -> Vec<Diagnostic> {
    let mut diags = vec![];
    check_selector(sel, path, lib_pure, &mut diags);
    diags
}

fn cost_err(path: &str, rule: &str, msg: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::ECost, path, msg.into(), COST_HINT).rule(rule)
}

fn check_base(base: &BaseSel, raw: &str, path: &str, lib_pure: &[String], diags: &mut Vec<Diagnostic>) {
    match base {
        BaseSel::Token(_) => diags.push(cost_err(path, raw, "逐章规则禁止 token: 命名空间")),
        BaseSel::Cookie(_) => diags.push(cost_err(path, raw, "逐章规则禁止 cookie: 命名空间")),
        BaseSel::Cache(_) => diags.push(cost_err(path, raw, "逐章规则禁止 cache: 命名空间")),
        BaseSel::Lib(name) => {
            if !lib_pure.iter().any(|f| f == name) {
                diags.push(cost_err(
                    path,
                    raw,
                    "逐章规则不得调用 lib.ctx（只有 lib.pure 可用）",
                ));
            }
        }
        BaseSel::Expr(e) => {
            let mut fns = std::collections::BTreeSet::new();
            crate::expr::check::collect(e, &mut std::collections::BTreeSet::new(), &mut fns);
            for f in fns {
                if matches!(f, crate::expr::FnId::Now | crate::expr::FnId::Rand | crate::expr::FnId::Cookie) {
                    diags.push(cost_err(
                        path,
                        raw,
                        format!("逐章规则禁止 expr:{}()", f.name()),
                    ));
                }
            }
        }
        _ => {
            // 白名单命名空间已在编译 ns 检查覆盖；此处只拦白名单外
            if let Some(bad) = raw.split(':').next() {
                let prefix = if raw.contains(':') { bad.to_string() + ":" } else { bad.to_string() };
                let known = crate::sugar::NAMESPACES.contains(&prefix.as_str())
                    || crate::sugar::BARE_NS.contains(&prefix.as_str());
                if known && !ALLOWED_NS.contains(&prefix.as_str()) {
                    diags.push(cost_err(
                        path,
                        raw,
                        format!("逐章规则禁止 {prefix} 命名空间"),
                    ));
                }
            }
        }
    }
}

fn check_selector(sel: &SelectorIR, path: &str, lib_pure: &[String], diags: &mut Vec<Diagnostic>) {
    check_base(&sel.base, &sel.raw, path, lib_pure, diags);
    for w in &sel.where_ {
        check_where(w, path, lib_pure, diags);
    }
    if let Some(WhereIR::Has(h)) = &sel.where_ {
        check_selector(h, path, lib_pure, diags);
    }
    for (_k, sub) in &sel.map {
        check_selector(sub, path, lib_pure, diags);
    }
    for s in &sel.strip {
        check_selector(s, path, lib_pure, diags);
    }
    for f in &sel.fallback {
        check_selector(f, path, lib_pure, diags);
    }
    match &sel.protect {
        Some(ProtectIR::Nodes(p)) => check_selector(p, path, lib_pure, diags),
        _ => {}
    }
    for step in &sel.transform {
        check_step(step, path, lib_pure, diags);
    }
}

fn check_where(w: &WhereIR, path: &str, lib_pure: &[String], diags: &mut Vec<Diagnostic>) {
    match w {
        WhereIR::Has(h) => check_selector(h, path, lib_pure, diags),
        WhereIR::Not(inner) => check_where(inner, path, lib_pure, diags),
        WhereIR::Text(_) | WhereIR::Cmp { .. } => {}
    }
}

fn check_step(step: &StepIR, path: &str, lib_pure: &[String], diags: &mut Vec<Diagnostic>) {
    let key = match step {
        StepIR::Trim { .. } => "trim",
        StepIR::Replace { .. } => "replace",
        StepIR::Regex { .. } => "regex",
        StepIR::Match { .. } => "match",
        StepIR::Prepend(_) => "prepend",
        StepIR::Append(_) => "append",
        StepIR::Absolute { .. } => "absolute",
        StepIR::Urlencode { .. } => "urlencode",
        StepIR::Urldecode { .. } => "urldecode",
        StepIR::Base64(_) => "base64",
        StepIR::Hex(_) => "hex",
        StepIR::Crypto { .. } => "crypto",
        StepIR::Font { .. } => "font",
        StepIR::Html => "html",
        StepIR::JsonStep => "json",
        StepIR::Date { .. } => "date",
        StepIR::Number { .. } => "number",
        StepIR::NumChapter => "numChapter",
        StepIR::CjkNumber(_) => "cjkNumber",
        StepIR::T2s => "t2s",
        StepIR::S2t => "s2t",
        StepIR::Slice { .. } => "slice",
        StepIR::Charset(_) => "charset",
        StepIR::Join(_) => "join",
        StepIR::Split(_) => "split",
        StepIR::Zip { .. } => "zip",
        StepIR::Protect(_) => "protect",
        StepIR::Unique => "unique",
        StepIR::Sort { .. } => "sort",
        StepIR::DropEmpty => "dropEmpty",
        StepIR::Js { fn_name, tier } => {
            match tier {
                crate::ir::Tier::Ctx => {
                    diags.push(cost_err(path, fn_name, "逐章规则不得调用 lib.ctx"))
                }
                crate::ir::Tier::Pure => {
                    if !lib_pure.iter().any(|f| f == fn_name) {
                        diags.push(cost_err(path, fn_name, "js 引用的函数不存在或不在 lib.pure"));
                    }
                }
            }
            return;
        }
    };
    if !ALLOWED_STEPS.contains(&key) {
        diags.push(cost_err(path, key, format!("逐章规则禁止管道步骤 {key}")));
    }
    // zip 的 with / protect 的节点选择器也必须合规
    if let StepIR::Zip { with, .. } = step {
        check_selector(with, path, lib_pure, diags);
    }
    if let StepIR::Protect(ProtectIR::Nodes(p)) = step {
        check_selector(p, path, lib_pure, diags);
    }
}
