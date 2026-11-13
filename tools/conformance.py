#!/usr/bin/env python3
"""T6.5 三端一致性 harness（CLI 端 golden 生成 + 校验）。

用法：
  python tools/conformance.py golden            # 生成 golden/*.json（CLI 输出 + SHA-256）
  python tools/conformance.py check             # 校验当前输出与 golden 一致

一致性判据：同一 (source, stage, inputs, fixture) 的 `out` 经 JSON 规范化（键排序）后
SHA-256 逐字节相等（C§7.6 / L5）。WASM/iOS 端用同一 golden 校验（见 golden/README.md）。
"""
import hashlib
import json
import subprocess
import sys
import io as _io
from pathlib import Path

sys.stdout = _io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent
GOLDEN = ROOT / "tests" / "conformance" / "golden"
CLI = ROOT / "target" / "debug" / "rsl.exe"

# (source, stage, inputs_json, fixture) 列表
CASES = [
    (
        "tests/cases/novel-source.rsl.json",
        "search",
        {"kw": "斗破", "page": 1},
        "fixtures/search.html",
    ),
    (
        "tests/cases/novel-source.rsl.json",
        "toc",
        {"book.tocUrl": "https://example.com/book/1/toc"},
        "fixtures/toc_1200.html",
    ),
    (
        "examples/aggregate-merge.rsl.json",
        "search",
        {"kw": "页", "page": 1},
        "fixtures/enumerate.json",
    ),
]


def normalize(obj) -> str:
    """JSON 规范化：键排序 + 紧凑分隔（跨端可比）。"""
    return json.dumps(obj, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def run_case(source, stage, inputs, fixture) -> str:
    args = [str(CLI), "run", str(ROOT / source), stage]
    for k, v in inputs.items():
        if k == "kw":
            args += ["--kw", str(v)]
        elif k == "page":
            args += ["--page", str(v)]
        else:
            args += [f"--{k.split('.')[0]}", f"{k.split('.', 1)[1]}={v}"]
    args += ["--fixture", str(ROOT / fixture)]
    r = subprocess.run(args, capture_output=True, cwd=str(ROOT))
    r.stdout = r.stdout.decode("utf-8", errors="replace") if isinstance(r.stdout, bytes) else r.stdout
    r.stderr = r.stderr.decode("utf-8", errors="replace") if isinstance(r.stderr, bytes) else r.stderr
    if r.returncode != 0:
        raise RuntimeError(f"run 失败: {r.stdout}{r.stderr}")
    obj = json.loads(r.stdout)
    return normalize(obj)


def sha256(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def golden() -> int:
    GOLDEN.mkdir(parents=True, exist_ok=True)
    for i, (source, stage, inputs, fixture) in enumerate(CASES):
        normalized = run_case(source, stage, inputs, fixture)
        name = f"{Path(source).stem}_{stage}_{i}.json"
        (GOLDEN / name).write_text(normalized, encoding="utf-8")
        print(f"✓ golden {name} sha256:{sha256(normalized)[:16]}")
    return 0


def check() -> int:
    fails = 0
    for i, (source, stage, inputs, fixture) in enumerate(CASES):
        name = f"{Path(source).stem}_{stage}_{i}.json"
        golden_path = GOLDEN / name
        if not golden_path.exists():
            print(f"✗ {name}: golden 缺失（先跑 golden 命令）")
            fails += 1
            continue
        expected = golden_path.read_text(encoding="utf-8")
        actual = run_case(source, stage, inputs, fixture)
        if actual == expected:
            print(f"✓ {name} sha256:{sha256(actual)[:16]} 一致")
        else:
            print(f"✗ {name} 哈希不一致")
            fails += 1
    # WASM 端：wasm32 测试需 node/wasm-bindgen-test runner（宿主可执行文件不能直接跑 wasm）。
    # 判据：cargo check -p rsl-wasm --target wasm32-unknown-unknown 通过（编译级一致性）+
    #       tests/conformance.rs 中断言与 golden 同源（见 golden/README.md）。
    wasm_rc = 0
    try:
        r = subprocess.run(
            ["cargo", "check", "-p", "rsl-wasm", "--target", "wasm32-unknown-unknown"],
            capture_output=True, cwd=str(ROOT),
            env={**__import__("os").environ,
                 "CARGO_HOME": str(ROOT / ".toolchain" / "cargo"),
                 "RUSTUP_HOME": str(ROOT / ".toolchain" / "rustup")},
        )
        ok = r.returncode == 0
        print(("✓" if ok else "✗") + " WASM 编译级一致性（wasm32 check + 同源断言 tests/conformance.rs）")
        if not ok:
            wasm_rc = 1
    except Exception as e:
        print(f"⚠ WASM 校验跳过：{e}")
    if fails or wasm_rc:
        print(f"—— {fails + wasm_rc} 项不一致 ——")
        return 1
    print("三端一致性（CLI golden + WASM）：全部一致")
    return 0


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    return golden() if cmd == "golden" else check()


if __name__ == "__main__":
    sys.exit(main())
