# Schema 驱动表单（D3 原型 + H2 选型建议）

## 现状（M1/M2 已实现，2026-11-15）

**自研选型已落地**（见下文"选型建议"，拍板=做）。实现位于 `editors/vscode/`：

| 模块 | 职责 |
|---|---|
| `form/form-model.js` | 纯模型：schema→配置派生、extract/apply（**未建模键原样保留**）、选择器/步骤序列化、骨架；UMD 双端共用 |
| `media/form.js` + `media/form.css` | WebView UI：第 1~7 步分区引导、中文标签+说明+示例占位（ZH 表有门禁锁定）、新手三步横幅、必填缺口 chips、诊断条 |
| `form-panel.js` | host 侧：面板生命周期、M2 双向（didChange↔表单刷新/apply→WorkspaceEdit）、M1 生成 |
| `test/form-model-test.js` | 21 单测：往返幂等、未建模保留、空容器回收、**文案全覆盖门禁** |

- **M1**：无活动 .rsl.json 时命令进骨架模式 → 填表 → 【生成源文件】产出四阶段完整骨架+表单值（真机验证：Untitled-1 生成 ✓）
- **M2**：编辑 .rsl.json 时命令绑定文档 → 表单实时反映文档 → 【应用到文档】写回（applyFormState 合并，未建模内容不丢）→ didChange → LSP 诊断回流表单顶栏（真机验证：改名→写入→落盘 ✓）
- 命令：`RSL: 打开表单编辑器`（F6）；校验统一走 rsl-lsp，不另造校验
- 已知边界：apply 以 2 空格规范缩进重排 JSON（值不丢）；打开表单面板时再按 F6 会回绑当前文档（不会掉骨架模式）

### 真机验收截图

- [`docs/images/20261115_form_新手元信息.png`](../docs/images/20261115_form_新手元信息.png) — 三步横幅+中文标签+说明+示例占位
- [`docs/images/20261115_form_search可选字段.png`](../docs/images/20261115_form_search可选字段.png) — 核心字段徽标/加工步骤/可选项引导
- [`docs/images/20261115_form_M1生成与M2回写.png`](../docs/images/20261115_form_M1生成与M2回写.png) — M1 生成通知 + M2 写入结果同屏

## 原型（TASK-BOOK-3 D3 交付物）

- `generate_form.py`：`schema/rsl.schema.json` → 两个产物
  - `form-config.json`：21 个顶层字段的表单控件配置（**技术栈无关中间表示**，
    字段/widget/required/$def 嵌套展开，选择器字段标 `selector` 控件）
  - `skeleton.rsl.json`：从 required 推导的最小骨架源（验收：`rsl lint` 0 发现）
- 重新生成：`python -I editors/form/generate_form.py`
- Schema 变更后同步：`editors/vscode/schema/rsl.schema.json` 也来自同一源（npm run sync-schema）

## H2 选型建议：自研轻量 WebView 表单（分两步），不引入 SurveyJS / json-editor

**推荐**：VS Code 扩展内嵌 WebView 面板，消费 `form-config.json` 渲染表单，
复用现有 LSP 做校验。先 M1（表单 → 生成 JSON 单向），再 M2（表单 ↔ 文本双向同步）。

### 为什么（对齐项目公理）

| 候选 | 评估 |
|---|---|
| SurveyJS | 重依赖（数百 KB 运行时），面向问卷/表单收集场景；selector 字段（`css:`/`json:` 前缀）无命名空间感知，定制成本 > 自研 |
| json-editor | 直接吃 JSON Schema 但 UI 不可控；对 RSL 的 out 契约/管道步骤等嵌套结构渲染粗糙；同样要为 selector 写定制控件 |
| **自研（推荐）** | 中间表示已备好（`form-config.json` 即数据）；渲染器是薄壳——与本项目两次"手写薄壳替代重依赖"的先例同构（tower-lsp→手写 JSON-RPC、cargo-fuzz→内置 fuzz） |

### 公理逐条对齐

- **规则是数据**：`form-config.json` 是数据不是代码，任何宿主/编辑器都能消费同一份
- **跨端一致**：WebView 技术跨平台；校验统一走 rsl-lsp（诊断即 UI，不另造校验）
- **能力按需付费**：零第三方运行时依赖；不装扩展就没有表单，文本编辑体验不受影响
- **失败可区分**：表单生成的 JSON 直接过 `rsl lint`（20 规则）+ LSP 诊断，失败语义与手写一致
- **UI 不进标准**：表单只是编辑器壳，语言契约零改动

### M1 / M2 边界

- **M1（表单→JSON）**：表单填写 → 生成完整 `.rsl.json`（skeleton + 表单值合并）→ `rsl lint` 把关。
  单向无同步难题，价值即已成立（新作者零起步）。
- **M2（双向同步）**：文本 ↔ 表单双向绑定。难点在文本编辑期间的局部解析——LSP 已有
  didChange 全量重发诊断，M2 可在同一管道上做（全量重建表单状态，先不做增量）。

> 本建议 2026-11-14 记录，拍板后按 M1 起步。
