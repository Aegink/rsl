"""RSL 仓库自检：Schema 自洽性 + 示例一致性。

无第三方依赖（不要求装 jsonschema），CI 与本地都能直接跑：

    python tools/check.py

校验内容：
1. schema/rsl.schema.json 可解析，且所有 $ref 指向存在的 $defs；
2. examples/*.json 全部通过 schema（最小 JSON Schema 子集实现）。

支持的关键字：type / required / properties / additionalProperties /
patternProperties / items / enum / oneOf / anyOf / allOf / minProperties /
maxProperties / minimum / maximum / exclusiveMinimum / minItems / maxItems /
uniqueItems / pattern / $ref。
"""
import json
import re
import sys

# Windows 默认控制台是 GBK，直接 print 勾号会 UnicodeEncodeError 打断校验。
# 这里强制 UTF-8 输出，失败时退回 replace，保证检查本身永远能跑完。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, ValueError):
        pass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Validator:
    def __init__(self, schema):
        self.root = schema

    def resolve(self, ref):
        cur = self.root
        for part in ref[2:].split("/"):
            cur = cur[part]
        return cur

    def check(self, inst, sch, path="$"):
        errs = []
        if not isinstance(sch, dict):
            return errs
        if "$ref" in sch:
            return self.check(inst, self.resolve(sch["$ref"]), path)

        t = sch.get("type")
        if t is not None:
            types = t if isinstance(t, list) else [t]
            if not any(self.is_type(inst, x) for x in types):
                errs.append(f"{path}: 期望类型 {types}，实际 {type(inst).__name__}")
                return errs

        if "enum" in sch and inst not in sch["enum"]:
            errs.append(f"{path}: 值 {inst!r} 不在枚举 {sch['enum']}")

        if "pattern" in sch and isinstance(inst, str) and not re.search(sch["pattern"], inst):
            errs.append(f"{path}: {inst!r} 不匹配 {sch['pattern']}")

        if isinstance(inst, (int, float)) and not isinstance(inst, bool):
            if "minimum" in sch and inst < sch["minimum"]:
                errs.append(f"{path}: {inst} < minimum {sch['minimum']}")
            if "maximum" in sch and inst > sch["maximum"]:
                errs.append(f"{path}: {inst} > maximum {sch['maximum']}")
            if "exclusiveMinimum" in sch and inst <= sch["exclusiveMinimum"]:
                errs.append(f"{path}: {inst} <= exclusiveMinimum")

        if isinstance(inst, dict):
            for req in sch.get("required", []):
                if req not in inst:
                    errs.append(f"{path}: 缺少必填字段 {req!r}")
            props = sch.get("properties", {})
            pats = sch.get("patternProperties", {})
            addl = sch.get("additionalProperties", True)
            if "minProperties" in sch and len(inst) < sch["minProperties"]:
                errs.append(f"{path}: 属性数 {len(inst)} < minProperties")
            if "maxProperties" in sch and len(inst) > sch["maxProperties"]:
                errs.append(f"{path}: 属性数 {len(inst)} > maxProperties")
            for k, v in inst.items():
                if k in props:
                    errs += self.check(v, props[k], f"{path}.{k}")
                    continue
                hit = False
                for pat, sub in pats.items():
                    if re.search(pat, k):
                        errs += self.check(v, sub, f"{path}.{k}")
                        hit = True
                        break
                if hit:
                    continue
                if addl is False:
                    errs.append(f"{path}: 不允许的字段 {k!r}")
                elif isinstance(addl, dict):
                    errs += self.check(v, addl, f"{path}.{k}")

        if isinstance(inst, list):
            if "minItems" in sch and len(inst) < sch["minItems"]:
                errs.append(f"{path}: 项数 {len(inst)} < minItems")
            if "maxItems" in sch and len(inst) > sch["maxItems"]:
                errs.append(f"{path}: 项数 {len(inst)} > maxItems")
            if sch.get("uniqueItems"):
                seen = []
                for x in inst:
                    if x in seen:
                        errs.append(f"{path}: uniqueItems 重复 {x!r}")
                    seen.append(x)
            if "items" in sch:
                for i, x in enumerate(inst):
                    errs += self.check(x, sch["items"], f"{path}[{i}]")

        for sub in sch.get("allOf", []):
            errs += self.check(inst, sub, path)
        if "anyOf" in sch and not any(not self.check(inst, s, path) for s in sch["anyOf"]):
            errs.append(f"{path}: 不满足 anyOf 任一项")
        if "oneOf" in sch:
            hits = sum(1 for s in sch["oneOf"] if not self.check(inst, s, path))
            if hits != 1:
                errs.append(f"{path}: oneOf 命中 {hits} 项（应为 1）")
        return errs

    @staticmethod
    def is_type(v, t):
        return {
            "object": isinstance(v, dict),
            "array": isinstance(v, list),
            "string": isinstance(v, str),
            "integer": isinstance(v, int) and not isinstance(v, bool),
            "number": isinstance(v, (int, float)) and not isinstance(v, bool),
            "boolean": isinstance(v, bool),
            "null": v is None,
        }.get(t, True)


