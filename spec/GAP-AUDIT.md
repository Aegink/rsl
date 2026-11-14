---
id: rsl-gap-audit
type: module-design
status: draft
title: 与现网引擎的能力差距审计（决策记录）
parent: rsl-arch
tags: [rsl, audit, decisions]
---

# 与现网引擎的能力差距审计

> 这份文档回答一个问题：**RSL 的每个字段为什么存在**。
> 它记录与 Legado（阅读 3.0 系）的能力对照、取舍决定，以及**明确不采纳**的设计。
> 改规范前先看这里，避免把已经被否决的东西重新引入。

审计依据（2026-11 复核）：

- 源码切片：`legado-E/analyzeRule/*.kt`、`BookSource.kt`、`BookContent.kt`、`JsExtensions.kt`（四版本对比）
- 知识库：`03_Knowledge/阅读书源引擎/`（20 篇源码级模块笔记）、`03_Knowledge/Legado书源/`（24 篇语法/配置层）
- 真实源样本：四类现网源样本（本地研究资料）

---

## 0. 结论

RSL 1.0 的**语法层**确实优于 Legado，但**能力层**只覆盖了"单站、单载体、无登录、纯 HTTP"的源——大约占现网源的 30%。差距不在字段数量，在三个结构性缺失：

1. **没有条目级载体模型**：一个源里不同书可能是小说/漫画/短剧（现网用 `module`/`tab_type`/`book.type` 三套枚举硬凑）。
2. **没有状态分层**：源配置、登录令牌、会话、缓存、契约上下文被混在几套变量域里。
3. **假装宿主不需要扩展点**：段评、书架、发现页筛选器无处安放。

本版（1.1）把这三件事连同 P0/P1/P2 全部能力一次补齐。

---

## 1. 能力差距矩阵

编号 `A<n>`；「落地」列指向规范章节。等级：**P0** 不补则主流站点写不出，**P1** 决定能否覆盖大类站点，**P2** 可后置。

### P0 · 致命

| # | 缺口 | 现网证据 | RSL 1.0 | 决定 | 落地 |
|---|---|---|---|---|---|
| A1 | 请求参数编码 | `url_charset` 与 `charset` 是两个独立开关，GBK 站搜索关键词必须按 GBK 编码 | 只有响应 `charset` | `request.encode.{params,body}` 与 `charset` 分离 | §4.3 |
| A2 | Cookie / 登录态读写 | `java.getCookie(tag,key)` / `cookie.setCookie` / `removeCookie`；手动 Token 注入是番茄系第一块砖 | 只有 `cookies: none\|session\|persistent` | `cookie:` 选择器 + `ctx.cookie` + `login.submit.cookie` | §4.5 |
| A3 | 逐资源请求头 + bytes 级解密契约 | 漫画/图片几乎 100% 防盗链，图 URL 逐个挂 `,{headers}`；`imageDecode` 入参出参都是 bytes | `media.headers` 全局一个 Referer；`imageDecode` 未定义类型 | 媒体项自带 `headers`；`media.decode` 声明 `in/out: bytes` | §4.6、§6.6.3 |
| A4 | 字体反爬 | `queryTTF` + `replaceFont`；番茄/晋江系必需 | 完全缺失 | `transform.font` + `features:["font"]` | §3.6、§9.1 |
| A5 | 密码学原语不全 | `JsEncodeUtils` 四版本各 30 个函数：AES/DES/3DES/HMAC/SHA256/hex/URL 编码 | `expr:` 只有 md5/sha1/base64，`features:["crypto"]` 没有函数表 | `expr:` 加密函数族 + `transform.crypto` + **按算法声明** `requires.crypto` | §4.4、§9.1 |
| A6 | 条目级载体类型 | `module=3/2/8/11/4/19` ↔ `book.type=0/32/64/4` 两套映射，聚合源一本书一个类型 | `type` 只在源根 | `out.map.type` 逐条覆盖 + `typeMap`；规定唯一映射链 | §2.3、§6.2 |
| A7 | 合法空 ≠ 失败 | 书架/历史/用户列表必须允许合法空；公共榜单空才重取 | 列表空即 `E_NO_MATCH` | `out.emptyOk` + `E_EMPTY`（info 级）+ `degraded` | §3.8、§11.3 |
| A8 | 不发起请求的阶段 | `ruleContent: {"url": ""}` 是唯一"别请求、我自己来"的表达，缺了就 `EOFException` | `request` 缺省=拼 baseUrl，没有"明确不请求" | `request.skip: true`；`parse` 语义正式定义 | §4.2、§6.1.2 |
| A9 | 分页游标透传 | 除 `offset` 外还要原样传播 `search_id/passback/session_id`；老 UGC 用 `next_offset`，新 commentapi 用字符串 cursor | 只有 `next`/`pageVar` | `paginate.kind: cursor` + `cursor.{take,into}` + `carry` | §4.7 |
| A10 | 第二轨道（弹幕/歌词） | `subContent` → `isAudio` 存 lyric、`isVideo` 存 danmaku（`BookContent.kt:149/155`） | `audio`/`video` 字段里没有轨道概念 | 统一 `tracks[]`（kind/format/url\|text），轨道失败不影响主内容 | §6.6.1 |

