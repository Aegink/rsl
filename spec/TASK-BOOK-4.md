---
id: rsl-taskbook-4
type: plan
status: active
title: RSL 任务书 v4（编辑 + 调试：五页面调试器 · serve 服务 · 三宿主 · explore 编辑露出）
parent: rsl-language
tags: [rsl, taskbook, v4, debugger, serve, editor, explore, tauri]
---

# RSL 任务书 v4（编辑 + 调试）

> 本文是第四轮施工图：把调研 [`docs/20261115_调研_发现与调试编辑器.md`](../docs/20261115_调研_发现与调试编辑器.md)
> 的结论立项为**编辑 + 调试**双轨能力。拓扑采用用户拍板的 **A+D+C 三宿主**：
> 一个调试协议（rsl-debug/1）+ 一个 web 调试前端，三个宿主（VS Code / 浏览器 / Tauri 桌面版）复用；
> 调试执行**永远在 Rust 内核**（跨端一致，杜绝"编辑器平行解析"）。
> 不修改 `RSL-1.0.md` 既有条款语义；全部增量以新 schema 文件 / 新 CLI 命令 / 编辑器模块落地。

---

## 0. 使用说明

### 0.1 权威顺序（冲突裁决梯）

1. `RSL-1.0.md` —— 语言契约（既有条款语义不变）
2. `TASK-BOOK.md`（V1）/ `TASK-BOOK-2.md`（V2）/ `TASK-BOOK-3.md`（V3）—— 已完结基线
3. **本文（TASK-BOOK-4）** —— 编辑 + 调试任务书
4. `docs/handoff/` 实施记录；`docs/20261115_调研_发现与调试编辑器.md` —— 本文证据链（调§n）

### 0.2 引用标记

| 标记 | 指向 | 例 |
|---|---|---|
| `R§6.3` / `R§5` / `R§17` | RSL-1.0.md 章节 | R§6.3 explore 契约 |
| `ERR§` | ERRORS.md 码表 | E_UNSUPPORTED |
| `K…` / `K表` | KERNEL-COMPARE.md 台账 | F.47 发现结构 |
| `调§n` | 20261115 调研报告章节 | 调§3 调试器机制 |
| `V3-D…` / `V2-D…` | 历轮裁决 | V3-D10 Schema 计数 |
| `trace.schema` 先例 | schema/trace.schema.json（rsl-trace/1，V3-D2） | 独立 schema 文件 + H 版本冻结 |

### 0.3 DoD 等级

沿用 V3§0.3：L0 编译 / L1 单测 / L2 验收命令可复现 / L3 真实站点手工清单 / **L-infra** 长时产物归档。

### 0.4 前置输入

| 输入 | 角色 |
|---|---|
| 调研报告（调§1~§9） | 本文全部任务的证据链 |
| TASK-BOOK-3 表单编辑器（已交付） | 编辑轨底座（form-model / ZH 文案门禁 / WebView 模式） |
| rsl-lsp（手写 JSON-RPC）+ trace.schema 先例 | 薄壳与 schema 稳定性先例 |
| 实测基线 | 21 表单单测 / npm 双测试 / 全门禁 / 双引擎 93+93 |

---

## 1. 交付总览

### 1.1 拓扑（A+D+C，用户拍板）

```
                ┌─ A · VS Code 扩展（T6）：编辑器 + webview 调试面板 + 实时转发
rsl-debug/1 ────┤
（Rust 内核执行）├─ D · rsl serve（T2）：本机回环 HTTP 调试服务 ← 任意浏览器
                └─ C · Tauri 桌面版（E3，P2）：同一前端 + serve 边车
```

**唯一执行体 = Rust 内核**；前端零解析逻辑（只渲染与编排）；会话状态在前端（serve 无状态）。

### 1.2 里程碑

