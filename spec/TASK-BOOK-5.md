---
id: rsl-taskbook-5
type: plan
status: active
title: RSL 任务书 v5（完整编辑器：管理 → 编辑 → 调试一体闭环）
parent: rsl-language
tags: [rsl, taskbook, v5, editor, debugger, logs, dark-mode, sources, example]
---

# RSL 任务书 v5（完整编辑器闭环）

> 用户七点诉求（2026-11-15）映射 + 两轮子代理调研（阅读 web 编辑器 / 调试日志与书源管理）
> 落位。权威顺序沿 V4§0.1。V4 P1（rsl-debug/1 + serve + 五页四tab + 链式/实时/诊断直通 +
> explore 编辑露出）已交付，本文是**产品闭环层**。

---

## 0. 调研输入（【源码】子代理报告，镜像源码存 scratch/_tmp_legado_research/ 与 _tmp_legado_src/）

| 调研 | 关键结论 | 落位 |
|---|---|---|
| 阅读 web 编辑器 | **单页三栏**：左编辑区（表单/JSON 双形态）+ 中操作按钮列 + 右 4-tab（编辑源 JSON/调试/源列表/帮助）；tab 状态 localStorage 持久化；编辑→调试**先保存再跳转**（App 同）；暗色 = Element Plus dark css-vars + `html.dark` + 跟随系统 | T1 工作台布局 / T5 UI 统一 |
| 调试日志 | 纯文本行流 `[mm:ss.SSS]` 相对时间戳 + 符号协议（`︾/︽` 阶段、`⇒` 动作、`≡` 信息、`◇` 指标、`┌/└` 字段树、堆栈=错误）；web 端**过滤原始 body**；**官方无日志复制按钮**（RSL 超越点）；无独立请求/状态行（动作行已含 URL） | T3 日志 |
| 书源管理 | 7 种排序 + 升降序 + 魔法过滤词（已启用/已禁用/需登录/无分组/group:xxx）+ 批量启用禁用/导出 + 导入统一确认（新增/更新/已有三态）+ 校验闭环（跑全链 + 失效分组 + respondTime 落库） | T4 管理 |

## 1. 诉求映射与交付（T1–T5）

### T1 编辑↔调试互通（用户诉求 1：反复修改+反复调试）
- **VS Code**：表单面板加「▶ 调试」按钮（debugPanel.open(当前文档)）；调试面板加「✎ 编辑」按钮（formPanel.open）——两面板互跳，同文档同 serve 会话；命令 `rsl.debug` 从表单面板可触发。
- **web 前端**：源文本编辑区（左栏）与调试结果区天然同屏（已有）；加「表单模式」切换入口（跳 VS Code / 浏览器表单页预留）。
- 立意校验：调试执行唯一内核（不因互通而复制执行体）✓；编辑缓冲推送即最新规则（免保存超越阅读的"先保存再跳"）✓。

### T2 P2 取舍（用户授权按立意选）
- **做**：书源管理进 web 前端（T4）——"完整编辑器"三件套闭环，价值密度最高。
- **推迟**：E3 Tauri 壳（打包/签名成本高、web-debug 已可浏览器直达；保留 P2 待评审）；G8 真 WebViewPort（webview 阶段诚实降级已符合"失败可区分"，真渲染归宿主 App）。

### T3 调试日志（用户诉求 3：详细/分类/复制）
- **产物→行流渲染**（前端，零内核改动）：每次 /debug/run 产物生成分类行流：
  - `[+xxms] ▶ 请求` 方法+URL（来自 request）
  - `[+xxms] ✅ 响应` 状态/耗时/字节数（response）
  - `[+xxms] ⚙ 解析` trace 条目（rule/matched/ms，字体树形）
  - `[+xxms] ⚠ 诊断` diagnostics 每条
  - `[+xxms] ✖ 错误` error
  - 阶段行 `︾ search 开始 / ︽ search 完成`（链式轨迹）
- 分类过滤 chips（全部/请求/解析/诊断/错误）+ 复制全部（含时间戳）+ 复制单条 + 导出 .txt。
- 运行期 JS 日志（__ctx.log → trace step=ctx.log）归入「脚本」类。
- **数据来源全部来自 rsl-debug/1 产物**（不新增内核 API——产物已有 trace/diagnostics/request/response）。

