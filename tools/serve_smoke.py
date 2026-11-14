"""rsl serve / web 调试前端冒烟（TASK-BOOK-4 T2/T3 验收，零第三方依赖）：

    python -I tools/serve_smoke.py

覆盖：
1. 服务生命周期：随机端口启动 → stdout 解析 → 回收；
2. 端点：GET /health、GET /debug/schema、GET /（前端）、静态文件、404、400；
3. POST /debug/run：源文本随请求携带（fixture 离线重放）→ rsl-debug/1 四数据面断言；
   explore 链式输入（group/filter）端到端；编译失败诊断直通（ok=false）；
4. 安全：非回环绑定拒绝（exit 2）；token 缺失 401 / 命中 200；路径穿越拒绝。
"""
import json
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass

ROOT = Path(__file__).resolve().parent.parent
FAILURES = []


def check(name, cond, detail=""):
    if cond:
        print(f"  ok  {name}")
    else:
        FAILURES.append(name)
        print(f"FAIL  {name}  {detail}")


def find_binary():
    exe = "rsl.exe" if sys.platform == "win32" else "rsl"
    for profile in ("debug", "release"):
        p = ROOT / f"target/{profile}/{exe}"
        if p.exists():
            return p
    return None


def http(method, url, body=None, token=None, headers=None):
    req_headers = dict(headers or {})
    if token:
        req_headers["Authorization"] = f"Bearer {token}"
    data = None
    if body is not None:
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        req_headers.setdefault("Content-Type", "application/json")
    req = urllib.request.Request(url, data=data, headers=req_headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()
    except Exception as e:  # noqa: BLE001
        return 0, str(e).encode()


def start_serve(args):
    exe = find_binary()
    proc = subprocess.Popen(
        [str(exe), "serve", *args],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    line = proc.stdout.readline().strip()
    m = re.search(r"listening=127\.0\.0\.1:(\d+)", line)
    if not m:
        proc.kill()
        raise RuntimeError(f"serve 未输出监听行：{line!r}")
    return proc, f"http://127.0.0.1:{m.group(1)}"


def stop(proc):
    proc.kill()
    try:
        proc.wait(timeout=5)
    except Exception:  # noqa: BLE001
        pass


def main():
    exe = find_binary()
    if exe is None:
        print("✗ 未找到 rsl 二进制（target/debug|rsl.exe）；先 cargo build -p rsl-cli")
        return 1

    fixture_html = (ROOT / "examples/fixtures/detail.fixture.html").read_text(encoding="utf-8")
    source_text = (ROOT / "examples/novel-html.rsl.json").read_text(encoding="utf-8")
    run_body = {
        "source_text": source_text,
        "stage": "detail",
        "label": "smoke",
        "inputs": {"book": {"url": "https://example.com/book/1", "name": "斗破苍穹"}},
        "fixture": {"status": 200, "headers": {"Content-Type": "text/html; charset=utf-8"}, "body": fixture_html},
    }

    # ── 1) 无 token 服务（默认随机端口）+ 前端静态 ──
    proc, base = start_serve(["--web", "editors/web-debug"])
    try:
        st, body = http("GET", f"{base}/health")
        d = json.loads(body)
        check("health ok", st == 200 and d.get("ok") is True, f"{st} {body[:80]}")

        st, body = http("GET", f"{base}/debug/schema")
        check("debug/schema 提供 rsl-debug/1", st == 200 and b"rsl-debug/1" in body)

        st, body = http("GET", f"{base}/")
        check("前端 index.html 加载", st == 200 and "调试器" in body.decode("utf-8", "replace"))

        for f in ("app.js", "style.css"):
            st, body = http("GET", f"{base}/{f}")
            check(f"静态 {f}", st == 200 and len(body) > 500)

        st, _ = http("GET", f"{base}/no-such.js")
        check("未知路径 404", st == 404, str(st))
        st, _ = http("GET", f"{base}/../schema/rsl.schema.json")
        check("路径穿越拒绝", st in (403, 404), str(st))
        st, _ = http("POST", f"{base}/debug/run", body=b"not-json")
        check("坏 JSON 400", st == 400, str(st))
        st, _ = http("POST", f"{base}/health", body=b"{}")
        check("方法不匹配 405/404", st in (404, 405), str(st))

        # ── 2) POST /debug/run：四数据面 ──
        st, body = http("POST", f"{base}/debug/run", body=run_body)
        d = json.loads(body)
        check("run HTTP 200", st == 200, str(st))
        check("产物 schema=rsl-debug/1", d.get("schema") == "rsl-debug/1")
        check("ok=true", d.get("ok") is True)
        # 数据面 1：请求侧（协议标头）
        req = d.get("request") or {}
        check("请求侧 template（原始地址）", req.get("template") == "${var:book.url}", str(req))
        check("请求侧 url/请求地址", req.get("url") == "https://example.com/book/1")
        check("请求侧 method/engine/parse/attempts",
              req.get("method") == "GET" and req.get("engine") == "http"
              and req.get("parse") == "html" and req.get("attempts") == 1)
        check("请求侧 headers 含 UA", any(h[0] == "User-Agent" for h in req.get("headers", [])))
        # 数据面 2：响应侧（原始数据）
        resp = d.get("response") or {}
        check("响应侧 status/sha256/preview",
              resp.get("status") == 200 and len(resp.get("sha256", "")) == 64
              and "斗破苍穹" in (resp.get("preview") or ""))
        check("响应侧 truncated=false binary=false", resp.get("truncated") is False and resp.get("binary") is False)
        # 数据面 3：解析结果
        out = d.get("out") or {}
        check("解析结果 out.name", out.get("name") == "斗破苍穹", str(out)[:100])
        check("解析结果 tocUrl（链式下一步）", out.get("tocUrl") == "https://example.com/book/1/toc")
        # 数据面 4：诊断/trace
        check("diagnostics/trace/requests 结构齐",
              isinstance(d.get("diagnostics"), list) and isinstance(d.get("trace"), list)
              and len(d.get("requests") or []) == 1)
        check("inputs 回显（book 注入）", (d.get("inputs") or {}).get("book", {}).get("name") == "斗破苍穹")

        # explore 链式输入端到端（group/filter → var:group/var:filter）
        explore_body = dict(run_body)
        explore_body["stage"] = "explore"
        explore_body["inputs"] = {"group": {"title": "玄幻", "params": {"cat": "xuanhuan"}}, "filter": {"order": "new"}}
        st, body = http("POST", f"{base}/debug/run", body=explore_body)
        d2 = json.loads(body)
        check("explore 运行成功（emptyOk）", st == 200 and d2.get("ok") is True)
        ei = d2.get("inputs") or {}
        check("explore inputs 回显 group/filter",
              ei.get("group", {}).get("title") == "玄幻" and ei.get("filter", {}).get("order") == "new")

        # 编译失败 → 诊断直通（ok=false + error）
        bad_src = dict(run_body)
        bad_src["source_text"] = '{"rsl": "2.0"'
        st, body = http("POST", f"{base}/debug/run", body=bad_src)
        d3 = json.loads(body)
        check("编译失败诊断直通", st == 200 and d3.get("ok") is False and (d3.get("error") or {}).get("code"))
    finally:
        stop(proc)

    # ── 3) token 服务（V4-D11） ──
    proc, base = start_serve(["--token", "secret-x1", "--web", "editors/web-debug"])
    try:
        st, _ = http("GET", f"{base}/health")
        check("无 token → 401", st == 401, str(st))
        st, _ = http("GET", f"{base}/health", token="wrong")
        check("错 token → 401", st == 401, str(st))
        st, body = http("GET", f"{base}/health", token="secret-x1")
        check("对 token → 200", st == 200, str(st))
        st, body = http("POST", f"{base}/debug/run", body=run_body, token="secret-x1")
        check("token 模式 run 正常", st == 200 and json.loads(body).get("ok") is True)
        # 静态壳豁免 token（iframe 子资源无法带 query token；壳内无用户数据）
        st, body = http("GET", f"{base}/")
        check("token 模式静态壳豁免", st == 200 and "调试器" in body.decode("utf-8", "replace"), str(st))
        st, body = http("GET", f"{base}/app.js")
        check("token 模式 app.js 豁免", st == 200 and len(body) > 500, str(st))
        st, body = http("GET", f"{base}/debug/schema")
        check("token 模式公开 schema 豁免", st == 200 and b"rsl-debug/1" in body, str(st))
        st, _ = http("POST", f"{base}/debug/run", body=run_body)
        check("数据面 run 仍强制 token", st == 401, str(st))
    finally:
        stop(proc)

    # ── 4) 非回环绑定拒绝（V4-D11） ──
    r = subprocess.run([str(exe), "serve", "--bind", "0.0.0.0"], cwd=ROOT,
                       capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=15)
    check("非回环绑定拒绝 exit=2", r.returncode == 2 and "拒绝绑定" in (r.stderr or ""), f"exit={r.returncode}")

    print()
    if FAILURES:
        print(f"结果：{len(FAILURES)} 处失败：{'、'.join(FAILURES)}")
        return 1
    print("结果：serve/前端冒烟全部通过")
    return 0


if __name__ == "__main__":
    sys.exit(main())
