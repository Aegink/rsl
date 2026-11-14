---
id: rsl-taskbook-2
type: plan
status: draft
title: RSL 内核增强任务书 v1.0（更稳定 · 更好用 · 更强大）
parent: rsl-taskbook
tags: [rsl, taskbook, v2, fuzzing, lsp, quickjs, jsonpath, research]
---

# RSL 内核增强任务书 v1.0

> 本文是**增强施工图**：在已完成的 V1 内核（M1–M6，commit `0d64a1e`）之上，规划"更稳定 / 更好用 / 更强大"三大方向的全部任务。
> 本文**不修改** `RSL-1.0.md` 语言契约与 `TASK-BOOK.md`（V1）的任何条款；所有增强在现有架构端口内完成。
> 全部任务经过九维交叉验证；研究依据全部标注【联网】或【实测】来源。

---

## 0. 使用说明

### 0.1 权威顺序（冲突裁决梯）

1. `RSL-1.0.md` —— 语言契约（不变）
2. `TASK-BOOK.md` —— V1 内核任务书（已完结，作为基线引用）
3. **本文** —— 增强任务书
4. `docs/handoff/handoff_2026-11-14_02-50.md` —— 实施记录与裁决 D1–D11
5. `scratch/batch_report.json` —— 4990 真实源压测报告（数据事实）

冲突时自下而上引用、自上而下裁决：契约 > V1 > 本文 > 实施记录。

### 0.2 引用标记

| 标记 | 指向 | 例 |
|---|---|---|
| `V1§2.2` | TASK-BOOK.md 章节 | V1§2.2 主矩阵 |
| `R§11.1` | RSL-1.0.md 章节 | 错误模型 |
| `D1–D11` | 实施记录中的裁决 | D6 Boa 引擎 |
| 【联网】 | 外部研究来源（附链接） | 【联网】QuickJS 性能 2× Boa |
| 【实测】 | 本仓压测/实测数据 | 【实测】E_SCHEMA 2721 |
| `A1` `B2` `C3` | 本文任务编号 | A1 fuzz |

### 0.3 DoD 等级（沿用 V1§0.3，扩展 infra 级）

| 等级 | 含义 | 证据 |
|---|---|---|
| L0 | 代码写完、能编译 | `cargo check` 通过 |
| L1 | 单测通过 | `cargo test` 相关用例绿 |
| L2 | 验收命令产出约定结果 | 任务"验收"列的命令可复现 |
| L3 | 真实数据/真实环境验证 | 4990 源全量 / 真机 / 真站点 |
| **L-infra** | 长时任务（fuzz/基准）产出持久化报告并归档 | `scratch/fuzz/report/`、`benches/` |

### 0.4 前置输入清单

| 输入 | 角色 | 本文消费方式 |
|---|---|---|
| `TASK-BOOK.md`（V1） | 基线 | 全部 V1 任务的完成状态与遗留 |
| `docs/handoff/…02-50.md` | 实施记录 | 裁决 D1–D11、五缺口修复清单 |
| `scratch/batch_report.json`【实测】 | 压测数据 | 错误码分布（E_SCHEMA 2721 / E_SELECTOR 1735）与不可转字段 Top15 |
| 七猫 L3 实测 | 端到端证据 | onResponse 解密、动态 IV、签名请求链路 |
| 联网研究（§4 落位表，12 来源） | 外部依据 | 每个任务的技术选型依据 |

---

## 1. 交付总览

### 1.1 三大方向与里程碑

| 方向 | 里程碑 | 交付 | 验收上限 |
|---|---|---|---|
| **A 更稳定** | S1 | fuzz harness ×5 + 种子语料 + 属性测试 ×8 + 协作式取消 + panic 审计 | 48h fuzz 零 panic 报告归档 |
| **B 更好用** | U1 | `rsl-lsp`（诊断/hover/completion/定义跳转）+ `rsl test --watch` + `rsl import` | VS Code 内实时 lint 七猫草稿全流程 |
| **C 更强大** | P1 | QuickJS 引擎切换（native）+ JSONPath RFC 9535 + scraper 升级 + 引擎基准套件 | 基准报告 + 31 测试 + 9 YAML 全绿 |

