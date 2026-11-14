//! L2 冒烟测试（T5.1/T5.2 雏形）：FixtureTransport 重放，不联网跑 search/toc/content 全链路。

use rsl_core::exec::ctx::ExecCx;
use rsl_core::exec::value::HtmlDoc;
use rsl_core::ports::{Clock, FixtureTransport, MemoryCookieStore, SystemClock};
use rsl_core::stage::{run_stage, HostCaps, ResponseReuse, RunInputs, StageResult};
use rsl_core::{compile, script};
use std::rc::Rc;

const SEARCH_HTML: &str = r#"<!doctype html><html><head><title>搜索</title></head><body>
<ul class="result">
  <li><a class="title" href="/book/1">斗破苍穹</a><span class="author">天蚕土豆</span><span class="words">530.25万字</span><span class="latest">第一千六百二十三章</span></li>
  <li><a class="title" href="/book/2">凡人修仙传</a><span class="author">忘语</span><span class="words">751.12万字</span><span class="latest">第两千四百五十六章</span></li>
  <li><a class="title" href="/book/3">诡秘之主</a><span class="author">爱潜水的乌贼</span><span class="words">321.4万字</span><span class="latest">第一千四百三十章</span></li>
</ul>
<a class="next-page" href="/search?p=2">下一页</a>
</body></html>"#;

const DETAIL_HTML: &str = r#"<!doctype html><html><body>
<h1 class="book-name">《斗破苍穹》</h1>
<div class="info"><span class="author">作 者：天蚕土豆</span></div>
<div id="intro">这是一部虚构的演示小说。</div>
<div class="tags"><a>玄幻</a><a>热血</a></div>
<a class="btn-read" href="/book/1/toc">开始阅读</a>
<div class="last-chapter"><a>第一千六百二十三章 大结局</a></div>
</body></html>"#;

const TOC_HTML: &str = r#"<!doctype html><html><body>
<div id="chapter-list">
  <ul>
    <li><a href="/c/1">第一章 风起</a></li>
    <li><a href="/c/2">第二章 云涌</a></li>
    <li><a href="/c/3">第三章 雷鸣</a></li>
    <li class="volume"><span class="vol">第一卷</span></li>
    <li><a href="/c/4">第四章 雨落</a><i class="vip">VIP</i></li>
    <li><a href="/c/5">第五章 天明</a></li>
  </ul>
</div>
</body></html>"#;

const CONTENT_HTML: &str = r#"<!doctype html><html><body>
<h1 class="chapter-title">第一章 风起</h1>
<div id="content">第一段正文。&nbsp;缩进测试。<br/>第二段正文。<br/>第三段正文。<br/><span class="ad">广告位</span></div>
<a id="next-page" href="/c/1_2">下一页</a>
</body></html>"#;

const CONTENT_P2_HTML: &str = r#"<!doctype html><html><body><div id="content">续页第四段。<br/>续页第五段。</div></body></html>"#;

fn resp(body: &str, content_type: &str) -> rsl_core::ports::HttpResponse {
    rsl_core::ports::HttpResponse {
        status: 200,
        headers: vec![("Content-Type".into(), content_type.into())],
        body: body.as_bytes().to_vec(),
        final_url: String::new(),
        ms: 1,
    }
}

const SOURCE_JSON: &str = r#"{
  "rsl": "2.0", "id": "fixture.example.com", "name": "夹具站", "type": "novel",
  "baseUrl": "https://fixture.example.com",
  "requires": { "features": ["js"], "crypto": [], "selectorLevel": { "css": "4" } },
  "http": { "headers": { "User-Agent": "rsl-test/1.0" }, "retry": { "times": 0, "backoff": "none" } },
  "lib": { "pure": "function cleanTitle(s){ return String(s||'').replace(/[《》\\[\\]]/g,'').trim(); }", "ctx": "" },
  "stages": {
    "search": {
      "request": { "url": "https://fixture.example.com/search", "params": { "q": "var:kw" } },
      "out": {
        "list": "css:ul.result > li",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }, { "js": "lib:cleanTitle" }] },
          "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] },
          "author": { "$": "css:span.author", "optional": true, "default": "" },
          "wordCount": { "$": "css:span.words", "transform": [{ "number": { "unit": true } }], "optional": true }
        },
        "emptyOk": true
      },
      "paginate": { "kind": "page", "pageVar": "page", "max": 2 }
    },
    "detail": {
      "request": { "url": "var:book.url" },
      "out": {
        "name": { "$": "css:h1.book-name", "transform": [{ "trim": true }, { "js": "lib:cleanTitle" }] },
        "author": { "$": "css:div.info span.author", "transform": [{ "regex": { "regex": "作\\s*者[:：]\\s*(\\S+)", "replace": "$1" } }] },
        "kind": { "$": "css:.tags a", "transform": [{ "join": "," }], "optional": true },
        "tocUrl": { "$": "css:a.btn-read", "attr": "href", "transform": [{ "absolute": true }], "optional": true }
      }
    },
    "toc": {
      "request": { "url": "var:book.tocUrl" },
      "order": "auto",
      "out": {
        "list": "css:div#chapter-list > ul > li",
        "map": {
          "title": { "$": "css:a", "transform": [{ "trim": true }] },
          "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] },
          "isVolume": { "$": "css:.volume", "exists": true },
          "isVip": { "$": "css:i.vip", "exists": true }
        }
      }
    },
    "content": {
      "request": { "url": "var:chapter.url" },
      "out": {
        "title": { "$": "css:h1.chapter-title", "transform": [{ "trim": true }] },
        "content": {
          "$": "css:div#content",
          "strip": ["css:span.ad"],
          "paragraphs": { "split": "br", "minLength": 1 },
          "transform": [{ "replace": [["&nbsp;", " "]] }, { "html": true }]
        },
        "nextPage": { "$": "css:a#next-page", "attr": "href", "transform": [{ "absolute": true }], "optional": true }
      }
    }
  }
}"#;

