//! T1.13 IR：`Step` 枚举 + `CompiledSource`（可 bincode 序列化落盘，二次加载跳过解析与语义分析）。
//! 编译期不落运行期缓存（regex/CSS/JSONPath 在执行期惰性编译并 memo，即"选择器实例化"）。

use crate::expr::Expr;
use crate::sugar::Seg;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// bincode 兼容的 JSON 值（serde_json::Value 依赖 deserialize_any，bincode 不支持）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IrValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<IrValue>),
    Map(BTreeMap<String, IrValue>),
}

impl IrValue {
    pub fn from_json(v: &serde_json::Value) -> IrValue {
        match v {
            serde_json::Value::Null => IrValue::Null,
            serde_json::Value::Bool(b) => IrValue::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    IrValue::Int(i)
                } else {
                    IrValue::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => IrValue::Str(s.clone()),
            serde_json::Value::Array(a) => IrValue::Arr(a.iter().map(IrValue::from_json).collect()),
            serde_json::Value::Object(m) => IrValue::Map(
                m.iter().map(|(k, x)| (k.clone(), IrValue::from_json(x))).collect(),
            ),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            IrValue::Null => serde_json::Value::Null,
            IrValue::Bool(b) => serde_json::Value::Bool(*b),
            IrValue::Int(i) => serde_json::json!(i),
            IrValue::Float(f) => serde_json::json!(f),
            IrValue::Str(s) => serde_json::Value::String(s.clone()),
            IrValue::Arr(a) => serde_json::Value::Array(a.iter().map(|x| x.to_json()).collect()),
            IrValue::Map(m) => serde_json::Value::Object(
                m.iter().map(|(k, x)| (k.clone(), x.to_json())).collect(),
            ),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            IrValue::Str(s) => Some(s),
            _ => None,
        }
    }
}

