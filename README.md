# RSL · Reader Source Language

> **RSL 是一门"阅读源"的描述语言**：用一份 JSON 说明怎么从**一个站点**取书——搜索、详情、目录、正文——由同一个内核在 iOS / Android / WASM / CLI 上执行。
> 与 Legado **不兼容**，也不打算兼容：存量源通过一次性转换器迁移。
> 当前版本 **2.0**（Draft），规范正文见 [`spec/RSL-1.0.md`](spec/RSL-1.0.md)。

| 卖点 | 含义 | 证据 |
|---|---|---|
| **跨端一致** | 同一份源在四个平台给出同一结果：正则限 RE2 子集、选择器语义显式、脚本只有两档、合并语义写进契约 | §3.4 · §7.1 · §12.4 |
| **失败有码** | 永不返回裸空串。"合法空 / 可降级 / 失败"是三种结果、三种码，附 `path` + `rule` + `hint` + `sample` | §11 · §11.3 |
| **编译期拦截** | 拼错 `var:book.tocUrl`、在 `node[]` 上跑正则、逐章规则里发请求，都在**加载前**变成错误码 | §13.3 · 附录 D |

---

## 5 分钟看懂一个源

小说站，四个阶段全在。注意三件事：**每个选择器都有前缀**、**`request` 是对象不是字符串尾巴**、**`out` 是唯一出口**。

```jsonc
{ "rsl": "2.0", "id": "novel.example", "name": "示例小说站", "type": "novel",   // 必填：版本 / 标识 / 名字 / 默认载体
  "baseUrl": "https://novel.example", "stages": {                             // 相对地址基准
    "search": {                                                                 // ① 搜索
      "request": { "url": "var:baseUrl/search", "params": { "q": "var:kw", "pn": "expr:page - 1" } },
      "out": { "emptyOk": true, "list": "css:ul.result > li",                   // emptyOk：没有结果是合法结果
        "map": { "name": { "$": "css:a.title", "transform": [{ "trim": true }] },  // → 条目字段 name
                 "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] } } },  // → 条目 url
      "paginate": { "kind": "page", "pageVar": "page", "max": 20 } },            // 分页交给引擎，不手拼 URL
    "detail": { "request": { "url": "var:book.url" },                           // ② 详情：var:book 由宿主显式注入
      "out": { "name": "css:h1", "author": "css:.author",                       // 字符串 = 省略 "$" 的快捷写法
               "tocUrl": { "$": "css:a.read", "attr": "href", "transform": [{ "absolute": true }] } } },  // 目录地址≠详情地址是常态
    "toc": { "request": { "url": "var:book.tocUrl" }, "reuseDetailHtml": true,   // ③ 目录：与详情同页则不重复请求
      "out": { "list": "css:#list dd", "map": { "title": "css:a",                // 逐章规则：只允许纯选择器
               "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] } } } },
    "content": { "request": { "url": "var:chapter.url", "charset": "gbk" },      // ④ 正文：响应解码用 charset
      "out": { "content": { "$": "css:div#content",                             // var:chapter 由宿主注入
               "strip": ["css:script", "css:.ad"],                              // 先删广告/脚本节点
               "paragraphs": { "split": "br" } } } }                            // 再拆成段落数组
  }
}
```

读这份文件时值得停一下的四个点：

| 看到 | 说明 |
|---|---|
| `css:` `var:` `expr:` | 命名空间，没有"无前缀默认解释"；漏了前缀 = `E_SELECTOR` |
| `var:kw` / `var:page` | 上下文变量，宿主注入、只读；`page` 由 `paginate` 提供 |
| `"out"` | 阶段的唯一输出契约；`detail` 拿到的 `var:book` 字段就来自 `search.out.map` |
| 没有 `script` / 没有 `requires` | 90% 的源不需要脚本和额外能力；不出现就不占认知 |

---

## 现在能跑什么