### P1 · 重要

| # | 缺口 | 现网证据 | 决定 | 落地 |
|---|---|---|---|---|
| A11 | WebView 取数与嗅探 | `webJs`（必须返回非空否则死循环）+ `sourceRegex` 资源嗅探，是音视频源唯一取法 | `request.engine/webJs/sniff` + `autoUpgradeIf` | §4.8、§9.3 |
| A12 | 多档媒体 | 短剧多清晰度靠 JS 现拼；`codec`/`size`/音色 `tone_id` 都是真实约束 | `video.variants[]` / `audio.tone`；宿主选档，规则不拼字符串 | §6.6 |
| A13 | 发现页筛选器 | `ExploreKind` 的 `select/toggle/button/text`，聚合源靠它切模式/线路/画质 | `explore.filters`（**数据选择声明**，不是控件编程）+ `when` 门控 | §6.3 |
| A14 | 源级持久化配置 | `source.getVariable()` 是所有真实源的配置单根 JSON，且不随源导出 | `vars.<k>.{value,label,options,persist,secret}` | §5.2 |
| A15 | 并行 fan-out | `java.ajaxAll(urls)` 是聚合/多容器源标配 | `let.<k>.{parallel,limit}` + `limits.maxParallel` | §6.1.1、§8.1 |
| A16 | 阶段保留字段表 | `SearchRule`/`BookInfoRule`/`TocRule`/`ContentRule` 各有一套字段 | search/explore/detail/toc/content 五张保留字段表进规范（编译器据此查拼写） | §6.2–§6.6 |
| A17 | URL 归属识别 | `bookUrlPattern` 用于"这本书属于哪个源"（换源/加书架/同名源隔离） | 顶层 `match.detail` | §2.1 |
| A18 | 逐章规则成本 | 目录阶段耗时 ≈ 章数 × 逐字段规则 × 跨桥次数；实测 807 章 × 3 字段 ≈ 19 秒撞穿预算 | **编译期** `E_COST`：逐章规则禁止 IO/脚本/不确定性 | §6.5.1、§8.3 |
| A19 | 缓存维度 | 完整结果长 TTL、部分结果短 TTL；缓存键必须含 book/mode/source | `cache.<k>.{key,ttl,ttlPartial}` | §4.6、§12.3 |
| A20 | 结果校验 | `checkKeyWord`（结果不含校验词即判失败），防"200 但内容是别的" | `out.verify` | §3.8 |
| A21 | 调试轨迹 | Legado 的 `[⇒◇┌└≡]` 符号流不可断言 | 结构化 `--trace`（含 `bridge` 跨桥计数）进标准 | §13.1 |

### P2 · 次要

| # | 缺口 | 决定 | 落地 |
|---|---|---|---|
| A22 | `dnsIp` / 代理账密 / TLS 指纹 | `http.dns` / `http.proxy` / `http.tls` | §4.1 |
| A23 | 分发与更新 | `rsl pack`（校验和+签名）、`rsl build`（确定性装配） | §14 |
| A24 | 测试证据等级 | L0–L5 等级进测试格式，低等级不得写成高等级可用 | §13.2 |
| A25 | 扩展阶段（书架/历史/推荐/作者页） | `hostStages` 声明式契约，宿主声明支持哪些 | §6.8 |

---

## 2. 明确不采纳的设计

以下写法在 RSL 里永远不合法。每条都对应一个真实需求，但都有更干净的替代：

