//! T1.12 feature / 算法声明检查（COMPILER §3.7）。用了什么就必须声明什么，缺 → E_UNSUPPORTED 并点名。
//! 唯一例外：hostStages 缺宿主支持 → E_HOST_STAGE（裁决 U3，不在本模块处理）。

use crate::error::{Code, Diagnostic};
use crate::ir::{BaseSel, RequestIR, SelectorIR, StageIR, StepIR};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct Usage {
    pub features: BTreeSet<String>,
    pub crypto_algs: BTreeSet<String>,
    /// (path, what) 用于点名。
    pub sites: Vec<(String, String)>,
}

/// 收集整份源的能力使用。
pub fn collect_usage(src: &crate::ir::CompiledSource) -> Usage {
    let mut u = Usage::default();
    let has_lib = !src.lib.pure.trim().is_empty() || !src.lib.ctx.trim().is_empty();
    if has_lib {
        u.features.insert("js".into());
    }
    for stage in src.stages.values().chain(src.host_stages.values()) {
        collect_stage(stage, &mut u);
    }
    if let Some(login) = &src.login {
        for s in &login.steps {
            if let Some(sub) = &s.submit {
                collect_request(&sub.req, &mut u);
            }
        }
        if let Some(r) = &login.check_req {
            collect_request(r, &mut u);
        }
    }
    for c in &src.cache {
        if let Some(r) = &c.req {
            collect_request(r, &mut u);
        }
    }
    u
}

fn mark(u: &mut Usage, feat: &str, site: &str, what: &str) {
    u.features.insert(feat.to_string());
    u.sites.push((site.to_string(), what.to_string()));
}

fn collect_stage(stage: &StageIR, u: &mut Usage) {
    let site = format!("stages.{}", stage.name);
    collect_request_opt(&stage.request, &site, u);
    for (_, l) in &stage.lets {
        match l {
            crate::ir::LetIR::Request { req, .. } => collect_request(req, u),
            crate::ir::LetIR::Expr(e) => {
                let mut fns = std::collections::BTreeSet::new();
                crate::expr::check::collect(e, &mut BTreeSet::new(), &mut fns);
                for f in fns {
                    if let Some((feat, alg)) = f.requirement() {
                        if !feat.is_empty() && f != crate::expr::FnId::Cookie {
                            u.features.insert(feat.to_string());
                            u.sites.push((site.clone(), format!("expr:{}", f.name())));
                            if let Some(a) = alg {
                                u.crypto_algs.insert(a.to_string());
                            }
                        }
                    }
                }
            }
            crate::ir::LetIR::Lib(_) => {
                u.features.insert("js".into());
            }
            _ => {}
        }
    }
    if !stage.script.before.is_none()
        || !stage.script.on_response.is_none()
        || !stage.script.after.is_none()
    {
        mark(u, "js", &site, "script 钩子");
    }
    for sel in iterate_stage_selectors(stage) {
        collect_selector(&sel, &site, u);
    }
}

fn collect_request_opt(req: &Option<RequestIR>, site: &str, u: &mut Usage) {
    if let Some(r) = req {
        collect_request(r, u);
        let _ = site;
    }
}

fn collect_request(req: &RequestIR, u: &mut Usage) {
    if req.web_js.is_some() || req.sniff.is_some() || req.engine == "webview" {
        u.features.insert("webview".into());
    }
}

/// 遍历阶段里的全部选择器（编译期 feature 分析用）。
fn iterate_stage_selectors(stage: &StageIR) -> Vec<SelectorIR> {
    let mut out = vec![];
    if let Some(p) = &stage.parse {
        out.push((**p).clone());
    }
    let o = &stage.out;
    for s in [&o.list, &o.title, &o.content, &o.images, &o.audio, &o.video, &o.files, &o.next_page, &o.pay_action] {
        if let Some(sel) = s {
            out.push((**sel).clone());
        }
    }
    for (_, sel) in &o.map {
        out.push(sel.clone());
    }
    if let Some(v) = &o.verify {
        let _ = v;
    }
    for (_, l) in &stage.lets {
        if let crate::ir::LetIR::Selector(s) = l {
            out.push((**s).clone());
        }
    }
    if let Some(gf) = &stage.groups_from {
        out.push((**gf).clone());
    }
    out
}