| 里程碑 | 交付 | 验收上限 |
|---|---|---|
| **V4-T 调试轨** | T1 调试产物 → T2 serve → T3 web 前端（五页×四tab）→ T4 链式 → T5 实时 → T6 VS Code 集成 → T7 诊断直通 | L2：serve_smoke + schema 校验 + npm；L3：真站五阶段手工清单 |
| **V4-E 编辑轨** | E1 explore 编辑露出 → E2 编辑-调试联动 | L1 单测 + L2 真机 |
| **V4-C 桌面轨（P2）** | E3 Tauri 壳 + G8 真 WebViewPort 议题 | P2 评审后另立细则 |

### 1.3 依赖图

```
T1 rsl-debug/1 ── T2 rsl serve ── T3 web 前端 ── T4 链式会话
                        │              │
                        └── T5 实时 ───┼── T6 VS Code 集成
E1 explore 编辑露出 ── E2 编辑-调试联动 ─┘
E3 Tauri（P2，消费 T2/T3）
```

### 1.4 规模预算

| 模块 | 预算 |
|---|---|
| T1 内核调试产物 + schema | ≈480（含单测） |
| T2 rsl serve | ≈420 |
| T3 web 调试前端 | ≈1400（vanilla HTML/JS/CSS，零依赖） |
| T4 链式会话 | ≈260（前端状态机） |
| T5 实时重跑 | ≈180 |
| T6 VS Code 集成 | ≈260 |
| T7 诊断直通/诚实降级 | ≈120 |
| E1 explore 编辑露出 | ≈280 |
| E2 编辑-调试联动 | ≈120 |
| 测试与脚本（serve_smoke 等） | ≈350 |
| **P1 合计** | **≈3870 行** |

---

## 2. 多维交叉验证

### 2.1 九个维度

同 V3§2.1：依据 / 来源 / 现状 / 任务 / 交付 / 验收 / 依赖 / 回归 / 优先级。

### 2.2 主矩阵（10 条能力）

