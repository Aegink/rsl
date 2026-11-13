//! T5.1–T5.3 测试工具链：fixture 录制（record）、断言引擎（YAML 用例 + 证据等级）、lint 20 条映射。

pub mod assert;
pub mod lint;
pub mod record;

use crate::error::Diagnostic;
use serde_json::Value;

/// 证据等级（S§13.2）：低等级通过不得写成高等级可用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    L0,
    L1,
    L2,
    L3,
    L4,
    L5,
}

impl Level {
    pub fn parse(s: &str) -> Level {
        match s {
            "L0" => Level::L0,
            "L1" => Level::L1,
            "L2" => Level::L2,
            "L3" | "l3" | "" => Level::L3,
            "L4" => Level::L4,
            "L5" => Level::L5,
            _ => Level::L3,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Level::L0 => "L0",
            Level::L1 => "L1",
            Level::L2 => "L2",
            Level::L3 => "L3",
            Level::L4 => "L4",
            Level::L5 => "L5",
        }
    }
}

/// 测试用例（S§13.2 YAML 格式）。
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub stage: String,
    pub level: Level,
    pub vars: Value,
    pub fixture: Option<String>,
    pub assert_exprs: Vec<String>,
    pub expect_error: Option<String>,
    pub at: Option<String>,
    /// merge 用例：多源 fixture 目录。
    pub merge_glob: Option<String>,
}

/// 断言结果。
#[derive(Debug, Clone)]
pub struct CaseResult {
    pub name: String,
    pub ok: bool,
    pub level: Level,
    pub detail: String,
}

pub fn all_ok(results: &[CaseResult]) -> bool {
    results.iter().all(|r| r.ok)
}

pub fn summarize(results: &[CaseResult]) -> String {
    let pass = results.iter().filter(|r| r.ok).count();
    let mut lines = vec![format!("—— {}/{} 用例通过 ——", pass, results.len())];
    for r in results {
        lines.push(format!(
            "  {} [{}] {}: {}",
            if r.ok { "✓" } else { "✗" },
            r.level.name(),
            r.name,
            r.detail
        ));
    }
    lines.join("\n")
}

/// 诊断序列化（record/trace 落盘用）。
pub fn diag_json(d: &Diagnostic) -> Value {
    serde_json::json!({
        "code": d.code.as_str(),
        "severity": d.severity.to_string(),
        "stage": d.stage,
        "path": d.path,
        "rule": d.rule,
        "message": d.message,
        "hint": d.hint,
        "degraded": d.degraded,
        "retryable": d.retryable,
    })
}
