---
id: rsl-comparison
type: guide
status: active
title: RSL 与阅读（Legado）对比
parent: rsl-language
tags: [rsl, comparison, legado, architecture, migration]
---

# RSL 与阅读（Legado）对比

> 本文面向**书源作者与使用者**：两种方案在同一件事上的写法差异、能力差异、工具链差异，
> 以及"该不该换"的诚实清单。
> 实现级逐层对比（含 Legado 源码行号证据）见 [`spec/KERNEL-COMPARE.md`](../spec/KERNEL-COMPARE.md)；
> 字段级迁移映射见 [`spec/MIGRATION.md`](../spec/MIGRATION.md)。三者分工互斥。

**对比口径**（2026-11-14 实测）

| 侧 | 对象 | 证据 |
|---|---|---|
| 阅读 | Legado（Rimchars Archive v15，内核 ≈27,400 行 Kotlin） | `KERNEL-COMPARE.md` §1.1，源码行号可查 |
| RSL | 本仓库 v2.0（内核 `rsl-core` 15,745 行 Rust + CLI/LSP/绑定，合计 20,722 行） | `crates/` 实测 `wc -l`；103+103 双引擎测试 |

RSL **不是 Legado 的 fork，也不做规则兼容**：规则语言重新设计，存量源经一次性转换器迁移
（`rsl convert`，10,671 真实源实测 99.98% 可转换）。两者可以长期共存。

---

## 1. 一张表看完

| 维度 | 阅读（Legado） | RSL |
|---|---|---|
| 规则形态 | 纯字符串（`String?`），8 级前缀 + 运行期 `isJSON` 粘滞解释 | 结构化 JSON，选择器强制命名空间前缀，Schema 2020-12 校验 |
| 编译期检查 | 无（加载后首次运行才发现拼错） | 编译期拦截：引用笔误 / 类型错误 / 逐章成本 / 算法声明，全部 `E_*` 码 |
| 失败语义 | 静默失败为主（空列表 / 空章节，原因不可知） | 三分语义：合法空（`E_EMPTY`）/ 可降级（`W_DEGRADED`）/ 失败（`E_*` + path + hint） |
| 脚本 | Rhino，120 个 `java.*` 宿主函数，可触 UI/文件/网络 | 两档隔离（pure 无 IO 无时间 / ctx 十接口白名单），函数白名单，libSteps 计量 |
| 跨端 | Android（Kotlin 内核 + WebView 绑定 93 处注入） | 一份内核四个端：CLI / iOS（Swift）/ Android（JNI）/ WASM，golden 三端逐字节一致 |
| 正则 | Java `Pattern`（含回溯，可被打挂） | RE2 子集（无回溯，线性时间），反向引用编译期拒绝 |
| 源格式审计 | 字符串拼接，难以静态分析 | 规则是数据：可 diff / 可 lint / 可 fuzz / 可回归 |
| 工具链 | App 内调试日志 | 13 个 CLI 命令 + 20 条 lint + 属性测试 + fuzz 713 万轮零 panic + 三端 golden |
| 生态存量 | 海量现网源 + 教程 | 一次性 `rsl convert`（实测 4916/4917 文件可转换）+ 迁移手册 |

---

## 2. 设计哲学：字符串规则 vs 数据契约

**阅读**把规则设计成"尽可能地短"：`class.title@text`、`tag.a.0@href`、`{{baseUrl}}` 混在
一条字符串里，靠 8 级前缀优先级（`@` → `##` → `.` → JSON → `:` → 正则 → `{{}}` → 纯文本）
在**运行时**猜意图。好处是手写快；代价是：

- 同一行字符串换个字段，语义就变（`isJSON` 粘滞状态）；
- 拼错一个字段名要到运行期才暴露，且大多表现为"静默拿不到数据"；
- 规则不可静态分析——这也是它无法被 lint/fuzz 的根本原因。

**RSL** 把规则设计成"尽可能地明"：每个选择器必有命名空间前缀（`css:` `json:` `re:` `var:` `expr:`…），
每阶段输出固定为 `out` 契约，阶段之间只经 `out` 显式传递变量。手写略长，但换来：

