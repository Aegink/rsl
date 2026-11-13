//! `expr:` 求值（EXPRESSIONS.md §4–6）。宿主通过 `ExprHost` 提供变量与需声明函数。

use super::parse::{BinOp, Expr, FnId, UnOp, VarPath};
use crate::error::{Code, Diagnostic};

/// 表达式运行期值。
#[derive(Debug, Clone, PartialEq)]
pub enum EVal {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    List(Vec<EVal>),
}

impl EVal {
    pub fn truthy(&self) -> bool {
        match self {
            EVal::Num(n) => *n != 0.0,
            EVal::Str(s) => !s.is_empty(),
            EVal::Bool(b) => *b,
            EVal::Null => false,
            EVal::List(l) => !l.is_empty(),
        }
    }

    /// str()：Num 去掉多余小数，Bool→"true"/"false"，Null→""。
    pub fn to_str(&self) -> String {
        match self {
            EVal::Num(n) => fmt_num(*n),
            EVal::Str(s) => s.clone(),
            EVal::Bool(b) => b.to_string(),
            EVal::Null => String::new(),
            EVal::List(_) => String::new(), // 不做数组↔字符串互转（§4）
        }
    }

    pub fn to_num(&self) -> Option<f64> {
        match self {
            EVal::Num(n) => Some(*n),
            EVal::Str(s) => s.trim().parse::<f64>().ok(),
            EVal::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            EVal::Null => None,
            EVal::List(_) => None,
        }
    }

    pub fn from_json(v: &serde_json::Value) -> EVal {
        match v {
            serde_json::Value::Null => EVal::Null,
            serde_json::Value::Bool(b) => EVal::Bool(*b),
            serde_json::Value::Number(n) => EVal::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => EVal::Str(s.clone()),
            serde_json::Value::Array(a) => {
                EVal::List(a.iter().map(EVal::from_json).collect())
            }
            serde_json::Value::Object(_) => EVal::Str(v.to_string()), // 对象收敛为字符串
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            EVal::Num(n) => serde_json::json!(n),
            EVal::Str(s) => serde_json::Value::String(s.clone()),
            EVal::Bool(b) => serde_json::Value::Bool(*b),
            EVal::Null => serde_json::Value::Null,
            EVal::List(l) => serde_json::Value::Array(l.iter().map(|x| x.to_json()).collect()),
        }
    }
}

pub fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let s = format!("{}", n);
        s
    }
}

/// 宿主接口：变量读取 + 需声明的函数（now/rand/crypto/cookie）。
pub trait ExprHost {
    /// ns: ""(裸名,即 var) | "var" | "let" | "session"；返回 None = 未定义。
    fn var(&self, ns: &str, path: &str) -> Option<EVal>;
    /// 需声明函数调用（now/rand/md5/sha1/sha256/hmac/hex*/base64*/urlEncode/cookie）。
    fn call_declared(
        &self,
        fid: FnId,
        args: &[EVal],
        path: &str,
    ) -> Result<EVal, Diagnostic>;
}

pub fn eval(e: &Expr, host: &dyn ExprHost, path: &str) -> Result<EVal, Diagnostic> {
    match e {
        Expr::Num(n) => Ok(EVal::Num(*n)),
        Expr::Str(s) => Ok(EVal::Str(s.clone())),
        Expr::Bool(b) => Ok(EVal::Bool(*b)),
        Expr::Null => Ok(EVal::Null),
        Expr::Var(vp) => {
            let (ns, p) = match vp {
                VarPath::Bare(p) => ("".to_string(), p.clone()),
                VarPath::Ns(ns, p) => (ns.clone(), p.clone()),
            };
            host.var(&ns, &p)
                .ok_or_else(|| eval_ref(&p, path))
        }
        Expr::Unary(op, a) => {
            let v = eval(a, host, path)?;
            match op {
                UnOp::Not => Ok(EVal::Bool(!v.truthy())),
                UnOp::Neg => match v {
                    EVal::Num(n) => Ok(EVal::Num(-n)),
                    EVal::Null => Ok(EVal::Null),
                    _ => Err(type_err(path, "负号需要 Num 输入")),
                },
            }
        }
        Expr::Ternary(c, t, f) => {
            let cv = eval(c, host, path)?;
            if cv.truthy() {
                eval(t, host, path)
            } else {
                eval(f, host, path)
            }
        }
        Expr::Bin(op, a, b) => eval_bin(*op, a, b, host, path),
        Expr::Call(fid, args) => {
            if fid.is_pure() {
                let mut vals = vec![];
                for a in args {
                    vals.push(eval(a, host, path)?);
                }
                eval_pure(*fid, &vals, path)
            } else {
                let mut vals = vec![];
                for a in args {
                    vals.push(eval(a, host, path)?);
                }
                host.call_declared(*fid, &vals, path)
            }
        }
    }
}

