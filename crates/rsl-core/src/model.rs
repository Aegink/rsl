//! T1.1 `model`：Schema 29 个 `$defs` 的 serde 结构。
//! 未知字段收集进 `extra`（flatten），由 analyze 报 `W_UNKNOWN_FIELD`——绝不拒绝加载（S§2.2）。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

// ───────────────────────── 根 ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub rsl: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub carrier: Option<String>,
    #[serde(rename = "baseUrl", default)]
    pub base_url: Option<String>,
    #[serde(rename = "match", default)]
    pub match_: Option<MatchCfg>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub requires: Option<Requires>,
    #[serde(default)]
    pub http: Option<HttpCfg>,
    #[serde(default)]
    pub vars: Option<BTreeMap<String, VarEntry>>,
    #[serde(default)]
    pub cookie: Option<CookieCfg>,
    #[serde(default)]
    pub cache: Option<BTreeMap<String, CacheEntry>>,
    #[serde(default)]
    pub media: Option<MediaCfg>,
    #[serde(default)]
    pub login: Option<LoginCfg>,
    #[serde(default)]
    pub merge: Option<MergeCfg>,
    #[serde(default)]
    pub lib: Option<LibCfg>,
    #[serde(default)]
    pub stages: Option<BTreeMap<String, StageCfg>>,
    #[serde(rename = "hostStages", default)]
    pub host_stages: Option<BTreeMap<String, StageCfg>>,
    #[serde(default)]
    pub limits: Option<Limits>,
    #[serde(default)]
    pub meta: Option<MetaCfg>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatchCfg {
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── requires ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Requires {
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub crypto: Vec<String>,
    #[serde(rename = "selectorLevel", default)]
    pub selector_level: Option<SelectorLevel>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectorLevel {
    #[serde(default)]
    pub css: Option<String>,
    #[serde(default)]
    pub xpath: Option<String>,
    #[serde(default)]
    pub jsonpath: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── http ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpCfg {
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub timeout: Option<i64>,
    #[serde(default)]
    pub retry: Option<RetryCfg>,
    #[serde(default)]
    pub rate: Option<RateCfg>,
    #[serde(default)]
    pub redirect: Option<RedirectCfg>,
    #[serde(default)]
    pub cookies: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
    #[serde(default)]
    pub dns: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub tls: Option<TlsCfg>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RetryCfg {
    #[serde(default)]
    pub times: Option<i64>,
    #[serde(default)]
    pub backoff: Option<String>,
    #[serde(default)]
    pub on: Vec<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateCfg {
    #[serde(default)]
    pub rps: Option<f64>,
    #[serde(default)]
    pub burst: Option<i64>,
    #[serde(rename = "windowMs", default)]
    pub window_ms: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RedirectCfg {
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TlsCfg {
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── vars / cookie / cache / media ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum VarEntry {
    Val(Value),
    Cfg(VarEntryCfg),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VarEntryCfg {
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub options: Vec<Value>,
    #[serde(default)]
    pub persist: Option<bool>,
    #[serde(default)]
    pub secret: Option<bool>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for VarEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if v.is_object() {
            let cfg = serde_json::from_value::<VarEntryCfg>(v).map_err(serde::de::Error::custom)?;
            Ok(VarEntry::Cfg(cfg))
        } else {
            Ok(VarEntry::Val(v))
        }
    }
}

impl VarEntry {
    pub fn value(&self) -> Option<&Value> {
        match self {
            VarEntry::Val(v) => Some(v),
            VarEntry::Cfg(c) => c.value.as_ref().or(c.default.as_ref()),
        }
    }
    pub fn label(&self) -> Option<&str> {
        match self {
            VarEntry::Val(_) => None,
            VarEntry::Cfg(c) => c.label.as_deref(),
        }
    }
    pub fn secret(&self) -> bool {
        matches!(self, VarEntry::Cfg(c) if c.secret.unwrap_or(false))
    }
    pub fn persist(&self) -> bool {
        matches!(self, VarEntry::Cfg(c) if c.persist.unwrap_or(false))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CookieCfg {
    #[serde(default)]
    pub persist: Option<bool>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub initial: Option<BTreeMap<String, String>>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheEntry {
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(default)]
    pub take: Option<Selector>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub ttl: Option<i64>,
    #[serde(rename = "ttlPartial", default)]
    pub ttl_partial: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaCfg {
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub decode: Option<MediaDecode>,
    #[serde(default)]
    pub cache: Option<MediaCacheCfg>,
    #[serde(rename = "maxBytes", default)]
    pub max_bytes: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct MediaDecode {
    #[serde(rename = "in", default)]
    pub input: Option<String>,
    #[serde(default)]
    pub out: Option<String>,
    #[serde(default)]
    pub fn_: Option<String>, // rename "fn"
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// serde 关键字 fn 需要 rename
impl<'de> Deserialize<'de> for MediaDecode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let mut out = MediaDecode::default();
        if let Some(m) = v.as_object() {
            out.input = m.get("in").and_then(|x| x.as_str()).map(String::from);
            out.out = m.get("out").and_then(|x| x.as_str()).map(String::from);
            out.fn_ = m.get("fn").and_then(|x| x.as_str()).map(String::from);
            for (k, val) in m {
                if k != "in" && k != "out" && k != "fn" {
                    out.extra.insert(k.clone(), val.clone());
                }
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MediaCacheCfg {
    #[serde(default)]
    pub ttl: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── login ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginCfg {
    #[serde(rename = "type", default)]
    pub login_type: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<LoginField>>,
    #[serde(default)]
    pub submit: Option<LoginSubmit>,
    #[serde(default)]
    pub steps: Option<Vec<LoginStep>>,
    #[serde(default)]
    pub check: Option<CheckCfg>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginField {
    pub key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub secret: Option<bool>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginSubmit {
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(default)]
    pub ok: Option<Selector>,
    #[serde(default)]
    pub token: Option<Selector>,
    #[serde(default)]
    pub cookie: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoginStep {
    pub id: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<LoginField>>,
    #[serde(default)]
    pub submit: Option<LoginSubmit>,
    #[serde(default)]
    pub next: Option<NextCfg>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NextCfg {
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub when: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CheckCfg {
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(default)]
    pub ok: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── merge / lib ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MergeCfg {
    #[serde(default)]
    pub key: Option<Vec<String>>,
    #[serde(default)]
    pub normalize: Option<Vec<String>>,
    #[serde(rename = "crossBucket", default)]
    pub cross_bucket: Option<bool>,
    #[serde(default)]
    pub order: Option<Vec<String>>,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(rename = "minScore", default)]
    pub min_score: Option<f64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LibCfg {
    #[serde(default)]
    pub pure: Option<String>,
    #[serde(default)]
    pub ctx: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── 阶段 ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageCfg {
    #[serde(rename = "let", default)]
    pub let_: Option<LetMap>,
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(default)]
    pub parse: Option<Selector>,
    #[serde(default)]
    pub out: Option<OutCfg>,
    #[serde(default)]
    pub paginate: Option<PaginateCfg>,
    #[serde(default)]
    pub script: Option<ScriptCfg>,
    #[serde(rename = "onError", default)]
    pub on_error: Option<String>,
    #[serde(rename = "timeoutMs", default)]
    pub timeout_ms: Option<i64>,
    #[serde(rename = "requiresLogin", default)]
    pub requires_login: Option<bool>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub dedupe: Option<String>,
    #[serde(rename = "reuseDetailHtml", default)]
    pub reuse_detail_html: Option<bool>,
    #[serde(default)]
    pub direct: Option<bool>,
    #[serde(default)]
    pub filters: Option<Vec<FilterCfg>>,
    #[serde(default)]
    pub groups: Option<Vec<GroupCfg>>,
    #[serde(rename = "groupsFrom", default)]
    pub groups_from: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// `let` 块：保持 JSON 声明顺序（S§6.1.1 按声明顺序求值）。
#[derive(Debug, Clone, Serialize, Default)]
pub struct LetMap(pub Vec<(String, LetEntry)>);

impl<'de> Deserialize<'de> for LetMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let obj = v
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("let 必须是对象"))?;
        let mut out = vec![];
        for (k, val) in obj {
            let e: LetEntry = serde_json::from_value(val.clone()).map_err(serde::de::Error::custom)?;
            out.push((k.clone(), e));
        }
        Ok(LetMap(out))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum LetEntry {
    Str(String),
    Request(LetRequest),
    Parallel(LetParallel),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LetRequest {
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(default)]
    pub take: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LetParallel {
    #[serde(default)]
    pub parallel: Vec<LetParallelItem>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LetParallelItem {
    #[serde(default)]
    pub request: Option<RequestCfg>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for LetEntry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if v.is_string() {
            return Ok(LetEntry::Str(v.as_str().unwrap().to_string()));
        }
        let obj = v
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("let 项必须是字符串或对象"))?;
        if obj.contains_key("parallel") {
            let p = serde_json::from_value::<LetParallel>(v).map_err(serde::de::Error::custom)?;
            return Ok(LetEntry::Parallel(p));
        }
        let r = serde_json::from_value::<LetRequest>(v).map_err(serde::de::Error::custom)?;
        Ok(LetEntry::Request(r))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestCfg {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub body: Option<BodyCfg>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub charset: Option<String>,
    #[serde(default)]
    pub encode: Option<EncodeCfg>,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(rename = "webJs", default)]
    pub web_js: Option<String>,
    #[serde(default)]
    pub sniff: Option<String>,
    #[serde(rename = "autoUpgradeIf", default)]
    pub auto_upgrade_if: Option<Selector>,
    #[serde(default)]
    pub skip: Option<bool>,
    #[serde(default)]
    pub retry: Option<RetryCfg>,
    #[serde(default)]
    pub timeout: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BodyCfg {
    #[serde(rename = "type", default)]
    pub body_type: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncodeCfg {
    #[serde(default)]
    pub params: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// next 允许单个选择器或选择器数组（S§4.7：toc 的 next 可返回 URL 数组）。
#[derive(Debug, Clone, Serialize)]
pub struct SelectorList(pub Vec<Selector>);

impl<'de> Deserialize<'de> for SelectorList {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        match &v {
            Value::Array(_) => {
                let list = serde_json::from_value::<Vec<Selector>>(v).map_err(serde::de::Error::custom)?;
                Ok(SelectorList(list))
            }
            _ => {
                let s = serde_json::from_value::<Selector>(v).map_err(serde::de::Error::custom)?;
                Ok(SelectorList(vec![s]))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaginateCfg {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(rename = "pageVar", default)]
    pub page_var: Option<String>,
    #[serde(default)]
    pub next: Option<SelectorList>,
    #[serde(default)]
    pub cursor: Option<CursorCfg>,
    #[serde(default)]
    pub enumerate: Option<EnumerateSpec>,
    #[serde(default)]
    pub into: Option<String>,
    #[serde(rename = "joinWith", default)]
    pub join_with: Option<String>,
    #[serde(default)]
    pub carry: Vec<String>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(rename = "stopIf", default)]
    pub stop_if: Option<StopIfCfg>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CursorCfg {
    #[serde(default)]
    pub take: Option<Selector>,
    #[serde(default)]
    pub into: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum EnumerateSpec {
    List(Vec<i64>),
    Range(EnumerateRange),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnumerateRange {
    pub from: Option<i64>,
    pub to: Option<i64>,
    #[serde(default)]
    pub step: Option<i64>,
}

impl<'de> Deserialize<'de> for EnumerateSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if let Some(arr) = v.as_array() {
            let list: Vec<i64> = arr
                .iter()
                .map(|x| {
                    x.as_i64()
                        .ok_or_else(|| serde::de::Error::custom("enumerate 数组项必须是整数"))
                })
                .collect::<Result<_, _>>()?;
            return Ok(EnumerateSpec::List(list));
        }
        let r = serde_json::from_value::<EnumerateRange>(v).map_err(serde::de::Error::custom)?;
        Ok(EnumerateSpec::Range(r))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StopIfCfg {
    #[serde(default)]
    pub empty: Option<String>,
    #[serde(rename = "noNew", default)]
    pub no_new: Option<bool>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutCfg {
    #[serde(rename = "emptyOk", default)]
    pub empty_ok: Option<bool>,
    #[serde(default)]
    pub verify: Option<VerifyCfg>,
    #[serde(default)]
    pub list: Option<Selector>,
    #[serde(default)]
    pub map: Option<BTreeMap<String, Selector>>,
    #[serde(default)]
    pub title: Option<Selector>,
    #[serde(default)]
    pub content: Option<Selector>,
    #[serde(default)]
    pub images: Option<Selector>,
    #[serde(default)]
    pub audio: Option<Selector>,
    #[serde(default)]
    pub video: Option<Selector>,
    #[serde(default)]
    pub files: Option<Selector>,
    #[serde(default)]
    pub feed: Option<FeedCfg>,
    #[serde(default)]
    pub tracks: Option<Vec<TrackDef>>,
    #[serde(rename = "nextPage", default)]
    pub next_page: Option<Selector>,
    #[serde(rename = "payAction", default)]
    pub pay_action: Option<Selector>,
    // detail 契约保留字段（S§6.4）
    #[serde(default)]
    pub name: Option<Selector>,
    #[serde(default)]
    pub author: Option<Selector>,
    #[serde(default)]
    pub intro: Option<Selector>,
    #[serde(default)]
    pub kind: Option<Selector>,
    #[serde(default)]
    pub cover: Option<Selector>,
    #[serde(rename = "wordCount", default)]
    pub word_count: Option<Selector>,
    #[serde(rename = "lastChapter", default)]
    pub last_chapter: Option<Selector>,
    #[serde(rename = "updateTime", default)]
    pub update_time: Option<Selector>,
    #[serde(rename = "tocUrl", default)]
    pub toc_url: Option<Selector>,
    #[serde(rename = "canRename", default)]
    pub can_rename: Option<Selector>,
    #[serde(default)]
    pub downloads: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerifyCfg {
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(rename = "minItems", default)]
    pub min_items: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeedCfg {
    #[serde(default)]
    pub items: Option<Selector>,
    #[serde(default)]
    pub map: Option<BTreeMap<String, Selector>>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackDef {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub url: Option<Selector>,
    #[serde(default)]
    pub text: Option<Selector>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptCfg {
    #[serde(default)]
    pub before: Option<ScriptRef>,
    #[serde(rename = "onResponse", default)]
    pub on_response: Option<ScriptRef>,
    #[serde(default)]
    pub after: Option<ScriptRef>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ScriptRef {
    Str(String),
    Obj(ScriptRefObj),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptRefObj {
    #[serde(default, rename = "fn")]
    pub fn_: Option<String>,
    #[serde(default)]
    pub ctx: Option<bool>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for ScriptRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if v.is_string() {
            return Ok(ScriptRef::Str(v.as_str().unwrap().to_string()));
        }
        let obj = serde_json::from_value::<ScriptRefObj>(v).map_err(serde::de::Error::custom)?;
        Ok(ScriptRef::Obj(obj))
    }
}

impl ScriptRef {
    pub fn fn_name(&self) -> Option<&str> {
        match self {
            ScriptRef::Str(s) => Some(s.as_str()),
            ScriptRef::Obj(o) => o.fn_.as_deref(),
        }
    }
    pub fn ctx_tier(&self) -> bool {
        match self {
            ScriptRef::Str(_) => false,
            ScriptRef::Obj(o) => o.ctx.unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterCfg {
    pub key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub persist: Option<bool>,
    #[serde(default)]
    pub options: Vec<FilterOption>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterOption {
    pub value: Option<Value>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupCfg {
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub params: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub when: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub groups: Option<Vec<GroupCfg>>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── 选择器 ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Selector {
    Str(String),
    Obj(SelectorObj),
}

impl<'de> Deserialize<'de> for Selector {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        match &v {
            Value::String(s) => Ok(Selector::Str(s.clone())),
            Value::Object(_) => {
                let o = serde_json::from_value::<SelectorObj>(v).map_err(serde::de::Error::custom)?;
                Ok(Selector::Obj(o))
            }
            _ => Err(serde::de::Error::custom(
                "选择器必须是字符串或对象（§3.1）",
            )),
        }
    }
}

impl Selector {
    pub fn base_str(&self) -> Option<&str> {
        match self {
            Selector::Str(s) => Some(s.as_str()),
            Selector::Obj(o) => o.base.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelectorObj {
    #[serde(rename = "$", default)]
    pub base: Option<String>,
    #[serde(default)]
    pub attr: Option<String>,
    #[serde(default)]
    pub range: Option<String>,
    #[serde(default)]
    pub pick: Option<Vec<i64>>,
    #[serde(default)]
    pub exclude: Option<Vec<i64>>,
    #[serde(default)]
    pub exists: Option<bool>,
    #[serde(default)]
    pub eq: Option<Value>,
    #[serde(default)]
    pub ne: Option<Value>,
    #[serde(default)]
    pub gt: Option<Value>,
    #[serde(default)]
    pub lt: Option<Value>,
    #[serde(default)]
    pub contains: Option<Value>,
    #[serde(default)]
    pub absolute: Option<AbsoluteSpec>,
    #[serde(rename = "where", default)]
    pub where_: Option<Value>,
    #[serde(default)]
    pub map: Option<BTreeMap<String, Selector>>,
    #[serde(default)]
    pub blocks: Option<BlocksCfg>,
    #[serde(default)]
    pub paragraphs: Option<ParagraphsCfg>,
    #[serde(default)]
    pub strip: Option<Vec<Selector>>,
    #[serde(default)]
    pub protect: Option<ProtectSpec>,
    #[serde(default)]
    pub transform: Option<Vec<TransformStep>>,
    #[serde(default)]
    pub fallback: Option<Vec<Selector>>,
    #[serde(default)]
    pub flags: Option<String>,
    #[serde(rename = "typeMap", default)]
    pub type_map: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub optional: Option<bool>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum AbsoluteSpec {
    Bool(bool),
    Obj(AbsoluteObj),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AbsoluteObj {
    #[serde(default)]
    pub base: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for AbsoluteSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        match v {
            Value::Bool(b) => Ok(AbsoluteSpec::Bool(b)),
            Value::Object(_) => {
                let o = serde_json::from_value::<AbsoluteObj>(v).map_err(serde::de::Error::custom)?;
                Ok(AbsoluteSpec::Obj(o))
            }
            _ => Err(serde::de::Error::custom(
                "absolute 必须是 true 或 {\"base\": …}",
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlocksCfg {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(rename = "minLength", default)]
    pub min_length: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParagraphsCfg {
    #[serde(default)]
    pub split: Option<String>,
    #[serde(rename = "minLength", default)]
    pub min_length: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ProtectSpec {
    Literals(Vec<String>),
    Nodes(Box<Selector>),
}

impl<'de> Deserialize<'de> for ProtectSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        match &v {
            Value::Array(arr) if arr.iter().all(|x| x.is_string()) => Ok(ProtectSpec::Literals(
                arr.iter().map(|x| x.as_str().unwrap().to_string()).collect(),
            )),
            Value::Array(_) => Err(serde::de::Error::custom(
                "protect 数组必须是字符串列表",
            )),
            Value::Object(_) => {
                let s = serde_json::from_value::<Selector>(v).map_err(serde::de::Error::custom)?;
                Ok(ProtectSpec::Nodes(Box::new(s)))
            }
            _ => Err(serde::de::Error::custom(
                "protect 必须是字符串数组或选择器（§3.6.2）",
            )),
        }
    }
}

// ───────────────────────── transform 步骤 ─────────────────────────

/// 31 个管道步骤（S§3.6 / REFERENCE §5）。
pub const STEP_KEYS: &[&str] = &[
    "trim", "replace", "regex", "match", "prepend", "append", "absolute",
    "urlencode", "urldecode", "base64", "hex", "crypto", "font", "html",
    "json", "date", "number", "numChapter", "cjkNumber", "t2s", "s2t",
    "slice", "charset", "join", "split", "zip", "protect", "unique",
    "sort", "dropEmpty", "js",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepKind {
    Trim,
    Replace,
    Regex,
    Match,
    Prepend,
    Append,
    Absolute,
    Urlencode,
    Urldecode,
    Base64,
    Hex,
    Crypto,
    Font,
    Html,
    Json,
    Date,
    Number,
    NumChapter,
    CjkNumber,
    T2s,
    S2t,
    Slice,
    Charset,
    Join,
    Split,
    Zip,
    Protect,
    Unique,
    Sort,
    DropEmpty,
    Js,
}

impl StepKind {
    pub fn from_key(k: &str) -> Option<StepKind> {
        Some(match k {
            "trim" => StepKind::Trim,
            "replace" => StepKind::Replace,
            "regex" => StepKind::Regex,
            "match" => StepKind::Match,
            "prepend" => StepKind::Prepend,
            "append" => StepKind::Append,
            "absolute" => StepKind::Absolute,
            "urlencode" => StepKind::Urlencode,
            "urldecode" => StepKind::Urldecode,
            "base64" => StepKind::Base64,
            "hex" => StepKind::Hex,
            "crypto" => StepKind::Crypto,
            "font" => StepKind::Font,
            "html" => StepKind::Html,
            "json" => StepKind::Json,
            "date" => StepKind::Date,
            "number" => StepKind::Number,
            "numChapter" => StepKind::NumChapter,
            "cjkNumber" => StepKind::CjkNumber,
            "t2s" => StepKind::T2s,
            "s2t" => StepKind::S2t,
            "slice" => StepKind::Slice,
            "charset" => StepKind::Charset,
            "join" => StepKind::Join,
            "split" => StepKind::Split,
            "zip" => StepKind::Zip,
            "protect" => StepKind::Protect,
            "unique" => StepKind::Unique,
            "sort" => StepKind::Sort,
            "dropEmpty" => StepKind::DropEmpty,
            "js" => StepKind::Js,
            _ => return None,
        })
    }

    pub fn as_key(&self) -> &'static str {
        match self {
            StepKind::Trim => "trim",
            StepKind::Replace => "replace",
            StepKind::Regex => "regex",
            StepKind::Match => "match",
            StepKind::Prepend => "prepend",
            StepKind::Append => "append",
            StepKind::Absolute => "absolute",
            StepKind::Urlencode => "urlencode",
            StepKind::Urldecode => "urldecode",
            StepKind::Base64 => "base64",
            StepKind::Hex => "hex",
            StepKind::Crypto => "crypto",
            StepKind::Font => "font",
            StepKind::Html => "html",
            StepKind::Json => "json",
            StepKind::Date => "date",
            StepKind::Number => "number",
            StepKind::NumChapter => "numChapter",
            StepKind::CjkNumber => "cjkNumber",
            StepKind::T2s => "t2s",
            StepKind::S2t => "s2t",
            StepKind::Slice => "slice",
            StepKind::Charset => "charset",
            StepKind::Join => "join",
            StepKind::Split => "split",
            StepKind::Zip => "zip",
            StepKind::Protect => "protect",
            StepKind::Unique => "unique",
            StepKind::Sort => "sort",
            StepKind::DropEmpty => "dropEmpty",
            StepKind::Js => "js",
        }
    }

    pub fn all_keys() -> &'static [&'static str] {
        STEP_KEYS
    }
}

/// 一个管道步骤：恰好一个步骤键 + 参数；其余键进 `extra_keys`（E_SCHEMA）。
#[derive(Debug, Clone, Serialize)]
pub struct TransformStep {
    pub kind: StepKind,
    pub params: Value,
    /// 整个原始对象（错误提示用）。
    pub raw: Value,
    /// 除步骤键外出现的其它**步骤键**（一个对象写多步 → E_SCHEMA）。
    pub dup_step_keys: Vec<String>,
    /// 非步骤键的未知字段（W_UNKNOWN_FIELD）。
    pub extra_keys: Vec<String>,
}

impl<'de> Deserialize<'de> for TransformStep {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let map = v
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("transform 步骤必须是对象"))?;
        let mut kind = None;
        let mut params = Value::Null;
        let mut dup = vec![];
        let mut extra_keys = vec![];
        for (k, val) in map {
            if let Some(kk) = StepKind::from_key(k) {
                if kind.is_none() {
                    kind = Some(kk);
                    params = val.clone();
                } else {
                    dup.push(k.clone());
                }
            } else {
                extra_keys.push(k.clone());
            }
        }
        let kind = kind.ok_or_else(|| {
            serde::de::Error::custom("transform 步骤缺少步骤键（31 个合法键之一）")
        })?;
        Ok(TransformStep {
            kind,
            params,
            raw: v,
            dup_step_keys: dup,
            extra_keys,
        })
    }
}

// ───────────────────────── limits ─────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Budget {
    Int(i64),
    Obj(BudgetObj),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetObj {
    pub value: Option<i64>,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl<'de> Deserialize<'de> for Budget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        if let Some(i) = v.as_i64() {
            return Ok(Budget::Int(i));
        }
        let o = serde_json::from_value::<BudgetObj>(v).map_err(serde::de::Error::custom)?;
        Ok(Budget::Obj(o))
    }
}

impl Budget {
    pub fn value(&self) -> Option<i64> {
        match self {
            Budget::Int(i) => Some(*i),
            Budget::Obj(o) => o.value,
        }
    }
    pub fn on(&self) -> &str {
        match self {
            Budget::Int(_) => "reject",
            Budget::Obj(o) => o.on.as_deref().unwrap_or("reject"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Limits {
    #[serde(rename = "maxBytes", default)]
    pub max_bytes: Option<Budget>,
    #[serde(rename = "maxNodes", default)]
    pub max_nodes: Option<Budget>,
    #[serde(rename = "timeoutMs", default)]
    pub timeout_ms: Option<i64>,
    #[serde(rename = "maxRedirects", default)]
    pub max_redirects: Option<i64>,
    #[serde(rename = "maxPages", default)]
    pub max_pages: Option<Budget>,
    #[serde(rename = "maxParallel", default)]
    pub max_parallel: Option<i64>,
    #[serde(rename = "libSteps", default)]
    pub lib_steps: Option<i64>,
    #[serde(rename = "scriptTimeoutMs", default)]
    pub script_timeout_ms: Option<i64>,
    #[serde(rename = "mediaBytes", default)]
    pub media_bytes: Option<i64>,
    #[serde(rename = "loginCheckTtlMs", default)]
    pub login_check_ttl_ms: Option<i64>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: None,
            max_nodes: None,
            timeout_ms: None,
            max_redirects: None,
            max_pages: None,
            max_parallel: None,
            lib_steps: None,
            script_timeout_ms: None,
            media_bytes: None,
            login_check_ttl_ms: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Limits {
    /// 缺省值（S§8.1 注释与 ERRORS §4.2：单请求超时默认 20s）。
    pub fn timeout_ms_or_default(&self) -> i64 {
        self.timeout_ms.unwrap_or(20_000)
    }
    pub fn max_parallel_or_default(&self) -> i64 {
        self.max_parallel.unwrap_or(4)
    }
    pub fn login_check_ttl_or_default(&self) -> i64 {
        self.login_check_ttl_ms.unwrap_or(300_000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetaCfg {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub updated: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub changelog: Vec<Value>,
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

// ───────────────────────── 载体 ─────────────────────────

pub const CARRIERS: &[&str] = &["novel", "comic", "audio", "video", "file", "rss"];

pub fn is_carrier(s: &str) -> bool {
    CARRIERS.contains(&s)
}
