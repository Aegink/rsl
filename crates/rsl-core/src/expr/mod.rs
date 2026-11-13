//! T4.1–T4.3 `expr:` 子语言：词法、Pratt 解析、编译期检查、求值（EXPRESSIONS.md）。
//! 唯一允许算术的位置；确定性计算，编译期可静态校验。

pub mod check;
pub mod eval;
pub mod parse;

pub use check::{compile, Declared, PerItem, Scope};
pub use eval::{EVal, ExprHost};
pub use parse::{parse, BinOp, Expr, FnId, UnOp, VarPath};