### 1.2 依赖图

```
        ┌─────────── A 更稳定 ───────────┐
        │ A1 fuzz ← A2 语料 ← A4 panic审计 │
        │ A3 proptest                      │
        │ A5 协作式取消 ──────────────┐    │
        └────────────────────────────┼────┘
                                     │ (C5 并发用 A5)
        ┌─────────── C 强大 ─────────┼────┐
        │ C1 QuickJS ← C5 基准套件    │    │
        │ C2 JSONPath 9535            │    │
        │ C3 scraper 升级             │    │
        │ C4 lol_html（未决，spike）  │    │
        └────────────────────────────┴────┘
        ┌─────────── B 更好用 ───────────┐
        │ B1 LSP ← (analyze 诊断复用)    │
        │ B2 watch ← testkit             │
        │ B3 import ← convert + batch    │
        └────────────────────────────────┘
```

硬约束（沿 V1§1.2）：`rsl-core` 保持同步无 tokio；LSP/watch 用独立 bin crate 包裹；`ScriptEngine`/`Transport` 端口不变。

### 1.3 规模预算

| 模块 | 新增预算 | 说明 |
|---|---|---|
| `fuzz/`（fuzz targets） | ~400 行 | 5 个 harness + 语料生成 |
| `rsl-core` 属性测试 | ~300 行 | proptest 8 条 |
| `rsl-core` 修改（取消/审计/C2/C3） | ~600 行 | 含删除手写 jsonpath |
| `crates/rsl-lsp` | ~900 行 | LSP server（新 crate） |
| `rsl-cli` 增量（watch/import/bench） | ~500 行 | |
| `benches/` | ~250 行 | criterion 基准 |
| **合计** | **≈2950 行** | |

---

## 2. 多维交叉验证

### 2.1 九个维度

| 维 | 名称 | 判什么 | 缺失意味着 |
|---|---|---|---|
| **D1** | 依据条款 | RSL 契约/V1 任务/内核铁律对应点 | 改动无锚点 |
| **D2** | 研究来源 | 【联网】外部证据 + 【实测】内部数据 | 拍脑袋 |
| **D3** | 现状基线 | 内核今天的确切状态 | 重复造轮子 |
| **D4** | 任务编号 | WBS 落点 | 无法派工 |
| **D5** | 交付物 | 具体文件/命令 | 无从验收 |
| **D6** | 验收 | 可执行命令与判据 | "完成"不可证 |
| **D7** | 依赖 | 前置任务/风险 | 顺序混乱 |
| **D8** | 回归 | 完成后必须保持绿的既有门禁 | 拆东墙补西墙 |
| **D9** | 优先级 | P0/P1/P2 | 资源错配 |

### 2.2 主矩阵（24 条能力）

> 单元格格式：内容或编号。九列全绿 = 可交付。

#### A 更稳定（1–10）

