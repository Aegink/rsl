//! T4.3 `expr:` 编译期检查（EXPRESSIONS.md §7）：
//! 变量可见性（E_REF）、函数存在（E_EXPR_FN）、feature/算法声明（E_UNSUPPORTED）、
//! 静态类型错误（E_TYPE）、长度/嵌套（E_EXPR_LIMIT）、逐章不确定性（E_COST）。

use super::parse::{parse, BinOp, Expr, FnId, VarPath};
use crate::error::{Code, Diagnostic};
use std::collections::BTreeSet;

/// 当前阶段可见的标量变量（引用图）。`paths` 为完整路径（如 `page`、`filter.mode`、`item.id`）。
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// 裸名/带前缀都按此集合查：`page`、`book.tocUrl`、`item.id`。
    pub paths: BTreeSet<String>,
    /// 通配根（如 map 作用域的 `item`）——任意子路径可见。
    pub wildcards: BTreeSet<String>,
    /// `let:` 已声明的名字（按声明顺序，前项可见）。
    pub lets: BTreeSet<String>,
    /// session 键不做编译期穷举（单次运行动态）；声明了 script/let 才允许读。
    pub session_ok: bool,
}

impl Scope {
    pub fn has(&self, ns: &str, path: &str) -> bool {
        match ns {
            "var" | "" => {
                if self.paths.contains(path) {
                    return true;
                }
                if let Some(root) = path.split('.').next() {
                    if self.wildcards.contains(root) {
                        return true;
                    }
                }
                false
            }
            "let" => self.lets.contains(path),
            "session" => self.session_ok,
            _ => false,
        }
    }
}

/// `requires` 声明（feature / 算法）。
#[derive(Debug, Clone, Default)]
pub struct Declared {
    pub features: BTreeSet<String>,
    pub crypto: BTreeSet<String>,
    pub has_login_or_cookie: bool,
}

/// 逐章规则标志：now()/rand()/cookie() → E_COST。
#[derive(Debug, Clone, Copy, Default)]
pub struct PerItem {
    pub enabled: bool,
}

/// 编译并检查。成功返回 AST；失败返回诊断。
pub fn compile(src: &str, path: &str, scope: &Scope, decl: &Declared, per_item: PerItem) -> Result<Expr, Diagnostic> {
    let e = parse(src, path)?;
    check_expr(&e, path, scope, decl, per_item)?;
    Ok(e)
}

pub fn compile_lenient(src: &str, path: &str) -> Result<Expr, Diagnostic> {
    parse(src, path)
}

