//! RSL LSP server（TASK-BOOK-2 B1 / V2-D4）。
//! 手写 stdio JSON-RPC（LSP 3.17 基本帧协议）——不引入 tokio/tower-lsp，
//! 与"core 无 tokio"约束同源：server 是薄 I/O 壳，能力全部来自 rsl-core。
//! 能力：publishDiagnostics（compile_str 诊断 + path 定位）/ hover / completion / definition。
//! 启动：rsl-lsp（stdio）。

use rsl_core::error::Diagnostic;
use rsl_core::{compile, sugar};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Mutex};

/// 18 命名空间（与 sugar::NAMESPACES 同步）。
const NS_DOCS: &[(&str, &str)] = &[
    ("css", "CSS 选择器（level 3 全部 + :is/:where/:not/:has，forgiving 处理）"),
    ("xpath", "XPath 子集：//、[@attr]、[n]、text()、*"),
    ("json", "JSONPath（RFC 9535 子集 + 过滤）：$.a.b、$..x、$[?@.vip==false]"),
    ("re", "正则提取：分组/整体捕获"),
    ("var", "变量引用：var:book.id（契约注入或源级 vars 声明）"),
    ("let", "会话派生变量：let:name（stages 顺序求值）"),
    ("session", "会话存储：session:key"),
    ("token", "登录令牌：token:name（login 产出）"),
    ("cookie", "Cookie：cookie:name"),
    ("cache", "缓存块：cache:name"),
    ("lib", "脚本函数引用：lib:fnName（pure/ctx 两档）"),
    ("expr", "内联表达式：expr:md5(var:kw)"),
    ("attr", "属性提取（对象形式 attr 字段优先）"),
    ("const", "常量：const:value"),
    ("text", "裸命名空间：当前节点文本（own+子孙）"),
    ("owntext", "裸命名空间：仅自身文本节点"),
    ("html", "裸命名空间：innerHTML"),
    ("tag", "裸命名空间：标签名"),
];

/// 31 管道步骤键（R§3.6）。
const STEP_KEYS: &[&str] = &[
    "trim", "replace", "regex", "match", "prepend", "append", "absolute", "urlencode",
    "urldecode", "base64", "hex", "html", "json", "date", "number", "numChapter",
    "cjkNumber", "t2s", "s2t", "slice", "join", "split", "zip", "protect", "font",
    "unique", "sort", "dropEmpty", "js", "parseTime", "decode",
];

type DocMap = Arc<Mutex<BTreeMap<String, String>>>;

fn main() {
    // windows-gnu 主线程栈过小（编译器深递归）—— 主逻辑挪 64MB 线程（同 CLI 既往处理）
    let spawned = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(server_loop);
    match spawned {
        Ok(h) => {
            if h.join().is_err() {
                eprintln!("E_PANIC: rsl-lsp 内部 panic（已捕获）");
                std::process::exit(1);
            }
        }
        Err(_) => server_loop(),
    }
}

