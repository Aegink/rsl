---
id: rsl-taskbook-3
type: plan
status: active
title: RSL 内核增补任务书 v1.0（登录会话 · 机械重登 · 引擎默认化 · 编辑器 · 受控远程 · 修复建议）
parent: rsl-language
tags: [rsl, taskbook, v3, login, relogin, quickjs, editor, jslib, diagnostics]
---

# RSL 内核增补任务书 v1.0（TASK-BOOK-3）

> 本文是第三轮施工图：把调研报告 [`docs/20261114_调研_六项差距.md`](../docs/20261114_调研_六项差距.md)
> 的六项、22 条候选设计全部立项。所有设计已对照项目公理（跨端一致 / 失败可区分 / 能力按需付费 /
> 规则是数据 / 源自包含可审计 / UI 不进标准）逐条校验（§2.3）。
> 不修改 `RSL-1.0.md` 既有条款语义；全部增量以**新字段 / 新端口 / 新命令 flag** 落地，
> 并遵守 §6.1 裁决。

---

## 0. 使用说明

### 0.1 权威顺序（冲突裁决梯）

1. `RSL-1.0.md` —— 语言契约（既有条款语义不变）
2. `TASK-BOOK.md`（V1）/ `TASK-BOOK-2.md`（V2）—— 已完结基线
3. **本文（TASK-BOOK-3）** —— 增补任务书
4. `docs/handoff/handoff_2026-11-14_11-00.md`、`_11-40.md` —— 实施记录
5. `docs/20261114_调研_六项差距.md` —— 调研证据链（本文的输入）

### 0.2 引用标记

| 标记 | 指向 | 例 |
|---|---|---|
| `V1§…` / `V2§…` | TASK-BOOK.md / TASK-BOOK-2.md 章节 | V2§3.1 |
| `R§…` | RSL-1.0.md 章节 | R§4.5 登录 |
| `K…`（K13/K17 等） | KERNEL-COMPARE.md 缺口台账 | K13 多步登录 |
| `A…`（A1–A44） | GAP-AUDIT.md 决策矩阵 | A7 |
| `调§n` | 调研报告章节 | 调§1 登录 WebView |
| `ERR§n` | ERRORS.md 章节 | ERR§4.6 `E_LOGIN_REQUIRED` |
| 【源码】【实测】【官方】【联网】 | 证据等级 | 同 KERNEL-COMPARE 体例 |
| `D1–D11` / `V2-D1…D6` / `V3-D1…` | 历轮裁决 | V3-D3 |

### 0.3 DoD 等级

沿用 V2§0.3：L0 编译通过 / L1 单测绿 / L2 验收命令可复现 / L3 真实数据验证 / **L-infra** 长时产物归档。

### 0.4 前置输入

| 输入 | 角色 |
|---|---|
| [`docs/20261114_调研_六项差距.md`](../docs/20261114_调研_六项差距.md) | 本文全部任务的证据链（阅读源码行号 / 联网来源 / RSL 代码事实） |
| `KERNEL-COMPARE.md` | 阅读侧源码证据（`SourceLoginActivity.kt:26-34`、`WebBook.kt:71-95…`、`RowUi.kt:3-22`） |
| `RSL-1.0.md` §16 | 不采纳清单（远程 jsLib / loginUi / java.*）——本文不推翻任何一条 |
| 实测基线 | 双引擎 57+57、fuzz 713 万轮零 panic、LSP 冒烟 5/5、QuickJS/Boa 2.00–3.44× |

---

## 1. 交付总览

### 1.1 六方向与里程碑

| 方向 | 里程碑 | 交付 | 验收上限 |
|---|---|---|---|
| **A 登录会话** | S3-A | `LoginSessionPort` 端口 + `login.type=webview` 契约 + 执行路径 + UA 一致性 | L2 契约/端口测试全绿；L3 手工真站验证清单 |
| **B 机械重登** | S3-B | `login.relogin` 契约 + 重登执行器（上限硬封顶）+ 取消集成 | L2：check 首败→重登→成功的 fixture 用例 |
| **C JS 引擎** | S3-C | native 默认 QuickJS + webJs 受控桥 + 双引擎 CI 门禁 | 默认 feature 下全量测试 + golden 全绿 |
| **D 编辑器** | S3-D | `rsl-vscode` 扩展 + `--trace-json` + Schema 驱动表单骨架 | 扩展协议测试通过；VS Code 实测截图（手工） |
| **E 受控远程** | S3-E | `rsl build --inline-jslib`（拉取→内联→sha256 manifest） | 构建产物运行时零远程拉取；重复构建 sha 一致 |
| **F 失败语义** | S3-F | `Diagnostic.fix`（E_REF/E_SELECTOR）+ LSP code action + 文案映射表 + `rsl check --probe` + `--snapshot` | fix 应用后编译通过（有效性测试）；LSP 冒烟 6/6 |

