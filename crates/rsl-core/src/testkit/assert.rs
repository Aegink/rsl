//! T5.2 断言引擎：S§13.2 YAML 用例 + `count()` / 路径断言 / `=~` 正则 / merge 断言。

use super::{CaseResult, Level, TestCase};
use crate::stage::merge::merge_sources;
use crate::stage::{HostCaps, ResponseReuse, RunInputs};
use serde_json::Value;

/// 运行一批用例（fixture 路径相对于 `fixtures_dir`）。
#[allow(clippy::too_many_arguments)]
pub fn run_cases(
    src_text: &str,
    cases: &[TestCase],
    fixtures_dir: &std::path::Path,
) -> Vec<CaseResult> {
    let mut results = vec![];
    for case in cases {
        let r = run_case(src_text, case, fixtures_dir);
        results.push(r);
    }
    results
}

pub fn run_case(src_text: &str, case: &TestCase, fixtures_dir: &std::path::Path) -> CaseResult {
    let mut fail = |detail: String| CaseResult {
        name: case.name.clone(),
        ok: false,
        level: case.level,
        detail,
    };
    // 编译（L1 起）
    let out = crate::compile(src_text);
    let Some(src) = out.source else {
        return fail(format!(
            "编译失败：{}",
            out.errors().first().map(|d| d.to_string()).unwrap_or_default()
        ));
    };
    crate::script::setup_engine(&src);

    // merge 用例
    if let Some(glob) = &case.merge_glob {
        return run_merge_case(&src, case, fixtures_dir, glob);
    }

    // fixture 传输（L2/L3 由 fixture 提供；真实 HTTP 用例显式声明 L3 且无 fixture 时走 ureq）
    let transport: std::rc::Rc<dyn crate::ports::Transport> = match &case.fixture {
        Some(f) => {
            let path = fixtures_dir.join(f);
            match super::record::load_fixture(&path) {
                Ok(resp) => std::rc::Rc::new(crate::ports::FixtureTransport {
                    responses: vec![resp],
                    idx: Default::default(),
                }),
                Err(e) => return fail(format!("fixture 加载失败 {f}: {e}")),
            }
        }
        None => {
            #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
            {
                std::rc::Rc::new(crate::http::UreqTransport::new(&src.http)) as std::rc::Rc<dyn crate::ports::Transport>
            }
            #[cfg(not(all(feature = "net", not(target_arch = "wasm32"))))]
            {
                std::rc::Rc::new(crate::ports::FixtureTransport { responses: vec![], idx: Default::default() })
                    as std::rc::Rc<dyn crate::ports::Transport>
            }
        }
    };
    let clock = crate::ports::SystemClock;
    let cookies = crate::ports::MemoryCookieStore::default();
    let mut cx = crate::exec::ctx::ExecCx::new(&src, transport.as_ref(), &clock);
    cx.cookie_store = Some(&cookies);

    // vars 注入
    if let Value::Object(m) = &case.vars {
        for (k, v) in m {
            if k == "kw" {
                cx.vars.insert("kw".into(), v.clone());
            } else if k == "page" {
                cx.vars.insert("page".into(), v.clone());
            } else {
                cx.vars.insert(k.clone(), v.clone());
            }
        }
    }

    let inputs = inputs_from_vars(&case.vars);
    let mut reuse = ResponseReuse::default();
    let result = crate::stage::run_stage(&src, &case.stage, &mut cx, &inputs, &mut reuse, &HostCaps::default());

    // expect_error
    if let Some(expect) = &case.expect_error {
        return match &result {
            Err(d) => {
                let code_ok = d.code.as_str() == expect;
                let at_ok = case
                    .at
                    .as_ref()
                    .map(|a| d.path.contains(a.as_str()))
                    .unwrap_or(true);
                CaseResult {
                    name: case.name.clone(),
                    ok: code_ok && at_ok,
                    level: case.level,
                    detail: format!(
                        "expect {expect}@{} → {}（{}）",
                        case.at.clone().unwrap_or_default(),
                        d.code.as_str(),
                        if at_ok { "path 匹配".to_string() } else { format!("path={}", d.path) }
                    ),
                }
            }
            Ok(_) => fail(format!("期望错误 {expect}，实际成功")),
        };
    }

    let out_json = match result {
        Ok(r) => r.out,
        Err(d) => return fail(format!("阶段执行失败：{}", d.code.as_str())),
    };

    // 断言求值
    let mut all_ok = true;
    let mut details = vec![];
    for expr in &case.assert_exprs {
        match eval_assert(expr, &out_json) {
            Ok(true) => details.push(format!("✓ {expr}")),
            Ok(false) => {
                all_ok = false;
                details.push(format!("✗ {expr}"));
            }
            Err(e) => {
                all_ok = false;
                details.push(format!("✗ {expr}（{e}）"));
            }
        }
    }
    CaseResult {
        name: case.name.clone(),
        ok: all_ok,
        level: case.level,
        detail: details.join("；"),
    }
}

