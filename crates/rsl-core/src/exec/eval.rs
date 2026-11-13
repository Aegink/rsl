//! T3.1/T3.5 选择器与管道求值。管线：base → strip/range/pick → 提取(attr/text/paragraphs/blocks/map)
//! → transform（含 zip/protect）→ fallback → cond → optional/default。

use super::ctx::{ExecCx, TraceEntry};
use super::hosts::{CtxHost, ExprHostImpl};
use super::value::{Block, Doc, HtmlDoc, Val};
use crate::error::{Code, Diagnostic};
use crate::ir::*;
use crate::selector::{jsonpath, xpath};
use ego_tree::NodeId;
use serde_json::Value;

/// 求值环境：文档引用（CSS/XPath 需要）。
pub struct Eval<'a, 'c> {
    pub cx: &'a mut ExecCx<'c>,
    pub doc: Option<&'a HtmlDoc>,
    /// 当前规则的原始输入（zip 的 with 在父作用域求值）。
    pub origin: Val,
    pub path: String,
    pub stage: String,
    /// true = out.list 位置：保留 NodeList，不做默认文本提取。
    pub keep_nodes: bool,
    /// 当前选择器的 strip 编译结果（extract 时使用）。
    pub cur_strips: std::rc::Rc<std::cell::RefCell<Vec<std::sync::Arc<scraper::Selector>>>>,
}

impl<'a, 'c> Eval<'a, 'c> {
    fn err(&self, code: Code, msg: impl Into<String>, hint: impl Into<String>) -> Diagnostic {
        Diagnostic::new(code, self.path.clone(), msg, hint)
    }

    // ───────────── 选择器 ─────────────