### 1.2 依赖图

```
A1 LoginSessionPort ──┬─ A2 login.type=webview 契约 ─ A3 执行路径 ─ A4 UA 一致性
                      └──────────────→ B2 机械重登执行器 ── B3 重登取消
C1 QuickJS 默认化 ── C3 双引擎门禁
C2 webJs 桥统一（独立）
D1 VS Code 扩展（依赖现有 LSP）── D3 表单骨架（后置）
D2 --trace-json（独立）
E1 inline-jslib ── E2 sha256 确定性
F1 Diagnostic.fix ── F2 LSP code action
F3 文案映射表 / F4 check --probe / F5 --snapshot（独立）
```

### 1.3 规模预算

| 模块 | 预算 |
|---|---|
| A 登录会话（ports + model/analyze + login.rs + 测试） | ≈670 行 |
| B 机械重登（model/analyze + login.rs + 测试） | ≈250 行 |
| C JS 引擎（features + webJs 桥 + 门禁） | ≈300 行 |
| D 编辑器（vscode 扩展 TS + trace-json + 表单骨架） | ≈900 行 |
| E 受控远程（build inline + 测试） | ≈500 行 |
| F 失败语义（fix + code action + 文案表 + probe + snapshot） | ≈900 行 |
| **合计** | **≈3520 行** |

---

## 2. 多维交叉验证

### 2.1 九个维度

同 V2§2.1（依据 / 来源 / 现状 / 任务 / 交付 / 验收 / 依赖 / 回归 / 优先级）。

### 2.2 主矩阵（22 条能力）

