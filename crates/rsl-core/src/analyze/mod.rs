//! 编译器编排（COMPILER §2 七步流水线）：
//! JSON 解析 → 版本协商 → 反序列化(model) → 消糖 → 语义分析(ns/refs/types/cost/graph/merge/feature)
//! → IR 降级 → CompiledSource。任何 error 级诊断都会让编译失败；warning 级随产物返回。

pub mod cost;
pub mod feature;
pub mod graph;
pub mod lib;
pub mod merge;
pub mod refs;
pub mod regex;
pub mod types;

use crate::error::{Code, Diagnostic};
use crate::expr::{self, Expr};
use crate::ir::*;
use crate::model::{self, Selector, StepKind, TransformStep};
use crate::sugar::{self, Seg};
use std::collections::{BTreeMap, BTreeSet};

pub struct CompileOutput {
    pub source: Option<CompiledSource>,
    /// error 级（阻断）+ warning 级全部诊断。
    pub diagnostics: Vec<Diagnostic>,
}

impl CompileOutput {
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| !d.code.is_warning()).collect()
    }
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.code.is_warning()).collect()
    }
    pub fn ok(&self) -> bool {
        self.source.is_some()
    }
}

/// 从 JSON 文本编译（入口）。
pub fn compile_str(text: &str) -> CompileOutput {
    let v: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return CompileOutput {
                source: None,
                diagnostics: vec![Diagnostic::from(e)],
            };
        }
    };
    compile_value(v)
}

pub fn compile_value(v: serde_json::Value) -> CompileOutput {
    let mut cx = Cx::default();
    let built = build(&mut cx, v);
    let has_error = cx.diags.iter().any(|d| !d.code.is_warning());
    CompileOutput {
        source: if has_error { None } else { built },
        diagnostics: cx.diags,
    }
}

#[derive(Default)]
struct Cx {
    diags: Vec<Diagnostic>,
    libs: lib::LibScan,
    lib_ctx_src: Option<String>,
    features: BTreeSet<String>,
    crypto: BTreeSet<String>,
    contract: refs::ContractFields,
    vars: BTreeMap<String, VarIR>,
    secrets: Vec<String>,
    has_login: bool,
    has_cookie_block: bool,
}

impl Cx {
    fn err(&mut self, d: Diagnostic) {
        self.diags.push(d);
    }
    fn decl(&self) -> expr::Declared {
        expr::Declared {
            features: self.features.clone(),
            crypto: self.crypto.clone(),
            has_login_or_cookie: self.has_login || self.has_cookie_block,
        }
    }
}

fn warn_unknown(extra: &BTreeMap<String, serde_json::Value>, path: &str, cx: &mut Cx) {
    for k in extra.keys() {
        if k.starts_with("x-") {
            continue;
        }
        cx.diags.push(
            Diagnostic::new(
                Code::WUnknownField,
                format!("{path}.{k}"),
                format!("未知字段 {k} 已忽略"),
                "自定义字段请写成 x- 前缀",
            ),
        );
    }
}

fn build(cx: &mut Cx, v: serde_json::Value) -> Option<CompiledSource> {
    // ── 版本协商 ──
    let rsl_raw = v.get("rsl").and_then(|x| x.as_str()).map(String::from);
    let rsl = match &rsl_raw {
        None => {
            cx.err(Diagnostic::new(Code::ESchema, "rsl", "缺少必填字段 rsl", "顶层写 \"rsl\": \"2.0\""));
            return None;
        }
        Some(s) => {
            let ok = s.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit()))
                && s.matches('.').count() == 1
                && !s.is_empty();
            if !ok {
                cx.err(Diagnostic::new(
                    Code::ESchema,
                    "rsl",
                    format!("rsl 版本 `{s}` 不符合 MAJOR.MINOR 格式"),
                    "写 \"2.0\"",
                ));
                return None;
            }
            let major: u32 = s.split('.').next().unwrap_or_default().parse().unwrap_or(0);
            if major != 2 {
                cx.err(Diagnostic::new(
                    Code::EVersionUnsupported,
                    "rsl",
                    format!("源需要 rsl {major}.x，本引擎支持 2.x"),
                    "用 rsl convert 重新转换或降级到 2.x",
                ));
                return None;
            }
            s.clone()
        }
    };

    // ── 反序列化 ──
    let src: model::Source = match serde_json::from_value(v.clone()) {
        Ok(s) => s,
        Err(e) => {
            cx.err(
                Diagnostic::new(Code::ESchema, "$", format!("结构不符合 Schema：{}", e), "对照 schema/rsl.schema.json 检查形状"),
            );
            return None;
        }
    };

    // ── 必填字段 ──
    let id = match &src.id {
        Some(i) => i.clone(),
        None => { cx.err(Diagnostic::new(Code::ESchema, "id", "缺少必填字段 id", "唯一标识，建议域名")); return None; }
    };
    let name = match &src.name {
        Some(n) => n.clone(),
        None => { cx.err(Diagnostic::new(Code::ESchema, "name", "缺少必填字段 name", "显示名")); return None; }
    };
    let carrier = match &src.carrier {
        Some(t) if model::is_carrier(t) => t.clone(),
        Some(t) => {
            cx.err(Diagnostic::new(Code::EEnum, "type", format!("`{t}` 不是合法载体"), format!("合法值：{}", model::CARRIERS.join(" "))));
            return None;
        }
        None => { cx.err(Diagnostic::new(Code::ESchema, "type", "缺少必填字段 type", format!("合法值：{}", model::CARRIERS.join(" ")))); return None; }
    };
    let base_url = match &src.base_url {
        Some(b) if !b.is_empty() => b.clone(),
        Some(_) => { cx.err(Diagnostic::new(Code::ESchema, "baseUrl", "baseUrl 不能为空", "相对地址基准")); return None; }
        None => { cx.err(Diagnostic::new(Code::ESchema, "baseUrl", "缺少必填字段 baseUrl", "如 https://example.com")); return None; }
    };

    warn_unknown(&src.extra, "$", cx);

    // ── requires ──
    let req = src.requires.clone().unwrap_or_default();
    for f in &req.features {
        const KNOWN: &[&str] = &["js", "time", "random", "crypto", "font", "webview", "detect", "xpath", "jsonpath", "css", "captcha", "tls"];
        if !KNOWN.contains(&f.as_str()) {
            cx.err(Diagnostic::new(Code::EEnum, "requires.features", format!("`{f}` 不是合法 feature"), format!("合法值：{}", KNOWN.join(" "))));
        }
    }
    for a in &req.crypto {
        const ALGS: &[&str] = &["md5", "sha1", "sha256", "sha512", "hmac-sha1", "hmac-sha256", "aes-cbc", "aes-gcm", "des", "triple-des", "base64", "hex", "url-encode"];
        if !ALGS.contains(&a.as_str()) {
            cx.err(Diagnostic::new(Code::EEnum, "requires.crypto", format!("`{a}` 不是合法算法名"), format!("合法值：{}", ALGS.join(" "))));
        }
    }
    cx.features.extend(req.features.iter().cloned());
    cx.crypto.extend(req.crypto.iter().cloned());

    // ── lib 扫描 ──
    let libcfg = src.lib.clone().unwrap_or_default();
    match lib::scan_lib(libcfg.pure.as_deref(), libcfg.ctx.as_deref()) {
        Ok(s) => cx.libs = s,
        Err(d) => cx.err(d),
    }
    cx.lib_ctx_src = libcfg.ctx.clone();

    // ── vars ──
    if let Some(vars) = &src.vars {
        for (k, ve) in vars {
            if refs::BUILTIN_KEYS.contains(&k.as_str()) {
                cx.err(Diagnostic::new(
                    Code::ERef,
                    format!("vars.{k}"),
                    format!("vars 键 `{k}` 与内置上下文键冲突"),
                    format!("改名；内置键：{}", refs::BUILTIN_KEYS.join(" ")),
                ));
            }
            let (value, label, persist, secret, options) = match ve {
                model::VarEntry::Val(val) => (IrValue::from_json(val), None, false, false, vec![]),
                model::VarEntry::Cfg(c) => {
                    warn_unknown(&c.extra, &format!("vars.{k}"), cx);
                    (
                        IrValue::from_json(&c.value.clone().or_else(|| c.default.clone()).unwrap_or(serde_json::Value::Null)),
                        c.label.clone(),
                        c.persist.unwrap_or(false),
                        c.secret.unwrap_or(false),
                        c.options.iter().map(IrValue::from_json).collect(),
                    )
                }
            };
            if secret {
                if let Some(s) = value.as_str() {
                    cx.secrets.push(s.to_string());
                }
            }
            cx.vars.insert(k.clone(), VarIR { value, label, persist, secret, options });
        }
    }

    // ── cookie / login 存在性（token:/cookie: 命名空间检查依据）──
    cx.has_login = src.login.is_some();
    cx.has_cookie_block = src.cookie.is_some();

    // ── http ──
    let http = build_http(cx, src.http.as_ref(), &refs::global_scope(&cx.vars));
    let Some(http) = http else { return None; };

    // ── limits ──
    let limits = build_limits(cx, src.limits.as_ref());

    // ── merge 校验（key 字段查 search.out.map）──
    let merge_cfg = src.merge.clone().unwrap_or_default();
    // 裁决 D8：默认键（name/author）不校验存在性（运行期缺失回退空串）；
    // 非默认键必须存在于 search.out.map → E_REF（lint #18 由负例 fixture 的非默认键触发）
    let search_map_keys = src
        .stages
        .as_ref()
        .and_then(|s| s.get("search"))
        .and_then(|st| st.out.as_ref())
        .and_then(|o| o.map.as_ref())
        .map(|m| m.keys().cloned().collect::<BTreeSet<String>>());
    let key = merge_cfg.key.clone().unwrap_or_else(|| vec!["name".into(), "author".into()]);
    // D8：默认键（name/author）不校验存在性（运行期缺失回退空串）；非默认键必须存在
    let non_default_keys: Vec<String> = key
        .iter()
        .filter(|k| k.as_str() != "name" && k.as_str() != "author")
        .cloned()
        .collect();
    let normalize = merge_cfg.normalize.clone().unwrap_or_else(|| vec!["trim".into()]);
    let order = merge_cfg
        .order
        .clone()
        .unwrap_or_else(|| vec!["exact".into(), "contains".into(), "weight".into(), "sources".into()]);
    warn_unknown(&merge_cfg.extra, "merge", cx);
    for d in merge::check_merge(&non_default_keys, &normalize, &order, search_map_keys.as_ref()) {
        cx.err(d);
    }
    let merge_ir = MergeIR {
        key,
        normalize,
        cross_bucket: merge_cfg.cross_bucket.unwrap_or(true),
        order,
        weight: merge_cfg.weight.unwrap_or(1.0),
        min_score: merge_cfg.min_score.unwrap_or(0.0),
    };

    // ── 契约字段集合（引用图数据源）──
    let stages_cfg = src.stages.clone().unwrap_or_default();
    // 阶段键合法性 + 必需性
    for k in stages_cfg.keys() {
        if !matches!(k.as_str(), "search" | "explore" | "detail" | "toc" | "content") {
            cx.err(Diagnostic::new(Code::EEnum, format!("stages.{k}"), format!("`{k}` 不是合法阶段"), "合法阶段：search explore detail toc content"));
        }
    }
    for req_stage in ["detail", "toc", "content"] {
        if !stages_cfg.contains_key(req_stage) {
            cx.err(Diagnostic::new(Code::ESchema, "stages", format!("stages 缺少必需阶段 `{req_stage}`"), "detail/toc/content 必需"));
        }
    }
    if !stages_cfg.contains_key("search") && !stages_cfg.contains_key("explore") {
        cx.err(Diagnostic::new(Code::ESchema, "stages", "search 与 explore 至少要有其一", "补一个入口阶段"));
    }
    if cx.diags.iter().any(|d| !d.code.is_warning()) {
        return None;
    }

    let mut contract = refs::ContractFields::default();
    if let Some(st) = stages_cfg.get("search") {
        if let Some(o) = &st.out {
            if let Some(m) = &o.map {
                contract.list_map_keys.extend(m.keys().cloned());
            }
        }
    }
    if let Some(st) = stages_cfg.get("explore") {
        if let Some(o) = &st.out {
            if let Some(m) = &o.map {
                contract.list_map_keys.extend(m.keys().cloned());
            }
        }
    }
    if let Some(st) = stages_cfg.get("detail") {
        if let Some(o) = &st.out {
            if let Some(m) = &o.map {
                contract.detail_keys.extend(m.keys().cloned());
            }
            for k in o.extra.keys() {
                contract.detail_keys.insert(k.clone());
            }
        }
    }
    if let Some(st) = stages_cfg.get("toc") {
        if let Some(o) = &st.out {
            if let Some(m) = &o.map {
                contract.toc_map_keys.extend(m.keys().cloned());
            }
        }
    }
    cx.contract = contract.clone();

    // ── login ──
    let login = match &src.login {
        Some(l) => match build_login(cx, l) {
            Ok(x) => Some(x),
            Err(()) => return None,
        },
        None => None,
    };

    // ── cache / media ──
    let mut cache_ir = vec![];
    if let Some(cache) = &src.cache {
        for (name, ce) in cache {
            warn_unknown(&ce.extra, &format!("cache.{name}"), cx);
            let req = match &ce.request {
                Some(r) => match build_request(cx, r, &format!("cache.{name}.request"), &refs::global_scope(&cx.vars)) {
                    Some(r) => r,
                    None => return None,
                },
                None => { cx.err(Diagnostic::new(Code::ESchema, format!("cache.{name}.request"), "cache 条目缺少 request", "补 request + take")); return None; }
            };
            let take = match &ce.take {
                Some(t) => match compile_selector(cx, t, &format!("cache.{name}.take"), &refs::global_scope(&cx.vars), None, false) {
                    Some(t) => Some(Box::new(t)),
                    None => return None,
                },
                None => None,
            };
            let key_expr = match &ce.key {
                Some(k) => match build_param(cx, &serde_json::Value::String(k.clone()), &format!("cache.{name}.key"), &refs::global_scope(&cx.vars)) {
                    Some(p) => Some(p),
                    None => return None,
                },
                None => None,
            };
            cache_ir.push(CacheIR {
                name: name.clone(),
                req: Some(req),
                take,
                key: key_expr,
                ttl: ce.ttl.unwrap_or(3600),
                ttl_partial: ce.ttl_partial.unwrap_or(0),
            });
        }
    }
    let media = src.media.as_ref().map(|m| {
        warn_unknown(&m.extra, "media", cx);
        let mut headers = vec![];
        if let Some(h) = &m.headers {
            for (k, val) in h {
                match sugar::parse_template(val, &format!("media.headers.{k}")) {
                    Ok(t) => headers.push((k.clone(), t)),
                    Err(d) => cx.err(d),
                }
            }
        }
        if let Some(dec) = &m.decode {
            if dec.input.as_deref() != Some("bytes") || dec.out.as_deref() != Some("bytes") {
                cx.err(Diagnostic::new(Code::ESchema, "media.decode", "decode 的 in/out 必须都是 bytes", "这是字节级契约（S§4.6）"));
            }
            if let Some(f) = &dec.fn_ {
                let fname = f.strip_prefix("lib:").unwrap_or(f);
                if !cx.libs.exists(fname) {
                    cx.err(Diagnostic::new(Code::EScript, "media.decode.fn", format!("lib 函数 `{fname}` 不存在"), "在 lib.pure 里定义它"));
                }
            }
        }
        MediaIR {
            headers,
            decode_fn: m.decode.as_ref().and_then(|d| d.fn_.clone()).map(|f| f.trim_start_matches("lib:").to_string()),
            cache_ttl: m.cache.as_ref().and_then(|c| c.ttl).unwrap_or(0),
            max_bytes: m.max_bytes.unwrap_or(20 * 1024 * 1024),
        }
    });

    // ── 逐阶段构建 ──
    let mut stages = BTreeMap::new();
    for (k, st) in &stages_cfg {
        let path = format!("stages.{k}");
        match build_stage(cx, k, st, &path) {
            Some(s) => {
                stages.insert(k.clone(), s);
            }
            None => return None,
        }
    }
    let mut host_stages = BTreeMap::new();
    if let Some(hs) = &src.host_stages {
        for (k, st) in hs {
            let path = format!("hostStages.{k}");
            match build_stage(cx, k, st, &path) {
                Some(s) => {
                    host_stages.insert(k.clone(), s);
                }
                None => return None,
            }
        }
    }
    // reuseDetailHtml 需要 detail（lint 17）
    if stages_cfg.get("toc").and_then(|t| t.reuse_detail_html).unwrap_or(false)
        && !stages_cfg.contains_key("detail")
    {
        cx.err(Diagnostic::new(Code::ESchema, "stages.toc.reuseDetailHtml", "reuseDetailHtml 需要 detail 阶段", "补 detail 或删掉 reuseDetailHtml"));
    }

    // ── feature 声明检查 ──
    if cx.diags.iter().all(|d| d.code.is_warning()) {
        // 组装临时 IR 以收集 feature 使用
        let probe = CompiledSource {
            rsl_version: rsl.clone(),
            id: id.clone(),
            name: name.clone(),
            carrier: carrier.clone(),
            base_url: base_url.clone(),
            match_detail: src.match_.as_ref().and_then(|m| m.detail.clone()),
            lang: src.lang.clone(),
            version: src.version.clone(),
            requires: RequiresIR {
                engine: req.engine.clone(),
                features: req.features.clone(),
                crypto: req.crypto.clone(),
                selector_css: req.selector_level.as_ref().and_then(|s| s.css.clone()),
                selector_xpath: req.selector_level.as_ref().and_then(|s| s.xpath.clone()),
                selector_jsonpath: req.selector_level.as_ref().and_then(|s| s.jsonpath.clone()),
            },
            http: http.clone(),
            vars: cx.vars.clone(),
            cookie_initial: src.cookie.as_ref().and_then(|c| c.initial.clone()).map(|m| m.into_iter().collect()).unwrap_or_default(),
            cookie_domain: src.cookie.as_ref().and_then(|c| c.domain.clone()),
            cookie_persist: src.cookie.as_ref().and_then(|c| c.persist).unwrap_or(true),
            cache: cache_ir.clone(),
            media: media.clone(),
            login: login.clone(),
            merge: merge_ir.clone(),
            lib: LibIR {
                pure: libcfg.pure.clone().unwrap_or_default(),
                ctx: libcfg.ctx.clone().unwrap_or_default(),
                pure_fns: cx.libs.pure_fns.clone(),
                ctx_fns: cx.libs.ctx_fns.clone(),
            },
            stages: stages.clone(),
            host_stages: host_stages.clone(),
            limits: limits.clone(),
            x_fields: src.extra.iter().map(|(k, x)| (k.clone(), IrValue::from_json(x))).collect(),
        };
        let usage = feature::collect_usage(&probe);
        for d in feature::check_declared(&usage, &probe.requires) {
            cx.err(d);
        }
        // 逐章白名单（T1.9）：toc.out.map.*
        if let Some(toc) = stages.get("toc") {
            let lib_pure = cx.libs.pure_fns.clone();
            let mut cost_diags = vec![];
            for (k, sel) in &toc.out.map {
                let p = format!("stages.toc.out.map.{k}");
                cost_diags.extend(cost::check_per_item(sel, &p, &lib_pure));
            }
            for d in cost_diags {
                cx.err(d);
            }
        }
    }

    if cx.diags.iter().any(|d| !d.code.is_warning()) {
        return None;
    }

    Some(CompiledSource {
        rsl_version: rsl,
        id,
        name,
        carrier,
        base_url,
        match_detail: src.match_.as_ref().and_then(|m| m.detail.clone()),
        lang: src.lang.clone(),
        version: src.version.clone(),
        requires: RequiresIR {
            engine: req.engine.clone(),
            features: req.features.clone(),
            crypto: req.crypto.clone(),
            selector_css: req.selector_level.as_ref().and_then(|s| s.css.clone()),
            selector_xpath: req.selector_level.as_ref().and_then(|s| s.xpath.clone()),
            selector_jsonpath: req.selector_level.as_ref().and_then(|s| s.jsonpath.clone()),
        },
        http,
        vars: cx.vars.clone(),
        cookie_initial: src.cookie.as_ref().and_then(|c| c.initial.clone()).map(|m| m.into_iter().collect()).unwrap_or_default(),
        cookie_domain: src.cookie.as_ref().and_then(|c| c.domain.clone()),
        cookie_persist: src.cookie.as_ref().and_then(|c| c.persist).unwrap_or(true),
        cache: cache_ir,
        media,
        login,
        merge: merge_ir,
        lib: LibIR {
            pure: libcfg.pure.unwrap_or_default(),
            ctx: libcfg.ctx.unwrap_or_default(),
            pure_fns: cx.libs.pure_fns.clone(),
            ctx_fns: cx.libs.ctx_fns.clone(),
        },
        stages,
        host_stages,
        limits,
        x_fields: src.extra.iter().map(|(k, x)| (k.clone(), IrValue::from_json(x))).collect(),
    })
}