fn collect_selector(sel: &SelectorIR, site: &str, u: &mut Usage) {
    match &sel.base {
        BaseSel::Re { .. } => {}
        BaseSel::Expr(e) => {
            let mut fns = std::collections::BTreeSet::new();
            crate::expr::check::collect(e, &mut BTreeSet::new(), &mut fns);
            for f in fns {
                if let Some((feat, alg)) = f.requirement() {
                    if !feat.is_empty() && f != crate::expr::FnId::Cookie {
                        u.features.insert(feat.to_string());
                        u.sites.push((site.to_string(), format!("expr:{}", f.name())));
                        if let Some(a) = alg {
                            u.crypto_algs.insert(a.to_string());
                        }
                    }
                }
            }
        }
        BaseSel::Lib(_) => {
            u.features.insert("js".into());
        }
        _ => {}
    }
    for step in &sel.transform {
        collect_step(step, site, u);
    }
    for s in &sel.strip {
        collect_selector(s, site, u);
    }
    for f in &sel.fallback {
        collect_selector(f, site, u);
    }
    for (_, sub) in &sel.map {
        collect_selector(sub, site, u);
    }
    if let Some(crate::ir::WhereIR::Has(h)) = &sel.where_ {
        collect_selector(h, site, u);
    }
    if let Some(crate::ir::ProtectIR::Nodes(p)) = &sel.protect {
        collect_selector(p, site, u);
    }
}

fn collect_step(step: &StepIR, site: &str, u: &mut Usage) {
    match step {
        StepIR::Crypto { alg, .. } => {
            u.features.insert("crypto".into());
            u.crypto_algs.insert(alg.clone());
            u.sites.push((site.to_string(), format!("transform.crypto:{alg}")));
        }
        StepIR::Font { .. } => {
            u.features.insert("font".into());
            u.sites.push((site.to_string(), "transform.font".to_string()));
        }
        StepIR::Js { .. } => {
            u.features.insert("js".into());
        }
        StepIR::Zip { with, .. } => collect_selector(with, site, u),
        StepIR::Protect(crate::ir::ProtectIR::Nodes(p)) => collect_selector(p, site, u),
        _ => {}
    }
}

/// 检查使用 vs 声明。缺什么报什么。
pub fn check_declared(
    usage: &Usage,
    requires: &crate::ir::RequiresIR,
) -> Vec<Diagnostic> {
    let mut diags = vec![];
    for f in &usage.features {
        if !requires.features.contains(f) {
            let site = usage
                .sites
                .iter()
                .find(|(_, what)| what_of(what) == Some(f))
                .map(|(p, what)| format!("{p}（{what}）"))
                .unwrap_or_default();
            diags.push(Diagnostic::new(
                Code::EUnsupported,
                "requires.features",
                format!("使用了 {f} 能力但 requires.features 未声明"),
                format!("requires.features 加 \"{f}\"；触发点：{site}"),
            ));
        }
    }
    for a in &usage.crypto_algs {
        if !requires.crypto.contains(a) {
            diags.push(Diagnostic::new(
                Code::EUnsupported,
                "requires.crypto",
                format!("crypto 算法 `{a}` 未声明"),
                format!("requires.crypto 加 \"{a}\"（features 也要含 crypto）"),
            ));
        }
    }
    diags
}

fn what_of(what: &str) -> Option<&str> {
    // "expr:now()" → time；"transform.crypto:aes-cbc" → crypto；"transform.font" → font；"script 钩子" → js
    if let Some(rest) = what.strip_prefix("expr:") {
        return Some(match rest {
            "now()" => "time",
            "rand()" => "random",
            _ => "crypto",
        });
    }
    if what.starts_with("transform.crypto:") {
        return Some("crypto");
    }
    if what == "transform.font" {
        return Some("font");
    }
    if what.contains("script") || what.contains("lib") {
        return Some("js");
    }
    None
}
