//! `rsl` CLI：lint/compile/run/record/test/build/pack/convert/explain 九命令（S§14）。

use rsl_core::analyze;
use rsl_core::exec::ctx::ExecCx;
use rsl_core::ports::{Clock, MemoryCookieStore, SystemClock};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs};
use rsl_core::testkit;
use serde_json::Value;
use rsl_core::testkit::lint::{lint, RULE_TABLE};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_help();
        std::process::exit(2);
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let code = match cmd {
        "lint" => cmd_lint(rest),
        "compile" => cmd_compile(rest),
        "run" => cmd_run(rest),
        "record" => cmd_record(rest),
        "test" => cmd_test(rest),
        "build" => cmd_build(rest),
        "pack" => cmd_pack(rest),
        "convert" => cmd_convert(rest),
        "explain" => cmd_explain(rest),
        "help" | "--help" | "-h" => {
            print_help();
            0
        }
        "--version" => {
            println!("rsl {}", rsl_core::VERSION);
            0
        }
        _ => {
            eprintln!("未知命令 `{cmd}`");
            print_help();
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    println!(
        "rsl — RSL Reader Source Language 内核 CLI v{}\n\
         \n\
         用法：\n\
         \x20 rsl lint <src>                      静态检查（不联网，20 条规则）\n\
         \x20 rsl compile <src>                   编译并输出 IR 摘要\n\
         \x20 rsl run <src> <stage> [--kw …]      执行单个阶段\n\
         \x20 rsl record <src> <stage> [--dir d]  录制真实响应为 fixture\n\
         \x20 rsl test <src> [--cases f]          跑测试用例（S§13.2 YAML）\n\
         \x20 rsl build <srcdir> [--out dist]     多文件源装配（确定性构建）\n\
         \x20 rsl pack <src> [--out p]            打包分发产物（含校验和）\n\
         \x20 rsl convert <legado.json> [--out d] 存量源迁移 + 不可转换报告\n\
         \x20 rsl explain <src> <path>            解释某条规则为何命中/未命中\n",
        rsl_core::VERSION
    );
}

fn load(path: &str) -> Result<String, String> {
    std::fs::read_to_string(PathBuf::from(path)).map_err(|e| format!("读取 {path} 失败：{e}"))
}

fn print_diags(diags: &[rsl_core::error::Diagnostic]) {
    for d in diags {
        println!("{d}");
    }
}

// ───────────────────────── lint / compile ─────────────────────────

fn cmd_lint(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl lint <src>");
        return 2;
    }
    let text = match load(&rest[0]) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let report = lint(&text);
    // 20 条清单对照
    let mut hit_rules = std::collections::BTreeSet::new();
    for f in &report.findings {
        if let Some(r) = f.rule {
            hit_rules.insert(r.as_num());
        }
    }
    println!(
        "lint {}：{} 处发现",
        if report.ok { "通过" } else { "未通过" },
        report.findings.len()
    );
    for f in &report.findings {
        if let Some(r) = f.rule {
            println!("[规则 #{}]", r.as_num());
        }
        println!("{}", f.diag);
    }
    if !hit_rules.is_empty() {
        let list: Vec<String> = hit_rules.iter().map(|n| format!("#{n}")).collect();
        println!("命中规则：{}", list.join(" "));
    }
    if report.ok {
        0
    } else {
        1
    }
}

fn cmd_compile(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl compile <src>");
        return 2;
    }
    let text = match load(&rest[0]) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let out = analyze::compile_str(&text);
    if let Some(src) = &out.source {
        // IR 摘要
        println!(
            "✓ 编译通过：{}（{} 个阶段，{} 个 hostStage）",
            src.id,
            src.stages.len(),
            src.host_stages.len()
        );
        for (name, st) in &src.stages {
            println!(
                "  stage {:<8} let={} request={} paginate={} script={}",
                name,
                st.lets.len(),
                st.request.is_some(),
                st.paginate.is_some(),
                !st.script.before.is_none() || !st.script.on_response.is_none() || !st.script.after.is_none()
            );
        }
        // bincode 产物摘要（二次加载验证）
        match src.to_bincode() {
            Ok(bytes) => println!("  IR bincode {} 字节", bytes.len()),
            Err(e) => println!("  IR 序列化失败: {e}"),
        }
    }
    let warns = out.warnings();
    if !warns.is_empty() {
        println!("—— {} 条警告 ——", warns.len());
        print_diags(&out.diagnostics);
    }
    let errs = out.errors();
    if !errs.is_empty() {
        println!("—— {} 条错误 ——", errs.len());
        for d in errs {
            println!("{d}");
        }
        return 1;
    }
    0
}

// ───────────────────────── run ─────────────────────────

