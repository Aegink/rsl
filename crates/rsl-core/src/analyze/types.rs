//! T1.7 管道类型推断（COMPILER §3.3）。
//! 裁决 D5（登记于 notes）：css/xpath 基座输出 NodeList，但运行期有隐式文本提取
//! （多命中 → 逐项取文本）。编译期：Node/NodeList 输入到字符串步骤一律放行（命中数
//! 静态未知，可能是 1）；**显式** StrList / Json / Bytes 等输入按签名严格判定 ——
//! `regex`/`match` 在 str[] 上 → E_TYPE（负例 #2 的触发面），join 在 Json 上 → E_TYPE。

use crate::error::{Code, Diagnostic};
use crate::ir::{BaseSel, StepIR};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Str,
    StrList,
    Node,
    NodeList,
    Json,
    JsonList,
    Bytes,
    Num,
    Bool,
    BlockList,
    Any,
}

impl Ty {
    pub fn name(&self) -> &'static str {
        match self {
            Ty::Str => "str",
            Ty::StrList => "str[]",
            Ty::Node => "node",
            Ty::NodeList => "node[]",
            Ty::Json => "json",
            Ty::JsonList => "json[]",
            Ty::Bytes => "bytes",
            Ty::Num => "num",
            Ty::Bool => "bool",
            Ty::BlockList => "block[]",
            Ty::Any => "any",
        }
    }
}

/// 基础选择器的输出类型（COMPILER §3.3 末行）。
pub fn base_out_type(base: &BaseSel) -> Ty {
    match base {
        BaseSel::Css(_) | BaseSel::Xpath(_) => Ty::NodeList,
        BaseSel::Json(p) => {
            if p.ends_with("[*]") || p.contains("[*]") {
                Ty::JsonList
            } else {
                Ty::Json
            }
        }
        BaseSel::Re { .. } => Ty::StrList,
        BaseSel::Text | BaseSel::OwnText | BaseSel::Html | BaseSel::Tag | BaseSel::Attr(_)
        | BaseSel::None => Ty::Str,
        BaseSel::Const(v) => match v {
            crate::ir::IrValue::Int(_) | crate::ir::IrValue::Float(_) => Ty::Num,
            crate::ir::IrValue::Bool(_) => Ty::Bool,
            crate::ir::IrValue::Str(_) => Ty::Str,
            _ => Ty::Json,
        },
        BaseSel::Var(_) | BaseSel::Let(_) | BaseSel::Session(_) | BaseSel::Token(_)
        | BaseSel::Cookie(_) | BaseSel::Cache(_) => Ty::Str,
        BaseSel::Lib(_) | BaseSel::Expr(_) => Ty::Any,
    }
}

fn type_err(path: &str, rule: &str, step: &str, got: Ty, need: &str, hint: &str) -> Diagnostic {
    Diagnostic::new(
        Code::EType,
        path,
        format!("步骤 {step} 需要 {need}，输入是 {}", got.name()),
        hint,
    )
    .rule(rule)
}

