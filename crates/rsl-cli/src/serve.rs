//! rsl serve（TASK-BOOK-4 T2）：本机回环 HTTP 调试服务。
//! std 手写最小 HTTP（V4-D2，沿手写 LSP 薄壳先例，零新依赖；短连接 JSON-only）。
//! 无状态（V4-D3）：源文本随请求携带，不读盘不落盘（S§17）；会话在前端。
//! 安全（V4-D11）：默认 127.0.0.1 绑定（非回环拒绝）+ 默认随机端口 + 可选 token。

use rsl_core::analyze;
use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{HttpResponse, MemoryCookieStore, SystemClock};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_HEADER: usize = 16 * 1024;
const MAX_BODY: usize = 10 * 1024 * 1024;

struct ServeState {
    token: Option<String>,
    web: Option<PathBuf>,
    /// /debug/run 串行化（H3：默认单线程执行语义；脚本引擎 thread_local，天然隔离）
    run_lock: Mutex<()>,
}

pub fn cmd_serve(rest: &[String]) -> i32 {
    let mut port: u16 = 0;
    let mut token: Option<String> = None;
    let mut web: Option<PathBuf> = None;
    let mut bind: Option<String> = None;
    let mut watch: Option<PathBuf> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--port" => match rest.get(i + 1).and_then(|s| s.parse().ok()) {
                Some(p) => {
                    port = p;
                    i += 2;
                }
                None => {
                    eprintln!("--port 需要数字参数");
                    return 2;
                }
            },
            "--token" => {
                token = rest.get(i + 1).cloned();
                i += 2;
            }
            "--web" => {
                web = rest.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            "--bind" => {
                bind = rest.get(i + 1).cloned();
                i += 2;
            }
            // T5 独立模式：notify 文件监听（变化仅记生命周期日志——serve 无状态（V4-D3），
            // 编辑缓冲仍由编辑端推送；推送通道（WS/SSE）归 P2 演进
            "--watch" => {
                watch = rest.get(i + 1).map(PathBuf::from);
                i += 2;
            }
            other => {
                eprintln!("未知参数 {other}；用法：rsl serve [--port N] [--token t] [--web dir] [--bind 127.0.0.1] [--watch src]");
                return 2;
            }
        }
    }
    // V4-D11：只允许回环绑定（显式 --bind 非回环 → 拒绝启动）
    let host = match bind.as_deref() {
        None => "127.0.0.1",
        Some("127.0.0.1") | Some("localhost") | Some("[::1]") | Some("::1") => "127.0.0.1",
        Some(other) => {
            eprintln!("拒绝绑定 {other}：调试服务只允许本机回环（127.0.0.1）——S§17/安全边界");
            return 2;
        }
    };
    let listener = match TcpListener::bind((host, port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("绑定 {host}:{port} 失败：{e}");
            return 1;
        }
    };
    let addr = listener.local_addr().expect("local_addr");
    let state = Arc::new(ServeState {
        token,
        web,
        run_lock: Mutex::new(()),
    });
    println!(
        "RSL_SERVE listening={addr} token={} web={} watch={} pid={}",
        if state.token.is_some() { "SET" } else { "NO" },
        state.web.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "-".into()),
        watch.as_deref().map(|p| p.display().to_string()).unwrap_or_else(|| "-".into()),
        std::process::id(),
    );
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    // T5 独立模式文件监听（notify 7，沿 test --watch 先例）：变化 → 生命周期日志
    if let Some(wpath) = watch {
        std::thread::spawn(move || {
            use notify::Watcher;
            let (tx, rx) = std::sync::mpsc::channel();
            let Ok(mut watcher) = notify::recommended_watcher(tx) else { return };
            let dir = wpath.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
            if watcher.watch(&dir, notify::RecursiveMode::NonRecursive).is_err() {
                return;
            }
            for _ in rx {
                println!("[serve] 源文件已变更：{}（编辑端拉最新文本重跑）", wpath.display());
                let _ = std::io::stdout().flush();
            }
        });
    }
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = handle_conn(&mut stream, &state);
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });
    }
    0
}