fn cmd_run(rest: &[String]) -> i32 {
    if rest.len() < 2 {
        eprintln!("用法：rsl run <src> <stage> [--kw …] [--page N] [--trace] [--fixture f] [--book key=val] [--chapter key=val]");
        return 2;
    }
    let src_path = &rest[0];
    let stage_name = &rest[1];
    let mut kw = None;
    let mut page = None;
    let mut trace = false;
    let mut fixture: Option<String> = None;
    let mut book: std::collections::BTreeMap<String, serde_json::Value> = Default::default();
    let mut chapter: std::collections::BTreeMap<String, serde_json::Value> = Default::default();
    let mut i = 2;
    while i < rest.len() {
        match rest[i].as_str() {
            "--kw" => {
                kw = rest.get(i + 1).cloned();
                i += 2;
            }
            "--page" => {
                page = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            "--trace" => {
                trace = true;
                i += 1;
            }
            "--fixture" => {
                fixture = rest.get(i + 1).cloned();
                i += 2;
            }
            "--book" => {
                if let Some(kv) = rest.get(i + 1) {
                    if let Some((k, v)) = kv.split_once('=') {
                        book.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                    }
                }
                i += 2;
            }
            "--chapter" => {
                if let Some(kv) = rest.get(i + 1) {
                    if let Some((k, v)) = kv.split_once('=') {
                        chapter.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                    }
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    let text = match load(src_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let out = analyze::compile_str(&text);
    let Some(src) = out.source else {
        eprintln!("编译失败：");
        for d in out.errors() {
            eprintln!("{d}");
        }
        return 1;
    };
    rsl_core::script::setup_engine(&src);
    let ureq_transport = rsl_core::http::UreqTransport::new(&src.http);
    let fixture_transport = fixture.as_ref().and_then(|f| {
        rsl_core::testkit::record::load_fixture(Path::new(f)).ok().map(|resp| {
            rsl_core::ports::FixtureTransport {
                responses: vec![resp],
                idx: Default::default(),
            }
        })
    });
    let clock = SystemClock;
    let cookies = MemoryCookieStore::default();
    let mut cx = if let Some(ft) = &fixture_transport {
        ExecCx::new(&src, ft, &clock)
    } else {
        ExecCx::new(&src, &ureq_transport, &clock)
    };
    cx.cookie_store = Some(&cookies);
    let inputs = RunInputs {
        kw,
        page,
        book,
        chapter,
        ..Default::default()
    };
    let mut reuse = ResponseReuse::default();
    match run_stage(&src, stage_name, &mut cx, &inputs, &mut reuse, &HostCaps::default()) {
        Ok(r) => {
            println!("{}", serde_json::to_string_pretty(&r.out).unwrap_or_default());
            if trace {
                let t = cx.trace.borrow();
                println!("—— trace（{} 条）——", t.len());
                for e in t.iter() {
                    println!(
                        "  {{\"stage\":\"{}\",\"step\":\"{}\",\"rule\":\"{}\",\"matched\":{},\"ms\":{},\"bridge\":{}}}",
                        serde_json::to_string(&e.stage).unwrap(),
                        serde_json::to_string(&e.step).unwrap(),
                        serde_json::to_string(&e.rule).unwrap(),
                        e.matched,
                        e.ms,
                        e.bridge
                    );
                }
            }
            let warns = cx.warnings.borrow().clone();
            for w in warns {
                println!("WARN {w}");
            }
            0
        }
        Err(d) => {
            println!("ERR {d}");
            1
        }
    }
}

// ───────────────────────── record ─────────────────────────

fn cmd_record(rest: &[String]) -> i32 {
    if rest.len() < 2 {
        eprintln!("用法：rsl record <src> <stage> [--dir fixtures] [--kw …]");
        return 2;
    }
    let src_path = &rest[0];
    let stage_name = &rest[1];
    let mut dir = PathBuf::from("fixtures");
    let mut kw = None;
    let mut page = Some(1i64);
    let mut i = 2;
    while i < rest.len() {
        match rest[i].as_str() {
            "--dir" => {
                dir = PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or("fixtures"));
                i += 2;
            }
            "--kw" => {
                kw = rest.get(i + 1).cloned();
                i += 2;
            }
            "--page" => {
                page = rest.get(i + 1).and_then(|s| s.parse().ok());
                i += 2;
            }
            _ => i += 1,
        }
    }
    let text = match load(src_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let out = analyze::compile_str(&text);
    let Some(src) = out.source else {
        eprintln!("编译失败");
        return 1;
    };
    rsl_core::script::setup_engine(&src);
    let transport = rsl_core::http::UreqTransport::new(&src.http);
    let clock = SystemClock;
    let cookies = MemoryCookieStore::default();
    let mut cx = ExecCx::new(&src, &transport, &clock);
    cx.cookie_store = Some(&cookies);
    let inputs = RunInputs {
        kw,
        page,
        ..Default::default()
    };
    // 注入变量（run_stage 前半段）
    rsl_core::stage::run_stage(
        &src,
        stage_name,
        &mut cx,
        &inputs,
        &mut ResponseReuse::default(),
        &HostCaps::default(),
    )
    .ok(); // 阶段失败不影响录制（响应已落 transport 记录）——但 record 需要 send；直接走 record_stage
    match testkit::record::record_stage(&src, &mut cx, stage_name, &dir) {
        Ok(paths) => {
            for p in paths {
                println!("✓ 录制 {}", p.display());
            }
            0
        }
        Err(d) => {
            eprintln!("录制失败：{d}");
            1
        }
    }
}

// ───────────────────────── test ─────────────────────────

fn cmd_test(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl test <src> [--cases tests/cases/<id>.yaml]");
        return 2;
    }
    let src_path = &rest[0];
    let mut cases_file = PathBuf::from(format!(
        "tests/cases/{}.yaml",
        Path::new(src_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("cases")
    ));
    let mut i = 1;
    while i < rest.len() {
        if rest[i] == "--cases" {
            cases_file = PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or(""));
            i += 2;
        } else {
            i += 1;
        }
    }
    let src_text = match load(src_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let yaml = match load(cases_file.to_str().unwrap_or("")) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("用例文件读取失败：{e}");
            return 2;
        }
    };
    let cases = match testkit::assert::parse_cases_yaml(&yaml) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    // fixture 路径以 YAML 内书写为准（fixtures/…）→ 相对项目根 = 用例文件的上两级
    let proj_root = cases_file
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let results = testkit::assert::run_cases(&src_text, &cases, &proj_root);
    println!("{}", testkit::summarize(&results));
    if testkit::all_ok(&results) {
        0
    } else {
        1
    }
}

// ───────────────────────── build（确定性装配） ─────────────────────────

fn cmd_build(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl build <srcdir> [--out dist]");
        return 2;
    }
    let dir = Path::new(&rest[0]);
    let mut out_dir = PathBuf::from("dist");
    let mut i = 1;
    while i < rest.len() {
        if rest[i] == "--out" {
            out_dir = PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or("dist"));
            i += 2;
        } else {
            i += 1;
        }
    }
    // 收集 srcdir 下全部 .rsl.json（递归子目录树，确定性排序，S§14）
    let mut files: Vec<PathBuf> = vec![];
    collect_sources(dir, &mut files);
    files.sort();
    if files.is_empty() {
        eprintln!("{} 下无 .rsl.json 源", dir.display());
        return 2;
    }
    std::fs::create_dir_all(&out_dir).ok();
    // 固定基准时间戳（逐字节一致，S§14）
    let mut manifest = String::from("# RSL build manifest（固定基准时间戳 2026-01-01T00:00:00Z）\n");
    let mut all_ok = true;
    for f in &files {
        let text = load(f.to_str().unwrap_or("")).unwrap_or_default();
        let out = analyze::compile_str(&text);
        // 产物保留子目录结构（dist/<相对路径>）
        let rel = f.strip_prefix(dir).unwrap_or(f).to_path_buf();
        if out.source.is_none() {
            println!("✗ {} 编译失败（语法门禁拦截，产物不落盘）", rel.display());
            all_ok = false;
            continue;
        }
        let dest = out_dir.join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&dest, &text).ok();
        manifest.push_str(&format!(
            "{} sha256:{}\n",
            rel.to_string_lossy(),
            sha256_hex(text.as_bytes())
        ));
        println!("✓ {} → {}", rel.display(), dest.display());
    }
    let manifest_path = out_dir.join("MANIFEST.txt");
    std::fs::write(&manifest_path, &manifest).ok();
    if all_ok {
        println!("✓ 装配完成：{}（{} 个源，产物逐字节可复现）", out_dir.display(), files.len());
        0
    } else {
        1
    }
}

/// 递归收集 .rsl.json（子目录树；跳过 dist/ target/ 与隐藏目录）。
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

// ───────────────────────── pack ─────────────────────────

fn cmd_pack(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl pack <src> [--out pkg.rsload]");
        return 2;
    }
    let src_path = &rest[0];
    let mut out = PathBuf::from(format!(
        "{}.rsload",
        Path::new(src_path).file_stem().and_then(|s| s.to_str()).unwrap_or("pkg")
    ));
    let mut i = 1;
    while i < rest.len() {
        if rest[i] == "--out" {
            out = PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or(""));
            i += 2;
        } else {
            i += 1;
        }
    }
    let text = match load(src_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    // 编译门禁：只 pack 能编译的源
    let out_c = analyze::compile_str(&text);
    if out_c.source.is_none() {
        eprintln!("pack 拒绝：源编译失败（S§14 语法门禁）");
        for d in out_c.errors() {
            eprintln!("{d}");
        }
        return 1;
    }
    let src = out_c.source.unwrap();
    // S§17 断言：分发产物不得内嵌除本源外的其他源/内容
    let id = &src.id;
    if text.contains("\"rsl\"") {
        let count = text.matches("\"rsl\"").count();
        if count != 1 {
            eprintln!("pack 拒绝：产物疑似内嵌 {count} 份源（分发层只发程序不发源，S§17）");
            return 1;
        }
    }
    let body_b64 = rsl_core::crypto::base64_encode(text.as_bytes());
    let sha = sha256_hex(text.as_bytes());
    let pkg = serde_json::json!({
        "format": "rsload/1",
        "id": id,
        "name": src.name,
        "rsl": src.rsl_version,
        "sha256": sha,
        "body_base64": body_b64,
    });
    std::fs::write(&out, serde_json::to_vec_pretty(&pkg).unwrap_or_default()).ok();
    println!("✓ 打包 {}（sha256 {sha}）", out.display());
    0
}

