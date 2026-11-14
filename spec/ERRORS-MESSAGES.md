---
id: rsl-errors-messages
type: reference
status: active
title: 诊断码 → 宿主文案映射表（36 码全覆盖）
parent: rsl-language
tags: [rsl, errors, ux, i18n]
---

# 诊断码 → 宿主文案映射表

> TASK-BOOK-3 F3：宿主不必各自猜语义。每码给 `title`（一句话标题）、
> `user_action`（建议动作）、`severity_hint`（呈现基调）。
> UX 三原则（NN/g）：可见、建设性、尊重用户投入——文案永远给"下一步做什么"。
> 门禁：`tools/consistency.py` 断言本表覆盖 `error.rs` 全部 36 码（缺一即红）。
> 新增错误码流程：`error.rs` → `ERRORS.md` → Schema → **本表** → consistency 同步（见 CONTRIBUTING）。

## 使用方式

- 宿主 UI：诊断弹层用 `title` 作标题、`user_action` 作正文按钮/引导文案。
- i18n：本表是键位的语义基线（`code` 即键），翻译不改变键名。
- 优先级：`Diagnostic.message/hint`（具体上下文）> 本表（通用语义）。

## 编译期（作者修复）

| 码 | title | user_action | severity_hint |
|---|---|---|---|
| `E_PARSE` | 源不是合法 JSON | 检查 JSON 语法：尾逗号、单引号、未转义反斜杠；正则 `\d` 要写成 `\\d` | error（阻断） |
| `E_SCHEMA` | 字段结构不符契约 | 对照 Schema 提示补齐/修正字段结构；看 hint 里的示例 | error（阻断） |
| `E_ENUM` | 枚举值不合法 | 换成 hint 列出的合法值之一 | error（阻断） |
| `E_REF` | 引用了不存在的变量/字段 | 对照引用图检查变量名拼写；若带"快速修复"建议可直接应用 | error（阻断） |
| `E_TYPE` | 类型不匹配 | 该位置要求的类型见 hint（如选择器型参数需在响应上下文） | error（阻断） |
| `E_REGEX` | 正则语法错误 | 修正正则；在 [regex101](https://regex101.com) 验证后回填 | error（阻断） |
| `E_REGEX_UNSUPPORTED` | 正则含不支持的语法 | 改用规范支持的子集（见 REFERENCE 正则章） | error（阻断） |
| `E_SELECTOR` | 选择器缺命名空间或语法错 | 补命名空间前缀（如 `css:`/`json:`）；若带"快速修复"可直接应用 | error（阻断） |
| `E_COST` | 该能力超出成本白名单 | 改用按需付费声明的写法；见 COST 章节白名单 | error（阻断） |
| `E_EXPR_SYNTAX` | 表达式语法错误 | 检查 `expr:` 表达式括号/运算符 | error（阻断） |
| `E_EXPR_FN` | 未知表达式函数 | 对照 expr 函数表改名 | error（阻断） |
| `E_EXPR_LIMIT` | 表达式超限 | 拆分复杂表达式或放宽 limits | error（阻断） |
| `E_TEMPLATE` | 模板语法/命名空间错 | 检查 `${}` 配对与命名空间拼写 | error（阻断） |

## 加载期（宿主/版本）

| 码 | title | user_action | severity_hint |
|---|---|---|---|
| `E_UNSUPPORTED` | 宿主能力不足 | 按提示调整 requires（去 feature）或更换支持该能力的宿主 | error（可配置降级） |
| `E_VERSION_UNSUPPORTED` | 源版本高于宿主 | 升级宿主内核，或降低源的 rsl 版本声明 | error（阻断） |

## 运行期（五阶段执行）

| 码 | title | user_action | severity_hint |
|---|---|---|---|
| `E_NET` | 网络请求失败 | 检查网络/域名可达性；可重试 | error（可重试） |
| `E_TIMEOUT` | 请求超时 | 重试或调大 http.timeout | error（可重试） |
| `E_CANCELLED` | 执行被取消 | 已返回部分结果；确认是否人为取消 | warning（伴随 partial） |
| `E_HTTP` | 服务端返回错误状态 | 看状态码：4xx 查请求参数，5xx 稍后重试 | error（按状态判定） |
| `E_CHARSET` | 字符解码失败 | 声明正确的 charset（gbk/big5/…） | error（阻断） |
| `E_LOGIN_REQUIRED` | 登录态失效 | 用宿主登录界面完成登录后重试（含隔离键说明见规范 §4.5） | error（宿主弹登录） |
| `E_CHALLENGE` | 需要人机验证/交互 | 宿主引导用户完成验证（验证码/滑块/WebView 确认） | error（宿主介入） |
| `E_DECODE` | 响应解码失败 | 检查 response 类型声明与实际内容是否匹配 | error（可重试） |
| `E_NO_MATCH` | 选择器未命中任何元素 | 站点改版或规则笔误；对照真实页面调规则 | error（可降级为空） |
| `E_EMPTY` | 合法空结果 | 无需操作——这是声明的合法空（info 不是错误） | info |
| `E_FIELD_MISSING` | 必需输出字段缺失 | 检查 out.map 声明与站点实际结构 | error（阻断该阶段） |
| `E_VERIFY` | 校验不符（如 sha256） | 核对来源可信后更新校验声明；确认内容未被篡改 | error（安全向，不重试） |
| `E_PROTECT_LOST` | 字体/内容保护未命中 | 站点字体混淆变更；更新 font 映射 | error（可降级） |
| `E_BUDGET` | 超出预算上限 | 提高 limits 对应项或精简规则 | error（策略性终止） |
| `E_SCRIPT` | 脚本执行失败 | 看 sample 定位脚本错误；lib 函数问题查 lib 定义 | error（可降级） |
| `E_HOST_STAGE` | 宿主阶段失败 | 检查宿主 hostStages 实现与入参 | error（宿主侧） |

## 警告（呈现但不停机）

| 码 | title | user_action | severity_hint |
|---|---|---|---|
| `W_UNKNOWN_FIELD` | 出现未登记字段 | 通常是拼写笔误或 x- 扩展字段；对照 Schema 确认 | warning |
| `W_BUDGET_TRUNCATED` | 触顶截断但结果可用 | 需要完整结果就调大上限；否则忽略 | warning |
| `W_DEGRADED` | 降级但保留了主结果 | 按 hint 决定是否修复（如 UA 不一致 → 宿主统一 UA） | warning |
| `W_DEPRECATED` | 使用了已弃用写法 | 按提示迁移到新写法；后续版本可能移除 | warning |
| `W_ZIP_LENGTH` | zip 两侧长度不等 | 确认列表对齐逻辑；短侧按策略处理 | warning |

---

## 一致性门禁

- 本表必须覆盖 `error.rs` 全部 36 码：`tools/consistency.py` 逐码检查，缺失即失败。
- `title` ≤ 40 字；`user_action` 必须以动词开头（给"下一步"，不给"抱歉"）。
