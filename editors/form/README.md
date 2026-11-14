# Schema 驱动表单（D3 原型 + H2 选型建议）

## 现状（TASK-BOOK-3 D3 已交付）

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