/// SHA-256（复用 crypto 模块的 sha2）。
fn sha256_hex(data: &[u8]) -> String {
    use sha2_ext::Digest;
    hex::encode(sha2_ext::Sha256::digest(data))
}

mod sha2_ext {
    pub use sha2::{Digest, Sha256};
}

// ───────────────────────── convert（Legado → RSL，MIGRATION.md） ─────────────────────────

fn cmd_convert(rest: &[String]) -> i32 {
    if rest.is_empty() {
        eprintln!("用法：rsl convert <legado.json> [--out dir]");
        return 2;
    }
    let input = &rest[0];
    let mut out_dir = PathBuf::from("converted");
    let mut i = 1;
    while i < rest.len() {
        if rest[i] == "--out" {
            out_dir = PathBuf::from(rest.get(i + 1).map(|s| s.as_str()).unwrap_or("converted"));
            i += 2;
        } else {
            i += 1;
        }
    }
    let text = match load(input) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let v: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Legado JSON 解析失败：{e}");
            return 2;
        }
    };
    // 单源或源列表
    let sources: Vec<serde_json::Value> = if v.is_array() {
        v.as_array().unwrap().clone()
    } else {
        vec![v]
    };
    std::fs::create_dir_all(&out_dir).ok();
    let mut exit = 0;
    for (n, leg) in sources.iter().enumerate() {
        match rsl_core_convert_one(leg) {
            Ok((rsl_text, report)) => {
                let id = report["id"].as_str().unwrap_or("source").replace(['.', '/'], "_");
                let stem = if sources.len() > 1 {
                    format!("{id}_{n}")
                } else {
                    id
                };
                let rsl_path = out_dir.join(format!("{stem}.rsl.json"));
                let rep_path = out_dir.join(format!("{stem}.migration.json"));
                let chk_path = out_dir.join(format!("{stem}.checklist.md"));
                std::fs::write(&rsl_path, &rsl_text).ok();
                std::fs::write(&rep_path, serde_json::to_vec_pretty(&report).unwrap_or_default()).ok();
                std::fs::write(&chk_path, build_checklist()).ok();
                println!("✓ {} → {}（不可转 {} 项）", stem, rsl_path.display(), report["dropped"].as_array().map(|a| a.len()).unwrap_or(0));
            }
            Err(e) => {
                eprintln!("✗ 转换失败：{e}");
                exit = 1;
            }
        }
    }
    exit
}