| 反例 | 它解决的问题 | 为什么不采纳 | 替代 |
|---|---|---|---|
| `data:name;base64,<json>,{opts}` 伪协议 | 跨阶段传结构化上下文 | 上下文进 URL 后不可校验、不可 diff、不可搜索 | `out` 契约 + `var:` |
| `{{ }}` 内执行任意表达式（含副作用，如删 cookie） | 动态值注入 | 模板里跑副作用，求值时机未定义 | 只允许 `var:` / `expr:` |
| `##正则##替换##` 尾符号编码操作类型 | 一次替换 vs 全文替换 | 用符号表达操作类型，无法组合 | `transform.regex.all` 布尔 |
| `@put` / `@get` 规则内变量 | 跨规则传值 | 四套变量域混用，作用域不清 | `let` + `session` |
| 单源内聚合多站点 | 一站式搜索 | 一个源 = 一个站点；聚合是宿主职责 | 宿主 `searchMany` |
| 任意自定义 UI 控件（`loginUi` / `push`） | 面板、开关、线路切换 | UI 进数据层导致"漏拷 action 全哑""this 断层静默失败" | `vars.persist` + `filters` + `hostStages` |
| 全字段可写 JS，作用域注入宿主对象 | 极限适配 | 绑定宿主语言，"80% 源在非 JVM 环境失效" | `lib.pure` / `lib.ctx` + 受控 `ctx` |
| 逐章规则里发请求/读配置 | 少写几个字段 | 成本乘以章数，实测 19 秒超时 | 编译期 `E_COST` |

---

## 3. 与 1.0 审计的对应关系

1.0 版审计用 `G1–G15` 编号，已全部被本版覆盖：

| 旧编号 | 本版 |
|---|---|
| G1 分页算术 | 已由 `expr:` 解决（§4.4） |
| G2 explore 分组 | §6.3（并扩展出 `filters` A13） |
| G3 登录体系 | §4.5（并扩展出 Cookie 读写 A2） |
| G4 多步请求 | §6.1.1（并扩展出并行 A15） |
| G5 目录 URL ≠ 详情 URL | §6.4 `tocUrl` 保留字段 |
| G6 版本协商 | §2.2 + Schema 根级 `additionalProperties: true` |
| G7 载体类型 | §2.3 + §6.6（条目级，A6/A12） |
| G8 目录语义 | §6.5 |
| G9 编码优先级 | §6.7 + 请求编码 A1 |
| G10 源级可配变量 | §5.2（A14） |
| G11 缓存 | §4.6（A19） |
| G12 WebView | §4.8（A11） |
| G13 图片解密/防盗链 | §4.6（A3） |
| G14 段评/章评 | §6.8 `hostStages`（A25） |
| G15 章节号归一 | §3.6 `numChapter` |

---

## 4. 未验证 / 待确认

诚实标注，不要当成已定事实：

- **性能数字**：19 秒目录、4800 次跨桥来自知识库的沐凡/PyFQ-Go 实测（807 章），未在本仓库复现。
- **四版本函数差异**：`ws*` 组仅 Rimchars 版有、`getApp*` 仅 refgd 版有——RSL 不复制这些宿主扩展，因此不作为能力目标。
- **`requires.crypto` 的算法集合**：当前按"现网常见"给出，尚未逐一验证各端库的可用性。
- **`hostStages` 的宿主支持范围**：没有任何宿主实现过，属规范先行。
- **`E_COST` 的判定边界**：`lib.pure` 允许在逐章规则里调用，但其步数仍会计入预算；是否需要更细的阈值待实现后校准。

---

## 5. 内核级补遗（A26–A44）

> **v2.0 已采纳**：A34 → `toc.reuseDetailHtml`；A35 → `protect`；A37 → `range` 负步长；A38 → `merge` 契约（§12.4）；A39 → `login.steps[]`（§4.5.2）；A41 → `content.direct`（§6.6.4）；A44 → `script.onResponse`（§7.5）。下表保留原始记录与证据，状态列标注去向。

来源：`KERNEL-COMPARE.md`（基于 Rimchars Archive v15 / commit `e111bf6` 全量 1742 个 `.kt` 逐层阅读）。这些是**读源码才暴露**的能力缺口，本版规范尚未覆盖，逐条待定：