#### T 调试轨（1–7）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **rsl-debug/1 调试产物** | 调§4.2/§4.3（四页面数据面内核已有：HttpResponse{status,headers,body,final_url,ms}；缺外吐） | 调§3.1 阅读"协议标头"tab 字段清单（原始地址/请求地址/重定向/重试/引擎/方式/解析方式） | `rsl run` 只出 out；trace-json 有诊断/计时无请求/响应 | T1 | ExecCx 捕获请求/响应（渲染后 URL、请求头、重定向、重试、引擎、解析方式；响应体解码预览 ≤256KB + sha256 + truncated）→ `schema/rsl-debug.schema.json`（rsl-debug/1，**独立 schema 文件，不动 rsl.schema.json $defs=32**，沿 trace.schema 先例）+ `rsl run --debug-json <p>` | 单测：fixture run 产物含四 tab 全数据；schema 校验（check.py 扩展项）；无请求阶段（纯 const）request=null 合法 | — | 全门禁 | P1 |
| 2 | **rsl serve 调试服务** | 调§6 选项 D；调§3.2 阅读 Web 服务协议先例 | 调§3.2（HTTP 保存 + WS 调试）；立意"单一执行体" | 无服务 | T2 | `rsl serve [--port 随机] [--token t] [--source p]`：std 手写回环 HTTP（短连接、GET/POST JSON，**零新依赖**，沿手写 LSP 先例）；端点：`POST /debug/run {source_text, stage, inputs}`（**源文本随请求携带——不读盘不落盘，实时+S§17**）、`GET /debug/schema`、`GET /health`、静态前端；**127.0.0.1 绑定**；无状态（会话在前端） | serve_smoke：起服务→POST fixture 源→断言 rsl-debug/1 产物四数据面；非回环绑定拒绝；无 token 时仅回环 | T1 | 全门禁 | P1 |
| 3 | **web 调试前端（五页×四tab）** | 调§1.2 2.2/2.4；调§5 Postman 四窗格先例 | 用户截图（阅读调试页）；调§3.1 | 无 | T3 | `editors/web-debug/`（vanilla HTML/JS/CSS 零依赖）：阶段下拉（search/**explore**/detail/toc/content）× 4 tab——原始数据（美化/取消换行/还原）、解析结果（美化）、协议标头（原始地址/请求地址/重定向/重试/引擎/方式/解析方式）、**可视界面**（search/explore→书卡片网格；detail→书籍页；toc→章节列表；content→正文排版，数据全部来自 out）；**前端零解析逻辑**（只渲染，V4-D1） | serve_smoke 断言前端可加载；真机清单：四 tab 与阅读截图逐项对齐 | T2 | — | P1 |
| 4 | **链式会话** | 调§1.2 2.3；调§3.2（阅读 key 约定/前置阶段须先成功） | 用户截图（点选搜索结果→详情） | `--book/--chapter` 手动注入存在；无编排 | T4 | 前端状态机：search/explore → **点选条目** → out.map 契约字段注入 `var:book`（detail）→ `var:book.tocUrl`（toc）→ 选中章节 `var:chapter.url`（content）；**发现独立入口**（groups 树点选 + filters 控件，不经搜索）——注入走既有 RunInputs（stage/mod.rs:335-342 已支持，**内核零改动**，V4-D6） | 真机链式五跳全通；断链时提示缺前置（对齐阅读"前置阶段须先成功"） | T3 | — | P1 |
| 5 | **实时生效（免保存）** | 调§1.2 2.1；调§3.2（阅读"保存即最新"，RSL 更进一步免保存） | 调§5（didChange/notify 先例：test --watch 用 notify 7） | 编辑→手动重跑 | T5 | 编辑端防抖 800ms → 用**编辑缓冲最新文本** POST 重跑当前阶段（serve 无状态幂等，V4-D10）；独立模式 notify 文件监听（依赖已在 CLI）；限流沿用源内 rate + **手动暂停开关** | 真机：改选择器 ≤1s 四 tab 全刷新，免保存 | T3 | — | P1 |
| 6 | **VS Code 集成** | 调§6 选项 A；调§3.3 官方 web 编辑器先例（前端壳 + 内核服务） | 调§6 立意校验表 | 扩展已有 LSP/表单 | T6 | 命令 `RSL: 调试源`（F6 已占表单→新键）+ 状态项：spawn `rsl serve`（随机端口+token，工作区生命周期）→ webview iframe 装前端 → didChange 转发（T5）→ 停止即回收进程 | npm 集成测试：spawn→health→run→回收；真机清单 | T2/T3 | 扩展既有测试 | P1 |
| 7 | **诊断直通与诚实降级** | 调§1.2 2.4（真实数据）；调§6 立意"失败可区分" | ERR§（E_UNSUPPORTED 等 36 码） | 诊断在 trace-json 有 | T7 | rsl-debug/1.diagnostics 全结构直通四 tab 顶部；**webview/登录型阶段 → E_UNSUPPORTED 原样展示 + 引导文案**（桌面调试器无该能力=诚实暴露，V4-D7；真能力 P2/E3-G8） | 真机：webview 源调试显示 E_UNSUPPORTED + 引导 | T3 | — | P1 |

