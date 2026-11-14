#!/usr/bin/env python3
"""A2 种子语料库生成（TASK-BOOK-2 §3.1 A2 / V2-D1）。

产出 fuzz/corpus/：
- neg_XX.json   15 编译期负例（复刻 crates/rsl-core/tests/m1_compile.rs）+ 版本负例
- pos_XX.json   examples/ 全部示例 + 七猫真实源
- case_NNN.json scratch/sources_raw 真实 Legado 源 → rsl convert → URL 消毒
- edge_NN.json  边界构造（空对象/深嵌套/超长串/坏版本/重复键…）

消毒规则（V2-D1 合规字段子集）：所有 http(s) URL 的 host → example.com（路径保留）；
headers 中 cookie/token/authorization 值 → 删除。站点信息不出 scratch 边界。

用法：python -I tools/gen_corpus.py [--limit 600]
"""
import io
import json
import re
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "fuzz" / "corpus"
RAW = ROOT / "scratch" / "sources_raw"
CLI = ROOT / "target" / "debug" / "rsl.exe"

# ───────────────────────── 骨架（与 m1_compile.rs BASE 一致） ─────────────────────────
BASE = r'''{
  "rsl": "2.0", "id": "neg.example.com", "name": "负例站", "type": "novel",
  "baseUrl": "https://neg.example.com",
  "requires": { "features": ["js"], "crypto": [] },
  "lib": { "pure": "function cleanTitle(s){ return String(s||'').trim(); }", "ctx": "" },
  "stages": {
    "search": {
      "request": { "url": "https://neg.example.com/search", "params": { "q": "var:kw" } },
      "out": { "list": "css:ul.result > li", "map": {
        "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
        "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
      }, "emptyOk": true }
    },
    "detail": {
      "request": { "url": "var:book.url" },
      "out": { "name": "css:h1", "tocUrl": { "$": "css:a.read", "attr": "href", "transform": [{ "absolute": true }], "optional": true } }
    },
    "toc": {
      "request": { "url": "var:book.tocUrl" },
      "out": { "list": "css:#list li", "map": {
        "title": { "$": "css:a", "transform": [{ "trim": true }] },
        "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] }
      } }
    },
    "content": {
      "request": { "url": "var:chapter.url" },
      "out": { "content": { "$": "css:div#content", "paragraphs": { "split": "br" } } }
    }
  }
}'''


def patch_json(fn):
    v = json.loads(BASE)
    fn(v)
    return json.dumps(v, ensure_ascii=False)


def patch_txt(src, frm, to):
    assert frm in src, f"patch 源不存在: {frm}"
    return src.replace(frm, to)


LOGIN_N09 = r'''"login": {
  "type": "form",
  "steps": [
    { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
      "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } },
      "next": { "to": "sms" } },
    { "id": "sms", "fields": [{ "key": "code", "label": "短信码", "secret": true }],
      "submit": { "request": { "url": "https://x/api/verify", "method": "POST", "body": { "type": "json", "data": { "code": "var:login.code" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } },
      "next": { "to": "cred" } }
  ]
},'''

LOGIN_N10 = r'''"login": {
  "type": "form",
  "steps": [
    { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
      "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } },
      "next": { "to": "snp" } }
  ]
},'''

LOGIN_N11 = r'''"login": {
  "type": "form",
  "steps": [
    { "id": "cred", "fields": [{ "key": "username", "label": "账号" }],
      "submit": { "request": { "url": "https://x/api/login", "method": "POST", "body": { "type": "json", "data": { "u": "var:login.username" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } } },
    { "id": "cred", "fields": [{ "key": "code", "label": "码", "secret": true }],
      "submit": { "request": { "url": "https://x/api/verify", "method": "POST", "body": { "type": "json", "data": { "code": "var:login.code" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } } }
  ]
},'''

NEGATIVES = {}