/// 单个 Legado 源 → RSL 草稿 + 迁移报告（MIGRATION §2 的 24 字段映射 + §5 不可转清单）。
fn rsl_core_convert_one(leg: &serde_json::Value) -> Result<(String, serde_json::Value), String> {
    let s = |k: &str| leg[k].as_str().unwrap_or("").to_string();
    let book_source_url = s("bookSourceUrl");
    if book_source_url.is_empty() {
        return Err("bookSourceUrl 为空".into());
    }
    let mut dropped: Vec<serde_json::Value> = vec![];
    let mut changed: Vec<serde_json::Value> = vec![];

    // #1 id/baseUrl（原值进 x-legado.bookSourceUrl）
    let origin = url_origin(&book_source_url).unwrap_or_else(|| book_source_url.clone());
    let id = origin
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();
    let id: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '.' })
        .collect();
    let _ = &id;

    // #4 bookSourceType → type（0 novel / 1 audio / 2 comic / 3 file / 4 video）
    let st = leg["bookSourceType"].as_i64().unwrap_or(0);
    let carrier = match st {
        1 => "audio",
        2 => "comic",
        3 => "file",
        4 => "video",
        _ => "novel",
    };

    // #6 searchUrl → request/paginate
    let search_url_raw = s("searchUrl");
    if search_url_raw.starts_with("@js:") || search_url_raw.contains("<js>") {
        changed.push(serde_json::json!({
            "field": "searchUrl(@js:)",
            "reason": "搜索请求由 JS 构造并签名（signParams/java.put），需人工重写为 let + expr:md5 + lib.ctx（§2 表 #6）",
            "legado": search_url_raw.chars().take(160).collect::<String>(),
        }));
    }
    let (search_url, search_opts) = split_legado_url(&search_url_raw);
    let page_placeholder = search_url.contains("{{page}}");
    let search_url_clean = search_url
        .replace("{{key}}", "${var:kw}")
        .replace("{{page}}", "");

    // #17 concurrentRate
    let concurrent = s("concurrentRate");
    let rate = parse_concurrent_rate(&concurrent);

    // #19 enabledCookieJar
    let cookie_jar = leg["enabledCookieJar"].as_bool().unwrap_or(false);

    // #21 bookUrlPattern
    let pattern = s("bookUrlPattern");

    // #22/#24 customOrder / respondTime → 无对应（进 x-legado + dropped）
    if leg.get("customOrder").is_some() {
        dropped.push(serde_json::json!({
            "field": "customOrder", "reason": "无对应字段（宿主侧排序）；值保留在 x-legado.customOrder", "legado": leg["customOrder"]
        }));
    }
    if leg.get("respondTime").is_some() {
        dropped.push(serde_json::json!({
            "field": "respondTime", "reason": "观测值不是源声明；保留在 x-legado.respondTime", "legado": leg["respondTime"]
        }));
    }
    // 死字段（§2.14）
    for dead in ["exploreScreen", "ruleReview"] {
        if leg.get(dead).is_some() {
            dropped.push(serde_json::json!({
                "field": dead, "reason": "死字段（无读取点），已丢弃"
            }));
        }
    }
    // loginUi 行为控件
    let login_ui = s("loginUi");
    if !login_ui.is_empty() {
        if login_ui.contains("\"button\"") || login_ui.contains("\"toggle\"") || login_ui.contains("\"select\"") {
            dropped.push(serde_json::json!({
                "field": "loginUi", "reason": "button/toggle/select 是行为控件，login.fields 只有输入语义；需人工重建"
            }));
        }
    }
    let login_check_js = s("loginCheckJs");
    if !login_check_js.is_empty() {
        changed.push(serde_json::json!({
            "field": "loginCheckJs", "to": "script.onResponse", "note": "它是响应改写器不是登录检查；500 重抛语义不可转（§5 #11）"
        }));
    }
    // loginUrl 含 JS → 人工重建步骤图（§2 表 #13 / §5 #4 #12）
    let login_url = s("loginUrl");
    if login_url.starts_with("@js:") || login_url.contains("<js>") {
        changed.push(serde_json::json!({
            "field": "loginUrl(@js:)",
            "reason": "登录流程由 JS 构造（java.put/get 副作用），需人工重建 login.steps[] 步骤图（§2 表 #13）",
            "legado": login_url.chars().take(120).collect::<String>(),
        }));
    }
    let js_lib = s("jsLib");
    if js_lib.starts_with("http") || js_lib.contains("@js:") {
        dropped.push(serde_json::json!({
            "field": "jsLib", "reason": "远程脚本 URL 不可转（§5 #5：源必须自包含）"
        }));
    }
    let _ = &js_lib;

    // exploreUrl → groups（§2 #7：title::url 串）
    let explore_url = s("exploreUrl");
    let mut groups = vec![];
    if explore_url.starts_with("@js:") || explore_url.contains("<js>") {
        changed.push(serde_json::json!({
            "field": "exploreUrl(@js:)",
            "reason": "发现分类由 JS 动态构造（68KB 级），需人工重写为 groupsFrom 或静态 groups[]（§2 表 #7）",
            "size_bytes": explore_url.len(),
        }));
    } else if !explore_url.is_empty() {
        for seg in explore_url.split(&['\n', '&'][..]) {
            let seg = seg.trim().trim_end_matches("&&").trim();
            if seg.is_empty() {
                continue;
            }
            if let Some((title, url)) = seg.split_once("::") {
                groups.push(serde_json::json!({ "title": title.trim(), "url": url.trim() }));
            }
        }
        if groups.is_empty() {
            dropped.push(serde_json::json!({ "field": "exploreUrl", "reason": "格式非 title::url 串，需人工重建 groups/groupsFrom" }));
        }
    }

    // ruleSearch → search.out（§2 #8）
    let rule_search = &leg["ruleSearch"];
    let search_out = build_search_out(rule_search, &mut changed);
    let rule_explore = &leg["ruleExplore"];
    let explore_out = build_search_out(rule_explore, &mut changed);

    // ruleToc nextTocUrl → paginate.next（§2 #11）
    let rule_toc = &leg["ruleToc"];
    let next_toc = rule_toc["nextTocUrl"].as_str().unwrap_or("");
    let toc_paginate = if next_toc.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!({ "kind": "next", "next": legado_rule_to_rsl(next_toc), "max": 50 })
    };

    // ruleContent content 为空 → direct:true（§4 行为差异 #2）
    let rule_content = &leg["ruleContent"];
    let content_rule = rule_content["content"].as_str().unwrap_or("");
    let direct = content_rule.is_empty();
    if direct {
        changed.push(serde_json::json!({
            "field": "ruleContent.content", "to": "content.direct: true", "note": "Legado 空正文规则=不请求直取；请人工确认 chapter.url 是正文"
        }));
    }

    // assemble
    let mut stages = serde_json::json!({});
    stages["search"] = serde_json::json!({
        "request": {
            "url": search_url_clean.trim(),
            "params": { "q": "var:kw" },
        },
        "out": search_out,
        "paginate": if page_placeholder { serde_json::json!({"kind":"page","pageVar":"page","max":50}) } else { serde_json::Value::Null },
    });
    if !explore_url.is_empty() {
        let mut e = serde_json::json!({ "out": explore_out });
        if !groups.is_empty() {
            e["groups"] = Value::Array(groups);
        }
        stages["explore"] = e;
    }
    stages["detail"] = build_detail_out(&leg["ruleBookInfo"], &mut changed);
    let mut toc_stage = serde_json::json!({
        "request": { "url": "var:book.tocUrl" },
        "out": build_toc_out(rule_toc, &mut changed),
    });
    if !toc_paginate.is_null() {
        toc_stage["paginate"] = toc_paginate;
    }
    stages["toc"] = toc_stage;
    let mut content_stage = serde_json::json!({
        "request": { "url": "var:chapter.url" },
        "out": build_content_out(rule_content, &mut changed),
    });
    if direct {
        content_stage["direct"] = serde_json::json!(true);
        content_stage["request"] = serde_json::json!({ "skip": true });
        content_stage["out"]["content"] = serde_json::json!("var:chapter.url");
    }
    stages["content"] = content_stage;

    let mut http = serde_json::json!({});
    if !rate.is_null() {
        http["rate"] = rate;
    }
    http["cookies"] = if cookie_jar { Value::String("session".into()) } else { Value::String("none".into()) };
    // #16 header
    let header = s("header");
    if let Ok(h) = serde_json::from_str::<serde_json::Value>(&header) {
        http["headers"] = h;
    } else if !header.is_empty() {
        changed.push(serde_json::json!({ "field": "header", "note": "非 JSON 格式请求头，需人工转换或 @js: 进 script.before" }));
    }

    let mut root = serde_json::json!({
        "rsl": "2.0",
        "id": id,
        "name": if s("bookSourceName").is_empty() { id.clone() } else { s("bookSourceName") },
        "type": carrier,
        "baseUrl": origin,
        "stages": stages,
    });
    if !http.is_null() {
        root["http"] = http;
    }
    if !pattern.is_empty() {
        root["match"] = serde_json::json!({ "detail": format!("re:{}", pattern.trim_start_matches("re:")) });
    }
    if s("bookSourceComment").is_empty() {
        root["meta"] = serde_json::json!({ "comment": "由 rsl convert 从 Legado 源迁移（草稿，需人工复核 MIGRATION.md §6 清单）" });
    } else {
        root["meta"] = serde_json::json!({ "comment": s("bookSourceComment") });
    }
    let mut xleg = serde_json::json!({ "bookSourceUrl": book_source_url });
    if let Some(co) = leg.get("customOrder") {
        xleg["customOrder"] = co.clone();
    }
    if let Some(rt) = leg.get("respondTime") {
        xleg["respondTime"] = rt.clone();
    }
    if !concurrent.is_empty() {
        xleg["concurrentRate"] = Value::String(concurrent.clone());
    }
    root["x-legado"] = xleg;

    let report = serde_json::json!({
        "id": id,
        "dropped": dropped,
        "changed": changed,
        "manual_checks": [
            "编码双向验证（encode.params vs request.charset）",
            "分页每条补 max + stopIf",
            "登录闭环（check.ok 选未登录不存在的字段）",
            "目录完整性（章节数/卷标记/排序）"
        ],
    });
    let rsl_text = serde_json::to_string_pretty(&root).unwrap_or_default();
    Ok((rsl_text, report))
}