// ───────────────────────── HTTP 最小实现 ─────────────────────────

struct Req {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

/// 读一个短连接请求（Request-Line + headers + Content-Length body）。
/// 返回 Err(状态码) 表示读层失败（400 坏请求 / 413 超限 / 408 无数据）。
fn read_request(stream: &mut TcpStream) -> Result<Req, u16> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if buf.len() > MAX_HEADER {
            return Err(413);
        }
        let n = stream.read(&mut chunk).map_err(|_| 400u16)?;
        if n == 0 {
            if buf.is_empty() {
                return Err(408); // 空连接（探活）静默关闭
            }
            return Err(400);
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    if method.is_empty() || target.is_empty() {
        return Err(400);
    }
    let mut headers: Vec<(String, String)> = vec![];
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    // body
    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY {
        return Err(413);
    }
    let mut body: Vec<u8> = buf[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut chunk).map_err(|_| 400u16)?;
        if n == 0 {
            return Err(400);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);
    // query
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (
            p.to_string(),
            q.split('&')
                .filter_map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                    Some((url_decode(k), url_decode(v)))
                })
                .collect(),
        ),
        None => (target, vec![]),
    };
    Ok(Req {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn respond_json(stream: &mut TcpStream, status: u16, v: &Value) {
    respond(stream, status, "application/json; charset=utf-8", v.to_string().as_bytes());
}

// ───────────────────────── 请求分派 ─────────────────────────

fn handle_conn(stream: &mut TcpStream, state: &ServeState) -> Result<(), std::io::Error> {
    let t0 = std::time::Instant::now();
    let req = match read_request(stream) {
        Ok(r) => r,
        Err(408) => return Ok(()), // 空连接
        Err(code) => {
            respond(stream, code, "application/json; charset=utf-8", br#"{"error":"bad request"}"#);
            return Ok(());
        }
    };
    // token 校验（V4-D11）：**数据面**（health/debug-run/debug-lint）强制；静态壳与公开 schema 豁免——
    // 壳内无用户数据，而 iframe 内子资源（css/js）无法携带查询串 token。
    let needs_token = matches!(
        (req.method.as_str(), req.path.as_str()),
        ("GET", "/health") | ("POST", "/debug/run") | ("POST", "/debug/lint")
    );
    if needs_token {
        if let Some(token) = &state.token {
            let presented = req
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
                .and_then(|(_, v)| v.strip_prefix("Bearer "))
                .or_else(|| {
                    req.headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("x-rsl-token"))
                        .map(|(_, v)| v.as_str())
                })
                .or_else(|| {
                    req.query
                        .iter()
                        .find(|(k, _)| k == "token")
                        .map(|(_, v)| v.as_str())
                });
            if presented != Some(token.as_str()) {
                log_req(&req, 401, t0);
                respond_json(stream, 401, &json!({"error": "token 不匹配"}));
                return Ok(());
            }
        }
    }
    let status = match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/health") => {
            respond_json(stream, 200, &json!({"ok": true, "service": "rsl-serve", "protocol": "rsl-debug/1"}));
            200
        }
        ("GET", "/debug/schema") => {
            let schema = include_str!("../../../schema/rsl-debug.schema.json");
            respond(stream, 200, "application/schema+json; charset=utf-8", schema.as_bytes());
            200
        }
        // 语言 Schema（公开契约，编辑器表单派生配置用；豁免 token）
        ("GET", "/lang/schema") => {
            let schema = include_str!("../../../schema/rsl.schema.json");
            respond(stream, 200, "application/schema+json; charset=utf-8", schema.as_bytes());
            200
        }
        ("POST", "/debug/run") => handle_debug_run(stream, state, &req),
        ("POST", "/debug/lint") => handle_debug_lint(stream, state, &req),
        ("GET", _) | ("HEAD", _) => {
            let rel: String = if req.path == "/" { "index.html".to_string() } else { req.path.trim_start_matches('/').to_string() };
            match serve_static(stream, state, &rel) {
                Some(status) => status,
                None => {
                    respond_json(
                        stream,
                        404,
                        &json!({"error": "not found", "hint": "GET /health | GET /debug/schema | POST /debug/run"}),
                    );
                    404
                }
            }
        }
        _ => {
            respond_json(stream, 405, &json!({"error": "method not allowed"}));
            405
        }
    };
    log_req(&req, status, t0);
    Ok(())
}

