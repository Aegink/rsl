# C5 引擎基准报告（TASK-BOOK-2 §3.3 C5 / §5.4）

- 日期：2026-11-14
- 套件：`crates/rsl-core/benches/engine_bench.rs`（criterion 0.5）
- 运行：`cargo bench -p rsl-core --features engine-quickjs --bench engine_bench -- --sample-size 20 --warm-up-time 1 --measurement-time 3`
- 负载取材：七猫 L3 真实链路（C1 基准负载取材依据，任务书 §4）

## 结果（中位数 [低 高]，3 负载 × 2 引擎）

| 负载 | Boa | QuickJS | QuickJS/Boa | ≥1.5× 判定 |
|---|---|---|---|---|
| qmDecrypt（50× AES-CBC sym 桥） | 1.987 ms [1.959 2.015] | 0.996 ms [0.979 1.017] | **2.00×** | ✅ |
| signParams（1000× 排序拼接 + md5 桥） | 11.75 ms [11.52 12.02] | 5.76 ms [5.64 5.87] | **2.04×** | ✅ |
| cleanTitle（10000× 正则剥离 + trim） | 321.2 ms [316.3 326.6] | 93.4 ms [91.9 95.3] | **3.44×** | ✅ |

## 结论（C1 验收判据）

**QuickJS 三负载全部 ≥1.5× Boa（2.00× / 2.04× / 3.44×）—— C1 的性能验收成立。**
与【联网】js-engine-benchmark 2025（QuickJS ≈2× Boa；Boa ≈3% V8）结论一致，本机
windows-gnu 实测落在同区间。每次迭代 = 完整引擎调用（含上下文创建），与生产路径一致。

## 附注

- 两引擎语义等价性：`cargo test -p rsl-core --features engine-quickjs` 55/55 全绿
  （Boa 侧同套件亦全绿）——切换无语义回归。
- 引擎选择：默认仍 Boa（wasm32 端一致，V2-D2）；native 端可 `--features engine-quickjs`
  显式切换（setup_engine 按 cfg 选择）。
- 局限：单一采样窗（20 样本×3s）；criterion 完整报告在 `target/criterion/`。
- 纯文本循环（cleanTitle）差异最大（3.44×）—— 正则引擎执行占比高；
  桥调用型负载（md5/sym）差异稳定在 2×—— 引擎解释器开销占比主导。
