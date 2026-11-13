//! JSONPath（RFC 9535 实用子集，T3.2）：`$`、`.name`、`['name']`、`[0]`/`[-1]`、
//! `[*]`、`[start:end:step]`、`..`（后代递归）、`[?(@.a=="b")]` 过滤。
//! 完整 RFC 9535 过滤表达式归宿主更高等级。

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Seg {
    Child(String),
    Wildcard,
    Index(i64),
    Slice(Option<i64>, Option<i64>, i64),
    Recursive, // .. 后跟 Child/Wildcard
    Filter { key: String, op: FilterOp, val: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Eq,
    Ne,
    Exists,
}

/// 解析 JSONPath。失败 → None。
pub fn parse(src: &str) -> Option<Vec<Seg>> {
    let s = src.trim();
    let s = s.strip_prefix('$').unwrap_or(s);
    let mut segs = vec![];
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    while i < ch.len() {
        match ch[i] {
            '.' => {
                if i + 1 < ch.len() && ch[i + 1] == '.' {
                    segs.push(Seg::Recursive);
                    i += 2;
                    continue;
                }
                i += 1;
                if i < ch.len() && ch[i] == '*' {
                    segs.push(Seg::Wildcard);
                    i += 1;
                    continue;
                }
                let start = i;
                while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_' || ch[i] == '-') {
                    i += 1;
                }
                if i == start {
                    return None;
                }
                segs.push(Seg::Child(ch[start..i].iter().collect()));
            }
            '[' => {
                let mut j = i + 1;
                let mut in_quote: Option<char> = None;
                while j < ch.len() {
                    let c = ch[j];
                    if let Some(q) = in_quote {
                        if c == q {
                            in_quote = None;
                        }
                    } else if c == '"' || c == '\'' {
                        in_quote = Some(c);
                    } else if c == ']' {
                        break;
                    }
                    j += 1;
                }
                if j >= ch.len() {
                    return None;
                }
                let inner: String = ch[i + 1..j].iter().collect();
                let seg = parse_bracket(&inner)?;
                segs.push(seg);
                i = j + 1;
            }
            '*' => {
                segs.push(Seg::Wildcard);
                i += 1;
            }
            _ => return None,
        }
    }
    Some(segs)
}

fn parse_bracket(inner: &str) -> Option<Seg> {
    let t = inner.trim();
    if t == "*" {
        return Some(Seg::Wildcard);
    }
    // ['name'] / ["name"]
    if (t.starts_with('\'') && t.ends_with('\'')) || (t.starts_with('"') && t.ends_with('"')) {
        return Some(Seg::Child(t[1..t.len() - 1].to_string()));
    }
    // [?(@.a=="b")] / [?(@.a)] / [?(@.a!="b")]
    if let Some(rest) = t.strip_prefix("?(") {
        let f = rest.strip_suffix(')')?.trim();
        let f = f.strip_prefix('@').unwrap_or(f);
        if let Some(eq) = f.find("==") {
            return Some(Seg::Filter {
                key: f[..eq].trim().trim_start_matches('.').to_string(),
                op: FilterOp::Eq,
                val: f[eq + 2..].trim().trim_matches(|c| c == '"' || c == '\'').to_string(),
            });
        }
        if let Some(eq) = f.find("!=") {
            return Some(Seg::Filter {
                key: f[..eq].trim().trim_start_matches('.').to_string(),
                op: FilterOp::Ne,
                val: f[eq + 2..].trim().trim_matches(|c| c == '"' || c == '\'').to_string(),
            });
        }
        return Some(Seg::Filter {
            key: f.trim().trim_start_matches('.').to_string(),
            op: FilterOp::Exists,
            val: String::new(),
        });
    }
    // [start:end:step] / [start:end] / [:end]
    if t.contains(':') {
        let parts: Vec<&str> = t.split(':').collect();
        if parts.len() > 3 {
            return None;
        }
        let p = |x: &str| -> Option<i64> { x.trim().parse().ok() };
        let step = if parts.len() == 3 {
            p(parts[2])?
        } else {
            1
        };
        if step == 0 {
            return None;
        }
        return Some(Seg::Slice(p(parts[0]), p(parts[1]), step));
    }
    // [0] / [-1]
    if let Ok(n) = t.parse::<i64>() {
        return Some(Seg::Index(n));
    }
    None
}