#### A 登录会话（1–4）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `LoginSessionPort` 端口 | V2-D4 端口先例；调§1 选项 B | 【源码】`SourceLoginActivity.kt:26-34`；【联网】[Tachiyomi Dokka](https://mokc.github.io/docs/) 拦截器模式 | `WebViewPort` 只有单次渲染（ports.rs:59） | A1 | `ports.rs` 新 trait（begin/poll/result）+ Fake 测试实现 | 端口单测：Pending→Done(cookies,ua)/Cancelled；CancelToken 可中断 | — | 57+57 全绿 | P1 |
| 2 | `login.type=webview` 契约 | R§4.5；`qr` 枚举悬空（analyze/mod.rs:2339）【实测】 | 【源码】两路径二选一（K 表 #25）；【联网】[Legado 规则说明](https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html) | type 枚举有 `qr`，无 webview 块 | A2 | `login.webview { entry, success }`（success 复用 SelectorIR/条件）+ Schema 新 `$def` + 编译器校验 | 契约正反例编译测试；consistency 计数同步 | A1 | Schema 29→30（V3-D10） | P1 |
| 3 | webview 登录执行路径 | R§4.5.3（登录态）；ERR§4.6 `E_LOGIN_REQUIRED`；调§1 | 【源码】WebView 路径"抓 cookie 不写 loginInfo"（K 表 #25） | 仅枚举，无执行 | A3 | `login.rs`：check 失败 →（声明了 session port）begin/poll → 回捕 cookie 入隔离键 → 重跑 check；未声明 → `E_UNSUPPORTED` | l2 单测（FakeSessionPort 脚本化）；无端口报 E_UNSUPPORTED | A1、A2 | ensure_login 既有语义不变 | P1 |
| 4 | UA 一致性契约 | 调§1 Tachiyomi 头号失败模式 | 【联网】[排障指南](https://tachiyomitips.com/tachiyomi-failed-to-bypass-cloudflare)：UA 不匹配 = #1 失败因 | 无校验 | A4 | 回捕 cookie 时比对会话 UA 与 `http.headers` UA → 不一致 `W_DEGRADED`（cookie 照常入隔离键，V3-D2） | 单测：UA 不一致触发 W_DEGRADED | A3 | 无 | P2 |

#### B 机械重登（5–7）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 5 | `login.relogin` 契约 | R§4.5；调§2 选项 A/C | 【源码】阅读无自动重登（K 表 #26 旁证：`loginCheckJs` 是改写器）【联网】[规则说明](https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html) 与源码存在分歧——RSL 以源码事实为准 | `check` 失败即 `E_LOGIN_REQUIRED` 停机（login.rs:18-62） | B1 | `login.relogin { max }`（默认 0；声明值 >2 编译期 `E_SCHEMA`，V3-D3）+ Schema | 契约正反例；max>2 报错 | A3 | check TTL 语义不变 | P2 |
| 6 | 机械重登执行器 | 调§2 选项 C | 【联网】拦截器"回捕→重放"事实标准（调§1/§2）；【实测】A5 CancelToken 已就位 | 无重登循环 | B2 | check 失败 → 重跑 `ensure_login`（steps 机器重放）→ **重放原阶段一次**；trace 记录；`login.steps` 含 webview/qr 步骤 → `E_CHALLENGE`（V3-D4：机械重登不含交互） | l2 fixture 用例：首败→重登→成功；上限用例：max 耗尽仍 `E_LOGIN_REQUIRED` | B1 | 原阶段成功路径字节级不变 | P2 |
| 7 | 重登取消与并发隔离 | V2 A5 | 【实测】CancelToken 四检查点 | 无循环可取消 | B3 | 循环边界 `check_cancelled`；隔离键（源 id+域名+登录名）下重登 | 取消中途 → partial + `E_CANCELLED` | B2 | A5 测试不变 | P2 |

#### C JS 引擎（8–10）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 8 | native 默认 QuickJS | V2-D2（wasm 维持 Boa）；C1 基准 | 【实测】QuickJS/Boa 2.00–3.44×、55/55 等价（[基准报告](../docs/20261114_C5_engine_bench报告.md)） | 默认 Boa，quickjs 显式 feature | C1 | rsl-cli/ffi/lsp 默认 feature 切 `engine-quickjs`（wasm 不动）；README/规格同步 | 默认 feature 下 `cargo test` + 四门禁全绿；golden 不变（IR 与引擎无关） | — | 全量回归 | P1 |
| 9 | webJs 受控桥统一 | 调§3 选项 B；R§4.8 | 【联网】能力模型共识：guest 无 ambient authority（[quickjs-sandbox 运行时选项](https://quickjs-sandbox.sebastianwessel.net)）；【源码】阅读 webJs 全自由 | `webJs` 是页面内自由 JS（webview.rs:34-48 只验非空） | C2 | `request.webJs` 升级对象形态 `{ fn, ctx: true }`：声明 `ctx:true` 时宿主把受控 `__ctx` 桥暴露给页面；未声明保持现状（V3-D6 向后兼容）；桥里调用未声明能力 → `E_UNSUPPORTED` | 契约编译测试；宿主桥接口冒烟（Fake WebViewPort） | — | 既有字符串形态 webJs 全部兼容 | P2 |
| 10 | 双引擎 CI 门禁 | V2 C1 | 【实测】55/55 双等价 | 门禁只跑默认引擎 | C3 | 门禁脚本增 `cargo test -p rsl-core --features engine-quickjs` 必绿项（并入 check.py 或 CI 文档） | check.py 增项执行 | C1 | — | P1 |

#### D 编辑器（11–14）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 11 | `rsl-vscode` 扩展 | 调§4 选项 A | 【联网】[LSP-for-DSL 论文](https://dl.acm.org/doi/10.1145/3339255.3339258)（一服务多 IDE）；[VS Code LSP 指南](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide) | LSP server 5/5 但无编辑器分发 | D1 | 扩展工程（LanguageClient 连 `rsl-lsp`；jsonValidation→schema；snippets；language-configuration） | node 协议测试（LSP 启动/诊断/补全冒烟）；VS Code 实测截图（手工，H1） | 现有 LSP | LSP 冒烟 5/5 保持 | P1 |
| 12 | `--trace-json` 调试数据面 | 调§4 选项 D | 【实测】trace 机制已有（TraceEntry） | trace 只进内存/日志 | D2 | `rsl run --trace-json <p>`：trace + 诊断 + 阶段计时 → 结构化 JSON（定义 schema，H4 稳定性承诺） | schema 校验 + 快照测试 | — | 现有 trace 行为不变 | P2 |
| 13 | Schema 驱动表单骨架 | 调§4 选项 B | 【联网】[JSONLogic 双模式](https://jsonlogiceditor.dev)、[SurveyJS 综述](https://surveyjs.io/stay-updated/blog/best-json-form-editor) | 无 | D3 | `editors/form/`：`rsl.schema.json` → 表单配置生成器 + JSON 骨架输出（原型，技术选型 H2 在原型中定） | 原型：schema → 表单配置 → 生成合法骨架源（过 lint） | #15 schema | — | P3 |
| 14 | 扩展内 snippets | 调§4 | 【实测】completion 已有 18 命名空间 + 31 步骤键 | 无片段 | D1 内 | `snippets/*.json`（五阶段骨架/分页四模式/登录/合并） | 片段生成源过 lint | D1 | — | P1 |

#### E 受控远程（15–17）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 15 | `--inline-jslib` 构建期内联 | R§16"自包含可审计可离线"；调§5 选项 B | 【联网】[Microsoft 供应链报告](https://www.microsoft.com/en-us/security/blog/2026/05/28/typosquatted-npm-packages-used-steal-cloud-ci-cd-secrets)、[GitHub WA 依赖威胁](https://learn.github.com/well-architected/library/application-security/recommendations/managing-dependency-threats)：签名+锁定+哈希是缓解共识；【源码】阅读 jsLib = 运行时任意 URL（K §6） | build 不处理远程 | E1 | 源内 `"x-jslib": [{ url, sha256? }]`（x- 扩展字段）→ build 拉取（重试 2）→ 内联进 `lib` → manifest 记 url+sha256 | build 测试（file:// 与 https fixture）；运行时零远程拉取断言 | — | 无 x-jslib 的源构建字节不变 | P1 |
| 16 | 内联确定性（sha256） | R§14 确定性装配 | 【实测】build 已有 sha256 manifest 先例 | — | E2 | 内容变化 → sha 不匹配 → 默认拒绝（`--force` 覆盖 + 警告）；同输入重复构建字节一致 | 确定性测试；篡改检测测试 | E1 | — | P1 |
| 17 | `x-` 扩展通道文档化 | R§ Schema additionalProperties | 【实测】W_UNKNOWN_FIELD 白名单机制 | x- 语义散落 | E3 | 规范附录：x- 字段登记规则（宿主自决、RSL 不背书）+ x-jslib 示例 | 文档 + consistency 不受影响 | — | — | P3 |

#### F 失败语义（18–22）

| # | 能力 | D1 依据 | D2 来源 | D3 现状 | D4 任务 | D5 交付 | D6 验收 | D7 依赖 | D8 回归 | D9 |
|---|---|---|---|---|---|---|---|---|---|---|
| 18 | `Diagnostic.fix` 结构化 | 调§6 选项 B | 【联网】[rustc 诊断指南](https://rustc-dev-guide.rust-lang.org/diagnostics.html) machine-applicable suggestion；[错误消息三要素](https://quuxplusone.github.io/blog/2022/01/12/whats-in-a-good-error-message/) Context+Error+Mitigation | hint 是纯文本 | F1 | `Diagnostic { fix: Option<FixSpan{range,replacement,message}> }`；**只做确定性修复**（V3-D8）：`E_REF`（最近似 id 替换）、`E_SELECTOR`（补命名空间前缀） | **fix 有效性测试：应用 fix 后编译通过**；模糊修复不做 | — | 36 码诊断字段增加不破坏序列化 | P1 |
| 19 | LSP code action | 调§6 | 【联网】rustc 模式 + LSP 3.17 codeAction | LSP 无 code action | F2 | `textDocument/codeAction` + `workspace/applyEdit`（消费 F1 fix） | LSP 冒烟扩展 6/6（新增 code action 场景） | F1 | 冒烟 5/5 原场景不变 | P1 |
| 20 | code→宿主文案映射表 | 调§6 选项 A | 【联网】[NN/g](https://www.nngroup.com/articles/error-message-guidelines/) 可见/建设性/尊重投入；【实测】36 码体系 | 文案散在各诊断 message | F3 | `docs/ERRORS-MESSAGES.md`（或 ERRORS 附录）：每码 {title, user_action, severity_hint} 36 全覆盖 + 一致性门禁增项（码表全覆盖） | 门禁断言 36 码全有文案 | — | consistency 现有断言不变 | P2 |
| 21 | `rsl check --probe` 批量测活 | 调§6 选项 C | 【联网】[LS-Toolkit](https://github.com/aminaqvq/legado-source-toolkit) 连通性/评分；【实测】import 聚合报告先例 | lint 只静态 | F4 | `rsl check <dir> [--probe] [--workers n]`：lint + probe（search/check 请求状态码 + 可解析性；**只测连通不落内容**，V3-D9）→ 聚合报告（batch_report 同构） | 不可达域名单测（E_NET 进报告）；真实站手工清单 | — | import/convert 报告同构性 | P2 |
| 22 | `explain --snapshot` 诊断快照 | 调§6 选项 D | 【实测】trace 机制 | 排查靠贴日志 | F5 | `rsl explain <src> --snapshot <p>`：编译结论 + 诊断 + trace → 单 JSON | 快照 schema 校验 + 幂等测试 | D2 schema | — | P3 |

### 2.3 立意公理逐条校验（六项 × 公理）

| 项 | 跨端一致 | 失败可区分 | 能力按需付费 | 规则是数据 | 自包含可审计 | UI 不进标准 |
|---|---|---|---|---|---|---|
| A 登录会话 | ✅ 协议四端同语义 | ✅ E_UNSUPPORTED/E_CHALLENGE/W_DEGRADED 全码化 | ✅ features:["webview"] 声明 | ✅ success 判定是 SelectorIR 数据 | ✅ cookie 入隔离键（宿主持有） | ✅ 交互 UI 全归宿主（端口只定义协议） |
| B 机械重登 | ✅ relogin 语义进契约 | ✅ 重登尝试进 trace；上限耗尽仍 E_LOGIN_REQUIRED | ✅ 默认 0，声明才启用 | ✅ relogin 块可 lint/fuzz | ✅ 凭据仍宿主持有 | ✅ 交互步骤排除在外（V3-D4） |
| C JS 引擎 | ✅ 双引擎等价 55/55 | ✅ E_UNSUPPORTED 桥越权 | ✅ 默认引擎切换零契约成本 | ✅ webJs.ctx 声明制 | ✅ 桥统一后无自由执行面 | — |
| D 编辑器 | ✅ LSP 一服务多 IDE | ✅ 诊断即 UI | ✅ 扩展可选安装 | ✅ **全部建立在 JSON 数据上** | — | — |
| E 受控远程 | ✅ 构建行为确定性 | ✅ sha 不符 → 构建失败有码 | ✅ x-jslib 可选字段 | ✅ manifest 可 diff | ✅ **运行时零远程拉取** | — |
| F 失败语义 | ✅ fix 跨端同语义 | ✅ 公理本体的下半句 | ✅ fix 只做确定性类 | ✅ **fix 也是数据** | — | — |

---

## 3. 任务分解（WBS）

> 每任务：目标 / 输入 / 输出 / 依赖 / 验收（可执行）/ 引用。执行顺序：P1 → P2 → P3。

### 3.1 A+B 登录与会话（合并排期）

| ID | 目标 | 输入 | 输出 | 依赖 | 验收（可执行） | 引用 |
|---|---|---|---|---|---|---|
| **A1** | `LoginSessionPort` 端口（begin/poll/result，同步轮询，可取消） | 调§1；`WebViewPort` 先例 | `ports.rs` 新 trait + 数据结构 + `FakeSessionPort`（测试用脚本化实现） | — | `cargo test -p rsl-core`（端口单测）；Fake 脚本：begin→Pending×2→Done | 调§1、V2-D4 |
| **A2** | `login.webview` 契约（entry + success 判定） | A1 | `model.rs LoginCfg` 扩展 + Schema `$def` + `analyze` 校验（entry 必填；success 是 SelectorIR） | A1 | 契约正例过 lint；反例（无 entry / success 非选择器）报 `E_SCHEMA`；`tools/consistency.py` 同步 | 调§1、R§4.5、ERR§4.6 |
| **A3** | webview 登录执行路径 | A1、A2 | `login.rs`：`ensure_login` 中 check 失败且 type=webview → 有 session port 则 begin/poll → 回捕 cookie（隔离键）→ 重跑 check；无 port → `E_UNSUPPORTED` | A1、A2 | l2 单测（FakeSessionPort）；未注入 port 的反例报 E_UNSUPPORTED | 调§1、K 表 #25 |
| **A4** | UA 一致性校验 | A3 | 回捕时比对 UA → `W_DEGRADED`（V3-D2） | A3 | 单测：UA 不一致 → warnings 含 W_DEGRADED | 调§1 |
| **B1** | `login.relogin` 契约 | A2 | `model.rs` + Schema + 编译器（max>2 → `E_SCHEMA`，V3-D3） | A2 | 契约正反例编译测试 | 调§2 |
| **B2** | 机械重登执行器 | B1、A3 | check 失败 → ensure_login 重跑 → 重放原阶段一次；trace；webview/qr 步骤 → `E_CHALLENGE`（V3-D4） | B1 | l2 fixture：check 首败二次成功 → 阶段成功且 trace 含重登；max 耗尽 → `E_LOGIN_REQUIRED` | 调§2、R§4.5、ERR§4.7 |
| **B3** | 重登取消 | V2 A5 | 循环边界 `check_cancelled` | B2 | 取消中途 → partial + `E_CANCELLED` | V2§3.1 A5 |

### 3.2 C JS 引擎

| ID | 目标 | 输入 | 输出 | 依赖 | 验收 | 引用 |
|---|---|---|---|---|---|---|
| **C1** | native 默认 QuickJS | 基准报告 | rsl-cli/ffi/lsp 的默认 features → `engine-quickjs`；wasm 不动；文档同步 | — | 默认 feature 下全量测试 + 四门禁全绿；golden 逐字节不变 | V2-D2、调§3 |
| **C2** | webJs 受控桥 | 调§3 | `request.webJs` 对象形态 `{ fn, ctx }`；`ctx:true` → 宿主暴露 `__ctx`；未声明能力调用 → `E_UNSUPPORTED`；字符串形态兼容 | — | 编译正反例；Fake WebViewPort 桥冒烟 | 调§3 |
| **C3** | 双引擎 CI 门禁 | C1 | check.py（或 CI 文档）增 quickjs 必绿项 | C1 | 门禁执行记录 | V2 C1 |

### 3.3 D 编辑器

| ID | 目标 | 输入 | 输出 | 依赖 | 验收 | 引用 |
|---|---|---|---|---|---|---|
| **D1** | `rsl-vscode` 扩展（含 snippets） | 现有 LSP + schema | `editors/vscode/`：package.json（contributes: languages/jsonValidation/snippets）+ client 启动 `rsl-lsp` + 打包说明 | — | `npm test`（node 协议冒烟：initialize→didOpen→diagnostics→completion）；手工截图清单（H1） | 调§4 |
| **D2** | `--trace-json` | trace 机制 | `rsl run <src> <stage> --trace-json <p>`：trace+诊断+计时 → JSON（schema 落 `schema/trace.schema.json`，H4） | — | 快照测试；schema 校验 | 调§4 |
| **D3** | Schema 驱动表单骨架（原型） | schema | `editors/form/`：schema → 表单配置 + JSON 骨架生成原型（选型 H2 原型中定） | — | 生成骨架过 `rsl lint` | 调§4 |

### 3.4 E 受控远程

| ID | 目标 | 输入 | 输出 | 依赖 | 验收 | 引用 |
|---|---|---|---|---|---|---|
| **E1** | `--inline-jslib` | R§16 | 源内 `x-jslib` 声明 → build 拉取（重试 2，支持 file:// 与 https://）→ 内联 `lib`（重名函数 → `E_SCRIPT`）→ manifest 条目（url+sha256） | — | build 测试（本地 fixture）；**运行时零远程拉取**断言（内联后源无 url 引用） | 调§5 |
| **E2** | 内联确定性 | E1 | 内容变化 → sha 不符默认拒绝（`--force` 覆盖）；同输入字节一致 | E1 | 确定性 + 篡改检测测试 | R§14 |
| **E3** | x- 扩展通道文档化 | Schema | 规范附录登记 x- 规则 + x-jslib 示例 | — | 文档 + consistency 通过 | 调§5 |

### 3.5 F 失败语义

| ID | 目标 | 输入 | 输出 | 依赖 | 验收 | 引用 |
|---|---|---|---|---|---|---|
| **F1** | `Diagnostic.fix` | 调§6 | `FixSpan{range,replacement,message}`；`E_REF`（最近似 id）、`E_SELECTOR`（补前缀）两类确定性 fix | — | **有效性测试：应用 fix 后编译通过**；无效候选不产 fix | 调§6 |
| **F2** | LSP code action | F1 | `textDocument/codeAction` + `applyEdit` | F1 | LSP 冒烟 6/6（新增场景：E_REF 诊断 → code action → 应用 → 诊断消失） | 调§6 |
| **F3** | 文案映射表 | 36 码表 | `spec/ERRORS-MESSAGES.md` 或 ERRORS 附录：36 码 × {title, user_action}；consistency 增全覆盖断言 | — | 门禁断言通过 | 调§6 |
| **F4** | `rsl check --probe` | import 报告先例 | lint + probe（状态码 + 可解析性；不落内容，V3-D9）→ 聚合报告 | — | 不可达域名 → E_NET 进报告；同构 batch_report | 调§6 |
| **F5** | `explain --snapshot` | D2 schema | 编译结论 + 诊断 + trace → 单 JSON 快照 | D2 | 快照幂等（两次输出一致）+ schema 校验 | 调§6 |

---

## 4. 前置研究落位表（全量落位）

| 来源 | 等级 | 关键结论 | 落位 |
|---|---|---|---|
| [Legado 书源规则说明（官方镜像）](https://mgz0227.github.io/The-tutorial-of-Legado/Rule/source.html) | 【联网】 | loginUrl WebView 登录链路；loginCheckJs"失效弹登录页"的说法（与源码分歧） | A2/A3、B1（以源码事实为准） |
| [Tachiyomi 扩展库 Dokka 文档](https://mokc.github.io/docs/) | 【联网】 | WebViewInterceptor：challenge 拦截 → WebView → 回捕 cf_clearance → 重放 | A1/A3/B2 |
| [TachiyomiTips 排障指南](https://tachiyomitips.com/tachiyomi-failed-to-bypass-cloudflare) | 【联网】 | UA 不一致 = 头号失败因 | A4 |
| KERNEL-COMPARE §275/§279 | 【源码】 | 两登录路径事实；阅读无自动重登；loginCheckJs 实为响应改写器 | A2/B1/B2 |
| [sebastianwessel/quickjs](https://github.com/sebastianwessel/quickjs) / [运行时选项](https://quickjs-sandbox.sebastianwessel.net) / [rquickjs worker 实践](https://daviddalbusco.com/blog/a-self-hostable-wasm-sandbox-for-javascript-workers) | 【联网】 | 能力模型（guest 无 ambient authority）+ 资源限制——与 RSL 两档同构 | C2 |
| [LSP-for-DSL（Bünder 2019）](https://dl.acm.org/doi/10.1145/3339255.3339258) / [VS Code LSP 指南](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide) | 【联网】 | 一语言服务多编辑器复用 | D1 |
| [JSONLogic Visual Editor](https://jsonlogiceditor.dev) / [SurveyJS 综述](https://surveyjs.io/stay-updated/blog/best-json-form-editor) | 【联网】 | 可视化与 JSON 双模式同步；schema 驱动表单 | D3 |
| [Microsoft 供应链报告](https://www.microsoft.com/en-us/security/blog/2026/05/28/typosquatted-npm-packages-used-steal-cloud-ci-cd-secrets) / [GitHub Well-Architected](https://learn.github.com/well-architected/library/application-security/recommendations/managing-dependency-threats) | 【联网】 | 签名/锁定/哈希缓解共识；运行时拉码不可审计 | E1/E2 |
| [rustc 诊断指南](https://rustc-dev-guide.rust-lang.org/diagnostics.html) / [错误消息三要素](https://quuxplusone.github.io/blog/2022/01/12/whats-in-a-good-error-message/) / [NN/g](https://www.nngroup.com/articles/error-message-guidelines/) | 【联网】 | machine-applicable suggestion；Context+Error+Mitigation；UX 三原则 | F1/F2/F3 |
| [LS-Toolkit](https://github.com/aminaqvq/legado-source-toolkit) / [书源调试指南](https://blog.csdn.net/gitblog_00160/article/details/156017657) | 【联网】 | 批量连通性/评分是社区刚需；修复仍人工 | F4 |
| 【实测】双引擎 55/55、基准 2.00–3.44×、LSP 5/5、fuzz 713 万轮 | 【实测】 | 本文各任务的质量基线 | C1/C3、D1、F1 |

---

## 5. 验收矩阵

### 5.1 关键验收命令

```bash
# A/B 登录与重登
cargo test -p rsl-core                                  # 全绿（含 LoginSessionPort/relogin 新测试）
rsl test examples/webview-login.rsl.json --cases …      # 新示例（若出）
# C 引擎
cargo test -p rsl-core                                  # 默认 feature（quickjs）
cargo test -p rsl-core --features engine-js             # Boa 侧保持全绿
python tools/conformance.py check                       # golden 三端一致
# D 编辑器
cd editors/vscode && npm test                           # 协议冒烟
rsl run examples/novel-html.rsl.json search --trace-json out/trace.json
# E 受控远程
rsl build fixtures/inline-fixture --inline-jslib --out dist   # 内联 + manifest sha
# F 语义
rsl lint fixtures/broken/x.rsl.json                     # 输出 fix（应用后编译通过）
python tools/lsp_smoke.py                               # 6/6（+code action 场景）
rsl check examples/ --probe --workers 8                 # 探活报告
```

### 5.2 全量回归门禁（完成任何任务后必须全绿）

```bash
python tools/check.py                  # + 双引擎必绿项（C3）
python tools/consistency.py            # + Schema 计数（V3-D10）+ 文案表覆盖（F3）
python tools/conformance.py check      # golden 三端
python prototype/expr_proto.py
cargo test -p rsl-core                 # 默认引擎
cargo test -p rsl-core --features engine-js   # Boa 侧
python tools/lsp_smoke.py              # LSP 冒烟（6/6）
```

### 5.3 LSP 冒烟扩展场景（F2 后 6/6）

6. 打开含 `var:book.urll` 笔误的源 → 诊断 E_REF + code action"替换为 book.url" → 应用 → 诊断消失。

---

## 6. 风险与未决

### 6.1 已定裁决（V3-D，直接执行）

| # | 裁决 | 依据 |
|---|---|---|
| **V3-D1** | `LoginSessionPort` 独立于 `WebViewPort`：渲染 ≠ 交互会话 | 调§1；两语义生命周期不同 |
| **V3-D2** | UA 不一致 → `W_DEGRADED` 警告且 cookie 照常入隔离键（有效性由 success 判定把关），不做硬拒绝 | 对齐"降级不清空主结果"铁律；Tachiyomi 社区经验是可修复项 |
| **V3-D3** | `relogin.max` 声明值 >2 → 编译期 `E_SCHEMA`；默认 0 | 防打站点；可预测性 |
| **V3-D4** | 机械重登不含任何交互步骤（webview/qr 型步骤 → `E_CHALLENGE` 抛宿主） | 机械可 lint/fuzz；交互不可判定 |
| **V3-D5** | native 默认 QuickJS；wasm 维持 Boa | 沿 V2-D2；基准 2.0–3.4× + 55/55 |
| **V3-D6** | webJs 桥为显式声明制（`{fn, ctx:true}`），未声明保持自由 JS 现状——向后兼容 | 既有源零破坏 |
| **V3-D7** | 远程拉取只发生在 `build --inline-jslib`；**运行时零远程拉取，永不改变** | R§16 自包含立场 |
| **V3-D8** | fix 只做确定性类（E_REF/E_SELECTOR 先行）；启发式修复不做 | 有效性可测（应用后编译通过）；假修比不修更糟 |
| **V3-D9** | `check --probe` 只测连通与可解析性，不抓取不落盘内容 | 合规边界 §17 |
| **V3-D10** | Schema `$defs` 数量随新 `$def` 增加；`tools/consistency.py` 的数量断言同步更新（29 → 实际值） | 一致性门禁自洽 |

### 6.2 未决（H，不阻塞 P1）

| # | 未决 | 阻塞 | 何时定 |
|---|---|---|---|
| H1 | VS Code 扩展在无 GUI 沙盒的验收边界（协议测试 + 手工截图清单） | D1 验收 | D1 实施时 |
| H2 | 表单编辑器选型（SurveyJS / json-editor / 自研） | D3 | D3 原型后 |
| H3 | inline-jslib 拉取第三方脚本的法律面（构建时行为，责任在源作者） | E1 | 文档化即可（E3） |
| H4 | trace.schema.json 的稳定性承诺（宿主面板依赖） | D2 | D2 实施时定义版本 |
| H5 | relogin 与 merge 多源交互（重登按源隔离，隔离键已有） | B2 | 实施确认 |

### 6.3 风险

| # | 风险 | 概率 | 缓解 |
|---|---|---|---|
| R1 | relogin 循环打站点 | 低 | V3-D3 硬上限 + V3-D4 交互排除 + CancelToken |
| R2 | QuickJS 默认化暴露长尾语义差 | 中 | 55/55 已证等价；Boa feature 保留一键回退 |
| R3 | webJs 桥宿主实现差异 | 中 | V3-D6 显式声明制；Fake port 契约测试 |
| R4 | inline-jslib 拉取网络失败/篡改 | 中 | 重试 + sha 校验拒绝 + `--force` 显式覆盖 |
| R5 | fix 误修 | 中 | V3-D8 只做确定性类；有效性测试（应用后编译通过） |

---

## 7. 索引

### 7.1 版本表

| 文档 | 版本 | 状态 |
|---|---|---|
| 本文 | 1.0 | **Active**（2026-11-14 起实施；22 条能力已全量交付，见 §7.2 自检） |
| 依赖：RSL-1.0.md | 2.0 | Active（不改动既有条款语义） |
| 依赖：TASK-BOOK.md / TASK-BOOK-2.md | v2.0 / v1.0 | ✅ 完结（基线） |
| 输入：调研报告 | 1.0 | Active（证据链） |

### 7.2 完成定义（会话收尾自检）

> 登录会话端口测试绿了吗？relogin fixture 用例绿了吗？默认引擎下全量回归绿了吗？扩展协议测试过了吗？
> 内联产物运行时零远程拉取证明了吗？fix 有效性测试（应用后编译通过）绿了吗？LSP 冒烟 6/6 了吗？
> 文案表 36 码全覆盖了吗？——任何一个"没有"就不算完成。
>
> **实施结果（2026-11-14）**：
>
> | 自检项 | 证据 | 结果 |
> |---|---|---|
> | 登录会话端口 | `tests/login_session.rs` a1_fake_session_port_script | ✅ 17/17 |
> | relogin fixture 用例 | b2_relogin_machine_success / b2_relogin_exhausted / b2_relogin_zero_keeps_legacy_semantics | ✅ |
> | 默认引擎全量回归 | `cargo test -p rsl-core`（QuickJS 70+）与 Boa 侧（--no-default-features）双绿 | ✅ |
> | 扩展协议测试 | `editors/vscode npm test` 6/6（+ 手工截图清单 H1 留桌面环境） | ✅ |
> | 内联零远程拉取 | `tests/build_inline.rs` 7/7 + CLI 双构建字节一致 + 产物无 x-jslib/url 残留 | ✅ |
> | fix 有效性 | `tests/fix_suggestions.rs` 6/6（应用后编译通过） | ✅ |
> | LSP 冒烟 6/6 | `tools/lsp_smoke.py`（含场景 6 code action→应用→诊断消失） | ✅ |
> | 文案表 36 码 | `spec/ERRORS-MESSAGES.md` + consistency 门禁断言 | ✅ |

### 7.3 与前轮的关系

V2 交付的端口抽象（Transport/WebViewPort/CookieStore/CancelToken）、双引擎等价（55/55）、
LSP（5/5）、build 确定性装配是本文全部任务的前置——本文是其"第三消费者"：
A 组在端口上加会话语义，C 组在引擎等价上做默认化，E 组在确定性装配上加受控远程，
F 组在诊断引擎上加可应用修复。四道既有门禁继续兜底一切改动。