- **加载前**报错：`var:book.urll`（拼错）→ `E_REF`；`node[]` 上跑正则 → `E_TYPE`；逐章规则里发请求 → `E_COST`；
- 规则是数据：能被 Schema 校验、被 20 条 lint 检查、被 fuzz 713 万轮、被三端 golden 断言；
- 阶段契约即文档：`detail` 能拿到什么，读 `search.out.map` 就知道。

---

## 3. 同一任务的两种写法

### 3.1 搜索 + 分页

**阅读（ruleSearch）**——规则字符串 + 手拼分页：

```json
{
  "bookSourceUrl": "https://demo.example",
  "searchUrl": "https://demo.example/search?q={{key}}&p={{page}}",
  "ruleSearch": {
    "bookList": "class.result@tag.li",
    "name": "tag.a@text",
    "bookUrl": "tag.a@href"
  }
}
```

**RSL（stages.search）**——契约化：

```jsonc
{
  "stages": {
    "search": {
      "request": { "url": "var:baseUrl/search", "params": { "q": "var:kw" } },
      "out": {
        "list": "css:ul.result > li",
        "map": {
          "name": { "$": "css:a.title", "transform": [{ "trim": true }] },
          "url":  { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] }
        },
        "emptyOk": true            // 没有结果是合法结果，不报错
      },
      "paginate": { "kind": "page", "pageVar": "page", "max": 20,
                    "stopIf": { "empty": "out.list" } }   // 终止条件写进契约，不会无限翻页
    }
  }
}
```

差异点：分页从"URL 模板里的 `{{page}}`"变成 `paginate` 块（四种模式：page/next/cursor/enumerate），
终止条件显式；`url` 字段的相对解析、`absolute` 转换都进编译期类型推断。

### 3.2 带签名的接口（Legado 需要 `@js:` 拼一长串）

**阅读**——在 `searchUrl` 里嵌 JS（`@js:` 开头整段当脚本执行，依赖 `java.*`）：

```
@js:var key = String(key); java.md5Encode('id=' + key + 'salt=xxx') + '&kw=' + key
```

**RSL**——签名进 `expr:`，算法必须声明（能力按需付费）：

```jsonc
{
  "requires": { "crypto": ["md5"] },
  "stages": {
    "search": {
      "request": {
        "url": "var:baseUrl/api/search",
        "params": { "sign": "expr:md5('id=' + var:kw + 'salt=xxx')", "kw": "var:kw" }
      }
    }
  }
}
```

漏写 `"crypto": ["md5"]` → 编译期 `E_UNSUPPORTED`，不是运行期 `undefined`。

### 3.3 正文清洗（广告剥离 + 段落）

**阅读**：`ruleContent.content` 一个字符串，广告清理靠 `replaceRegex` 字符串正则（Java 回溯语义）。

**RSL**：

```jsonc
"out": {
  "content": {
    "$": "css:div#content",
    "strip": ["css:script", "css:.ad"],       // DOM 级剥除，先于文本提取
    "protect": ["keep-me"],                    // 管道期间保护占位（避免误清洗）
    "paragraphs": { "split": "br", "minLength": 1 }
  }
}
```

`strip` 是真实 DOM 操作（先删节点再取文本），`paragraphs` 四档切分（br/block/line/auto），
`replace`/`html` 等管道步骤在纯文本上运行——顺序在契约里，不靠约定。

### 3.4 多源合并（阅读无内置方案）

阅读在 UI 层做三层三键去重（源内 `bookUrl` → 跨源 `name+author` → DB 主键），键与逻辑散在
`SearchModel`/DB 里，源作者无法声明。

RSL 把合并写进契约：

```jsonc
"merge": {
  "key": ["name", "author"],       // 折叠键（跨桶）
  "crossBucket": true,
  "sort": [{ "by": "sources", "order": "desc" }, { "by": "weight", "order": "desc" }]
}
```

`rsl test` 可以用 YAML 断言直接验证合并行为（本仓 `tests/cases/aggregate-merge.yaml`）。

---

## 4. 能力对照矩阵

> ✅ 支持 ｜ ⚠️ 有但语义受限 ｜ ❌ 无（或明确不采纳）

