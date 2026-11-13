// T6.5 WASM 三端一致性真跑（node + wasm-bindgen glue）：
// 同一 fixture 在 WASM 引擎上跑 search/toc/enumerate，输出 JSON 规范化后
// 与 tests/conformance/golden/*.json（CLI 端生成）逐字节对比哈希。
import { RslEngine, version, compile } from "./pkg/rsl_wasm.js";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const GOLDEN = join(ROOT, "tests", "conformance", "golden");

const read = (p) => readFileSync(join(ROOT, p), "utf-8");
const b64 = (s) => Buffer.from(s, "utf-8").toString("base64");
const transport = (body, contentType = "text/html; charset=utf-8") =>
  JSON.stringify({
    responses: [
      { url: "fixture://", status: 200, content_type: contentType, body_base64: b64(body) },
    ],
  });

// JSON 规范化（与 tools/conformance.py 一致：键排序 + 紧凑分隔）
const normalize = (obj) => JSON.stringify(sortDeep(obj));
function sortDeep(v) {
  if (Array.isArray(v)) return v.map(sortDeep);
  if (v && typeof v === "object") {
    const out = {};
    for (const k of Object.keys(v).sort()) out[k] = sortDeep(v[k]);
    return out;
  }
  return v;
}
const sha256 = (s) => createHash("sha256").update(s, "utf-8").digest("hex");

let fails = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "✓" : "✗"} ${name}${detail ? " — " + detail : ""}`);
  if (!ok) fails++;
};

console.log(`WASM engine: ${version()}`);

// ── 用例 1：search（novel-source + search.html）──
{
  const src = read("tests/cases/novel-source.rsl.json");
  const eng = new RslEngine(src, transport(read("fixtures/search.html")));
  const r = JSON.parse(eng.run_stage("search", JSON.stringify({ kw: "斗破", page: 1 })));
  if (!r.ok) console.log("[dbg] search error:", JSON.stringify(r.error));
  check("wasm search ok=true", r.ok === true);
  const list = r.out.list;
  check("wasm search 3 条", list.length === 3);
  check("wasm list[0].map.name", list[0].map.name === "斗破苍穹");

  const goldenFile = join(GOLDEN, "novel-source.rsl_search_0.json");
  const goldenRaw = readFileSync(goldenFile, "utf-8");
  // golden 存的是 normalize 字符串；WASM 输出双方向 sortDeep 后比哈希
  const wasmNorm = normalize(r.out);
  const goldenNorm = normalize(JSON.parse(goldenRaw));
  check(
    "wasm search 哈希 == CLI golden",
    sha256(wasmNorm) === sha256(goldenNorm),
    `${sha256(wasmNorm).slice(0, 16)} vs ${sha256(goldenNorm).slice(0, 16)}`,
  );
}

// ── 用例 2：toc 1200 章 ──
{
  const src = read("tests/cases/novel-source.rsl.json");
  const eng = new RslEngine(src, transport(read("fixtures/toc_1200.html")));
  const r = JSON.parse(
    eng.run_stage("toc", JSON.stringify({ book: { tocUrl: "https://example.com/book/1/toc" } })),
  );
  check("wasm toc 1200 章", r.ok && r.out.list.length === 1200);
  const goldenFile = join(GOLDEN, "novel-source.rsl_toc_1.json");
  const goldenRaw = readFileSync(goldenFile, "utf-8");
  const wasmNorm = normalize(r.out);
  const goldenNorm = normalize(JSON.parse(goldenRaw));
  check(
    "wasm toc 哈希 == CLI golden",
    sha256(wasmNorm) === sha256(goldenNorm),
    `${sha256(wasmNorm).slice(0, 16)} vs ${sha256(goldenNorm).slice(0, 16)}`,
  );
}

// ── 用例 3：enumerate（agg 源）──
{
  const src = read("examples/aggregate-merge.rsl.json");
  const eng = new RslEngine(src, transport(read("fixtures/enumerate.json"), "application/json"));
  const r = JSON.parse(eng.run_stage("search", JSON.stringify({ kw: "页", page: 1 })));
  check("wasm enumerate 2 条", r.ok && r.out.list.length === 2);
  const goldenFile = join(GOLDEN, "aggregate-merge.rsl_search_2.json");
  const goldenRaw = readFileSync(goldenFile, "utf-8");
  const wasmNorm = normalize(r.out);
  const goldenNorm = normalize(JSON.parse(goldenRaw));
  check(
    "wasm enumerate 哈希 == CLI golden",
    sha256(wasmNorm) === sha256(goldenNorm),
    `${sha256(wasmNorm).slice(0, 16)} vs ${sha256(goldenNorm).slice(0, 16)}`,
  );
}

// ── 编译拦截冒烟 ──
{
  const d = JSON.parse(compile('{"rsl":"2.0","id":"x"}'));
  check(
    "wasm compile 非法源 → E_SCHEMA",
    d.ok === false && d.diagnostics.some((x) => x.code === "E_SCHEMA"),
  );
}

console.log();
if (fails) {
  console.log(`—— ${fails} 项失败 ——`);
  process.exit(1);
}
console.log("WASM 三端一致性（node 真跑）：全部通过");