struct App {
    transport: FixtureTransport,
    cookies: MemoryCookieStore,
}

fn build_app() -> App {
    build_app_from(0)
}

fn build_app_from(start: usize) -> App {
    let transport = FixtureTransport {
        responses: vec![
            resp(SEARCH_HTML, "text/html; charset=utf-8"),
            resp(DETAIL_HTML, "text/html; charset=utf-8"),
            resp(TOC_HTML, "text/html; charset=utf-8"),
            resp(CONTENT_HTML, "text/html; charset=utf-8"),
            resp(CONTENT_P2_HTML, "text/html; charset=utf-8"),
        ],
        idx: std::sync::atomic::AtomicUsize::new(start),
    };
    App {
        transport,
        cookies: MemoryCookieStore::default(),
    }
}

fn fresh_cx<'a>(app: &'a App, src: &'a rsl_core::ir::CompiledSource) -> ExecCx<'a> {
    let mut cx = ExecCx::new(src, &app.transport, &SystemClock);
    cx.cookie_store = Some(&app.cookies);
    cx
}

fn run(app: &App, stage: &str, inputs: RunInputs) -> (StageResult, Vec<rsl_core::exec::ctx::TraceEntry>) {
    let out = compile(SOURCE_JSON);
    let src = out.source.expect("编译通过");
    script::setup_engine(&src);
    let mut cx = fresh_cx(app, &src);
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, stage, &mut cx, &inputs, &mut reuse, &HostCaps::default());
    let trace = cx.trace.borrow().clone();
    let warns = cx.warnings.borrow().clone();
    for w in warns {
        eprintln!("WARN: {} {} {}", w.code, w.message, w.rule.clone().unwrap_or_default());
    }
    (r.expect(&format!("阶段 {stage} 应成功")), trace)
}

#[test]
fn search_full_chain() {
    let app = build_app();
    let (r, trace) = run(
        &app,
        "search",
        RunInputs {
            kw: Some("斗破".into()),
            page: Some(1),
            ..Default::default()
        },
    );
    let list = r.out.get("list").and_then(|v| v.as_array()).expect("list 数组");
    assert_eq!(list.len(), 3, "搜索命中 3 条");
    assert_eq!(list[0]["map"]["name"], "斗破苍穹");
    assert_eq!(list[0]["map"]["url"], "https://fixture.example.com/book/1");
    assert_eq!(list[0]["map"]["author"], "天蚕土豆");
    // 字数："530.25万字" → 5302500
    let wc = list[0]["map"]["wordCount"].as_f64().unwrap_or(0.0);
    assert!((wc - 5_302_500.0).abs() < 1.0, "wordCount={wc}");
    // trace 有记录
    assert!(!trace.is_empty());
    let _ = HtmlDoc::parse_fragment("");
}

#[test]
fn detail_chain() {
    let app = build_app_from(1);
    let mut book = std::collections::BTreeMap::new();
    book.insert("url".to_string(), serde_json::json!("https://fixture.example.com/book/1"));
    book.insert("name".to_string(), serde_json::json!("斗破苍穹"));
    let (r, _) = run(
        &app,
        "detail",
        RunInputs {
            book,
            ..Default::default()
        },
    );
    assert_eq!(r.out["name"], "斗破苍穹");
    assert_eq!(r.out["author"], "天蚕土豆");
    assert_eq!(r.out["kind"], "玄幻,热血");
    assert_eq!(r.out["tocUrl"], "https://fixture.example.com/book/1/toc");
}

#[test]
fn toc_chain() {
    let app = build_app_from(2);
    let mut book = std::collections::BTreeMap::new();
    book.insert("tocUrl".to_string(), serde_json::json!("https://fixture.example.com/book/1/toc"));
    let (r, _) = run(&app, "toc", RunInputs { book, ..Default::default() });
    let list = r.out.get("list").and_then(|v| v.as_array()).expect("toc list");
    assert_eq!(list.len(), 6);
    assert_eq!(list[0]["map"]["title"], "第一章 风起");
    assert_eq!(list[0]["map"]["url"], "https://fixture.example.com/c/1");
    assert_eq!(list[3]["map"]["isVolume"], serde_json::json!(true));
    assert_eq!(list[4]["map"]["isVip"], serde_json::json!(true));
}