def main():
    schema = json.loads((ROOT / "schema/rsl.schema.json").read_text(encoding="utf-8"))
    v = Validator(schema)
    failures = 0

    refs, defs = set(), set(schema["$defs"])

    def walk(o):
        if isinstance(o, dict):
            for k, val in o.items():
                if k == "$ref" and isinstance(val, str) and val.startswith("#/$defs/"):
                    refs.add(val.split("/")[-1])
                walk(val)
        elif isinstance(o, list):
            for x in o:
                walk(x)

    walk(schema)
    missing = sorted(refs - defs)
    print(f"schema: $defs={len(defs)} refs={len(refs)} 缺失引用={missing or '无'}")
    failures += len(missing)

    for p in sorted((ROOT / "examples").glob("*.json")):
        doc = json.loads(p.read_text(encoding="utf-8"))
        errs = v.check(doc, schema)
        if errs:
            failures += len(errs)
            print(f"✗ {p.name}: {len(errs)} 处不符")
            for e in errs[:20]:
                print("   ", e)
        else:
            print(f"✓ {p.name} 通过")

    # ── rsl-debug/1 样例产物校验（TASK-BOOK-4 T1）──
    # schema 独立文件（V4-D5：rsl.schema.json $defs=32 不变）；样例由
    # `rsl run examples/novel-html.rsl.json detail --book url=… --fixture … --debug-json`
    # 从 example.com 夹具离线生成，内核输出形状漂移即在此红灯。
    debug_schema_path = ROOT / "schema/rsl-debug.schema.json"
    debug_dir = ROOT / "examples/debug"
    if not debug_schema_path.exists():
        print("✗ 缺少 schema/rsl-debug.schema.json")
        failures += 1
    elif not debug_dir.exists() or not list(debug_dir.glob("*.json")):
        print("✗ 缺少 rsl-debug/1 样例产物（examples/debug/）")
        failures += 1
    else:
        dv = Validator(json.loads(debug_schema_path.read_text(encoding="utf-8")))
        for p in sorted(debug_dir.glob("*.json")):
            doc = json.loads(p.read_text(encoding="utf-8"))
            errs = dv.check(doc, dv.root)
            if errs:
                failures += len(errs)
                print(f"✗ {p.name}: {len(errs)} 处不符（rsl-debug/1）")
                for e in errs[:20]:
                    print("   ", e)
            else:
                print(f"✓ {p.name} 通过（rsl-debug/1）")

    # ── #24 门禁扩展：L-infra 产物存在性（fuzz/基准报告归档，TASK-BOOK-2 §2.2#24）──
    fuzz_report = ROOT / "scratch/fuzz/report/compile_str.json"
    if fuzz_report.exists():
        try:
            d = json.loads(fuzz_report.read_text(encoding="utf-8"))
            crashes = d.get("crashes", 1)
            rounds = d.get("rounds", 0)
            if crashes != 0 or rounds < 10000:
                print(f"✗ fuzz 报告不达标：crashes={crashes} rounds={rounds}（{fuzz_report.name}）")
                failures += 1
            else:
                print(f"✓ fuzz 长时报告：{d.get('target')} {rounds} 轮零 panic")
        except Exception as e:
            print(f"✗ fuzz 报告损坏：{e}")
            failures += 1
    else:
        print(f"✗ 缺少 fuzz 长时报告：{fuzz_report}")
        failures += 1
    bench_report = ROOT / "docs/20261114_C5_engine_bench报告.md"
    if bench_report.exists():
        print("✓ 引擎基准报告归档存在")
    else:
        print("✗ 缺少引擎基准报告归档")
        failures += 1

    # ── C3 门禁扩展（TASK-BOOK-3）：双引擎必绿项 ──
    # 默认引擎（QuickJS，V3-D5）与 Boa 回退侧各跑一遍 rsl-core 全套件。
    # 已由调用方先跑 cargo 时这里做"报告存在性 + 命令提示"级校验；
    # 直接执行见 --with-cargo（CI 传 --with-cargo 时现场跑两套件）。
    dual_arg = "--with-cargo" in sys.argv
    cargo = _find_cargo()
    if dual_arg and cargo:
        for label, args in (
            ("QuickJS(默认)", ["test", "-p", "rsl-core"]),
            ("Boa(回退)", ["test", "-p", "rsl-core",
                          "--no-default-features", "--features", "engine-js,net"]),
        ):
            import subprocess
            r = subprocess.run([cargo, *args], cwd=ROOT, capture_output=True, text=True,
                               encoding="utf-8", errors="replace")
            out = (r.stdout or "") + (r.stderr or "")
            ok_lines = [ln for ln in out.splitlines() if ln.startswith("test result: ok.")]
            bad = [ln for ln in out.splitlines()
                   if "FAILED" in ln or ln.startswith("error")]
            if r.returncode == 0 and ok_lines and not bad:
                n = 0
                for ln in ok_lines:
                    m = re.search(r"ok\.\s*(\d+) passed", ln)
                    if m:
                        n += int(m.group(1))
                print(f"✓ 双引擎门禁 {label}：{n} 测试全绿")
            else:
                print(f"✗ 双引擎门禁 {label} 失败（exit={r.returncode}）")
                for ln in bad[:5]:
                    print("   ", ln)
                failures += 1
    elif dual_arg and not cargo:
        print("✗ --with-cargo 找不到 cargo（PATH 未配置）")
        failures += 1
    else:
        print("ℹ 双引擎门禁：默认 QuickJS + Boa 回退两套件须全绿"
              "（cargo test -p rsl-core / --no-default-features --features engine-js,net）")

    print("结果:", "全部通过" if failures == 0 else f"{failures} 处问题")
    return 0 if failures == 0 else 1


def _find_cargo():
    """定位 cargo：CARGO 环境变量 → PATH → 本仓 .toolchain。"""
    import os
    import shutil
    c = os.environ.get("CARGO")
    if c and Path(c).exists():
        return c
    p = shutil.which("cargo")
    if p:
        return p
    local = ROOT / ".toolchain/cargo/bin"
    for exe in ("cargo.exe", "cargo"):
        cand = local / exe
        if cand.exists():
            return str(cand)
    return None


if __name__ == "__main__":
    sys.exit(main())