// ───────────────────────── http / limits ─────────────────────────

fn build_http(cx: &mut Cx, cfg: Option<&model::HttpCfg>, _scope: &refs::Scope) -> Option<HttpIR> {
    let mut headers = vec![];
    if let Some(c) = cfg {
        warn_unknown(&c.extra, "http", cx);
        for (k, val) in &c.headers {
            match sugar::parse_value_string(val, &format!("http.headers.{k}")) {
                Ok(t) => headers.push((k.clone(), t)),
                Err(d) => cx.err(d),
            }
        }
        if let Some(retry) = &c.retry {
            check_retry(cx, retry, "http.retry");
        }
        if let Some(rate) = &c.rate {
            warn_unknown(&rate.extra, "http.rate", cx);
            if rate.window_ms.is_none() && rate.rps.is_none() {
                cx.err(Diagnostic::new(Code::ESchema, "http.rate", "rate 需要 {windowMs,max} 或 rps", "两种写法二选一"));
            }
            if rate.max.is_none() && rate.rps.is_none() {
                cx.err(Diagnostic::new(Code::ESchema, "http.rate", "rate 缺少 max 或 rps", "如 {\"windowMs\":60000,\"max\":20}"));
            }
        }
        if let Some(cookies) = &c.cookies {
            if !matches!(cookies.as_str(), "none" | "session" | "persistent") {
                cx.err(Diagnostic::new(Code::EEnum, "http.cookies", format!("`{cookies}` 不是合法枚举值"), "合法值：none | session | persistent"));
            }
        }
        if let Some(tls) = &c.tls {
            if tls.fingerprint.is_some() && !cx.features.contains("tls") {
                // 加载期能力由宿主判定；此处只登记
            }
            warn_unknown(&tls.extra, "http.tls", cx);
        }
    }
    let c = cfg.cloned().unwrap_or_default();
    Some(HttpIR {
        headers,
        timeout: c.timeout.unwrap_or(20_000),
        retry: c.retry.map(|r| RetryIR {
            times: r.times.unwrap_or(0),
            backoff: r.backoff.unwrap_or_else(|| "none".into()),
            on: r.on,
        }).unwrap_or(RetryIR { times: 0, backoff: "none".into(), on: vec![] }),
        rate: c.rate.and_then(|r| {
            if let (Some(w), Some(m)) = (r.window_ms, r.max) {
                Some(RateIR { window_ms: w, max: m })
            } else if let Some(rps) = r.rps {
                let burst = r.burst.unwrap_or(1);
                Some(RateIR { window_ms: 1000, max: ((rps * burst as f64) as i64).max(1) })
            } else {
                None
            }
        }),
        redirect_max: c.redirect.and_then(|r| r.max).unwrap_or(5),
        cookies_mode: c.cookies.unwrap_or_else(|| "session".into()),
        proxy: c.proxy.filter(|p| p != "none"),
        dns: c.dns.unwrap_or_default(),
        tls_fingerprint: c.tls.and_then(|t| t.fingerprint),
    })
}

fn check_retry(cx: &mut Cx, r: &model::RetryCfg, path: &str) {
    warn_unknown(&r.extra, path, cx);
    if let Some(b) = &r.backoff {
        if !matches!(b.as_str(), "none" | "linear" | "exponential") {
            cx.err(Diagnostic::new(Code::EEnum, path, format!("backoff `{b}` 不是合法枚举值"), "合法值：none | linear | exponential"));
        }
    }
}

