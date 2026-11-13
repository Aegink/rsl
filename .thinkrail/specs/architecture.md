---
id: rsl-arch
type: architecture-design
status: draft
title: RSL 内核架构与制品边界
parent: rsl-goal
tags: [rsl, architecture]
---

## 驱动

内核必须能编译到 iOS（staticlib）、Android（JNI）、WASM 和 CLI，所以**任何一处依赖 JVM 或宿主语言运行时都是不可接受的**。这一条决定了后面所有分层。

## 决策

**两层校验，职责不重叠。** JSON Schema 只判形状（编辑器秒级反馈、语言无关）；编译器判语义（命名空间、管道类型、引用图、正则子集、逐章成本、feature 声明）。schema 允许未知字段，由编译器给 `W_UNKNOWN_FIELD`——否则"新增字段可忽略"的承诺失效。

**五个固定阶段 + 宿主扩展阶段。** search / explore / detail / toc / content 是内核调度的；书架、历史、推荐等由 `hostStages` 声明，内核只负责执行它们的契约，不认识它们的业务。

**契约即类型。** 每个阶段 `out` 的保留字段表是规范的硬内容，编译器据此做字段拼写检查与跨阶段引用图（`var:book.tocUrl` 未在 detail 契约里声明 → `E_REF`）。禁止"第一个结果项自动提升为 `var:book`"这类隐式通道。

**逐章规则必须纯。** 目录/正文的逐字段规则按章求值，任何跨桥调用（配置读、网络、缓存、脚本）都会被乘以章数。这不是性能建议，是**编译期规则**：`E_COST`。

**脚本分两档。** `lib.pure` 无副作用、无 IO，可被 `transform.js` 调用；`lib.ctx` 可读写会话与上下文，禁 IO，需 `features` 声明。两档都在 QuickJS 沙箱内，都受 `libSteps` 确定性计量。

**能力按算法粒度声明。** `requires.features` 不是一个布尔桶：`crypto: ["aes-cbc","hmac-sha256"]`、`font`、`webview`、`detect` 各自独立，缺哪个报哪个。

## 模块与依赖边

内核 crate 按职责切，依赖单向（下行依赖上行，禁止反向）：

| 模块 | 职责 | 依赖 |
|---|---|---|
| `model` | serde 结构、Schema 映射 | — |
| `sugar` | 字符串选择器 → 对象、快捷字段展开 | model |
| `analyze` | 命名空间/类型/引用图/正则子集/成本/feature 检查 | model, sugar |
| `ir` | 中间表示与降级 | model, analyze |
| `expr` | `expr:` 词法、Pratt 解析、闭包树求值 | model |
| `selector` | css / xpath / json / regex / font 各后端 | model |
| `exec` | 管道执行器（纯 reduce） | ir, selector, expr |
| `http` | 传输抽象、重试、限流、cookie jar、WebView 端口 | model |
| `script` | QuickJS 沙箱与受控桥 | model |
| `stage` | 五阶段调度、分页、上下文、预算计量 | exec, http, script |
| `testkit` | fixture 录制与断言 | stage |
| `cli` | 命令行 | 以上全部 |

**制品边界**（`spec/` 下三份文档是同一边界的三种视角，不重复叙述）：

- `RSL-1.0.md` — 语言契约本身（写源的人只读这一份）。
- `COMPILER.md` — 编译器如何判定这份契约（实现者读）。
- `EXPRESSIONS.md` — `expr:` 子语言（与 COMPILER 的 Pratt 设计成对）。

`GAP-AUDIT.md` 记录的是**为什么有这些字段**——它与现网引擎的差距矩阵和取舍决定；改规范先看它，避免把已被否决的设计重新引入。

## 不变量

- 没有 `java`、`org.*`、`globalThis.fetch`、文件系统、反射出现在任何书源可见的作用域里。
- 阶段之间无共享可变全局；上下文由契约注入，可审计、可快照、可重放。
- 同一份输入 + 同一份 `limits` ⇒ 同一份输出（步数计量优先于墙钟）。
- 内核不认识任何具体站点、域名或书源；所有站点知识都在源文件里。
- 分发层只发程序，不发源、不发公共实例。

## 范围之外

聚合合并算法、UI 渲染、内容合规判定、账号体系——都在宿主侧。标准为它们定义端口（`searchMany`、`hostStages`、`login` 的 `E_LOGIN_REQUIRED` 握手），不为它们定义实现。
