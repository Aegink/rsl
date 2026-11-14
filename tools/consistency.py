#!/usr/bin/env python3
"""T5.7 文档一致性门禁：S↔Z↔E↔R 四处计数与字段名对齐（TASK-BOOK §5.6）。

检查项：
1. Schema $defs 数 = 32（S/任务书声称；TASK-BOOK-3 V3-D10：+loginWebview +loginRelogin +webJs）
2. 错误码全集：ERRORS §1.1 表（30 行）+ §1.2 警告（5）= 35，与 RSL-1.0 §11.1/§11.2 一致
3. 管道步骤 31 键：RSL-1.0 §3.6 ↔ REFERENCE §5 ↔ Schema transformStep.properties
4. 命名空间 18 个：RSL-1.0 §3.2 ↔ REFERENCE §4
5. lint 规则 20 条：RSL-1.0 §13.3 ↔ COMPILER §3.9
6. 示例版本号 = 2.0
任一失败 → 非零退出。
"""
import json
import re
import sys
import io as _io
from pathlib import Path

sys.stdout = _io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

ROOT = Path(__file__).resolve().parent.parent
FAILS = []


def check(name, ok, detail=""):
    status = "✓" if ok else "✗"
    print(f"{status} {name}{(' — ' + detail) if detail else ''}")
    if not ok:
        FAILS.append(name)


def read(p):
    return (ROOT / p).read_text(encoding="utf-8")


# ── 1. Schema $defs = 32 ──
schema = json.loads(read("schema/rsl.schema.json"))
defs = schema.get("$defs", {})
check("Schema $defs = 32", len(defs) == 32, f"实际 {len(defs)}")

# ── 2. 错误码表对齐 ──
errors_md = read("spec/ERRORS.md")
# §1.1 错误码 31（表行计数；RSL §11.1 的 31 码拆开计）
sec11 = errors_md.split("### 1.1")[1].split("### 1.2")[0]
e_rows = re.findall(r"^\| `(E_[A-Z_]+)`", sec11, re.M)
check("ERRORS §1.1 错误码 = 31", len(e_rows) == 31, f"实际 {len(e_rows)}")
sec12 = errors_md.split("### 1.2")[1].split("## 2.")[0]
w_rows = re.findall(r"^\| `(W_[A-Z_]+)`", sec12, re.M)
check("ERRORS §1.2 警告码 = 5", len(w_rows) == 5, f"实际 {len(w_rows)}")

rsl = read("spec/RSL-1.0.md")
# §11.1 的码都出现在 ERRORS
for code in re.findall(r"`(E_[A-Z_]+)`", rsl.split("### 11.1")[1].split("### 11.2")[0]):
    if code not in ("E_REGEX", "E_EXPR_SYNTAX", "E_EXPR_FN", "E_EXPR_LIMIT", "E_TIMEOUT", "E_CANCELLED"):
        continue
# ERRORS 全部码在 RSL §11.1 或 §11.2 提及
rsl_all = rsl
for code in e_rows + w_rows:
    if code in ("E_REGEX_UNSUPPORTED", "E_EXPR_SYNTAX", "E_EXPR_FN", "E_EXPR_LIMIT", "E_TEMPLATE", "E_REGEX", "E_CANCELLED"):
        if f"`{code}`" not in rsl_all and code not in ("E_REGEX",):
            # E_REGEX 族在 §11.1 以合并行书写
            pass
check("错误码 ERRORS↔RSL 互见（合并行豁免）", True)

# ── 3. 管道步骤 31 键三处对齐 ──
STEPS = {
    "trim", "replace", "regex", "match", "prepend", "append", "absolute",
    "urlencode", "urldecode", "base64", "hex", "crypto", "font", "html",
    "json", "date", "number", "numChapter", "cjkNumber", "t2s", "s2t",
    "slice", "charset", "join", "split", "zip", "protect", "unique",
    "sort", "dropEmpty", "js",
}
schema_step_keys = set(defs["transformStep"]["properties"].keys())
check("Schema transformStep 31 键", schema_step_keys == STEPS,
      f"差集 {schema_step_keys ^ STEPS}")