fn build_limits(cx: &mut Cx, cfg: Option<&model::Limits>) -> LimitsIR {
    let mut out = LimitsIR {
        max_bytes: 2 * 1024 * 1024,
        max_bytes_on: "reject".into(),
        max_nodes: 5000,
        max_nodes_on: "truncate".into(),
        timeout_ms: 20_000,
        max_redirects: 5,
        max_pages: 50,
        max_pages_on: "truncate".into(),
        max_parallel: 4,
        lib_steps: 1_000_000,
        script_timeout_ms: 3000,
        media_bytes: 20 * 1024 * 1024,
        login_check_ttl_ms: 300_000,
    };
    if let Some(l) = cfg {
        warn_unknown(&l.extra, "limits", cx);
        if let Some(b) = &l.max_bytes {
            out.max_bytes = b.value().unwrap_or(out.max_bytes);
            out.max_bytes_on = b.on().into();
            check_on(cx, b.on(), "limits.maxBytes");
        }
        if let Some(b) = &l.max_nodes {
            out.max_nodes = b.value().unwrap_or(out.max_nodes);
            out.max_nodes_on = b.on().into();
            check_on(cx, b.on(), "limits.maxNodes");
        }
        if let Some(b) = &l.max_pages {
            out.max_pages = b.value().unwrap_or(out.max_pages);
            out.max_pages_on = b.on().into();
            check_on(cx, b.on(), "limits.maxPages");
        }
        if let Some(v) = l.timeout_ms {
            out.timeout_ms = v;
        }
        if let Some(v) = l.max_redirects {
            out.max_redirects = v;
        }
        if let Some(v) = l.max_parallel {
            out.max_parallel = v;
        }
        if let Some(v) = l.lib_steps {
            out.lib_steps = v;
        }
        if let Some(v) = l.script_timeout_ms {
            out.script_timeout_ms = v;
        }
        if let Some(v) = l.media_bytes {
            out.media_bytes = v;
        }
        if let Some(v) = l.login_check_ttl_ms {
            out.login_check_ttl_ms = v;
        }
    }
    out
}

fn check_on(cx: &mut Cx, on: &str, path: &str) {
    if !matches!(on, "reject" | "truncate") {
        cx.err(Diagnostic::new(Code::EEnum, path, format!("`{on}` 不是合法枚举值"), "合法值：reject | truncate"));
    }
}

// ───────────────────────── 请求编译 ─────────────────────────

/// 参数值编译：expr: → Expr；选择器字符串 → Selector；带 ${} → Template；否则字面量。
fn build_param(cx: &mut Cx, raw: &serde_json::Value, path: &str, scope: &refs::Scope) -> Option<ParamExpr> {
    match raw {
        serde_json::Value::String(s) => Some(build_param_str(cx, s, path, scope)),
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {
            Some(ParamExpr::Raw(IrValue::from_json(raw)))
        }
        serde_json::Value::Null => Some(ParamExpr::Raw(IrValue::Null)),
        other => Some(ParamExpr::Raw(IrValue::from_json(other))),
    }
}

pub fn build_param_str(cx: &mut Cx, s: &str, path: &str, scope: &refs::Scope) -> ParamExpr {
    // expr: 整体表达式
    if let Some(e) = s.strip_prefix("expr:") {
        return match compile_expr(cx, e, path, scope, false) {
            Some(ex) => ParamExpr::Expr(ex),
            None => ParamExpr::Raw(IrValue::Null),
        };
    }
    // 含 ${} 模板
    if s.contains("${") {
        let segs = match sugar::parse_template(s, path) {
            Ok(t) => t,
            Err(d) => {
                cx.err(d);
                return ParamExpr::Raw(IrValue::Null);
            }
        };
        // 模板段里的变量也要查引用
        for seg in &segs {
            if let Seg::Tmpl { ns, expr } = seg {
                check_template_ns_ref(cx, ns, expr, path, scope);
            }
        }
        // 单段模板：收敛为对应 ParamExpr
        if let [Seg::Tmpl { ns, expr }] = &segs[..] {
            if ns == "expr" {
                return match compile_expr(cx, expr, path, scope, false) {
                    Some(ex) => ParamExpr::Expr(ex),
                    None => ParamExpr::Raw(IrValue::Null),
                };
            }
        }
        return ParamExpr::Template(segs);
    }
    // 选择器字符串（var:kw 等）
    if sugar::is_selector_string(s) {
        let sel = Selector::Str(s.to_string());
        return match compile_selector(cx, &sel, path, scope, None, false) {
            Some(ir) => ParamExpr::Selector(Box::new(ir)),
            None => ParamExpr::Raw(IrValue::Null),
        };
    }
    ParamExpr::Template(vec![Seg::Lit(s.to_string())])
}

/// 模板段的命名空间引用检查（var:/let:/session:/token:/cookie: 查引用图）。
fn check_template_ns_ref(cx: &mut Cx, ns: &str, expr: &str, path: &str, scope: &refs::Scope) {
    match ns {
        "expr" => {
            compile_expr(cx, expr, path, scope, false);
        }
        "var" | "" => {
            if !scope.has("var", expr) {
                cx.err(ref_undef(path, &format!("var:{expr}")));
            }
        }
        "let" => {
            if !scope.has("let", expr) {
                cx.err(ref_undef(path, &format!("let:{expr}")));
            }
        }
        "session" => {
            if !scope.session_ok {
                cx.err(ref_undef(path, &format!("session:{expr}")));
            }
        }
        _ => {}
    }
}

fn ref_undef(path: &str, show: &str) -> Diagnostic {
    Diagnostic::new(
        Code::ERef,
        path,
        format!("{show} 未定义或当前阶段不可见"),
        "对照引用图（COMPILER.md §3.2）：契约字段来自 out.map 声明；let 必须同阶段先声明",
    )
}

pub fn compile_expr(
    cx: &mut Cx,
    src: &str,
    path: &str,
    scope: &refs::Scope,
    per_item: bool,
) -> Option<Expr> {
    let mut escope = expr::Scope {
        paths: BTreeSet::new(),
        wildcards: scope.wildcards.clone(),
        lets: scope.lets.clone(),
        session_ok: scope.session_ok,
    };
    for p in &scope.paths {
        // 表达式只见标量根（baseUrl/url/response/charset/kw/page/scriptResult/filter.<k> 根级）
        escope.paths.insert(p.clone());
    }
    match expr::compile(src, path, &escope, &cx.decl(), expr::PerItem { enabled: per_item }) {
        Ok(e) => Some(e),
        Err(d) => {
            cx.err(d);
            None
        }
    }
}

fn build_request(cx: &mut Cx, cfg: &model::RequestCfg, path: &str, scope: &refs::Scope) -> Option<RequestIR> {
    warn_unknown(&cfg.extra, path, cx);
    // method / response / engine 枚举
    if let Some(m) = &cfg.method {
        if !matches!(m.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "HEAD") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.method"), format!("`{m}` 不是合法方法"), "合法值：GET POST PUT DELETE HEAD"));
        }
    }
    if let Some(r) = &cfg.response {
        if !matches!(r.as_str(), "html" | "json" | "text" | "bytes") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.response"), format!("`{r}` 不是合法枚举值"), "合法值：html | json | text | bytes"));
        }
    }
    if let Some(e) = &cfg.engine {
        if !matches!(e.as_str(), "http" | "webview" | "auto") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.engine"), format!("`{e}` 不是合法枚举值"), "合法值：http | webview | auto"));
        }
    }
    if let Some(retry) = &cfg.retry {
        check_retry(cx, retry, &format!("{path}.retry"));
    }
    let url = match &cfg.url {
        Some(u) => match sugar::parse_value_string(u, &format!("{path}.url")) {
            Ok(t) => {
                for seg in &t {
                    if let Seg::Tmpl { ns, expr } = seg {
                        check_template_ns_ref(cx, ns, expr, &format!("{path}.url"), scope);
                    }
                }
                Some(t)
            }
            Err(d) => {
                cx.err(d);
                None
            }
        },
        None => None,
    };
    let mut params = vec![];
    if let Some(ps) = &cfg.params {
        for (k, val) in ps {
            let p = match build_param(cx, val, &format!("{path}.params.{k}"), scope) {
                Some(p) => p,
                None => return None,
            };
            params.push((k.clone(), p));
        }
    }
    let mut headers = vec![];
    if let Some(hs) = &cfg.headers {
        for (k, val) in hs {
            match sugar::parse_value_string(val, &format!("{path}.headers.{k}")) {
                Ok(t) => {
                    for seg in &t {
                        if let Seg::Tmpl { ns, expr } = seg {
                            check_template_ns_ref(cx, ns, expr, &format!("{path}.headers.{k}"), scope);
                        }
                    }
                    headers.push((k.clone(), t));
                }
                Err(d) => cx.err(d),
            }
        }
    }
    let body = cfg.body.as_ref().map(|b| {
        warn_unknown(&b.extra, &format!("{path}.body"), cx);
        if let Some(bt) = &b.body_type {
            if !matches!(bt.as_str(), "form" | "json" | "raw" | "multipart") {
                cx.err(Diagnostic::new(Code::EEnum, format!("{path}.body.type"), format!("`{bt}` 不是合法枚举值"), "合法值：form | json | raw | multipart"));
            }
        }
        let mut data = BTreeMap::new();
        let mut raw_data = None;
        match &b.data {
            Some(serde_json::Value::Object(m)) => {
                for (k, val) in m {
                    let p = match build_param(cx, val, &format!("{path}.body.data.{k}"), scope) {
                        Some(p) => p,
                        None => ParamExpr::Raw(IrValue::Null),
                    };
                    data.insert(k.clone(), p);
                }
            }
            Some(other) if other.is_string() => {
                if let Some(s) = other.as_str() {
                    raw_data = Some(build_param_str(cx, s, &format!("{path}.body.data"), scope));
                }
            }
            Some(other) => {
                raw_data = Some(ParamExpr::Raw(IrValue::from_json(other)));
            }
            None => {}
        }
        BodyIR {
            kind: b.body_type.clone().unwrap_or_else(|| "form".into()),
            data,
            raw: raw_data,
        }
    });
    Some(RequestIR {
        url,
        method: cfg.method.clone().unwrap_or_else(|| "GET".into()),
        params,
        body,
        headers,
        charset: cfg.charset.clone(),
        encode_params: cfg.encode.as_ref().and_then(|e| e.params.clone()),
        encode_body: cfg.encode.as_ref().and_then(|e| e.body.clone()),
        response: match cfg.response.as_deref() {
            Some("json") => RespKind::Json,
            Some("text") => RespKind::Text,
            Some("bytes") => RespKind::Bytes,
            _ => RespKind::Html,
        },
        engine: cfg.engine.clone().unwrap_or_else(|| "http".into()),
        web_js: cfg.web_js.clone(),
        sniff: cfg.sniff.clone(),
        auto_upgrade_if: match &cfg.auto_upgrade_if {
            Some(s) => compile_selector(cx, s, &format!("{path}.autoUpgradeIf"), scope, None, false)
                .map(Box::new),
            None => None,
        },
        skip: cfg.skip.unwrap_or(false),
        retry: cfg.retry.as_ref().map(|r| RetryIR {
            times: r.times.unwrap_or(0),
            backoff: r.backoff.clone().unwrap_or_else(|| "none".into()),
            on: r.on.clone(),
        }),
        timeout: cfg.timeout,
    })
}

// ───────────────────────── 阶段编译 ─────────────────────────

