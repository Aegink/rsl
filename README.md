<div align="center">

# RSL · Reader Source Language

**一门"阅读源"的描述语言：用一份 JSON 声明怎么从站点取书，由同一个内核在 CLI / iOS / Android / WASM 上执行。**

`RSL 2.0` · Rust 实现 · MIT License · 与阅读（Legado）不兼容——存量源经一次性转换器迁移

[规范](spec/RSL-1.0.md) · [对比阅读](docs/comparison-vs-legado.md) · [迁移手册](spec/MIGRATION.md) · [错误全书](spec/ERRORS.md)

</div>

---

## 为什么是 RSL

给"读书软件如何拿书"这件事一个**可审计、可测试、跨端一致**的规则标准。

| 设计目标 | 含义 | 在规范中的锚点 |
|---|---|---|
| **跨端一致** | 同一份源在四个端给出同一结果：正则限 RE2 子集（无回溯）、选择器语义显式、脚本两档隔离、合并语义写进契约 | §3.4 · §7.1 · §12.4 |
| **失败有码** | 永不返回裸空串。"合法空 / 可降级 / 失败"是三种结果、三种码，附 `path` + `hint` + `sample` | §11 · §11.3 |
| **编译期拦截** | 拼错 `var:book.tocUrl`、在 `node[]` 上跑正则、逐章规则里发请求——都在**加载前**变成错误码 | §13.3 · 附录 D |
| **能力按需付费** | 90% 的源不需要登录/WebView/加解密；它们是可选块，不出现就不占认知 | §4 · §6 |
| **规则是数据** | 可 diff、可 lint、可 fuzz、可回归——这是与"字符串规则"的根本区别 | 全文 |

一句话立场：**不兼容 Legado 规则，也不打算兼容**。那套字符串规则的 8 级前缀优先级与
运行期粘滞解释，正是静默失败与不可静态分析的根源。存量源走 `rsl convert` 一次性迁移
（10,671 真实源实测 99.98% 可转换），对比详见
[**RSL 与阅读（Legado）对比**](docs/comparison-vs-legado.md)。

---

## 5 分钟看懂一个源

小说站，五阶段里的四个。注意三件事：**每个选择器都有前缀**、**`request` 是对象不是字符串**、
**`out` 是唯一出口**。

```jsonc
{ "rsl": "2.0", "id": "novel.example", "name": "示例小说站", "type": "novel",
  "baseUrl": "https://novel.example", "stages": {
    "search": {                                                                  // ① 搜索
      "request": { "url": "var:baseUrl/search", "params": { "q": "var:kw", "pn": "expr:page - 1" } },
      "out": { "emptyOk": true, "list": "css:ul.result > li",                    // emptyOk：没有结果是合法结果
        "map": { "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
                 "url":  { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] } } },
      "paginate": { "kind": "page", "pageVar": "page", "max": 20 } },            // 分页交给引擎，不手拼 URL
    "detail": { "request": { "url": "var:book.url" },                            // ② 详情：var:book 由宿主显式注入
      "out": { "name": "css:h1", "author": "css:.author",
               "tocUrl": { "$": "css:a.read", "attr": "href", "transform": [{ "absolute": true }] } } },
    "toc": { "request": { "url": "var:book.tocUrl" }, "reuseDetailHtml": true,   // ③ 目录：与详情同页则不重复请求
      "out": { "list": "css:#list dd", "map": { "title": "css:a",                // 逐章规则：只允许纯选择器（编译期 E_COST 把关）
               "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] } } } },
    "content": { "request": { "url": "var:chapter.url", "charset": "gbk" },      // ④ 正文：显式解码
      "out": { "content": { "$": "css:div#content",
               "strip": ["css:script", "css:.ad"],                              // 先删广告/脚本节点
               "paragraphs": { "split": "br" } } } }                            // 再拆成段落数组
  }
}
```

| 看到 | 说明 |
|---|---|
| `css:` `var:` `expr:` | 命名空间，共 18 个；漏前缀 = 加载即 `E_SELECTOR` |
| `var:kw` / `var:page` | 上下文变量，宿主注入、只读；`page` 由 `paginate` 提供 |
| `"out"` | 阶段的唯一输出契约；`detail` 拿到的 `var:book` 字段就来自 `search.out.map` |
| 没有 `login` / 没有 `script` | 90% 的源不需要；不出现就不占认知 |

---

## 快速开始

### 构建

```bash
# Rust stable（1.85+，edition 2021）
cargo build --release -p rsl-cli
# 产物：target/release/rsl(.exe) —— 九命令单二进制
```

<details>
<summary>windows-gnu 工具链（本仓开发环境，含 LSP/基准）</summary>

```bash
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu -p rsl-cli
```
注意：MinGW 链接的主线程栈偏小，CLI/LSP 已内置 64MB 工作线程；自建入口遇到
深递归溢出时同样处理即可。

</details>

### 九命令

