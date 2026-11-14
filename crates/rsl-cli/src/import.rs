//! B3 import（TASK-BOOK-2 §3.2 B3）：`rsl import <url-or-file>`。
//! 识别本地文件 / yuancang API / 其它 URL（AOAOSTAR 等返回 Legado JSON 数组的聚合）。
//! 拉取（并发 + 重试）→ convert → lint 聚合报告（JSON 同构 batch_report.json）→ 可选 --pack。
//! 依赖 main.rs 的 convert 单源实现（super::rsl_core_convert_one / super::load）。

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn cmd_import(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!(
            "用法：rsl import <url-or-file> [--out dir] [--limit N] [--workers 12] [--report <path>] [--pack]"
        );
        return 2;
    }
    let input = rest[0].clone();
    let mut out_dir = PathBuf::from("imported");
    let mut limit: usize = 0;
    let mut workers = 12usize;
    let mut report_path = String::new();
    let mut pack = false;
    let mut i = 1;
    while i < rest.len() {
        match rest[i].as_str() {
            "--out" => {
                out_dir =
                    PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or("imported"));
                i += 2;
            }
            "--limit" => {
                limit = rest.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "--workers" => {
                workers = rest.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(12);
                i += 2;
            }
            "--report" => {
                report_path = rest.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--pack" => {
                pack = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    let t0 = std::time::Instant::now();
    // ── 1. 采集 Legado 源列表 ──
    let collect = || -> Result<Vec<Value>, String> {
        let local = Path::new(&input);
        if local.exists() {
            let text = super::load(&input)?;
            let v: Value = serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败：{e}"))?;
            return Ok(expand_legado_sources(v));
        }
        if input.contains("yuancang.cc") {
            return import_yuancang(&input, limit, workers);
        }
        // 通用：URL 返回 Legado JSON（数组/映射/单源）
        let body = http_get_retries(&input, 2)?;
        let v: Value =
            serde_json::from_str(&body).map_err(|e| format!("URL 内容不是 Legado JSON：{e}"))?;
        let mut all = expand_legado_sources(v);
        if limit > 0 && all.len() > limit {
            all.truncate(limit);
        }
        Ok(all)
    };
    let sources = match collect() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("采集失败：{e}");
            return 2;
        }
    };
    println!("采集 Legado 源 {} 个", sources.len());
    if sources.is_empty() {
        eprintln!("未采集到任何含 bookSourceUrl 的源");
        return 2;
    }

    // ── 2. 并发 convert + lint ──
    let _ = std::fs::create_dir_all(&out_dir);
    let pool = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .min(workers);
    let chunks: Vec<Vec<Value>> = sources
        .chunks(sources.len().div_ceil(pool).max(1))
        .map(|c| c.to_vec())
        .collect();
    let agg: Vec<Value> = std::thread::scope(|scope| {
        let mut handles = vec![];
        for chunk in &chunks {
            let out_dir_c = out_dir.clone();
            handles.push(scope.spawn(move || -> Vec<Value> {
                chunk
                    .iter()
                    .filter_map(|leg| import_convert_one(leg, &out_dir_c))
                    .collect()
            }));
        }
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });

    // ── 3. 聚合报告（batch_report.json 同构） ──
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut dropped_fields: BTreeMap<String, usize> = BTreeMap::new();
    let mut convert_ok = 0usize;
    let mut lint_pass = 0usize;
    let mut lint_fail = 0usize;
    let legado_total = sources.len();
    for a in &agg {
        if a["convert_ok"].as_bool().unwrap_or(false) {
            convert_ok += 1;
        }
        lint_pass += a["lint_pass"].as_u64().unwrap_or(0) as usize;
        lint_fail += a["lint_fail"].as_u64().unwrap_or(0) as usize;
        if let Some(m) = a["codes"].as_object() {
            for (k, v) in m {
                *codes.entry(k.clone()).or_insert(0) += v.as_u64().unwrap_or(0) as usize;
            }
        }
        if let Some(m) = a["dropped_fields"].as_object() {
            for (k, v) in m {
                *dropped_fields.entry(k.clone()).or_insert(0) +=
                    v.as_u64().unwrap_or(0) as usize;
            }
        }
    }
    let mut codes_v: Vec<(String, usize)> = codes.into_iter().collect();
    codes_v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut drops_v: Vec<(String, usize)> = dropped_fields.into_iter().collect();
    drops_v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let report = json!({
        "输入": input,
        "文件数": 1,
        "Legado源总数": legado_total,
        "convert成功": convert_ok,
        "草稿lint通过": lint_pass,
        "草稿lint失败": lint_fail,
        "lint错误码Top12": codes_v
            .iter()
            .take(12)
            .map(|(k, v)| json!([k, v]))
            .collect::<Vec<_>>(),
        "不可转字段Top15": drops_v
            .iter()
            .take(15)
            .map(|(k, v)| json!([k, v]))
            .collect::<Vec<_>>(),
        "耗时秒": t0.elapsed().as_secs_f64().round(),
    });
    if !report_path.is_empty() {
        let _ = std::fs::write(
            &report_path,
            serde_json::to_vec_pretty(&report).unwrap_or_default(),
        );
    }
    println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());

    // ── 4. 可选 --pack：合法草稿 → pack 目录 ──
    if pack {
        let pack_dir = out_dir.join("pack");
        let _ = std::fs::create_dir_all(&pack_dir);
        let mut packed = 0usize;
        for entry in std::fs::read_dir(&out_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.to_str().map(|s| s.contains(".rsl.")).unwrap_or(false)
            {
                if let Ok(text) = super::load(p.to_str().unwrap_or("")) {
                    let out = rsl_core::analyze::compile_str(&text);
                    if out.ok() {
                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("s");
                        let _ = std::fs::write(
                            pack_dir.join(format!("{stem}.rsl.json")),
                            serde_json::to_vec_pretty(&out.source).unwrap_or_default(),
                        );
                        packed += 1;
                    }
                }
            }
        }
        println!("pack：{packed} 个合法草稿 → {}", pack_dir.display());
    }
    0
}