fn check_expr(e: &Expr, path: &str, scope: &Scope, decl: &Declared, per_item: PerItem) -> Result<(), Diagnostic> {
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => Ok(()),
        Expr::Var(vp) => {
            let (ns, p) = match vp {
                VarPath::Bare(p) => ("", p.as_str()),
                VarPath::Ns(ns, p) => (ns.as_str(), p.as_str()),
            };
            if scope.has(ns, p) {
                Ok(())
            } else {
                Err(ref_undefined(path, vp))
            }
        }
        Expr::Unary(_, a) => check_expr(a, path, scope, decl, per_item),
        Expr::Ternary(c, t, f) => {
            check_expr(c, path, scope, decl, per_item)?;
            check_expr(t, path, scope, decl, per_item)?;
            check_expr(f, path, scope, decl, per_item)
        }
        Expr::Bin(op, a, b) => {
            check_expr(a, path, scope, decl, per_item)?;
            check_expr(b, path, scope, decl, per_item)?;
            // 静态可判类型错误：Num + {Bool|Null|Str} / {Bool|Null} 参与算术
            if matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod
            ) {
                let bad = |x: &Expr| matches!(x, Expr::Bool(_) | Expr::Null);
                let num_lit = |x: &Expr| matches!(x, Expr::Num(_));
                let str_lit = |x: &Expr| matches!(x, Expr::Str(_));
                let mismatch = match op {
                    BinOp::Add => {
                        (num_lit(a) && (bad(b) || str_lit(b)))
                            || (num_lit(b) && (bad(a) || str_lit(a)))
                    }
                    _ => (num_lit(a) && bad(b)) || (num_lit(b) && bad(a)),
                };
                if mismatch {
                    return Err(Diagnostic::new(
                        Code::EType,
                        path,
                        "静态可判的算术类型错误",
                        "`+` 规则：数字+数字、字符串+任意（拼接）、列表+列表；其余用 str()/num() 显式转换",
                    ));
                }
            }
            Ok(())
        }
        Expr::Call(fid, args) => {
            // 逐章不确定性
            if per_item.enabled && matches!(fid, FnId::Now | FnId::Rand | FnId::Cookie) {
                return Err(Diagnostic::new(
                    Code::ECost,
                    path,
                    format!("逐章规则禁止使用 {}", fid.name()),
                    "把该计算挪到 list 选择器的父规则或阶段 let 里一次算完",
                ));
            }
            // feature / 算法声明（裁决 D2：未声明 → E_UNSUPPORTED，与 T1.12 负例矩阵一致）
            if let Some((feat, alg)) = fid.requirement() {
                if *fid == FnId::Cookie {
                    if !decl.has_login_or_cookie {
                        return Err(Diagnostic::new(
                            Code::EUnsupported,
                            path,
                            "cookie() 需要源声明 login 或 cookie 块",
                            "声明 login（§4.5）或预置 cookie.initial",
                        ));
                    }
                } else {
                    if !decl.features.contains(feat) {
                        return Err(Diagnostic::new(
                            Code::EUnsupported,
                            path,
                            format!("feature `{feat}` 未声明（函数 {} 使用了它）", fid.name()),
                            format!("requires.features 加 \"{feat}\""),
                        ));
                    }
                    if let Some(alg) = alg {
                        if !decl.crypto.contains(alg) {
                            return Err(Diagnostic::new(
                                Code::EUnsupported,
                                path,
                                format!("crypto 算法 `{alg}` 未声明"),
                                format!("requires.crypto 加 \"{alg}\"（features 也要含 crypto）"),
                            ));
                        }
                    }
                    if *fid == FnId::Hmac {
                        // hmac(alg, key, s)：第一个参数若是字面量算法名，要求对应 hmac-<alg>
                        if let Some(Expr::Str(alg)) = args.first() {
                            let a = alg.to_lowercase().trim_start_matches("hmac-").to_string();
                            let key = format!("hmac-{a}");
                            if !decl.crypto.contains(&key) {
                                return Err(Diagnostic::new(
                                    Code::EUnsupported,
                                    path,
                                    format!("crypto 算法 `{key}` 未声明"),
                                    format!("requires.crypto 加 \"{key}\""),
                                ));
                            }
                        }
                    }
                }
            }
            for a in args {
                check_expr(a, path, scope, decl, per_item)?;
            }
            Ok(())
        }
    }
}

fn ref_undefined(path: &str, vp: &VarPath) -> Diagnostic {
    let (show, ns_kind) = match vp {
        VarPath::Bare(p) => (p.clone(), "变量"),
        VarPath::Ns(ns, p) => (format!("{ns}:{p}"), "变量"),
    };
    Diagnostic::new(
        Code::ERef,
        path,
        format!("{ns_kind} {show} 未定义或当前阶段不可见"),
        "对照引用图（COMPILER.md §3.2）：book.*/chapter.* 字段必须来自 out.map 声明；带 . 的嵌套标量先落 let",
    )
}

/// 收集表达式里用到的全部变量路径与函数（引用图/feature 分析用）。
pub fn collect(e: &Expr, vars: &mut BTreeSet<(String, String)>, fns: &mut BTreeSet<FnId>) {
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {}
        Expr::Var(vp) => {
            let (ns, p) = match vp {
                VarPath::Bare(p) => ("".to_string(), p.clone()),
                VarPath::Ns(ns, p) => (ns.clone(), p.clone()),
            };
            vars.insert((ns, p));
        }
        Expr::Unary(_, a) => collect(a, vars, fns),
        Expr::Ternary(c, t, f) => {
            collect(c, vars, fns);
            collect(t, vars, fns);
            collect(f, vars, fns);
        }
        Expr::Bin(_, a, b) => {
            collect(a, vars, fns);
            collect(b, vars, fns);
        }
        Expr::Call(fid, args) => {
            fns.insert(*fid);
            for a in args {
                collect(a, vars, fns);
            }
        }
    }
}