def build_negatives():
    n = NEGATIVES
    # 01 选择器缺命名空间 → E_SELECTOR
    n["neg_01"] = patch_json(lambda v: v["stages"]["search"]["out"]["map"].__setitem__("name", "li.title"))
    # 02 列表上 regex → E_TYPE
    n["neg_02"] = patch_json(lambda v: v["stages"]["content"]["out"]["content"].__setitem__(
        "transform", [{"split": "\n"}, {"regex": {"regex": "x"}}]))
    # 03 book.urll 引用笔误 → E_REF
    n["neg_03"] = patch_txt(BASE, "var:book.tocUrl", "var:book.urll")
    # 04 正则反向引用 → E_REGEX_UNSUPPORTED
    n["neg_04"] = patch_txt(BASE, '"css:a.title"', '"re:(a)' + chr(92) + chr(92) + '1"')
    # 05 per-item 调 ctx 函数 → E_COST
    def _n05(v):
        v["lib"]["ctx"] = "function ctxFetch(ctx, s){ return ctx.http.get('https://x'); }"
        v["stages"]["toc"]["out"]["map"]["updateTime"] = {
            "$": "text",
            "transform": [{"js": {"fn": "lib:ctxFetch", "ctx": True}}],
            "optional": True,
        }
    n["neg_05"] = patch_json(_n05)
    # 06 md5 未声明 crypto → E_UNSUPPORTED
    n["neg_06"] = patch_json(lambda v: v["stages"]["search"]["request"]["params"].__setitem__(
        "sign", "expr:md5(var:kw)"))
    # 07 选择器调 ctx 函数 → E_SCRIPT
    def _n07(v):
        v["lib"]["ctx"] = "function fetcher(ctx){ return 1; }"
        v["stages"]["search"]["out"]["map"]["name"]["$"] = "lib:fetcher"
        v["stages"]["search"]["out"]["map"]["name"]["transform"] = None
    n["neg_07"] = patch_json(_n07)
    # 08 stages 缺 toc → E_SCHEMA
    n["neg_08"] = patch_txt(
        BASE,
        '    "toc": {',
        '    "tocs": {"request": {"url":"var:book.url"}, "out": {"list": "css:li", "map": {"title":"css:a","url":{"$":"css:a","attr":"href"}}},')
    # 09 login 步骤环 → E_SCHEMA
    n["neg_09"] = patch_txt(BASE, '"stages":', LOGIN_N09 + '\n  "stages":')
    # 10 next.to 指向缺失 id → E_REF
    n["neg_10"] = patch_txt(BASE, '"stages":', LOGIN_N10 + '\n  "stages":')
    # 11 步骤 id 重复 → E_SCHEMA
    n["neg_11"] = patch_txt(BASE, '"stages":', LOGIN_N11 + '\n  "stages":')
    # 12 content direct 与 request 并存 → E_SCHEMA
    n["neg_12"] = patch_json(lambda v: v["stages"]["content"].__setitem__("direct", True))
    # 13 enumerate into 字面槽位 → E_SCHEMA
    n["neg_13"] = patch_json(lambda v: v["stages"]["search"].__setitem__(
        "paginate", {"kind": "enumerate", "enumerate": [1, 2], "into": "params.q", "pageVar": "page"}))
    # 14 onResponse 调 ctx.http → E_SCRIPT
    t = patch_txt(BASE,
                  '"request": { "url": "var:chapter.url" },',
                  '"request": { "url": "var:chapter.url" },\n      "script": { "onResponse": { "fn": "lib:fixBody", "ctx": true } },')
    n["neg_14"] = patch_txt(t, '"ctx": ""',
                            '"ctx": "function fixBody(ctx, body){ return ctx.http.get(\'https://x\'); }"')
    # 15 merge.key 引用不存在字段 → E_REF
    n["neg_15"] = patch_txt(
        BASE,
        '"requires": { "features": ["js"], "crypto": [] },',
        '"requires": { "features": ["js"], "crypto": [] },\n  "merge": { "key": ["name", "authro"] },')
    # 版本负例（加载期）
    n["neg_16_version"] = patch_txt(BASE, '"rsl": "2.0"', '"rsl": "3.0"')