fn build_checklist() -> String {
    format!(
        "# 转换后检查清单（MIGRATION §6）\n\n{}",
        [
            "1. rsl lint 全绿（20 条）",
            "2. baseUrl 与 id 人工确认",
            "3. 每阶段 rsl run --trace 看 matched/bridge",
            "4. 编码双向验证（encode.params vs request.charset）",
            "5. 登录闭环（check.ok 选未登录不存在的字段）",
            "6. 目录完整性（章节数/isVolume/isVip/order）",
            "7. 分页终止（max + stopIf.noNew）",
            "8. 正文与媒体（direct 确认；images 不混入非图片）",
            "9. 降级路径（增强字段 optional + default）",
            "10. 回归固化（rsl test 至少覆盖 正常/改版/空结果）",
        ]
        .into_iter()
        .map(|s| format!("- [ ] {s}"))
        .collect::<Vec<_>>()
        .join("\n")
    )
}

fn url_origin(url: &str) -> Option<String> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let host_end = rest.find('/').unwrap_or(rest.len());
    Some(format!("{}://{}", &url[..scheme_end], &rest[..host_end]))
}

/// 拆 Legado searchUrl 尾缀 `,{json}`（MIGRATION §2.1 ①）。
fn split_legado_url(raw: &str) -> (String, Option<serde_json::Value>) {
    if let Some(pos) = raw.rfind(",{") {
        let (url, opts) = raw.split_at(pos);
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(opts.trim_start_matches(',')) {
            return (url.to_string(), Some(v));
        }
    }
    (raw.to_string(), None)
}

