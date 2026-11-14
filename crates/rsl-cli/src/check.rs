//! TASK-BOOK-3 F4：`rsl check <dir> [--probe] [--workers n] [--out p]`
//! 批量静态检查 + 可选连通性探活（对标社区 LS-Toolkit 的批量测活，聚合报告 batch_report 同构）。
//!
//! V3-D9 合规边界（S§17）：probe **只测连通与可解析性**——读响应 ≤4KB 后立即丢弃，
//! 不落盘、不解析出内容、报告里不含任何正文样本。探活目标 = search 阶段请求 URL。

use rsl_core::analyze;
use rsl_core::error::Code;
use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{SystemClock, Transport};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 空传输（probe 渲染 URL 模板时不发请求；真正探活在 CLI 侧 ureq）。
struct NoTransport;

impl Transport for NoTransport {
    fn send(&self, _req: &rsl_core::ports::HttpRequest) -> Result<rsl_core::ports::HttpResponse, rsl_core::error::Diagnostic> {
        Err(rsl_core::error::Diagnostic::new(
            Code::ENet,
            "probe",
            "check --probe 的 URL 渲染阶段不应发请求",
            "—",
        ))
    }
}

pub fn cmd_check(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl check <dir> [--probe] [--workers n] [--out report.json]");
        return 2;
    }
    let dir_owned = rest[0].clone();
    let dir = Path::new(&dir_owned);
    let mut probe = false;
    let mut workers = 4usize;
    let mut out: Option<PathBuf> = None;
    let mut i = 1;
    while i < rest.len() {
        match rest[i].as_str() {
            "--probe" => probe = true,
            "--workers" => {
                workers = rest.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(4);
                i += 2;
                continue;
            }
            "--out" => {
                out = rest.get(i + 1).map(PathBuf::from);
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    let mut files: Vec<PathBuf> = vec![];
    collect_sources(dir, &mut files);
    files.sort();
    if files.is_empty() {
        eprintln!("{} 下无 .rsl.json 源", dir.display());
        return 2;
    }
    let files = Arc::new(Mutex::new(files));
    let items: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
    let items_arc = Arc::clone(&items);
    let n_workers = workers.min(files.lock().unwrap_or_else(|e| e.into_inner()).len()).max(1);
    let mut handles = vec![];
    for _ in 0..n_workers {
        let files = Arc::clone(&files);
        let items = Arc::clone(&items);
        let probe = probe;
        let base = dir.to_path_buf();
        handles.push(std::thread::spawn(move || loop {
            let next = files.lock().unwrap_or_else(|e| e.into_inner()).pop();
            let Some(f) = next else { break };
            let item = check_one(&f, &base, probe);
            items.lock().unwrap_or_else(|e| e.into_inner()).push(item);
        }));
    }
    for h in handles {
        h.join().ok();
    }
    // 汇总（确定性顺序：按文件路径排序）
    let mut items = items.lock().unwrap_or_else(|e| e.into_inner()).clone();
    drop(items_arc);
    items.sort_by(|a, b| {
        a["file"].as_str().unwrap_or("").cmp(b["file"].as_str().unwrap_or(""))
    });
    let total = items.len();
    let lint_pass = items.iter().filter(|x| x["lint"]["ok"] == json!(true)).count();
    let probe_ok = items
        .iter()
        .filter(|x| x["probe"]["ok"] == json!(true))
        .count();
    let probe_fail = probe
        && items.iter().any(|x| x.get("probe").is_some() && x["probe"]["ok"] != json!(true));
    let report = json!({
        "schema": "rsl-check-report/1",
        "probe": probe,
        "summary": {
            "total": total,
            "lint_pass": lint_pass,
            "lint_fail": total - lint_pass,
            "probe_ok": probe_ok,
            "probe_fail": if probe {
                items.iter().filter(|x| x.get("probe").is_some() && x["probe"]["ok"] != json!(true)).count()
            } else { 0 }
        },
        "items": items,
    });
    let _ = probe_fail;
    let text = serde_json::to_string_pretty(&report).unwrap_or_default();
    match &out {
        Some(p) => {
            std::fs::write(p, text + "\n").ok();
            println!("报告 → {}", p.display());
        }
        None => println!("{text}"),
    }
    let s = &report["summary"];
    println!(
        "✓ check 完成：{} 源，lint 通过 {}，失败 {}{}",
        s["total"],
        s["lint_pass"],
        s["lint_fail"],
        if probe {
            format!("；probe 连通 {}，不通 {}", s["probe_ok"], s["probe_fail"])
        } else {
            String::new()
        }
    );
    if s["lint_fail"].as_u64().unwrap_or(0) == 0 {
        0
    } else {
        1
    }
}

/// 单源检查：lint（+ 可选 probe）。probe 不落内容（V3-D9）。
fn check_one(f: &Path, base: &Path, probe: bool) -> Value {
    let rel = f.strip_prefix(base).unwrap_or(f).to_string_lossy().to_string();
    let text = std::fs::read_to_string(f).unwrap_or_default();
    let out = analyze::compile_str(&text);
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let lint_ok = out.source.is_some();
    for d in out.errors() {
        if d.code.is_warning() {
            continue;
        }
        *codes.entry(d.code.as_str().to_string()).or_insert(0) += 1;
    }
    let mut item = json!({
        "file": rel,
        "id": out.source.as_ref().map(|s| s.id.clone()).unwrap_or_default(),
        "lint": { "ok": lint_ok, "codes": codes },
    });
    if !probe || !lint_ok {
        return item;
    }
    item["probe"] = probe_source(out.source.as_ref().unwrap());
    item
}

/// 连通性探活：search 阶段 URL（模板以占位变量渲染）→ GET ≤4KB → 丢弃。
fn probe_source(src: &rsl_core::ir::CompiledSource) -> Value {
    let Some(stage) = src.stages.get("search") else {
        return json!({ "ok": false, "code": "E_UNSUPPORTED", "message": "无 search 阶段，无法探活" });
    };
    let Some(req) = &stage.request else {
        return json!({ "ok": false, "code": "E_UNSUPPORTED", "message": "search 无 request，无法探活" });
    };
    // 渲染 URL 模板（var 用源默认值；kw 等输入变量给占位）
    let clock = SystemClock;
    let mut cx = ExecCx::new(src, &NoTransport, &clock);
    let segs = req
        .url
        .clone()
        .unwrap_or_else(|| vec![rsl_core::sugar::Seg::Lit(src.base_url.clone())]);
    let url = match rsl_core::stage::render_request_template(&mut cx, &segs, "probe.url") {
        Ok(v) => v.as_str().map(String::from).unwrap_or_default(),
        Err(d) => {
            return json!({ "ok": false, "code": d.code.as_str(), "message": d.message });
        }
    };
    if url.is_empty() {
        return json!({ "ok": false, "code": "E_TEMPLATE", "message": "URL 渲染为空（含必填变量）" });
    }
    let t0 = std::time::Instant::now();
    let probe = ureq::get(&url)
        .set("User-Agent", "rsl-check-probe/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .call();
    let ms = t0.elapsed().as_millis() as u64;
    match probe {
        Ok(resp) => {
            let status = resp.status();
            // 读 ≤4KB 后立即丢弃——不落盘、不解析、不进报告（V3-D9）
            use std::io::Read as _;
            let mut head = [0u8; 4096];
            let n = resp
                .into_reader()
                .take(4096)
                .read(&mut head)
                .unwrap_or(0);
            let readable = n > 0 && std::str::from_utf8(&head[..n]).is_ok();
            json!({
                "ok": status < 400,
                "status": status,
                "ms": ms,
                "readable": readable,
                "bytes_seen": n,
            })
        }
        Err(ureq::Error::Status(code, _)) => {
            json!({ "ok": false, "code": "E_HTTP", "status": code, "ms": ms })
        }
        Err(_) => {
            json!({ "ok": false, "code": "E_NET", "ms": ms })
        }
    }
}

/// 递归收集 .rsl.json（同 build 的收集规则）。
fn collect_sources(cur: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(cur) else { return };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|f| f.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if name.starts_with('.') || name == "dist" || name == "target" {
            continue;
        }
        if p.is_dir() {
            collect_sources(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("json")
            && name.contains(".rsl")
        {
            out.push(p);
        }
    }
}