#### E 编辑轨（8–10）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 8 | **explore 编辑露出** | 调§1.1（发现有契约无露出）；调§7 G5 | R§6.3；K F.47（RSL 结构强） | form-model modeledPaths 无 explore.* | E1 | 表单编辑器补分区"发现（explore）"：filters（控件化：options→多选/单选）、groups 两级树编辑、groupsFrom、request/out/paginate；ZH 文案走 ZH 表（**门禁自动锁定**，V3 表单先例） | 单测：explore extract/apply 往返 + 未建模保留；真机：groups 树增删 + filters 改值 → apply 写回 | — | form 21 测试不回归 | P1 |
| 9 | **编辑-调试联动** | 用户强调"编辑、调试很重要"；调§1.2 2.1 | 本文 T5/T6 | 编辑与调试割裂 | E2 | 表单/文本任一改动 → 调试面板实时反映（同一 serve 会话）；诊断 `path` ↔ 表单字段/文本行**双向定位**（诊断点表单字段跳转；表单字段高亮对应诊断） | 真机：表单改 search 书名选择器 → 调试面板解析结果即时变化；诊断 chip 点击跳表单字段 | E1/T5 | form 测试 | P1 |
| 10 | **Tauri 独立编辑器（P2）** | 调§1.2 2.5（"独立做一个完整的编辑器"）；调§5 Legado Tauri 先例 | 调§6 选项 C 立意校验 | 无 | E3 | P2 范围草案：Tauri 壳（打包 web 前端 + `rsl serve` 边车）+ 源管理树 + 导入/pack/lint 入口 + 设置；**G8**：Tauri 才能实现真 WebViewPort（webview/登录阶段的桌面调试能力） | P2 评审后另立细则（打包/签名/更新策略 H2） | T2/T3 | — | P2 |

### 2.3 立意公理逐格校验（编辑、调试双轨 × 六公理）

| 项 | 跨端一致 | 失败可区分 | 能力按需付费 | 规则是数据 | 源自包含可审计 | UI 不进标准 |
|---|---|---|---|---|---|---|
| **调试（T1~T7）** | ✅ 执行唯一在 Rust 内核；rsl-debug/1 四端同源——"模拟 vs 真实"分歧天然不存在（调§5 反面教材的正面解法） | ✅ diagnostics 全结构直通四 tab；E_UNSUPPORTED 诚实展示不假装成功 | ✅ webview/登录阶段按声明诚实拒绝；能力缺口如实呈现 | ✅ 调试产物 = rsl-debug/1 数据（schema 冻结承诺沿 trace 先例）；前端零解析逻辑 | ✅ serve 不读盘不落盘、源文本随请求携带、响应体仅内存预览（S§17）；回环绑定 | ✅ 面板/前端是宿主工具；协议进 schema，UI 不进 |
| **编辑（E1~E3）** | ✅ explore 编辑产出即契约数据（R§6.3），四端无歧义 | ✅ apply 后诊断直通（LSP 已有）；编辑-联动让失败即刻可见 | ✅ explore filters/能力开关控件化=按需付费的 UI 化 | ✅ 表单/调试/文本三视图共享同一 JSON（rules 是唯一数据源） | ✅ 编辑器不发源不落敏感值（secret 脱敏沿 V3） | ✅ 表单/调试均为宿主工具，语言契约零改动 |

### 2.4 交叉对账（每条能力 × 既有契约/裁决）

| 能力 | 对账项 | 结论 |
|---|---|---|
| T1 | trace.schema 先例（rsl-trace/1 冻结承诺） | rsl-debug/1 同体例：独立 schema 文件 + 版本冻结；**rsl.schema.json $defs 保持 32**（V3-D10 断言不回归） |
| T1 | 调§3.1 阅读协议标头字段清单 | 原始地址/请求地址/重定向/重试/引擎/方式/解析方式逐项有映射（ExecCx 捕获 + RequestIR 字段） |
| T2 | CONTRIBUTING 分层红线（core 无 tokio/无进程内网络） | serve 在 **rsl-cli**（进程边界），std 手写 HTTP 零新依赖（沿手写 LSP 薄壳先例） |
| T2 | R§17 法律与分发边界 | 不落盘、仅回环、响应体内存预览；文档同步边界条款 |
| T3 | 阅读 4 tab 字段（用户截图逐项） | 原始数据/解析结果/协议标头/可视界面全映射；额外提供诊断顶栏（阅读没有的 RSL 增强） |
| T4 | R§5 变量契约（var:book/var:chapter/var:group/var:filter）+ stage/mod.rs:335-342 注入实现 | 链式注入零内核改动；发现的选中条目=§5"宿主显式传入"原生语义 |
| T4 | K F.47 发现结构 | groups 树/filters 控件直接消费 R§6.3 数据——**RSL 强结构在调试器里首次可視** |
| T5 | notify 7 已在 rsl-cli（test --watch） | 独立模式文件监听零新依赖 |
| T6 | V3 表单模式（spawn/手写 rpc.js） | 复用扩展工程（lib/rpc.js/resolveLspPath 先例）；F6 已占表单→调试键另设（H5） |
| T7 | ERR§ 36 码 | 零新错误码；E_UNSUPPORTED 语义复用（webview/登录能力缺失） |
| E1 | V3 ZH 文案门禁（表单 21 测试） | explore 分区文案同门禁自动锁定；$defs=32 不变（表单只读 schema，不增 $def） |
| E3 | 调§5 Legado Tauri 先例 | P2 细则另立（H2）；P1 协议（rsl-debug/1 + serve API）为其地基，不返工 |