/// 展开各种 Legado 导出形态（数组 / {名:源} 映射 / 单源）。
pub fn expand_legado_sources(v: Value) -> Vec<Value> {
    let mut out = vec![];
    let mut queue = vec![v];
    while let Some(el) = queue.pop() {
        if let Some(arr) = el.as_array() {
            for x in arr {
                queue.push(x.clone());
            }
            continue;
        }
        if let Some(m) = el.as_object() {
            if !m.contains_key("bookSourceUrl")
                && !m.is_empty()
                && m.values().all(|val| val.is_object())
            {
                for (_, val) in m {
                    queue.push(val.clone());
                }
                continue;
            }
            if m.contains_key("bookSourceUrl") {
                out.push(el);
            }
        }
    }
    out
}

fn http_get_retries(url: &str, retries: usize) -> Result<String, String> {
    let mut last = String::new();
    for _ in 0..=retries {
        match ureq::get(url)
            .set("User-Agent", "rsl-import/1.0")
            .timeout(std::time::Duration::from_secs(30))
            .call()
        {
            Ok(r) => return r.into_string().map_err(|e| e.to_string()),
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    Err(last)
}

/// yuancang API：分页列出（category_name=书源）→ 并发取 content。
fn import_yuancang(url: &str, limit: usize, workers: usize) -> Result<Vec<Value>, String> {
    let _ = url; // API 同源；url 仅用于判定
    let base = "https://yuancang.cc/api/v1";
    let mut ids: Vec<(i64, String)> = vec![];
    let mut page = 1usize;
    loop {
        let api = format!("{base}/sources?page={page}&page_size=100");
        let body = http_get_retries(&api, 2)?;
        let d: Value = serde_json::from_str(&body).map_err(|e| format!("API JSON 解析失败：{e}"))?;
        let items = d
            .pointer("/data/items")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            break;
        }
        for it in &items {
            if it.get("category_name").and_then(|c| c.as_str()) == Some("书源") {
                ids.push((
                    it["id"].as_i64().unwrap_or(0),
                    it.get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string(),
                ));
            }
        }
        let total = d.pointer("/data/total").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
        if page * 100 >= total {
            break;
        }
        page += 1;
    }
    if limit > 0 && ids.len() > limit {
        ids.truncate(limit);
    }
    println!("yuancang：命中 书源 {} 个，并发 {workers} 拉取", ids.len());
    let pool = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .min(workers);
    let chunks: Vec<Vec<(i64, String)>> = ids
        .chunks(ids.len().div_ceil(pool).max(1))
        .map(|c| c.to_vec())
        .collect();
    let all: Vec<Value> = std::thread::scope(|scope| {
        let mut handles = vec![];
        for chunk in &chunks {
            handles.push(scope.spawn(move || -> Vec<Value> {
                let mut got = vec![];
                for (sid, _) in chunk {
                    let api = format!("{base}/sources/{sid}/content");
                    if let Ok(body) = http_get_retries(&api, 2) {
                        if let Ok(v) = serde_json::from_str::<Value>(&body) {
                            let inner = if v.is_string() {
                                serde_json::from_str::<Value>(v.as_str().unwrap_or(""))
                                    .unwrap_or(Value::Null)
                            } else {
                                v
                            };
                            for x in expand_legado_sources(inner) {
                                got.push(x);
                            }
                        }
                    }
                }
                got
            }));
        }
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    Ok(all)
}

/// 单源：convert → 写草稿 + lint 统计。
fn import_convert_one(leg: &Value, out_dir: &Path) -> Option<Value> {
    let (rsl_text, report) = match super::rsl_core_convert_one(leg) {
        Ok(x) => x,
        Err(_) => {
            return Some(json!({
                "convert_ok": false, "lint_pass": 0, "lint_fail": 0,
                "codes": {}, "dropped_fields": {}
            }))
        }
    };
    let id = report["id"]
        .as_str()
        .unwrap_or("source")
        .replace(['.', '/'], "_");
    let rsl_path = out_dir.join(format!("{id}.rsl.json"));
    let _ = std::fs::write(&rsl_path, &rsl_text);
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut lint_pass = 0usize;
    let mut lint_fail = 0usize;
    let lint = rsl_core::testkit::lint::lint(&rsl_text);
    let mut has_err = false;
    for d in lint.findings.iter() {
        if d.diag.code.is_warning() {
            continue;
        }
        has_err = true;
        *codes.entry(d.diag.code.as_str().to_string()).or_insert(0) += 1;
    }
    if has_err {
        lint_fail = 1;
    } else {
        lint_pass = 1;
    }
    let mut dropped_fields: BTreeMap<String, usize> = BTreeMap::new();
    for d in report["dropped"].as_array().into_iter().flatten() {
        if let Some(f) = d.get("field").and_then(|x| x.as_str()) {
            let key = f.split('(').next().unwrap_or(f).to_string();
            *dropped_fields.entry(key).or_insert(0) += 1;
        }
    }
    Some(json!({
        "convert_ok": true,
        "lint_pass": lint_pass,
        "lint_fail": lint_fail,
        "codes": codes,
        "dropped_fields": dropped_fields,
    }))
}