/// 单步类型变换。Err = E_TYPE 诊断。
pub fn step_apply(step: &StepIR, input: Ty, path: &str, rule: &str) -> Result<Ty, Diagnostic> {
    use StepIR::*;
    let node_lenient = |got: Ty| matches!(got, Ty::Node | Ty::NodeList);
    Ok(match step {
        Trim { .. } | Replace { .. } => match input {
            Ty::Str | Ty::StrList | Ty::Node | Ty::NodeList | Ty::Any => {
                if node_lenient(input) {
                    Ty::StrList
                } else {
                    input
                }
            }
            // D9：JSON 值在运行期多为字符串，宽化放行
            Ty::Json | Ty::JsonList => Ty::Str,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    step_key(step),
                    other,
                    "Str|Node",
                    "trim/replace 用于文本或节点",
                ))
            }
        },
        Regex { .. } | Match { .. } => match input {
            Ty::Str | Ty::Any => Ty::Str,
            // 节点输入：隐式文本提取（命中数未知，运行期逐项应用）→ 放行
            Ty::Node | Ty::NodeList => Ty::Any,
            // D9：JSON 字符串值逐项应用
            Ty::Json | Ty::JsonList => Ty::Str,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    step_key(step),
                    other,
                    "Str",
                    match other {
                        Ty::StrList => "先 join 或对项求值（{\"$\":\"text\"} 单项化）",
                        _ => "先加 {\"$\":\"text\"} 再做字符串步骤",
                    },
                ))
            }
        },
        Prepend(_) | Append(_) | Absolute { .. } | Urlencode { .. } | Urldecode { .. }
        | Font { .. } | Html | Date { .. } | NumChapter | CjkNumber(_) | T2s | S2t
        | Slice { .. } => match input {
            Ty::Str | Ty::Any => Ty::Str,
            Ty::Node | Ty::NodeList | Ty::StrList | Ty::Json | Ty::JsonList => Ty::Any, // 隐式提取/逐项应用
            other => {
                return Err(type_err(
                    path,
                    rule,
                    step_key(step),
                    other,
                    "Str",
                    "先加 {\"$\":\"text\"} 再做字符串步骤",
                ))
            }
        },
        Number { .. } => match input {
            Ty::Str | Ty::Any => Ty::Num,
            Ty::Node | Ty::NodeList | Ty::StrList | Ty::Json | Ty::JsonList => Ty::Any,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "number",
                    other,
                    "Str",
                    "number 把文本转数字",
                ))
            }
        },
        JsonStep => match input {
            Ty::Str | Ty::Any => Ty::Json,
            Ty::Node | Ty::NodeList => Ty::Any,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "json",
                    other,
                    "Str",
                    "json 步骤把字符串解析为 JSON；json 输入无需再解析",
                ))
            }
        },
        Base64(_) | Hex(_) | Crypto { .. } => match input {
            Ty::Str | Ty::Bytes | Ty::Any => Ty::Str,
            Ty::Node | Ty::NodeList | Ty::Json | Ty::JsonList => Ty::Any,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    step_key(step),
                    other,
                    "Str|Bytes",
                    "response:\"bytes\" 时先 transform.charset 再跑文本步骤",
                ))
            }
        },
        Charset(_) => match input {
            Ty::Bytes | Ty::Any => Ty::Str,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "charset",
                    other,
                    "Bytes",
                    "仅在 response:\"bytes\" 时 charset 有意义",
                ))
            }
        },
        Join(_) => match input {
            Ty::StrList | Ty::NodeList | Ty::JsonList | Ty::Json | Ty::Any | Ty::BlockList => Ty::Str,
            Ty::Node => Ty::Str,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "join",
                    other,
                    "str[]",
                    "join 用于列表；单值直接使用",
                ))
            }
        },
        Split(_) => match input {
            Ty::Str | Ty::Any | Ty::Node | Ty::NodeList | Ty::Json | Ty::JsonList => Ty::StrList,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "split",
                    other,
                    "Str",
                    "split 把字符串切成列表",
                ))
            }
        },
        Zip { .. } => match input {
            Ty::StrList | Ty::NodeList | Ty::JsonList | Ty::Any => Ty::StrList,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    "zip",
                    other,
                    "str[]",
                    "zip 对列表交叉合并；with 在父作用域求值",
                ))
            }
        },
        Unique | Sort { .. } | DropEmpty => match input {
            Ty::StrList | Ty::NodeList | Ty::JsonList | Ty::BlockList | Ty::Any => input,
            Ty::Node => Ty::NodeList,
            other => {
                return Err(type_err(
                    path,
                    rule,
                    step_key(step),
                    other,
                    "List",
                    "unique/sort/dropEmpty 作用于列表",
                ))
            }
        },
        Protect(_) => input,
        Js { .. } => Ty::Any,
    })
}

fn step_key(s: &StepIR) -> &'static str {
    match s {
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
        StepIR::Js { .. } => "js",
    }
}