/// 求值：返回命中值列表。
pub fn eval(v: &Value, segs: &[Seg]) -> Vec<Value> {
    let mut cur = vec![v.clone()];
    let mut recursive_next = false;
    for seg in segs {
        let mut out = vec![];
        let seg_recursive = recursive_next;
        recursive_next = false;
        match seg {
            Seg::Recursive => {
                recursive_next = true;
                continue;
            }
            Seg::Child(name) => {
                for c in &cur {
                    if seg_recursive {
                        for d in descendants(c, name) {
                            out.push(d);
                        }
                    } else {
                        match c.get(name) {
                            Some(x) => out.push(x.clone()),
                            None => {}
                        }
                    }
                }
            }
            Seg::Wildcard => {
                for c in &cur {
                    match c {
                        Value::Array(a) => out.extend(a.clone()),
                        Value::Object(m) => out.extend(m.values().cloned()),
                        _ => {}
                    }
                }
            }
            Seg::Index(n) => {
                for c in &cur {
                    if let Value::Array(a) = c {
                        let idx = if *n < 0 { a.len() as i64 + *n } else { *n };
                        if idx >= 0 && (idx as usize) < a.len() {
                            out.push(a[idx as usize].clone());
                        }
                    }
                }
            }
            Seg::Slice(start, end, step) => {
                for c in &cur {
                    if let Value::Array(a) = c {
                        let len = a.len() as i64;
                        let s = norm(*start, len, true);
                        let e = norm(*end, len, false);
                        let st = *step;
                        if st > 0 {
                            let mut i = s;
                            while i < e {
                                if i >= 0 && (i as usize) < a.len() {
                                    out.push(a[i as usize].clone());
                                }
                                i += st;
                            }
                        } else {
                            let mut i = e - 1;
                            while i > s {
                                if i >= 0 && (i as usize) < a.len() {
                                    out.push(a[i as usize].clone());
                                }
                                i += st;
                            }
                        }
                    }
                }
            }
            Seg::Filter { key, op, val } => {
                for c in &cur {
                    if let Value::Array(a) = c {
                        for item in a {
                            if filter_match(item, key, op, val) {
                                out.push(item.clone());
                            }
                        }
                    }
                }
            }
        }
        cur = out;
    }
    cur
}

fn norm(n: Option<i64>, len: i64, is_start: bool) -> i64 {
    match n {
        None => {
            if is_start {
                0
            } else {
                len
            }
        }
        Some(x) => {
            let v = if x < 0 { len + x } else { x };
            v.clamp(0, len)
        }
    }
}

fn descendants(v: &Value, name: &str) -> Vec<Value> {
    let mut out = vec![];
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(name) {
                out.push(x.clone());
            }
            for x in m.values() {
                out.extend(descendants(x, name));
            }
        }
        Value::Array(a) => {
            for x in a {
                if let Value::Object(mm) = x {
                    if let Some(y) = mm.get(name) {
                        out.push(y.clone());
                    }
                }
                out.extend(descendants(x, name));
            }
        }
        _ => {}
    }
    out
}

fn filter_match(item: &Value, key: &str, op: &FilterOp, val: &str) -> bool {
    let got = item.get(key);
    match op {
        FilterOp::Exists => got.is_some(),
        FilterOp::Eq => match got {
            Some(Value::String(s)) => s == val,
            Some(other) => other.to_string() == val
                || match (num_of(other), val.parse::<f64>()) {
                    (Some(a), Ok(b)) => a == b,
                    _ => false,
                },
            None => false,
        },
        FilterOp::Ne => !filter_match(item, key, &FilterOp::Eq, val),
    }
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// 路径是否为数组语义（类型推断用）。
pub fn is_list_path(p: &str) -> bool {
    p.contains("[*]") || p.contains("[?(") || p.ends_with("[*]")
}