fn log_req(req: &Req, status: u16, t0: std::time::Instant) {
    println!("[serve] {} {} {} {}ms", req.method, req.path, status, t0.elapsed().as_millis());
}

/// 静态前端（--web 目录；拒绝路径穿越）。返回状态码。
fn serve_static(stream: &mut TcpStream, state: &ServeState, rel: &str) -> Option<u16> {
    let root = state.web.as_deref()?;
    let rel_clean = rel.replace('\\', "/");
    if rel_clean.starts_with('/') || rel_clean.split('/').any(|seg| seg == "..") {
        respond_json(stream, 403, &json!({"error": "路径不允许"}));
        return Some(403);
    }
    let path = root.join(&rel_clean);
    let (root_canon, path_canon) = match (root.canonicalize(), path.canonicalize()) {
        (Ok(r), Ok(p)) => (r, p),
        _ => return None, // 不存在 → 调用方回 404
    };
    if !path_canon.starts_with(&root_canon) {
        respond_json(stream, 403, &json!({"error": "路径不允许"}));
        return Some(403);
    }
    let Ok(bytes) = std::fs::read(&path_canon) else { return None };
    let ctype = match path_canon.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    };
    respond(stream, 200, ctype, &bytes);
    Some(200)
}

// ───────────────────────── POST /debug/lint ─────────────────────────

/// 只编译不运行：源文本 → 全部诊断（error+warning）。
/// web 表单编辑器的实时诊断数据源（TASK-BOOK-5：编辑↔调试同内核，杜绝前端平行校验）。
fn handle_debug_lint(stream: &mut TcpStream, _state: &ServeState, req: &Req) -> u16 {
    let body: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            respond_json(stream, 400, &json!({"error": format!("请求体不是合法 JSON：{e}")}));
            return 400;
        }
    };
    let Some(source_text) = body["source_text"].as_str() else {
        respond_json(stream, 400, &json!({"error": "缺少 source_text"}));
        return 400;
    };
    let t0 = std::time::Instant::now();
    let _guard = _state.run_lock.lock().unwrap_or_else(|e| e.into_inner());
    let out = analyze::compile_str(source_text);
    let diags: Vec<Value> = out
        .diagnostics
        .iter()
        .map(|d| serde_json::to_value(d).unwrap_or(Value::Null))
        .collect();
    respond_json(
        stream,
        200,
        &json!({
            "ok": out.source.is_some(),
            "diagnostics": diags,
            "elapsed_ms": t0.elapsed().as_millis() as u64,
        }),
    );
    200
}

// ───────────────────────── POST /debug/run ─────────────────────────