---

## 3. 任务分解（WBS）

> 执行顺序：T1 → T2 → T3 → T4/T5/T7（可并行）→ T6 → E1 → E2 →（P2）E3。

### 3.1 T 调试轨

| ID | 目标 | 输入 | 输出 | 依赖 | 验收（可执行） | 引用 |
|---|---|---|---|---|---|---|
| **T1** | rsl-debug/1 调试产物 | 调§4.3；trace.schema 先例 | `stage/debug.rs`：ExecCx 请求/响应捕获（含解码预览/truncated/sha256/ms/final_url/重定向/重试/引擎）+ 产物组装；`schema/rsl-debug.schema.json`；CLI `rsl run --debug-json <p>` | — | fixture 单测断言四数据面；check.py 增 schema 校验项（样例产物）；无请求阶段 request=null | 调§4.2/§4.3 |
| **T2** | rsl serve | T1 | `rsl-cli/src/serve.rs`：std 手写回环 HTTP；`POST /debug/run`（source_text/stage/inputs）→ rsl-debug/1；`GET /debug/schema`、`/health`、静态前端；`--port/--token/--source`；生命周期日志 | T1 | tools/serve_smoke.py：起服务→fixture 源 run→四数据面断言→token/回环拒绝用例→回收 | 调§3.2/§6 |
| **T3** | web 调试前端 | T2 | `editors/web-debug/`（vanilla）：五阶段页 × 四 tab + 工具条（美化/取消换行/还原）+ 输入区（kw/页码/filters 控件/groups 树）+ 可视渲染四形态（卡片/书籍页/章节列表/正文排版）+ 诊断顶栏 | T2 | serve_smoke 前端加载断言；真机清单：四 tab 对照阅读截图逐项 | 调§3.1/§5 |
| **T4** | 链式会话 | T3 | 前端状态机：结果点选 → 契约字段注入（var:book/book.tocUrl/chapter.url）→ 下阶段；发现独立入口（groups 树/filters）；断链提示 | T3 | 真机五跳全通（fixture 源）+ 缺前置提示 | 调§3.2/R§5/R§6.3-6.6 |
| **T5** | 实时生效 | T3 | 编辑端防抖 800ms → POST 最新文本重跑；独立模式 notify 文件监听；暂停开关 | T3 | 真机：改选择器 ≤1s 四 tab 刷新（免保存）；连发改写不叠加（防抖） | 调§3.2 |
| **T6** | VS Code 集成 | T2/T3 | 扩展：命令/状态项/serve 生命周期/webview iframe（token 随查询串）/didChange 转发 | T2/T3 | npm 集成测试（spawn→health→run→回收）；真机清单 | 调§3.3/§6 |
| **T7** | 诊断直通与诚实降级 | T3 | diagnostics 顶栏组件；E_UNSUPPORTED 场景展示 + 引导文案 | T3 | 真机：webview 源显示 E_UNSUPPORTED + 引导 | ERR§/调§6 |

### 3.2 E 编辑轨

