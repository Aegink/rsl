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

    print("结果:", "全部通过" if failures == 0 else f"{failures} 处问题")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