| # | 能力 | D1 依据 | D2 研究来源 | D3 现状 | D4 任务 | D5 交付物 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `compile_str` fuzz harness | V1§3（7 步流水线） | 【联网】[Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html)、[safeguard.sh](https://safeguard.sh/resources/blog/fuzz-a-rust-library-with-cargo-fuzz) | 零 fuzz | A1 | `fuzz/fuzz_targets/compile_str.rs` | 24h 无 panic；发现即归档最小用例 | A4 | 31 测试 | P0 |
| 2 | `jsonpath::parse` fuzz | R§3.2 json: | 【联网】[fuzze.rs](https://fuzze.rs/blog/fuzzing-rust-end-to-end) | 手写子集未 fuzz | A1 | `fuzz/fuzz_targets/jsonpath.rs` | 同上 | A4 | 七猫 json: 全链路 | P0 |
| 3 | xpath parse fuzz | R§3.2 xpath: | 【联网】同上 | 子集解析器未 fuzz | A1 | `fuzz/fuzz_targets/xpath.rs` | 同上 | A4 | 七猫 toc | P0 |
| 4 | `sugar::parse_template` + expr fuzz | R§4.1 ${} / §4.4 expr | 【实测】E_TEMPLATE 36 例 | 未 fuzz | A1 | `fuzz/fuzz_targets/template_expr.rs` | 同上 | A4 | expr_proto 27 | P0 |
| 5 | 种子语料库 | 【实测】4990 真实源 | 【实测】batch_report | 语料零散 | A2 | `fuzz/corpus/`（合规字段子集，不含站点信息） | 语料覆盖 ≥ 300 合法源 + 全部 15 负例 | — | — | P0 |
| 6 | 铁律属性：永不裸空串 | R§11 铁律 1 | 【实测】31 测试已有样例断言 | 点断言非属性化 | A3 | `tests/properties.rs` P1 | proptest 1000 case | — | 31 测试 | P0 |
| 7 | 铁律属性：降级不清空主结果 | R§11.3 | 【实测】toc isVip 案例 | 单点覆盖 | A3 | `tests/properties.rs` P2 | 随机 map 字段缺失 → 主结果保留 | — | l2_smoke | P0 |
| 8 | 铁律属性：severity↔code 全表一致 | R§11.1 | 【实测】error.rs 表驱动 | 无属性验证 | A3 | `tests/properties.rs` P3 | 全码扫描断言 | — | m1_compile | P1 |
| 9 | 协作式取消（请求/页/步骤边界） | R§12.5 | 【实测】content nextPage 循环无取消点 | 仅 AtomicBool 全局轮询 | A5 | cancel token 注入 ExecCx + 循环边界检查 | 分页中途取消 → partial 结果 + E_CANCELLED | — | 9 YAML | P0 |
| 10 | panic 边界审计 | V1§0.3 L1 | 【联网】[safeguard.sh](https://safeguard.sh/resources/blog/fuzz-a-rust-library-with-cargo-fuzz)：Rust 不防 panic | grep unwrap ≈ 数十处 | A4 | 审计清单 + 修复 PR | `cargo clippy` unwrap/expect 归零（lib 路径） | A1 | 全量 | P0 |

#### B 更好用（11–18）

| # | 能力 | D1 依据 | D2 研究来源 | D3 现状 | D4 任务 | D5 交付物 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 11 | LSP server 骨架 | V1§2.2 lint 20 条 | 【联网】[LSP 3.17](https://microsoft.github.io/language-server-protocol/specification)、[tower-lsp] | 无编辑器集成 | B1 | `crates/rsl-lsp`（tower-lsp） | LSP4J/vscode 冒烟：initialize→diagnostics | B4 | analyze 单测 | P1 |
| 12 | publishDiagnostics（保存即 lint） | R§13.3 | 【联网】[dbt LSP 博客](https://www.getdbt.com/blog/language-server-protocol)：一个 server 通吃编辑器 | CLI-only | B1 | 诊断推送（错误码+path+hint） | 打开七猫草稿 → 3s 内出诊断 | 11 | lint 输出格式 | P1 |
| 13 | hover：选择器/变量解释 | R§3.2/§5.3 | 【联网】[Gauge DSL+LSP](https://gauge.org/2017/12/07/language-server-protocol-and-dsls) | 无 | B1 | hover 处理器 | hover `var:book.id` → 显示来源契约 | 11 | — | P2 |
| 14 | completion：命名空间/管道步骤键 | R§3.2/§3.6 | 【联网】同上 | 无 | B1 | completion 列表（18 命名空间 + 31 步骤键） | `css:` 触发 → 补全列表 | 11 | — | P2 |
| 15 | goto-definition：`var:book.id` → out.map 声明处 | R§5.4 | 【实测】引用图已有（analyze/refs） | 图在内核，无跳转 | B1 | definition 处理器 | 跳转到声明字段行 | 11 | 引用图单测 | P2 |
| 16 | `rsl test --watch` | V1§3.5 B2 | 【联网】[Contextive LSP 实践](https://dev.to/contextive/experiences-building-a-language-server-protocol-plugin-46b0)（防抖重分析） | CLI-only | B2 | notify 防抖 + 增量重跑 | 改 fixture → 2s 内重跑受影响用例 | — | 9 YAML | P1 |
| 17 | `rsl import <url-or-file>` | V1§3.5 B3 | 【联网】[AOAOSTAR 聚合 3907 条](https://legado.aoaostar.com)、【实测】yuancang API | batch_test 散在 scratch | B3 | `rsl import` 命令（列表→拉取→转换→报告→--pack） | 对 yuancang 单页 100 源一键完成 | B5 | convert 单测 | P1 |
| 18 | import 统计报告（错误码/不可转分布） | 【实测】batch_report.json | 【实测】E_SCHEMA 2721 | 散在 scratch | B3 | 内置聚合（migration 聚合报告） | 报告 JSON 与 batch_report 同构 | 17 | — | P1 |

#### C 更强大（19–24）

| # | 能力 | D1 依据 | D2 研究来源 | D3 现状 | D4 任务 | D5 交付物 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 19 | QuickJS 引擎切换（native） | V1 T4.4 端口 | 【联网】[js-engine-benchmark 2025](https://dev.to/ahaoboy/js-engine-benchmark-2025-7-8-163b)：QuickJS ≈2× Boa | Boa（D6 裁决） | C1 | `ScriptEngine` 的 quickjs 实现（feature 切换） | 基准：qmDecrypt 等价负载 ≥1.5× Boa；31 测试全绿 | C5 | l2_smoke/九 YAML | P1 |
| 20 | JSONPath RFC 9535 全合规 | R§3.2 json: / §9.2 selectorLevel 9535 | 【联网】[jsonpath-rust](https://github.com/besok/jsonpath-rust)（[0.7.5](https://crates.io/crates/jsonpath-rust/0.7.5)）RFC 合规 + 函数扩展；[RFC 9535](https://datatracker.ietf.org/doc/rfc9535/) | 手写子集（无过滤/函数） | C2 | 替换 selector/jsonpath.rs 内部实现（接口不变） | 合规测试集全过 + 七猫/测试源全绿；`[?@.x==false]` 可用 | — | 全部 json: 用例 | P1 |
| 21 | scraper/selectors 升级（CSS4 :has()） | R§9.2 css level 4 | 【联网】[selectors 0.26 已支持 :has()](https://crates.io/crates/selectors)、[htmlq #65 panic 案例](https://github.com/mgdm/htmlq/issues/65) | scraper 0.20（selectors 旧 pin） | C3 | 升级 + 兼容修复 | `:has()`/`:is()` 用例通过 + 31 测试全绿 | — | l2_smoke | P2 |
| 22 | lol_html 大页流式（spike） | R§8.2 maxBytes | 【联网】[lol_html 常量内存](https://github.com/cloudflare/lol_html)、[docs.rs #876](https://github.com/rust-lang/docs.rs/issues/876)：远快于 html5ever | 全 DOM 物化 | C4 | spike 报告 + 原型（超阈值正文提取） | spike 结论：可行/不可行 + 数据 | — | content 用例 | P3 |
| 23 | 引擎基准测试套件 | V1§0.3 L-infra | 【联网】[Boa nightly 基准实践] | 无基准 | C5 | `benches/engine_bench.rs`（criterion） | 归档报告：签名/解密/正则三负载 | C1 | — | P1 |
| 24 | 回归门禁扩展（fuzz 语料入库 + L-infra 报告归档） | V1§2.5 | 【实测】batch_report 模式 | 门禁只有 check/consistency | A2/C5 | `.gitignore` 调整 + 门禁脚本增项 | `python tools/check.py` + fuzz 报告存在性检查 | A1 | 全部 | P1 |

### 2.3 矩阵读数

| 维 | 覆盖 | 说明 |
|---|---|---|
| D1 依据 | 24/24 | 全部锚定 V1 任务或 RSL 契约 |
| D2 来源 | 24/24 | 12 个【联网】来源 + 5 项【实测】数据 |
| D3 现状 | 24/24 | 逐条核对内核当前代码 |
| D5 交付 | 24/24 | 全部到文件级 |
| D6 验收 | 24/24 | 全部可执行 |
| **D9 P0** | 10 项（A 全部 + C5 部分） | 稳定性先行 |

### 2.4 缺口与未决登记（H 编号）

| # | 缺口/未决 | 处置 |
|---|---|---|
| H1 | QuickJS 在 `x86_64-pc-windows-gnu` 的绑定可用性未证（rquickjs 0.6 需 libclang） | C1 拆两个 spike：`rquickjs 0.9+`（检查是否含 gnu 预生成绑定）与 `quick-js`；两者皆败 → 保留 Boa 并登记（不阻塞其他任务） |
| H2 | `jsonpath-rust` 的 filter 语法与 RSL `where` 的语义边界 | C2 内做映射表：RFC 9535 filter 覆盖 where 的 eq/ne/gt/lt/has/text/not —— 有多余能力时以 RSL `where` 白名单裁剪 |
| H3 | tower-lsp 引入 tokio —— 与"core 无 tokio"约束冲突？ | 不冲突：tokio 仅进 `rsl-lsp` bin crate；core 保持同步 |
| H4 | fuzz 语料含真实站点 URL 是否违反 S§17 | 语料只入库**合规字段子集**（选择器/管道），bookSourceUrl 全部替换为 example.com；站点信息留 scratch（gitignore） |
| H5 | `:has()` 需 scraper ≥0.23，API 破坏面未知 | C3 拆：先 `cargo upgrade` 观察 breakage，破坏点逐个修（预计 selector parse 签名/ego_tree 版本） |
| H6 | 4990 源里 2736 个非 Legado 方言（actionID 格式）是否值得做第二转换器 | **不在本文范围**——登记为后续评估（V1§16 精神：不做没把握的兼容） |

---

## 3. 任务分解（WBS）

> 每任务：目标 / 输入 / 输出 / 依赖 / 验收（可执行）/ 引用。

### 3.1 A 更稳定（A1–A5）

| ID | 目标 | 输入 | 输出 | 依赖 | 验收（可执行） | 引用 |
|---|---|---|---|---|---|---|
| **A1** | 5 个 fuzz harness：compile_str / jsonpath / xpath / template+expr / convert-full | A2 语料、A4 审计 | `fuzz/` crate（nightly） | A2、A4 | `cargo fuzz build`；各 target 跑 1h 冒烟零 panic；发现用例最小化归档 `scratch/fuzz/artifacts/` | §2.2#1–4 |
| **A2** | 种子语料库：4990 源的合规子集（去站点信息）+ 15 负例 + 边界构造 | batch_report | `fuzz/corpus/*.json` + 生成脚本 `tools/gen_corpus.py` | — | 语料 ≥300 合法源；`tools/check.py` 不受影响 | §2.2#5 |
| **A3** | proptest 8 条：三条铁律 + 模板/expr/jsonpath/xpath/regex/选择器不 panic | error.rs、铁律 | `tests/properties.rs` | A4 | `cargo test --properties` 1000 case × 8 全绿 | §2.2#6–8 |
| **A4** | panic 边界审计：lib 路径 unwrap/expect/panic! 清零或 catch_unwind 包裹（convert 入口） | 内核源码 | 审计清单 + 修复 | — | `cargo clippy -p rsl-core -- -W clippy::unwrap_used` 非 test 路径归零；batch convert 恶意输入不崩 | §2.2#10 |
| **A5** | 协作式取消：CancelToken（Arc<AtomicBool>+generation）注入 ExecCx；分页循环/nextPage/login 步/parallel 批次边界检查；请求级取消 | 内核 | 修改 `stage/`、`ports.rs` | — | 测试：content nextPage 中途 cancel → 返回 partial + E_CANCELLED；YAML 9/9 不变 | §2.2#9 |

### 3.2 B 更好用（B1–B4）

| ID | 目标 | 输入 | 输出 | 依赖 | 验收（可执行） | 引用 |
|---|---|---|---|---|---|---|
| **B1** | `rsl-lsp` server：initialize / publishDiagnostics（20 条 lint） / hover（命名空间、变量来源、步骤签名） / completion（18 命名空间 + 31 步骤键 + `var:` 契约字段） / definition（var: → out.map 声明处） | analyze 诊断、引用图 | `crates/rsl-lsp`（tower-lsp + tokio，独立 bin） | A4 | 冒烟脚本：LSP4J 或 vscode-languageserver 模拟 initialize→didOpen→diagnostics≤3s→hover→completion→definition | §2.2#11–15 |
| **B2** | `rsl test --watch [--cases f]`：notify 防抖（500ms）监听源/fixture/用例文件变更，增量重跑 | testkit | `rsl-cli` watch 子命令 | — | 手改 fixture 保存 → 2s 内自动重跑并输出摘要；Ctrl-C 干净退出 | §2.2#16 |
| **B3** | `rsl import <url-or-file> [--out dir] [--limit N] [--report]`：识别 yuancang/AOAOSTAR/本地文件 → 拉取（并发 12 + 重试） → convert → 聚合报告（JSON 同构 batch_report） → 可选 --pack | batch_test 逻辑 | `rsl-cli` import 子命令（逻辑上移 testkit::batch） | A4 | 对 yuancang 单页 100 源跑通；报告 JSON schema 与 batch_report 一致 | §2.2#17–18 |
| **B4** | LSP 冒烟环境：`tests/lsp/`（vscode-languageclient 或裸 LSP4J JSON-RPC 脚本） | B1 | 冒烟脚本 + README | B1 | 三种编辑器中至少 VS Code 实测诊断/hover 截图归档 | §2.2#11 |

### 3.3 C 更强大（C1–C5）

| ID | 目标 | 输入 | 输出 | 依赖 | 验收（可执行） | 引用 |
|---|---|---|---|---|---|---|
| **C1** | QuickJS native 引擎：spike `rquickjs 0.9`/`quick-js` 在 windows-gnu 的可用性（H1）→ 实现 `ScriptEngine` quickjs 后端（feature `engine-quickjs`），两档隔离/确定性计量语义与 boa 相同 | boa 实现作参照 | `script/quickjs_engine.rs` + feature 切换 | C5 | 基准：qmDecrypt 等价负载 ≥1.5× Boa；31 测试全绿（切换后） | §2.2#19、H1 |
| **C2** | JSONPath RFC 9535：`jsonpath-rust` 替换手写 evaluator；`where` 语义映射白名单（H2）；解析错误 → E_SELECTOR | selector/jsonpath.rs | 内部替换 + 兼容层 | A4 | RFC 合规测试子集全过；七猫/测试源 json: 全绿；新增 `[?@.vip==false]` 过滤用例 | §2.2#20、H2 |
| **C3** | scraper/selectors 升级至含 :has() 版本（H5）；兼容修复 | Cargo.toml | 版本升级 + 破坏修复 | A4 | `:has()`/`:is()`/`:where()` 新用例；31 测试 + 9 YAML 全绿 | §2.2#21 |
| **C4** | lol_html 大页流式 spike（未决 C4） | lol_html 文档 | spike 报告 + 原型分支 | — | spike 结论文档：500KB 页内存/耗时对比 DOM 方案；可行则出集成设计 | §2.2#22 |
| **C5** | 引擎基准套件：criterion 三负载（qmDecrypt AES-CBC / signParams md5 链 / cleanTitle 正则链）×（boa/quickjs） | C1 | `benches/engine_bench.rs` + 归档报告 | C1 | 报告含中位数/均值/方差；QuickJS ≥1.5× Boa 结论成立与否明确 | §2.2#23 |

---

## 4. 前置研究落位表（12 来源全量落位）

| 来源 | 等级 | 关键结论 | 落位任务 |
|---|---|---|---|
| [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html) | 【联网】 | cargo-fuzz=libFuzzer 包装；需 nightly | A1 |
| [safeguard.sh fuzz 指南](https://safeguard.sh/resources/blog/fuzz-a-rust-library-with-cargo-fuzz) | 【联网】 | Rust 不防 panic/hang——正是解析器风险面 | A1、A4 |
| [fuzze.rs 端到端实战](https://fuzze.rs/blog/fuzzing-rust-end-to-end) | 【联网】 | 1 小时出首个 bug；crash 最小化流程 | A1 |
| [safeguard.sh 结构化模糊](https://safeguard.sh/resources/blog/fuzz-a-rust-library-with-cargo-fuzz) | 【联网】 | arbitrary crate 结构化输入对 JSON 解析器最深 | A1、A2 |
| [jsonpath-rust](https://github.com/besok/jsonpath-rust) / [0.7.5](https://crates.io/crates/jsonpath-rust/0.7.5) | 【联网】 | RFC 9535 全合规 + 函数扩展 | C2 |
| [RFC 9535](https://datatracker.ietf.org/doc/rfc9535/) | 【联网】 | 2024-02 IETF 标准；合规测试集存在 | C2 |
| [js-engine-benchmark 2025](https://dev.to/ahaoboy/js-engine-benchmark-2025-7-8-163b) / [repo](https://github.com/ahaoboy/js-engine-benchmark) | 【联网】 | QuickJS ≈2× Boa；Boa ≈3% V8 | C1、C5 |
| [lol_html](https://github.com/cloudflare/lol_html) / [docs.rs #876](https://github.com/rust-lang/docs.rs/issues/876) / [Zenrows 对比](https://www.zenrows.com/blog/rust-html-parser) | 【联网】 | 常量内存流式 ≫ html5ever；无 DOM/选择器 | C4 |
| [selectors crate](https://crates.io/crates/selectors) / [htmlq #65](https://github.com/mgdm/htmlq/issues/65) | 【联网】 | :has() 需新版 selectors（0.26）；旧版 panic | C3 |
| [LSP 3.17](https://microsoft.github.io/language-server-protocol/specification) | 【联网】 | publishDiagnostics/pull 诊断规范 | B1 |
| [Gauge LSP 案例](https://gauge.org/2017/12/07/language-server-protocol-and-dsls) / [dbt LSP](https://www.getdbt.com/blog/language-server-protocol) / [Contextive 实践](https://dev.to/contextive/experiences-building-a-language-server-protocol-plugin-46b0) | 【联网】 | 一 server 通吃编辑器；防抖重分析 | B1、B2 |
| [AOAOSTAR 聚合](https://legado.aoaostar.com) / [Legado 规则说明](https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html) | 【联网】 | 生态源持续更新；JSOUP 方言规范未变 | B3 |
| 【实测】batch_report.json | 【实测】 | 4916/4917 convert；E_SCHEMA 2721 / E_SELECTOR 1735；不可转 Top：content 2341 / kind 1177 / bookUrl 877 | 全部 |
| 【实测】七猫 L3 | 【实测】 | onResponse+ctx.crypto 动态 IV 解密链路成立；第二章 VIP 拒绝为 API 限制 | C1 基准负载取材 |

---

## 5. 验收矩阵

### 5.1 fuzz 目标与判据

| target | 输入构造 | 冒烟时长 | 判据 |
|---|---|---|---|
| `compile_str` | 语料 + arbitrary 变异 | 24h | 零 panic/hang；发现归档 |
| `jsonpath` | 合法路径 + 变异 | 8h | 同上 |
| `xpath` | 合法路径 + 变异 | 8h | 同上 |
| `template_expr` | `${}`/expr 拼接 | 8h | 同上 |
| `convert_full` | 整源文件变异 | 24h | 同上（catch_unwind 外层） |

### 5.2 属性测试清单

| # | 属性 | 输入域 |
|---|---|---|
| P1 | `eval_selector` 任意输入 → Val 或 Diagnostic，永不 panic 且永不返回"无码失败" | 随机选择器串 |
| P2 | 可选字段命中 0 → 主结果保留 + 降级标记 | 随机 map 缺失 |
| P3 | `Diagnostic.severity == code.severity()` 对全 36 码 | 表驱动扫描 |
| P4 | `parse_template` 任意串 → Seg 列表或 E_TEMPLATE（无 panic） | 随机串 |
| P5 | `jsonpath::parse` 任意串 → 同上 | 随机串 |
| P6 | `expr::parse` 任意串 → AST 或 E_EXPR_*（无 panic、深度有限） | 随机串 |
| P7 | `regex::check` 任意串 → Ok 或 E_REGEX/E_REGEX_UNSUPPORTED | 随机串 |
| P8 | 编译产物 bincode 往返 = 结构相等 | 随机合法源 |

### 5.3 LSP 验收场景（B4）

1. 打开 `本地语料（不入库）` → ≤3s 收到诊断（含 W_UNKNOWN_FIELD 计数）
2. 修改 `json:$.data.book.title` 为非法选择器 → 诊断增量更新指向该行
3. hover `var:book.id` → 显示"来源：stages.search.out.map.id（契约注入 S§5.4）"
4. completion 输入 `css:` 之外的裸选择器 → 列出 18 命名空间
5. goto `var:chapter.url` → 定位 `toc.out.map.url`

### 5.4 引擎基准（C5）

| 负载 | 输入 | 判据 |
|---|---|---|
| qmDecrypt | 七猫真实 content（b64→AES-CBC 解密→UTF-8） | QuickJS/Boa 耗时比 ≥1.5 |
| signParams | 4 参数排序拼接 + md5 ×1000 | 同上 |
| cleanTitle | 《》[] 剥离 + trim ×10000 | 同上 |

### 5.5 全量回归门禁（完成任何任务后必须全绿）

```bash
python tools/check.py                  # schema + 5 示例
python tools/consistency.py            # 文档一致性
python prototype/expr_proto.py         # 18+9
cargo test -p rsl-core                 # 31+（含新增属性测试）
rsl test tests/cases/novel-source.rsl.json --cases tests/cases/novel-html.yaml
rsl test examples/aggregate-merge.rsl.json --cases tests/cases/aggregate-merge.yaml
python tools/conformance.py check      # golden 三方一致
```

---

## 6. 风险与未决

### 6.1 已定裁决（V2-D 编号，直接执行）

| # | 裁决 | 依据 |
|---|---|---|
| **V2-D1** | fuzz 语料只入库合规字段子集；站点信息留 scratch（gitignore） | S§17 + H4 |
| **V2-D2** | QuickJS 只做 native 端；wasm 端维持 Boa（wasm32 编译已验证） | H1 + 实测 |
| **V2-D3** | JSONPath 替换保留现有 `E_SELECTOR` 错误码与 path 格式（调用方无感知） | R§11.1 |
| **V2-D4** | LSP 用独立 crate（tokio 只进 rsl-lsp），core 保持同步 | H3 |
| **V2-D5** | lol_html 只做 spike 不直接集成；结论进 C4 报告后再定 | 0.24s 已达标 |

### 6.2 未决（不阻塞 P0）

| # | 未决 | 阻塞 | 何时定 |
|---|---|---|---|
| U1 | QuickJS windows-gnu 绑定可用性 | C1 | spike 后（1 天内出结论） |
| U2 | jsonpath-rust filter 与 `where` 的白名单映射 | C2 | 实现前设计 |
| U3 | scraper 升级破坏面 | C3 | cargo upgrade 后 |
| U4 | 非 Legado 方言（2736 源）第二转换器 | — | 本文不决策（H6） |

### 6.3 风险

| # | 风险 | 概率 | 缓解 |
|---|---|---|---|
| R1 | fuzz 发现大量 panic → 修复挤占工期 | 中 | P0 先审计 unwrap（A4），fuzz 后置发现减半 |
| R2 | QuickJS 双绑定都不可用 | 低 | Boa 保留 + 基准归档，登记结论（不阻塞） |
| R3 | jsonpath-rust API 与手写版语义差（如多命中返回形态） | 中 | 兼容层 + 全量 json: 用例回归 |
| R4 | LSP 工程量超预算 | 中 | P2 项（hover/completion/definition）可裁剪，诊断先上线 |
| R5 | scraper 升级连锁破坏 | 低 | 独立任务 + 全量回归门禁兜底 |

---

## 7. 索引

### 7.1 版本表

| 文档 | 版本 | 状态 |
|---|---|---|
| 本文 | 1.0 | Draft（评审后转 Active） |
| 依赖：TASK-BOOK.md | v2.0 | ✅ 完结（基线） |
| 依赖：RSL-1.0.md | 2.0.0-draft.1 | 不变 |

### 7.2 完成定义（会话收尾自检）

> fuzz 报告归档了吗？属性测试全绿了吗？LSP 冒烟截图有了吗？基准报告归档了吗？全量回归门禁绿了吗？——任何一个"没有"就不算完成。

### 7.3 与 V1 的关系

V1 交付的**端口抽象**（ScriptEngine/Transport/Clock）与**诊断引擎**（analyze 20 条）是本文全部任务的前置——本文是它们的"第二消费者"：换引擎不动内核、LSP 复用诊断、import 复用 convert。这是 V1§1.2 依赖图设计的直接回报。