fn server_loop() {
    let docs: DocMap = Arc::new(Mutex::new(BTreeMap::new()));
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    loop {
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => return, // EOF
                Ok(_) => {}
            }
            let t = line.trim();
            if t.is_empty() {
                break;
            }
            if let Some(v) = t.strip_prefix("Content-Length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        if content_length == 0 {
            continue;
        }
        let mut body = vec![0u8; content_length];
        if stdin.lock().read_exact(&mut body).is_err() {
            return;
        }
        let msg: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => {
                let result = json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "hoverProvider": true,
                        "completionProvider": { "triggerCharacters": [":", ".", "\""] },
                        "definitionProvider": true,
                    },
                    "serverInfo": { "name": "rsl-lsp", "version": rsl_core::VERSION },
                });
                write_msg(&mut out, id, Some(result), None);
            }
            "initialized" => {}
            "shutdown" => write_msg(&mut out, id, Some(Value::Null), None),
            "exit" => return,
            "textDocument/didOpen" | "textDocument/didChange" | "textDocument/didSave" => {
                let text = params
                    .pointer("/textDocument/text")
                    .and_then(|t| t.as_str())
                    .map(String::from);
                let uri = params
                    .pointer("/textDocument/uri")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                if let (Some(t), false) = (text, uri.is_empty()) {
                    docs.lock().unwrap().insert(uri.clone(), t.clone());
                    let diags = diagnostics_of(&t);
                    let version = params.pointer("/textDocument/version").cloned();
                    let note = json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {
                            "uri": uri,
                            "diagnostics": diags,
                            "version": version,
                        }
                    });
                    write_raw(&mut out, &note);
                }
            }
            "textDocument/didClose" => {}
            "textDocument/hover" => {
                let uri = params
                    .pointer("/textDocument/uri")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let text = docs.lock().unwrap().get(&uri).cloned();
                let result = match text {
                    Some(t) => hover_result(&t, &params.pointer("/position").cloned().unwrap_or(Value::Null)),
                    None => Value::Null,
                };
                write_msg(&mut out, id, Some(result), None);
            }
            "textDocument/completion" => {
                let uri = params
                    .pointer("/textDocument/uri")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let text = docs.lock().unwrap().get(&uri).cloned();
                let pos = params.pointer("/position").cloned().unwrap_or(Value::Null);
                let result = match text {
                    Some(t) => completion_result(&t, &pos),
                    None => Value::Null,
                };
                write_msg(&mut out, id, Some(result), None);
            }
            "textDocument/definition" => {
                let uri = params
                    .pointer("/textDocument/uri")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let text = docs.lock().unwrap().get(&uri).cloned();
                let pos = params.pointer("/position").cloned().unwrap_or(Value::Null);
                let result = match text {
                    Some(t) => definition_result(&t, &pos),
                    None => Value::Null,
                };
                write_msg(&mut out, id, Some(result), None);
            }
            _ => {
                if id.is_some() {
                    write_msg(
                        &mut out,
                        id,
                        None,
                        Some(json!({"code": -32601, "message": "MethodNotFound"})),
                    );
                }
            }
        }
    }
}

fn write_raw(out: &mut impl Write, v: &Value) {
    let body = serde_json::to_vec(v).unwrap_or_default();
    let _ = write!(out, "Content-Length: {}\r\n\r\n", body.len());
    let _ = out.write_all(&body);
    let _ = out.flush();
}

fn write_msg(out: &mut impl Write, id: Option<Value>, result: Option<Value>, error: Option<Value>) {
    let mut m = json!({"jsonrpc": "2.0"});
    if let Some(id) = id {
        m["id"] = id;
    }
    if let Some(e) = error {
        m["error"] = e;
    } else {
        m["result"] = result.unwrap_or(Value::Null);
    }
    write_raw(out, &m);
}

// ───────────────────────── 能力实现 ─────────────────────────

fn pos_range(text: &str, key: &str) -> Option<Value> {
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(&format!("\"{key}\"")) {
        let start = from + rel;
        let after = &text[start + key.len() + 2..];
        if after.trim_start().starts_with(':') {
            let line = text[..start].bytes().filter(|b| *b == b'\n').count();
            let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let col = text[line_start..start].chars().count();
            let key_len = key.chars().count() + 2;
            return Some(json!({
                "start": {"line": line, "character": col},
                "end": {"line": line, "character": col + key_len}
            }));
        }
        from = start + key.len() + 2;
        if from >= text.len() {
            break;
        }
    }
    None
}

fn diagnostics_of(text: &str) -> Value {
    let outp = compile(text);
    let mut arr = vec![];
    for d in outp.diagnostics.iter() {
        arr.push(diag_json(d, text));
    }
    Value::Array(arr)
}

fn diag_json(d: &Diagnostic, text: &str) -> Value {
    let sev = match d.severity {
        rsl_core::error::Severity::Error => 1,
        rsl_core::error::Severity::Warning => 2,
        rsl_core::error::Severity::Info => 3,
    };
    let last_seg = d.path.split('.').next_back().unwrap_or(&d.path).to_string();
    let range = pos_range(text, &d.path)
        .or_else(|| pos_range(text, &last_seg))
        .unwrap_or_else(|| json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}));
    json!({
        "range": range,
        "severity": sev,
        "code": d.code.as_str(),
        "source": "rsl",
        "message": format!("{}（at {}）\nhint: {}", d.message, d.path, d.hint),
    })
}

