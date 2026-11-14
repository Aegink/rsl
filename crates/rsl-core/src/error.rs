//! T1.14 错误类型：31 个错误码 + 5 个警告码（RSL-1.0 §11 / ERRORS.md）。
//! 每个诊断都带 `code/severity/stage/path/rule/message/hint/sample/degraded/retryable` 十字段，
//! `sample` 经过 secret 脱敏——永不出现 `vars.<k>.secret` 的值。

use serde::{Deserialize, Serialize};
use std::fmt;

/// 31 个错误码（E_）+ 5 个警告码（W_）。见 ERRORS.md §1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Code {
    // ── 编译期 ──
    EParse,
    ESchema,
    EEnum,
    ERef,
    EType,
    ERegex,
    ERegexUnsupported,
    ESelector,
    ECost,
    EExprSyntax,
    EExprFn,
    EExprLimit,
    ETemplate,
    // ── 加载期 ──
    EUnsupported,
    EVersionUnsupported,
    // ── 运行期 ──
    ENet,
    ETimeout,
    ECancelled,
    EHttp,
    ECharset,
    ELoginRequired,
    EChallenge,
    EDecode,
    ENoMatch,
    EEmpty,
    EFieldMissing,
    EVerify,
    EProtectLost,
    EBudget,
    EScript,
    EHostStage,
    // ── 警告 ──
    WUnknownField,
    WBudgetTruncated,
    WDegraded,
    WDeprecated,
    WZipLength,
}

impl Code {
    pub fn as_str(&self) -> &'static str {
        match self {
            Code::EParse => "E_PARSE",
            Code::ESchema => "E_SCHEMA",
            Code::EEnum => "E_ENUM",
            Code::ERef => "E_REF",
            Code::EType => "E_TYPE",
            Code::ERegex => "E_REGEX",
            Code::ERegexUnsupported => "E_REGEX_UNSUPPORTED",
            Code::ESelector => "E_SELECTOR",
            Code::ECost => "E_COST",
            Code::EExprSyntax => "E_EXPR_SYNTAX",
            Code::EExprFn => "E_EXPR_FN",
            Code::EExprLimit => "E_EXPR_LIMIT",
            Code::ETemplate => "E_TEMPLATE",
            Code::EUnsupported => "E_UNSUPPORTED",
            Code::EVersionUnsupported => "E_VERSION_UNSUPPORTED",
            Code::ENet => "E_NET",
            Code::ETimeout => "E_TIMEOUT",
            Code::ECancelled => "E_CANCELLED",
            Code::EHttp => "E_HTTP",
            Code::ECharset => "E_CHARSET",
            Code::ELoginRequired => "E_LOGIN_REQUIRED",
            Code::EChallenge => "E_CHALLENGE",
            Code::EDecode => "E_DECODE",
            Code::ENoMatch => "E_NO_MATCH",
            Code::EEmpty => "E_EMPTY",
            Code::EFieldMissing => "E_FIELD_MISSING",
            Code::EVerify => "E_VERIFY",
            Code::EProtectLost => "E_PROTECT_LOST",
            Code::EBudget => "E_BUDGET",
            Code::EScript => "E_SCRIPT",
            Code::EHostStage => "E_HOST_STAGE",
            Code::WUnknownField => "W_UNKNOWN_FIELD",
            Code::WBudgetTruncated => "W_BUDGET_TRUNCATED",
            Code::WDegraded => "W_DEGRADED",
            Code::WDeprecated => "W_DEPRECATED",
            Code::WZipLength => "W_ZIP_LENGTH",
        }
    }

    /// severity 由码决定：只有 E_EMPTY 是 info，W_ 是 warning，其余 error。
    pub fn severity(&self) -> Severity {
        match self {
            Code::EEmpty => Severity::Info,
            Code::WUnknownField
            | Code::WBudgetTruncated
            | Code::WDegraded
            | Code::WDeprecated
            | Code::WZipLength => Severity::Warning,
            _ => Severity::Error,
        }
    }

    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Code::WUnknownField
                | Code::WBudgetTruncated
                | Code::WDegraded
                | Code::WDeprecated
                | Code::WZipLength
        )
    }

    pub fn all_codes() -> &'static [&'static str] {
        &[
            "E_PARSE", "E_SCHEMA", "E_ENUM", "E_REF", "E_TYPE", "E_REGEX",
            "E_REGEX_UNSUPPORTED", "E_SELECTOR", "E_COST", "E_EXPR_SYNTAX",
            "E_EXPR_FN", "E_EXPR_LIMIT", "E_TEMPLATE", "E_UNSUPPORTED",
            "E_VERSION_UNSUPPORTED", "E_NET", "E_TIMEOUT", "E_CANCELLED",
            "E_HTTP", "E_CHARSET", "E_LOGIN_REQUIRED", "E_CHALLENGE",
            "E_DECODE", "E_NO_MATCH", "E_EMPTY", "E_FIELD_MISSING",
            "E_VERIFY", "E_PROTECT_LOST", "E_BUDGET", "E_SCRIPT",
            "E_HOST_STAGE", "W_UNKNOWN_FIELD", "W_BUDGET_TRUNCATED",
            "W_DEGRADED", "W_DEPRECATED", "W_ZIP_LENGTH",
        ]
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        })
    }
}