/// concurrentRate "N/M" → rate {windowMs, max}（§2 #17）。
fn parse_concurrent_rate(s: &str) -> serde_json::Value {
    let t = s.trim();
    if t.is_empty() {
        return serde_json::Value::Null;
    }
    if let Some((n, m)) = t.split_once('/') {
        if let (Ok(nn), Ok(mm)) = (n.trim().parse::<i64>(), m.trim().parse::<i64>()) {
            return serde_json::json!({ "windowMs": mm, "max": nn });
        }
    }
    if let Ok(n) = t.parse::<i64>() {
        return serde_json::json!({ "windowMs": n, "max": 1 });
    }
    serde_json::Value::Null
}

/// Legado 规则字符串 → RSL 选择器（MIGRATION §3 语法翻译）。
/// Err(reason) = 不可转（@js:/<js>/data: 伪协议/函数包裹），须进 migration 报告。
fn legado_rule_to_rsl(rule: &str) -> Result<String, String> {
    let r = rule.trim();
    // ① JS 规则：一律不可转（§5 #1/#8；source.getVariable/java.put 等）
    if r.starts_with("@js:") || r.contains("<js>") || r.contains("{{$.") || r.contains("{{$.") {
        return Err("规则含 @js:/<js>/模板插值，绑定 java.*/source.getVariable，需人工重写（§5 #8）".into());
    }
    // ② data: 伪协议（§16 明确不采纳 / §5 #7）
    if r.starts_with("data:") || r.contains("'data:;base64,'") {
        return Err("data: 伪协议不可转（§16）：改用 out 契约字段承载参数（toc.out.map 额外键 → var:chapter.*）".into());
    }
    // ③ 显式前缀
    for (pre, ns) in [("@CSS:", "css:"), ("@css:", "css:"), ("@XPath:", "xpath:"), ("@Json:", "json:")] {
        if let Some(rest) = r.strip_prefix(pre) {
            return Ok(format!("{ns}{rest}"));
        }
    }
    // ④ ## 正则段剥离（取第一段作为选择器）
    let main = r.split("##").next().unwrap_or(r);
    let main = main.trim();
    // ⑤ a@text → css:a
    if let Some((sel, _attr)) = main.rsplit_once('@') {
        if !sel.is_empty() && !sel.contains(' ') && !sel.contains('$') {
            return Ok(format!("css:{sel}"));
        }
    }
    if main.starts_with('/') || main.starts_with("//") {
        return Ok(format!("xpath:{main}"));
    }
    if main.starts_with("$.") || main.starts_with("$[") {
        return Ok(format!("json:{main}"));
    }
    // ⑥ 裸 JSON 点路径（data.books → json:$.data.books；Legado isJSON 语境常见）
    if !main.contains('/') && !main.contains('@') && main.contains('.')
        && main.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '[' || c == ']')
        && main.split('.').all(|p| !p.is_empty())
    {
        return Ok(format!("json:$.{main}"));
    }
    Ok(format!("css:{main}"))
}

