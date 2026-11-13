# RSL 路线图

> 本文件是**计划**，不是规范。规范见 `spec/RSL-1.0.md`，决策依据见 `spec/GAP-AUDIT.md`。
> 上一版 `REVISION-PLAN.md` 的内容已全部并入规范与审计，该文件已删除。

---

## 已完成（文档层，2026-11-20）

| 项 | 产出 |
|---|---|
| 语言契约 v2.0 | `spec/RSL-1.0.md` 整体重写：`login.steps` 多步登录、`merge` 多源合并、`script.onResponse`、`paginate.enumerate`、`reuseDetailHtml`、`content.direct`、`protect`、`transform.zip`、`range` 负步长、`stopIf.noNew`、lint 20 条 |
| Schema | `schema/rsl.schema.json` 重写，根级允许未知字段，**29 个 `$defs`**（含 `login`/`merge`/`retry`） |
| 示例 | **4 个**：`novel-html` / `comic-api` / `video-login` / `aggregate-merge`，全部通过 Schema 校验 |
| 编译器设计 | `spec/COMPILER.md` 重写：`E_COST`、引用图含契约字段、lib 两档纯度、步骤图校验、lint 20 条映射，规模 **12600 行** |
| 表达式规范 | `spec/EXPRESSIONS.md` 重写：只保留摘要与编码，对称加解密走 `transform.crypto` |
| 差距审计 | `spec/GAP-AUDIT.md`：A1–A44 决策矩阵 + 反例清单 |
| 派生文档 | `CONCEPTS` / `REFERENCE` / `PATTERNS` / `ERRORS` / `MIGRATION` 五份新增，按"学/写/查/错/迁"分工 |
| 原型 | `prototype/expr_proto.py` 与 `EXPRESSIONS.md` 同步 |

---

## 下一步

顺序不要乱：**编译器 → 执行器 → 测试 → 绑定 → 工具链**。先写 UI 或先写 HTTP 都会让语义检查流于形式。

| 里程碑 | 产出 | 验收 |
|---|---|---|
| **M1 编译器** | `crates/rsl-core`：解析 → Schema → 语义分析 → IR；`rsl compile` / `rsl lint` | 3 个示例 0 错误；8 个负例全部报出正确错误码（见 `COMPILER.md §7`） |
| **M2 执行器** | `rsl-http` + `exec` + `stage`：五阶段、三种分页、预算计量 | `rsl run <src> search --kw 测试` 对 1 个真实站点返回 ≥1 条 |
| **M3 测试** | `rsl record` / `rsl test` / `--trace` | 同一源在站点改版后能定位到具体字段 |
| **M4 绑定** | `rsl-ffi` + `RSLKit`（Swift）+ WASM | iOS 直接调用；三端同 fixture 输出一致 |
| **M5 工具链** | `rsl build`（确定性装配）/ `rsl pack` / `rsl convert` | `build` 产物逐字节可复现；转换器给出不可转换报告 |
| **M6 编辑器** | 可视化规则编辑器（Web） | 点选生成规则，与 JSON 1:1 映射 |

---

## 未决问题（需要实测后定）

1. `E_COST` 的判定阈值：`lib.pure` 在逐章规则里的步数是否要单独设限。
2. `hostStages` 的宿主支持矩阵：先由 CLI 模拟，再进真机。
3. `requires.crypto` 各端库可用性核对（Rust / Swift / WASM）。
4. `paginate.cursor` 在"服务端返回 offset 而非游标"场景下的写法是否需要 `offsetVar` 语法糖。
5. **A34 `toc.reuseDetailHtml`**：建议直接进 1.1（详情与目录同 URL 时复用响应，省一次请求，不引入新概念）。待 M1 编译器加字段后由真实源验证命中率。
6. **A35 `transform.protect`**：`<usehtml>` 这类"这段原文不许被后续规则碰到"的需求，是用 `protect` 列表还是引入占位符语法，需先看真实源里有多少处需要保护。
7. **A36 用户级净化层的归属**：`ContentProcessor` 在阅读里是 339 行独立管线，RSL 决定写成"宿主职责 + `hostStages` 端口"。若宿主不实现，源无法自救——需在 1.1 里明确这是宿主必做项而非可选。
8. **A38 多源搜索合并语义**：阅读的 4 桶 + `origins.size` 降序是**一个具体算法**，RSL 现在是空白。要么写进规范（宿主必做），要么承认三端会分叉。倾向写进规范。
9. **A39 多步骤登录**：`login.fields` 静态表单装不下"账号密码 → 验证码 → 短信码"。`login.steps[]` 是唯一干净出路，但要先确认有多少真实站点需要两步以上。
10. **A44 `script.after` 的作用对象**：必须定死是"原始响应字节"还是"解析结果"。这条不定，`loginCheckJs` / `bodyJs` 的等价性就永远悬着。
11. **A41 纯 URL 正文的示例**：`request.skip` + `out.content: "var:chapter.url"` 零成本，等改 1.1 时顺手补进 §4.2.1。

---

## 质量门禁

- 任何规范改动必须先更新 `GAP-AUDIT.md` 的决策记录，再改正文。
- 任何示例改动必须过 `python tools/check.py`（schema + 示例一致性），CI 里同一条命令。
- 任何字段的删除都要在变更记录里写明理由，不允许静默移除。