```bash
# 文档层（无第三方依赖）
python tools/check.py                # Schema 自洽性 + 示例一致性（5 个示例）
python tools/consistency.py          # 文档一致性门禁（S↔Z↔E↔R 计数对齐）
python prototype/expr_proto.py       # expr: 原型（18 求值 + 9 编译期）

# Rust 内核（.toolchain/cargo，rustc 1.100 gnu）
cargo test -p rsl-core               # 27 个测试：M1 编译器 18 + L2 执行链 4 + expr 2 + 探针 3

# CLI 九命令（target/debug/rsl.exe）
rsl lint examples/novel-html.rsl.json
rsl compile examples/video-login.rsl.json
rsl test tests/cases/novel-source.rsl.json --cases tests/cases/novel-html.yaml      # 7/7
rsl test examples/aggregate-merge.rsl.json --cases tests/cases/aggregate-merge.yaml # 2/2
rsl convert <legado.json> --out converted    # Legado 迁移草稿 + 报告
python tools/conformance.py check            # 三端一致性 golden（CLI + WASM 编译级）
```

> Rust 工具链装在工作区 `.toolchain/`（不入库）：`export CARGO_HOME=$PWD/.toolchain/cargo RUSTUP_HOME=$PWD/.toolchain/rustup PATH=$PWD/.toolchain/cargo/bin:$PATH`

```bash
python prototype/expr_proto.py  # expr: 原型：18 条求值 + 9 条编译期拦截
```
```
--- 编译期拦截 ---
OK   pagee - 1                -> E_REF
OK   md5('x')                 -> E_UNSUPPORTED
OK   a.b                      -> E_EXPR_SYNTAX
求值 18/18 通过；编译期用例 9/9 符合预期
```

**Rust 内核已实现**（M1–M6 全部落地，见下方「当前状态」）：`crates/`（rsl-core / rsl-cli / rsl-ffi / rsl-wasm）、`bindings/`（iOS RSLKit / Android JNI）、`tests/`（l2_smoke / m1_compile / m4_expr / conformance）。`rsl lint` / `rsl compile` / `rsl run` / `rsl test` / `rsl convert` / `rsl record` / `rsl build` / `rsl pack` / `rsl explain` 九命令全部可用。

> `examples/` 五个源都已声明 `rsl: "2.0"`，可对照 §附录 A 的说明逐个阅读。实现层裁决（D1–D11）登记于 `docs/handoff/handoff_2026-11-14_02-50.md`。

---

## 文档地图（按"先读哪个"排序）

| # | 文档 | 什么时候读 | 内容 | 状态 |
|---|---|---|---|---|
| 1 | [`spec/CONCEPTS.md`](spec/CONCEPTS.md) | **先读这个** | 心智模型：10 个"为什么这么设计"，不抄契约细节 | ✅ |
| 2 | [`spec/PATTERNS.md`](spec/PATTERNS.md) | 写源时查 | 任务配方：搜索 / 目录 / 正文 / 分页 / 登录的标准写法 | ✅ |
| 3 | [`spec/REFERENCE.md`](spec/REFERENCE.md) | 查字段时查 | 全字段速查表 | ✅ |
| 4 | [`spec/ERRORS.md`](spec/ERRORS.md) | 报错时查 | 错误全书：31 个错误码的成因与修法 + 5 个警告码 | ✅ |
| 5 | [`spec/RSL-1.0.md`](spec/RSL-1.0.md) | 写源 / 实现内核必读 | **唯一权威**。契约正文 v2.0：数据模型、取值、请求、状态、五阶段、脚本、预算、能力、错误、执行、工具链 | ✅ |
| 6 | [`spec/MIGRATION.md`](spec/MIGRATION.md) | 从 Legado 迁 | 24 字段映射 + 22 条语法翻译 + 12 条行为差异 + 15 条不可转项 | ✅ |
| 7 | [`spec/COMPILER.md`](spec/COMPILER.md) | 实现者 | 编译流水线、引用图、类型推断、`E_COST` 成本分析、步骤图校验、IR、模块拆分 | ✅ |
| 8 | [`spec/EXPRESSIONS.md`](spec/EXPRESSIONS.md) | 实现者 | `expr:` 子语言：文法、类型、内置函数（含加密族）、检查清单 | ✅ |
| 9 | [`spec/TASK-BOOK.md`](spec/TASK-BOOK.md) | **派工 / 验收** | 施工图：58 条能力 × 九维交叉验证矩阵、T1–T6 任务分解、前置文档全量落位、缺口 G-01–G-12 | ✅ |
| 10 | [`spec/GAP-AUDIT.md`](spec/GAP-AUDIT.md) | 改规范前先看 | 与现网引擎的差距矩阵（A1–A44）、取舍决定、明确不采纳的设计 | ✅ |
| 11 | [`spec/KERNEL-COMPARE.md`](spec/KERNEL-COMPARE.md) | 关心架构的人 | 内核级逐层对比：Rimchars Archive v15（1742 个 `.kt` / 38.7 万行）vs RSL，58 条差异 + 源码行号 + 12 处 Legado 更强的地方 | ✅ |
| 12 | [`spec/ROADMAP.md`](spec/ROADMAP.md) | 所有人 | 里程碑 M1–M6 与验收口径（计划，不是规范） | ✅ |
| 13 | [`spec/INDUSTRY-RESEARCH.md`](spec/INDUSTRY-RESEARCH.md) | 追溯依据 | 业界调研原始记录，**非规范**；与正文冲突以 `RSL-1.0.md` 为准 | ✅ |
| 14 | [`schema/rsl.schema.json`](schema/rsl.schema.json) | 编辑器 / CI | JSON Schema 2020-12，29 个 `$defs`，根级 `additionalProperties: true` | ✅ |
| 15 | [`examples/`](examples/) | 想看完整源 | 五个可校验示例（HTML 小说 / JSON 漫画 / 带登录的视频 / 聚合合并 / 边界能力 where·zip·parse·stopIf），域名为虚构地址 | ✅ |