```bash
rsl lint <src.rsl.json>          # 静态检查（20 条 lint + 编译期诊断，不联网）
rsl compile <src.rsl.json>       # 编译 → IR 摘要
rsl explain <src.rsl.json>       # 编译结论全览（给书源做"体检报告"）
rsl run <src> <stage> [--kw …]   # 执行单个阶段
rsl test <src> --cases <yaml>    # L2 fixture 重放回归（不联网；--watch 增量监听）
rsl record <src> <stage>         # 录制真实响应为 fixture
rsl convert <legado.json>        # Legado → RSL 草稿 + 不可转清单 + 复核清单
rsl import <url-or-file>         # 批量拉取→转换→聚合报告→打包（yuancang / 本地 / 通用 URL）
rsl build / pack                 # 确定性装配 + sha256 manifest（--inline-jslib 构建期内联远程库）/ 打包
rsl check <dir> [--probe]        # 批量 lint（--probe 附连通性探活，不落内容）
rsl fuzz <target> [--minutes n]  # 内置模糊测试（5 个 harness）
```

### 编辑器支持

```bash
cargo build -p rsl-lsp && rsl-lsp   # stdio LSP server
# publishDiagnostics（保存即 lint）/ hover（变量来源契约）/ completion（18 命名空间 + 31 管道步骤）
# definition（var:book.id → 声明处）/ code action（E_REF/E_SELECTOR 确定性快速修复）
```

现成 VS Code 扩展：[`editors/vscode/`](editors/vscode/)（诊断/hover/补全/定义/快速修复 +
snippets + Schema 校验，`npm test` 6 场景协议冒烟）。调试数据面：`rsl run --trace-json`
（`schema/trace.schema.json`，版本 rsl-trace/1）。

---

## 架构

```
crates/
  rsl-core        内核：analyze（编译器/引用图/类型推断/E_COST）→ ir（bincode）
                  → selector（css/xpath/jsonpath 9535）→ exec（求值/ctx 桥/取消）
                  → stage（五阶段编排/分页/登录/合并）→ script（boa / quickjs）
                  → testkit（lint/golden）              —— 同步、无 tokio、无 IO 依赖
  rsl-cli         九命令 + fuzz harness（单二进制）
  rsl-lsp         LSP server（stdio JSON-RPC，std only）
  rsl-ffi         C ABI（rsl.h）
  rsl-wasm        wasm-bindgen（wasm32-unknown-unknown）
bindings/
  ios/            Swift RSLKit
  android/        JNI
```

`ports.rs` 定义 `Transport / WebViewPort / LoginSessionPort / CookieStore / Clock` 端口——宿主注入，内核自身不联网。
分层硬约束：`selector` 不依赖 `http`、`exec` 不依赖 `stage`、`analyze` 只读 `model + ir + expr`。
JS 引擎经 `ScriptEngine` 端口注入：native 默认 QuickJS（基准 QuickJS/Boa = 2.0–3.4×，
语义等价 55/55），wasm 维持 Boa；Boa 回退 `--no-default-features --features engine-js,net`。

---

## 工程质量（实测）

| 项 | 结果 |
|---|---|
| 测试 | `cargo test -p rsl-core` **57 通过**；`--features engine-quickjs` 同套件 **57 通过**（双引擎语义等价） |
| 模糊测试 | 5 harness；compile_str **60 分钟 / 7,134,278 轮零 panic**（结构化变异 + 语料 1022 文件） |
| 属性测试 | 8 条铁律 × 1000 case（永不裸空串 / 降级不清空主结果 / severity↔code 全表一致…） |
| 三端一致性 | golden SHA-256：CLI ↔ WASM 逐字节一致（`tools/conformance.py`） |
| 真实源压测 | 10,671 源：convert 99.98% 可转换；lint 直通 485 源 |
| 真实商业 API | 七猫全链路（md5 签名 + AES-CBC 动态 IV），正文页 ≈0.24s |
| panic 边界 | lib 路径 unwrap/expect 归零 + CLI/LSP 入口 catch_unwind（[审计清单](docs/20261114_A4_panic审计清单.md)） |
| 门禁 | check（schema+示例+fuzz 报告归档）/ consistency（文档计数）/ conformance（golden）/ expr_proto 四道 CI 门禁 |