| ID | 目标 | 输入 | 输出 | 依赖 | 验收 | 引用 |
|---|---|---|---|---|---|---|
| **E1** | explore 编辑露出 | R§6.3 | form-model 补 `explore.filters/groups/groupsFrom/request/out.list/out.map.*/paginate`（filters 控件化、groups 两级树）+ ZH 文案 | — | 单测往返 + 未建模保留；真机增删 groups/改 filters → apply 写回 | 调§7 G5/K F.47 |
| **E2** | 编辑-调试联动 | E1/T5 | 表单/文本改动 → 调试实时反映；诊断 path ↔ 表单字段双向定位 | E1/T5 | 真机：改书名选择器 → 解析结果即时变化；诊断 chip 跳字段 | 用户诉求 2.1 |
| **E3** | Tauri 独立编辑器（P2） | T2/T3 | P2 范围草案：壳 + 边车 + 源管理 + 导入/pack/lint + 设置；G8 真 WebViewPort | T2/T3 | P2 评审后另立 | 调§5/§7 G7/G8 |

---

## 4. 前置研究落位表

| 来源 | 等级 | 关键结论 | 落位 |
|---|---|---|---|
| [legado 技能包·发现模块](file:///C:/Users/wangh/.zcode/skills/legado/references/knowledge/阅读书源引擎/阅读书源引擎-发现模块.md) | 【源码】 | ExploreRule 10 字段与 SearchRule 同构；exploreUrl 四写法；ExploreKind 5 交互类型；动态发现范式与翻页占位符坑 | 调§2；T3/T4/E1 |
| [KERNEL-COMPARE.md](KERNEL-COMPARE.md) | 【源码】 | F.47 发现结构 RSL 强；§2.12 编排；exploreUrl 展开与回退 | 调§2.2/§4.1；E1 |
| 用户截图（阅读 iOS 调试页 ×7） | 【实测】 | 四 tab 字段清单/五阶段下拉/链式交互/发现两级 tab | T3/T4 验收基准 |
| [gedoor/legado（BookSourceDebugActivity）](https://github.com/gedoor/legado) | 【源码】 | 调试 Activity 位置与形态 | 调§3.1 |
| [legado-debug.py](https://github.com/aminaqvq/legado-source-toolkit)（技能包 tools） | 【源码】 | Web 服务调试协议：HTTP 保存 + WS `{tag,key}`；五阶段 key 约定；前置阶段依赖 | 调§3.2；T2/T4 |
| [gedoor/legado_web_source_editor](https://github.com/gedoor/legado_web_source_editor) | 【联网】 | 前端壳 + App 服务执行（VITE_API=1122）；已迁 modules/web | 调§3.3；T2/T6 |
| [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol) / [架构概述](https://github.com/microsoft/debug-adapter-protocol/blob/main/overview.md) / [VS Code 调试扩展指南](https://code.visualstudio.com/api/extension-guides/debugger-extension) | 【联网】 | 三层适配器架构；粒度结论：声明式规则调试=阶段执行+数据查看，非断点单步 | 调§5；本文不引完整 DAP |
| [Legado Tauri 桌面版](https://docs.legadoteam.org) | 【联网】 | 独立 app 先例（Rust 壳 + web 前端） | 调§5；E3 |
| [Legado 书源规则说明（官方镜像）](https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html) | 【联网】 | 调试流程官方描述 | 调§3 |
| 社区 Python 模拟调试引擎（Legado调试引擎与自动化.md） | 【联网】 | 反面教材："模拟只是近似，官方源码才是唯一权威" | 调§5；V4-D1 依据 |
| 【实测】trace.schema/rsl-trace/1、RunInputs 注入、notify 7、form 21 单测 | 【实测】 | 本文各任务的地基 | T1/T4/T5/E1 |

---

## 5. 验收矩阵

### 5.1 关键验收命令

```bash
# T1 调试产物
cargo test -p rsl-core                                  # 含 debug 捕获单测
rsl run examples/novel-html.rsl.json search --kw 斗 --debug-json scratch/debug.json
python -I tools/check.py                                # 含 rsl-debug.schema 校验项（样例产物）
# T2/T3 serve + 前端
python -I tools/serve_smoke.py                          # 起服务/四数据面/token/回环断言/回收
# T6 扩展
cd editors/vscode && npm test                           # 协议 6/6 + 表单 21 + serve 集成
# E1/E2
node editors/vscode/test/form-model-test.js             # explore 往返 + 文案门禁
```

### 5.2 全量回归门禁（完成任何任务后必须全绿）

```bash
python tools/check.py                  # + rsl-debug/1 样例产物 schema 校验（C3/门禁扩展）
python tools/consistency.py            # $defs=32 不变（调试 schema 独立文件）
python tools/conformance.py check      # golden 三端
python prototype/expr_proto.py
cargo test -p rsl-core                 # 默认 QuickJS
cargo test -p rsl-core --no-default-features --features engine-js,net   # Boa 回退
python -I tools/lsp_smoke.py           # LSP 6/6
python -I tools/serve_smoke.py         # 新增：serve/前端冒烟
```

### 5.3 真机手工清单（L3）

- [ ] 五阶段页 × 四 tab 与阅读截图逐项对齐（原始数据/解析结果/协议标头/可视界面）
- [ ] 链式五跳：搜索→点选→详情→目录→正文（fixture 源全通；真实站手工选 1 站）
- [ ] 发现独立入口：groups 树点选 + filters 改值 → 列表刷新
- [ ] 免保存实时：改选择器 ≤1s 刷新；连发改写不叠加
- [ ] webview 源 → E_UNSUPPORTED 诚实展示 + 引导
- [ ] 诊断顶栏 ↔ 表单字段双向定位

---

## 6. 风险与未决

### 6.1 已定裁决（V4-D，直接执行）

| # | 裁决 | 依据 |
|---|---|---|
| **V4-D1** | 调试执行唯一在 Rust 内核；前端零解析逻辑（只渲染/编排） | 调§5 反面教材（模拟内核分歧）；立意"跨端一致" |
| **V4-D2** | serve = rsl-cli 内 std 手写最小回环 HTTP（GET/POST JSON 短连接），**零新依赖**（沿手写 LSP 薄壳先例） | CONTRIBUTING 分层红线；手写薄壳先例 ×2 |
| **V4-D3** | serve 无状态：会话在前端；**源文本随请求携带**（不读盘不落盘）——实时与 S§17 同时满足 | 调§3.2 保存即最新机制；R§17 |
| **V4-D4** | 响应体预览 ≤256KB + sha256 + truncated 标记；超限部分不进产物（大响应不卡 UI） | 调§7 R3 |
| **V4-D5** | rsl-debug/1 为独立 schema 文件（schema/rsl-debug.schema.json），版本字段冻结承诺沿 trace 先例；**rsl.schema.json $defs 保持 32** | V3-D10 / trace 先例 |
| **V4-D6** | 链式注入走既有 RunInputs（book/chapter/group/filter），编排在前端——内核零编排逻辑 | stage/mod.rs:335-342 已实现；R§5 |
| **V4-D7** | webview/登录阶段在桌面调试器 → E_UNSUPPORTED 诚实展示 + 引导；真 WebViewPort 能力归 P2/E3-G8 | 立意"失败可区分"；调§7 G8 |
| **V4-D8** | 可视界面渲染纯前端（out 数据 → 卡片/书籍页/章节列表/正文排版），内核无渲染逻辑 | UI 不进标准 |
| **V4-D9** | filters/groups 交互 = 前端控件 → RunInputs 注入（内核已支持零改动）；filters 数据面即"能力按需付费"的 UI 化 | R§6.3 |
| **V4-D10** | 实时重跑 = 编辑端职责（didChange/notify 防抖 800ms）；serve 无状态幂等；暂停开关必备（防打站点） | 调§7 R2 |
| **V4-D11** | serve 安全：127.0.0.1 绑定 + 默认随机端口 + 可选 token；任何请求/响应不落盘 | R§17 / 调§7 R1 |
| **V4-D12** | explore 编辑文案走 ZH 表门禁（新手文案延续）；表单 $defs/门禁计数不回归 | V3 表单先例 |

### 6.2 未决（H，不阻塞 P1 主体）

| # | 未决 | 阻塞 | 何时定 |
|---|---|---|---|
| H1 | web 前端技术栈（vanilla 先行；组件复杂度超阈值再评估轻框架） | T3 | T3 实施中 |
| H2 | Tauri 打包/签名/更新策略 | E3 | P2 评审 |
| H3 | serve 并发模型（默认单线程串行；多源并发另议） | T2 | T2 实施确认 |
| H4 | 响应体编码还原边界（gbk/big5 等 decode_response 已有；样本覆盖） | T1 | T1 实施确认 |
| H5 | 调试命令键位（F6 已占表单；候选 F7/Ctrl+Alt+D）与 webview CSP（frame-src 127.0.0.1） | T6 | T6 实施确认 |
| H6 | 订阅源（rss）调试——阅读有 `--rss`，RSL 无订阅源契约 | — | 不纳入（GAP-AUDIT 立场）；后续独立议题 |

### 6.3 风险

| # | 风险 | 概率 | 缓解 |
|---|---|---|---|
| R1 | serve 暴露本机端口被滥用 | 低 | V4-D11：回环 + 随机端口 + token + 无持久化；文档明示边界 |
| R2 | 实时重跑高频打站点 | 中 | 防抖 800ms + 源内 rate 限流沿用 + 暂停开关（V4-D10） |
| R3 | 大响应卡前端 | 中 | V4-D4 预览上限 + truncated 提示 |
| R4 | 前端复杂度膨胀失控 | 中 | vanilla + 分区上限；H1 阈值评估 |
| R5 | 链式选值字段错位 | 中 | 只传 out.map 契约字段（name/url/…），前端不发明字段 |
| R6 | 手写 HTTP 健壮性 | 低 | 限定回环/单用户/JSON-only；serve_smoke 边界用例（坏请求/超长头拒绝） |

---

## 7. 索引

### 7.1 版本表

| 文档 | 版本 | 状态 |
|---|---|---|
| 本文 | 1.0 | **Draft**（评审后转 Active） |
| 依赖：RSL-1.0.md | 2.0 | Active（不改动既有条款语义） |
| 依赖：TASK-BOOK-3（表单/引擎基线） | v1.0 | ✅ 完结 |
| 输入：调研报告（发现与调试编辑器） | 1.0 | Active（证据链） |

### 7.2 完成定义（会话收尾自检）

> 调试产物四数据面齐了吗（schema 校验过）？serve_smoke 绿了吗（token/回环用例含）？前端五页×四 tab 与截图对齐了吗？
> 链式五跳全通了吗（fixture 源）？免保存实时 ≤1s 了吗（防抖+暂停开关）？VS Code 集成回收干净了吗？
> explore 表单分区往返幂等了吗（未建模保留）？新手文案门禁还绿吗？——任何一个"没有"就不算完成。

### 7.3 与前轮的关系

V3 交付的表单编辑器（form-model/ZH 门禁/WebView 模式）是 E1/E2 的直接底座；trace.schema（rsl-trace/1）
是 T1 的 schema 先例；手写 LSP 与扩展 rpc.js 是 T2/T6 的薄壳先例；RunInputs 注入（V2 内核）是 T4 的零改动地基。
调研报告（20261115）G1~G8 差距表由本文逐条立项：G1→T1、G2/G6→T3、G3→T4、G4→T5、G5→E1、G7→E3（P2）、G8→V4-D7/P2。