fn eval_ref(name: &str, path: &str) -> Diagnostic {
    Diagnostic::new(
        Code::ERef,
        path,
        format!("变量 `{name}` 未定义或运行期不可见"),
        "检查引用图与阶段可见性（COMPILER.md §3.2）",
    )
}

fn type_err(path: &str, msg: &str) -> Diagnostic {
    Diagnostic::new(Code::EType, path, msg.to_string(), "按 EXPRESSIONS.md §4 的类型规则")
}

fn eval_bin(op: BinOp, a: &Expr, b: &Expr, host: &dyn ExprHost, path: &str) -> Result<EVal, Diagnostic> {
    let l = eval(a, host, path)?;
    let r = eval(b, host, path)?;
    match op {
        BinOp::Add => match (&l, &r) {
            (EVal::Num(x), EVal::Num(y)) => Ok(EVal::Num(x + y)),
            (EVal::List(x), EVal::List(y)) => {
                let mut out = x.clone();
                out.extend(y.clone());
                Ok(EVal::List(out))
            }
            (EVal::Str(_), _) => Ok(EVal::Str(format!("{}{}", l.to_str(), r.to_str()))),
            (_, EVal::Str(_)) => Ok(EVal::Str(format!("{}{}", l.to_str(), r.to_str()))),
            (EVal::Null, _) | (_, EVal::Null) => Ok(EVal::Null),
            _ => Err(type_err(path, "`+` 只支持 数字+数字 / 字符串+任意 / 列表+列表")),
        },
        BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            let (x, y) = numeric_pair(&l, &r, path)?;
            let v = match op {
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => {
                    if y == 0.0 {
                        return Err(type_err(path, "除以 0"));
                    }
                    x / y
                }
                BinOp::Mod => {
                    if y == 0.0 {
                        return Err(type_err(path, "模 0"));
                    }
                    x % y
                }
                _ => unreachable!(),
            };
            Ok(EVal::Num(v))
        }
        BinOp::Eq | BinOp::Ne => {
            let eq = equals(&l, &r);
            Ok(EVal::Bool(if op == BinOp::Eq { eq } else { !eq }))
        }
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let (x, y) = numeric_pair(&l, &r, path)?;
            let v = match op {
                BinOp::Lt => x < y,
                BinOp::Le => x <= y,
                BinOp::Gt => x > y,
                BinOp::Ge => x >= y,
                _ => unreachable!(),
            };
            Ok(EVal::Bool(v))
        }
        BinOp::And => Ok(EVal::Bool(l.truthy() && r.truthy())),
        BinOp::Or => Ok(EVal::Bool(l.truthy() || r.truthy())),
    }
}

fn numeric_pair(l: &EVal, r: &EVal, path: &str) -> Result<(f64, f64), Diagnostic> {
    let x = l
        .to_num()
        .ok_or_else(|| type_err(path, "比较/算术两边必须可转 Num"))?;
    let y = r
        .to_num()
        .ok_or_else(|| type_err(path, "比较/算术两边必须可转 Num"))?;
    Ok((x, y))
}