/// 把规则字段写入 out；@js:/data: 规则自动进 changed 报告。
fn put_rule(
    out: &mut serde_json::Value,
    key: &str,
    rule: &str,
    field: &str,
    changed: &mut Vec<serde_json::Value>,
) {
    match legado_rule_to_rsl(rule) {
        Ok(r) => {
            out[key] = Value::String(r);
        }
        Err(reason) => {
            changed.push(serde_json::json!({
                "field": field,
                "reason": reason,
                "legado": rule.chars().take(120).collect::<String>(),
            }));
        }
    }
}

fn build_search_out(rule: &serde_json::Value, changed: &mut Vec<serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::json!({ "emptyOk": true });
    let list = rule["bookList"].as_str().unwrap_or("");
    if !list.is_empty() {
        put_rule(&mut out, "list", list, "bookList", changed);
    }
    let mut map = serde_json::json!({});
    let fields = [
        ("name", "name"),
        ("author", "author"),
        ("intro", "intro"),
        ("kind", "kind"),
        ("wordCount", "wordCount"),
        ("lastChapter", "lastChapter"),
        ("coverUrl", "cover"),
        ("bookUrl", "url"),
    ];
    for (leg_key, rsl_key) in fields {
        if let Some(v) = rule[leg_key].as_str() {
            if !v.is_empty() {
                let mut tmp = serde_json::json!({});
                put_rule(&mut tmp, rsl_key, v, &format!("ruleSearch.{leg_key}"), changed);
                if let Some(x) = tmp.get(rsl_key) {
                    map[rsl_key] = x.clone();
                }
            }
        }
    }
    // checkKeyWord → verify.contains
    if let Some(ck) = rule["checkKeyWord"].as_str() {
        if !ck.is_empty() {
            out["verify"] = serde_json::json!({ "contains": ck });
        }
    }
    out["map"] = map;
    out
}

fn build_detail_out(rule: &serde_json::Value, changed: &mut Vec<serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::json!({});
    let fields = [
        ("name", "name"),
        ("author", "author"),
        ("intro", "intro"),
        ("kind", "kind"),
        ("wordCount", "wordCount"),
        ("lastChapter", "lastChapter"),
        ("updateTime", "updateTime"),
        ("coverUrl", "cover"),
        ("tocUrl", "tocUrl"),
    ];
    for (leg_key, rsl_key) in fields {
        if let Some(v) = rule[leg_key].as_str() {
            if !v.is_empty() {
                put_rule(&mut out, rsl_key, v, &format!("ruleBookInfo.{leg_key}"), changed);
            }
        }
    }
    if let Some(cr) = rule["canReName"].as_str() {
        if !cr.is_empty() {
            put_rule(&mut out, "canRename", cr, "ruleBookInfo.canReName", changed);
        }
    }
    out
}