文档之间的分工是**互斥**的：想学 → `CONCEPTS.md`；想写 → `PATTERNS.md`；想查 → `REFERENCE.md`；报错 → `ERRORS.md`；想知道为什么 → `GAP-AUDIT.md` 与 `KERNEL-COMPARE.md`。`RSL-1.0.md` 只回答"合不合法、引擎必须怎么做"。

---

## 设计公理

1. **规则是数据。** 可 diff、可 lint、可回归、可被程序断言。
2. **没有隐式通道。** 阶段之间只通过 `out` 契约传递；不存在"第一个结果自动变成 `var:book`"。
3. **失败必须可区分。** "确实为空""可降级""失败"是三种结果，三种错误码。
4. **跨端一致优先于方便。** 任何依赖宿主语言运行时的能力都不得进标准。
5. **能力按需付费。** 90% 的源不需要登录/WebView/媒体解密；它们必须是可选块，不出现就不占认知。
6. **凡是被证明有用的工程优化，必须进标准。** 详情→目录响应复用、占位保护、页码枚举——这些不是"实现细节"，是作者会反复重新发明的东西。

---

## 当前状态（v2.0）

| 层 | 状态 |
|---|---|
| 语言契约 v2.0 | ✅ 完成（`login.steps[]`、`merge`、`script.onResponse`、`paginate.enumerate`、`toc.reuseDetailHtml`、`content.direct`、`protect`、`E_COST`） |
| JSON Schema | ✅ 完成（2020-12，29 个 `$defs`，四个示例全部通过校验） |
| 编译器设计 | ✅ 完成（引用图、类型推断、`E_COST`、feature/算法检查、`lint` 20 条） |
| `expr:` 原型 | ✅ 完成（Python 原型：求值 + 编译期拦截） |
| Rust 内核 | ✅ 完成（M1–M4：编译器/传输/执行器/脚本表达式，`crates/rsl-core` 27 测试全绿；boa 引擎两档 pure/ctx） |
| 工具链 | ✅ 完成（M5：CLI 九命令、`rsl test` YAML 用例 9/9、fixture 录制、convert 迁移、文档一致性门禁） |
| 绑定 | ✅ 完成（M6：C ABI `rsl.h` + Swift RSLKit + Android JNI + WASM `run_stage`；conformance golden 哈希一致） |
| 编辑器 | ⬜ 未开始（M6 编辑器属 ROADMAP 后续，非任务书范围） |

---

## 法律与分发边界

1. 本仓库**不包含任何书源、内容或站点信息**，示例域名为虚构地址。
2. 引擎是中立的 HTTP + 解析工具，性质等同 `curl` + 解析库。
3. 不提供公共服务实例、不做内容聚合、不绕付费墙。
4. 源由使用者自行编写与导入，合规责任在使用者。
5. 分发层只发程序，不发源，不发公共实例。

> 背景：Legado 主仓库 2026-05 因侵权被清空，Tachiyomi 扩展体系反复被 DMCA 清洗。架构上内容与软件彻底分离、分发上绝不集中托管源，是这套标准的前提条件，不是可选项。
