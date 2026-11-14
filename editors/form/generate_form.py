#!/usr/bin/env python3
"""TASK-BOOK-3 D3：Schema 驱动表单骨架（原型）。

输入 schema/rsl.schema.json → 输出两件：
1. editors/form/form-config.json —— 每个顶层字段的表单控件配置（选型 H2 待原型验证后定，
   本原型输出"字段 → widget"中间表示，任何表单引擎都能消费）；
2. editors/form/skeleton.rsl.json —— 从 Schema required 推导的最小合法骨架源
   （验收：`rsl lint` 通过）。

用法：python -I editors/form/generate_form.py
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCHEMA = ROOT / "schema/rsl.schema.json"
OUT_CFG = Path(__file__).resolve().parent / "form-config.json"
OUT_SKEL = Path(__file__).resolve().parent / "skeleton.rsl.json"

# 类型 → 控件映射（表单引擎无关的中间表示）
WIDGET = {
    "string": "text",
    "integer": "number",
    "number": "number",
    "boolean": "checkbox",
    "array": "list",
    "object": "group",
}

# 需要专用选择器控件的字段（选择器文本 + 命名空间感知）
SELECTOR_FIELDS = {"$", "list", "parse", "ok", "success", "content", "next"}


def resolve_ref(schema, ref):
    cur = schema
    for part in ref[2:].split("/"):
        cur = cur[part]
    return cur


def widget_for(prop, schema):
    if "$ref" in prop:
        target = resolve_ref(schema, prop["$ref"])
        if target.get("$ref") == "#/$defs/selector" or (
            isinstance(target, dict) and target.get("title") == "选择器"
        ):
            return "selector"
        return widget_for(target, schema)
    t = prop.get("type")
    if isinstance(t, list):
        t = t[0] if t else None
    if "enum" in prop:
        return "select"
    if t == "string" and prop.get("format") == "uri":
        return "url"
    return WIDGET.get(t, "text")


def field_config(schema, defs):
    """顶层字段 → 表单配置（含嵌套一层 $ref 展开）。"""
    cfg = []
    for name, prop in schema.get("properties", {}).items():
        entry = {
            "field": name,
            "widget": widget_for(prop, schema),
            "required": name in schema.get("required", []),
            "description": prop.get("description", ""),
        }
        if "enum" in prop:
            entry["options"] = prop["enum"]
        if "$ref" in prop:
            target_name = prop["$ref"].split("/")[-1]
            target = defs.get(target_name, {})
            entry["$def"] = target_name
            sub_props = target.get("properties", {})
            if sub_props:
                entry["fields"] = [
                    {
                        "field": k,
                        "widget": widget_for(v, schema),
                        "required": k in target.get("required", []),
                        "$def": v["$ref"].split("/")[-1] if "$ref" in v else None,
                    }
                    for k, v in sub_props.items()
                ]
        cfg.append(entry)
    return {"schema_version": "2.0", "fields": cfg}


def gen_skeleton(schema):
    """最小合法骨架：meta required + 四阶段 + 五阶段契约必需输出。"""
    req = set(schema.get("required", []))
    skel = {}
    if "rsl" in req:
        skel["rsl"] = "2.0"
    skel["id"] = "form-skel.example.com"
    skel["name"] = "表单骨架站（生成物）"
    if "type" in req:
        skel["type"] = "novel"
    if "carrier" in req:
        skel["carrier"] = "web"
    skel["baseUrl"] = "https://form-skel.example.com"
    if "requires" in req or True:
        skel["requires"] = {"features": [], "crypto": []}
    skel["stages"] = {
        "search": {
            "request": {
                "url": "https://form-skel.example.com/search",
                "params": {"q": "var:kw"},
            },
            "out": {
                "list": "css:ul.list li",
                "map": {
                    "name": {"$": "css:a.title", "transform": [{"trim": True}]},
                    "url": {
                        "$": "css:a.title",
                        "attr": "href",
                        "transform": [{"absolute": True}],
                    },
                },
                "emptyOk": True,
            },
        },
        "detail": {
            "request": {"url": "var:book.url"},
            "out": {
                "name": "css:h1",
                "tocUrl": {"$": "css:a.read", "attr": "href", "optional": True},
            },
        },
        "toc": {
            "request": {"url": "var:book.tocUrl"},
            "out": {
                "list": "css:#list li",
                "map": {
                    "title": {"$": "css:a", "transform": [{"trim": True}]},
                    "url": {"$": "css:a", "attr": "href", "transform": [{"absolute": True}]},
                },
            },
        },
        "content": {
            "request": {"url": "var:chapter.url"},
            "out": {"content": {"$": "css:div#content", "paragraphs": {"split": "br"}}},
        },
    }
    return skel


def main():
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    defs = schema.get("$defs", {})
    cfg = field_config(schema, defs)
    OUT_CFG.write_text(
        json.dumps(cfg, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    skel = gen_skeleton(schema)
    OUT_SKEL.write_text(
        json.dumps(skel, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"表单配置 → {OUT_CFG}（{len(cfg['fields'])} 顶层字段）")
    print(f"骨架源   → {OUT_SKEL}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