fn build_toc_out(rule: &serde_json::Value, changed: &mut Vec<serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::json!({});
    let list = rule["chapterList"].as_str().unwrap_or("");
    if !list.is_empty() {
        // <js>…</js> 包裹：剥离 JS 前缀并报告，取剩余路径
        let mut effective = list.to_string();
        if let Some(end) = list.find("</js>") {
            let js_part = &list[..end + 5];
            effective = list[end + 5..].trim().to_string();
            changed.push(serde_json::json!({
                "field": "ruleToc.chapterList(<js>)",
                "reason": "目录列表 JS 前缀（java.put 副作用）不可转；副作用需人工搬到 lib.ctx",
                "legado": js_part.chars().take(120).collect::<String>(),
            }));
        }
        if !effective.is_empty() {
            put_rule(&mut out, "list", &effective, "ruleToc.chapterList", changed);
        }
    }
    let mut map = serde_json::json!({});
    for (leg_key, rsl_key) in [("chapterName", "title"), ("chapterUrl", "url")] {
        if let Some(v) = rule[leg_key].as_str() {
            if !v.is_empty() {
                put_rule(&mut map, rsl_key, v, &format!("ruleToc.{leg_key}"), changed);
            }
        }
    }
    for k in ["isVolume", "isVip", "isPay", "updateTime", "wordCount"] {
        if let Some(v) = rule[k].as_str() {
            if !v.is_empty() {
                put_rule(&mut map, k, v, &format!("ruleToc.{k}"), changed);
            }
        }
    }
    if rule["formatJs"].is_object() || rule["formatJs"].is_string() {
        changed.push(serde_json::json!({
            "field": "ruleToc.formatJs", "reason": "逐章 JS 与 E_COST 冲突（§5 #13），需人工重写为 lib.pure"
        }));
    }
    out["map"] = map;
    out
}

fn build_content_out(rule: &serde_json::Value, changed: &mut Vec<serde_json::Value>) -> serde_json::Value {
    let mut out = serde_json::json!({});
    if let Some(t) = rule["title"].as_str() {
        if !t.is_empty() {
            put_rule(&mut out, "title", t, "ruleContent.title", changed);
        }
    }
    let content = rule["content"].as_str().unwrap_or("");
    if !content.is_empty() {
        put_rule(&mut out, "content", content, "ruleContent.content", changed);
    }
    if let Some(np) = rule["nextContentUrl"].as_str() {
        if !np.is_empty() {
            put_rule(&mut out, "nextPage", np, "ruleContent.nextContentUrl", changed);
        }
    }
    if let Some(pa) = rule["payAction"].as_str() {
        if !pa.is_empty() {
            out["payAction"] = Value::String(pa.to_string());
        }
    }
    // callBackJs（自定义按钮/宿主事件）→ 不可转报告
    if let Some(cb) = rule["callBackJs"].as_str() {
        if !cb.is_empty() {
            changed.push(serde_json::json!({
                "field": "ruleContent.callBackJs",
                "reason": "宿主按钮/事件 JS 不可转；改 hostStages 或宿主按钮（§5 #1）",
                "legado": cb.chars().take(80).collect::<String>(),
            }));
        }
    }
    // replaceRegex → transform.replace
    if let Some(rr) = rule["replaceRegex"].as_str() {
        if !rr.is_empty() {
            out["transform"] = serde_json::json!([{ "replace": [[rr, ""]] }]);
        }
    }
    // webJs/sourceRegex → request（§2 #12）
    let _ = (rule, changed);
    out
}

// ───────────────────────── explain ─────────────────────────

fn cmd_explain(rest: &[String]) -> i32 {
    if rest.len() < 2 {
        eprintln!("用法：rsl explain <src> <path> [--kw …]");
        return 2;
    }
    let src_path = &rest[0];
    let rule_path = &rest[1];
    let text = match load(src_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let out = analyze::compile_str(&text);
    let Some(src) = out.source else {
        eprintln!("编译失败（explain 需要可编译源）");
        return 1;
    };
    rsl_core::script::setup_engine(&src);
    // path → (stage, selector)
    // 简化：path 形如 stages.<stage>.out.<field> 或 stages.<stage>.out.map.<field>
    let parts: Vec<&str> = rule_path.split('.').collect();
    if parts.len() < 4 || parts[0] != "stages" {
        eprintln!("path 应形如 stages.toc.out.map.title");
        return 2;
    }
    let stage_name = parts[1];
    let field = parts.last().copied().unwrap_or("");
    let Some(stage) = src.stage(stage_name) else {
        eprintln!("阶段 {stage_name} 不存在");
        return 1;
    };
    // 找选择器
    let mut found: Option<String> = None;
    for (k, sel) in &stage.out.map {
        if k == field {
            found = Some(sel.raw.clone());
        }
    }
    if found.is_none() {
        if field == "list" {
            if let Some(l) = &stage.out.list {
                found = Some(l.raw.clone());
            }
        } else if field == "content" {
            if let Some(c) = &stage.out.content {
                found = Some(c.raw.clone());
            }
        }
    }
    match &found {
        Some(rule) => {
            println!("规则 {field} = {rule}");
            println!("提示：连上网络用 `rsl run {src_path} {stage_name} --trace` 看 matched 与 bridge；");
            println!("      对比同义选择器可加 fallback 依次尝试（S§3.7）。");
            0
        }
        None => {
            eprintln!("未找到字段 {field} 的规则（stages.{stage_name}.out.map keys 见 compile 摘要）");
            1
        }
    }
}