fn build_stage(cx: &mut Cx, name: &str, cfg: &model::StageCfg, path: &str) -> Option<StageIR> {
    warn_unknown(&cfg.extra, path, cx);
    let has_script = cfg.script.is_some();
    // 阶段作用域（let 渐进并入）
    let mut stage_ir = StageIR {
        name: name.to_string(),
        lets: vec![],
        request: None,
        parse: None,
        out: OutIR::default(),
        paginate: None,
        script: ScriptHooksIR::default(),
        on_error: None,
        timeout_ms: cfg.timeout_ms,
        requires_login: cfg.requires_login.unwrap_or(false),
        order: None,
        dedupe: None,
        reuse_detail_html: cfg.reuse_detail_html.unwrap_or(false),
        direct: cfg.direct.unwrap_or(false),
        filters: vec![],
        groups: vec![],
        groups_from: None,
    };

    // onError
    if let Some(oe) = &cfg.on_error {
        if oe != "abort" && oe != "skip" && !oe.starts_with("fallbackTo:") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.onError"), format!("`{oe}` 不是合法枚举值"), "合法值：abort | skip | fallbackTo:<stage>"));
        }
        stage_ir.on_error = Some(oe.clone());
    }
    // order / dedupe（toc 专属）
    if let Some(o) = &cfg.order {
        if !matches!(o.as_str(), "auto" | "asc" | "desc") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.order"), format!("`{o}` 不是合法枚举值"), "合法值：auto | asc | desc"));
        }
        stage_ir.order = Some(match o.as_str() {
            "asc" => TocOrder::Asc,
            "desc" => TocOrder::Desc,
            _ => TocOrder::Auto,
        });
    }
    if let Some(d) = &cfg.dedupe {
        if !matches!(d.as_str(), "url" | "title" | "none") {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.dedupe"), format!("`{d}` 不是合法枚举值"), "合法值：url | title | none"));
        }
        stage_ir.dedupe = Some(match d.as_str() {
            "title" => Dedupe::Title,
            "none" => Dedupe::None,
            _ => Dedupe::Url,
        });
    }

    // let（有序，后项可见前项）。作用域信息直接取自 cfg，引用图与 IR 构建解耦
    let filter_keys: Vec<String> = cfg
        .filters
        .as_ref()
        .map(|fs| fs.iter().filter_map(|f| f.key.clone()).collect())
        .unwrap_or_default();
    let has_groups = cfg.groups.as_ref().map(|g| !g.is_empty()).unwrap_or(false)
        || cfg.groups_from.is_some();
    let is_enumerate = cfg
        .paginate
        .as_ref()
        .and_then(|p| p.kind.as_deref())
        .map(|k| k == "enumerate")
        .unwrap_or(false);
    let scope_info = refs::StageScopeInfo {
        name,
        filter_keys: &filter_keys,
        has_paginate: cfg.paginate.is_some(),
        is_enumerate,
        has_groups,
        has_script,
    };
    let mut scope = refs::stage_scope(&scope_info, &cx.contract);
    // 并入 vars
    for k in cx.vars.keys() {
        scope.paths.insert(k.clone());
    }
    for k in ["baseUrl", "url", "response", "charset"] {
        scope.paths.insert(k.to_string());
    }
    if let Some(lets) = &cfg.let_ {
        let mut declared: BTreeSet<String> = BTreeSet::new();
        // 循环引用检查：引用了比自己后声明的 let → E_REF
        let names: Vec<&String> = lets.0.iter().map(|(k, _)| k).collect();
        for (idx, (k, le)) in lets.0.iter().enumerate() {
            if let model::LetEntry::Str(s) = le {
                if s.starts_with("expr:") {
                    for other in names.iter().skip(idx + 1) {
                        if s.contains(&format!("let:{other}")) {
                            cx.err(ref_undef(&format!("{path}.let.{k}"), &format!("let:{other}（let 按声明顺序求值，后项不可引用）")));
                        }
                    }
                }
            }
        }
        for (k, le) in &lets.0 {
            let lpath = format!("{path}.let.{k}");
            let ir = match le {
                model::LetEntry::Str(s) => {
                    if let Some(e) = s.strip_prefix("expr:") {
                        match compile_expr(cx, e, &lpath, &scope, false) {
                            Some(ex) => LetIR::Expr(ex),
                            None => return None,
                        }
                    } else if sugar::is_selector_string(s) {
                        if let Some(f) = s.strip_prefix("lib:") {
                            check_lib_call(cx, f, &lpath);
                            LetIR::Lib(f.to_string())
                        } else {
                            let sel = Selector::Str(s.clone());
                            match compile_selector(cx, &sel, &lpath, &scope, None, false) {
                                Some(ir) => LetIR::Selector(Box::new(ir)),
                                None => return None,
                            }
                        }
                    } else {
                        cx.err(Diagnostic::new(Code::ESchema, &lpath, format!("let 值 `{s}` 不是 expr: / lib: / 选择器"), "用 expr:… 或选择器字符串"));
                        return None;
                    }
                }
                model::LetEntry::Request(lr) => {
                    warn_unknown(&lr.extra, &lpath, cx);
                    let req = match &lr.request {
                        Some(r) => build_request(cx, r, &format!("{lpath}.request"), &scope)?,
                        None => {
                            cx.err(Diagnostic::new(Code::ESchema, &lpath, "let 请求项缺少 request", "{\"request\": …, \"take\": …}"));
                            return None;
                        }
                    };
                    let take = match &lr.take {
                        Some(t) => compile_selector(cx, t, &format!("{lpath}.take"), &scope, None, false).map(Box::new),
                        None => None,
                    };
                    LetIR::Request { req, take }
                }
                model::LetEntry::Parallel(lp) => {
                    warn_unknown(&lp.extra, &lpath, cx);
                    let mut items = vec![];
                    for (i, it) in lp.parallel.iter().enumerate() {
                        match &it.request {
                            Some(r) => {
                                let req = build_request(cx, r, &format!("{lpath}.parallel[{i}].request"), &scope)?;
                                items.push(req);
                            }
                            None => {
                                cx.err(Diagnostic::new(Code::ESchema, &format!("{lpath}.parallel[{i}]"), "parallel 项缺少 request", "每项写 {\"request\": …}"));
                                return None;
                            }
                        }
                    }
                    LetIR::Parallel {
                        items,
                        limit: lp.limit.unwrap_or(4).max(1) as usize,
                    }
                }
            };
            declared.insert(k.clone());
            scope.lets = declared.clone();
            stage_ir.lets.push((k.clone(), ir));
        }
    }

    // request
    if let Some(r) = &cfg.request {
        stage_ir.request = Some(build_request(cx, r, &format!("{path}.request"), &scope)?);
    }
    // parse
    if let Some(p) = &cfg.parse {
        stage_ir.parse = compile_selector(cx, p, &format!("{path}.parse"), &scope, None, false).map(Box::new);
    }

    // out
    if let Some(o) = &cfg.out {
        stage_ir.out = build_out(cx, name, o, path, &scope)?;
    } else {
        cx.err(Diagnostic::new(Code::ESchema, format!("{path}.out"), "阶段缺少 out 契约", "每个阶段都要声明 out"));
        return None;
    }

    // paginate
    if let Some(pg) = &cfg.paginate {
        stage_ir.paginate = Some(build_paginate(cx, pg, path, &scope)?);
    }

    // script 钩子
    if let Some(sc) = &cfg.script {
        warn_unknown(&sc.extra, &format!("{path}.script"), cx);
        stage_ir.script = build_hooks(cx, sc, path)?;
    }

    // filters / groups（explore）
    if let Some(fs) = &cfg.filters {
        for f in fs {
            warn_unknown(&f.extra, &format!("{path}.filters"), cx);
            let key = match &f.key {
                Some(k) => k.clone(),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.filters"), "filter 缺少 key", "每个 filter 都要 key"));
                    return None;
                }
            };
            stage_ir.filters.push(FilterIR {
                key,
                label: f.label.clone(),
                default: f.default.clone().map(|x| IrValue::from_json(&x)),
                persist: f.persist.unwrap_or(false),
                options: f.options.iter().map(|o| (IrValue::from_json(&o.value.clone().unwrap_or(serde_json::Value::Null)), o.label.clone())).collect(),
            });
        }
    }
    if let Some(gs) = &cfg.groups {
        fn conv_group(cx: &mut Cx, g: &model::GroupCfg, path: &str) -> Option<GroupIR> {
            warn_unknown(&g.extra, path, cx);
            let title = g.title.clone()?;
            let groups = if let Some(sub) = &g.groups {
                let mut v = vec![];
                for (i, sg) in sub.iter().enumerate() {
                    v.push(conv_group(cx, sg, &format!("{path}.groups[{i}]"))?);
                }
                v
            } else {
                vec![]
            };
            Some(GroupIR {
                title,
                url: g.url.clone(),
                params: g.params.clone().map(|m| m.into_iter().map(|(k, x)| (k, IrValue::from_json(&x))).collect()).unwrap_or_default(),
                when: g.when.clone().map(|m| m.into_iter().map(|(k, x)| (k, IrValue::from_json(&x))).collect()).unwrap_or_default(),
                groups,
            })
        }
        for (i, g) in gs.iter().enumerate() {
            match conv_group(cx, g, &format!("{path}.groups[{i}]")) {
                Some(gir) => stage_ir.groups.push(gir),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.groups[{i}]"), "group 缺少 title", "每个 group 都要 title"));
                    return None;
                }
            }
        }
    }
    if let Some(gf) = &cfg.groups_from {
        stage_ir.groups_from = compile_selector(cx, gf, &format!("{path}.groupsFrom"), &scope, None, false).map(Box::new);
    }

    // direct × 非 skip request（lint 8）
    if stage_ir.direct {
        if let Some(r) = &cfg.request {
            let has_non_skip = r.url.is_some()
                || r.method.is_some()
                || r.params.is_some()
                || r.body.is_some()
                || r.headers.is_some()
                || r.response.is_some()
                || r.charset.is_some();
            if has_non_skip {
                cx.err(Diagnostic::new(
                    Code::ESchema,
                    format!("{path}.request"),
                    "direct: true 与非 skip 的 request 互斥",
                    "direct 时 request 整体删掉或只留 {\"skip\": true}",
                ));
                return None;
            }
        }
    }

    // 步骤图之外：paginate.cursor into / enumerate into 的路径合法性在 build_paginate 内检查
    Some(stage_ir)
}

fn build_paginate(cx: &mut Cx, pg: &model::PaginateCfg, path: &str, scope: &refs::Scope) -> Option<PaginateIR> {
    warn_unknown(&pg.extra, path, cx);
    let kind = match &pg.kind {
        Some(k) => k.as_str(),
        None => {
            cx.err(Diagnostic::new(Code::ESchema, format!("{path}.kind"), "paginate 缺少 kind", "四选一：page | next | cursor | enumerate"));
            return None;
        }
    };
    // pageVar × enumerate 互斥（lint 7）
    if pg.page_var.is_some() && pg.enumerate.is_some() {
        cx.err(Diagnostic::new(
            Code::ESchema,
            format!("{path}"),
            "paginate.pageVar 与 paginate.enumerate 互斥",
            "保留 pageVar 删掉 enumerate，或改用 kind:\"enumerate\" 并去掉 pageVar",
        ));
        return None;
    }
    let kind_ir = match kind {
        "page" => {
            PaginateKind::Page {
                page_var: pg.page_var.clone().unwrap_or_else(|| "page".into()),
            }
        }
        "next" => {
            if pg.next.is_none() {
                cx.err(Diagnostic::new(Code::ESchema, path, "kind=next 缺少 next 选择器", "补 next 或改 kind"));
                return None;
            }
            let mut sels = vec![];
            for (i, s) in pg.next.as_ref().map(|n| &n.0).into_iter().flatten().enumerate() {
                match compile_selector(cx, s, &format!("{path}.next[{i}]"), scope, None, false) {
                    Some(ir) => sels.push(ir),
                    None => return None,
                }
            }
            PaginateKind::Next { sels }
        }
        "cursor" => {
            let cur = match &pg.cursor {
                Some(c) => c,
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, path, "kind=cursor 缺少 cursor 块", "cursor: {take, into}"));
                    return None;
                }
            };
            let take = match &cur.take {
                Some(t) => match compile_selector(cx, t, &format!("{path}.cursor.take"), scope, None, false) {
                    Some(ir) => ir,
                    None => return None,
                },
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.cursor.take"), "cursor 缺少 take 选择器", "从响应取游标，如 json:$.data.next"));
                    return None;
                }
            };
            let into = match &cur.into {
                Some(i) => i.clone(),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.cursor.into"), "cursor 缺少 into", "游标注入位置，如 params.offset"));
                    return None;
                }
            };
            check_into_path(cx, &into, &format!("{path}.cursor.into"));
            PaginateKind::Cursor { take: Box::new(take), into }
        }
        "enumerate" => {
            let spec = match &pg.enumerate {
                Some(s) => s,
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, path, "kind=enumerate 缺少 enumerate 值", "字面数组或 {from,to,step}"));
                    return None;
                }
            };
            let values = match spec {
                model::EnumerateSpec::List(v) => v.clone(),
                model::EnumerateSpec::Range(r) => {
                    let (from, to) = match (r.from, r.to) {
                        (Some(f), Some(t)) => (f, t),
                        _ => {
                            cx.err(Diagnostic::new(Code::ESchema, path, "enumerate 区间缺 from/to", "如 {\"from\":1,\"to\":5}"));
                            return None;
                        }
                    };
                    let step = r.step.unwrap_or(1);
                    if step <= 0 {
                        cx.err(Diagnostic::new(Code::ESchema, path, format!("enumerate step={step} 必须 > 0"), "不做自动纠正（S§4.7.1）"));
                        return None;
                    }
                    if from > to {
                        cx.err(Diagnostic::new(Code::ESchema, path, format!("enumerate from={from} > to={to}"), "from 必须 ≤ to"));
                        return None;
                    }
                    (from..=to).step_by(step as usize).collect()
                }
            };
            let into = match &pg.into {
                Some(i) => i.clone(),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.into"), "enumerate 缺少 into", "每个值注入哪里，如 params.pn"));
                    return None;
                }
            };
            check_into_path(cx, &into, &format!("{path}.into"));
            PaginateKind::Enumerate {
                values,
                join_with: pg.join_with.clone(),
                into,
            }
        }
        other => {
            cx.err(Diagnostic::new(Code::EEnum, format!("{path}.kind"), format!("`{other}` 不是合法分页方式"), "合法值：page | next | cursor | enumerate"));
            return None;
        }
    };
    if let Some(empty) = &pg.stop_if.as_ref().and_then(|s| s.empty.as_ref()) {
        if !empty.starts_with("out.") {
            cx.err(Diagnostic::new(Code::ESchema, format!("{path}.stopIf.empty"), "stopIf.empty 是结果路径（out.*）", "如 \"out.list\""));
        }
    }
    if let Some(s) = &pg.stop_if {
        warn_unknown(&s.extra, &format!("{path}.stopIf"), cx);
    }
    Some(PaginateIR {
        kind: kind_ir,
        carry: pg.carry.clone(),
        max: pg.max,
        stop_empty_path: pg.stop_if.as_ref().and_then(|s| s.empty.clone()),
        stop_no_new: pg.stop_if.as_ref().and_then(|s| s.no_new).unwrap_or(false),
    })
}