| 能力 | 阅读 | RSL | 说明 |
|---|---|---|---|
| 搜索/发现/详情/目录/正文 五阶段 | ✅ | ✅ | 阶段名固定 + `hostStages` 扩展位 |
| 分页：URL 模板 | ✅ | ✅ | `paginate.kind=page` |
| 分页：next 链接 | ✅ | ✅ | `kind=next`（选择器取下一页地址） |
| 分页：游标 | ✅（JS 手写） | ✅ | `kind=cursor` 进契约 |
| 分页：枚举拼接 | ✅（JS 手写） | ✅ | `kind=enumerate` + `joinWith` 一次拼接 |
| 分页终止条件 | ⚠️ 靠 max/手写 JS | ✅ | `stopIf.empty` / `stopIf.noNew` 契约化 |
| 无限翻页保护 | ⚠️（搜索无分页源可无限） | ✅ | `limits.maxPages` 硬上限 |
| 登录：表单 | ✅ | ✅ | `login.fields + submit` |
| 登录：多步（验证码/短信） | ✅ | ✅ | `login.steps[]` + `next.when` 步骤机（环检测编译期） |
| 登录：WebView | ✅ | ⚠️ | `login.type=qr` 经 `WebViewPort` 端口；路径由宿主实现 |
| token/cookie 自动产出 | ✅ | ✅ | `submit.token / submit.cookie` |
| 失效自动重登 | ❌（阅读也无） | ⚠️ | `requiresLogin + check`；`E_LOGIN_REQUIRED` 明确报错 |
| JS 脚本 | ✅ Rhino + 120 `java.*` | ⚠️ 两档白名单 | 能力面收窄是**故意的**（可审计、可跨端）；`java.*` 不采纳 |
| 对称加解密 | ✅（JS 里手写或 java.*） | ✅ | `ctx.crypto.sym`（aes-cbc/ctr、des、aes-gcm）+ `requires.crypto` 声明 |
| 摘要/HMAC/Base64 | ✅ | ✅ | `expr:md5/sha1/sha256/hmac` + `base64/hex` 步骤 |
| 简繁转换 | ✅（源自带字表） | ✅ | `t2s/s2t` 步骤，OpenCC 全表内置（4148+498 / 4012+49277 行） |
| 数字章节归一 | ✅（JS 手写） | ✅ | `numChapter/cjkNumber` 内置（"第一百二十三章" → 123） |
| 字体反爬 | ✅（JS 手写映射） | ✅ | `font` 步骤 + `typeMap` |
| 多源合并 | ❌（UI 层硬编码） | ✅ | `merge` 契约（键/跨桶/排序） |
| 章节排序自动判向 | ✅ | ✅ | `toc.order=auto` |
| 详情→目录同页复用 | ❌（重复请求） | ✅ | `toc.reuseDetailHtml`（实测真实站点省 1 请求/章） |
| 正文分页拼接 | ✅ | ✅ | `out.nextPage` 自动循环（受 `maxPages`） |
| 占位保护 | ❌ | ✅ | `protect`（replace 期间防误清洗，`E_PROTECT_LOST` 可诊断） |
| 取消执行 | ⚠️（App 层协程取消） | ✅ | 协作式 CancelToken，边界级 partial + `E_CANCELLED` |
| 响应钩子 | ✅（`loginCheckJs` 等） | ✅ | `script.onResponse`（编译期禁止递归 http，`E_SCRIPT`） |
| 编码 | ⚠️（GBK 支持但 charset 兼容陷阱多） | ✅ | `request.charset` 显式（encoding_rs，测试覆盖 GBK fixture） |
| UTF-16 站点 | ✅ | ✅ | `charset` 显式声明 |
| CSS4 `:has()/:is()` | ✅（Jsoup） | ✅ | scraper 0.29，forgiving 处理 |
| JSONPath | ✅（JS 手写或 `$.`） | ✅ | RFC 9535：手写子集 + jsonpath-rust 兜底 |
| XPath | ✅（Jsoup） | ✅ | 子集：`//`、`[@attr]`、`[n]`、`text()` |
| Schema 校验 | ❌ | ✅ | JSON Schema 2020-12（32 `$defs`） |
| 静态 lint | ❌ | ✅ | 20 条规则（`rsl lint`） |
| 模糊测试 | ❌ | ✅ | 5 harness；compile_str 60min/713 万轮零 panic |
| 属性测试 | ❌ | ✅ | proptest 8 条铁律 |
| 三端一致性断言 | ❌ | ✅ | golden SHA-256：CLI ↔ WASM 逐字节一致 |
| 规则编辑器 | ✅（App 内） | ⬜ | ROADMAP 后续（LSP 已提供诊断/补全/跳转） |
| 源内远程依赖 | ✅（远程 `jsLib`） | ❌ | `GAP-AUDIT` §16 明确不采纳：自包含、可审计、可离线 |

