#!/usr/bin/env python3
"""FFI 链接冒烟（T6.1）：ctypes 加载 rsl_ffi.dll，验证 C ABI 全链路。

判据：
1. rsl_version 返回 rsl-core x.y.z
2. rsl_compile 合法源 → ok=true；非法源 → ok=false 且 diagnostics 带 E_* 码
3. rsl_engine_new + rsl_engine_run_stage(fixture 传输) → search 输出 3 条
4. rsl_last_error 在失败时返回 code|path|message
"""
import ctypes
import json
import sys
import io as _io
from pathlib import Path

sys.stdout = _io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent
DLL = ROOT / "target" / "release" / "rsl_ffi.dll"

lib = ctypes.CDLL(str(DLL))

lib.rsl_version.restype = ctypes.c_void_p
lib.rsl_compile.restype = ctypes.c_void_p
lib.rsl_compile.argtypes = [ctypes.c_char_p]
lib.rsl_last_error.restype = ctypes.c_void_p
lib.rsl_engine_new.restype = ctypes.c_void_p
lib.rsl_engine_new.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
lib.rsl_engine_run_stage.restype = ctypes.c_void_p
lib.rsl_engine_run_stage.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_char_p]
lib.rsl_engine_free.argtypes = [ctypes.c_void_p]
lib.rsl_string_free.argtypes = [ctypes.c_void_p]

fails = []


def check(name, ok, detail=""):
    print(f"{'✓' if ok else '✗'} {name}{(' — ' + detail) if detail else ''}")
    if not ok:
        fails.append(name)


def take_str(ptr):
    """void_p → bytes 并释放（T6.1 句柄模型：调用方负责 rsl_string_free）。"""
    if not ptr:
        return None
    s = ctypes.cast(ptr, ctypes.c_char_p).value
    lib.rsl_string_free(ptr)
    return s.decode("utf-8") if s else None


def call_compile(text: str) -> dict:
    ptr = lib.rsl_compile(text.encode("utf-8"))
    return json.loads(take_str(ptr))


# ① 版本
ver = take_str(lib.rsl_version())
check("rsl_version", ver.startswith("rsl-core"), ver)

# ② 编译校验
source = (ROOT / "examples" / "novel-html.rsl.json").read_text(encoding="utf-8")
good = json.loads(take_str(lib.rsl_compile(source.encode("utf-8"))))
check("rsl_compile 合法源 ok=true", good["ok"] is True)
bad = json.loads(take_str(lib.rsl_compile(b'{"rsl":"2.0","id":"x"}')))
check(
    "rsl_compile 非法源 ok=false + E_SCHEMA",
    bad["ok"] is False and any(d["code"] == "E_SCHEMA" for d in bad["diagnostics"]),
)

# ③ 引擎 + fixture 传输 + run_stage
import base64

search_html = (ROOT / "fixtures" / "search.html").read_text(encoding="utf-8")
transport = {
    "responses": [
        {
            "url": "https://example.com/search?q=斗破",
            "status": 200,
            "content_type": "text/html; charset=utf-8",
            "body_base64": base64.b64encode(search_html.encode("utf-8")).decode("ascii"),
        }
    ]
}
transport_json = json.dumps(transport)

handle = lib.rsl_engine_new(
    (ROOT / "tests" / "cases" / "novel-source.rsl.json").read_text(encoding="utf-8").encode("utf-8"),
    transport_json.encode("utf-8"),
)
check("rsl_engine_new 创建成功", handle is not None and handle != 0)

if handle:
    inputs = json.dumps({"kw": "斗破", "page": 1})
    raw = take_str(lib.rsl_engine_run_stage(handle, b"search", inputs.encode("utf-8")))
    if raw:
        out = json.loads(raw)
        items = out.get("list", [])
        check("run_stage search 3 条", len(items) == 3, f"实际 {len(items)}")
        check(
            "list[0].map.name == 斗破苍穹",
            items and items[0]["map"]["name"] == "斗破苍穹",
        )
    else:
        check("run_stage search 3 条", False, take_str(lib.rsl_last_error()))
    lib.rsl_engine_free(handle)

print()
if fails:
    print(f"—— {len(fails)} 项失败 ——")
    sys.exit(1)
print("FFI 冒烟：全部通过")