### T4 书源管理（用户诉求 5/6：导入→管理→进入编辑或调试）
- web 前端加「源库」视图（与调试器同页 tab 切换，沿阅读"单页多 tab"骨架）：
  - 导入：粘贴 JSON（单源/数组）/本地文件选择 → 解析校验（E_SCHEMA 用 check 逻辑前端简化：rsl 字段存在性）→ 列表（localStorage，键=源 id）。
  - 列表：名称/URL/分组模糊筛选 + 启用开关（本地）+ 排序（名称/最近使用）+ 单源操作：**调试**（载入调试器）/ **编辑**（载入源文本区）/ 导出 JSON / 删除。
  - 状态持久化 localStorage（serve 无状态不动——管理状态是**前端状态**，符合"规则是数据"：真正的源文件仍在用户文件系统/VS Code 工作区）。
- VS Code 侧：管理即工作区文件树（已有资源管理器），不重复造列表。

### T5 UI 统一 + 暗色（用户诉求 2：参考阅读 web，去硬编码色）
- **CSS 变量重构**：两套前端（web-debug + vscode form）统一 design tokens：
  `--bg / --bg-card / --bg-code / --fg / --fg-muted / --border / --accent / --accent-fg / --danger / --warn / --ok`。
- **三层暗色**：`prefers-color-scheme` 默认跟随系统 + 手动切换按钮（浅/深/跟随）+ localStorage 记忆；VS Code 表单面板用 `--vscode-*` 变量映射（已有基础）。
- 去硬编码：全量替换 web-debug/style.css 与 form.css 的字面色值为变量；布局向阅读三栏靠拢（左输入/主区/右结果已接近，微调间距与层级）。
- 美化：卡片阴影/圆角统一、按钮层级（primary/普通/图标）、表格斑马纹、滚动条样式。

## 2. 某商业阅读 API 真实站点 L3（用户诉求 7）—— ✅ 本轮已完成（记录）
- 用户源 用户提供的本地 Legado 源文件（不入库）→ `scratch/_example/example.legado.json`。
- convert 诚实上报 15 项需人工（@js: 签名/68KB 动态发现/@put 链步）——符合 MIGRATION 语义。
- **手工重写 `scratch/_example/example.rsl.json`**（不入库，S§17）：签名=md5(排序拼接+key) 全 expr:md5 表达（params 排序即 BTreeMap 序）；
  正文 AES-128-CBC（key <站点正文密钥（不入库）>，IV=base64解码前16字节）经 script.onResponse（ctx 档）+ b64hex 桥解密。
- **内核通用修复**（非站点妥协，用户红线）：
  1. `crypto_sym` JS 桥回归：V2 用 `serde_json::to_string(&Value)` 包一切 → JS 侧拿到带引号 base64（Boa 时代 json_to_js 无此问题）→ `value_to_bridge_string`（String 直传）——任何源受益。
  2. sym 输入支持 `hex:` 前缀（通用编码入口，二进制切片场景通用）。
  3. 新增通用二进制工具桥 `__ctx.b64hex / __ctx.b64utf8`（base64↔hex 安全字符串穿越 JS 边界）。
  - 单测锁定：webjs_bridge 8 测试（sym 往返无引号 + b64 工具往返）。
- **L3 实测结果**：search（31 条真实书目，签名 URL 正确）→ detail（斗破苍穹/天蚕土豆/封面/简介）→ toc（1663 章真实列表+content_md5）→ content（AES 解密 142 段真实正文）**链式四跳全通**；
  explore 略（68KB 动态分类，独立议题）；VIP 章 API 侧拒绝（与 Legado 原源同，非内核缺陷——11-14 轮已记录）。
- 调试器在过程中的真实作用：定位 sym 引号回归（产品自证）。

## 3. WBS（本轮执行序）
| # | 项 | 输出 | 验收 |
|---|---|---|---|
| 1 | T1 互通 | form-panel/debug-panel 互跳 + 命令 | npm 测试 + 真机 F6/F7 互跳截图 |
| 2 | T5 UI | style.css/form.css 变量化 + 暗色三层 | 双主题截图对比 + 门禁 |
| 3 | T3 日志 | app.js 日志面板（分类 chips/复制/导出） | 真机截图 + 复制功能验证 |
| 4 | T4 管理 | app.js 源库 tab（导入/列表/进入编辑或调试） | 真机导入真实站点源→列表→一键调试 |
| 5 | 收尾 | 门禁全绿 + notes/handoff + commit | 全部 |

## 4. 版本表
| 文档 | 版本 | 状态 |
|---|---|---|
| 本文 | 1.0 | Active |
| 依赖 TASK-BOOK-4 | 1.0 | Active（P1 ✅） |