# ───────────────────────── 消毒（V2-D1） ─────────────────────────
URL_RE = re.compile(r"https?://[^/\"'\s\\]+", re.IGNORECASE)
TLD = r"(?:com|net|cc|org|io|me|top|xyz|cn|vip|site|online|app|info|club|tv|art|pro|fun|icu|wiki|life|mobi|biz|store|tech|space|website|link|zone|red|bar|cool|city|ltd|group|agency|win|pics|photo|press|rocks|systems|network|center|services|support|solutions|company|academy|fan|film|fish|soft|gift|pink|loan|date|name)"
DOMAIN_RE = re.compile(
    r"(?<![\w.@\-])((?:[a-z0-9][a-z0-9\-]*\.)+" + TLD + r")(?![\w\-])", re.IGNORECASE
)
SENSITIVE_HEADER_KEYS = {"cookie", "authorization", "token", "x-token", "api-key", "apikey", "secret"}


def scrub_domain(m):
    """裸域名 → *.example.com（保留子域结构；punycode 一并处理）。"""
    host = m.group(1).lower()
    if host.endswith("example.com"):
        return m.group(1)
    parts = host.split(".")
    parts[-2:] = ["example", "com"]
    return ".".join(parts)


def sanitize(v):
    """递归消毒：URL host → example.com；敏感头删值。返回 (obj, n_url, n_sensitive)。"""
    n_url = n_sen = 0
    if isinstance(v, dict):
        out = {}
        for k, val in v.items():
            if isinstance(k, str) and k.lower() in SENSITIVE_HEADER_KEYS and isinstance(val, str):
                out[k] = "***"
                n_sen += 1
                continue
            o2, a, b = sanitize(val)
            out[k] = o2
            n_url += a
            n_sen += b
        return out, n_url, n_sen
    if isinstance(v, list):
        out2 = []
        for x in v:
            o2, a, b = sanitize(x)
            out2.append(o2)
            n_url += a
            n_sen += b
        return out2, n_url, n_sen
    if isinstance(v, str):
        def _rep(m):
            nonlocal n_url
            n_url += 1
            return "https://example.com"
        v = DOMAIN_RE.sub(scrub_domain, v)  # 裸域名（id/x-legado 等）
        return URL_RE.sub(_rep, v), n_url, n_sen
    return v, n_url, n_sen


# ───────────────────────── 真实源转换 ─────────────────────────
def convert_one(f: Path):
    out_dir = ROOT / "fuzz" / "_conv_tmp" / f.stem
    try:
        r = subprocess.run([str(CLI), "convert", str(f), "--out", str(out_dir)],
                           capture_output=True, timeout=60, cwd=str(ROOT))
        if r.returncode != 0:
            return f, 0
        drafts = list(out_dir.glob("*.rsl.json"))
        return f, len(drafts)
    except Exception:
        return f, 0


def lint_one(d: Path):
    try:
        r = subprocess.run([str(CLI), "lint", str(d)],
                           capture_output=True, timeout=60, cwd=str(ROOT))
        return r.returncode == 0
    except Exception:
        return False


def collect_real(limit: int):
    files = sorted(RAW.glob("*.json"))
    if limit:
        files = files[:limit]
    work = ROOT / "fuzz" / "_conv_tmp"
    work.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=8) as ex:
        futs = [ex.submit(convert_one, f) for f in files]
        done = 0
        for fut in as_completed(futs):
            fut.result()
            done += 1
            if done % 500 == 0:
                print(f"  convert {done}/{len(files)}", flush=True)
    # 第二阶段：lint 过滤（合法段 case_*；带错误但可解析段 case_bad_*，fuzz 变异种子）
    drafts = sorted(work.rglob("*.rsl.json"))
    print(f"  草稿 {len(drafts)}，开始 lint 过滤", flush=True)
    idx_ok = idx_bad = 0
    with ThreadPoolExecutor(max_workers=8) as ex:
        results = list(ex.map(lint_one, drafts))
    for d, ok in zip(drafts, results):
        try:
            doc = json.loads(d.read_text(encoding="utf-8"))
        except Exception:
            continue
        clean, _, _ = sanitize(doc)
        body = json.dumps(clean, ensure_ascii=False)
        if ok:
            CORPUS.joinpath(f"case_{idx_ok:04d}.json").write_text(body, encoding="utf-8")
            idx_ok += 1
        else:
            # 带错误种子抽 ~1/9 入库（保持 ~500，控制语料体积）
            if idx_bad % 9 == 0:
                CORPUS.joinpath(f"case_bad_{idx_bad:04d}.json").write_text(body, encoding="utf-8")
            idx_bad += 1
    shutil.rmtree(work, ignore_errors=True)
    return idx_ok, idx_bad