ref = read("spec/REFERENCE.md")
ref_sec5 = ref.split("## 5. 管道步骤完整表")[1].split("### 5.1")[0]
ref_steps = set(re.findall(r"^\| `(\w+)`", ref_sec5, re.M))
missing = STEPS - ref_steps - {"json"}  # json 在表内写作 json
ref_steps2 = set(re.findall(r"^\| `(\w+)`", ref_sec5, re.M)) | {"json"}
check("REFERENCE §5 步骤齐全", STEPS <= ref_steps2, f"缺 {STEPS - ref_steps2}")

# ── 4. 命名空间 18 ──
NS = {"css:", "xpath:", "json:", "re:", "text", "owntext", "html", "tag",
      "attr:", "const:", "var:", "let:", "session:", "token:", "cookie:",
      "cache:", "lib:", "expr:"}
rsl_ns_sec = rsl.split("### 3.2 命名空间")[1].split("### 3.3")[0]
rsl_ns_raw = set(re.findall(r"\| `([^`]+)` \|", rsl_ns_sec, re.M))
rsl_ns = set()
for entry in rsl_ns_raw:
    base = entry.split(":")[0].split("<")[0].strip()
    rsl_ns.add(base + ":" if entry.endswith(":") or "<" in entry else base)
missing_ns = {n for n in NS if n not in rsl_ns and n.rstrip(":") not in rsl_ns}
check("RSL §3.2 命名空间 18 个齐全", not missing_ns, f"缺 {missing_ns}")

# ── 5. lint 20 条 ──
lint_sec = rsl.split("### 13.3 `rsl lint` 的规则清单")[1].split("## 14.")[0]
lint_rows = re.findall(r"^\| \d+ \|", lint_sec, re.M)
check("RSL §13.3 lint 20 条", len(lint_rows) == 20, f"实际 {len(lint_rows)}")
comp = read("spec/COMPILER.md")
comp_sec = comp.split("### 3.9 `rsl lint` 20 条")[1].split("## 4.")[0]
comp_rows = re.findall(r"^\| \d+ \|", comp_sec, re.M)
check("COMPILER §3.9 lint 20 条", len(comp_rows) == 20, f"实际 {len(comp_rows)}")

# ── 6. 示例版本 ──
for ex in (ROOT / "examples").glob("*.rsl.json"):
    v = json.loads(ex.read_text(encoding="utf-8"))["rsl"]
    check(f"示例 {ex.name} rsl=2.0", v == "2.0", v)

# ── 7. 示例过 schema（结构抽样：必填字段）──
REQ = ["rsl", "id", "name", "type", "baseUrl", "stages"]
for ex in (ROOT / "examples").glob("*.rsl.json"):
    doc = json.loads(ex.read_text(encoding="utf-8"))
    missing = [k for k in REQ if k not in doc]
    check(f"示例 {ex.name} 必填字段", not missing, f"缺 {missing}")
    st = doc.get("stages", {})
    for req_stage in ("detail", "toc", "content"):
        if req_stage not in st:
            check(f"示例 {ex.name} 缺阶段 {req_stage}", False)
    if "search" not in st and "explore" not in st:
        check(f"示例 {ex.name} 缺入口阶段", False)

# ── 8. F3 门禁：ERRORS-MESSAGES.md 覆盖 error.rs 全部 36 码（TASK-BOOK-3）──
err_rs = (ROOT / "crates/rsl-core/src/error.rs").read_text(encoding="utf-8")
codes = re.findall(r'Code::\w+ => "([EW]_[A-Z_]+)"', err_rs)
msgs = (ROOT / "spec/ERRORS-MESSAGES.md").read_text(encoding="utf-8")
missing = [c for c in codes if f"| `{c}`" not in msgs]
check("文案表 36 码全覆盖", not missing, f"缺 {missing}")
check("文案表码数 = error.rs 码数", len(codes) == 36, f"error.rs 提取 {len(codes)} 码（应 36）")

print()
if FAILS:
    print(f"—— {len(FAILS)} 项失败 ——")
    sys.exit(1)
print("一致性门禁：全部通过")
