#!/usr/bin/env python3
"""生成 GBK 编码的搜索页 fixture（TASK-BOOK §5.3 gbk.html，真实 GBK 字节）。"""
import io
import sys
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
ROOT = Path(__file__).resolve().parent.parent

HTML = (
    '<!doctype html><html><head><meta charset="gbk"><title>GBK 站</title></head><body>\n'
    '<ul class="result">\n'
    '  <li><a class="title" href="/book/1">射雕英雄传</a><span class="author">金庸</span></li>\n'
    '  <li><a class="title" href="/book/2">天龙八部</a><span class="author">金庸</span></li>\n'
    '</ul>\n'
    '</body></html>\n'
)
out = ROOT / "fixtures" / "gbk.html"
out.write_bytes(HTML.encode("gbk"))
print(f"✓ {out}（GBK 字节 {out.stat().st_size}）")