/// 无状态执行：源文本随请求携带（V4-D3），编译→单阶段→rsl-debug/1 产物。
/// 失败（编译错/运行错）也产出完整产物——诊断直通（T7），out=null。
fn handle_debug_run(stream: &mut TcpStream, state: &ServeState, req: &Req) -> u16 {
    let body: Value = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => {
            respond_json(stream, 400, &json!({"error": format!("请求体不是合法 JSON：{e}")}));
            return 400;
        }
    };
    let Some(source_text) = body["source_text"].as_str() else {
        respond_json(stream, 400, &json!({"error": "缺少 source_text（源文本随请求携带）"}));
        return 400;
    };
    let Some(stage) = body["stage"].as_str() else {
        respond_json(stream, 400, &json!({"error": "缺少 stage"}));
        return 400;
    };
    let label = body["label"].as_str().unwrap_or("serve").to_string();
    let (inputs, inputs_echo) = parse_inputs(body.get("inputs"));

    let t0 = std::time::Instant::now();
    let _guard = state.run_lock.lock().unwrap_or_else(|e| e.into_inner());
    let out = analyze::compile_str(source_text);
    let Some(src) = out.source else {
        // 编译失败：诊断直通（四 tab 顶栏展示），HTTP 200 + ok=false（产物层面表达失败）
        let diags: Vec<Value> = out
            .errors()
            .iter()
            .map(|d| serde_json::to_value(d).unwrap_or(Value::Null))
            .collect();
        let artifact = json!({
            "schema": "rsl-debug/1",
            "source": label,
            "sourceId": "",
            "stage": stage,
            "ok": false,
            "elapsed_ms": t0.elapsed().as_millis() as u64,
            "inputs": inputs_echo,
            "request": Value::Null,
            "response": Value::Null,
            "requests": [],
            "out": Value::Null,
            "partial": false,
            "trace": [],
            "diagnostics": diags,
            "error": diags.first().cloned().unwrap_or(Value::Null),
        });
        respond_json(stream, 200, &artifact);
        return 200;
    };
    rsl_core::script::setup_engine(&src);

    // 传输：fixture 随请求携带（离线重放/冒烟）否则真实网络（Ureq）
    struct TextFixtureTransport(HttpResponse);
    impl rsl_core::ports::Transport for TextFixtureTransport {
        fn send(&self, _req: &rsl_core::ports::HttpRequest) -> Result<HttpResponse, rsl_core::error::Diagnostic> {
            Ok(self.0.clone())
        }
    }
    let fixture_transport = body.get("fixture").map(|f| {
        let headers = f["headers"]
            .as_object()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default();
        let body_bytes = if let Some(b64) = f["body_base64"].as_str() {
            rsl_core::crypto::base64_decode(b64).unwrap_or_default()
        } else {
            f["body"].as_str().unwrap_or_default().as_bytes().to_vec()
        };
        TextFixtureTransport(HttpResponse {
            status: f["status"].as_u64().unwrap_or(200) as u16,
            headers,
            body: body_bytes,
            final_url: f["url"].as_str().unwrap_or_default().to_string(),
            ms: 0,
        })
    });
    let ureq_transport = rsl_core::http::UreqTransport::new(&src.http);
    let cookies = MemoryCookieStore::default();
    let clock = SystemClock;
    let mut cx = match &fixture_transport {
        Some(ft) => ExecCx::new(&src, ft, &clock),
        None => ExecCx::new(&src, &ureq_transport, &clock),
    };
    cx.cookie_store = Some(&cookies);
    let mut reuse = ResponseReuse::default();
    let run_res = run_stage(&src, stage, &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let elapsed_ms = t0.elapsed().as_millis() as u64;
    let artifact = rsl_core::stage::debug::build_artifact(&src, &label, stage, &cx, &inputs, &run_res, elapsed_ms);
    respond_json(stream, 200, &artifact);
    200
}

/// inputs 解析（R§5：kw/page/book/chapter/group/filter）→ (RunInputs, 回显 Value)。
fn parse_inputs(v: Option<&Value>) -> (RunInputs, Value) {
    let Some(v) = v else { return (RunInputs::default(), json!({})) };
    let map_of = |x: &Value| -> std::collections::BTreeMap<String, Value> {
        x.as_object()
            .map(|m| m.iter().map(|(k, val)| (k.clone(), val.clone())).collect())
            .unwrap_or_default()
    };
    let inputs = RunInputs {
        kw: v["kw"].as_str().map(String::from),
        page: v["page"].as_i64(),
        book: map_of(&v["book"]),
        chapter: map_of(&v["chapter"]),
        group: if v["group"].is_null() { None } else { Some(v["group"].clone()) },
        filter: map_of(&v["filter"]),
        login_values: Default::default(),
        extra: Default::default(),
    };
    let echo = json!({
        "kw": inputs.kw,
        "page": inputs.page,
        "book": Value::Object(inputs.book.clone().into_iter().collect()),
        "chapter": Value::Object(inputs.chapter.clone().into_iter().collect()),
        "group": inputs.group.clone(),
        "filter": Value::Object(inputs.filter.clone().into_iter().collect()),
    });
    (inputs, echo)
}
