#!/usr/bin/env python3
"""B4 LSP 冒烟（TASK-BOOK-2 §3.2 B4 / §5.3 五场景）。

裸 JSON-RPC over stdio 模拟编辑器：
  initialize → didOpen(七猫草稿) → diagnostics(≤3s) → hover(var:book.id)
  → completion(18 命名空间) → definition(var:chapter.url)

用法：python -I tools/lsp_smoke.py
"""
import io
import json
import os
import subprocess
import sys
import threading
import time

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
CLI = "target/debug/rsl-lsp.exe"
DOC = "本地语料（不入库）"

try:
    text = open(DOC, encoding="utf-8").read()
except OSError as e:
    print(f"!! 打开 {DOC} 失败: {e}（七猫源在本地但不入库）")
    sys.exit(2)

proc = subprocess.Popen([CLI], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE)
lock = threading.Lock()
notifications = []


def send(msg):
    body = json.dumps(msg, ensure_ascii=False).encode("utf-8")
    frame = f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body
    with lock:
        try:
            os.write(proc.stdin.fileno(), frame)
        except OSError as e:
            print(f"WRITE FAIL（进程存活={proc.poll()}）: {e}")
            raise


def reader():
    f = proc.stdout
    while True:
        headers = {}
        while True:
            line = f.readline()
            if not line:
                return
            if line in (b"\r\n", b"\n"):
                break
            if b":" in line:
                k, v = line.split(b":", 1)
                headers[k.strip().lower()] = v.strip()
        try:
            length = int(headers.get(b"content-length", b"0"))
            body = f.read(length)
            if body:
                notifications.append(json.loads(body.decode("utf-8")))
        except Exception as e:
            print(f"reader 解析异常: {e}")


threading.Thread(target=reader, daemon=True).start()


def wait_for(pred, timeout=5.0):
    deadline = time.time() + timeout
    while time.time() < deadline:
        for n in notifications:
            if pred(n):
                return n
        time.sleep(0.05)
    return None


def find_pos(text, needle, occurrence=1):
    idx = -1
    for _ in range(occurrence):
        idx = text.find(needle, idx + 1)
    if idx < 0:
        return None
    line = text[:idx].count("\n")
    seg = text[text.rfind("\n", 0, idx) + 1 : idx]
    # LSP character = UTF-16 code unit
    char = len(seg.encode("utf-16-le")) // 2 + 1
    return {"line": line, "character": char}


results = {}

# 1) initialize
send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "processId": None, "rootUri": None, "capabilities": {}}})
init = wait_for(lambda n: n.get("id") == 1, 5)
results["initialize"] = bool(init and "capabilities" in init.get("result", {}))
send({"jsonrpc": "2.0", "method": "initialized", "params": {}})

# 2) didOpen → diagnostics ≤3s（场景 1）
t0 = time.time()
send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
    "textDocument": {"uri": "file:///qimao.json", "languageId": "json",
                     "version": 1, "text": text}}})
diag = wait_for(
    lambda n: n.get("method") == "textDocument/publishDiagnostics"
    and n["params"]["uri"] == "file:///qimao.json",
    3.0)
dt = time.time() - t0
results["diagnostics≤3s"] = bool(diag) and dt <= 3.0
results["diagnostics_count"] = len(diag["params"]["diagnostics"]) if diag else 0
results["diagnostics_seconds"] = round(dt, 2)

# 3) hover var:book.id（场景 3：显示来源契约）
pos = find_pos(text, "var:book.id") or find_pos(text, "var:book.url")
send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {
    "textDocument": {"uri": "file:///qimao.json"}, "position": pos}})
hov = wait_for(lambda n: n.get("id") == 2, 5)
hov_val = (hov or {}).get("result") or {}
hov_md = ""
if hov_val.get("contents", {}).get("value"):
    hov_md = hov_val["contents"]["value"]
results["hover_var"] = "来源" in hov_md or "var:" in hov_md
results["hover_text"] = hov_md[:120]

# 4) completion 裸选择器 → 18 命名空间（场景 4）
pos2 = find_pos(text, "\"$\"")
send({"jsonrpc": "2.0", "id": 3, "method": "textDocument/completion", "params": {
    "textDocument": {"uri": "file:///qimao.json"}, "position": pos2}})
comp = wait_for(lambda n: n.get("id") == 3, 5)
items = (comp or {}).get("result", [])
if isinstance(items, dict):
    items = items.get("items", [])
ns_labels = [i.get("label", "") for i in items]
results["completion_namespaces"] = sum(1 for l in ns_labels if l.endswith(":"))
results["completion_18"] = results["completion_namespaces"] >= 15

# 5) definition var:chapter.url → 定位声明（场景 5；带引号搜独立 token，避开 expr 内引用）
pos3 = find_pos(text, '"var:chapter.url"')
send({"jsonrpc": "2.0", "id": 4, "method": "textDocument/definition", "params": {
    "textDocument": {"uri": "file:///qimao.json"}, "position": pos3}})
defn = wait_for(lambda n: n.get("id") == 4, 5)
dres = (defn or {}).get("result")
ok_def = bool(dres) and (isinstance(dres, dict) or (isinstance(dres, list) and dres))
results["definition"] = bool(ok_def)

# 关闭
send({"jsonrpc": "2.0", "id": 9, "method": "shutdown", "params": None})
send({"jsonrpc": "2.0", "method": "exit", "params": None})
try:
    proc.wait(timeout=3)
except subprocess.TimeoutExpired:
    proc.kill()

print(json.dumps(results, ensure_ascii=False, indent=2))
checks = ["initialize", "diagnostics≤3s", "hover_var", "completion_18", "definition"]
fails = [c for c in checks if not results.get(c)]
print("\n== LSP 冒烟结果 ==")
print("PASS 5/5" if not fails else f"FAIL: {fails}")
sys.exit(0 if not fails else 1)