fn run_merge_case(
    _src: &crate::ir::CompiledSource,
    case: &TestCase,
    fixtures_dir: &std::path::Path,
    glob: &str,
) -> CaseResult {
    let fail = |d: String| CaseResult {
        name: case.name.clone(),
        ok: false,
        level: case.level,
        detail: d,
    };
    // 读 fixtures/merge/*.json：每份 = {source: 名称, items: [...]}
    let dir = fixtures_dir.join(glob.trim_end_matches("/*").trim_end_matches('/'));
    let mut entries = vec![];
    let mut names = vec![];
    let read = std::fs::read_dir(&dir);
    match read {
        Ok(rd) => {
            for f in rd.flatten() {
                let p = f.path();
                if p.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(v) = serde_json::from_slice::<Value>(&std::fs::read(&p).unwrap_or_default()) {
                        let source = v["source"].as_str().unwrap_or("src").to_string();
                        let items = v["items"].as_array().cloned().unwrap_or_default();
                        names.push(source.clone());
                        entries.push((source, items));
                    }
                }
            }
        }
        Err(e) => return fail(format!("merge fixture 目录读取失败：{e}")),
    }
    if entries.is_empty() {
        return fail("无 merge fixture".into());
    }
    let merged = merge_sources(entries, &_src.merge);
    // 断言（基于 merged 顶层数组）
    let arr = Value::Array(merged);
    let mut all_ok = true;
    let mut details = vec![];
    for expr in &case.assert_exprs {
        match eval_assert(expr, &arr) {
            Ok(true) => details.push(format!("✓ {expr}")),
            Ok(false) | Err(_) => {
                all_ok = false;
                details.push(format!("✗ {expr}"));
            }
        }
    }
    let _ = &names;
    CaseResult {
        name: case.name.clone(),
        ok: all_ok,
        level: case.level,
        detail: details.join("；"),
    }
}

