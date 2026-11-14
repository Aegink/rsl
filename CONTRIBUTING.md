# 贡献指南

感谢关注 RSL。本仓是"规范 + 参考实现"，贡献前请先读
[`spec/CONCEPTS.md`](spec/CONCEPTS.md)（设计心智模型）与
[`spec/GAP-AUDIT.md`](spec/GAP-AUDIT.md)（哪些能力**明确不采纳**及理由）。

## 仓库边界（先读）

- **不发源、不发内容**：任何 PR 不得包含真实站点地址、书源文件、抓包数据；
  测试一律使用 `example.com` 虚构域名的 fixture。
- **跨端一致优先于方便**：依赖宿主运行时行为的能力不得进标准（见 `RSL-1.0.md` §1.2）。
- **失败必须可区分**：新能力必须定义完整的错误/降级语义，不接受静默空。

## 开发环境

```bash
cargo build -p rsl-cli -p rsl-lsp
cargo test -p rsl-core                              # 57 测试
cargo test -p rsl-core --features engine-quickjs    # 双引擎同套件
```

可选：nightly 工具链用于 fuzz 变体实验；JS 引擎基准在
`crates/rsl-core/benches/engine_bench.rs`。

## 四道门禁（任何改动必须全绿）

```bash
python tools/check.py               # Schema 自洽 + 示例 + fuzz/基准归档存在性
python tools/consistency.py         # 文档计数对齐（错误码/步骤键/命名空间/lint 条数）
python tools/conformance.py check   # 三端 golden（CLI ↔ WASM 逐字节一致）
cargo test -p rsl-core              # 全部测试（含 properties.rs 属性测试）
```

改 `spec/` 文档时注意：错误码、管道步骤键、命名空间、lint 条数在四份文档间是**计数对齐**的，
加一项要同步 `RSL-1.0.md` / `ERRORS.md` / `REFERENCE.md` / `schema/rsl.schema.json`
（`tools/consistency.py` 会替你把关）。

## 提交规范

- commit message：`YYYYMMDD_<主题>`（与本仓历史一致）。
- 涉及语言契约的改动（`RSL-1.0.md`）：先在 issue/讨论里给动机与跨端影响分析；
  重大语义变更需更新 `GAP-AUDIT.md` 的对应决策。
- 新增错误码：`error.rs` → `ERRORS.md` §1 → Schema → `consistency.py` 四处同步 + 属性测试
  `p3_severity_table_consistent` 的 36 码表（现为 36，加码后同步数量断言）。
- 新增 CLI 命令：`main.rs` 分发 + `print_help` + 本 README 命令表。

## 分层红线

`crates/rsl-core` 内：

- `selector` 不得依赖 `http`；
- `exec` 不得依赖 `stage`；
- `analyze` 只读 `model + ir + expr`；
- `rsl-core` 保持同步、无 tokio、无进程内网络调用（全部经 `ports.rs` 端口注入）；
  tokio 只允许出现在 `rsl-lsp`（当前为 std 实现，未使用）。
- lib 路径禁止 `unwrap()/expect()/panic!`（A4 门禁）；宿主入口用 `catch_unwind` 兜底。

## 报告问题

- 行为不符规范：附最小 `rsl.json` + `rsl explain` 输出（`example.com` 域名）。
- panic：本仓 CLI/LSP 已包 `catch_unwind` 并输出 `E_PANIC`；附触发输入与命令行。