/// 机器可应用修复建议（TASK-BOOK-3 F1，对齐 rustc machine-applicable suggestion）。
/// V3-D8 只做确定性类：在源文档中把首个 `find` 精确替换为 `replacement`——
/// 有效性由"应用后编译通过"测试锁定；启发式修复不进本结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    /// 待替换的精确原文片段（如 `cs:`、`var:book.urll`）。
    pub find: String,
    /// 替换文本（如 `css:`、`var:book.url`）。
    pub replacement: String,
    /// 建议说明（如"最近似命名空间 css"）。
    pub message: String,
}

/// 结构化诊断对象（规范 §11 错误模型）。永不返回裸空串。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    /// 运行期必填（五阶段或 hostStages.<k>）；编译期/加载期为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// JSON 路径，如 `stages.toc.out.map.title`。
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub message: String,
    pub hint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degraded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    /// 运行期部分结果标记（E_CANCELLED 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
    /// 确定性修复建议（仅 E_REF/E_SELECTOR 两类产出，F1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(code: Code, path: impl Into<String>, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: code.severity(),
            stage: None,
            path: path.into(),
            rule: None,
            message: message.into(),
            hint: hint.into(),
            sample: None,
            degraded: None,
            retryable: None,
            partial: None,
            fix: None,
        }
    }

    /// 附带确定性修复建议（F1）。
    pub fn fix(mut self, find: impl Into<String>, replacement: impl Into<String>, message: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            find: find.into(),
            replacement: replacement.into(),
            message: message.into(),
        });
        self
    }

    pub fn stage(mut self, stage: impl Into<String>) -> Self {
        self.stage = Some(stage.into());
        self
    }

    pub fn rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }

    pub fn sample(mut self, sample: impl Into<String>) -> Self {
        let s: String = sample.into();
        self.sample = Some(truncate_sample(&s));
        self
    }

    pub fn degraded(mut self, v: bool) -> Self {
        self.degraded = Some(v);
        self
    }

    pub fn retryable(mut self, v: bool) -> Self {
        self.retryable = Some(v);
        self
    }

    pub fn partial(mut self, v: bool) -> Self {
        self.partial = Some(v);
        self
    }

    /// secret 脱敏：任何 secret 值不出现在 sample 里（由调用方传入脱敏函数后调用）。
    pub fn redact(&mut self, secrets: &[String]) {
        if let Some(s) = &mut self.sample {
            for sec in secrets {
                if !sec.is_empty() && s.contains(sec.as_str()) {
                    *s = s.replace(sec.as_str(), "****");
                }
            }
        }
        if let Some(r) = &mut self.rule {
            for sec in secrets {
                if !sec.is_empty() && r.contains(sec.as_str()) {
                    *r = r.replace(sec.as_str(), "****");
                }
            }
        }
        if let Some(fx) = &mut self.fix {
            for sec in secrets {
                if !sec.is_empty() {
                    if fx.find.contains(sec.as_str()) {
                        fx.find = fx.find.replace(sec.as_str(), "****");
                    }
                    if fx.message.contains(sec.as_str()) {
                        fx.message = fx.message.replace(sec.as_str(), "****");
                    }
                }
            }
        }
    }
}

fn truncate_sample(s: &str) -> String {
    const MAX: usize = 240;
    let s = s.replace(['\n', '\r', '\t'], " ");
    let mut out: String = s.chars().take(MAX).collect();
    if s.chars().count() > MAX {
        out.push('…');
    }
    out
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(st) = &self.stage {
            write!(f, " @{}", st)?;
        }
        write!(f, " at {}", self.path)?;
        write!(f, "\n  {}", self.message)?;
        if let Some(r) = &self.rule {
            write!(f, "\n  rule: {}", r)?;
        }
        write!(f, "\n  hint: {}", self.hint)?;
        if let Some(s) = &self.sample {
            write!(f, "\n  sample: {}", s)?;
        }
        Ok(())
    }
}

/// 运行期错误（区别于编译期诊断集合）。
pub type RslResult<T> = Result<T, Diagnostic>;

pub fn err(code: Code, path: &str, msg: &str, hint: &str) -> Diagnostic {
    Diagnostic::new(code, path, msg, hint)
}

impl From<serde_json::Error> for Diagnostic {
    /// E_PARSE：JSON 语法错误（serde 自带行列号）。
    fn from(e: serde_json::Error) -> Self {
        Diagnostic::new(
            Code::EParse,
            "$",
            format!("{}", e),
            "检查 JSON 语法：尾逗号、单引号、未转义反斜杠、BOM；正则 \\d 必须写成 \\\\d",
        )
        .retryable(false)
    }
}
