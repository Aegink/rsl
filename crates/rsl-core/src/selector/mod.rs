//! T3.1/T3.2 选择器后端：CSS（scraper）、XPath 子集、JSONPath（RFC 9535 子集）、正则。
//! 编译期惰性编译并 memo（进程级缓存，满足"二次加载 = 反序列化 + 选择器实例化"）。

pub mod jsonpath;
pub mod xpath;

use crate::error::{Code, Diagnostic};
use ego_tree::NodeId;
use scraper::ElementRef;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn cache() -> &'static Mutex<HashMap<String, std::sync::Arc<scraper::Selector>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, std::sync::Arc<scraper::Selector>>>> =
        OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn compile_css(source: &str) -> Result<std::sync::Arc<scraper::Selector>, Diagnostic> {
    // forgiving 处理 :is/:where/:not/:has —— scraper 的 selectors crate 已按规范解析
    let mut c = cache().lock().unwrap();
    if let Some(s) = c.get(source) {
        return Ok(s.clone());
    }
    match scraper::Selector::parse(source) {
        Ok(sel) => {
            let arc = std::sync::Arc::new(sel);
            c.insert(source.to_string(), arc.clone());
            Ok(arc)
        }
        Err(e) => Err(Diagnostic::new(
            Code::ESelector,
            "$",
            format!("CSS 选择器语法非法：`{source}`（{e}）"),
            "检查括号/属性选择器",
        )
        .rule(source)),
    }
}

/// CSS 选择：对文档根或指定节点求值，返回文档序节点列表。
pub fn select_css(
    doc: &scraper::Html,
    root: Option<NodeId>,
    source: &str,
) -> Result<Vec<NodeId>, Diagnostic> {
    match root {
        Some(rid) => select_under(doc, rid, source),
        None => {
            let sel = compile_css(source)?;
            let mut out = vec![];
            let root_node = doc.tree.root();
            for child in root_node.children() {
                if let Some(el) = ElementRef::wrap(child) {
                    if element_matches(&el, &sel) {
                        out.push(child.id());
                    }
                    for d in el.select(&sel) {
                        out.push(d.id());
                    }
                }
            }
            out.dedup();
            Ok(out)
        }
    }
}

/// 节点是否匹配 CSS 选择器（where.has 用）。
pub fn node_matches(doc: &scraper::Html, id: NodeId, source: &str) -> bool {
    let Ok(sel) = compile_css(source) else {
        return false;
    };
    if let Some(n) = doc.tree.get(id) {
        if let Some(el) = ElementRef::wrap(n) {
            return element_matches(&el, &sel);
        }
    }
    false
}

/// 节点的后代里是否有匹配（where.has 需要"含匹配子元素"）。
pub fn node_has_descendant(doc: &scraper::Html, id: NodeId, source: &str) -> bool {
    let Ok(sel) = compile_css(source) else {
        return false;
    };
    if let Some(n) = doc.tree.get(id) {
        if let Some(el) = ElementRef::wrap(n) {
            return el.select(&sel).next().is_some();
        }
    }
    false
}

/// 提取节点文本（含子节点；默认分隔）。
pub fn node_text(doc: &scraper::Html, id: NodeId, own_only: bool) -> String {
    node_text_stripped(doc, id, own_only, &[])
}

/// 提取节点文本（含子节点）。文本块以单空格连接、末尾去空白；
/// `strips` 命中的子树整段跳过（正文去广告，S§3.1 strip）。
pub fn node_text_stripped(
    doc: &scraper::Html,
    id: NodeId,
    own_only: bool,
    strips: &[std::sync::Arc<scraper::Selector>],
) -> String {
    let Some(n) = doc.tree.get(id) else {
        return String::new();
    };
    let mut out = String::new();
    collect_text(n, &mut out, own_only, strips);
    out.trim().to_string()
}

fn collect_text(
    node: ego_tree::NodeRef<'_, scraper::node::Node>,
    out: &mut String,
    own_only: bool,
    strips: &[std::sync::Arc<scraper::Selector>],
) {
    match node.value() {
        scraper::node::Node::Text(t) => {
            let t = t.trim();
            if !t.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(t);
            }
        }
        scraper::node::Node::Element(_) => {
            if own_only {
                return;
            }
            if let Some(el) = ElementRef::wrap(node) {
                if strips.iter().any(|s| element_matches(&el, s)) {
                    return; // 剥除子树
                }
            }
            for child in node.children() {
                collect_text(child, out, own_only, strips);
            }
        }
        _ => {}
    }
}

