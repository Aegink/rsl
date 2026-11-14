# C4 lol_html 大页流式 spike 报告（TASK-BOOK-2 §3.3 C4 / V2-D5）

- 日期：2026-11-14
- 原型：`crates/rsl-core/tests/spike_lol_html.rs`（lol_html 1.2，dev-dependency）
- 结论先行：**可行，但不集成** —— 速度 2.3–3.1×、内存常量收益明确；
  但内核的 `paragraphs/strip/map/blocks` 全部建立在 DOM 语义上，流式改造
  等价实现成本 ≫ 收益。按 V2-D5 登记结论，保留为"超阈值正文直取"的
  未来优化选项（触发条件：`limits.maxBytes` 大页且 out.content 仅需纯文本）。

## 数据（本机实测，1 次）

| 指标 | DOM（现内核 scraper） | lol_html 流式（文本回调） | lol_html 纯改写 |
|---|---|---|---|
| 页面 | 1061 KB（3000 段 + 3000 广告 span + script） | 同左 | 同左 |
| 耗时 | 88 ms | 38 ms | 28 ms |
| 提取 | 正文 954 KB | 全文档 1061 KB（含结构字节，需配合元素裁剪） | — |
| 内存 | O(n)：ego-tree 全量物化 | 常量【联网】（lol_html 文档与 docs.rs #876 案例） | 同左 |
| 语义 | 完整（选择器/strip/段落切分全可用） | 仅文本/元素回调；无 DOM 查询 | 同左 |

## 判据对照（任务书 D6）

- "500KB 页内存/耗时对比 DOM 方案" → 已测（1061KB、88 vs 38/28 ms）✅
- "可行则出集成设计" → 设计要点（若未来集成）：
  1. 触发条件：响应体 > `limits.maxBytes` 且 content 选择器为单一容器文本
  2. 流式管线：`lol_html` + `element!(content_sel)` 标记 → 文本回调收集 →
     `strip` 目标元素 `el.remove()`；`paragraphs` 退化为按块级标签切分
  3. 不支持：`map`/`blocks`/`protect`/transform 全部 —— 大页 fallback 只保留 content 纯文本
  4. 风险：与现有 DOM 路径的结果差异需要新文档档位（如 `content.mode: "stream"` 显式声明）

## 附：研究依据

- lol_html 常量内存与速度优势：【联网】https://github.com/cloudflare/lol_html 、
  https://github.com/rust-lang/docs.rs/issues/876 、https://www.zenrows.com/blog/rust-html-parser
- 现内核 L3 实测 0.24s/页已达标（七猫 L3）——性能非当前瓶颈，故不集成（V2-D5）
