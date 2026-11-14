//! A1 fuzz harness ×5 + 进程内循环 fuzzer（TASK-BOOK-2 §3.1 A1 / V2-D6 裁决）。
//!
//! windows-gnu 上 libFuzzer/cargo-fuzz 支持不完整（MSVC-only 路线），改用
//! 进程内变异循环：种子语料（fuzz/corpus）→ 结构化/字节变异 → catch_unwind + panic hook
//! 双保险 → 崩溃最小化归档 scratch/fuzz/artifacts/ → 报告 scratch/fuzz/report/。
//! harness 形态与 cargo-fuzz 的 `fn(&[u8])` 一致，后续接入 libFuzzer 零成本。

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 变异用 token 池（语法相邻片段，与 properties.rs 的 nasty_string 同源）。
const TOKENS: &[&str] = &[
    "css:", "re:", "json:$.", "xpath://", "var:", "let:", "expr:", "lib:", "${", "}",
    "(", ")", "[", "]", "{", "}", "@", ".", "::", "a.title", "div#x", "\\", "\"", "'",
    "|", "?", "*", "==", "false", "true", "1", "-1", "\n", "%", "trim", "regex", "attr",
    "href", "rsl", "stages", "out", "list", "map", "paginate", "page", "0", "9223372036854775807",
    "é", "\u{0}", "\t", "  ", "&", ">", "<", "\u{10FFFF}", "null", "nan",
];

/// 五个 fuzz 目标（输入为任意字节）。
pub fn run_target(name: &str, data: &[u8]) {
    match name {
        "compile_str" => {
            let s = String::from_utf8_lossy(data);
            let _ = rsl_core::compile(&s);
        }
        "convert_full" => {
            // 整源文件变异：JSON 解析（失败即跳过——convert 输入本就须为 JSON）→ compile
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
                let s = serde_json::to_string(&v).unwrap_or_default();
                let _ = rsl_core::compile(&s);
            }
        }
        "jsonpath" => {
            let s = String::from_utf8_lossy(data);
            if let Some(segs) = rsl_core::selector::jsonpath::parse(&s) {
                let doc = serde_json::json!({"a": [{"b": 1, "c": "x"}, {"b": false}], "d": {"e": [1, 2, 3]}});
                let _ = rsl_core::selector::jsonpath::eval(&doc, &segs);
            }
        }
        "xpath" => {
            let s = String::from_utf8_lossy(data);
            let html = rsl_core::exec::value::HtmlDoc::parse(
                r#"<html><body><ul><li><a href="/1">一</a></li><li class="x">二</li></ul></body></html>"#,
            );
            let _ = rsl_core::selector::xpath::eval(&html.html, None, &s);
        }
        "template_expr" => {
            let s = String::from_utf8_lossy(data);
            let _ = rsl_core::sugar::parse_template(&s, "fuzz");
            let _ = rsl_core::expr::parse(&s, "fuzz");
        }
        _ => panic!("未知 fuzz target: {name}"),
    }
}

pub const TARGETS: &[&str] = &["compile_str", "convert_full", "jsonpath", "xpath", "template_expr"];

// ───────────────────────── 变异器 ─────────────────────────

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// JSON 结构感知变异：解析成功 → 递归变异字符串/数值/键；失败 → 字节级变异。
fn mutate(seed: &[u8], rng: &mut Rng, depth: usize) -> Vec<u8> {
    if depth < 3 && rng.below(10) < 4 {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(seed) {
            let mut v2 = v.clone();
            mutate_value(&mut v2, rng, 0);
            if let Ok(s) = serde_json::to_vec(&v2) {
                return s;
            }
        }
    }
    mutate_bytes(seed, rng)
}