// ───────────────────────── 选择器 IR ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaseSel {
    Css(String),
    Xpath(String),
    Json(String),
    Re { source: String, flags: String },
    Text,
    OwnText,
    Html,
    Tag,
    Attr(String),
    Const(IrValue),
    Var(String),
    Let(String),
    Session(String),
    Token(String),
    Cookie(String),
    Cache(String),
    Lib(String),
    Expr(Expr),
    /// `$` 省略 = `text`（S§3.1）。
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CondOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Contains,
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cond {
    pub op: CondOp,
    pub val: IrValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WhereIR {
    Has(Box<SelectorIR>),
    Not(Box<WhereIR>),
    /// `text`: 正则匹配文本
    Text(String),
    Cmp { op: CondOp, val: IrValue },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtectIR {
    Literals(Vec<String>),
    Nodes(Box<SelectorIR>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectorIR {
    /// 原始文本（诊断用）。
    pub raw: String,
    pub base: BaseSel,
    pub attr: Option<String>,
    pub exists: bool,
    /// range: (start, end, step)
    pub range: Option<(Option<i64>, Option<i64>, Option<i64>)>,
    pub pick: Option<Vec<i64>>,
    pub exclude: Vec<i64>,
    pub where_: Option<WhereIR>,
    pub map: Vec<(String, Box<SelectorIR>)>,
    /// blocks: (mode, min_length)
    pub blocks: Option<(String, i64)>,
    /// paragraphs: (split, min_length)
    pub paragraphs: Option<(String, i64)>,
    pub strip: Vec<Box<SelectorIR>>,
    pub protect: Option<ProtectIR>,
    pub transform: Vec<StepIR>,
    pub fallback: Vec<Box<SelectorIR>>,
    pub flags: String,
    pub type_map: Option<BTreeMap<String, String>>,
    pub optional: bool,
    pub default: Option<IrValue>,
    pub limit: Option<usize>,
    pub cond: Option<Cond>,
}

// ───────────────────────── 管道步骤 IR ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    Pure,
    Ctx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CryptoMode {
    Encrypt,
    Decrypt,
}

/// 参数值编译产物：expr / 选择器 / 字面量 / 模板。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamExpr {
    Expr(Expr),
    Selector(Box<SelectorIR>),
    Template(Vec<Seg>),
    Raw(IrValue),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepIR {
    Trim { chars: Option<Vec<String>> },
    Replace { pairs: Vec<(String, String)> }, // from（可带 re: 前缀）→ to
    Regex { src: String, repl: String, all: bool },
    Match { src: String, n: usize, group: Option<String> },
    Prepend(String),
    Append(String),
    Absolute { base: Option<ParamExpr> },
    Urlencode { charset: Option<String> },
    Urldecode { charset: Option<String> },
    Base64(bool), // true=encode
    Hex(bool),
    Crypto { alg: String, mode: CryptoMode, key: ParamExpr, iv: Option<ParamExpr>, padding: Option<String> },
    Font { map_fn: String, reference: Option<ParamExpr> },
    Html,
    JsonStep,
    Date { from: String, to: String, tz: i64 },
    Number { unit: bool },
    NumChapter,
    CjkNumber(bool), // true = "to"
    T2s,
    S2t,
    Slice { start: Option<i64>, end: Option<i64>, step: Option<i64> },
    Charset(String),
    Join(String),
    Split(String),
    Zip { with: Box<SelectorIR>, sep: String },
    Protect(ProtectIR),
    Unique,
    Sort { desc: bool },
    DropEmpty,
    Js { fn_name: String, tier: Tier },
}

// ───────────────────────── 请求 IR ─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RespKind {
    Html,
    Json,
    Text,
    Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestIR {
    pub url: Option<Vec<Seg>>,
    pub method: String,
    pub params: Vec<(String, ParamExpr)>,
    pub body: Option<BodyIR>,
    pub headers: Vec<(String, Vec<Seg>)>,
    pub charset: Option<String>,
    pub encode_params: Option<String>,
    pub encode_body: Option<String>,
    pub response: RespKind,
    pub engine: String, // http | webview | auto
    pub web_js: Option<WebJsIR>,
    pub sniff: Option<String>,
    pub auto_upgrade_if: Option<Box<SelectorIR>>,
    pub skip: bool,
    pub retry: Option<RetryIR>,
    pub timeout: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyIR {
    pub kind: String, // form | json | raw | multipart
    pub data: BTreeMap<String, ParamExpr>,
    pub raw: Option<ParamExpr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryIR {
    pub times: i64,
    pub backoff: String, // none | linear | exponential
    pub on: Vec<i64>,
}

// ───────────────────────── let / 分页 / 脚本 ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LetIR {
    Expr(Expr),
    Selector(Box<SelectorIR>),
    Lib(String),
    Request { req: RequestIR, take: Option<Box<SelectorIR>> },
    Parallel { items: Vec<RequestIR>, limit: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaginateKind {
    Page { page_var: String },
    Next { sels: Vec<SelectorIR> },
    Cursor { take: Box<SelectorIR>, into: String },
    Enumerate { values: Vec<i64>, join_with: Option<String>, into: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginateIR {
    pub kind: PaginateKind,
    pub carry: Vec<String>,
    pub max: Option<i64>,
    pub stop_empty_path: Option<String>,
    pub stop_no_new: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookIR {
    pub fn_name: String,
    pub ctx_tier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptHooksIR {
    pub before: Option<HookIR>,
    pub on_response: Option<HookIR>,
    pub after: Option<HookIR>,
}

// ───────────────────────── 契约 IR ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyIR {
    pub contains: Option<Vec<Seg>>,
    pub min_items: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackIR {
    pub kind: String,
    pub format: String,
    pub url: Option<Box<SelectorIR>>,
    pub text: Option<Box<SelectorIR>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedIR {
    pub items: Box<SelectorIR>,
    pub map: Vec<(String, SelectorIR)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutIR {
    pub list: Option<Box<SelectorIR>>,
    pub map: Vec<(String, SelectorIR)>,
    pub empty_ok: bool,
    pub verify: Option<VerifyIR>,
    pub title: Option<Box<SelectorIR>>,
    pub content: Option<Box<SelectorIR>>,
    pub images: Option<Box<SelectorIR>>,
    pub audio: Option<Box<SelectorIR>>,
    pub video: Option<Box<SelectorIR>>,
    pub files: Option<Box<SelectorIR>>,
    pub feed: Option<FeedIR>,
    pub tracks: Vec<TrackIR>,
    pub next_page: Option<Box<SelectorIR>>,
    pub pay_action: Option<Box<SelectorIR>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TocOrder {
    Auto,
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dedupe {
    Url,
    Title,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterIR {
    pub key: String,
    pub label: Option<String>,
    pub default: Option<IrValue>,
    pub persist: bool,
    pub options: Vec<(IrValue, Option<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupIR {
    pub title: String,
    pub url: Option<String>,
    pub params: BTreeMap<String, IrValue>,
    pub when: BTreeMap<String, IrValue>,
    pub groups: Vec<GroupIR>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageIR {
    pub name: String,
    pub lets: Vec<(String, LetIR)>,
    pub request: Option<RequestIR>,
    pub parse: Option<Box<SelectorIR>>,
    pub out: OutIR,
    pub paginate: Option<PaginateIR>,
    pub script: ScriptHooksIR,
    pub on_error: Option<String>,
    pub timeout_ms: Option<i64>,
    pub requires_login: bool,
    pub order: Option<TocOrder>,
    pub dedupe: Option<Dedupe>,
    pub reuse_detail_html: bool,
    pub direct: bool,
    pub filters: Vec<FilterIR>,
    pub groups: Vec<GroupIR>,
    pub groups_from: Option<Box<SelectorIR>>,
}

// ───────────────────────── 登录 IR ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginFieldIR {
    pub key: String,
    pub label: Option<String>,
    pub secret: bool,
    pub default: Option<IrValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitIR {
    pub req: RequestIR,
    pub ok: Box<SelectorIR>,
    pub token: Option<Box<SelectorIR>>,
    pub cookie: Option<Box<SelectorIR>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginStepIR {
    pub id: String,
    pub fields: Vec<LoginFieldIR>,
    pub submit: Option<SubmitIR>,
    pub next_to: Option<String>,
    pub next_when: Option<Box<SelectorIR>>,
}

/// C2：webJs 归一化 IR。ctx=true 时宿主向页面暴露受控 __ctx 桥（能力声明制）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebJsIR {
    pub code: String,
    pub ctx: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginIR {
    pub login_type: String,
    /// 多步：拓扑序排列；单步：长度 1（由 fields+submit 合成）。
    pub steps: Vec<LoginStepIR>,
    pub check_req: Option<RequestIR>,
    pub check_ok: Option<Box<SelectorIR>>,
    pub headers: Vec<(String, Vec<Seg>)>,
    pub is_multi: bool,
    /// A2：WebView 交互登录块（entry + success）。
    pub webview: Option<WebviewLoginIR>,
    /// B1：机械重登次数上限（0 = 关闭）。
    pub relogin_max: i64,
}

/// A2 IR：entry URL + 成功判定选择器。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebviewLoginIR {
    pub entry: String,
    pub success: Box<SelectorIR>,
}

// ───────────────────────── 全局块 IR ─────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheIR {
    pub name: String,
    pub req: Option<RequestIR>,
    pub take: Option<Box<SelectorIR>>,
    pub key: Option<ParamExpr>,
    pub ttl: i64,
    pub ttl_partial: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaIR {
    pub headers: Vec<(String, Vec<Seg>)>,
    pub decode_fn: Option<String>,
    pub cache_ttl: i64,
    pub max_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeIR {
    pub key: Vec<String>,
    pub normalize: Vec<String>,
    pub cross_bucket: bool,
    pub order: Vec<String>,
    pub weight: f64,
    pub min_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibIR {
    pub pure: String,
    pub ctx: String,
    pub pure_fns: Vec<String>,
    pub ctx_fns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateIR {
    pub window_ms: i64,
    pub max: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpIR {
    pub headers: Vec<(String, Vec<Seg>)>,
    pub timeout: i64,
    pub retry: RetryIR,
    pub rate: Option<RateIR>,
    pub redirect_max: i64,
    pub cookies_mode: String, // none | session | persistent
    pub proxy: Option<String>,
    pub dns: BTreeMap<String, String>,
    pub tls_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LimitsIR {
    pub max_bytes: i64,
    pub max_bytes_on: String,
    pub max_nodes: i64,
    pub max_nodes_on: String,
    pub timeout_ms: i64,
    pub max_redirects: i64,
    pub max_pages: i64,
    pub max_pages_on: String,
    pub max_parallel: i64,
    pub lib_steps: i64,
    pub script_timeout_ms: i64,
    pub media_bytes: i64,
    pub login_check_ttl_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarIR {
    pub value: IrValue,
    pub label: Option<String>,
    pub persist: bool,
    pub secret: bool,
    pub options: Vec<IrValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequiresIR {
    pub engine: Option<String>,
    pub features: Vec<String>,
    pub crypto: Vec<String>,
    pub selector_css: Option<String>,
    pub selector_xpath: Option<String>,
    pub selector_jsonpath: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSource {
    pub rsl_version: String,
    pub id: String,
    pub name: String,
    pub carrier: String,
    pub base_url: String,
    pub match_detail: Option<String>,
    pub lang: Option<String>,
    pub version: Option<String>,
    pub requires: RequiresIR,
    pub http: HttpIR,
    pub vars: BTreeMap<String, VarIR>,
    pub cookie_initial: Vec<(String, String)>,
    pub cookie_domain: Option<String>,
    pub cookie_persist: bool,
    pub cache: Vec<CacheIR>,
    pub media: Option<MediaIR>,
    pub login: Option<LoginIR>,
    pub merge: MergeIR,
    pub lib: LibIR,
    pub stages: BTreeMap<String, StageIR>,
    pub host_stages: BTreeMap<String, StageIR>,
    pub limits: LimitsIR,
    /// x- 保留字段（往返不丢，S§9.4）。
    pub x_fields: BTreeMap<String, IrValue>,
}

impl CompiledSource {
    /// bincode 往返（T1.13 验收）。
    pub fn to_bincode(&self) -> Result<Vec<u8>, String> {
        bincode::serialize(self).map_err(|e| e.to_string())
    }

    pub fn from_bincode(bytes: &[u8]) -> Result<Self, String> {
        bincode::deserialize(bytes).map_err(|e| e.to_string())
    }

    pub fn stage(&self, name: &str) -> Option<&StageIR> {
        self.stages
            .get(name)
            .or_else(|| self.host_stages.get(name))
    }
}
