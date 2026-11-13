//! T1.5 引用图（COMPILER §3.2）：按阶段推导可见变量，逐条 var:/let:/session:/token:/cookie:/cache: 查表。
//! `book.*`/`chapter.*`/`login.*` 的字段集合来自契约声明，不是通配——拼错在编译期变 E_REF。
//! 裁决 D3：book.* 含隐式 `id` 字段（官方示例 video/comic 依赖 var:book.id）。

use std::collections::{BTreeMap, BTreeSet};

/// 某阶段的可见变量集合。
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// 精确路径（如 `page`、`filter.mode`、`book.tocUrl`）。
    pub paths: BTreeSet<String>,
    /// 通配根（如 map 作用域的 `item`）——任意子路径可见。
    pub wildcards: BTreeSet<String>,
    /// let 名（按声明顺序由调用方累积）。
    pub lets: BTreeSet<String>,
    pub session_ok: bool,
}

impl Scope {
    pub fn has(&self, ns: &str, path: &str) -> bool {
        match ns {
            "var" | "" => {
                if self.paths.contains(path) {
                    return true;
                }
                // 通配根：item.id / group.params.cat / login.username
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

/// 内置上下文键（vars 不得冲突）。
pub const BUILTIN_KEYS: &[&str] = &[
    "baseUrl", "url", "response", "charset", "kw", "page", "group", "filter",
    "login", "book", "chapter",
];

/// 五阶段契约保留字段。
pub const BOOK_SEARCH_RESERVED: &[&str] = &["name", "url", "type", "id"];
pub const DETAIL_RESERVED: &[&str] = &[
    "name", "author", "intro", "kind", "cover", "wordCount", "lastChapter",
    "updateTime", "type", "tocUrl", "canRename", "downloads", "extra",
];
pub const CHAPTER_RESERVED: &[&str] = &[
    "title", "url", "isVolume", "isPay", "isVip", "cover", "wordCount", "updateTime",
];

#[derive(Debug, Default, Clone)]
pub struct ContractFields {
    /// search/explore 的 out.map 键（作为 book.* 的来源）。
    pub list_map_keys: BTreeSet<String>,
    /// detail 的 out 键（供 toc/content 的 book.*）。
    pub detail_keys: BTreeSet<String>,
    /// toc 的 out.map 键（作为 chapter.*）。
    pub toc_map_keys: BTreeSet<String>,
}

/// 全局作用域：vars ∪ {baseUrl,url,response,charset}。
pub fn global_scope(vars: &BTreeMap<String, crate::ir::VarIR>) -> Scope {
    let mut s = Scope::default();
    for k in vars.keys() {
        s.paths.insert(k.clone());
    }
    for k in ["baseUrl", "url", "response", "charset"] {
        s.paths.insert(k.to_string());
    }
    s
}

/// 阶段作用域输入（从 model cfg 直接取，避免 IR 未建时信息缺失）。
pub struct StageScopeInfo<'a> {
    pub name: &'a str,
    pub filter_keys: &'a [String],
    pub has_paginate: bool,
    pub is_enumerate: bool,
    pub has_groups: bool,
    pub has_script: bool,
}

/// 阶段作用域（不含 map 作用域的 item）。
pub fn stage_scope(
    info: &StageScopeInfo,
    contract: &ContractFields,
) -> Scope {
    let mut s = Scope::default();
    let name = info.name;
    match name {
        "search" => {
            s.paths.insert("kw".into());
        }
        "explore" => {
            s.wildcards.insert("group".into());
            // filter.<key>
            for f in info.filter_keys {
                s.paths.insert(format!("filter.{f}"));
            }
        }
        "detail" => {
            for k in &contract.list_map_keys {
                s.paths.insert(format!("book.{k}"));
            }
            for k in BOOK_SEARCH_RESERVED {
                s.paths.insert(format!("book.{k}"));
            }
        }
        "toc" => {
            for k in &contract.list_map_keys {
                s.paths.insert(format!("book.{k}"));
            }
            for k in BOOK_SEARCH_RESERVED {
                s.paths.insert(format!("book.{k}"));
            }
            for k in &contract.detail_keys {
                s.paths.insert(format!("book.{k}"));
            }
            for k in DETAIL_RESERVED {
                s.paths.insert(format!("book.{k}"));
            }
        }
        "content" => {
            for k in &contract.list_map_keys {
                s.paths.insert(format!("book.{k}"));
            }
            for k in BOOK_SEARCH_RESERVED {
                s.paths.insert(format!("book.{k}"));
            }
            for k in &contract.detail_keys {
                s.paths.insert(format!("book.{k}"));
            }
            for k in DETAIL_RESERVED {
                s.paths.insert(format!("book.{k}"));
            }
            for k in &contract.toc_map_keys {
                s.paths.insert(format!("chapter.{k}"));
            }
            for k in CHAPTER_RESERVED {
                s.paths.insert(format!("chapter.{k}"));
            }
        }
        _ => {
            // hostStages：同五阶段规则；out.map 声明什么可见什么（走 detail 一套近似）
            for k in &contract.list_map_keys {
                s.paths.insert(format!("book.{k}"));
            }
        }
    }
    // 裁决 D7：enumerate 阶段 into 目标占位 var:pages 视为可见（值由引擎注入）
    if info.is_enumerate {
        s.paths.insert("pages".into());
    }
    // var:page：有 paginate（非 enumerate）或 explore 有 groups
    let page_visible = (info.has_paginate && !info.is_enumerate)
        || (info.name == "explore" && info.has_groups);
    if page_visible {
        s.paths.insert("page".into());
    }
    if info.has_script {
        s.paths.insert("scriptResult".into());
    }
    s
}

/// map 作用域：单条列表项（+ item.* 通配）。
pub fn map_scope(base: &Scope) -> Scope {
    let mut s = base.clone();
    s.wildcards.insert("item".into());
    s
}

/// 登录步骤作用域：+ login.<field>（仅该步的 fields）。
pub fn login_step_scope(base: &Scope, fields: &[String]) -> Scope {
    let mut s = base.clone();
    for f in fields {
        s.paths.insert(format!("login.{f}"));
    }
    s
}