#[test]
fn content_chain_with_next_page() {
    let app = build_app_from(3);
    let mut chapter = std::collections::BTreeMap::new();
    chapter.insert("url".to_string(), serde_json::json!("https://fixture.example.com/c/1"));
    let (r, _) = run(&app, "content", RunInputs { chapter, ..Default::default() });
    assert_eq!(r.out["title"], "第一章 风起");
    let paras = r.out.get("content").and_then(|v| v.as_array()).expect("段落");
    // 广告被剥除；&nbsp; 反转义；nextPage 续页拼接 → 5 段
    assert_eq!(paras.len(), 5, "正文段落数（含 nextPage 续页）: {paras:?}");
    assert_eq!(paras[0], "第一段正文。 缩进测试。");
    assert!(!serde_json::to_string(&paras).unwrap().contains("广告位"));
}

// ─────────────── A5 协作式取消（TASK-BOOK-2 §3.1 A5） ───────────────

/// 包装 transport：第 N 次请求完成后置位取消（模拟宿主中途取消）。
struct CancelAfterN {
    inner: FixtureTransport,
    token: rsl_core::ports::CancelToken,
    n: usize,
    hits: std::sync::atomic::AtomicUsize,
}

impl rsl_core::ports::Transport for CancelAfterN {
    fn send(&self, req: &rsl_core::ports::HttpRequest) -> Result<rsl_core::ports::HttpResponse, rsl_core::error::Diagnostic> {
        let r = self.inner.send(req)?;
        if self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 >= self.n {
            self.token.cancel();
        }
        Ok(r)
    }
}

#[test]
fn a5_cancel_mid_nextpage_partial() {
    let token = rsl_core::ports::CancelToken::new();
    let transport = CancelAfterN {
        inner: FixtureTransport {
            responses: vec![
                resp(CONTENT_HTML, "text/html; charset=utf-8"),
                resp(CONTENT_P2_HTML, "text/html; charset=utf-8"),
            ],
            idx: std::sync::atomic::AtomicUsize::new(0),
        },
        token: token.clone(),
        n: 1, // 第 1 页取完即取消 → nextPage 第 2 页不发
        hits: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let out = compile(SOURCE_JSON);
    let src = out.source.expect("编译通过");
    script::setup_engine(&src);
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    cx.cancel_token = Some(token);
    let mut chapter = std::collections::BTreeMap::new();
    chapter.insert("url".to_string(), serde_json::json!("https://fixture.example.com/c/1"));
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "content", &mut cx, &RunInputs { chapter, ..Default::default() }, &mut reuse, &HostCaps::default());
    // 部分结果：Ok(partial)
    let stage = r.expect("取消应返回部分结果而非 Err");
    assert!(stage.partial, "应标记 partial");
    let paras = stage.out.get("content").and_then(|v| v.as_array()).expect("段落");
    assert_eq!(paras.len(), 3, "只有第 1 页 3 段（未拼续页）: {paras:?}");
    assert!(paras[0].as_str().unwrap_or_default().contains("第一段正文"));
    // E_CANCELLED 诊断推送
    let warns = cx.warnings.borrow();
    assert!(
        warns.iter().any(|d| d.code.as_str() == "E_CANCELLED"),
        "应有 E_CANCELLED 诊断: {:?}",
        warns.iter().map(|d| d.code.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn a5_cancel_before_request_errors() {
    let token = rsl_core::ports::CancelToken::new();
    token.cancel(); // 请求前已取消
    let transport = CancelAfterN {
        inner: FixtureTransport {
            responses: vec![resp(SEARCH_HTML, "text/html; charset=utf-8")],
            idx: std::sync::atomic::AtomicUsize::new(0),
        },
        token: token.clone(),
        n: usize::MAX,
        hits: std::sync::atomic::AtomicUsize::new(0),
    };
    let cookies = MemoryCookieStore::default();
    let out = compile(SOURCE_JSON);
    let src = out.source.expect("编译通过");
    script::setup_engine(&src);
    let mut cx = ExecCx::new(&src, &transport, &SystemClock);
    cx.cookie_store = Some(&cookies);
    cx.cancel_token = Some(token);
    let mut reuse = ResponseReuse::default();
    let r = run_stage(&src, "search", &mut cx, &RunInputs { kw: Some("斗".into()), ..Default::default() }, &mut reuse, &HostCaps::default());
    // 两种合法形态：Err(E_CANCELLED)（请求边界）或 Ok(partial 空列表)（页边界先行）
    match r {
        Err(d) => assert_eq!(d.code.as_str(), "E_CANCELLED"),
        Ok(s) => assert!(s.partial, "页边界取消应标 partial"),
    }
}
