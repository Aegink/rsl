#!/usr/bin/env python3
"""生成 100 项列表 fixture（budget_truncate.html，配小 maxNodes 触发截断警告）。"""
import io
import sys
from pathlib import Path

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
ROOT = Path(__file__).resolve().parent.parent

items = [f'  <li><a class="t" href="/b/{i}">条目 {i}</a></li>' for i in range(1, 101)]
html = (
    '<!doctype html><html><body>\n<ul class="list">\n' + "\n".join(items) + "\n</ul>\n</body></html>\n"
)
out = ROOT / "fixtures" / "budget_truncate.html"
out.write_text(html, encoding="utf-8")
print(f"✓ {out}（100 项）")