fn check_into_path(cx: &mut Cx, into: &str, path: &str) {
    if !(into.starts_with("params.") || into.starts_with("body.data.") || into.starts_with("url.")) {
        cx.err(Diagnostic::new(
            Code::ERef,
            path,
            format!("into 路径 `{into}` 不合法"),
            "注入位置只能是 params.* / body.data.* / url.*",
        ));
    }
}

fn build_hooks(cx: &mut Cx, sc: &model::ScriptCfg, path: &str) -> Option<ScriptHooksIR> {
    let mut hooks = ScriptHooksIR::default();
    let self_ctx_src: Option<String> = cx.lib_ctx_src.clone();
    let build_one = |cx: &mut Cx, r: &model::ScriptRef, p: &str| -> Option<HookIR> {
        let fname = match r.fn_name() {
            Some(f) => f.trim_start_matches("lib:").to_string(),
            None => {
                cx.err(Diagnostic::new(Code::ESchema, p, "script 钩子缺少 fn", "写 lib:fnName 或 {\"fn\": \"lib:fnName\"}"));
                return None;
            }
        };
        let tier_ctx = r.ctx_tier();
        check_lib_call(cx, &fname, p);
        if tier_ctx && !cx.libs.is_ctx(&fname) {
            cx.err(Diagnostic::new(Code::EScript, p, format!("`{fname}` 不在 lib.ctx 档"), "{\"ctx\":true} 只能调用 lib.ctx 函数"));
        }
        if !tier_ctx && !cx.libs.is_pure(&fname) {
            cx.err(Diagnostic::new(Code::EScript, p, format!("`{fname}` 不在 lib.pure 档"), "不写 ctx:true 时调用 lib.pure 函数"));
        }
        Some(HookIR { fn_name: fname, ctx_tier: tier_ctx })
    };
    if let Some(b) = &sc.before {
        hooks.before = Some(build_one(cx, b, &format!("{path}.script.before"))?);
    }
    if let Some(o) = &sc.on_response {
        let hook = build_one(cx, o, &format!("{path}.script.onResponse"))?;
        // onResponse 禁调 ctx.http（lint #16）：轻量扫描函数体（含可到达同档函数）
        if hook.ctx_tier {
            if let Some(src) = &self_ctx_src {
                if lib::scan_ctx_http(src, &hook.fn_name, 0) {
                    cx.err(Diagnostic::new(
                        Code::EScript,
                        &format!("{path}.script.onResponse"),
                        format!("onResponse 函数 `{}` 调用了 ctx.http（会形成 响应→请求→响应 递归）", hook.fn_name),
                        "onResponse 只做响应改写；取数用 let 或 script.before",
                    ));
                    return None;
                }
            }
        }
        hooks.on_response = Some(hook);
    }
    if let Some(a) = &sc.after {
        hooks.after = Some(build_one(cx, a, &format!("{path}.script.after"))?);
    }
    Some(hooks)
}

fn check_lib_call(cx: &mut Cx, fname: &str, path: &str) {
    if !cx.libs.exists(fname) {
        cx.err(Diagnostic::new(
            Code::EScript,
            path,
            format!("lib 函数 `{fname}` 不存在"),
            "在 lib.pure / lib.ctx 里定义；写法 function NAME( … ）",
        ));
    }
}

fn build_out(cx: &mut Cx, stage_name: &str, o: &model::OutCfg, path: &str, scope: &refs::Scope) -> Option<OutIR> {
    warn_unknown(&o.extra, path, cx);
    let mut out = OutIR::default();
    out.empty_ok = o.empty_ok.unwrap_or(false);
    if let Some(v) = &o.verify {
        warn_unknown(&v.extra, &format!("{path}.verify"), cx);
        let contains = match &v.contains {
            Some(c) => match sugar::parse_value_string(c, &format!("{path}.verify.contains")) {
                Ok(t) => {
                    for seg in &t {
                        if let Seg::Tmpl { ns, expr } = seg {
                            check_template_ns_ref(cx, ns, expr, &format!("{path}.verify.contains"), scope);
                        }
                    }
                    Some(t)
                }
                Err(d) => {
                    cx.err(d);
                    None
                }
            },
            None => None,
        };
        out.verify = Some(VerifyIR { contains, min_items: v.min_items });
    }
    let is_list_stage = matches!(stage_name, "search" | "explore" | "toc") || path.starts_with("hostStages");
    // list
    if let Some(l) = &o.list {
        out.list = compile_selector(cx, l, &format!("{path}.list"), scope, None, false).map(Box::new);
    } else if is_list_stage {
        cx.err(Diagnostic::new(Code::ESchema, format!("{path}.list"), "列表型契约缺少 list", "search/explore/toc 必须有 out.list"));
        return None;
    }
    // detail 契约保留字段（S§6.4）编入 out.map 统一求值
    let detail_slots: [(&Option<Selector>, &str); 11] = [
        (&o.name, "name"),
        (&o.author, "author"),
        (&o.intro, "intro"),
        (&o.kind, "kind"),
        (&o.cover, "cover"),
        (&o.word_count, "wordCount"),
        (&o.last_chapter, "lastChapter"),
        (&o.update_time, "updateTime"),
        (&o.toc_url, "tocUrl"),
        (&o.can_rename, "canRename"),
        (&o.downloads, "downloads"),
    ];
    let mut detail_map: Vec<(String, Selector)> = vec![];
    for (sel, key) in detail_slots {
        if let Some(x) = sel {
            detail_map.push((key.to_string(), x.clone()));
        }
    }
    // map
    let per_item = stage_name == "toc";
    let map_effective = o.map.clone().or_else(|| {
        // 无 map 但有 detail 保留字段 → 用保留字段当 map（S§6.4）
        if detail_map.is_empty() {
            None
        } else {
            Some(detail_map.iter().cloned().collect::<BTreeMap<String, Selector>>())
        }
    });
    if let Some(m) = &map_effective {
        // search 的必填字段
        if stage_name == "search" {
            for req_key in ["name", "url"] {
                if !m.contains_key(req_key) {
                    cx.err(Diagnostic::new(Code::ESchema, format!("{path}.map"), format!("search.out.map 缺少必填字段 `{req_key}`"), "name/url 是搜索契约必填"));
                }
            }
        }
        if stage_name == "toc" && !m.contains_key("url") {
            cx.err(Diagnostic::new(Code::ESchema, format!("{path}.map"), "toc.out.map 缺少必填字段 `url`", "url 是正文入口"));
        }
        let merged: BTreeMap<String, Selector> = {
            let mut mm = m.clone();
            for (k, v) in detail_map {
                mm.entry(k).or_insert(v);
            }
            mm
        };
        for (k, sel) in &merged {
            let mscope = refs::map_scope(scope);
            let sel_path = format!("{path}.map.{k}");
            match compile_selector(cx, sel, &sel_path, &mscope, Some(k), per_item) {
                Some(ir) => out.map.push((k.clone(), ir)),
                None => return None,
            }
        }
    } else if is_list_stage {
        cx.err(Diagnostic::new(Code::ESchema, format!("{path}.map"), "列表型契约缺少 map", "search/explore/toc 必须有 out.map"));
        return None;
    }
    // 六载体字段
    let slots: [(&Option<Selector>, &mut Option<Box<SelectorIR>>, &str); 8] = [
        (&o.title, &mut out.title, "title"),
        (&o.content, &mut out.content, "content"),
        (&o.images, &mut out.images, "images"),
        (&o.audio, &mut out.audio, "audio"),
        (&o.video, &mut out.video, "video"),
        (&o.files, &mut out.files, "files"),
        (&o.next_page, &mut out.next_page, "nextPage"),
        (&o.pay_action, &mut out.pay_action, "payAction"),
    ];
    for (field, slot, slot_name) in slots {
        if let Some(sel) = field {
            let p = format!("{path}.{slot_name}");
            let ir = compile_selector(cx, sel, &p, scope, Some(slot_name), false)?;
            *slot = Some(Box::new(ir));
        }
    }
    if let Some(f) = &o.feed {
        warn_unknown(&f.extra, &format!("{path}.feed"), cx);
        let items = match &f.items {
            Some(i) => compile_selector(cx, i, &format!("{path}.feed.items"), scope, None, false)?,
            None => {
                cx.err(Diagnostic::new(Code::ESchema, format!("{path}.feed.items"), "feed 缺少 items", "rss 契约要有 feed.items"));
                return None;
            }
        };
        let mut map = vec![];
        if let Some(m) = &f.map {
            let mscope = refs::map_scope(scope);
            for (k, sel) in m {
                let ir = compile_selector(cx, sel, &format!("{path}.feed.map.{k}"), &mscope, Some(k), true)?;
                map.push((k.clone(), ir));
            }
        }
        out.feed = Some(FeedIR { items: Box::new(items), map });
    }
    if let Some(tracks) = &o.tracks {
        for (i, t) in tracks.iter().enumerate() {
            warn_unknown(&t.extra, &format!("{path}.tracks[{i}]"), cx);
            let kind = t.kind.clone().unwrap_or_default();
            let format = t.format.clone().unwrap_or_default();
            if !matches!(kind.as_str(), "lyric" | "danmaku" | "subtitle") {
                cx.err(Diagnostic::new(Code::EEnum, format!("{path}.tracks[{i}].kind"), format!("`{kind}` 不是合法轨道类型"), "合法值：lyric | danmaku | subtitle"));
            }
            if !matches!(format.as_str(), "lrc" | "bili-xml" | "ass" | "vtt" | "text") {
                cx.err(Diagnostic::new(Code::EEnum, format!("{path}.tracks[{i}].format"), format!("`{format}` 不是合法轨道格式"), "合法值：lrc | bili-xml | ass | vtt | text"));
            }
            if t.url.is_none() && t.text.is_none() {
                cx.err(Diagnostic::new(Code::ESchema, format!("{path}.tracks[{i}]"), "track 的 url 与 text 二选一", "给 url 引擎拉取；给 text 直接用"));
            }
            let url = match &t.url {
                Some(u) => compile_selector(cx, u, &format!("{path}.tracks[{i}].url"), scope, None, false).map(Box::new),
                None => None,
            };
            let text = match &t.text {
                Some(x) => compile_selector(cx, x, &format!("{path}.tracks[{i}].text"), scope, None, false).map(Box::new),
                None => None,
            };
            out.tracks.push(TrackIR { kind, format, url, text });
        }
    }
    Some(out)
}