---

## 5. 失败语义：这一条值得单独说

阅读的典型失败：**书源不返回结果，界面显示"搜索无结果"，原因不可知**——可能是规则失效、
站点改版、风控页、编码错、脚本抛异常。作者只能在 App 里打日志逐项排查。

RSL 的失败是一张表（[`spec/ERRORS.md`](../spec/ERRORS.md)，31 错误码 + 5 警告码）：

| 场景 | 阅读的表现 | RSL 的表现 |
|---|---|---|
| 选择器拼错命名空间 | 规则当纯文本，静默空结果 | 加载即 `E_SELECTOR`（附 path + 最近似提示） |
| 引用了不存在的变量 | 运行期 undefined，静默 | 编译期 `E_REF`（列出合法 id） |
| 站点改版列表消失 | 空列表 | `E_NO_MATCH` / `E_EMPTY` 三分（emptyOk 声明是否合法空） |
| 正则被恶意输入打挂 | 卡顿/ANR | RE2 线性时间，无回溯，不可能指数爆炸 |
| 脚本死循环 | 卡顿 | libSteps 计量 + 指令中断 → `E_BUDGET` |
| 逐章规则里发请求 | 隐式可行（性能坑） | 编译期 `E_COST` 拒绝 |
| 可降级字段缺失 | 静默空 | `W_DEGRADED` 警告 + 主结果保留（铁律：降级不清空主结果） |

`rsl explain <src>` 能把一个源的全部编译期结论打出来——相当于给书源配了"体检报告"。

---

## 6. 工具链对比

| 环节 | 阅读 | RSL |
|---|---|---|
| 写源 | App 内文本框 + 调试日志 | 任意编辑器（JSON Schema 校验）+ `rsl-lsp`（诊断/补全/hover/跳转） |
| 静态检查 | 无 | `rsl lint`（20 条）+ 编译期 36 类诊断 |
| 单源回归 | 手动点 | `rsl test src.rsl.json --cases cases.yaml`（L2 fixture 重放，不联网） |
| 批量迁移 | — | `rsl convert`（+ migration.json 不可转清单）|
| 批量导入 | 手动 | `rsl import <url-or-file>`（拉取→转换→聚合报告→打包） |
| 打包分发 | 手动分享 JSON | `rsl build`（确定性装配 + sha256 manifest）/ `rsl pack` |
| 录制测试夹具 | — | `rsl record`（真实响应 → fixture） |
| 解释器 | — | `rsl explain`（编译结论全量输出） |
| 模糊测试 | — | `rsl fuzz <target>`（内置 5 harness） |
| CI 门禁 | — | check/consistency/conformance/expr_proto + fuzz 报告存在性 |

---

## 7. 稳定性与性能（实测数据）

| 实验 | 结果 | 出处 |
|---|---|---|
| 真实商业 API 端到端（md5 签名 + AES-CBC 动态 IV 解密） | 全链路跑通，正文页 ≈0.24s | `docs/20261115_开源前全量真源压测报告.md` |
| 10,671 真实书源批量压测 | convert 成功 4,916/4,917 文件（99.98%）；lint 直通 485 源 | `docs/20261115_开源前全量真源压测报告.md` |
| fuzz（compile_str，含结构化变异） | 60 分钟 / 7,134,278 轮 / 零 panic | `fuzz 报告归档（L-infra 产物）` |
| 属性测试 | 8 条铁律 × 1000 case 全绿 | `crates/rsl-core/tests/properties.rs` |
| JS 引擎基准（qmDecrypt/signParams/cleanTitle） | QuickJS = Boa 的 2.00× / 2.04× / 3.44× | `docs/20261114_C5_engine_bench报告.md` |
| 双引擎语义等价 | 55/55 同套件全绿（Boa 与 QuickJS） | `cargo test -p rsl-core --features engine-quickjs` |
| panic 边界审计 | lib 路径 unwrap/expect 36 → 0；CLI 入口 catch_unwind | `docs/20261114_A4_panic审计清单.md` |
| lol_html 流式 spike（1061KB 页） | 流式 2.3–3.1× 于 DOM；暂不集成（DOM 语义优先） | `docs/20261114_C4_lolhtml_spike.md` |

