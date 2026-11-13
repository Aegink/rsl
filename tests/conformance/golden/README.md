# T6.5 三端一致性测试

同一 fixture 在 CLI / WASM / iOS 上：`out` 序列化后 SHA-256 相等（`C§7.6`）。

## golden 基线

`golden/novel_search.json` 由 CLI 生成（`rsl run --fixture`），
三端对同一 (source, stage, inputs, fixture) 输出逐字节比较；不等则输出首个差异路径。

## harness

- CLI：`tools/conformance.py` — 调 `rsl run --fixture` 取输出 → SHA-256 → 与 golden 对比。
- WASM：`rsl-wasm` 同一 fixture 跑 `run_stage` → 同一 JSON 规范化（键排序）→ SHA-256 对比。
- iOS：RSLKit `runSync` 同流程（真机/模拟器上跑，本仓提供 Swift 侧脚本骨架）。

JSON 规范化规则（保证跨端哈希可比）：
1. `serde_json::to_string`（BTreeMap 键序 = serde 默认序）；
2. 数值统一 `f64` 序列化；
3. 不含时间戳/墙钟字段（确定性 S§7.3）。
