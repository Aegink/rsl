# fuzz/ —— A1 fuzz 基建（TASK-BOOK-2 §3.1 A1）

## 形态

- `corpus/` —— A2 种子语料库（1022 文件：485 合法源 + 500 带错误变异种子 + 16 编译期负例
  + 6 正例 + 15 边界构造；URL host 全部消毒为 example.com，V2-D1）
- harness —— `crates/rsl-cli/src/fuzz.rs::run_target`，五个目标 `fn(&[u8])`：
  `compile_str` / `convert_full` / `jsonpath` / `xpath` / `template_expr`

## V2-D6 裁决：进程内循环 fuzzer

【实测】cargo-fuzz（libFuzzer）在 `x86_64-pc-windows-gnu` 不可行：
libFuzzer/ASan runtime 仅支持 MSVC 链接；本机无 MSVC 构建环境（V1 裁决 D5：windows-gnu 工具链）。
nightly-x86_64-pc-windows-gnu 已装（1.102.0-nightly），但 `-C sanitizer` 链接失败是必然。

**替代**：进程内变异循环 fuzzer（`rsl fuzz <target> --minutes N`）：
- 种子：`corpus/` + 目标专用解析器种子
- 变异：JSON 结构感知（4 层递归值/键变异）+ 字节级六式（截断/位翻转/插块/交换/拼接/越界数值）
- 双保险：`panic::set_hook`（归档现场）+ `catch_unwind`（继续执行）
- 发现即归档：`scratch/fuzz/artifacts/<target>/crash_NNNN.bin`（最小化靠语料回放）
- 报告：`scratch/fuzz/report/<target>.json`（rounds/seconds/crashes/verdict，L-infra）

harness 保持 `fn(&[u8])` 形态 —— 未来在 Linux/MSVC 环境接入 cargo-fuzz 零改动。

## 验收记录（2026-11-14）

- 冒烟：5 目标 × 20000 轮 = **100k 轮零 panic**（A4 审计 unwrap 归零的直接回报）
- 长时：`compile_str` 60 min + `convert_full` 30 min + `jsonpath`/`xpath`/`template_expr` 各 15 min
  报告归档 `scratch/fuzz/report/`