/// 光标处字符串 token（行内向左/右找 " 边界）。
fn token_at(text: &str, pos: &Value) -> Option<(String, Value)> {
    let line = pos.get("line").and_then(|v| v.as_u64())? as usize;
    let ch = pos.get("character").and_then(|v| v.as_u64())? as usize;
    let lines: Vec<&str> = text.lines().collect();
    if line >= lines.len() {
        return None;
    }
    let cur: Vec<char> = lines[line].chars().collect();
    if ch > cur.len() {
        return None;
    }
    let mut s = None;
    for i in (0..ch.min(cur.len())).rev() {
        if cur[i] == '"' {
            s = Some(i);
            break;
        }
    }
    let mut e = None;
    for i in (ch + 1)..cur.len() {
        if cur[i] == '"' {
            e = Some(i);
            break;
        }
    }
    let (s, e) = (s?, e?);
    let content: String = cur[s + 1..e].iter().collect();
    let range = json!({
        "start": {"line": line, "character": s + 1},
        "end": {"line": line, "character": e}
    });
    Some((content, range))
}

fn hover_result(text: &str, pos: &Value) -> Value {
    let Some((token, range)) = token_at(text, pos) else {
        return Value::Null;
    };
    let md = hover_markdown(&token);
    json!({
        "contents": { "kind": "markdown", "value": md },
        "range": range,
    })
}

fn hover_markdown(token: &str) -> String {
    if let Some(path) = token.strip_prefix("var:") {
        let root = path.split('.').next().unwrap_or("");
        let src = match root {
            "book" => "detail 阶段 out（契约注入 S§5.4）",
            "chapter" => "toc 阶段 out.list[].map（契约注入）",
            "kw" | "page" => "搜索输入（RunInputs，宿主注入）",
            "baseUrl" | "charset" | "url" | "response" => "请求上下文变量（fetch 注入）",
            "item" => "逐项作用域（S§3.3，列表当前项）",
            _ => "源级 vars 声明或宿主注入",
        };
        return format!(
            "**var:{path}**\n\n- 来源：{src}\n- 求值：路径 `{path}`（点路径取值）"
        );
    }
    if let Some(rest) = token.strip_prefix("expr:") {
        return format!(
            "**expr:{rest}**\n\n内联表达式（R§4.4）：编译期类型检查 + 运行期求值"
        );
    }
    let ns = token.split(':').next().unwrap_or("");
    if let Some((_, doc)) = NS_DOCS.iter().find(|(n, _)| *n == ns) {
        return format!("**{ns}:**\n\n{doc}");
    }
    if STEP_KEYS.contains(&token.trim()) {
        return format!(
            "**{}**\n\n管道步骤（R§3.6）；参数形态见 REFERENCE §7 步骤表",
            token.trim()
        );
    }
    if token.starts_with("json:") || token.starts_with("$.") {
        return format!(
            "**{token}**\n\nJSONPath（RFC 9535 子集）：手写求值器覆盖 `$.a.b[*]`/`$..x`/filter；完整 filter 交 jsonpath-rust"
        );
    }
    format!("`{token}`")
}

fn completion_result(text: &str, pos: &Value) -> Value {
    let line = pos.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let ch = pos.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let mut before: Vec<char> = text
        .lines()
        .take(line)
        .flat_map(|l| l.chars().chain(std::iter::once('\n')))
        .collect();
    if let Some(l) = text.lines().nth(line) {
        before.extend(l.chars().take(ch));
    }
    let b: String = before.into_iter().collect();
    let in_transform = b
        .rfind("transform")
        .map(|i| b[i..].contains('{'))
        .unwrap_or(false);
    let items: Vec<Value> = if in_transform {
        STEP_KEYS
            .iter()
            .map(|k| {
                json!({
                    "label": format!("\"{k}\""),
                    "kind": 5,
                    "detail": "管道步骤键（R§3.6）",
                })
            })
            .collect()
    } else {
        NS_DOCS
            .iter()
            .map(|(ns, doc)| {
                json!({
                    "label": format!("{ns}:"),
                    "kind": 14,
                    "detail": doc,
                })
            })
            .collect()
    };
    Value::Array(items)
}

fn definition_result(text: &str, pos: &Value) -> Value {
    let Some((token, _)) = token_at(text, pos) else {
        return Value::Null;
    };
    if let Some(path) = token.strip_prefix("var:") {
        if let Some(leaf) = path.split('.').next_back() {
            if let Some(range) = pos_range(text, leaf) {
                return json!({
                    "uri": "file:///current.rsl.json",
                    "range": range
                });
            }
        }
    }
    Value::Null
}

/// 编译期自检（保持 sugar import 有意义）。
#[allow(dead_code)]
fn ns_check() {
    let _ = sugar::check_ns("css:");
}
