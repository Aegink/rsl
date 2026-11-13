//! `expr:` 词法与 Pratt 解析（EXPRESSIONS.md §2–3）。
//! 禁止：赋值、正则字面量、属性访问 `.`（裸标识后）、下标、注释、分号。
//! 裁决 D1：`var:`/`let:`/`session:` 前缀后的点路径合法（官方示例依赖 `var:item.id`）；
//!          裸 `a.b` 仍 `E_EXPR_SYNTAX`（与 expr_proto 基线一致）。

use crate::error::{Code, Diagnostic};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum UnOp {
    Not,
    Neg,
}

/// 变量引用：裸标识（简写）或命名空间路径。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VarPath {
    /// `page` ≡ `var:page`
    Bare(String),
    /// `var:book.tocUrl` / `let:ts` / `session:uid`
    Ns(String, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum FnId {
    // 纯函数（无需声明）
    Min,
    Max,
    Abs,
    Round,
    Ceil,
    Floor,
    Len,
    Upper,
    Lower,
    Trim,
    Replace,
    Split,
    Join,
    Contains,
    Num,
    Str,
    // 需声明
    Now,
    Rand,
    Md5,
    Sha1,
    Sha256,
    Hmac,
    HexEncode,
    HexDecode,
    Base64Encode,
    Base64Decode,
    UrlEncode,
    Cookie,
}

impl FnId {
    pub fn from_name(name: &str) -> Option<FnId> {
        Some(match name {
            "min" => FnId::Min,
            "max" => FnId::Max,
            "abs" => FnId::Abs,
            "round" => FnId::Round,
            "ceil" => FnId::Ceil,
            "floor" => FnId::Floor,
            "len" => FnId::Len,
            "upper" => FnId::Upper,
            "lower" => FnId::Lower,
            "trim" => FnId::Trim,
            "replace" => FnId::Replace,
            "split" => FnId::Split,
            "join" => FnId::Join,
            "contains" => FnId::Contains,
            "num" => FnId::Num,
            "str" => FnId::Str,
            "now" => FnId::Now,
            "rand" => FnId::Rand,
            "md5" => FnId::Md5,
            "sha1" => FnId::Sha1,
            "sha256" => FnId::Sha256,
            "hmac" => FnId::Hmac,
            "hexEncode" => FnId::HexEncode,
            "hexDecode" => FnId::HexDecode,
            "base64Encode" => FnId::Base64Encode,
            "base64Decode" => FnId::Base64Decode,
            "urlEncode" => FnId::UrlEncode,
            "cookie" => FnId::Cookie,
            _ => return None,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            FnId::Min => "min",
            FnId::Max => "max",
            FnId::Abs => "abs",
            FnId::Round => "round",
            FnId::Ceil => "ceil",
            FnId::Floor => "floor",
            FnId::Len => "len",
            FnId::Upper => "upper",
            FnId::Lower => "lower",
            FnId::Trim => "trim",
            FnId::Replace => "replace",
            FnId::Split => "split",
            FnId::Join => "join",
            FnId::Contains => "contains",
            FnId::Num => "num",
            FnId::Str => "str",
            FnId::Now => "now",
            FnId::Rand => "rand",
            FnId::Md5 => "md5",
            FnId::Sha1 => "sha1",
            FnId::Sha256 => "sha256",
            FnId::Hmac => "hmac",
            FnId::HexEncode => "hexEncode",
            FnId::HexDecode => "hexDecode",
            FnId::Base64Encode => "base64Encode",
            FnId::Base64Decode => "base64Decode",
            FnId::UrlEncode => "urlEncode",
            FnId::Cookie => "cookie",
        }
    }

    /// 纯函数 = 无需 feature 声明。
    pub fn is_pure(&self) -> bool {
        !matches!(
            self,
            FnId::Now
                | FnId::Rand
                | FnId::Md5
                | FnId::Sha1
                | FnId::Sha256
                | FnId::Hmac
                | FnId::HexEncode
                | FnId::HexDecode
                | FnId::Base64Encode
                | FnId::Base64Decode
                | FnId::UrlEncode
                | FnId::Cookie
        )
    }

    /// 返回 (feature, crypto 算法)。None = 纯函数。
    pub fn requirement(&self) -> Option<(&'static str, Option<&'static str>)> {
        Some(match self {
            FnId::Now => ("time", None),
            FnId::Rand => ("random", None),
            FnId::Md5 => ("crypto", Some("md5")),
            FnId::Sha1 => ("crypto", Some("sha1")),
            FnId::Sha256 => ("crypto", Some("sha256")),
            FnId::Hmac => ("crypto", None), // 算法由第一个参数决定
            FnId::HexEncode | FnId::HexDecode => ("crypto", Some("hex")),
            FnId::Base64Encode | FnId::Base64Decode => ("crypto", Some("base64")),
            FnId::UrlEncode => ("crypto", Some("url-encode")),
            FnId::Cookie => ("", None), // 要求源声明 login 或 cookie
            _ => return None,
        })
    }

    pub fn arity(&self) -> (usize, usize) {
        match self {
            FnId::Min | FnId::Max => (1, usize::MAX),
            FnId::Now => (0, 0),
            FnId::Rand => (0, 1),
            FnId::Hmac => (3, 3),
            FnId::UrlEncode => (1, 2),
            FnId::Replace => (3, 3),
            FnId::Split | FnId::Join | FnId::Contains => (2, 2),
            _ => (1, 1),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Var(VarPath),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(FnId, Vec<Expr>),
}

// ───────────────────────── 词法 ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    NsIdent(String, String), // prefix, path
    Op(&'static str),
    LParen,
    RParen,
    Comma,
}

const NS_PREFIXES: &[&str] = &["var", "let", "session"];

fn lex(src: &str, path: &str) -> Result<Vec<(Tok, usize)>, Diagnostic> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut out = vec![];
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let mut j = i;
            let mut saw_dot = false;
            while j < b.len() && (b[j].is_ascii_digit() || (b[j] == '.' && !saw_dot)) {
                if b[j] == '.' {
                    saw_dot = true;
                }
                j += 1;
            }
            let s: String = b[i..j].iter().collect();
            let n: f64 = s
                .parse()
                .map_err(|_| Diagnostic::new(Code::EExprSyntax, path, format!("非法数字 `{s}`"), "检查数字字面量"))?;
            out.push((Tok::Num(n), start));
            i = j;
            continue;
        }
        if c == '\'' || c == '"' {
            let quote = c;
            let mut j = i + 1;
            let mut s = String::new();
            let mut closed = false;
            while j < b.len() {
                if b[j] == '\\' && j + 1 < b.len() {
                    match b[j + 1] {
                        '\\' => s.push('\\'),
                        '\'' => s.push('\''),
                        '"' => s.push('"'),
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        other => {
                            s.push('\\');
                            s.push(other);
                        }
                    }
                    j += 2;
                    continue;
                }
                if b[j] == quote {
                    closed = true;
                    j += 1;
                    break;
                }
                s.push(b[j]);
                j += 1;
            }
            if !closed {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "字符串未闭合",
                    "补上配对的引号",
                ));
            }
            out.push((Tok::Str(s), start));
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '_') {
                j += 1;
            }
            let name: String = b[i..j].iter().collect();
            // 命名空间前缀：var: / let: / session:
            if j < b.len() && b[j] == ':' && NS_PREFIXES.contains(&name.as_str()) {
                let mut k = j + 1;
                let mut p = String::new();
                while k < b.len()
                    && (b[k].is_ascii_alphanumeric()
                        || b[k] == '_'
                        || b[k] == '.'
                        || b[k] > '\u{4e00}')
                {
                    p.push(b[k]);
                    k += 1;
                }
                if p.is_empty() {
                    return Err(Diagnostic::new(
                        Code::EExprSyntax,
                        path,
                        format!("`{name}:` 后缺少路径"),
                        "写全变量路径，如 var:page",
                    ));
                }
                out.push((Tok::NsIdent(name.clone(), p), start));
                i = k;
                continue;
            }
            out.push((Tok::Ident(name), start));
            i = j;
            continue;
        }
        // 运算符
        let two: String = b[i..(i + 2).min(b.len())].iter().collect();
        let two_op = match two.as_str() {
            "==" => "==",
            "!=" => "!=",
            "<=" => "<=",
            ">=" => ">=",
            "&&" => "&&",
            "||" => "||",
            _ => "",
        };
        if !two_op.is_empty() {
            out.push((Tok::Op(two_op), start));
            i += 2;
            continue;
        }
        let one_op = match c {
            '+' => "+",
            '-' => "-",
            '*' => "*",
            '/' => "/",
            '%' => "%",
            '<' => "<",
            '>' => ">",
            '!' => "!",
            '?' => "?",
            ':' => ":",
            '=' => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "赋值不允许（`=`）",
                    "比较用 ==；expr 是纯计算，无副作用",
                ))
            }
            '.' => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "属性访问 `.` 不允许（裸标识后）",
                    "带 `.` 的路径只能出现在 var:/let:/session: 前缀后；否则先用选择器取出再放 let",
                ))
            }
            '[' => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "下标 `[]` 不允许",
                    "已有列表用选择器 pick/range；JSON 字段走 json: 路径",
                ))
            }
            '`' => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "反引号不允许",
                    "字符串只用 ' 或 \"",
                ))
            }
            ';' => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    "分号不允许",
                    "expr 是单条表达式",
                ))
            }
            _ => "",
        };
        if !one_op.is_empty() {
            out.push((Tok::Op(one_op), start));
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push((Tok::LParen, start));
                i += 1;
            }
            ')' => {
                out.push((Tok::RParen, start));
                i += 1;
            }
            ',' => {
                out.push((Tok::Comma, start));
                i += 1;
            }
            _ => {
                return Err(Diagnostic::new(
                    Code::EExprSyntax,
                    path,
                    format!("非法字符 `{c}`（列 {start}）"),
                    "expr 只允许 §2 词法表中的记号",
                ));
            }
        }
    }
    Ok(out)
}

