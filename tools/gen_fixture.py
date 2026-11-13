#!/usr/bin/env python3
"""生成 1200 章目录 fixture（TASK-BOOK §5.3 toc_1200.html）。"""
import io
import sys
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
ROOT = Path(__file__).resolve().parent.parent

items = []
for i in range(1, 1201):
    vol = " 第一卷" if 1 <= i <= 100 else (" 第二卷" if 101 <= i <= 200 else "")
    vip = '<i class="vip">VIP</i>' if i % 7 == 0 else ""
    items.append(f'    <li><a href="/c/{i}">第{i}章{vol}</a>{vip}</li>')
html = (
    '<!doctype html><html><head><title>1200 章</title></head><body>\n'
    '<div id="chapter-list"><ul>\n' + "\n".join(items) + "\n</ul></div>\n"
    "</body></html>"
)
out = ROOT / "fixtures" / "toc_1200.html"
out.write_text(html, encoding="utf-8")
print(f"✓ {out}（{1200} 项，{len(html)} 字节）")