/// == 同类型直接比；Num↔Str 时 Str 尝试转 Num。
fn equals(l: &EVal, r: &EVal) -> bool {
    match (l, r) {
        (EVal::Null, EVal::Null) => true,
        (EVal::Null, _) | (_, EVal::Null) => false,
        (EVal::Bool(a), EVal::Bool(b)) => a == b,
        (EVal::Num(a), EVal::Num(b)) => a == b,
        (EVal::Num(a), EVal::Str(b)) => b.trim().parse::<f64>().map(|x| x == *a).unwrap_or(false),
        (EVal::Str(a), EVal::Num(b)) => a.trim().parse::<f64>().map(|x| x == *b).unwrap_or(false),
        (EVal::Str(a), EVal::Str(b)) => a == b,
        (EVal::List(a), EVal::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| equals(x, y))
        }
        _ => false,
    }
}

fn eval_pure(fid: FnId, args: &[EVal], path: &str) -> Result<EVal, Diagnostic> {
    let need = |i: usize| -> Result<EVal, Diagnostic> {
        args.get(i)
            .ok_or_else(|| type_err(path, "参数不足"))
            .cloned()
    };
    Ok(match fid {
        FnId::Min | FnId::Max => {
            let mut best: Option<f64> = None;
            for a in args {
                let n = a
                    .to_num()
                    .ok_or_else(|| type_err(path, "min/max 参数必须可转 Num"))?;
                best = Some(match (best, fid) {
                    (None, _) => n,
                    (Some(b), FnId::Min) => b.min(n),
                    (Some(b), _) => b.max(n),
                    (_, _) => n,
                });
            }
            EVal::Num(best.unwrap_or(0.0))
        }
        FnId::Abs => EVal::Num(need(0)?.to_num().unwrap_or(0.0).abs()),
        FnId::Round => {
            let n = need(0)?.to_num().unwrap_or(0.0);
            EVal::Num(n.round())
        }
        FnId::Ceil => EVal::Num(need(0)?.to_num().unwrap_or(0.0).ceil()),
        FnId::Floor => EVal::Num(need(0)?.to_num().unwrap_or(0.0).floor()),
        FnId::Len => match need(0)? {
            EVal::Str(s) => EVal::Num(s.chars().count() as f64),
            EVal::List(l) => EVal::Num(l.len() as f64),
            _ => EVal::Num(0.0),
        },
        FnId::Upper => EVal::Str(need(0)?.to_str().to_uppercase()),
        FnId::Lower => EVal::Str(need(0)?.to_str().to_lowercase()),
        FnId::Trim => EVal::Str(need(0)?.to_str().trim().to_string()),
        FnId::Replace => {
            let s = need(0)?.to_str();
            let from = need(1)?.to_str();
            let to = need(2)?.to_str();
            EVal::Str(if from.is_empty() { s } else { s.replace(&from, &to) })
        }
        FnId::Split => {
            let s = need(0)?.to_str();
            let sep = need(1)?.to_str();
            if sep.is_empty() {
                EVal::List(vec![EVal::Str(s)])
            } else {
                EVal::List(s.split(&sep as &str).map(|p| EVal::Str(p.to_string())).collect())
            }
        }
        FnId::Join => match need(0)? {
            EVal::List(l) => {
                let sep = need(1)?.to_str();
                let parts: Vec<String> = l.iter().map(|x| x.to_str()).collect();
                EVal::Str(parts.join(&sep))
            }
            _ => EVal::Str(String::new()),
        },
        FnId::Contains => {
            let s = need(0)?.to_str();
            let sub = need(1)?.to_str();
            EVal::Bool(s.contains(&sub))
        }
        FnId::Num => EVal::Num(need(0)?.to_num().unwrap_or(0.0)),
        FnId::Str => EVal::Str(need(0)?.to_str()),
        _ => unreachable!("非纯函数不应进入 eval_pure"),
    })
}