/// innerHTML（含子节点 HTML）。
pub fn node_inner_html(doc: &scraper::Html, id: NodeId) -> String {
    let Some(n) = doc.tree.get(id) else {
        return String::new();
    };
    let mut out = String::new();
    if let Some(el) = ElementRef::wrap(n) {
        out.push_str(&el.html());
    }
    out
}

pub fn node_tag(doc: &scraper::Html, id: NodeId) -> String {
    let Some(n) = doc.tree.get(id) else {
        return String::new();
    };
    if let Some(el) = ElementRef::wrap(n) {
        return el.value().name().to_string();
    }
    String::new()
}

pub fn node_attr(doc: &scraper::Html, id: NodeId, name: &str) -> Option<String> {
    let Some(n) = doc.tree.get(id) else {
        return None;
    };
    let el = ElementRef::wrap(n)?;
    el.value().attr(name).map(String::from)
}

/// 从节点开始的 CSS 选择（后代 + 自身）。
pub fn select_under(
    doc: &scraper::Html,
    root: NodeId,
    source: &str,
) -> Result<Vec<NodeId>, Diagnostic> {
    let sel = compile_css(source)?;
    let mut out = vec![];
    if let Some(n) = doc.tree.get(root) {
        if let Some(el) = ElementRef::wrap(n) {
            if element_matches(&el, &sel) {
                out.push(root);
            }
            for d in el.select(&sel) {
                out.push(d.id());
            }
        }
    }
    out.dedup();
    Ok(out)
}

/// 简易 HTML 解码（实体反转义，transform.html 用）。
/// 已解码文本中的 U+00A0（&nbsp; 的解析产物）一并归一为普通空格。
pub fn decode_entities(s: &str) -> String {
    let s = if s.contains('\u{00a0}') { s.replace('\u{00a0}', " ") } else { s.to_string() };
    if !s.contains('&') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '&' {
            // &xxx; 或 &#n; 或 &#xh;
            let mut j = i + 1;
            let mut name = String::new();
            while j < ch.len() && ch[j] != ';' && j - i < 12 {
                name.push(ch[j]);
                j += 1;
            }
            if j < ch.len() && ch[j] == ';' && !name.is_empty() {
                let rep = match name.as_str() {
                    "nbsp" => "\u{00a0}",
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    "mdash" => "\u{2014}",
                    "ndash" => "\u{2013}",
                    "hellip" => "\u{2026}",
                    "ldquo" => "\u{201c}",
                    "rdquo" => "\u{201d}",
                    "lsquo" => "\u{2018}",
                    "rsquo" => "\u{2019}",
                    "middot" => "\u{00b7}",
                    _ => {
                        if let Some(num) = name.strip_prefix('#') {
                            let cp = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                num.parse::<u32>().ok()
                            };
                            if let Some(c) = cp.and_then(char::from_u32) {
                                i = j + 1;
                                out.push(c);
                                continue;
                            }
                        }
                        ""
                    }
                };
                if !rep.is_empty() {
                    out.push_str(rep);
                    i = j + 1;
                    continue;
                }
            }
        }
        out.push(ch[i]);
        i += 1;
    }
    out
}

/// ElementRef 自身匹配判断（scraper 0.20 的 ElementRef 无 is 方法；
/// 从父节点选择后按 id 比对，使自身也能命中——where.has 与 li.volume 场景需要）。
pub fn element_matches(el: &ElementRef, sel: &std::sync::Arc<scraper::Selector>) -> bool {
    let this = el.id();
    let Some(parent) = el.parent() else {
        return false;
    };
    if let Some(pe) = ElementRef::wrap(parent) {
        return pe.select(sel).any(|m| m.id() == this);
    }
    // 父是文档节点：逐元素子节点选择
    for sibling in parent.children() {
        if let Some(se) = ElementRef::wrap(sibling) {
            if se.select(sel).any(|m| m.id() == this) {
                return true;
            }
        }
    }
    false
}