    /// 求值一个选择器。fail_empty=true 表示"未命中"返回 Val::Empty 而不是错误。
    pub fn eval_selector(&mut self, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        let t0 = crate::ports::now_ms() as u64;
        // fallback：第一个非空生效（S§3.7）
        let mut val = if sel.fallback.is_empty() {
            self.eval_one(sel)?
        } else {
            let mut found = Val::Empty;
            for (i, f) in sel.fallback.iter().enumerate() {
                let mut sub = Eval {
                    cx: self.cx,
                    doc: self.doc,
                    origin: self.origin.clone(),
                    path: format!("{}.fallback[{}]", self.path, i),
                    stage: self.stage.clone(),
                keep_nodes: false,
                cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                let r = sub.eval_one(f)?;
                if !r.is_empty_val() {
                    found = r;
                    break;
                }
            }
            // fallback 全空：走主链（可能为空 → optional/default）
            if found.is_empty_val() {
                Val::Empty
            } else {
                found
            }
        };

        // 属性提取（fallback 时对命中项做 attr）
        if let Some(a) = &sel.attr {
            val = self.extract_attr(val, a);
        }

        // cond（断言位置：ok/when）
        if let Some(c) = &sel.cond {
            let ok = self.check_cond(&val.scalarize(), c)?;
            return Ok(Val::Bool(ok));
        }

        // optional / default
        if val.is_empty_val() {
            if sel.optional {
                if let Some(d) = &sel.default {
                    val = Val::Json(d.to_json());
                } else {
                    val = Val::Empty;
                }
            } else if sel.fallback.is_empty()
                && !matches!(sel.base, BaseSel::Expr(_) | BaseSel::Lib(_))
            {
                return Err(self.err(
                    Code::ENoMatch,
                    format!("选择器命中 0 个元素：{}", sel.raw),
                    "换选择器（rsl explain 对比）；或加 fallback / optional:true",
                )
                .rule(&sel.raw)
                .sample("（响应片段见 --trace）"));
            }
        }

        let ms = (crate::ports::now_ms() as u64).saturating_sub(t0);
        let matched = match &val {
            Val::NodeList(l) => l.len(),
            Val::StrList(l) => l.len(),
            Val::BlockList(l) => l.len(),
            Val::Json(Value::Array(a)) => a.len(),
            Val::Empty => 0,
            _ => 1,
        };
        self.cx.trace.borrow_mut().push(TraceEntry {
            stage: self.stage.clone(),
            step: self.path.clone(),
            rule: sel.raw.clone(),
            matched,
            ms,
            bridge: if sel.transform.is_empty() { 0 } else { 1 },
            sample: Some(val_scalar_sample(&val)),
        });
        Ok(val)
    }

    fn eval_one(&mut self, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        // 基座
        *self.cur_strips.borrow_mut() = self.compiled_strips(sel)?;
        let mut val = self.eval_base(&sel.base, &sel.flags)?;
        // strip（节点子树排除）
        if !sel.strip.is_empty() && matches!(val, Val::Node(_) | Val::NodeList(_)) {
            val = self.apply_strip(val, sel)?;
        }
        // range / pick / exclude / limit
        val = self.apply_index_ops(val, sel)?;
        // where
        if let Some(w) = &sel.where_ {
            val = self.apply_where(val, w)?;
        }
        // exists
        if sel.exists {
            let hit = !val.is_empty_val();
            return Ok(Val::Bool(hit));
        }
        // 提取
        if let Some(a) = &sel.attr {
            val = self.extract_attr(val, a);
        } else if sel.paragraphs.is_some() {
            val = self.extract_paragraphs(val, sel)?;
        } else if sel.blocks.is_some() {
            val = self.extract_blocks(val, sel)?;
        } else if !sel.map.is_empty() {
            val = self.eval_map(val, sel)?;
        } else if sel.transform.is_empty() {
            // 默认提取：Node → 文本（D5 裁决）；out.list 位置保留节点
            if !self.keep_nodes {
                val = self.extract_text_default(val)?;
            }
        }
        // protect + transform
        if !sel.transform.is_empty() {
            val = self.run_transform(val, &sel.transform, Some(sel))?;
        }
        // typeMap
        if let Some(tm) = &sel.type_map {
            let s = val.to_text();
            if let Some(mapped) = tm.get(&s) {
                val = Val::Str(mapped.clone());
            } else {
                // 未命中映射 → 空串（合法空）
                val = Val::Str(String::new());
            }
        }

        // limit 收敛
        if let Some(limit) = sel.limit {
            if let Val::StrList(l) = &val {
                if l.len() > limit {
                    val = Val::StrList(l[..limit].to_vec());
                }
            }
        }
        Ok(val)
    }

    fn eval_base(&mut self, base: &BaseSel, flags: &str) -> Result<Val, Diagnostic> {
        Ok(match base {
            BaseSel::None => self.extract_text_default(self.current_scope_nodes())?,
            BaseSel::Css(src) => {
                let Some(doc) = self.doc else {
                    return Err(self.err(Code::EType, "css: 需要 HTML 文档作用域", "response 是 json/text 时用 json:/re:"));
                };
                let root = self.current_root(doc);
                let ids = crate::selector::select_css(&doc.html, root, src)?;
                if ids.is_empty() {
                    Val::NodeList(vec![])
                } else {
                    Val::NodeList(ids)
                }
            }
            BaseSel::Xpath(src) => {
                let Some(doc) = self.doc else {
                    return Err(self.err(Code::EType, "xpath: 需要 HTML 文档作用域", "response 是 json/text 时用 json:/re:"));
                };
                let root = self.current_root(doc);
                match xpath::eval(&doc.html, root, src) {
                    Some(xpath::XpOut::Nodes(ids)) => Val::NodeList(ids),
                    Some(xpath::XpOut::Strs(s)) => {
                        if s.len() == 1 {
                            Val::Str(s[0].clone())
                        } else {
                            Val::StrList(s)
                        }
                    }
                    None => Val::Empty,
                }
            }
            BaseSel::Json(path) => {
                let v = self.json_scope();
                let segs = jsonpath::parse(path).ok_or_else(|| {
                    self.err(Code::ESelector, format!("JSONPath 语法非法：`{path}`"), "检查 .name / [*] / [0..n] / [?(…)] 写法")
                })?;
                let hits = jsonpath::eval(&v, &segs);
                if hits.is_empty() {
                    Val::Empty
                } else if hits.len() == 1 {
                    Val::Json(hits[0].clone())
                } else {
                    Val::Json(Value::Array(hits))
                }
            }
            BaseSel::Re { source, flags } => {
                let s = self.origin_text();
                let re = compile_regex(source, flags, &self.path)?;
                let mut out = vec![];
                for caps in re.captures_iter(&s) {
                    if caps.len() > 1 {
                        for gi in 1..caps.len() {
                            if let Some(m) = caps.get(gi) {
                                out.push(m.as_str().to_string());
                            }
                        }
                    } else if let Some(m) = caps.get(0) {
                        out.push(m.as_str().to_string());
                    }
                }
                if out.len() == 1 {
                    Val::Str(out.remove(0))
                } else {
                    Val::StrList(out)
                }
            }
            BaseSel::Text => {
                let v = self.current_scope_nodes();
                self.extract_text_kind(v, false)
            }
            BaseSel::OwnText => {
                let v = self.current_scope_nodes();
                self.extract_text_kind(v, true)
            }
            BaseSel::Html => {
                let v = self.current_scope_nodes();
                match v {
                    Val::Node(id) => {
                        let doc = self.doc.ok_or_else(|| self.err(Code::EType, "html 需要 HTML 作用域", "—"))?;
                        Val::Str(crate::selector::node_inner_html(&doc.html, id))
                    }
                    Val::NodeList(l) => {
                        let doc = self.doc.ok_or_else(|| self.err(Code::EType, "html 需要 HTML 作用域", "—"))?;
                        Val::StrList(l.iter().map(|id| crate::selector::node_inner_html(&doc.html, *id)).collect())
                    }
                    other => other,
                }
            }
            BaseSel::Tag => {
                let v = self.current_scope_nodes();
                match v {
                    Val::Node(id) => {
                        let doc = self.doc.ok_or_else(|| self.err(Code::EType, "tag 需要 HTML 作用域", "—"))?;
                        Val::Str(crate::selector::node_tag(&doc.html, id))
                    }
                    Val::NodeList(l) => {
                        let doc = self.doc.ok_or_else(|| self.err(Code::EType, "tag 需要 HTML 作用域", "—"))?;
                        Val::StrList(l.iter().map(|id| crate::selector::node_tag(&doc.html, *id)).collect())
                    }
                    other => other,
                }
            }
            BaseSel::Attr(a) => {
                let v = self.current_scope_nodes();
                self.extract_attr(v, a)
            }
            BaseSel::Const(v) => Val::Json(v.to_json()),
            BaseSel::Var(p) => match self.cx.get_var(p) {
                Some(v) => Value::from(v).into_val(),
                None => {
                    return Err(self.err(
                        Code::ERef,
                        format!("变量 var:{p} 运行期不可见"),
                        "对照引用图；宿主必须显式注入（S§5.4）",
                    ))
                }
            },
            BaseSel::Let(p) | BaseSel::Session(p) => {
                let ns = if matches!(base, BaseSel::Let(_)) { "let" } else { "session" };
                match self.cx.session.borrow().get(p).cloned() {
                    Some(v) => v.into_val(),
                    None => {
                        return Err(self.err(
                            Code::ERef,
                            format!("{ns}:{p} 运行期未定义"),
                            "let 必须在本阶段先求值；session 由 script/let 写入",
                        ))
                    }
                }
            }
            BaseSel::Token(p) => match self.cx.tokens.borrow().get(p).cloned() {
                Some(v) => Val::Str(v),
                None => {
                    return Err(self.err(
                        Code::ELoginRequired,
                        format!("token:{p} 不存在"),
                        "先完成登录（login.steps）",
                    ))
                }
            },
            BaseSel::Cookie(p) => match self.cx.get_cookie(p) {
                Some(v) => Val::Str(v),
                None => Val::Str(String::new()),
            },
            BaseSel::Cache(p) => match self.cx.cache.borrow().get(p).cloned() {
                Some(v) => v.into_val(),
                None => Val::Empty,
            },
            BaseSel::Lib(f) => {
                // lib.pure 函数调用（入参 = 当前输入文本）
                let engine = self.cx_script_engine()?.ok_or_else(|| {
                    self.err(Code::EScript, format!("lib:{f} 需要脚本引擎"), "宿主注入 script 引擎（features 含 js）")
                })?;
                let input = self.origin_text();
                let input_json = Value::String(input);
                let r = engine.call_pure(f, &[&input_json], &self.path)?;
                crate::exec::value::Val::Json(r)
            }
            BaseSel::Expr(e) => {
                let host = ExprHostImpl { cx: self.cx, doc_val: self.origin.clone() };
                let v = crate::expr::eval::eval(e, &host, &self.path)?;
                v.into_val()
            }
        })
    }

    /// 当前作用域：map 项 / 阶段根。
    fn current_scope_nodes(&self) -> Val {
        match &self.origin {
            Val::Node(id) => Val::Node(*id),
            Val::NodeList(l) => Val::NodeList(l.clone()),
            Val::Empty => Val::NodeList(vec![]),
            _ => Val::Empty,
        }
    }

    fn current_root(&self, _doc: &HtmlDoc) -> Option<NodeId> {
        match &self.origin {
            Val::Node(id) => Some(*id),
            Val::NodeList(l) => l.first().copied(),
            _ => None,
        }
    }

    /// JSON 作用域：阶段根（parse 后）或 origin。
    fn json_scope(&self) -> Value {
        match &self.origin {
            Val::Json(v) => v.clone(),
            Val::Str(s) => Value::String(s.clone()),
            Val::Empty => Value::Null,
            _ => Value::Null,
        }
    }

    fn origin_text(&self) -> String {
        self.origin.to_text()
    }

    // ───────────── 提取 ─────────────

    fn extract_text_default(&mut self, v: Val) -> Result<Val, Diagnostic> {
        Ok(match v {
            Val::Node(_) | Val::NodeList(_) => self.extract_text_kind(v, false),
            other => other,
        })
    }

    fn extract_text_kind(&self, v: Val, own: bool) -> Val {
        let Some(doc) = self.doc else {
            return v;
        };
        let strips = self.cur_strips.borrow().clone();
        match &v {
            Val::Node(id) => Val::Str(crate::selector::node_text_stripped(&doc.html, *id, own, &strips)),
            Val::NodeList(l) => {
                let strs: Vec<String> = l.iter().map(|id| crate::selector::node_text_stripped(&doc.html, *id, own, &strips)).collect();
                match strs.len() {
                    0 => Val::Str(String::new()),
                    1 => Val::Str(strs[0].clone()),
                    _ => Val::StrList(strs),
                }
            }
            _ => v,
        }
    }

    fn extract_attr(&self, v: Val, a: &str) -> Val {
        let Some(doc) = self.doc else {
            return v;
        };
        match &v {
            Val::Node(id) => match crate::selector::node_attr(&doc.html, *id, a) {
                Some(x) => Val::Str(x),
                None => Val::Empty,
            },
            Val::NodeList(l) => {
                let strs: Vec<String> = l
                    .iter()
                    .filter_map(|id| crate::selector::node_attr(&doc.html, *id, a))
                    .collect();
                match strs.len() {
                    0 => Val::Empty,
                    1 => Val::Str(strs[0].clone()),
                    _ => Val::StrList(strs),
                }
            }
            _ => v,
        }
    }

    fn extract_paragraphs(&mut self, v: Val, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        let Some(doc) = self.doc else {
            return Err(self.err(Code::EType, "paragraphs 需要 HTML 作用域", "JSON 用 json: 路径"));
        };
        let (split, min_len) = sel.paragraphs.clone().unwrap_or(("auto".into(), 1));
        let strips = self.cur_strips.borrow().clone();
        let paras: Vec<String> = if split == "br" {
            // br 模式：按 <br> 元素切块（保持 DOM 结构语义）
            let ids: Vec<NodeId> = match &v {
                Val::Node(id) => vec![*id],
                Val::NodeList(l) => l.clone(),
                _ => vec![],
            };
            let mut out = vec![];
            for id in ids {
                let Some(n) = doc.html.tree.get(id) else { continue };
                let mut cur = String::new();
                for d in n.descendants() {
                    if d.id() == id {
                        continue;
                    }
                    // 剥除判断：自身或（根内）任一祖先命中 strip 选择器 → 整段跳过
                    let is_stripped = std::iter::once(d)
                        .chain(d.ancestors().take_while(|a| a.id() != id))
                        .any(|a| {
                            scraper::ElementRef::wrap(a)
                                .map(|e| strips.iter().any(|st| crate::selector::element_matches(&e, st)))
                                .unwrap_or(false)
                        });
                    if is_stripped {
                        continue;
                    }
                    match d.value() {
                        scraper::node::Node::Text(t) => {
                            let t = t.trim();
                            if !t.is_empty() {
                                if !cur.is_empty() {
                                    cur.push(' ');
                                }
                                cur.push_str(t);
                            }
                        }
                        scraper::node::Node::Element(el) => {
                            if el.name() == "br" {
                                out.push(std::mem::take(&mut cur));
                            }
                        }
                        _ => {}
                    }
                }
                if !cur.trim().is_empty() {
                    out.push(cur);
                }
            }
            out.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
        } else {
            let text = match &v {
                Val::Node(id) => crate::selector::node_text_stripped(&doc.html, *id, false, &strips),
                Val::NodeList(l) => l
                    .iter()
                    .map(|id| crate::selector::node_text_stripped(&doc.html, *id, false, &strips))
                    .collect::<Vec<_>>()
                    .join(" "),
                Val::Str(s) => s.clone(),
                _ => String::new(),
            };
            split_paragraphs(&text, &split, vec![])
        };
        let filtered: Vec<String> = paras
            .into_iter()
            .filter(|p| p.chars().count() >= min_len as usize)
            .collect();
        Ok(Val::StrList(filtered))
    }

    fn strip_css(&self, sel: &SelectorIR) -> Result<Vec<String>, Diagnostic> {
        let mut out = vec![];
        for s in &sel.strip {
            if let BaseSel::Css(c) = &s.base {
                out.push(c.clone());
            }
        }
        Ok(out)
    }

    fn extract_blocks(&mut self, v: Val, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        let Some(doc) = self.doc else {
            return Err(self.err(Code::EType, "blocks 需要 HTML 作用域", "JSON 用 json: 路径"));
        };
        let strips = self.strip_css(sel)?;
        let mut blocks: Vec<Block> = vec![];
        let ids: Vec<NodeId> = match &v {
            Val::Node(id) => vec![*id],
            Val::NodeList(l) => l.clone(),
            _ => vec![],
        };
        let strips_sel: Vec<std::sync::Arc<scraper::Selector>> = strips
            .iter()
            .filter_map(|c| crate::selector::compile_css(c).ok())
            .collect();
        for id in ids {
            let Some(n) = doc.html.tree.get(id) else { continue };
            let Some(el) = scraper::ElementRef::wrap(n) else { continue };
            for d in el.descendants() {
                if d.value().is_element() {
                    let did = d.id();
                    // 被剥除子树跳过
                    let stripped = d
                        .ancestors()
                        .take_while(|a| a.id() != id)
                        .chain(std::iter::once(d))
                        .any(|a| {
                            scraper::ElementRef::wrap(a)
                                .map(|e| strips_sel.iter().any(|sel| crate::selector::element_matches(&e, sel)))
                                .unwrap_or(false)
                        });
                    if stripped {
                        continue;
                    }
                    let tag = crate::selector::node_tag(&doc.html, did);
                    if tag == "img" {
                        if let Some(u) = crate::selector::node_attr(&doc.html, did, "src") {
                            blocks.push(Block { text: None, image: Some(u) });
                        }
                        continue;
                    }
                    // 叶子元素或文本节点聚合段落
                    if let Some(t) = d.first_child() {
                        if t.value().is_text() && d.children().count() == 1 {
                            let txt = crate::selector::node_text(&doc.html, did, true);
                            if !txt.trim().is_empty() {
                                blocks.push(Block { text: Some(txt.trim().to_string()), image: None });
                            }
                        }
                    }
                }
            }
        }
        Ok(Val::BlockList(blocks))
    }

    /// map：对列表每项执行子结构提取（作用域 = 单项 + item.* 通配）。
    fn eval_map(&mut self, v: Val, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        let items: Vec<Val> = match v {
            Val::NodeList(l) => l.into_iter().map(Val::Node).collect(),
            Val::Json(Value::Array(a)) => a.into_iter().map(Val::Json).collect(),
            Val::Json(x) => vec![Val::Json(x)],
            Val::StrList(l) => l.into_iter().map(|s| Val::Str(s)).collect(),
            Val::Empty => vec![],
            other => vec![other],
        };
        let mut out: Vec<Value> = vec![];
        let n = items.len();
        for (i, item) in items.into_iter().enumerate() {
            let mut sub = Eval {
                cx: self.cx,
                doc: self.doc,
                origin: item,
                path: format!("{}.map[{}]", self.path, i),
                stage: self.stage.clone(),
                keep_nodes: false,
                cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
            };
            let mut obj = serde_json::Map::new();
            for (k, fsel) in &sel.map {
                let r = sub.eval_selector(fsel)?;
                obj.insert(k.clone(), finalize_val(r));
            }
            let _ = n;
            out.push(Value::Object(obj));
        }
        Ok(Val::Json(Value::Array(out)))
    }

    fn apply_strip(&mut self, v: Val, _sel: &SelectorIR) -> Result<Val, Diagnostic> {
        // strip 的效果在文本/段落提取时体现（node_text_stripped）；节点列表原样保留
        Ok(v)
    }

    /// 当前选择器的 strip 选择器列表（已编译）。
    fn compiled_strips(&self, sel: &SelectorIR) -> Result<Vec<std::sync::Arc<scraper::Selector>>, Diagnostic> {
        let mut strips = vec![];
        for s in &sel.strip {
            if let BaseSel::Css(c) = &s.base {
                strips.push(crate::selector::compile_css(c)?);
            }
        }
        Ok(strips)
    }

    fn apply_index_ops(&mut self, v: Val, sel: &SelectorIR) -> Result<Val, Diagnostic> {
        // 收敛为列表
        let as_list = |v: &Val| -> Vec<Val> {
            match v {
                Val::StrList(l) => l.iter().map(|s| Val::Str(s.clone())).collect(),
                Val::NodeList(l) => l.iter().map(|n| Val::Node(*n)).collect(),
                Val::Json(Value::Array(a)) => a.iter().map(|x| Val::Json(x.clone())).collect(),
                Val::BlockList(l) => l.iter().map(|b| Val::Json(serde_json::to_value(b).unwrap())).collect(),
                other => vec![other.clone()],
            }
        };
        let from_list = |l: Vec<Val>| -> Val {
            if l.is_empty() {
                return Val::Empty;
            }
            match &l[0] {
                Val::Str(_) => {
                    let strs: Vec<String> = l.into_iter().map(|x| x.to_text()).collect();
                    if strs.len() == 1 {
                        Val::Str(strs[0].clone())
                    } else {
                        Val::StrList(strs)
                    }
                }
                Val::Node(_) => {
                    let ids: Vec<NodeId> = l.into_iter().filter_map(|x| match x {
                        Val::Node(id) => Some(id),
                        _ => None,
                    }).collect();
                    if ids.len() == 1 {
                        Val::Node(ids[0])
                    } else {
                        Val::NodeList(ids)
                    }
                }
                Val::Json(_) => {
                    let vs: Vec<Value> = l.into_iter().map(|x| x.to_json()).collect();
                    if vs.len() == 1 {
                        Val::Json(vs[0].clone())
                    } else {
                        Val::Json(Value::Array(vs))
                    }
                }
                _ => Val::Json(Value::Array(l.iter().map(|x| x.to_json()).collect())),
            }
        };
        // exclude 可与 range/pick 共存（先取子集再排除）
        if sel.range.is_some() || sel.pick.is_some() || !sel.exclude.is_empty() {
            let items = as_list(&v);
            let mut selected: Vec<Val> = if let Some((s, e, st)) = &sel.range {
                slice_indices(items.len(), *s, *e, st.unwrap_or(1))
                    .into_iter()
                    .filter_map(|i| items.get(i).cloned())
                    .collect()
            } else if let Some(picks) = &sel.pick {
                picks
                    .iter()
                    .filter_map(|p| {
                        let idx = if *p < 0 { items.len() as i64 + *p } else { *p };
                        if idx >= 0 && (idx as usize) < items.len() {
                            items.get(idx as usize).cloned()
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                items.clone()
            };
            if !sel.exclude.is_empty() {
                let excl: std::collections::BTreeSet<usize> = sel
                    .exclude
                    .iter()
                    .filter_map(|p| {
                        let idx = if *p < 0 { selected.len() as i64 + *p } else { *p };
                        if idx >= 0 && (idx as usize) < selected.len() {
                            Some(idx as usize)
                        } else {
                            None
                        }
                    })
                    .collect();
                selected = selected
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| !excl.contains(i))
                    .map(|(_, x)| x)
                    .collect();
            }
            if let Some(limit) = sel.limit {
                selected.truncate(limit);
            }
            return Ok(from_list(selected));
        }
        Ok(v)
    }

    fn apply_where(&mut self, v: Val, w: &WhereIR) -> Result<Val, Diagnostic> {
        match v {
            Val::NodeList(l) => {
                let Some(doc) = self.doc else { return Ok(Val::NodeList(l)) };
                let mut kept = vec![];
                for id in l {
                    if self.node_where(doc, id, w)? {
                        kept.push(id);
                    }
                }
                Ok(Val::NodeList(kept))
            }
            Val::Json(Value::Array(a)) => {
                let mut kept = vec![];
                for item in a {
                    if self.json_where(&item, w)? {
                        kept.push(item);
                    }
                }
                Ok(Val::Json(Value::Array(kept)))
            }
            other => Ok(other),
        }
    }

    fn node_where(&mut self, doc: &HtmlDoc, id: NodeId, w: &WhereIR) -> Result<bool, Diagnostic> {
        Ok(match w {
            WhereIR::Has(h) => {
                if let BaseSel::Css(c) = &h.base {
                    crate::selector::node_has_descendant(&doc.html, id, c)
                } else {
                    false
                }
            }
            WhereIR::Not(inner) => !self.node_where(doc, id, inner)?,
            WhereIR::Text(src) => {
                let re = compile_regex(src, "", &self.path)?;
                let text = crate::selector::node_text(&doc.html, id, false);
                re.is_match(&text)
            }
            WhereIR::Cmp { op, val } => {
                let text = crate::selector::node_text(&doc.html, id, false).trim().to_string();
                cmp_values(&Val::Str(text), op, &val.to_json())
            }
        })
    }

    fn json_where(&mut self, item: &Value, w: &WhereIR) -> Result<bool, Diagnostic> {
        Ok(match w {
            WhereIR::Has(_) => false,
            WhereIR::Not(inner) => !self.json_where(item, inner)?,
            WhereIR::Text(src) => {
                let re = compile_regex(src, "", &self.path)?;
                let s = match item {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                re.is_match(&s)
            }
            WhereIR::Cmp { op, val } => cmp_values(&Val::Json(item.clone()), op, &val.to_json()),
        })
    }

    fn check_cond(&mut self, v: &Val, c: &Cond) -> Result<bool, Diagnostic> {
        Ok(cmp_values(v, &c.op, &c.val.to_json()))
    }

    // ───────────── 管道 ─────────────

    pub fn run_transform(
        &mut self,
        val: Val,
        steps: &[StepIR],
        sel: Option<&SelectorIR>,
    ) -> Result<Val, Diagnostic> {
        // 选择器层 protect：作用于整个 transform（S§3.6.2）
        let outer_prot: Option<(Vec<String>, Vec<String>)> = match sel {
            Some(s) => match &s.protect {
                Some(ProtectIR::Literals(ls)) => Some(make_placeholders(ls)),
                _ => None,
            },
            None => None,
        };
        let mut cur = val;
        if let Some((lits, phs)) = &outer_prot {
            cur = apply_placeholders(cur, lits, phs);
        }
        // 管道内 protect：从该步开始生效
        let mut active_prot: Option<(Vec<String>, Vec<String>)> = None;
        for (i, step) in steps.iter().enumerate() {
            if let StepIR::Protect(ProtectIR::Literals(ls)) = step {
                if active_prot.is_none() {
                    active_prot = Some(make_placeholders(ls));
                }
                let (lits, phs) = active_prot.as_ref().unwrap();
                cur = apply_placeholders(cur, lits, phs);
                continue;
            }
            cur = self.apply_step(cur, step, i)?;
        }
        if let Some((lits, phs)) = &active_prot {
            cur = restore_placeholders(cur, lits, phs, &self.path)?;
        }
        if let Some((lits, phs)) = &outer_prot {
            cur = restore_placeholders(cur, lits, phs, &self.path)?;
        }
        Ok(cur)
    }

    pub fn apply_step(&mut self, val: Val, step: &StepIR, idx: usize) -> Result<Val, Diagnostic> {
        crate::exec::ctx::tick_steps(self.cx, 1)?;
        // D5 运行时兑现：Node/NodeList 输入在步骤边界隐式文本提取
        let val = match val {
            Val::Node(_) | Val::NodeList(_) => {
                let strips = self.cur_strips.borrow().clone();
                self.extract_text_kind(val, false)
            }
            other => other,
        };

        Ok(match step {
            StepIR::Trim { chars } => {
                let trims: Vec<char> = chars
                    .clone()
                    .map(|cs| cs.iter().flat_map(|s| s.chars()).collect())
                    .unwrap_or_else(|| vec![' ', '\t', '\n', '\r', '\u{00a0}', '\u{3000}']);
                self.map_strings(val, |s| {
                    let r = s.trim_matches(|c| trims.contains(&c));
                    r.to_string()
                })?
            }
            StepIR::Replace { pairs } => self.map_strings_nodes(val, |s| {
                let mut out = s.to_string();
                for (from, to) in pairs {
                    if let Some(src) = from.strip_prefix("re:") {
                        if let Ok(re) = regex::Regex::new(src) {
                            out = re.replace_all(&out, to.replace('$', "$$").replace("$$", "$")).to_string();
                        }
                    } else {
                        out = out.replace(from.as_str(), to);
                    }
                }
                out
            })?,
            StepIR::Regex { src, repl, all } => {
                let re = compile_regex(src, "", &self.path)?;
                self.map_strings(val, |s| {
                    if *all {
                        re.replace_all(s, expand_repl(repl, &re, s)).to_string()
                    } else {
                        re.replace(s, expand_repl(repl, &re, s)).to_string()
                    }
                })?
            }
            StepIR::Match { src, n, group } => {
                let re = compile_regex(src, "", &self.path)?;
                self.map_strings(val, |s| {
                    let mut cnt = 0usize;
                    let want = n.max(&1);
                    for caps in re.captures_iter(s) {
                        cnt += 1;
                        if cnt == *want {
                            if let Some(g) = group {
                                if let Ok(gi) = g.parse::<usize>() {
                                    if let Some(m) = caps.get(gi) {
                                        return m.as_str().to_string();
                                    }
                                } else if let Some(m) = caps.name(g) {
                                    return m.as_str().to_string();
                                }
                            }
                            if let Some(m) = caps.get(1) {
                                return m.as_str().to_string();
                            }
                            return caps.get(0).map(|m| m.as_str().to_string()).unwrap_or_default();
                        }
                    }
                    String::new()
                })?
            }
            StepIR::Prepend(p) => self.map_strings(val, |s| format!("{p}{s}"))?,
            StepIR::Append(a) => self.map_strings(val, |s| format!("{s}{a}"))?,
            StepIR::Absolute { base } => {
                let base_url = match base {
                    Some(ParamExpr::Template(_)) | Some(ParamExpr::Expr(_)) | Some(ParamExpr::Raw(_)) => {
                        let host = ExprHostImpl { cx: self.cx, doc_val: self.origin.clone() };
                        crate::exec::ctx::eval_param(base.as_ref().unwrap(), &host, &self.path)?
                            .as_str()
                            .unwrap_or_default()
                            .to_string()
                    }
                    _ => self.cx.get_var("baseUrl").and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
                };
                self.map_strings(val, |s| absolutize(&base_url, s))?
            }
            StepIR::Urlencode { charset } => {
                let cs = charset.clone();
                self.map_strings(val, |s| crate::crypto::url_encode(s, cs.as_deref()))?
            }
            StepIR::Urldecode { charset } => {
                let cs = charset.clone();
                self.map_strings(val, |s| crate::crypto::url_decode(s, cs.as_deref()).unwrap_or_else(|_| s.to_string()))?
            }
            StepIR::Base64(enc) => {
                let e = *enc;
                match val {
                    Val::Bytes(b) if e => Val::Str(crate::crypto::base64_encode(&b)),
                    Val::Str(s) if e => Val::Str(crate::crypto::base64_encode(s.as_bytes())),
                    Val::Str(s) => Val::Bytes(crate::crypto::base64_decode(&s)?),
                    Val::Bytes(_) if !e => return Err(self.err(Code::EType, "bytes 上做 base64 decode 无意义", "decode 用于 Str 输入")),
                    other => other,
                }
            }
            StepIR::Hex(enc) => {
                let e = *enc;
                match val {
                    Val::Str(s) if e => Val::Str(hex::encode(s.as_bytes())),
                    Val::Bytes(b) if e => Val::Str(hex::encode(&b)),
                    Val::Str(s) => {
                        let b = hex::decode(s.trim()).map_err(|e| {
                            self.err(Code::EScript, format!("Hex 解码失败：{e}"), "输入必须是偶数位十六进制")
                        })?;
                        Val::Bytes(b)
                    }
                    other => other,
                }
            }
            StepIR::Crypto { alg, mode, key, iv, padding } => {
                let host = ExprHostImpl { cx: self.cx, doc_val: self.origin.clone() };
                let key_str = crate::exec::ctx::eval_param(key, &host, &self.path)?
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let iv_str = match iv {
                    Some(x) => crate::exec::ctx::eval_param(x, &host, &self.path)?
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    None => String::new(),
                };
                let data: Vec<u8> = match &val {
                    Val::Bytes(b) => b.clone(),
                    Val::Str(s) => crate::crypto::base64_decode(&s).unwrap_or_else(|_| s.clone().into_bytes()),
                    _ => {
                        return Err(self.err(Code::EType, format!("crypto 需要 Str|Bytes，输入是 {}", val.type_name()), "—"))
                    }
                };
                let out = crate::crypto::symmetric(
                    alg,
                    if matches!(mode, CryptoMode::Encrypt) { "encrypt" } else { "decrypt" },
                    key_str.as_bytes(),
                    iv_str.as_bytes(),
                    &data,
                    padding.as_deref(),
                )?;
                Val::Bytes(out)
            }
            StepIR::Font { map_fn, reference } => {
                let _ = reference;
                let engine = self.cx_script_engine()?;
                match engine {
                    Some(e) => {
                        let path = self.path.clone();
                        self.map_strings(val, |s| {
                            let mut out = String::new();
                            for c in s.chars() {
                                let arg = Value::String(c.to_string());
                                let r = e.call_pure(map_fn, &[&arg], &path)
                                    .unwrap_or_else(|_| Value::String(c.to_string()));
                                out.push_str(r.as_str().unwrap_or(&c.to_string()));
                            }
                            out
                        })?
                    }
                    None => val,
                }
            }
            StepIR::Html => self.map_strings(val, crate::selector::decode_entities)?,
            StepIR::JsonStep => match val {
                Val::Str(s) => {
                    let cleaned = strip_json_noise(&s);
                    let parsed: Value = serde_json::from_str(&cleaned).map_err(|e| {
                        self.err(Code::EScript, format!("JSON 解析失败：{e}"), "检查响应片段；或用 transform.regex 先抽取")
                    })?;
                    Val::Json(parsed)
                }
                Val::Json(v) => Val::Json(v),
                other => return Err(self.err(Code::EType, format!("json 步骤需要 Str，输入是 {}", other.type_name()), "先取文本")),
            },
            StepIR::Date { from: _, to, tz } => {
                let tz = *tz;
                self.map_strings(val, |s| transform_date(s, to, tz))?
            }
            StepIR::Number { unit } => {
                let u = *unit;

                match val {
                    Val::Str(s) => Val::Num(parse_number(&s, u)),
                    Val::StrList(l) => Val::Num(l.first().map(|s| parse_number(s, u)).unwrap_or(0.0)),
                    Val::NodeList(_) | Val::Node(_) => {
                        let s = val.to_text();
                        Val::Num(parse_number(&s, u))
                    }
                    Val::Num(n) => Val::Num(n),
                    other => return Err(self.err(Code::EType, format!("number 需要 Str，输入是 {}", other.type_name()), "—")),
                }
            }
            StepIR::NumChapter => self.map_strings(val, num_chapter)?,
            StepIR::CjkNumber(to) => {
                let t = *to;
                self.map_strings(val, |s| if t { num_chapter(s) } else { num_to_cjk(s) })?
            }
            StepIR::T2s => self.map_strings(val, t2s)?,
            StepIR::S2t => self.map_strings(val, s2t)?,
            StepIR::Slice { start, end, step } => {
                let (s, e, st) = (*start, *end, step.unwrap_or(1));
                match val {
                    Val::Str(s2) => {
                        let chars: Vec<char> = s2.chars().collect();
                        let idxs = slice_indices(chars.len(), s, e, st);
                        let out: String = idxs.into_iter().filter_map(|i| chars.get(i).copied()).collect();
                        Val::Str(out)
                    }
                    other => return Err(self.err(Code::EType, format!("slice 需要 Str，输入是 {}", other.type_name()), "数组切片用选择器 range")),
                }
            }
            StepIR::Charset(cs) => match val {
                Val::Bytes(b) => {
                    let (r, _, _) = match cs.as_str() {
                        "gbk" | "gb2312" | "gb18030" => encoding_rs::GBK.decode(&b),
                        "big5" => encoding_rs::BIG5.decode(&b),
                        _ => encoding_rs::UTF_8.decode(&b),
                    };
                    Val::Str(r.into_owned())
                }
                other => return Err(self.err(Code::EType, format!("charset 需要 Bytes，输入是 {}", other.type_name()), "仅 response:\"bytes\" 时有意义")),
            },
            StepIR::Join(sep) => match val {
                Val::StrList(l) => Val::Str(l.join(sep)),
                Val::NodeList(l) => {
                    let doc = self.doc.ok_or_else(|| self.err(Code::EType, "join 需要 HTML 作用域取文本", "—"))?;
                    let strs: Vec<String> = l.iter().map(|id| crate::selector::node_text(&doc.html, *id, false)).map(|s| s.trim().to_string()).collect();
                    Val::Str(strs.join(sep))
                }
                Val::Json(Value::Array(a)) => Val::Str(
                    a.iter()
                        .map(|x| match x {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(sep),
                ),
                other => return Err(self.err(Code::EType, format!("join 需要 str[]，输入是 {}", other.type_name()), "join 用于列表")),
            },
            StepIR::Split(sep) => {
                let s = val.to_text();
                if sep.is_empty() {
                    Val::StrList(vec![s])
                } else {
                    Val::StrList(s.split(sep.as_str()).map(|p| p.to_string()).collect())
                }
            }
            StepIR::Zip { with, sep } => {
                // with 在父作用域求值（S§3.6.1）
                let mut sub = Eval {
                    cx: self.cx,
                    doc: self.doc,
                    origin: self.origin.clone(),
                    path: format!("{}.zip.with", self.path),
                    stage: self.stage.clone(),
                keep_nodes: false,
                cur_strips: std::rc::Rc::new(std::cell::RefCell::new(vec![])),
                };
                let other_v = sub.eval_selector(with)?;
                let other: Vec<String> = match other_v {
                    Val::StrList(l) => l,
                    Val::Str(s) => vec![s],
                    Val::Json(Value::Array(a)) => a
                        .iter()
                        .map(|x| match x {
                            Value::String(s) => s.clone(),
                            o => o.to_string(),
                        })
                        .collect(),
                    other => vec![other.to_text()],
                };
                match val {
                    Val::StrList(l) => {
                        if l.len() != other.len() {
                            self.cx.warn(Diagnostic::new(
                                Code::WZipLength,
                                self.path.clone(),
                                format!("zip 两列长度不等：{} vs {}，已丢弃多余项", l.len(), other.len()),
                                "两列可能来自不同容器；确认 with 在父作用域求值",
                            )
                            .rule(&with.raw));
                        }
                        let n = l.len().min(other.len());
                        Val::StrList(
                            (0..n)
                                .map(|i| {
                                    if sep.is_empty() {
                                        format!("{}{}", l[i], other[i])
                                    } else {
                                        format!("{}{}{}", l[i], sep, other[i])
                                    }
                                })
                                .collect(),
                        )
                    }
                    other => return Err(self.err(Code::EType, format!("zip 需要 str[]，输入是 {}", other.type_name()), "zip 对列表交叉合并")),
                }
            }
            StepIR::Protect(_) => val, // 在 run_transform 入口已处理
            StepIR::Unique => {
                let items = list_of(&val);
                let mut seen = std::collections::BTreeSet::new();
                let mut out = vec![];
                for x in items {
                    let k = x.to_text();
                    if seen.insert(k) {
                        out.push(x);
                    }
                }
                rebuild_list(out)?
            }
            StepIR::Sort { desc } => {
                let mut items = list_of(&val);
                items.sort_by_key(|x| x.to_text());
                if *desc {
                    items.reverse();
                }
                rebuild_list(items)?
            }
            StepIR::DropEmpty => {
                let items = list_of(&val);
                rebuild_list(items.into_iter().filter(|x| !x.to_text().trim().is_empty()).collect())?
            }
            StepIR::Js { fn_name, tier } => {
                let engine = self.cx_script_engine()?;
                let Some(engine) = engine else {
                    return Err(self.err(Code::EScript, format!("js 调用需要脚本引擎（{fn_name}）"), "源声明 features 含 js 且宿主提供引擎"));
                };
                crate::exec::ctx::tick_steps(self.cx, 1)?;
                let input = val_json(&val);
                let out = match tier {
                    Tier::Pure => engine.call_pure(fn_name, &[&input], &self.path)?,
                    Tier::Ctx => {
                        let host = crate::exec::hosts::CtxHost { cx: self.cx };
                        engine.call_ctx(fn_name, &[&input], &host, &self.path)?
                    }
                };
                Value::from(out).into_val()
            }
        })
    }

    fn cx_script_engine(
        &mut self,
    ) -> Result<Option<std::rc::Rc<dyn crate::script::ScriptEngine>>, Diagnostic> {
        // 由 stage 层注入的引擎；这里从全局取（exec 不依赖 script crate 内部）
        Ok(crate::script::get_engine())
    }

    fn map_strings(&mut self, val: Val, f: impl Fn(&str) -> String) -> Result<Val, Diagnostic> {
        Ok(match val {
            Val::Str(s) => Val::Str(f(&s)),
            Val::StrList(l) => Val::StrList(l.iter().map(|s| f(s)).collect()),
            Val::Node(_) | Val::NodeList(_) => {
                let s = self.extract_text_kind(val, false);
                match s {
                    Val::Str(s) => Val::Str(f(&s)),
                    Val::StrList(l) => Val::StrList(l.iter().map(|s| f(s)).collect()),
                    other => other,
                }
            }
            Val::Json(Value::Array(a)) => Val::Json(Value::Array(
                a.iter()
                    .map(|x| {
                        if let Value::String(s) = x {
                            Value::String(f(s))
                        } else {
                            x.clone()
                        }
                    })
                    .collect(),
            )),
            Val::Json(Value::String(s)) => Val::Json(Value::String(f(&s))),
            other => other,
        })
    }

    fn map_strings_nodes(
        &mut self,
        val: Val,
        f: impl Fn(&str) -> String,
    ) -> Result<Val, Diagnostic> {
        // replace 对 Node 输入：取 innerHTML → 替换 → Str
        Ok(match &val {
            Val::Node(_) | Val::NodeList(_) => {
                let s = self.extract_text_kind(val.clone(), false);
                match s {
                    Val::Str(s) => Val::Str(f(&s)),
                    Val::StrList(l) => Val::StrList(l.iter().map(|s| f(s)).collect()),
                    o => o,
                }
            }
            _ => self.map_strings(val, f)?,
        })
    }
}

// ───────────── 辅助 ─────────────

trait IntoVal {
    fn into_val(self) -> Val;
}

impl IntoVal for Value {
    fn into_val(self) -> Val {
        match self {
            Value::String(s) => Val::Str(s),
            Value::Array(_) | Value::Object(_) => Val::Json(self),
            other => Val::Json(other),
        }
    }
}

impl IntoVal for crate::expr::EVal {
    fn into_val(self) -> Val {
        match self {
            crate::expr::EVal::Num(n) => Val::Num(n),
            crate::expr::EVal::Str(s) => Val::Str(s),
            crate::expr::EVal::Bool(b) => Val::Bool(b),
            crate::expr::EVal::Null => Val::Empty,
            crate::expr::EVal::List(l) => Val::Json(Value::Array(l.iter().map(|x| x.to_json()).collect())),
        }
    }
}

pub fn finalize_val(v: Val) -> Value {
    match v {
        Val::Node(_) | Val::NodeList(_) => {
            // 未提取文本的节点：空串（不应该发生——默认提取已处理）
            Value::String(String::new())
        }
        Val::Empty => Value::Null,
        Val::Num(n) => serde_json::json!(n),
        Val::Bool(b) => Value::Bool(b),
        other => other.to_json(),
    }
}

fn val_scalar_sample(v: &Val) -> String {
    let s = v.to_text();
    let mut out: String = s.chars().take(60).collect();
    if s.chars().count() > 60 {
        out.push('…');
    }
    out
}

fn val_json(v: &Val) -> Value {
    match v {
        Val::Str(s) => Value::String(s.clone()),
        Val::StrList(l) => Value::Array(l.iter().map(|s| Value::String(s.clone())).collect()),
        Val::Json(x) => x.clone(),
        Val::Bytes(b) => Value::String(String::from_utf8_lossy(b).to_string()),
        Val::BlockList(l) => serde_json::to_value(l).unwrap_or(Value::Null),
        Val::Empty => Value::Null,
        Val::Num(n) => serde_json::json!(n),
        Val::Bool(b) => Value::Bool(*b),
        Val::Node(_) | Val::NodeList(_) => Value::String(String::new()),
    }
}

fn list_of(v: &Val) -> Vec<Val> {
    match v {
        Val::StrList(l) => l.iter().map(|s| Val::Str(s.clone())).collect(),
        Val::NodeList(l) => l.iter().map(|n| Val::Node(*n)).collect(),
        Val::Json(Value::Array(a)) => a.iter().map(|x| Val::Json(x.clone())).collect(),
        Val::BlockList(l) => l.iter().map(|b| Val::Json(serde_json::to_value(b).unwrap())).collect(),
        other => vec![other.clone()],
    }
}

fn rebuild_list(items: Vec<Val>) -> Result<Val, Diagnostic> {
    Ok(match items.first() {
        Some(Val::Str(_)) => {
            let strs: Vec<String> = items.iter().map(|x| x.to_text()).collect();
            Val::StrList(strs)
        }
        Some(Val::Json(_)) => Val::Json(Value::Array(items.iter().map(|x| x.to_json()).collect())),
        _ => Val::StrList(items.iter().map(|x| x.to_text()).collect()),
    })
}

pub fn compile_regex(src: &str, flags: &str, path: &str) -> Result<regex::Regex, Diagnostic> {
    let mut pattern = String::new();
    if flags.contains('m') {
        pattern.push_str("(?m)");
    }
    if flags.contains('s') {
        pattern.push_str("(?s)");
    }
    if flags.contains('i') {
        pattern.push_str("(?i)");
    }
    pattern.push_str(src);
    regex::Regex::new(&pattern).map_err(|e| {
        Diagnostic::new(
            Code::ERegex,
            path,
            format!("正则编译失败：{e}"),
            "检查括号配对与转义",
        )
    })
}

pub fn slice_indices(len: usize, start: Option<i64>, end: Option<i64>, step: i64) -> Vec<usize> {
    let n = len as i64;
    let norm = |x: i64| -> i64 {
        let v = if x < 0 { n + x } else { x };
        v.clamp(0, n)
    };
    let s = start.map(norm).unwrap_or(if step > 0 { 0 } else { n - 1 });
    let e = end.map(norm).unwrap_or(if step > 0 { n } else { -1 });
    let mut out = vec![];
    if step > 0 {
        let mut i = s;
        while i < e {
            if i >= 0 && i < n {
                out.push(i as usize);
            }
            i += step;
        }
    } else {
        let mut i = s;
        while i > e {
            if i >= 0 && i < n {
                out.push(i as usize);
            }
            i += step;
        }
    }
    out
}

pub fn absolutize(base: &str, s: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with("http://") || t.starts_with("https://") || t.starts_with("data:") {
        return t.to_string();
    }
    match url::Url::parse(base) {
        Ok(b) => match b.join(t) {
            Ok(u) => u.to_string(),
            Err(_) => t.to_string(),
        },
        Err(_) => {
            if let Some(stripped) = base.strip_suffix('/') {
                format!("{stripped}{t}")
            } else {
                format!("{base}/{t}")
            }
        }
    }
}

/// regex 替换的 $1 展开（rust regex 的 $name 语义一致）。
fn expand_repl(tpl: &str, re: &regex::Regex, s: &str) -> String {
    if let Some(caps) = re.captures(s) {
        let mut out = tpl.to_string();
        for gi in (1..caps.len()).rev() {
            let rep = caps.get(gi).map(|m| m.as_str()).unwrap_or("");
            out = out.replace(&format!("${gi}"), rep);
        }
        return out;
    }
    tpl.to_string()
}

/// 段落切分（split: br/block/line/auto）。
pub fn split_paragraphs(text: &str, split: &str, strips: Vec<String>) -> Vec<String> {
    let _ = strips;
    let nl: char = char::from_u32(10).unwrap();
    let cr: char = char::from_u32(13).unwrap();
    let parts: Vec<String> = match split {
        "br" => text
            .replace("<br/>", "\u{000A}")
            .replace("<br>", "\u{000A}")
            .replace("<br />", "\u{000A}")
            .split(nl)
            .map(|p| p.to_string())
            .collect(),
        "line" => text.split(nl).map(|p| p.to_string()).collect(),
        "block" => text
            .split(&[nl, cr][..])
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.to_string())
            .collect(),
        _ => text
            .split("\u{000A}\u{000A}")
            .flat_map(|blk| blk.split(nl))
            .map(|p| p.to_string())
            .collect(),
    };
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn make_placeholders(lits: &[String]) -> (Vec<String>, Vec<String>) {
    let mut l = vec![];
    let mut p = vec![];
    for (i, x) in lits.iter().enumerate() {
        l.push(x.clone());
        p.push(format!("\u{0}p{i}\u{0}"));
    }
    (l, p)
}

fn apply_placeholders(v: Val, lits: &[String], phs: &[String]) -> Val {
    match v {
        Val::Str(s) => {
            let mut out = s;
            for (lit, ph) in lits.iter().zip(phs.iter()) {
                out = out.replace(lit.as_str(), ph);
            }
            Val::Str(out)
        }
        Val::StrList(l) => Val::StrList(
            l.into_iter()
                .map(|s| {
                    let mut out = s;
                    for (lit, ph) in lits.iter().zip(phs.iter()) {
                        out = out.replace(lit.as_str(), ph);
                    }
                    out
                })
                .collect(),
        ),
        other => other,
    }
}

fn restore_placeholders(
    v: Val,
    lits: &[String],
    phs: &[String],
    path: &str,
) -> Result<Val, Diagnostic> {
    let restore = |s: String| -> Result<String, Diagnostic> {
        let mut out = s.clone();
        for (i, ph) in phs.iter().enumerate() {
            if s.contains(ph) {
                out = out.replace(ph, &lits[i]);
            }
            // 占位符未出现：原文本就无该片段，无需还原
        }
        Ok(out)
    };
    Ok(match v {
        Val::Str(s) => Val::Str(restore(s)?),
        Val::StrList(l) => {
            let mut out = vec![];
            for s in l {
                out.push(restore(s)?);
            }
            Val::StrList(out)
        }
        Val::Json(Value::Array(a)) => {
            let mut out = vec![];
            for x in a {
                if let Value::String(st) = x {
                    out.push(Value::String(restore(st)?));
                } else {
                    out.push(x);
                }
            }
            Val::Json(Value::Array(out))
        }
        other => other,
    })
}

/// JSON 前噪声剥离（JSONP/HTML 包裹）。
fn strip_json_noise(s: &str) -> String {
    let t = s.trim();
    // JSONP: fn({...})
    if let (Some(i), Some(j)) = (t.find('('), t.rfind(')')) {
        if i < j && (t[..i].contains("Jsonp") || t[..i].contains("callback") || !t[..i].contains(' ')) {
            let inner = &t[i + 1..j];
            if inner.trim_start().starts_with('{') {
                return inner.to_string();
            }
        }
    }
    // HTML 注释包裹
    let t2 = t
        .trim_start_matches(|c: char| c.is_whitespace() || c == '<' && false)
        .trim();
    // 外层 HTML 里找 <script> 或 {...}
    if t2.starts_with('{') || t2.starts_with('[') {
        return t2.to_string();
    }
    if let (Some(i), Some(j)) = (t2.find('{'), t2.rfind('}')) {
        if i < j {
            return t2[i..=j].to_string();
        }
    }
    t2.to_string()
}

fn parse_number(s: &str, unit: bool) -> f64 {
    let t: String = s
        .chars()
        .map(|c| {
            match c {
                '，' | '．' => '.',
                '０'..='９' => ((c as u32) - 0xFF10 + 0x30) as u8 as char,
                _ => c,
            }
        })
        .collect();
    let cleaned: String = t.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    let mut n: f64 = cleaned.parse().unwrap_or(0.0);
    if unit {
        let low = s.to_lowercase();
        if low.contains("万") || low.contains("w") {
            n *= 10_000.0;
        }
        if low.contains("亿") {
            n *= 100_000_000.0;
        }
        if low.contains("k") && !low.contains("万") {
            n *= 1000.0;
        }
        if low.contains("m") && !low.contains("万") {
            n *= 1_000_000.0;
        }
    }
    n
}

/// "第一百二十三章" → "123"
pub fn num_chapter(s: &str) -> String {
    // 先找数字
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        return digits;
    }
    let map = cjk_digit_map();
    let chars: Vec<char> = s.chars().collect();
    let mut total: i64 = -1;
    let mut cur: i64 = 0;
    let mut saw_cjk = false;
    for c in chars {
        if let Some(d) = map.get(&c) {
            saw_cjk = true;
            cur = cur * 10 + *d as i64;
        } else {
            match c {
                '十' => {
                    saw_cjk = true;
                    if cur == 0 {
                        cur = 1;
                    }
                    total = total.max(0) + cur * 10;
                    cur = 0;
                }
                '百' => {
                    saw_cjk = true;
                    if cur == 0 {
                        cur = 1;
                    }
                    total = total.max(0) + cur * 100;
                    cur = 0;
                }
                '千' => {
                    saw_cjk = true;
                    if cur == 0 {
                        cur = 1;
                    }
                    total = total.max(0) + cur * 1000;
                    cur = 0;
                }
                '万' => {
                    saw_cjk = true;
                    if cur == 0 {
                        cur = 1;
                    }
                    total = (total.max(0) + cur) * 10_000;
                    cur = 0;
                }
                '两' => {
                    saw_cjk = true;
                    cur = cur * 10 + 2;
                }
                _ => {
                    if saw_cjk && cur >= 0 {
                        total = total.max(0) + cur;
                        cur = 0; // 尾部非数字字符（如"章"）已并入，避免重复累计
                    }
                }
            }
        }
    }
    if saw_cjk {
        total = total.max(0) + cur;
        if total > 0 {
            return total.to_string();
        }
    }
    String::new()
}

fn cjk_digit_map() -> std::collections::HashMap<char, u8> {
    std::collections::HashMap::from([
        ('零', 0u8),
        ('〇', 0),
        ('一', 1),
        ('二', 2),
        ('三', 3),
        ('四', 4),
        ('五', 5),
        ('六', 6),
        ('七', 7),
        ('八', 8),
        ('九', 9),
    ])
}

/// "123" → "一百二十三"
pub fn num_to_cjk(s: &str) -> String {
    let n: i64 = s.trim().parse().unwrap_or(0);
    if n <= 0 {
        return s.to_string();
    }
    let d = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    let mut out = String::new();
    let wan = n / 10_000;
    let rest = n % 10_000;
    if wan > 0 {
        out.push_str(&small_cjk(wan, d));
        out.push('万');
    }
    if rest > 0 {
        out.push_str(&small_cjk(rest, d));
    }
    if out.is_empty() {
        out = "零".into();
    }
    out
}

fn small_cjk(mut n: i64, d: [&str; 10]) -> String {
    let mut out = String::new();
    let qian = n / 1000;
    n %= 1000;
    let bai = n / 100;
    n %= 100;
    let shi = n / 10;
    n %= 10;
    if qian > 0 {
        out.push_str(d[qian as usize]);
        out.push('千');
    }
    if bai > 0 {
        out.push_str(d[bai as usize]);
        out.push('百');
    } else if qian > 0 && (shi > 0 || n > 0) {
        out.push('零');
    }
    if shi > 0 {
        if shi == 1 && qian == 0 && bai == 0 {
            out.push('十');
        } else {
            out.push_str(d[shi as usize]);
            out.push('十');
        }
    } else if bai > 0 && n > 0 {
        out.push('零');
    }
    if n > 0 {
        out.push_str(d[n as usize]);
    }
    if out.is_empty() {
        out = "零".into();
    }
    out
}

/// 简繁转换词表（核心常用字；覆盖常见站名场景）。t2s: 繁→简。
pub fn t2s(s: &str) -> String {
    super::cjk::opencc_t2s(s)
}

pub fn s2t(s: &str) -> String {
    super::cjk::opencc_s2t(s)
}

/// 日期格式化：from 任意（尽力提取），to = yyyy-MM-dd 等模式。
pub fn transform_date(s: &str, to: &str, tz: i64) -> String {
    // 提取数字时间成分
    let nums: Vec<String> = regex::Regex::new(r"\d+")
        .unwrap()
        .find_iter(s)
        .map(|m| m.as_str().to_string())
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 + tz as i64 * 3600)
        .unwrap_or(0);
    // 相对时间：x分钟前 / x小时前 / x天前
    let low = s.to_string();
    let ago = |u: i64| -> i64 { now - u * 60 };
    let stamp: i64 = if low.contains("分钟前") {
        if let Some(n) = nums.first().and_then(|n| n.parse::<i64>().ok()) {
            ago(n)
        } else {
            now
        }
    } else if low.contains("小时前") {
        if let Some(n) = nums.first().and_then(|n| n.parse::<i64>().ok()) {
            ago(n * 60)
        } else {
            now
        }
    } else if low.contains("天前") {
        if let Some(n) = nums.first().and_then(|n| n.parse::<i64>().ok()) {
            ago(n * 1440)
        } else {
            now
        }
    } else if low.contains("昨天") {
        now - 1440 * 60
    } else if nums.len() >= 3 {
        let (y, mo, d) = (
            nums[0].parse::<i64>().unwrap_or(1970),
            nums[1].parse::<i64>().unwrap_or(1),
            nums[2].parse::<i64>().unwrap_or(1),
        );
        civil_to_days(y, mo, d) * 86400
    } else if nums.len() >= 2 {
        // MM-dd → 补当前年
        let y = 2026;
        let (mo, d) = (
            nums[0].parse::<i64>().unwrap_or(1),
            nums[1].parse::<i64>().unwrap_or(1),
        );
        civil_to_days(y, mo, d) * 86400
    } else {
        now
    };
    let (y, mo, d) = days_to_civil(stamp / 86400);
    to.replace("yyyy", &format!("{y:04}"))
        .replace("MM", &format!("{mo:02}"))
        .replace("dd", &format!("{d:02}"))
        .replace("HH", "00")
        .replace("mm", "00")
        .replace("ss", "00")
}

pub fn civil_to_days(y: i64, m: i64, d: i64) -> i64 {
    // Howard Hinnant 算法
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn days_to_civil(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn cmp_values(left: &Val, op: &CondOp, right: &Value) -> bool {
    let lt = left.to_text();
    let rt = match right {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match op {
        CondOp::Exists => !lt.is_empty(),
        CondOp::Contains => lt.contains(&rt),
        CondOp::Eq => {
            if lt == rt {
                true
            } else {
                match (lt.parse::<f64>(), right.as_f64().or_else(|| rt.parse::<f64>().ok())) {
                    (Ok(a), Some(b)) => (a - b).abs() < 1e-9,
                    _ => false,
                }
            }
        }
        CondOp::Ne => !cmp_values(left, &CondOp::Eq, right),
        CondOp::Gt | CondOp::Lt => {
            let (a, b) = match (lt.parse::<f64>(), rt.parse::<f64>()) {
                (Ok(a), Ok(b)) => (a, b),
                _ => return false,
            };
            if matches!(op, CondOp::Gt) {
                a > b
            } else {
                a < b
            }
        }
    }
}