// ───────────────────────── 选择器编译 ─────────────────────────

pub fn compile_selector(
    cx: &mut Cx,
    sel: &Selector,
    path: &str,
    scope: &refs::Scope,
    out_key: Option<&str>,
    per_item: bool,
) -> Option<SelectorIR> {
    let (obj, raw_text) = match sel {
        Selector::Str(s) => (None, s.clone()),
        Selector::Obj(o) => (Some(o), o.base.clone().unwrap_or_default()),
    };
    if let Some(o) = obj {
        warn_unknown(&o.extra, path, cx);
    }

    // 基座
    let base = if raw_text.is_empty() && obj.is_some() {
        BaseSel::None
    } else {
        parse_base(cx, &raw_text, path, scope, obj.and_then(|o| o.flags.as_deref()).unwrap_or(""))?
    };

    let flags = obj.and_then(|o| o.flags.clone()).unwrap_or_default();

    // attr
    let attr = obj.and_then(|o| o.attr.clone());
    if let Some(a) = &attr {
        if a.is_empty() {
            cx.err(Diagnostic::new(Code::ESelector, path, "attr 属性名为空", "写 attr:href 或 {\"attr\":\"href\"}").rule(&raw_text));
            return None;
        }
    }
    if let BaseSel::Json(_) = base {
        if attr.is_some() {
            cx.err(Diagnostic::new(
                Code::EType,
                path,
                "选择器的 attr 用在 JSON 作用域",
                "JSON 用 json: 路径取字段；attr 只作用于 HTML/XML 节点",
            )
            .rule(&raw_text));
            return None;
        }
    }

    // 互斥与形状（T1.4 / lint 4,5）
    let range = obj.and_then(|o| o.range.clone());
    let pick = obj.and_then(|o| o.pick.clone());
    if range.is_some() && pick.is_some() {
        cx.err(Diagnostic::new(
            Code::ESchema,
            path,
            "range 与 pick 互斥",
            "二选一：数组切片用 range，按下标取值用 pick",
        )
        .rule(&raw_text));
        return None;
    }
    let blocks = obj.and_then(|o| o.blocks.clone());
    let paragraphs = obj.and_then(|o| o.paragraphs.clone());
    if blocks.is_some() && paragraphs.is_some() {
        cx.err(Diagnostic::new(
            Code::ESchema,
            path,
            "blocks 与 paragraphs 互斥",
            "二选一：图文混排用 blocks，纯文本段落用 paragraphs",
        )
        .rule(&raw_text));
        return None;
    }
    if let Some(b) = &blocks {
        if let Some(mode) = &b.mode {
            if !matches!(mode.as_str(), "flow" | "paragraph") {
                cx.err(Diagnostic::new(Code::EEnum, path, format!("blocks.mode `{mode}` 不是合法枚举值"), "合法值：flow | paragraph"));
            }
        }
    }
    if let Some(p) = &paragraphs {
        if let Some(sp) = &p.split {
            if !matches!(sp.as_str(), "br" | "block" | "line" | "auto") {
                cx.err(Diagnostic::new(Code::EEnum, path, format!("paragraphs.split `{sp}` 不是合法枚举值"), "合法值：br | block | line | auto"));
            }
        }
    }
    // typeMap 只允许出现在契约保留字段 type 上（T1.4）
    if let Some(o) = obj {
        if o.type_map.is_some() && out_key != Some("type") {
            cx.err(Diagnostic::new(
                Code::ESchema,
                path,
                "typeMap 只允许出现在契约保留字段 type 上",
                "把该规则挪到 out.map.type 并用 typeMap 映射载体",
            )
            .rule(&raw_text));
            return None;
        }
        if o.type_map.is_some() && !matches!(base, BaseSel::Json(_) | BaseSel::Var(_) | BaseSel::Let(_) | BaseSel::Session(_) | BaseSel::Re { .. } | BaseSel::Expr(_) | BaseSel::Const(_) | BaseSel::Attr(_) | BaseSel::None) {
            cx.err(Diagnostic::new(Code::EType, path, "typeMap 的取值源需要是文本/JSON 值", "先取字符串再映射").rule(&raw_text));
        }
    }

    // range 解析
    let range_ir = match &range {
        Some(r) => match parse_range(r) {
            Ok(t) => Some(t),
            Err(msg) => {
                cx.err(Diagnostic::new(Code::ESchema, path, format!("range `{r}` 不合法：{msg}"), "格式 start:end[:step]，语义同 Python 切片")
                    .rule_owned(&raw_text));
                return None;
            }
        },
        None => None,
    };

    // where
    let mut where_ir = None;
    if let Some(o) = obj {
        if let Some(w) = &o.where_ {
            match build_where(cx, w, path, scope, per_item) {
                Some(x) => where_ir = Some(x),
                None => return None,
            }
        }
    }

    // 条件字段（eq/ne/gt/lt/contains）
    let mut cond = None;
    if let Some(o) = obj {
        let c = if let Some(v) = &o.eq {
            Some(Cond { op: CondOp::Eq, val: IrValue::from_json(v) })
        } else if let Some(v) = &o.ne {
            Some(Cond { op: CondOp::Ne, val: IrValue::from_json(v) })
        } else if let Some(v) = &o.gt {
            Some(Cond { op: CondOp::Gt, val: IrValue::from_json(v) })
        } else if let Some(v) = &o.lt {
            Some(Cond { op: CondOp::Lt, val: IrValue::from_json(v) })
        } else if let Some(v) = &o.contains {
            Some(Cond { op: CondOp::Contains, val: IrValue::from_json(v) })
        } else {
            None
        };
        cond = c;
    }

    // strip / map / fallback / protect / transform
    let mut strip_ir = vec![];
    if let Some(o) = obj {
        if let Some(ss) = &o.strip {
            for (i, s) in ss.iter().enumerate() {
                match compile_selector(cx, s, &format!("{path}.strip[{i}]"), scope, None, false) {
                    Some(ir) => strip_ir.push(Box::new(ir)),
                    None => return None,
                }
            }
        }
    }
    let mut map_ir = vec![];
    if let Some(o) = obj {
        if let Some(m) = &o.map {
            let mscope = refs::map_scope(scope);
            for (k, sub) in m {
                match compile_selector(cx, sub, &format!("{path}.map.{k}"), &mscope, Some(k), per_item) {
                    Some(ir) => map_ir.push((k.clone(), Box::new(ir))),
                    None => return None,
                }
            }
        }
    }
    let mut fallback_ir = vec![];
    if let Some(o) = obj {
        if let Some(fs) = &o.fallback {
            for (i, f) in fs.iter().enumerate() {
                match compile_selector(cx, f, &format!("{path}.fallback[{i}]"), scope, None, per_item) {
                    Some(ir) => fallback_ir.push(Box::new(ir)),
                    None => return None,
                }
            }
        }
    }
    let mut protect_ir = None;
    if let Some(o) = obj {
        if let Some(p) = &o.protect {
            protect_ir = Some(match p {
                model::ProtectSpec::Literals(ls) => ProtectIR::Literals(ls.clone()),
                model::ProtectSpec::Nodes(s) => {
                    match compile_selector(cx, s, &format!("{path}.protect"), scope, None, false) {
                        Some(ir) => ProtectIR::Nodes(Box::new(ir)),
                        None => return None,
                    }
                }
            });
        }
    }
    let mut transform_ir = vec![];
    if let Some(o) = obj {
        if let Some(ts) = &o.transform {
            for (i, t) in ts.iter().enumerate() {
                match compile_step(cx, t, &format!("{path}.transform[{i}]"), scope, per_item) {
                    Some(s) => transform_ir.push(s),
                    None => return None,
                }
            }
        }
    }

    let attr_present = attr.is_some();
    let ir = SelectorIR {
        raw: raw_text.clone(),
        attr,
        base,
        exists: obj.and_then(|o| o.exists).unwrap_or(false),
        range: range_ir,
        pick: pick,
        exclude: obj.and_then(|o| o.exclude.clone()).unwrap_or_default(),
        where_: where_ir,
        map: map_ir,
        blocks: blocks.map(|b| (b.mode.unwrap_or_else(|| "flow".into()), b.min_length.unwrap_or(0))),
        paragraphs: paragraphs.map(|p| (p.split.unwrap_or_else(|| "auto".into()), p.min_length.unwrap_or(1))),
        strip: strip_ir,
        protect: protect_ir,
        transform: transform_ir,
        fallback: fallback_ir,
        flags,
        type_map: obj.and_then(|o| o.type_map.clone()),
        optional: obj.and_then(|o| o.optional).unwrap_or(false),
        default: obj.and_then(|o| o.default.clone()).map(|x| IrValue::from_json(&x)),
        limit: obj.and_then(|o| o.limit).map(|l| l as usize),
        cond,
    };

    // ── 管道类型推断（T1.7）──
    let mut ty = types::base_out_type(&ir.base);
    if attr_present {
        ty = types::Ty::Str;
    }
    if ir.paragraphs.is_some() {
        ty = types::Ty::StrList;
    }
    if ir.blocks.is_some() {
        ty = types::Ty::BlockList;
    }
    for step in ir.transform.iter() {
        match types::step_apply(step, ty, path, &ir.raw) {
            Ok(t) => ty = t,
            Err(d) => {
                cx.err(d);
                return None;
            }
        }
    }
    // join 之外的最终输出仍是列表时，标量消费由运行期处理

    // ── 逐章白名单（T1.9）──
    if per_item {
        let ds = cost::check_per_item(&ir, path, &cx.libs.pure_fns);
        for d in ds {
            cx.err(d);
        }
    }

    Some(ir)
}

trait RuleOwned {
    fn rule_owned(self, r: &str) -> Self;
}
impl RuleOwned for Diagnostic {
    fn rule_owned(mut self, r: &str) -> Self {
        self.rule = Some(r.to_string());
        self
    }
}

fn parse_range(r: &str) -> Result<(Option<i64>, Option<i64>, Option<i64>), String> {
    let parts: Vec<&str> = r.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err("需要 start:end[:step]".into());
    }
    let p = |s: &str| -> Result<Option<i64>, String> {
        if s.is_empty() {
            Ok(None)
        } else {
            s.parse::<i64>().map(Some).map_err(|_| format!("`{s}` 不是整数"))
        }
    };
    let start = p(parts[0])?;
    let end = p(parts[1])?;
    let step = if parts.len() == 3 { p(parts[2])? } else { None };
    if let Some(s) = step {
        if s == 0 {
            return Err("step 不能为 0".into());
        }
    }
    Ok((start, end, step))
}

fn build_where(
    cx: &mut Cx,
    w: &serde_json::Value,
    path: &str,
    scope: &refs::Scope,
    per_item: bool,
) -> Option<WhereIR> {
    let m = w.as_object()?;
    let mut keys = vec![];
    for k in m.keys() {
        if !matches!(k.as_str(), "has" | "not" | "eq" | "ne" | "gt" | "lt" | "text") {
            cx.diags.push(Diagnostic::new(
                Code::WUnknownField,
                format!("{path}.where.{k}"),
                format!("where 运算符 `{k}` 不认识，已忽略"),
                "合法运算符：has | not | eq | ne | gt | lt | text（多键 = AND）",
            ));
        } else {
            keys.push(k.clone());
        }
    }
    if keys.is_empty() {
        cx.err(Diagnostic::new(Code::ESchema, path, "where 没有合法运算符", "has | not | eq | ne | gt | lt | text"));
        return None;
    }
    // 单键结构足够（多键 AND 由运行期处理；这里取第一个主键并保留原始）
    let build_one = |cx: &mut Cx, k: &str, v: &serde_json::Value, p: &str| -> Option<WhereIR> {
        Some(match k {
            "has" => {
                let sel = serde_json::from_value::<Selector>(v.clone()).ok()?;
                let ir = compile_selector(cx, &sel, &format!("{p}.has"), scope, None, per_item)?;
                WhereIR::Has(Box::new(ir))
            }
            "not" => {
                let inner = build_where(cx, v, p, scope, per_item)?;
                WhereIR::Not(Box::new(inner))
            }
            "text" => WhereIR::Text(
                v.as_str()
                    .map(|s| s.trim_start_matches("re:").to_string())
                    .unwrap_or_default(),
            ),
            op => WhereIR::Cmp {
                op: match op {
                    "eq" => CondOp::Eq,
                    "ne" => CondOp::Ne,
                    "gt" => CondOp::Gt,
                    _ => CondOp::Lt,
                },
                val: IrValue::from_json(v),
            },
        })
    };
    let first = keys[0].clone();
    let Some(first_sel) = m.get(&first) else { return None; };
    let mut out = build_one(cx, &first, first_sel, path)?;
    // 多键 AND：用 Has 链不合适——保留第一个，其余在运行期由 raw 重建
    // （编译期校验各键合法即可）
    for k in keys.iter().skip(1) {
        if let Some(sel) = m.get(k) {
            build_one(cx, k, sel, &format!("{path}.{k}"))?;
        }
    }
    // 多键：包一层 Not(Not(...)) 不行——直接返回第一个键的结果，多键校验已完成
    // 运行期按 raw 重建；这里保底返回单键 IR
    let _ = &mut out;
    Some(out)
}

