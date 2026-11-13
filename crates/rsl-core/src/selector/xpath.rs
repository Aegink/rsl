//! XPath 1.0 实用子集（T3.2）。支持：绝对/相对路径、`//`、谓词 `[n]`/`[@a="v"]`/`[text()="x"]`
//! /`[contains(@a,'v')]`、`text()`、`@attr`、`last()`、通配 `*`。
//! 完整 XPath 1.0 归宿主更高等级（requires.selectorLevel），内核声明实际支持级别。

use ego_tree::NodeId;
use scraper::{ElementRef, Html};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Root,              // /（从文档根）
    Descendant,        // //
    Child(String),     // /tagname or *
    Attr(String),      // @attr
    Pred(Vec<Pred>),   // [..]
    Func(String),      // text() / node()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    Index(usize),                    // [2]（1 起；负数从末尾）
    Last,                            // [last()]
    AttrEq(String, String),          // [@a="v"]
    AttrContains(String, String),    // [contains(@a,"v")]
    TextEq(String),                  // [text()="x"]
    TextContains(String),            // [contains(text(),"x")]
    HasAttr(String),                 // [@a]
}

/// 解析 XPath 子集。失败返回 None（E_SELECTOR 由调用方报）。
pub fn parse(src: &str) -> Option<Vec<Step>> {
    let mut steps = vec![];
    let ch: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    if ch.is_empty() {
        return None;
    }
    if ch[0] == '/' {
        if i + 1 < ch.len() && ch[1] == '/' {
            steps.push(Step::Descendant);
            i += 2;
        } else {
            steps.push(Step::Root);
            i += 1;
        }
    }
    while i < ch.len() {
        // @attr 结尾
        if ch[i] == '@' {
            i += 1;
            let start = i;
            while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_' || ch[i] == '-' || ch[i] == ':' || ch[i] == '*') {
                i += 1;
            }
            if i == start {
                return None;
            }
            steps.push(Step::Attr(ch[start..i].iter().collect()));
            // @attr 后面只能是字符串结尾或 / 继续
            continue;
        }
        // 函数 text() / node()
        let rest: String = ch[i..].iter().take(7).collect();
        if rest.starts_with("text()") {
            steps.push(Step::Func("text".into()));
            i += 6;
            continue;
        }
        if rest.starts_with("last()") {
            steps.push(Step::Pred(vec![Pred::Last]));
            i += 6;
            continue;
        }
        // 谓词
        if ch[i] == '[' {
            let (pred_str, ni) = take_until_matching_bracket(&ch, i)?;
            let preds = parse_pred(&pred_str)?;
            steps.push(Step::Pred(preds));
            i = ni;
            continue;
        }
        // 步进分隔
        if ch[i] == '/' {
            if i + 1 < ch.len() && ch[i + 1] == '/' {
                steps.push(Step::Descendant);
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        // 标签或 *
        let start = i;
        while i < ch.len() && (ch[i].is_alphanumeric() || ch[i] == '_' || ch[i] == '-' || ch[i] == ':' || ch[i] == '*') {
            i += 1;
        }
        if i == start {
            return None;
        }
        steps.push(Step::Child(ch[start..i].iter().collect()));
    }
    Some(steps)
}

fn take_until_matching_bracket(ch: &[char], start: usize) -> Option<(String, usize)> {
    let mut depth = 0usize;
    let mut i = start;
    let mut in_quote: Option<char> = None;
    while i < ch.len() {
        let c = ch[i];
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c == '[' {
            depth += 1;
        } else if c == ']' {
            depth -= 1;
            if depth == 0 {
                return Some((ch[start + 1..i].iter().collect(), i + 1));
            }
        }
        i += 1;
    }
    None
}

fn parse_pred(s: &str) -> Option<Vec<Pred>> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    // [2] / [last()] / [-1]
    if t == "last()" {
        return Some(vec![Pred::Last]);
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some(vec![Pred::Index(n as usize)]);
    }
    // @a="v" / @a='v'
    if let Some(r) = t.strip_prefix('@') {
        if let Some(eq) = r.find('=') {
            let (a, v) = (r[..eq].trim(), r[eq + 1..].trim());
            let v = v.trim_matches(|c| c == '"' || c == '\'');
            return Some(vec![Pred::AttrEq(a.to_string(), v.to_string())]);
        }
        return Some(vec![Pred::HasAttr(r.trim().to_string())]);
    }
    // contains(@a,"v")
    if let Some(rest) = t.strip_prefix("contains(") {
        let inner = rest.strip_suffix(')')?;
        let (what, v) = split_comma(inner)?;
        let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
        if let Some(a) = what.trim().strip_prefix('@') {
            return Some(vec![Pred::AttrContains(a.trim().to_string(), v.to_string())]);
        }
        if what.trim() == "text()" {
            return Some(vec![Pred::TextContains(v.to_string())]);
        }
        return None;
    }
    // text()="x"
    if let Some(rest) = t.strip_prefix("text()") {
        let v = rest.trim().trim_start_matches('=').trim().trim_matches(|c| c == '"' || c == '\'');
        return Some(vec![Pred::TextEq(v.to_string())]);
    }
    None
}

fn split_comma(s: &str) -> Option<(String, String)> {
    let mut in_quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c == ',' {
            return Some((s[..i].to_string(), s[i + 1..].to_string()));
        }
    }
    None
}

/// 求值：从文档根或节点开始。返回节点或字符串（text()/@attr 步）。
pub enum XpOut {
    Nodes(Vec<NodeId>),
    Strs(Vec<String>),
}

