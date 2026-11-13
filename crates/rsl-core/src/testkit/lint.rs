//! T5.3 `rsl lint` 20 条 ↔ 编译阶段映射（S§13.3 / COMPILER §3.9）。
//! lint 不联网，跑的就是 analyze；20 条规则一一映射，每条可被单测覆盖。

use crate::analyze::compile_str;
use crate::error::{Code, Diagnostic};

/// lint 规则编号（S§13.3 的 1–20）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleId {
    R1SelectorNs,
    R2VarRef,
    R3LetDeclared,
    R4RangePick,
    R5BlocksParagraphs,
    R6PaginateKind,
    R7PageVarEnumerate,
    R8DirectRequest,
    R9LoginStepsFields,
    R10StepGraph,
    R11PipelineType,
    R12PerItemCost,
    R13RegexSubset,
    R14FeatureDeclared,
    R15LibTier,
    R16OnResponseHttp,
    R17ReuseNeedsDetail,
    R18MergeKey,
    R19ReservedFieldType,
    R20UnknownField,
}

impl RuleId {
    pub fn as_num(&self) -> u8 {
        match self {
            RuleId::R1SelectorNs => 1,
            RuleId::R2VarRef => 2,
            RuleId::R3LetDeclared => 3,
            RuleId::R4RangePick => 4,
            RuleId::R5BlocksParagraphs => 5,
            RuleId::R6PaginateKind => 6,
            RuleId::R7PageVarEnumerate => 7,
            RuleId::R8DirectRequest => 8,
            RuleId::R9LoginStepsFields => 9,
            RuleId::R10StepGraph => 10,
            RuleId::R11PipelineType => 11,
            RuleId::R12PerItemCost => 12,
            RuleId::R13RegexSubset => 13,
            RuleId::R14FeatureDeclared => 14,
            RuleId::R15LibTier => 15,
            RuleId::R16OnResponseHttp => 16,
            RuleId::R17ReuseNeedsDetail => 17,
            RuleId::R18MergeKey => 18,
            RuleId::R19ReservedFieldType => 19,
            RuleId::R20UnknownField => 20,
        }
    }

    /// 错误码 → 规则映射（lint #19 由编译器 E_SCHEMA 的保留字段检查产出）。
    pub fn from_code(code: Code, path: &str, message: &str) -> Option<RuleId> {
        Some(match code {
            Code::ESelector => RuleId::R1SelectorNs,
            Code::ERef => {
                if path.contains("merge.key") {
                    RuleId::R18MergeKey
                } else if message.contains("let") {
                    RuleId::R3LetDeclared
                } else if path.contains("next.to") {
                    RuleId::R10StepGraph
                } else {
                    RuleId::R2VarRef
                }
            }
            Code::ESchema => {
                if message.contains("range") && message.contains("pick") {
                    RuleId::R4RangePick
                } else if message.contains("blocks") && message.contains("paragraphs") {
                    RuleId::R5BlocksParagraphs
                } else if path.contains("paginate") && message.contains("kind") {
                    RuleId::R6PaginateKind
                } else if message.contains("pageVar") && message.contains("enumerate") {
                    RuleId::R7PageVarEnumerate
                } else if message.contains("direct") {
                    RuleId::R8DirectRequest
                } else if message.contains("steps") && message.contains("fields") {
                    RuleId::R9LoginStepsFields
                } else if message.contains("环") || message.contains("重复") || message.contains("next.to") {
                    RuleId::R10StepGraph
                } else if message.contains("reuseDetailHtml") {
                    RuleId::R17ReuseNeedsDetail
                } else if message.contains("typeMap") {
                    RuleId::R19ReservedFieldType
                } else if message.contains("互斥") && message.contains("登录") {
                    RuleId::R9LoginStepsFields
                } else {
                    RuleId::R19ReservedFieldType
                }
            }
            Code::EType => RuleId::R11PipelineType,
            Code::ECost => RuleId::R12PerItemCost,
            Code::ERegex | Code::ERegexUnsupported => RuleId::R13RegexSubset,
            Code::EUnsupported => RuleId::R14FeatureDeclared,
            Code::EScript => {
                if message.contains("ctx.http") {
                    RuleId::R16OnResponseHttp
                } else {
                    RuleId::R15LibTier
                }
            }
            Code::WUnknownField => RuleId::R20UnknownField,
            _ => return None,
        })
    }
}

/// lint 结果条目。
#[derive(Debug, Clone)]
pub struct LintFinding {
    pub rule: Option<RuleId>,
    pub diag: Diagnostic,
}

pub struct LintReport {
    pub findings: Vec<LintFinding>,
    pub ok: bool,
}

/// 跑 lint：编译期全套检查 + 20 条规则映射。
pub fn lint(text: &str) -> LintReport {
    let out = compile_str(text);
    let findings = out
        .diagnostics
        .iter()
        .map(|d| LintFinding {
            rule: RuleId::from_code(d.code, &d.path, &d.message),
            diag: d.clone(),
        })
        .collect();
    LintReport {
        ok: out.ok(),
        findings,
    }
}

/// 20 条规则清单（报告用）。
pub const RULE_TABLE: &[(&str, &str)] = &[
    ("1", "选择器必须有命名空间"),
    ("2", "var: 引用必须在引用图内"),
    ("3", "let: 必须在同阶段先声明"),
    ("4", "range 与 pick 互斥"),
    ("5", "blocks 与 paragraphs 互斥"),
    ("6", "paginate.kind 与字段匹配"),
    ("7", "pageVar 与 enumerate 互斥"),
    ("8", "direct 与非 skip 的 request 互斥"),
    ("9", "login.steps 与顶层 fields 互斥"),
    ("10", "login.steps[].next.to 必须存在；steps[].id 必须唯一"),
    ("11", "管道类型推断"),
    ("12", "逐章规则白名单"),
    ("13", "正则 RE2 子集"),
    ("14", "features / crypto 声明"),
    ("15", "lib: 函数存在性与档位"),
    ("16", "onResponse 不得调 ctx.http"),
    ("17", "reuseDetailHtml 必须有 detail 阶段"),
    ("18", "merge.key 字段必须存在于 search.out.map"),
    ("19", "保留字段类型"),
    ("20", "x- 前缀之外的未知字段"),
];