| # | 缺口 | 现网证据 | 现状 | 处置建议 |
|---|---|---|---|---|
| A26 | `%%` 交叉合并 | `AnalyzeByJSoup.kt:110`（按下标 zip 交错） | `fallback` 只等价 `\|\|` | 加 `transform.zip` |
| A27 | `<1,2,3>` 页码枚举 | `AnalyzeUrl.kt:206-216` | 只能用 `expr:` 手算 | `paginate.enumerate` |
| A28 | `data:` URI 取数 | `AnalyzeUrl.kt:634` | 无 | `request.dataUri: true` |
| A29 | `type` → hex 二进制 | `AnalyzeUrl.kt:424` | `response:"bytes"` 可等价，但规范未写明路径 | 规范补等价写法 |
| A30 | `upload()` multipart | `AnalyzeUrl.kt:682` | 无 | 写进非目标 |
| A31 | `bodyJs` 响应二次处理 | `AnalyzeUrl.kt:507` | `script.after` 可等价 | 规范补等价写法 |
| A32 | `formatJs` 逐章标题格式化 | `BookChapterList.kt:139` | 被 `E_COST` 直接禁止 | 给阶段级 `script.after` 的批量后处理出口 |
| A33 | `title` 里拆 `imgUrl` | `BookContent.kt:184` | `toc.out.map.cover` 覆盖了结果，未覆盖写法 | 视需求决定 |
| A34 | **详情→目录 HTML 复用** | `WebBook.kt:303-311` | ✅ **v2.0 已采纳** | `toc.reuseDetailHtml`（§6.5.2） |
| A35 | **`<usehtml>` 占位保护** | `BookContent.kt:232-245` | ✅ **v2.0 已采纳** | `protect`（§3.6.2），失败报 `E_PROTECT_LOST` |
| A36 | **用户级净化层** | `ContentProcessor.kt:196-339`（用户自定义替换规则，单条 3000ms 超时后自动禁用） | 只有源级一层（`content.transform`） | 明确写进"宿主职责"，并给 `hostStages` 式端口，别让源承担用户偏好 |
| A37 | **反向切片简写 `[-1:0]`** | `AnalyzeByJSoup.kt:370` | ✅ **v2.0 已采纳** | `range: "-1:0:-1"`（§3.1） |
| A38 | **多源搜索结果合并语义** | `SearchModel.kt:99-107,126-194` | ✅ **v2.0 已采纳** | `merge` 顶层契约（§12.4），`crossBucket` 修掉跨桶重复 |
| A39 | **多步骤登录** | `SourceLoginJsExtensions.kt:49-132` | ✅ **v2.0 已采纳** | `login.steps[]` + `next.when`（§4.5.2） |
| A40 | 阶段级回退 | `BookList.kt:86`（发现 `bookList` 空 → **整条**回退 `searchRule`） | 只有选择器级 `fallback` | **不采纳**：整条回退会静默换义，要回退就显式写两个阶段（K12） |
| A41 | 纯 URL 正文 | `WebBook.kt:393-397` | ✅ **v2.0 已采纳** | `content.direct`（§6.6.4），从隐式兜底改为显式声明 |
| A42 | 发现行为控件 `button` | `ExploreKind.kt:6-26`（`type ∈ {url,text,button,toggle,select}`） | `filters` 只覆盖 `toggle`/`select` 的数据面；`button`（换线路、手动刷新）无对应物 | 归 `hostStages`，不进语言（K15） |
| A43 | 书单源 | `BookListRule.kt:6-17`（`SearchRule`/`ExploreRule` 都实现它） | 无书单概念 | **不采纳**：书单是宿主聚合概念，不是源类型（K16） |
| A44 | **`script.after` 能否改写原始响应** | `loginCheckJs` 实为响应改写器（`WebBook.kt:71-95`）；`bodyJs` 同理 | 规范没写清 `script.after` 作用在**原始字节**还是解析结果 | 必须定死。否则 `loginCheckJs`/`bodyJs` 的等价物不存在（K17） |
| A44 | **响应改写钩子** | `loginCheckJs` 实为响应改写器（`WebBook.kt:71-95`）；`bodyJs` 同理 | ✅ **v2.0 已采纳** | `script.onResponse`（§7.5），作用于解码后、`parse` 前的原始响应 |
> **剩余待定**（9 条）：A26 `%%`（有 `zip` 可替代，优先级低）、A27 `<1,2,3>`（已被 `enumerate` 覆盖，待删）、A28 `data:`、A29 `type`→hex、A30 `upload()`、A31 `bodyJs`（已被 `onResponse` 覆盖）、A32 `formatJs`、A33 `title` 拆 `imgUrl`、A36 用户级净化层（写进宿主职责，待补端口定义）、A40 阶段级回退（明确不做）、A42 发现 `button`（归 `hostStages`）、A43 书单源（明确不做）。
>
> 判定口径：**凡是 v2.0 已写入正文的，按正文执行**；上表"已采纳"只是历史记录，不再作为待办。