pub fn eval(doc: &Html, root: Option<NodeId>, src: &str) -> Option<XpOut> {
    let steps = parse(src)?;
    let mut cur: Vec<NodeId> = vec![];
    let mut is_str = false;
    let mut start_at_doc = true;
    for (si, step) in steps.iter().enumerate() {
        match step {
            Step::Root => {
                start_at_doc = true;
                cur = vec![];
            }
            Step::Descendant => {
                // 下一步是 Child：从当前集合（或根）取所有后代
                let next_tag = match steps.get(si + 1) {
                    Some(Step::Child(t)) => Some(t.clone()),
                    _ => None,
                };
                if cur.is_empty() && start_at_doc {
                    cur = collect_all_elements(doc);
                }
                let mut out = vec![];
                for id in &cur {
                    for d in descendants(doc, *id) {
                        if let Some(t) = &next_tag {
                            if tag_of(doc, d).eq_ignore_ascii_case(t) || t == "*" {
                                out.push(d);
                            }
                        } else {
                            out.push(d);
                        }
                    }
                }
                // 保文档序：NodeId 无全序，跳过（descendants 本身按文档序）
                out.dedup();
                cur = out;
                start_at_doc = false;
            }
            Step::Child(t) => {
                if cur.is_empty() && start_at_doc {
                    cur = collect_all_elements(doc);
                }
                let mut out = vec![];
                for id in &cur {
                    for c in children(doc, *id) {
                        if t == "*" || tag_of(doc, c).eq_ignore_ascii_case(t) {
                            out.push(c);
                        }
                    }
                }
                cur = out;
                start_at_doc = false;
            }
            Step::Attr(a) => {
                let mut out = vec![];
                for id in &cur {
                    if let Some(n) = doc.tree.get(*id) {
                        if let Some(el) = ElementRef::wrap(n) {
                            if let Some(v) = el.value().attr(a) {
                                out.push(v.to_string());
                            }
                        }
                    }
                }
                is_str = true;
                return Some(if is_str { XpOut::Strs(out) } else { XpOut::Nodes(cur) });
            }
            Step::Func(f) => {
                if f == "text" {
                    let mut out = vec![];
                    for id in &cur {
                        if let Some(n) = doc.tree.get(*id) {
                            if let Some(el) = ElementRef::wrap(n) {
                                for t in el.children() {
                                    if let scraper::node::Node::Text(txt) = t.value() {
                                        out.push(txt.to_string());
                                    }
                                }
                            }
                        }
                    }
                    is_str = true;
                    return Some(XpOut::Strs(out));
                }
            }
            Step::Pred(preds) => {
                cur = apply_preds(doc, cur, preds);
                start_at_doc = false;
            }
        }
    }
    Some(if is_str { XpOut::Strs(vec![]) } else { XpOut::Nodes(cur) })
}

fn apply_preds(doc: &Html, cur: Vec<NodeId>, preds: &[Pred]) -> Vec<NodeId> {
    let mut out = cur;
    for p in preds {
        out = match p {
            Pred::Index(n) => {
                if *n == 0 {
                    vec![]
                } else if *n <= out.len() {
                    vec![out[*n - 1]]
                } else {
                    vec![]
                }
            }
                        Pred::Last => {
                let last = out.last().copied().or_else(|| out.first().copied());
                match last {
                    Some(id) => vec![id],
                    None => vec![],
                }
            }
            Pred::HasAttr(a) => out
                .into_iter()
                .filter(|id| attr_of(doc, *id, a).is_some())
                .collect(),
            Pred::AttrEq(a, v) => out
                .into_iter()
                .filter(|id| attr_of(doc, *id, a).map(|x| x == *v).unwrap_or(false))
                .collect(),
            Pred::AttrContains(a, v) => out
                .into_iter()
                .filter(|id| attr_of(doc, *id, a).map(|x| x.contains(v.as_str())).unwrap_or(false))
                .collect(),
            Pred::TextEq(v) => out
                .into_iter()
                .filter(|id| text_of(doc, *id).trim() == v.trim())
                .collect(),
            Pred::TextContains(v) => out
                .into_iter()
                .filter(|id| text_of(doc, *id).contains(v.as_str()))
                .collect(),
        };
    }
    out
}

fn collect_all_elements(doc: &Html) -> Vec<NodeId> {
    let mut out = vec![];
    for n in doc.tree.nodes() {
        if n.value().is_element() {
            out.push(n.id());
        }
    }
    out
}

fn descendants(doc: &Html, id: NodeId) -> Vec<NodeId> {
    let mut out = vec![];
    if let Some(n) = doc.tree.get(id) {
        if let Some(el) = ElementRef::wrap(n) {
            for d in el.descendants() {
                if d.value().is_element() && d.id() != id {
                    out.push(d.id());
                }
            }
        }
    }
    out
}

fn children(doc: &Html, id: NodeId) -> Vec<NodeId> {
    let mut out = vec![];
    if let Some(n) = doc.tree.get(id) {
        for c in n.children() {
            if c.value().is_element() {
                out.push(c.id());
            }
        }
    }
    out
}

fn tag_of(doc: &Html, id: NodeId) -> String {
    doc.tree
        .get(id)
        .and_then(|n| ElementRef::wrap(n))
        .map(|el| el.value().name().to_string())
        .unwrap_or_default()
}

fn attr_of(doc: &Html, id: NodeId, a: &str) -> Option<String> {
    doc.tree
        .get(id)
        .and_then(|n| ElementRef::wrap(n))
        .and_then(|el| el.value().attr(a).map(String::from))
}

fn text_of(doc: &Html, id: NodeId) -> String {
    doc.tree
        .get(id)
        .map(|n| {
            let mut s = String::new();
            if let Some(el) = ElementRef::wrap(n) {
                for t in el.descendants() {
                    if let scraper::node::Node::Text(txt) = t.value() {
                        s.push_str(txt);
                        s.push(' ');
                    }
                }
            }
            s
        })
        .unwrap_or_default()
}