/// 编译一个 transform 步骤（T1.2 快捷展开 + T1.7/T1.8/T1.12 检查）。
fn compile_step(
    cx: &mut Cx,
    t: &TransformStep,
    path: &str,
    scope: &refs::Scope,
    per_item: bool,
) -> Option<StepIR> {
    if !t.dup_step_keys.is_empty() {
        cx.err(Diagnostic::new(
            Code::ESchema,
            path,
            format!("一个 transform 步骤对象只能有一个步骤键，发现 {}", t.dup_step_keys.join("/")),
            "每步一个对象，依次放入 transform 数组",
        ));
        return None;
    }
    for k in &t.extra_keys {
        cx.diags.push(Diagnostic::new(
            Code::WUnknownField,
            format!("{path}.{k}"),
            format!("transform 步骤里的未知键 `{k}` 已忽略"),
            "确认是 31 个合法步骤键之一",
        ));
    }
    let p = &t.params;
    let step = match t.kind {
        StepKind::Trim => {
            let chars = p.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
            StepIR::Trim { chars }
        }
        StepKind::Replace => {
            let mut pairs = vec![];
            if let Some(arr) = p.as_array() {
                for pair in arr {
                    if let Some(pa) = pair.as_array() {
                        if pa.len() >= 2 {
                            let f = pa[0].as_str().unwrap_or("");
                            let to = pa[1].as_str().unwrap_or("");
                            if regex::has_re_prefix(f) {
                                let src = f.trim_start_matches("re:");
                                if let Err(d) = regex::check(src, "", path, &t.raw.to_string()) {
                                    cx.err(d);
                                    return None;
                                }
                            }
                            pairs.push((f.to_string(), to.to_string()));
                        }
                    }
                }
            }
            StepIR::Replace { pairs }
        }
        StepKind::Regex => {
            let src = p.get("regex").and_then(|x| x.as_str()).unwrap_or("");
            if src.is_empty() {
                cx.err(Diagnostic::new(Code::ESchema, path, "regex 步骤缺少 regex 参数", "{\"regex\": {\"regex\": …}}"));
                return None;
            }
            if let Err(d) = regex::check(src, "", path, &t.raw.to_string()) {
                cx.err(d);
                return None;
            }
            StepIR::Regex {
                src: src.to_string(),
                repl: p.get("replace").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                all: p.get("all").and_then(|x| x.as_bool()).unwrap_or(false),
            }
        }
        StepKind::Match => {
            let src = p.get("regex").and_then(|x| x.as_str()).unwrap_or("");
            if let Err(d) = regex::check(src, "", path, &t.raw.to_string()) {
                cx.err(d);
                return None;
            }
            StepIR::Match {
                src: src.to_string(),
                n: p.get("n").and_then(|x| x.as_u64()).unwrap_or(1) as usize,
                group: p.get("group").and_then(|x| {
                    x.as_str().map(String::from).or_else(|| x.as_u64().map(|n| n.to_string()))
                }),
            }
        }
        StepKind::Prepend => StepIR::Prepend(p.as_str().unwrap_or("").to_string()),
        StepKind::Append => StepIR::Append(p.as_str().unwrap_or("").to_string()),
        StepKind::Absolute => {
            let base = match p {
                serde_json::Value::Bool(_) => None,
                serde_json::Value::Object(_) => {
                    let b = p.get("base").and_then(|x| x.as_str()).unwrap_or("var:baseUrl");
                    Some(build_param_str(cx, b, path, scope))
                }
                _ => None,
            };
            StepIR::Absolute { base }
        }
        StepKind::Urlencode | StepKind::Urldecode => {
            let charset = if p.is_object() { p.get("charset").and_then(|x| x.as_str()).map(String::from) } else { None };
            if t.kind == StepKind::Urlencode {
                StepIR::Urlencode { charset }
            } else {
                StepIR::Urldecode { charset }
            }
        }
        StepKind::Base64 => StepIR::Base64(p.as_str() == Some("encode")),
        StepKind::Hex => StepIR::Hex(p.as_str() == Some("encode")),
        StepKind::Crypto => {
            let alg = p.get("alg").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if alg.is_empty() {
                cx.err(Diagnostic::new(Code::ESchema, path, "crypto 步骤缺少 alg", "{\"crypto\": {\"alg\":\"aes-cbc\",\"mode\":\"decrypt\",…}}"));
                return None;
            }
            let mode = p.get("mode").and_then(|x| x.as_str()).unwrap_or("");
            if mode != "encrypt" && mode != "decrypt" {
                cx.err(Diagnostic::new(Code::EEnum, path, format!("crypto.mode `{mode}` 不是合法枚举值"), "合法值：encrypt | decrypt"));
                return None;
            }
            let key = match p.get("key").and_then(|x| x.as_str()) {
                Some(k) => build_param_str(cx, k, &format!("{path}.crypto.key"), scope),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, path, "crypto 步骤缺少 key", "key 可以是 var:/字面量"));
                    return None;
                }
            };
            let iv = p.get("iv").and_then(|x| x.as_str()).map(|k| build_param_str(cx, k, &format!("{path}.crypto.iv"), scope));
            cx.features.insert("crypto".into());
            cx.crypto.insert(alg.clone());
            StepIR::Crypto {
                alg,
                mode: if mode == "encrypt" { CryptoMode::Encrypt } else { CryptoMode::Decrypt },
                key,
                iv,
                padding: p.get("padding").and_then(|x| x.as_str()).map(String::from),
            }
        }
        StepKind::Font => {
            let map_fn = p.get("map").and_then(|x| x.as_str()).unwrap_or("").trim_start_matches("lib:").to_string();
            if map_fn.is_empty() {
                cx.err(Diagnostic::new(Code::ESchema, path, "font 步骤缺少 map", "{\"font\": {\"map\":\"lib:glyph\",\"ref\":\"var:normalFont\"}}"));
                return None;
            }
            check_lib_call(cx, &map_fn, path);
            // D10：font.ref 是宿主注入的标准字形名（如 var:normalFont），豁免引用图校验
            let reference = p.get("ref").and_then(|x| x.as_str()).map(|r| ParamExpr::Template(
                sugar::parse_template(r, path).unwrap_or_else(|_| vec![sugar::Seg::Lit(r.to_string())]),
            ));
            cx.features.insert("font".into());
            StepIR::Font { map_fn, reference }
        }
        StepKind::Html => StepIR::Html,
        StepKind::Json => StepIR::JsonStep,
        StepKind::Date => {
            let from = p.get("from").and_then(|x| x.as_str()).unwrap_or("x").to_string();
            let to = p.get("to").and_then(|x| x.as_str()).unwrap_or("yyyy-MM-dd").to_string();
            let tz = p.get("tz").and_then(|x| x.as_i64()).unwrap_or(8);
            StepIR::Date { from, to, tz }
        }
        StepKind::Number => {
            let unit = if p.is_object() { p.get("unit").and_then(|x| x.as_bool()).unwrap_or(false) } else { false };
            StepIR::Number { unit }
        }
        StepKind::NumChapter => StepIR::NumChapter,
        StepKind::CjkNumber => StepIR::CjkNumber(p.as_str() == Some("to")),
        StepKind::T2s => StepIR::T2s,
        StepKind::S2t => StepIR::S2t,
        StepKind::Slice => {
            let s = p.as_str().unwrap_or("0:");
            match parse_range(s) {
                Ok((a, b, c)) => StepIR::Slice { start: a, end: b, step: c },
                Err(msg) => {
                    cx.err(Diagnostic::new(Code::ESchema, path, format!("slice `{s}` 不合法：{msg}"), "字符串切片 start:end[:step]"));
                    return None;
                }
            }
        }
        StepKind::Charset => StepIR::Charset(p.as_str().unwrap_or("utf-8").to_string()),
        StepKind::Join => StepIR::Join(p.as_str().unwrap_or("").to_string()),
        StepKind::Split => StepIR::Split(p.as_str().unwrap_or("").to_string()),
        StepKind::Zip => {
            let with_raw = p.get("with").and_then(|x| x.as_str()).unwrap_or("");
            if with_raw.is_empty() {
                cx.err(Diagnostic::new(Code::ESchema, path, "zip 步骤缺少 with", "{\"zip\": {\"with\":\"css:.price\",\"sep\":\" · \"}}"));
                return None;
            }
            let with = Selector::Str(with_raw.to_string());
            let with_ir = compile_selector(cx, &with, &format!("{path}.zip.with"), scope, None, per_item)?;
            let sep = p.get("sep").and_then(|x| x.as_str()).unwrap_or("").to_string();
            StepIR::Zip { with: Box::new(with_ir), sep }
        }
        StepKind::Protect => match serde_json::from_value::<model::ProtectSpec>(p.clone()) {
            Ok(model::ProtectSpec::Literals(ls)) => StepIR::Protect(ProtectIR::Literals(ls)),
            Ok(model::ProtectSpec::Nodes(s)) => {
                let ir = compile_selector(cx, &s, &format!("{path}.protect"), scope, None, false)?;
                StepIR::Protect(ProtectIR::Nodes(Box::new(ir)))
            }
            Err(e) => {
                cx.err(Diagnostic::new(Code::ESchema, path, format!("protect 形状不合法：{e}"), "字符串数组或选择器（§3.6.2）"));
                return None;
            }
        },
        StepKind::Unique => StepIR::Unique,
        StepKind::Sort => {
            let desc = if p.is_object() { p.get("desc").and_then(|x| x.as_bool()).unwrap_or(false) } else { false };
            StepIR::Sort { desc }
        }
        StepKind::DropEmpty => StepIR::DropEmpty,
        StepKind::Js => {
            let (fname, tier_ctx) = match p {
                serde_json::Value::String(s) => (s.trim_start_matches("lib:").to_string(), false),
                serde_json::Value::Object(_) => (
                    p.get("fn").and_then(|x| x.as_str()).unwrap_or("").trim_start_matches("lib:").to_string(),
                    p.get("ctx").and_then(|x| x.as_bool()).unwrap_or(false),
                ),
                _ => {
                    cx.err(Diagnostic::new(Code::ESchema, path, "js 步骤参数不合法", "\"lib:fn\" 或 {\"fn\":\"lib:fn\",\"ctx\":true}"));
                    return None;
                }
            };
            if fname.is_empty() {
                cx.err(Diagnostic::new(Code::ESchema, path, "js 步骤缺少函数名", "写 lib:fnName"));
                return None;
            }
            check_lib_call(cx, &fname, path);
            if tier_ctx {
                if !cx.libs.is_ctx(&fname) {
                    cx.err(Diagnostic::new(Code::EScript, path, format!("`{fname}` 不在 lib.ctx 档"), "ctx:true 只能调用 lib.ctx 函数"));
                    return None;
                }
                if per_item {
                    // 逐章 ctx 由 cost 检查覆盖
                }
            } else if !cx.libs.is_pure(&fname) {
                cx.err(Diagnostic::new(Code::EScript, path, format!("`{fname}` 不在 lib.pure 档"), "不写 ctx:true 时调用 lib.pure 函数"));
                return None;
            }
            cx.features.insert("js".into());
            StepIR::Js { fn_name: fname, tier: if tier_ctx { Tier::Ctx } else { Tier::Pure } }
        }
    };
    Some(step)
}