**内核规模对比**：阅读 ≈27,400 行 Kotlin（196 文件，登录编排 816 行在 UI 层无法剥离）；
RSL `rsl-core` 15,745 行 Rust（12 模块分层，exec 不依赖 stage、selector 不依赖 http）。

---

## 8. 迁移路径与成本

```bash
rsl convert <legado.json> --out converted        # 尽力转换，产三件套：
#   <id>.rsl.json       —— 转换草稿
#   <id>.migration.json —— 不可转/降级清单（逐条带原因）
#   <id>.checklist.md   —— 人工复核清单
rsl lint converted/<id>.rsl.json                  # 静态检查
rsl explain converted/<id>.rsl.json               # 编译结论全览
```

实测：4,916/4,917 文件可转换；**lint 直通 485 源**（其余需按清单人工复核——主要是
`@js:` 脚本依赖，转换器诚实挖除而非猜测翻译）。

**一定转不了的五大类**（`MIGRATION.md` §5）：操作宿主 UI 的（`loginUi` 控件）、远程 `jsLib`、
伪协议与越界能力（`data:`/`upload()`/`java.*`）、死字段、带宿主状态的隐式语义。
每条带 Legado 原字段与原因，绝不静默丢弃。

**复核流程**：`MIGRATION.md` §1.2 的六步清单（双向验证编码、核对列表页即详情页、逐个
`rsl explain` 被改写规则……）。一句话：**转换器给你 90% 的体力活，剩下 10% 是语义判断，工具不替你做主。**

---

## 9. 何时不该换（诚实清单）

以下情况，**继续用阅读**是更好的选择：

1. 源深度依赖 `java.*` 宿主函数（文件/剪贴板/通知等 App 能力）——RSL 不提供等价面。
2. 依赖远程 `jsLib` 的源集合——RSL 要求自包含。
3. 依赖 `loginUi` 自定义控件（button/toggle/select）的登录流——RSL 的登录是契约化字段，
   宿主实现控件形态，复杂 UI 交互需要宿主适配。
4. 只想在 Android 上"装了就用"——RSL 当前没有 App 形态（内核 + CLI + 绑定）。
5. 站点风控依赖 WebView 指纹/环境——RSL 的 WebView 走端口注入，取决于宿主实现。

**适合换的场景**：想要跨端复用（iOS/桌面/浏览器扩展）、想要可审计可测试的书源资产、
想要编译期发现站点改版（而不是静默空结果）、想要把源当"代码"做 CI。

---

## 10. 状态与成熟度

| 项 | 状态 |
|---|---|
| 语言契约 v2.0 | ✅ 冻结（`RSL-1.0.md`，Schema 2020-12） |
| Rust 内核 | ✅ 全部落地（编译器/传输/执行/脚本 + 增强 A/B/C 三方向） |
| 绑定 | ✅ C ABI / Swift / JNI / WASM（golden 三端一致）；真机链接验证待外部环境 |
| 编辑器 | ⬜ 未开始（LSP 已就位） |
| 兼容性承诺 | 与 Legado **不兼容且不打算兼容**；迁移走一次性转换器 |

相关阅读：[`spec/CONCEPTS.md`](../spec/CONCEPTS.md)（为什么这么设计）·
[`spec/GAP-AUDIT.md`](../spec/GAP-AUDIT.md)（与现网引擎差距矩阵，哪些不采纳、为什么）·
[`spec/KERNEL-COMPARE.md`](../spec/KERNEL-COMPARE.md)（实现级逐层对比，含源码行号）·
[`spec/MIGRATION.md`](../spec/MIGRATION.md)（字段级迁移手册）。