内核规模：`rsl-core` 15,745 行 Rust（对比阅读内核 ≈27,400 行 Kotlin，见
[对比文档 §7](docs/comparison-vs-legado.md#7-稳定性与性能实测数据)）。

---

## 与阅读（Legado）的关系

**不兼容，也不打算兼容**——但不对立：

- 规则语言重新设计（字符串 → 数据契约），换取编译期检查与跨端一致；
- 存量源经 `rsl convert` 一次性迁移，不可转项逐条登记，绝不静默丢弃；
- 设计上刻意保留的能力差（远程 `jsLib`、`java.*` 不采纳）见
  [`spec/GAP-AUDIT.md`](spec/GAP-AUDIT.md) A1–A44 决策矩阵。

📖 **[RSL 与阅读（Legado）对比](docs/comparison-vs-legado.md)**——同一任务的两种写法、
35+ 项能力对照矩阵、失败语义对比、迁移成本与"何时不该换"的诚实清单。

---

## 文档地图（互斥分工）

| # | 文档 | 什么时候读 |
|---|---|---|
| 1 | [`spec/CONCEPTS.md`](spec/CONCEPTS.md) | **先读这个**——10 个"为什么这么设计" |
| 2 | [`spec/PATTERNS.md`](spec/PATTERNS.md) | 写源时查——任务配方（搜索/目录/正文/分页/登录） |
| 3 | [`spec/REFERENCE.md`](spec/REFERENCE.md) | 查字段——全字段速查 |
| 4 | [`spec/ERRORS.md`](spec/ERRORS.md) | 报错时查——31 错误码 + 5 警告码的成因与修法；宿主文案映射表见 [`spec/ERRORS-MESSAGES.md`](spec/ERRORS-MESSAGES.md) |
| 5 | [`spec/RSL-1.0.md`](spec/RSL-1.0.md) | 写源 / 实现内核必读——**唯一权威**契约正文 |
| 6 | [`spec/MIGRATION.md`](spec/MIGRATION.md) | 从 Legado 迁——24 字段映射 + 22 条语法翻译 + 不可转清单 |
| 7 | [`docs/comparison-vs-legado.md`](docs/comparison-vs-legado.md) | 选型评估——与阅读的全面对比 |
| 8 | [`spec/COMPILER.md`](spec/COMPILER.md) | 实现者——编译流水线、引用图、类型推断、E_COST |
| 9 | [`spec/EXPRESSIONS.md`](spec/EXPRESSIONS.md) | 实现者——`expr:` 子语言文法与内置函数 |
| 10 | [`spec/GAP-AUDIT.md`](spec/GAP-AUDIT.md) | 改规范前——与现网引擎差距矩阵（A1–A44）与取舍 |
| 11 | [`spec/KERNEL-COMPARE.md`](spec/KERNEL-COMPARE.md) | 关心架构——与阅读内核逐层对比（源码行号证据） |
| 12 | [`spec/TASK-BOOK.md`](spec/TASK-BOOK.md) / [`spec/TASK-BOOK-2.md`](spec/TASK-BOOK-2.md) / [`spec/TASK-BOOK-3.md`](spec/TASK-BOOK-3.md) | 派工史——V1 基线（M1–M6）、V2 增强（A/B/C 三方向）、V3 增补（登录会话/机械重登/编辑器/受控远程/修复建议） |
| 13 | [`spec/ROADMAP.md`](spec/ROADMAP.md) | 路线——里程碑与验收口径 |
| 14 | [`spec/INDUSTRY-RESEARCH.md`](spec/INDUSTRY-RESEARCH.md) | 追溯依据——业界调研原始记录（非规范） |
| 15 | [`schema/rsl.schema.json`](schema/rsl.schema.json) | 编辑器 / CI——JSON Schema 2020-12（29 `$defs`） |
| 16 | [`examples/`](examples/) | 五个可校验示例（小说/漫画/登录视频/聚合合并/边界能力） |
| 17 | [`docs/`](docs/) | 工程记录——审计清单、spike 报告、基准报告、交接文档 |

文档分工互斥：想学 → `CONCEPTS`；想写 → `PATTERNS`；想查 → `REFERENCE`；报错 → `ERRORS`；
想知道为什么 → `GAP-AUDIT` 与 `KERNEL-COMPARE`；想知道与阅读差在哪 → `comparison-vs-legado`。
`RSL-1.0.md` 只回答"合不合法、引擎必须怎么做"。

---

## 安全与合规边界

1. 本仓库**不包含任何书源、内容或站点信息**；示例与测试语料全部使用 `example.com` 虚构域名。
2. 引擎是中立的 HTTP + 解析工具，性质等同 `curl` + 解析库。
3. 不提供公共服务实例、不做内容聚合、不绕付费墙。
4. 源由使用者自行编写与导入，合规责任在使用者。
5. 分发层只发程序，不发源，不发公共实例。

> 背景：Legado 主仓库 2026-05 因侵权被清空，Tachiyomi 扩展体系反复被 DMCA 清洗。
> 架构上内容与软件彻底分离、分发上绝不集中托管源，是这套标准的前提条件，不是可选项。

---

## 贡献

见 [CONTRIBUTING.md](CONTRIBUTING.md)。改动必须通过四道门禁：

```bash
python tools/check.py               # schema + 示例 + fuzz/基准归档存在性
python tools/consistency.py         # 文档计数对齐（S↔Z↔E↔R）
python tools/conformance.py check   # 三端 golden
cargo test -p rsl-core              # 57 测试（--features engine-quickjs 亦须全绿）
```

## License

[MIT](LICENSE)