fn mutate_value(v: &mut serde_json::Value, rng: &mut Rng, depth: usize) {
    if depth > 6 {
        return;
    }
    match v {
        serde_json::Value::String(s) => {
            match rng.below(4) {
                0 => {
                    let t = TOKENS[rng.below(TOKENS.len())];
                    s.push_str(t);
                }
                1 => {
                    let cut = rng.below(s.len() + 1);
                    s.truncate(s.char_indices().nth(cut).map(|(i, _)| i).unwrap_or(s.len()));
                }
                2 => {
                    let t = TOKENS[rng.below(TOKENS.len())];
                    *s = format!("{t}{s}");
                }
                _ => {
                    let t = TOKENS[rng.below(TOKENS.len())];
                    *s = t.to_string();
                }
            }
            if s.len() > 4096 {
                *s = s.chars().take(4096).collect();
            }
        }
        serde_json::Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0) * (rng.below(7) as f64 - 3.0).powi(3);
            *v = serde_json::json!(f);
        }
        serde_json::Value::Array(a) if !a.is_empty() => {
            let i = rng.below(a.len());
            match rng.below(4) {
                0 => mutate_value(&mut a[i], rng, depth + 1),
                1 => {
                    a.insert(i, serde_json::json!(TOKENS[rng.below(TOKENS.len())]));
                }
                2 => {
                    a.remove(i);
                }
                _ => {
                    let n = a.len();
                    if n > 1 {
                        let j = rng.below(n);
                        a.swap(i, j);
                    }
                }
            }
        }
        serde_json::Value::Object(m) if !m.is_empty() => {
            let keys: Vec<String> = m.keys().cloned().collect();
            let i = rng.below(keys.len());
            let k = keys[i].clone();
            match rng.below(4) {
                0 => mutate_value(m.get_mut(&k).unwrap(), rng, depth + 1),
                1 => {
                    m.insert(format!("{k}x"), serde_json::json!(TOKENS[rng.below(TOKENS.len())]));
                }
                2 => {
                    m.remove(&k);
                }
                _ => {
                    let e = m.remove(&k);
                    if let Some(e) = e {
                        m.insert(k, e);
                    }
                }
            }
        }
        serde_json::Value::Null => {
            *v = serde_json::json!(TOKENS[rng.below(TOKENS.len())]);
        }
        _ => {}
    }
}

fn mutate_bytes(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = seed.to_vec();
    match rng.below(6) {
        0 => out.truncate(rng.below(out.len() + 1)), // 截断
        1 => {
            if !out.is_empty() {
                let i = rng.below(out.len());
                out[i] ^= 1 << rng.below(8); // 位翻转
            }
        }
        2 => {
            let t = TOKENS[rng.below(TOKENS.len())];
            let i = rng.below(out.len() + 1);
            out.splice(i..i, t.bytes()); // 插块
        }
        3 => {
            if out.len() > 4 {
                let i = rng.below(out.len());
                let j = rng.below(out.len());
                out.swap(i, j); // 交换
            }
        }
        4 => {
            // 与另一 token 拼接
            let t = TOKENS[rng.below(TOKENS.len())];
            out.extend_from_slice(t.as_bytes());
        }
        _ => {
            // 越界小数值注入
            let t = ["9223372036854775808", "-9223372036854775809", "1e999", "-0"];
            let t = t[rng.below(4)];
            let i = rng.below(out.len() + 1);
            out.splice(i..i, t.bytes());
        }
    }
    if out.len() > 256 * 1024 {
        out.truncate(256 * 1024);
    }
    out
}

// ───────────────────────── runner ─────────────────────────

const CRASH_COUNT: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static LAST_INPUT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

pub struct FuzzOpts {
    pub target: String,
    pub minutes: u64,
    pub rounds: u64,
    pub corpus_dir: PathBuf,
    pub artifacts_dir: PathBuf,
    pub report_path: PathBuf,
    pub seed: u64,
}

