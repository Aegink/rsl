#!/usr/bin/env python3
"""语料裸域名补充消毒（V2-D1 完整化）：id/x-legado 等字段的裸域名 → *.example.com。
同步更新 tools/gen_corpus.py 的 sanitize（可重现）。"""
import io
import json
import pathlib
import re
import sys

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "fuzz" / "corpus"

TLD = r"(?:com|net|cc|org|io|me|top|xyz|cn|vip|site|online|app|info|club|tv|art|pro|fun|icu|wiki|life|mobi|biz|store|tech|space|website|link|zone|red|bar|cool|city|ltd|group|agency|win|pics|photo|press|rocks|systems|network|center|services|support|solutions|company|academy|wiki|fan|film|fish|soft|gift|pink|sexy|loan|date|institute|legal|market|partners|properties|rehab|supplies|supply|ventures|wtf|name|gov|edu)"
DOMAIN_RE = re.compile(r"(?<![\w.@\-])((?:[a-z0-9][a-z0-9\-]*\.)+" + TLD + r")(?![\w\-])", re.IGNORECASE)
ALLOW = {"example.com", "neg.example.com", "prop.example.com", "rt.example.com", "fixture.example.com", "css4.example.com", "edge.example.com"}


def scrub_domain(m: re.Match) -> str:
    host = m.group(1).lower()
    if host in ALLOW or host.endswith(".example.com"):
        return m.group(1)
    # 保留子域结构，域名部分 → example.com（保持语料结构形态）
    parts = host.split(".")
    parts[-2:] = ["example", "com"]
    return ".".join(parts)


def sanitize_text(s: str) -> str:
    return DOMAIN_RE.sub(scrub_domain, s)


def sanitize_value(v):
    if isinstance(v, dict):
        return {k: sanitize_value(val) for k, val in v.items()}
    if isinstance(v, list):
        return [sanitize_value(x) for x in v]
    if isinstance(v, str):
        return sanitize_text(v)
    return v


def main():
    n_files = n_hosts = 0
    for f in sorted(CORPUS.glob("*.json")):
        if f.name.startswith("edge_13"):
            continue  # 孤立代理畸形测试文件，不重写
        raw = f.read_text(encoding="utf-8", errors="replace")
        # 直接文本级替换（保留 JSON 结构；域名替换不产生引号/转义变化）
        cleaned = sanitize_text(raw)
        # 语义层再过一遍（id 等字段值已被文本级覆盖；本遍兜底键值形态）
        try:
            doc = json.loads(cleaned)
            cleaned2 = json.dumps(sanitize_value(doc), ensure_ascii=False)
            # 文本级与语义级结果不一致时以语义级为准（键序由原文件 dict 顺序保持）
            cleaned = cleaned2
        except Exception:
            pass
        if cleaned != raw:
            f.write_text(cleaned, encoding="utf-8")
            n_files += 1
    # 验证
    pat = re.compile(r"(?<![\w.@\-])((?:[a-z0-9][a-z0-9\-]*\.)+" + TLD + r")(?![\w\-])", re.IGNORECASE)
    left = {}
    for f in CORPUS.glob("*.json"):
        for m in pat.findall(f.read_text(encoding="utf-8", errors="replace")):
            if m.lower() not in ALLOW and not m.lower().endswith(".example.com"):
                left[m] = left.get(m, 0) + 1
    print(f"改写文件: {n_files}; 残留非白名单域名: {left if left else '无'}")
    return 0 if not left else 1


if __name__ == "__main__":
    sys.exit(main())