// ───────────────────────── Pratt 解析 ─────────────────────────

pub struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    path: String,
    depth: usize,
    max_depth: usize,
}

/// 解析 `expr:` 源（不含 `expr:` 前缀）。深度超限 → E_EXPR_LIMIT。
pub fn parse(src: &str, path: &str) -> Result<Expr, Diagnostic> {
    if src.chars().count() > 512 {
        return Err(Diagnostic::new(
            Code::EExprLimit,
            path,
            format!("表达式 {} 字符超上限 512", src.chars().count()),
            "拆成 let 中间值",
        ));
    }
    let toks = lex(src, path)?;
    let mut p = Parser {
        toks,
        pos: 0,
        path: path.to_string(),
        depth: 0,
        max_depth: 32,
    };
    if p.toks.is_empty() {
        return Err(Diagnostic::new(Code::EExprSyntax, path, "空表达式", "写一条合法表达式"));
    }
    let e = p.ternary()?;
    if p.pos < p.toks.len() {
        let (t, col) = p.toks[p.pos].clone();
        return Err(Diagnostic::new(
            Code::EExprSyntax,
            path,
            format!("意外记号 `{tok:?}`（列 {col}）", tok = t),
            "检查运算符配对",
        ));
    }
    Ok(e)
}

fn syn(p: &Parser, msg: impl Into<String>, hint: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::EExprSyntax, p.path.clone(), msg, hint)
}