# ───────────────────────── 边界构造 ─────────────────────────
def build_edges():
    bs = chr(92)
    edges = {
        "edge_01_empty_obj": "{}",
        "edge_02_empty_arr": "[]",
        "edge_03_null": "null",
        "edge_04_no_version": '{"id":"x","name":"y"}',
        "edge_05_bad_version": '{"rsl":"abc","id":"x","name":"y"}',
        "edge_06_version_float": '{"rsl":2.0,"id":"x","name":"y"}',
        "edge_07_deep_nest": '{"rsl":"2.0","id":"x","name":"y","stages":' + '{"a":' * 40 + "{}" + "}" * 40 + "}",
        "edge_08_long_str": json.dumps({"rsl": "2.0", "id": "x", "name": "y" * 20000}),
        "edge_09_dup_keys": '{"rsl":"2.0","rsl":"3.0","id":"x","name":"y"}',
        "edge_10_bom": chr(0xFEFF) + BASE,
        "edge_11_trailing": BASE + "trailing-garbage",
        "edge_12_long_selector": patch_json(lambda v: v["stages"]["search"]["out"].__setitem__(
            "list", "css:" + "a>" * 400)),
        "edge_13_lone_surrogate": '{"rsl":"2.0","id":"x","name":"' + bs + "ud800" + '孤立代理"}',
        "edge_14_dup_stage_names": patch_json(lambda v: v["stages"].__setitem__("toc2", dict(v["stages"]["toc"]))),
        "edge_15_huge_regex": patch_json(lambda v: v["stages"]["content"]["out"]["content"].__setitem__(
            "$", "re:" + "(a|b)" * 200)),
    }
    return edges


def main():
    limit = 0  # 0 = 全量
    if "--limit" in sys.argv:
        limit = int(sys.argv[sys.argv.index("--limit") + 1])
    if CORPUS.exists():
        shutil.rmtree(CORPUS)
    CORPUS.mkdir(parents=True)
    build_negatives()
    for name, text in NEGATIVES.items():
        (CORPUS / f"{name}.json").write_text(text, encoding="utf-8")
    print(f"负例: {len(NEGATIVES)}")
    pos = 0
    for f in sorted((ROOT / "examples").glob("*.rsl.json")):
        if f.exists():
            shutil.copy(f, CORPUS / f"pos_{f.name}")
            pos += 1
    # 七猫真实源：消毒后入库（S§17 / V2-D1 —— converted/ 不入库，此处为合规子集）
    qm = ROOT / "本地语料（不入库）"
    if qm.exists():
        doc = json.loads(qm.read_text(encoding="utf-8"))
        clean, _, _ = sanitize(doc)
        (CORPUS / "pos_qimao-rsl.json").write_text(
            json.dumps(clean, ensure_ascii=False), encoding="utf-8")
        pos += 1
    print(f"正例: {pos}")
    n_ok, n_bad = collect_real(limit)
    print(f"真实源语料: 合法 {n_ok} / 带错误种子 {n_bad}")
    edges = build_edges()
    for name, text in edges.items():
        (CORPUS / f"{name}.json").write_text(text, encoding="utf-8")
    print(f"边界: {len(edges)}")
    total = len(list(CORPUS.glob("*.json")))
    print(f"语料合计: {total}  （目录 fuzz/corpus/）")
    if n_ok < 300:
        print(f"!! 合法语料 {n_ok} < 300")
        raise SystemExit(2)


if __name__ == "__main__":
    main()
