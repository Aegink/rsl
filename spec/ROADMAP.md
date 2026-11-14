# RSL 路线图

> 本文件是**计划与交付状态**，不是规范。规范见 `spec/RSL-1.0.md`，决策依据见 `spec/GAP-AUDIT.md`，
> 派工史见 `spec/TASK-BOOK*.md`（V1–V5）。

---

## 当前状态（2026-11-15，开源发布版）

| 里程碑 | 状态 |
|---|---|
| V1 内核（编译器/传输/执行器/脚本表达式） | ✅ 完成（`TASK-BOOK.md` M1–M6） |
| V1 工具链（CLI 13 命令 / YAML 用例 / fixture / convert / 确定性 build / 门禁） | ✅ 完成 |
| V1 绑定（C ABI / Swift RSLKit / JNI / WASM，三端 golden 一致） | ✅ 完成（真机链接验证待外部环境） |
| V2 增强（fuzz 713 万轮 / proptest / 取消 / panic 审计 / LSP / watch / import / QuickJS 默认 / JSONPath 9535 / CSS4 / 基准） | ✅ 完成（`TASK-BOOK-2.md`） |
| V3 增补 22 条（登录会话 / 机械重登 / 诊断 fix / 编辑器 / inline-jslib / 受控远程 / QuickJS 默认化） | ✅ 完成（`TASK-BOOK-3.md`） |
| V4 编辑+调试（rsl-debug/1 四数据面 / rsl serve 回环 / web 前端五页四tab+日志 / 链式·实时·诊断直通 / explore 编辑露出 / VS Code F6+F7） | ✅ 完成（`TASK-BOOK-4.md`） |
| V5 完整编辑器闭环（源库/编辑器/调试器三视图一体 + 三层主题 + 分类日志复制导出 + /debug/lint + /lang/schema + 真实站点 L3 五链路全通） | ✅ 完成（`TASK-BOOK-5.md`） |
| V2+ 可视化规则编辑器（表单） | ✅ 完成（VS Code F6 表单 + 浏览器编辑器视图，28 表单测试） |
| P2：Tauri 桌面壳 + 真 WebViewPort（webview/登录阶段的桌面真实渲染） | ⬜ 评审后另立细则（`TASK-BOOK-5.md` P2 节） |
| P2：订阅源（rss）契约 | ⬜ 不纳入（GAP-AUDIT 立场） |

## 质量门禁（当前基线）

| 门禁 | 基线 |
|---|---|
| cargo 双引擎（QuickJS 默认 / Boa 回退） | **103 + 103** 全绿 |
| 全功能压测（**4991 真实阅读源**，离线全量） | convert 99.98% · **零 panic** · 失败全部为 Legado 方言残留（报告见 `docs/20261115_开源前全量真源压测报告.md`） |
| serve / 前端冒烟 | 42 用例 |
| npm（扩展协议 / 表单 / serve 集成） | 6 + 28 + 9 |
| LSP 冒烟 | 6/6 |
| expr 原型 | 18 + 9 |
| fuzz | 713 万轮零 panic |
| 三端一致性 | CLI ↔ WASM golden 逐字节一致 |
| 真实站点 L3 | 某商业阅读 API 五链路（签名 MD5 / AES-CBC 解密 / 1663 章 / 动态榜单）全通 |

## 语言契约现状

| 项 | 状态 |
|---|---|
| 语言契约 v2.0（`spec/RSL-1.0.md`） | Active：login.steps 多步登录、merge 多源合并、script.onResponse、paginate 四种、reuseDetailHtml、content.direct、protect、transform.zip/range、stopIf.noNew、lint 20 条 |
| Schema | `schema/rsl.schema.json`：**32 个 `$defs`**；调试产物 `schema/rsl-debug.schema.json`（rsl-debug/1）与 trace `schema/trace.schema.json`（rsl-trace/1）独立冻结 |
| 示例 | **5 个**（novel-html / comic-api / video-login / aggregate-merge / edge-cases）+ `examples/debug/` 调试样例产物 + `examples/fixtures/` 合规夹具，全部过 Schema 校验 |
| 编辑器 | VS Code 扩展（诊断/hover/补全/定义/快速修复/表单 F6/调试器 F7）+ 浏览器编辑台（源库/编辑器/调试器三视图，`editors/web-debug/`） |
| 错误码 | **36 码**（`ERRORS.md` 码表 + `ERRORS-MESSAGES.md` 文案表 + consistency 门禁对齐） |

## 未决（需要实测或评审后定）

1. `E_COST` 阈值的真实源分布验证（已有 4991 源压测数据可复核）。
2. `hostStages` 宿主支持矩阵：CLI 模拟之外的真机矩阵。
3. `requires.crypto` 各端库可用性核对（Rust ✓ / Swift 待真机 / WASM ✓）。
4. A34/A35/A36/A38/A39/A44 等 GAP-AUDIT 挂起项的实测验证（多数已由真实源 L3 侧面验证）。
5. **E3 Tauri 桌面壳 + G8 真 WebViewPort**（P2 评审后另立细则）。
6. 订阅源（rss）契约（不纳入，GAP-AUDIT 立场）。

## 质量门禁纪律

- 任何规范改动必须先更新 `GAP-AUDIT.md` 的决策记录，再改正文。
- 任何示例改动必须过 `python tools/check.py`（schema + 示例一致性），CI 里同一条命令。
- 任何字段的删除都要在变更记录里写明理由，不允许静默移除。