impl Parser {
    fn peek(&self) -> Option<&(Tok, usize)> {
        self.toks.get(self.pos)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if op == "," {
            if matches!(self.peek(), Some((Tok::Comma, _))) {
                self.pos += 1;
                return true;
            }
            return false;
        }
        if let Some((Tok::Op(o), _)) = self.peek() {
            if *o == op {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn enter(&mut self) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(Diagnostic::new(
                Code::EExprLimit,
                self.path.clone(),
                format!("表达式嵌套深度 {} 超上限 {}", self.depth, self.max_depth),
                "拆成 let 中间值",
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// ternary := logicOr [ "?" expr ":" expr ]
    fn ternary(&mut self) -> Result<Expr, Diagnostic> {
        self.enter()?;
        let cond = self.logic_or()?;
        let e = if self.eat_op("?") {
            let t = self.ternary()?;
            if !self.eat_op(":") {
                return Err(syn(self, "三元缺少 `:`", "a ? b : c"));
            }
            let f = self.ternary()?;
            Expr::Ternary(Box::new(cond), Box::new(t), Box::new(f))
        } else {
            cond
        };
        self.leave();
        Ok(e)
    }

    fn logic_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.logic_and()?;
        while self.eat_op("||") {
            let r = self.logic_and()?;
            l = Expr::Bin(BinOp::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn logic_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.equality()?;
        while self.eat_op("&&") {
            let r = self.equality()?;
            l = Expr::Bin(BinOp::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn equality(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.relational()?;
        loop {
            let op = if self.eat_op("==") {
                BinOp::Eq
            } else if self.eat_op("!=") {
                BinOp::Ne
            } else {
                break;
            };
            let r = self.relational()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn relational(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.additive()?;
        loop {
            let op = if self.eat_op("<=") {
                BinOp::Le
            } else if self.eat_op(">=") {
                BinOp::Ge
            } else if self.eat_op("<") {
                BinOp::Lt
            } else if self.eat_op(">") {
                BinOp::Gt
            } else {
                break;
            };
            let r = self.additive()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn additive(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.multiplicative()?;
        loop {
            let op = if self.eat_op("+") {
                BinOp::Add
            } else if self.eat_op("-") {
                BinOp::Sub
            } else {
                break;
            };
            let r = self.multiplicative()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn multiplicative(&mut self) -> Result<Expr, Diagnostic> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat_op("*") {
                BinOp::Mul
            } else if self.eat_op("/") {
                BinOp::Div
            } else if self.eat_op("%") {
                BinOp::Mod
            } else {
                break;
            };
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        if self.eat_op("!") {
            let e = self.unary()?;
            return Ok(Expr::Unary(UnOp::Not, Box::new(e)));
        }
        if self.eat_op("-") {
            let e = self.unary()?;
            return Ok(Expr::Unary(UnOp::Neg, Box::new(e)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let (tok, _col) = self
            .peek()
            .cloned()
            .ok_or_else(|| syn(self, "表达式意外结束", "检查是否写完"))?;
        self.pos += 1;
        match tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::NsIdent(prefix, path_part) => Ok(Expr::Var(VarPath::Ns(prefix, path_part))),
            Tok::Ident(name) => {
                // 字面量关键字
                if name == "true" {
                    return Ok(Expr::Bool(true));
                }
                if name == "false" {
                    return Ok(Expr::Bool(false));
                }
                if name == "null" {
                    return Ok(Expr::Null);
                }
                // 函数调用？
                if matches!(self.peek(), Some((Tok::LParen, _))) {
                    self.pos += 1;
                    let fid = FnId::from_name(&name).ok_or_else(|| {
                        Diagnostic::new(
                            Code::EExprFn,
                            self.path.clone(),
                            format!("函数 `{name}` 不存在"),
                            "可用函数见 EXPRESSIONS.md §5（min/max/abs/…/md5/urlEncode）",
                        )
                    })?;
                    let mut args = vec![];
                    if !matches!(self.peek(), Some((Tok::RParen, _))) {
                        loop {
                            let a = self.ternary()?;
                            args.push(a);
                            if self.eat_op(",") {
                                continue;
                            }
                            break;
                        }
                    }
                    if !matches!(self.peek(), Some((Tok::RParen, _))) {
                        return Err(syn(self, "函数调用缺少 `)`", "补右括号"));
                    }
                    self.pos += 1;
                    let (lo, hi) = fid.arity();
                    if args.len() < lo || args.len() > hi {
                        return Err(Diagnostic::new(
                            Code::EExprFn,
                            self.path.clone(),
                            format!(
                                "函数 `{name}` 参数数量 {} 不合法（应为 {}）",
                                args.len(),
                                if lo == hi {
                                    lo.to_string()
                                } else {
                                    format!("{lo}..{hi}")
                                }
                            ),
                            "按 EXPRESSIONS.md §5 的签名调用",
                        ));
                    }
                    Ok(Expr::Call(fid, args))
                } else if matches!(self.peek(), Some((Tok::Op(o), _)) if *o == ".") {
                    Err(syn(
                        self,
                        format!("属性访问 `{name}.` 不允许"),
                        "带 `.` 的路径只能写在 var:/let:/session: 前缀后",
                    ))
                } else {
                    // 裸标识 = 变量简写（page ≡ var:page）；true/false/null 在词法层已排除 Ident
                    Ok(Expr::Var(VarPath::Bare(name)))
                }
            }
            Tok::LParen => {
                let e = self.ternary()?;
                if !matches!(self.peek(), Some((Tok::RParen, _))) {
                    return Err(syn(self, "括号未闭合", "补右括号"));
                }
                self.pos += 1;
                Ok(e)
            }
            Tok::Op(o) => Err(syn(
                self,
                format!("意外运算符 `{o}`"),
                "检查表达式语法",
            )),
            Tok::RParen | Tok::Comma => Err(syn(self, "意外的括号/逗号", "检查表达式语法")),
        }
    }
}