/// 从 vars 提取 RunInputs。
fn inputs_from_vars(vars: &Value) -> RunInputs {
    let mut inputs = RunInputs::default();
    if let Value::Object(m) = vars {
        for (k, v) in m {
            match k.as_str() {
                "kw" => inputs.kw = v.as_str().map(String::from),
                "page" => inputs.page = v.as_i64(),
                _ => {
                    if k.starts_with("book.") {
                        inputs
                            .book
                            .insert(k.trim_start_matches("book.").to_string(), v.clone());
                    } else if k.starts_with("chapter.") {
                        inputs
                            .chapter
                            .insert(k.trim_start_matches("chapter.").to_string(), v.clone());
                    } else {
                        inputs.extra.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    inputs
}

/// 断言表达式求值：
/// `count("list") == 20` · `"list[0].map.name" == "斗破苍穹"` · `"list[0].map.url" =~ ^…$`
/// `merged[0].sources == 3` · `count(merged) == 12`
pub fn eval_assert(expr: &str, out: &Value) -> Result<bool, String> {
    let e = expr.trim();
    // count(X) OP N
    if let Some(rest) = e.strip_prefix("count(") {
        let (path, op, n) = parse_count(rest)?;
        let target: &Vec<Value> = if path == "merged" {
            out.as_array().ok_or("merged 不是数组")?
        } else {
            get_path(out, &path)?.as_array().ok_or(format!("{path} 不是数组"))?
        };
        let actual = target.len();
        return Ok(match op {
            "==" => actual == n,
            ">=" => actual >= n,
            "<=" => actual <= n,
            ">" => actual > n,
            "<" => actual < n,
            _ => return Err(format!("未知比较符 {op}")),
        });
    }
    // 路径断言 == / != / =~
    let (path, op, rhs) = parse_path_assert(e)?;
    let op: &str = &op;
    let actual = get_path(out, &path)?;
    let rhs_v: Value = serde_json::Value::String(rhs.clone());
    let matched: bool = match op {
        "==" => value_eq(&actual, &rhs_v) || actual.as_str() == Some(rhs.as_str()),
        "!=" => !(value_eq(&actual, &rhs_v) || actual.as_str() == Some(rhs.as_str())),
        "=~" => {
            let s = actual.as_str().map(|x| x.to_string()).unwrap_or_else(|| actual.to_string());
            let re = regex::Regex::new(&rhs).map_err(|e| format!("正则非法: {e}"))?;
            re.is_match(&s)
        }
        "contains" => actual.to_string().contains(&rhs),
        _ => return Err(format!("未知断言运算符 {op}")),
    };
    Ok(matched)
}

fn value_eq(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Value::Number(x), y) => {
            let xf = x.as_f64().unwrap_or(f64::NAN);
            match y.as_f64() {
                Some(yf) => (xf - yf).abs() < 1e-9,
                None => y.as_str().and_then(|s| s.parse::<f64>().ok()).map(|yf| (xf - yf).abs() < 1e-9).unwrap_or(false),
            }
        }
        _ => false,
    }
}

fn parse_count(rest: &str) -> Result<(String, &str, usize), String> {
    let close = rest.rfind(')').ok_or("count 缺右括号")?;
    let path = rest[..close].trim().trim_matches('"').to_string();
    let tail = rest[close + 1..].trim();
    let (op, n) = tail
        .split_once(char::is_whitespace)
        .ok_or("count 断言缺比较")?;
    let n: usize = n.trim().parse().map_err(|_| "count 比较值非数字")?;
    Ok((path, op.trim(), n))
}

fn parse_path_assert(e: &str) -> Result<(String, &str, String), String> {
    // "path" == value | "path" =~ ^re$
    let quoted = e.trim_start().starts_with('"');
    if quoted {
        // 找闭合引号
        let inner = &e[1..];
        let close = inner.rfind('"').ok_or("路径缺右引号")?;
        let path = inner[..close].to_string();
        let rest = inner[close + 1..].trim();
        for op in ["==", "!=", "=~", "contains"] {
            if let Some(v) = rest.strip_prefix(op) {
                let mut val = v.trim().to_string();
                if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2) {
                    val = val[1..val.len() - 1].to_string();
                }
                return Ok((path, op, val));
            }
        }
        return Err(format!("断言缺运算符: {e}"));
    }
    // 裸路径 merged[0].sources == 3
    let parts: Vec<&str> = e.splitn(3, char::is_whitespace).collect();
    if parts.len() < 3 {
        return Err(format!("断言格式: {e}"));
    }
    let (path, op, val) = (parts[0].trim(), parts[1], parts[2].trim());
    let val = val.trim_matches('"').to_string();
    Ok((path.to_string(), op, val))
}

/// JSON 路径：`list[0].map.name` / `merged[0].sources`。
pub fn get_path<'a>(v: &'a Value, path: &str) -> Result<&'a Value, String> {
    let mut cur = v;
    let segs: Vec<&str> = path.split('.').collect();
    for (i, seg) in segs.iter().enumerate() {
        // 处理 name[0] 形式（merged/list 作为根数组时直接取根）
        if let Some(br) = seg.find('[') {
            let name = &seg[..br];
            if !name.is_empty() {
                cur = match cur.get(name) {
                    Some(x) => x,
                    None if (name == "merged" || name == "list") && cur.is_array() && i == 0 => cur,
                    None => return Err(format!("路径缺字段: {path}@{name}")),
                };
            }
            for idx_part in seg[br..].split('[').skip(1) {
                let idx: usize = idx_part
                    .trim_end_matches(']')
                    .parse()
                    .map_err(|_| format!("下标非法: {seg}"))?;
                cur = cur.get(idx).ok_or_else(|| format!("下标越界: {path}@{seg}"))?;
            }
        } else if i == 0 && *seg == "merged" || *seg == "list" {
            cur = cur.get(seg).unwrap_or(cur);
        } else {
            cur = cur.get(seg).ok_or_else(|| format!("路径缺字段: {path}@{seg}"))?;
        }
    }
    Ok(cur)
}

/// YAML 用例解析（serde_yaml → TestCase）。
pub fn parse_cases_yaml(text: &str) -> Result<Vec<TestCase>, String> {
    let yv: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| format!("YAML 解析失败: {e}"))?;
    let v: Value = serde_json::to_value(&yv).map_err(|e| format!("YAML→JSON 转换失败: {e}"))?;
    let mut cases = vec![];
    let arr = v["cases"].as_array().ok_or("cases 必须是列表")?;
    for c in arr {
        let assert_exprs = c["assert"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| match x {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        cases.push(TestCase {
            name: c["name"].as_str().unwrap_or("未命名").to_string(),
            stage: c["stage"].as_str().unwrap_or("search").to_string(),
            level: Level::parse(c["level"].as_str().unwrap_or("L2")),
            vars: c.get("vars").cloned().unwrap_or(Value::Null),
            fixture: c["fixture"].as_str().map(String::from),
            assert_exprs,
            expect_error: c["expect_error"].as_str().map(String::from),
            at: c["at"].as_str().map(String::from),
            merge_glob: c["merge"].as_str().map(String::from),
        });
    }
    Ok(cases)
}