pub fn run_fuzz(opts: &FuzzOpts) -> i32 {
    // panic hook：记录现场 → 归档最小用例（不中断循环，catch_unwind 兜底）
    let artifacts = opts.artifacts_dir.clone();
    let target_name = opts.target.clone();
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        CRASH_COUNT.fetch_add(1, Ordering::SeqCst);
        let n = CRASH_COUNT.load(Ordering::SeqCst);
        let dir = artifacts.join(&target_name);
        let _ = std::fs::create_dir_all(&dir);
        LAST_INPUT.with(|li| {
            if let Some(data) = li.borrow().as_ref() {
                let path = dir.join(format!("crash_{n:04}.bin"));
                let _ = std::fs::write(&path, data);
                eprintln!("[fuzz] CRASH #{n} 归档 {}", path.display());
            }
        });
        eprintln!("[fuzz] panic: {info}");
        default_hook(info);
    }));

    // 种子语料
    let mut corpus: Vec<Vec<u8>> = vec![];
    if let Ok(rd) = std::fs::read_dir(&opts.corpus_dir) {
        let mut files: Vec<_> = rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort_by_key(|e| e.file_name());
        for e in files {
            if let Ok(b) = std::fs::read(e.path()) {
                corpus.push(b);
            }
        }
    }
    // 目标专用种子：parser 类目标补选择器/模板种子
    match opts.target.as_str() {
        "jsonpath" => ["$.a[*].b", "$..c", "$[?@.b==1]", "$.d.e[1:]", "$['a'][0]"]
            .iter()
            .for_each(|s| corpus.push(s.as_bytes().to_vec())),
        "xpath" => ["//ul/li", "//a[@href]/text()", "//li[@class='x']", "//ul//a[2]", "count(//li)"]
            .iter()
            .for_each(|s| corpus.push(s.as_bytes().to_vec())),
        "template_expr" => ["${var:x}", "css:a.title|trim", "${expr:md5('x')}", "${1+1}", "${split(a,',')|join('-')}"]
            .iter()
            .for_each(|s| corpus.push(s.as_bytes().to_vec())),
        _ => {}
    }
    if corpus.is_empty() {
        corpus.push(b"{}".to_vec());
    }

    let mut rng = Rng(opts.seed | 1);
    let start = Instant::now();
    let deadline = start + Duration::from_secs(opts.minutes * 60);
    let mut executed: u64 = 0;
    let mut covered_kinds: u64 = 0;
    while executed < opts.rounds && Instant::now() < deadline {
        let seed = &corpus[rng.below(corpus.len())];
        let data = mutate(seed, &mut rng, 0);
        covered_kinds |= 1 << (data.first().map(|b| *b % 64).unwrap_or(0));
        LAST_INPUT.with(|li| *li.borrow_mut() = Some(data.clone()));
        let t = opts.target.clone();
        let d = data.clone();
        let r = std::panic::catch_unwind(move || run_target(&t, &d));
        if r.is_err() {
            // hook 已归档；继续（find-use-case 语义：发现即归档，不中断）
        }
        executed += 1;
        if executed % 2000 == 0 {
            eprintln!(
                "[fuzz {}] {} rounds, {} crashes, {:.0}s",
                opts.target,
                executed,
                CRASH_COUNT.load(Ordering::SeqCst),
                start.elapsed().as_secs_f32()
            );
        }
    }
    // 报告
    let report = serde_json::json!({
        "target": opts.target,
        "rounds": executed,
        "seconds": start.elapsed().as_secs_f64().round(),
        "corpus_seeds": corpus.len(),
        "crashes": CRASH_COUNT.load(Ordering::SeqCst),
        "coverage_kinds": covered_kinds,
        "seed": opts.seed,
        "verdict": if CRASH_COUNT.load(Ordering::SeqCst) == 0 { "zero-panic" } else { "crashes-found" },
        "generated_at": chrono_like_now(),
    });
    let _ = std::fs::create_dir_all(opts.report_path.parent().unwrap_or(&PathBuf::from(".")));
    let _ = std::fs::write(&opts.report_path, serde_json::to_string_pretty(&report).unwrap());
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if CRASH_COUNT.load(Ordering::SeqCst) == 0 {
        0
    } else {
        1
    }
}

pub fn chrono_like_now() -> String {
    // 本地时间 yyyy-MM-dd HH:MM:SS（无 chrono 依赖，用 ports::now_ms）
    let ms = rsl_core::ports::now_ms();
    let secs = ms / 1000;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // days 自 1970-01-01（UTC）
    let mut y = 1970i64;
    let mut d = days;
    loop {
        let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
        let len = if leap { 366 } else { 365 };
        if d >= len {
            d -= len;
            y += 1;
        } else {
            break;
        }
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mdays = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while d >= mdays[m] {
        d -= mdays[m];
        m += 1;
    }
    format!("{y:04}-{:02}-{:02} {h:02}:{mi:02}:{s:02}", m + 1, d + 1)
}

/// `rsl fuzz` 命令入口。返回退出码。
pub fn cmd_fuzz(rest: &[String]) -> i32 {
    let mut target = "compile_str".to_string();
    let mut minutes = 5u64;
    let mut rounds = u64::MAX;
    let mut seed = 0xC0FFEEu64;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--minutes" => {
                i += 1;
                minutes = rest.get(i).and_then(|s| s.parse().ok()).unwrap_or(5);
            }
            "--rounds" => {
                i += 1;
                rounds = rest.get(i).and_then(|s| s.parse().ok()).unwrap_or(u64::MAX);
            }
            "--seed" => {
                i += 1;
                seed = rest.get(i).and_then(|s| s.parse().ok()).unwrap_or(0xC0FFEE);
            }
            other => target = other.to_string(),
        }
        i += 1;
    }
    if !TARGETS.contains(&target.as_str()) {
        eprintln!("未知 fuzz target `{target}`；可选：{}", TARGETS.join(" "));
        return 2;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report_dir = root.join("scratch/fuzz/report");
    let opts = FuzzOpts {
        target: target.clone(),
        minutes,
        rounds,
        corpus_dir: root.join("fuzz/corpus"),
        artifacts_dir: root.join("scratch/fuzz/artifacts"),
        report_path: report_dir.join(format!("{target}.json")),
        seed,
    };
    println!("fuzz target={target} minutes={minutes} corpus={}", opts.corpus_dir.display());
    run_fuzz(&opts)
}
