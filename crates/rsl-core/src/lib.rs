//! RSL 内核（rsl-core）。模块依赖硬约束（COMPILER §5 / 任务书 §1.2）：
//! `selector` 不得依赖 `http`；`exec` 不得依赖 `stage`；`analyze` 只读 `model` + `ir` + `expr`。

pub mod analyze;
pub mod build;
pub mod crypto;
pub mod error;
pub mod exec;
pub mod expr;
pub mod http;
pub mod ir;
pub mod model;
pub mod ports;
pub mod script;
pub mod selector;
pub mod stage;
pub mod testkit;
pub mod sugar;
pub mod webview;

pub const RSL_MAJOR: u32 = 2;
pub const RSL_MINOR: u32 = 0;
pub const VERSION: &str = "2.0.0";

/// 便捷入口：编译一份源。
pub fn compile(text: &str) -> analyze::CompileOutput {
    analyze::compile_str(text)
}