/// 基座命名空间编译（T1.3 命名空间校验 + 引用图 + lib/expr/regex）。
fn parse_base(cx: &mut Cx, s: &str, path: &str, scope: &refs::Scope, flags: &str) -> Option<BaseSel> {
    if s.is_empty() {
        return Some(BaseSel::None);
    }
    // 无参命名空间
    match s {
        "text" => return Some(BaseSel::Text),
        "owntext" => return Some(BaseSel::OwnText),
        "html" => return Some(BaseSel::Html),
        "tag" => return Some(BaseSel::Tag),
        _ => {}
    }
    let (ns, rest) = match s.find(':') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => {
            cx.err(selector_ns_err(path, s));
            return None;
        }
    };
    match ns {
        "css" => {
            // CSS 语法预校验
            if rest.trim().is_empty() {
                cx.err(Diagnostic::new(Code::ESelector, path, "css: 表达式为空", "写 css:div.title 之类").rule(s));
                return None;
            }
            if scraper::Selector::parse(rest).is_err() {
                cx.err(Diagnostic::new(
                    Code::ESelector,
                    path,
                    format!("CSS 选择器语法非法：`{rest}`"),
                    "检查括号/属性选择器；CSS3 为主，:is/:where/:not/:has 按 forgiving 处理",
                )
                .rule(s));
                return None;
            }
            Some(BaseSel::Css(rest.to_string()))
        }
        "xpath" => {
            if rest.trim().is_empty() {
                cx.err(Diagnostic::new(Code::ESelector, path, "xpath: 表达式为空", "写 xpath://div[@id=\"list\"]").rule(s));
                return None;
            }
            Some(BaseSel::Xpath(rest.to_string()))
        }
        "json" => {
            if rest.trim().is_empty() {
                cx.err(Diagnostic::new(Code::ESelector, path, "json: 路径为空", "写 json:$.data.list[*]").rule(s));
                return None;
            }
            Some(BaseSel::Json(rest.to_string()))
        }
        "re" => {
            if let Err(d) = regex::check(rest, flags, path, s) {
                cx.err(d);
                return None;
            }
            Some(BaseSel::Re { source: rest.to_string(), flags: flags.to_string() })
        }
        "attr" => {
            if rest.is_empty() {
                cx.err(Diagnostic::new(Code::ESelector, path, "attr: 参数为空", "写 attr:href").rule(s));
                return None;
            }
            Some(BaseSel::Attr(rest.to_string()))
        }
        "const" => {
            // JSON 解析，失败当字符串（S§3.2）
            let v: serde_json::Value = serde_json::from_str(rest).unwrap_or(serde_json::Value::String(rest.to_string()));
            Some(BaseSel::Const(IrValue::from_json(&v)))
        }
        "var" => {
            if !scope.has("var", rest) {
                cx.err(ref_undef(path, s));
                return None;
            }
            Some(BaseSel::Var(rest.to_string()))
        }
        "let" => {
            if !scope.has("let", rest) {
                cx.err(ref_undef(path, s));
                return None;
            }
            Some(BaseSel::Let(rest.to_string()))
        }
        "session" => {
            if !scope.has("session", rest) {
                cx.err(ref_undef(path, s));
                return None;
            }
            Some(BaseSel::Session(rest.to_string()))
        }
        "token" => {
            if !cx.has_login {
                cx.err(ref_undef(path, s));
                return None;
            }
            Some(BaseSel::Token(rest.to_string()))
        }
        "cookie" => {
            if !cx.has_login && !cx.has_cookie_block {
                cx.err(ref_undef(path, s));
                return None;
            }
            Some(BaseSel::Cookie(rest.to_string()))
        }
        "cache" => Some(BaseSel::Cache(rest.to_string())),
        "lib" => {
            if rest.is_empty() {
                cx.err(Diagnostic::new(Code::ESelector, path, "lib: 参数为空", "写 lib:cleanTitle").rule(s));
                return None;
            }
            if !cx.libs.exists(rest) {
                cx.err(Diagnostic::new(
                    Code::EScript,
                    path,
                    format!("lib 函数 `{rest}` 不存在"),
                    "在 lib.pure / lib.ctx 里定义；写法 function NAME( …）",
                )
                .rule(s));
                return None;
            }
            if cx.libs.is_ctx(rest) {
                cx.err(Diagnostic::new(
                    Code::EScript,
                    path,
                    format!("`lib:{rest}` 是 ctx 档函数，不能从选择器调用"),
                    "ctx 函数只能从 script 钩子或 {\"js\":{\"ctx\":true}} 调用",
                )
                .rule(s));
                return None;
            }
            cx.features.insert("js".into());
            Some(BaseSel::Lib(rest.to_string()))
        }
        "expr" => {
            let e = compile_expr(cx, rest, path, scope, false)?;
            Some(BaseSel::Expr(e))
        }
        _ => {
            cx.err(selector_ns_err(path, s));
            None
        }
    }
}

fn selector_ns_err(path: &str, s: &str) -> Diagnostic {
    Diagnostic::new(
        Code::ESelector,
        path,
        format!("选择器 `{s}` 缺命名空间或前缀未知"),
        format!("18 个合法命名空间：{}", sugar::NAMESPACES.join(" ")),
    )
    .rule(s)
}

// ───────────────────────── 登录编译 ─────────────────────────

fn build_login(cx: &mut Cx, cfg: &model::LoginCfg) -> Result<LoginIR, ()> {
    warn_unknown(&cfg.extra, "login", cx);
    if let Some(t) = &cfg.login_type {
        if !matches!(t.as_str(), "form" | "cookie" | "token" | "qr") {
            cx.err(Diagnostic::new(Code::EEnum, "login.type", format!("`{t}` 不是合法枚举值"), "合法值：form | cookie | token | qr"));
        }
    }
    let is_multi = cfg.steps.is_some();
    // 互斥（lint 9）
    if is_multi && (cfg.fields.is_some() || cfg.submit.is_some()) {
        cx.err(Diagnostic::new(
            Code::ESchema,
            "login",
            "login.steps 与顶层 fields/submit 互斥",
            "单步用 fields+submit；多步用 steps[]，二选一",
        ));
        return Err(());
    }
    if !is_multi && cfg.submit.is_none() {
        cx.err(Diagnostic::new(Code::ESchema, "login.submit", "单步登录缺少 submit", "fields + submit 成对出现"));
        return Err(());
    }

    let mut headers = vec![];
    if let Some(h) = &cfg.headers {
        for (k, val) in h {
            match sugar::parse_template(val, &format!("login.headers.{k}")) {
                Ok(t) => headers.push((k.clone(), t)),
                Err(d) => cx.err(d),
            }
        }
    }

    // 步骤编译
    let mut steps_ir: Vec<LoginStepIR> = vec![];
    if is_multi {
        let Some(steps) = cfg.steps.as_ref() else { return Err(()); };
        for (i, st) in steps.iter().enumerate() {
            warn_unknown(&st.extra, &format!("login.steps[{i}]"), cx);
            let id = st.id.clone().unwrap_or_default();
            let mut fields = vec![];
            if let Some(fs) = &st.fields {
                for f in fs {
                    let key = match &f.key {
                        Some(k) => k.clone(),
                        None => {
                            cx.err(Diagnostic::new(Code::ESchema, &format!("login.steps[{i}].fields"), "loginField 缺少 key", "每项都要 key"));
                            return Err(());
                        }
                    };
                    if f.secret.unwrap_or(false) {
                        if let Some(serde_json::Value::String(s)) = &f.default {
                            cx.secrets.push(s.clone());
                        }
                    }
                    fields.push(LoginFieldIR {
                        key,
                        label: f.label.clone(),
                        secret: f.secret.unwrap_or(false),
                        default: f.default.clone().map(|x| IrValue::from_json(&x)),
                    });
                }
            }
            // 本步作用域
            let base_scope = refs::global_scope(&cx.vars);
            let step_scope = refs::login_step_scope(&base_scope, fields.iter().map(|f| f.key.clone()).collect::<Vec<_>>().as_slice());
            let submit = match &st.submit {
                Some(s) => Some(build_submit(cx, s, &format!("login.steps[{i}].submit"), &step_scope)?),
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, &format!("login.steps[{i}].submit"), "login step 缺少 submit", "每步都要 submit"));
                    return Err(());
                }
            };
            let (next_to, next_when) = match &st.next {
                Some(n) => {
                    warn_unknown(&n.extra, &format!("login.steps[{i}].next"), cx);
                    let to = n.to.clone().unwrap_or_default();
                    let when = match &n.when {
                        Some(w) => compile_selector(cx, w, &format!("login.steps[{i}].next.when"), &step_scope, None, false).map(Box::new),
                        None => None,
                    };
                    (Some(to), when)
                }
                None => (None, None),
            };
            steps_ir.push(LoginStepIR { id, fields, submit, next_to, next_when });
        }
        // 步骤图校验（T1.10）
        let pairs: Vec<(String, Option<String>)> = steps_ir
            .iter()
            .map(|s| (s.id.clone(), s.next_to.clone()))
            .collect();
        match graph::check_graph(&pairs, "login.steps") {
            Ok(_) => {}
            Err(d) => {
                cx.err(d);
                return Err(());
            }
        }
    } else {
        // 单步合成
        let mut fields = vec![];
        if let Some(fs) = &cfg.fields {
            for f in fs {
                if let Some(k) = &f.key {
                    if f.secret.unwrap_or(false) {
                        if let Some(serde_json::Value::String(s)) = &f.default {
                            cx.secrets.push(s.clone());
                        }
                    }
                    fields.push(LoginFieldIR {
                        key: k.clone(),
                        label: f.label.clone(),
                        secret: f.secret.unwrap_or(false),
                        default: f.default.clone().map(|x| IrValue::from_json(&x)),
                    });
                }
            }
        }
        let scope = refs::login_step_scope(&refs::global_scope(&cx.vars), fields.iter().map(|f| f.key.clone()).collect::<Vec<_>>().as_slice());
        let Some(submit_cfg) = cfg.submit.as_ref() else { return Err(()); };
        let submit = build_submit(cx, submit_cfg, "login.submit", &scope)?;
        steps_ir.push(LoginStepIR {
            id: "login".into(),
            fields,
            submit: Some(submit),
            next_to: None,
            next_when: None,
        });
    }

    // check
    let (check_req, check_ok) = match &cfg.check {
        Some(c) => {
            let scope = refs::global_scope(&cx.vars);
            let req = match &c.request {
                Some(r) => build_request(cx, r, "login.check.request", &scope).ok_or(())?,
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, "login.check.request", "check 缺少 request", "{\"request\": …, \"ok\": …}"));
                    return Err(());
                }
            };
            let ok = match &c.ok {
                Some(o) => match compile_selector(cx, o, "login.check.ok", &scope, None, false) {
                    Some(ir) => Box::new(ir),
                    None => return Err(()),
                },
                None => {
                    cx.err(Diagnostic::new(Code::ESchema, "login.check.ok", "check 缺少 ok 判定", "选一个未登录时不存在的字段"));
                    return Err(());
                }
            };
            (Some(req), Some(ok))
        }
        None => (None, None),
    };

    Ok(LoginIR {
        login_type: cfg.login_type.clone().unwrap_or_else(|| "form".into()),
        steps: steps_ir,
        check_req,
        check_ok,
        headers,
        is_multi,
    })
}

fn build_submit(cx: &mut Cx, s: &model::LoginSubmit, path: &str, scope: &refs::Scope) -> Result<SubmitIR, ()> {
    warn_unknown(&s.extra, path, cx);
    let req = match &s.request {
        Some(r) => build_request(cx, r, &format!("{path}.request"), scope).ok_or(())?,
        None => {
            cx.err(Diagnostic::new(Code::ESchema, &format!("{path}.request"), "submit 缺少 request", "登录提交是一个 request"));
            return Err(());
        }
    };
    let ok = match &s.ok {
        Some(o) => match compile_selector(cx, o, &format!("{path}.ok"), scope, None, false) {
            Some(ir) => Box::new(ir),
            None => return Err(()),
        },
        None => {
            cx.err(Diagnostic::new(Code::ESchema, &format!("{path}.ok"), "submit 缺少 ok 成功判定", "如 {\"$\":\"json:$.code\",\"eq\":0}"));
            return Err(());
        }
    };
    let token = match &s.token {
        Some(t) => compile_selector(cx, t, &format!("{path}.token"), scope, None, false).map(Box::new),
        None => None,
    };
    let cookie = match &s.cookie {
        Some(c) => compile_selector(cx, c, &format!("{path}.cookie"), scope, None, false).map(Box::new),
        None => None,
    };
    Ok(SubmitIR { req, ok, token, cookie })
}


