//! 运行期值模型（T3.1/T3.5）。类型格：Str|StrList|Node|NodeList|Json|JsonList|Bytes|Num|Bool|BlockList。

use serde_json::Value;

/// HTML 文档容器（节点 id 指向 scraper 的 ego-tree）。
#[derive(Clone)]
pub struct HtmlDoc {
    pub html: scraper::Html,
}

impl HtmlDoc {
    pub fn parse(text: &str) -> HtmlDoc {
        HtmlDoc {
            html: scraper::Html::parse_document(text),
        }
    }
    pub fn parse_fragment(text: &str) -> HtmlDoc {
        HtmlDoc {
            html: scraper::Html::parse_fragment(text),
        }
    }
}

/// 文档（阶段根作用域）。
pub enum Doc {
    Html(HtmlDoc),
    Json(Value),
    Text(String),
    Bytes(Vec<u8>),
}

/// 正文块（图文混排，S§6.6.3）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Block {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// 运行期值。
#[derive(Debug, Clone)]
pub enum Val {
    Str(String),
    StrList(Vec<String>),
    Node(ego_tree::NodeId),
    NodeList(Vec<ego_tree::NodeId>),
    Json(Value),
    Bytes(Vec<u8>),
    Num(f64),
    Bool(bool),
    BlockList(Vec<Block>),
    Empty,
}

impl Val {
    /// 标量化（单元素列表 → 元素；多元素 → join "\n"）。
    pub fn scalarize(self) -> Val {
        match self {
            Val::StrList(mut l) => match l.len() {
                0 => Val::Str(String::new()),
                1 => Val::Str(l.remove(0)),
                _ => Val::Str(l.join("\n")),
            },
            Val::NodeList(mut l) => match l.len() {
                0 => Val::Str(String::new()),
                1 => Val::Node(l.remove(0)),
                _ => Val::Node(l.remove(0)), // 多命中节点标量化取首节点（文本提取在 Eval 层）
            },
            other => other,
        }
    }

    pub fn to_text(&self) -> String {
        match self {
            Val::Str(s) => s.clone(),
            Val::StrList(l) => l.join("\n"),
            Val::Num(n) => crate::expr::eval::fmt_num(*n),
            Val::Bool(b) => b.to_string(),
            Val::Json(v) => match v {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            },
            Val::Empty => String::new(),
            _ => String::new(),
        }
    }

    pub fn is_empty_val(&self) -> bool {
        match self {
            Val::Str(s) => s.is_empty(),
            Val::StrList(l) => l.is_empty(),
            Val::NodeList(l) => l.is_empty(),
            Val::Json(Value::Null) => true,
            Val::BlockList(l) => l.is_empty(),
            Val::Empty => true,
            _ => false,
        }
    }

    /// 进 JSON 契约输出。
    pub fn to_json(&self) -> Value {
        match self {
            Val::Str(s) => Value::String(s.clone()),
            Val::StrList(l) => Value::Array(l.iter().map(|s| Value::String(s.clone())).collect()),
            Val::Num(n) => {
                // 跨端字节一致：整数值 f64 输出为整数（serde "5302500.0" vs JS "5302500"）
                if n.fract() == 0.0 && n.abs() < 9.0e15 {
                    serde_json::json!(*n as i64)
                } else {
                    serde_json::json!(n)
                }
            }
            Val::Bool(b) => Value::Bool(*b),
            Val::Json(v) => v.clone(),
            Val::BlockList(l) => serde_json::to_value(l).unwrap_or(Value::Null),
            Val::Bytes(_) => Value::String("<bytes>".into()),
            Val::Empty => Value::Null,
            _ => Value::Null,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Val::Str(_) => "str",
            Val::StrList(_) => "str[]",
            Val::Node(_) => "node",
            Val::NodeList(_) => "node[]",
            Val::Json(v) => {
                if v.is_array() {
                    "json[]"
                } else {
                    "json"
                }
            }
            Val::Bytes(_) => "bytes",
            Val::Num(_) => "num",
            Val::Bool(_) => "bool",
            Val::BlockList(_) => "block[]",
            Val::Empty => "empty",
        }
    }
}

/// 把选择器对象输出收敛为契约字段值。
pub fn finalize(v: Val) -> Value {
    match v {
        Val::Node(_) => Value::String(String::new()),
        Val::NodeList(l) => {
            if l.is_empty() {
                Value::Null
            } else {
                Value::String(String::new())
            }
        }
        other => other.to_json(),
    }
}
