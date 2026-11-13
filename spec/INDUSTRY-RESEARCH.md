# RSL 业界方案调研报告（INDUSTRY-RESEARCH）

> 调研对象：Scrapy / Firecrawl / Crawl4AI / Jina Reader / Trafilatura / Readability.js / Boilerpipe / JSONPath RFC 9535 / CSS Selectors L4 / OpenAPI 3.1-3.2 / JSON Schema 2020-12 / HAR 1.2 / Playwright / Puppeteer / RSS-Atom / schema.org / Tachiyomi-Mihon / Suwayomi / Deno permissions / wasmtime-WASI / QuickJS
> 结论导向：给 RSL（JSON 声明式书源语言，Rust 内核，编译到 iOS/Android/WASM）提可落地的改法。
> 调研方式：直接抓取官方文档/规范原文（RFC、W3C、docs.scrapy.org、docs.crawl4ai.com、docs.firecrawl.dev、playwright.dev、docs.deno.com、json-schema.org、spec.openapis.org、har 规范仓库、各项目 README）。
> 说明：本报告中的 JSON 片段是**建议草案**，沿用你现有语汇（`css:` / `json:` / `slice` / `pick` / `transform` / `out` / stage script），新增字段以 `_x_` 标注不重要，重点是**语义与形状**。

---

## 一、调研清单

### 1. Scrapy（Python 爬虫框架）
**一句话定位**：组件化、中间件驱动的通用爬虫框架，事实上的爬虫"标准骨架"。

**该学**：
- **Item Field 的"字段声明即契约"**：`Item` 必须声明 `Field`，未声明字段直接抛 `KeyError`，这是**廉价的 typo 防护**（docs.scrapy.org/topics/items.html）。RSL 的 `out` 已经接近，但缺少"未声明字段写入即报错"的硬约束。
- **Item Loader 的 input/output processor**：同一字段多次抽取的结果先经 input processor（逐值）再经 output processor（聚合），这让"多选择器合并成一个字段"成为一等公民（`TakeFirst` / `Join` / `MapCompose`）。
- **组件优先级字典**：`ITEM_PIPELINES = {"a.B": 300, "c.D": 800}`，数值越小越先执行；settings 有明确优先级层级（默认 < 项目 < 命令 < spider），且支持"组件覆盖组件"（`_BASE` 继承、addon priority）。
- **Downloader Middleware 的三钩子模型**：`process_request` / `process_response` / `process_exception`，且允许 `process_request` 返回 `Request`（重调度）或 `Response`（短路），`process_exception` 返回 `None` 表示交给下一个中间件。**这是"可组合的重试/限速/反爬"的标准形态。**
- **异常即控制流**：`IgnoreRequest`（静默丢弃请求）、`DropItem`（丢弃条目，可带 log_level）、`CloseSpider(reason, error=True)`（带机器可读 reason）、`NotConfigured`（组件自我禁用）。**每个异常都携带语义而非仅仅报错**。
- **`RetryMiddleware` + `get_retry_request()`**：重试策略是独立可替换对象，不是硬编码。
- **`LinkExtractor` 的参数矩阵**：`allow/deny/allow_domains/deny_domains/deny_extensions/restrict_xpaths/restrict_css/restrict_text/tags/attrs/deny_tags/deny_attrs/schemes/canonicalize/unique`。这几乎就是"分页/链接发现"应该具备的全部旋钮，而且 **deny 优先于 allow**、**有默认的 `IGNORED_EXTENSIONS` 黑名单**。
- **Spider Contracts**：`@url` / `@returns items 1 16` / `@scrapes Title Author`，让"规则自测"变成声明的一部分，`scrapy check` 可批量跑。**这是书源健康检查的现成范式。**
- **`JsonValidationMiddleware`**：把"响应校验"放到下载器层，校验失败即视为错误并重试。

**不该学**：
- 全代码即配置（spider 是 Python 类）。RSL 的价值就是**不执行任意代码**，不能退回"写 Python"。
- 中间件通过"返回不同对象表达不同意图"（返回 `None` vs `Request` vs `Response`）——在 JSON 里没法表达"返回 None"，必须改成显式的 `{action: "continue"|"retry"|"drop"}`。
- 同步/异步混用的隐式调度，和 RSL 阶段式执行模型冲突。

### 2. Firecrawl / Crawl4AI / Jina Reader
**一句话定位**：把"网页 → LLM 友好结构化内容"做成产品级管道，重点解决**渲染、降噪、多格式输出**。

**该学**：
- **Jina Reader 的 `x-respond-with` 多格式枚举**：`markdown` / `html` / `text` / `screenshot` / `pageshot` / `frontmatter` / `markdown+frontmatter`。**同一份抓取，多种投影（projection）**，而不是反复重抓。（注意 `frontmatter` 与普通 `markdown` 的区别是"是否过 readability 过滤"。）
- **Jina 的 `x-engine: browser | curl | auto`**：**先轻后重**——auto 优先走 curl 路径，判断需要 JS 再升级浏览器。直接对应 RSL 的"渲染降级链"。
- **Jina 的 `x-max-tokens`（截断）vs `x-token-budget`（超预算直接拒绝）**：两种预算语义必须分开。RSL 目前只有 `maxBytes`，缺"拒绝 vs 截断"的显式区分。
- **Firecrawl 的 `formats[]` + `onlyMainContent` + `actions`（waitFor / 点击 / 滚动）+ `metadata.statusCode` + `provider.steps`**：输出里带**执行轨迹**，便于诊断。
- **Crawl4AI 的 `fit_markdown`（BM25 / Pruning 过滤后的正文）与 `raw markdown` 并存**，并且 `content_source` 可选（raw html / cleaned html / fit html）。**"原文 + 过滤后"双份输出**对阅读器非常有用（正文要有标点，导航要去掉）。
- **Crawl4AI 的链接引用化 `[text][1]` + 尾部链接表**，以及 `ignore_links` / `skip_internal_links` 细粒度开关。

**不该学**：
- LLM 驱动的 `/extract`（prompt + JSON Schema 让模型抽数据）。书源要的是**确定性、可复现、零成本**，LLM 抽取无法调试、无法回归测试、延迟不可控。
- 云服务强绑定的计费/并发模型。
- 把 markdown 当作唯一中间表示：小说正文里 markdown 的 `_`、`*`、`#` 会与正文冲突（"他说：**你**"），RSL 正文应保留结构化块而非 markdown 字符串。

### 3. Trafilatura / Readability.js / Boilerpipe
**一句话定位**：无规则/低规则的主正文抽取算法族，用于"网站改版但没改规则也能出内容"。

**该学**：
- **Readability 的 `isProbablyReaderable(document, {minContentLength:140, minScore:20})`**：**先低成本判断"这页能不能走通用降级"，再决定跑重算法**。这是性能关键——不要每页都跑全量正文识别。（Readability 还有 `charThreshold:500`、`nbTopCandidates:5`、`linkDensityModifier` 等可调参数。）
- **Readability 认 JSON-LD**：`disableJSONLD` 选项说明它**默认优先用 schema.org 字段**做元数据。→ 印证"ld+json 兜底"是业界默认做法。
- **Trafilatura 的"规则 + 算法混合"**：官方评测明确写 "Rule-based approaches such as trafilatura's obtain balanced results despite a lack of precision. **Combined with an algorithmic approach they perform significantly better than the other tested solutions.**" → **混合优于纯算法、也优于纯规则**。
- **Trafilatura 的朴素基线**（评测里作为 baseline）："提取 `JSON article` 元素内的纯文本，或 `p/code/quote` 元素组合" —— 这个 baseline 打赢了好几个算法库。**说明"通用兜底"用极简启发式就够了**，不必上完整算法。
- **Boilerpipe 的"链接密度 + 文本密度"判废** 仍然是最便宜有效的降噪手段（`linkDensity > 0.5` 判为导航）。

**不该学**：
- 纯统计文本密度做**章节列表**抽取（<a> 密集但全是章节链接，会被误判为 boilerplate）。目录页是"链接即内容"的页面，密度启发式必须能按 stage 关闭。
- 依赖 CJK 分词/语言的模型型抽取（成本高、移动端跑不动）。

### 4. JSONPath RFC 9535 与 CSS Selectors Level 4
**一句话定位**：两个必须精确对齐的"选择器语法底座"。

**JSONPath RFC 9535（2024-02 发布，Proposed Standard）确认能力**：
- 段类型：`$`、`@`、`.`名 / `['名']`、`[n]`、`[start:end:step]`、`[*]`、`..`（descendant）。
- **过滤选择器 `?<logical-expr>`**，支持 `&&` `||` `!`、`==` `!=` `<` `<=` `>` `>=`、存在性测试（`?@.a`）、括号分组、正则字面量 `/re/flags`。
- **函数扩展（§2.4）**：`length()`、`count()`、`match()`（全串匹配）、`search()`（子串搜索）、`value()`（nodelist → value）。函数有**类型签名与 well-typed 校验**（§2.4.3 给出 `$[?length(@) < 3]` 合法而 `$[?length(@.*) < 3]` 不合法的规则）。
- **"单一定值查询（singular query）"** 概念：只有 singular query 才保证 nodelist 长度为 1，这是 `value()` 能被安全使用的前提。
- 官方有 **JSONPath Compliance Test Suite**（jsonpath-standard/jsonpath-compliance-test-suite），可直接拿来当 RSL 的 `json:` 后端回归集。

**CSS Selectors Level 4 确认能力**：
- 逻辑伪类 `:is()` / `:where()` / `:not()`（`:not()` 可带选择器列表）、**关系伪类 `:has()`（`4.5`，参数是相对选择器，隐式以 `:scope` 为锚）**。
- 结构化伪类 `:nth-child(An+B [of S])`、`:nth-last-child()`、`:nth-of-type()`、`:nth-last-of-type()`、`:first/last/only-child`、`:first/last/only-of-type`、`:empty`、`:root`。
- 属性选择器 `[attr]` `[attr=v]` `[attr~=v]` `[attr|=v]` `[attr^=v]` `[attr$=v]` `[attr*=v]`，**以及 `[attr=v i]` / `[attr=v s]` 大小写敏感修饰符**。
- 组合子：后代、`>`、`+`、`~`、**`||`（列组合子）**；`:scope`；`:lang()`、`:dir()`。
- **`<forgiving-selector-list>`**：列表里某一条解析失败时**整条忽略而非整体报错**（`is()`/`where()`/`has()` 的参数是 forgiving 的）。这点对"一个选择器写错拖垮整个书源"的健壮性极重要。
- Playwright 在标准 CSS 之上扩了 `:visible`、`:has()`、shadow DOM 穿透、"按布局选"（已废弃）。**`:visible` 很有价值**：小说站常把"广告/推荐位"设为 `display:none`，标准 CSS 选择器仍会选中它们。

**我们的判断**：**当前 RSL 的 `css:` 与 `json:` 语义没有显式声明"支持到哪一档"，这是最大隐患**——不同端（Rust 内核 / WASM / 未来 JS 实现）会各自退化。

### 5. OpenAPI / JSON Schema / JSON-LD
**一句话定位**：声明式协议的"版本化与扩展"成熟范式。

**该学**：
- **JSON Schema 的 dialect + vocabulary 双层模型**：`$schema` 声明**方言**（一套语义），`$vocabulary` 声明**本方言由哪些词汇表组成、哪些是必须理解的、哪些是可选的**。这是"不破坏兼容地加特性"的官方答案——新加的关键字放进**可选 vocabulary**，老实现直接忽略，新实现才生效。
- **"未知关键字默认忽略"**（JSON Schema 校验器的基本行为）：加字段不破坏旧解析器。
- **`format` 从断言降级为注解（annotation-only）**：JSON Schema 明确 `format` 默认只做注解、不断言。**"软约束"和"硬约束"要分层**，同理 RSL 里的建议性 lint（如"这个 css 选择器可能过脆"）不应阻断加载。
- **OpenAPI 的 `x-` 扩展前缀** + **minor 版本只加不删**（OAS 3.2 原文："Certain fields or features may be marked **Deprecated**... expected to remain part of the OAS until the next major version"）。
- **OpenAPI 的 `jsonSchemaDialect`**：文档级别声明"本描述里所有 schema 用哪个方言"，让演进可以按文档粒度切换。
- **JSON-LD 的 `@context` + `@type` + `@id`**：语义可外部寻址；`schema.org` 用它做跨站点的词汇共享。

**不该学**：
- OpenAPI 那句"Occasionally, **non-backwards compatible changes may be made in minor versions** where impact is believed to be low"——这是 OpenAPI 自己在道歉，别学。书源是**离线分发到用户设备**的，客户端版本更新滞后远大于服务端，破坏兼容的代价高一个数量级。
- JSON-LD 的完整解析（`@context` 远程加载、JSON-LD 1.1 的 framing/expansion）。我们只需要"从 `application/ld+json` 里按 JSON 取字段"。

### 6. HAR（HTTP Archive 1.2）
**一句话定位**：浏览器 DevTools 通用的"请求-响应全量录制"交换格式。

**结构确认**：`log{version, creator, browser, pages[], entries[], comment}`；`entry{ startedDateTime, time, request, response, cache, timings, serverIPAddress, connection, pageref }`；`request{method, url, httpVersion, headers[], queryString[], cookies[], postData{mimeType,text,params[]}, headersSize, bodySize}`；`response{status, statusText, httpVersion, headers[], cookies[], content{size, compression, mimeType, text, encoding}, redirectURL, headersSize, bodySize}`。**`content.encoding: "base64"` 是二进制（图片/字体）的标准表达。**

**该学**：
- **一个文件 = 一次会话的全部请求**，且 `pageref` 把 entry 归到 page。这正是"书源回放 fixture"需要的：**一次录制，覆盖 search→detail→toc→content 四个阶段**。
- `content.encoding` 的 base64 约定 + `mimeType`，用于 GBK 中文站点的**原始字节回放**（不能存成 UTF-8 文本，否则编码检测逻辑无法被测到）。
- `timings` 字段可用于**模拟慢速响应**，测超时/重试逻辑。

**不该学**：
- HAR 的庞大冗余（`headersSize`/`bodySize`/`cache`/`timings` 大多用不上），**不要直接规定"RSL fixture 必须是 HAR"**。做成"**可导入 HAR，内部用精简格式存**"。

### 7. Playwright / Puppeteer 的 selector 与 codegen
**一句话定位**：测试自动化领域的"选择器生成 + 录制即生成"。

**该学**：
- **codegen 的选择器优先级**：官方原文 "prioritizing **role, text and test id** locators"；且"If the generator finds multiple elements matching the locator, it will **improve the locator to make it resilient** that uniquely identify the target element"。→ **自动生成选择器时，优先语义/文本锚点，其次结构，且自动消除歧义**。
- **Locator 链式组合**：`page.locator('ul > li').filter({hasText:'x'}).nth(2)`、`frameLocator()`。**"选择器 → 过滤 → 取第 n 个"是一条链**，不是一次性字符串。
- **`css=` / `xpath=` / `text=` 显式引擎前缀** + **`>>` 链式引擎**。**RSL 的 `css:` / `xpath:` 前缀思路与此完全一致，方向正确**，可以借鉴 `>>` 做链式。
- **`expect(element).toBeVisible()` 等"录制断言"**：codegen 能一键生成"断言这段文字包含 X"。→ 书源自测生成器应能"点一个元素 → 生成 `assert` 子句"。
- **Playwright 布局选择器已被官方标注 deprecated**（"Matching based on layout may produce unexpected results... a different element could be matched when layout changes by one pixel"）。→ **不要设计"按坐标/按视觉位置"的选择器**，这条坑别人已经踩过。

**不该学**：
- 完整的浏览器自动化 DSL（click/type/hover 的完整语义）。RSL 只需要极少量"渲染准备动作"。
- 把所有选择器都塞进一个字符串（Playwright 自己的 `>>` 链式在复杂场景下已经很难读）。

### 8. RSS / Atom 与 schema.org Book / Article
**一句话定位**：站点自愿暴露的结构化数据，是零成本兜底源。

**该学**：
- **RSS/Atom 作为"更新流"的标准源**：`<item><title><link><pubDate><description>` / Atom `entry{title, link[rel=alternate], updated, id, summary, content}`。小说站的"最近更新"页常有 RSS，可作 `explore` 阶段的免费兜底。
- **`application/ld+json` 里的 `schema.org/Book`**：`name`、`author`（`Person.name`）、`isbn`、`bookEdition`、`bookFormat`、`numberOfPages`、`inLanguage`、`publisher`、`datePublished`、`aggregateRating`、`workExample`（Work→Edition 关系，Google Book 文档明确区分 **Work（抽象作品）与 Edition（具体版本，带 ISBN）**）。
- **`schema.org/Article`**：`headline`、`datePublished`、`dateModified`、`articleBody`、`author`、`image`。`articleBody` 是**正文兜底的金矿**（Trafilatura 的 baseline 就是"提取 JSON article 元素内的文本"）。
- **Google Book 文档的 Work/Edition 二分**：RSL 的 `detail.out` 应显式区分 `book`（作品）与 `edition`（版本/卷），否则多版本站点会串字段。

**不该学**：
- 依赖站点一定有 ld+json（中文小说站覆盖率实际很低，多数是模板站，几乎没有）。
- RSS 作为**唯一**入口（很多站把 RSS 关了或只有摘要）。

### 9. Tachiyomi / Mihon 扩展体系与 Suwayomi
**一句话定位**：漫画阅读器的"源"生态，扩展 = 每源一个 Android APK。

**事实**：Mihon（Tachiyomi 的社区延续）扩展是 **APK 分发、每源一个扩展包**；Suwayomi-Server 的卖点是 "Installing and executing **Mihon (Tachiyomi)'s Extensions**"，即**把同一批 APK 跑在 JVM 服务端**，客户端通过它拿源——这是"**用服务端抹平移动端分发问题**"，不是真正解决了分发。

**该学**：
- **源仓库（repo index）模式**：一个 `index.json` 列出所有源、版本号、minAppVersion、下载地址、语言分类、NSFW 标记。→ RSL 的**书源仓库索引**应包含 `minEngineVersion`、`capabilities`、`trust`、`sha256`。
- **引擎版本与源版本的兼容校验**：Tachiyomi 用 `extVersionCode` + `libVersion` 双版本号，`minAppVersion` 不满足则拒绝安装。→ RSL 需要 `engine: ">=1.4 <2.0"`。
- **能力探测**：老客户端遇到不认识的源能力（如某新版源用到的 `webview` 能力）应**禁用该源而不是崩溃**。
- **Suwayomi 的教训**：把"执行"从客户端搬到服务端，客户端就只需要一个通用协议。→ **RSL 应该有"远程执行器"抽象**（本地 Rust 内核 / 服务端 / 浏览器扩展都能跑同一份 RSL）。

**不该学**：
- **每源一个二进制包**——这正是 RSL 要避免的。扩展里能写任意 Kotlin 代码，安全边界为零，审核成本随源数量线性增长。
- 依赖 JVM/Android API 的源代码（`Jsoup` 的 `Element` 直接暴露给源作者），导致无法跨端。

### 10. Deno permissions / WASM 沙箱（wasmtime、WASI、QuickJS）
**一句话定位**：受限脚本执行的权限模型与沙箱实现。

**该学（Deno）**：
- **默认全禁、显式授予**：`--allow-net` / `--allow-read` / `--allow-env`。
- **按资源粒度授权**：`--allow-net=example.com`（只允许这个主机）、`--allow-read=./data`（只允许这个目录）、`--allow-env=API_KEY`（只允许这一个变量）。**这是 RSL 的 `http` 接口应采用的形态**：不是"允许联网"，而是"允许访问这些 host"。
- **`--deny-*` 优先于 `--allow-*`**（`--allow-read --deny-read=/etc` 读一切除 /etc）。→ **deny 优先于 allow** 的原则，与 Scrapy LinkExtractor 一致。
- **权限不可自我升级**："Code can not escalate its privileges without user consent"。
- **权限与"执行时间/内存"分离**：Deno 的权限模型只管 IO，不算计费/时限。RSL 的 `budget` 应只管时间/内存/字节。
- **Permission broker**：把权限请求集中到一个审核点（对 RSL 来说就是"书源安装时一次性审阅其声明的 `permissions`"）。

**该学（WASM/WASI）**：
- **能力（capability）模型**：WASI 的 `preopen` 目录、只给 fd 不给路径。**模块没有"路径"概念，只有被授予的资源句柄**——这比"路径白名单"更难绕过（无 TOCTOU、无 symlink 逃逸）。
- **Fuel / epoch 中断**：wasmtime 的 `consume_fuel` 与 `epoch_interruption` 是**确定性、可中断**的资源计量，比"看门狗线程 kill"可靠。→ RSL 的 `script` 钩子如果最终做 WASM，应使用 fuel 计量而非墙钟超时。

**该学（QuickJS）**：
- 可嵌入、体积小（QuickJS-ng 是活跃维护的 fork），适合移动端；但**无内置权限模型**，必须由宿主注入受限全局对象（即 RSL 的 `vars/http/cache/log`）。

**不该学**：
- Deno 的"同线程同权限级"模型（原文："All code executing on the same thread shares the same privilege level"）——RSL 如果多个书源在同一进程跑，需要**每源独立上下文**，否则一个源的权限会被另一个源借用。
- 让脚本 `eval` / `new Function` / 动态 `import`。RSL 的脚本应**只允许一次编译、不可动态求值**。

---
## 二、可直接吸收的设计建议

> 每条：建议 → 依据 → 落到 RSL 的具体改法 → 收益/成本。建议已按"是否影响可用性"排序。

### S1 · 选择器能力分级声明（**最高优先级**）
**依据**：Playwright 的 `css=` / `xpath=` 引擎前缀；CSS Selectors L4 的 `<forgiving-selector-list>`；JSONPath RFC 9535 的 well-typed 函数签名。
**问题**：RSL 现在只写"支持 CSS3 + 常用 L4"，但没规定**具体到哪一档**。Rust `scraper`、Swift、JS 侧实现会各退一步，同一份源在不同端行为不同——这直接违反"多端一致"。
**改法**：

```jsonc
"requires": {
  "engine": ">=1.0 <2.0",
  "selectorLevel": { "css": "4", "xpath": "1.0", "jsonpath": "9535" },
  "features": ["js", "time", "crypto"]
}
```

- 规范固定一张"能力矩阵表"：每个 level 必须支持哪些伪类/函数（`:has()`、`:nth-child(An+B of S)`、`length()`、`match()`）。
- `:is()` / `:where()` / `:not()` / `:has()` 的参数按 **forgiving** 处理：其中某条解析失败只忽略该条，不让整个源报错。
- `jsonpath` 直接用官方 **JSONPath Compliance Test Suite** 做回归，跑不过的实现不得声称支持 9535。

**收益**：多端行为可预期，这是"兼容性强"的实质。**成本**：写一张矩阵表，实现端按表补齐。

### S2 · 分页与链接发现的"旋钮矩阵"
**依据**：Scrapy `LinkExtractor` 的 `allow/deny/restrict_css/deny_extensions/canonicalize/unique`（且 **deny 优先于 allow**）。
**改法**：把 `paginate` 从"一个选择器"升级为可配置对象，但**全部字段可选**，最简写法不变：

```jsonc
"paginate": {
  "next": { "$": "css:a.next", "attr": "href", "absolute": true },
  "max": 50,
  "deny": ["re:logout", "re:\\?page=1$"],       // deny 优先
  "dedupe": true,
  "concurrency": 4,
  "order": "sequential",                        // 多 URL 时按原顺序拼接
  "stopIf": { "empty": "var:items" }
}
```

**收益**：解决"翻回首页死循环""并发后章节乱序"两个真实故障。**成本**：低。

### S3 · 渲染降级链（先轻后重）
**依据**：Jina Reader 的 `x-engine: browser | curl | auto`（auto = 先 curl，需要时升浏览器）；Readability 的 `isProbablyReaderable`（先低成本判断再跑重算法）。
**改法**：

```jsonc
"request": {
  "url": "https://example.com/book/1",
  "engine": "auto",          // http（默认）| webview | auto
  "autoUpgradeIf": { "css": "#app > div", "empty": true }   // 条件满足才升级
}
```

- `auto`：先 HTTP；若响应满足 `autoUpgradeIf`（目标容器为空/被 JS 占位）→ 自动升级 WebView 重取一次。
- 好处：90% 的站不需要 WebView 开销，剩下 10% 也不会失败。

**收益**：兼顾性能与覆盖率。**成本**：中等（需要 WebView 宿主端口）。

### S4 · 正文抽取兜底（规则 + 算法混合）
**依据**：Trafilatura 官方评测原文：*"Rule-based approaches ... Combined with an algorithmic approach they perform significantly better than the other tested solutions."* 且其 baseline 是"取 `article` 里的 `p/code/quote` 元素"；Readability 的 `linkDensity > 0.5` 判导航。
**改法**：

```jsonc
"content": {
  "out": {
    "content": {
      "$": "css:div#content",
      "auto": { "fallback": true, "minLength": 200 }   // 规则失败或过短时启用
    }
  }
}
```

`auto.fallback` 的算法顺序（规范固定，保证跨端一致）：
1. `<article>` / `[itemprop=articleBody]` 内取块级文本
2. `ld+json` 的 `articleBody`
3. 链接密度 < 0.5 且文本长度最大的容器

**收益**：站点改版时源不至于立刻报废。**成本**：约 300 行（不引入机器学习）。

### S5 · 版本演进：方言 + 可选词汇
**依据**：JSON Schema 的 `$schema`（方言）+ `$vocabulary`（可选词汇，老实现忽略）；"未知关键字默认忽略"；OpenAPI 的 `x-` 前缀 + "minor 只加不删"。
**改法**（已部分落地）：

1. **Schema 根级 `additionalProperties: true`**（已完成）——未知字段进编译器告警而非拒绝。
2. `require` 用**区间**表达：`"engine": ">=1.4 <2.0"`，取代 Tachiyomi 式的单值 `minAppVersion`。
3. `format` 类软约束（如"这个 CSS 选择器看起来过脆"）只出 `W_*` 警告，**不阻断加载**（学 JSON Schema 把 `format` 降级为 annotation）。
4. 新增能力一律走 `features` 声明 + `x-` 扩展，**不改主版本**。

**收益**：避免重蹈 OpenAPI 自认的"minor 里做破坏性变更"。**成本**：已在改。

### S6 · 测试：Contracts + HAR 双轨
**依据**：Scrapy Contracts（`@url` / `@returns items 1 16` / `@scrapes Title Author`，`scrapy check` 批量跑）；HAR 1.2 的 `entries[]` + `content.encoding: base64` + `pageref`。
**改法**：

- **契约内联在源里**（可选，仿 `@returns`）：

```jsonc
"meta": {
  "contracts": [
    { "stage": "search", "vars": { "kw": "测试" }, "returns": { "min": 1, "max": 20 } },
    { "stage": "toc", "scrapes": ["url", "title"] }
  ]
}
```

- **fixture 支持导入 HAR**：一次录制覆盖 search→detail→toc→content，`pageref` 映射到 stage 名；GBK 站点用 `content.encoding: "base64"` 存原始字节，**否则编码检测逻辑测不到**。
- 内部存精简格式，**不强制 HAR**（HAR 冗余字段太多）。

**收益**：源可回归，改版能定位。**成本**：中等。

### S7 · 权限模型：按资源粒度授权 + deny 优先
**依据**：Deno `--allow-net=example.com`、`--deny-*` 优先于 `--allow-*`、权限不可自我升级；WASI 的 capability（只给句柄不给路径）；wasmtime 的 fuel/epoch 确定性中断。
**改法**：

```jsonc
"permissions": {
  "net": ["example.com", "cdn.example.com"],      // 不是"允许联网"，而是允许这些 host
  "deny": ["admin.example.com"],
  "storage": { "vars": true, "cache": true }
}
```

- **加载时一次性审阅**（Deno 的 permission broker 思路），运行期不可升级。
- 多个源在同一进程跑时，**每源独立上下文**（学 Deno 的反面教训：同线程共享权限级会被借用）。
- 脚本预算用**步数（fuel）**而非墙钟（wasmtime 做法），保证可复现。

**收益**：安全边界清晰，是"扩展性强"不变成"扩展成坑"的前提。**成本**：低到中。

### S8 · 结构化数据兜底（`ld+json`）
**依据**：Readability 默认启用 JSON-LD；schema.org `Book`/`Article`；Trafilatura baseline 用 `articleBody`。
**改法**：`detail` 阶段可选 `"autoMetadata": true`，引擎自动从 `<script type="application/ld+json">` 取 `name`/`author.name`/`image`/`isbn` 等，**只在规则未声明该字段时生效**。正文同理（S4 第 2 步）。
**注意**：中文小说站 ld+json 覆盖率低，**只能当兜底，不能当主路径**。**成本**：约 150 行。

### S9 · 分发：源是数据，不是代码
**依据**：Mihon 每源一个 APK；Suwayomi 用服务端"抹平"分发；Tachiyomi 的 `extVersionCode` + `libVersion` + `minAppVersion` 双版本校验。
**改法**：

- 书源仓库索引 `index.json`：`{id, version, engine, capabilities, trust, sha256, langs}`。
- 客户端遇到不认识的 capability → **禁用该源并提示升级**，而不是崩溃。
- **远程执行器抽象**：本地 Rust 内核 / 自托管服务端 / 浏览器扩展，跑同一份 RSL（学 Suwayomi 的通用协议思路，但不学它的 APK 方案）。

**收益**：避免"每源一包"的审核与维护灾难。**成本**：低（主要是约定）。

### S10 · 调试输出结构化
**依据**：Firecrawl 返回 `metadata.statusCode` 与 `provider.steps`；Crawl4AI 同时输出 raw 与 fit。
**改法**：`rsl run --trace` 输出 JSON 数组，每元素 `{stage, step, rule, matched, ms, sample}`；可选 `--keep-raw` 同时保留原始响应与解析结果，便于"是没匹配到，还是匹配错了"。
**收益**：替代 Legado 的符号流。**成本**：低。

---

## 三、不值得学的设计

| 来源 | 不学什么 | 原因 |
|---|---|---|
| Firecrawl `/extract` | LLM（prompt + schema）抽取 | 不确定、不可回归、有延迟与成本；书源必须确定性 |
| Crawl4AI | 以 markdown 为唯一中间表示 | 正文里的 `*` `_` `#` 会与 markdown 冲突；RSL 用结构化 block |
| Trafilatura | 语言模型型抽取、CJK 分词依赖 | 移动端跑不动，且不可解释 |
| Trafilatura/Boilerpipe | 对**目录页**用文本密度判废 | 目录页"链接即内容"，会被误判为导航；必须按 stage 关闭 |
| OpenAPI | "minor 里允许破坏性变更" | OpenAPI 自己在文档里道歉；书源离线分发，客户端滞后更严重 |
| JSON-LD 完整解析 | `@context` 远程加载 / framing | 只需按 JSON 取值，不需要语义网那一套 |
| HAR | 强制用 HAR 做 fixture 格式 | 冗余字段过多；只做"可导入" |
| Playwright | 布局选择器（按坐标选） | 官方已 deprecated：布局变一像素就选错 |
| Playwright | 完整浏览器自动化 DSL | 书源只需要"渲染准备"，不需要 click/type 全套 |
| Scrapy | "返回 None / Request / Response 表示不同意图" | JSON 表达不了；改成显式 `{action: "continue"\|"retry"\|"drop"}` |
| Tachiyomi | 每源一个 APK | 审核与维护成本线性爆炸，安全边界为零 |
| Deno | 同线程共享权限级 | 多源同进程会互相借用权限 |
| Rhino（Legado） | 把宿主对象注入脚本作用域 | 一旦暴露就永远绑死某个运行时 |

---

## 四、优先级排序

| 优先级 | 建议 | 理由 |
|---|---|---|
| **高** | S1 选择器能力分级 | 不做则"多端一致"是空话 |
| **高** | S5 版本演进（方言+可选词汇） | 已发现实际矛盾，属于工程信誉 |
| **高** | S3 渲染降级链 | 决定能否覆盖动态站点 |
| **中** | S4 正文抽取兜底 | 大幅降低源失效频率 |
| **中** | S2 分页旋钮 | 解决死循环与乱序两个真故障 |
| **中** | S7 权限模型 | 安全边界，扩展时不失控 |
| **中** | S6 测试双轨 | 可维护性的核心抓手 |
| **低** | S8 ld+json 兜底 | 中文站点覆盖率低，锦上添花 |
| **低** | S10 结构化调试 | 体验改善 |
| **低** | S9 仓库索引与远程执行器 | 属于生态议程，不是标准本身 |

---

## 五、一句话总结

业界在"声明式抓取"这件事上已经收敛出几条共识，我们恰好在正确方向上，但缺三样东西：
**① 明确的能力等级（否则多端必然漂移）；② 先轻后重的降级链（否则覆盖率上不去）；③ 规则失败时的算法兜底（否则源维护成本下不来）。**
这三样补齐后，RSL 才真正同时满足"兼容性强"和"维护成本低"。

---

## 六、补充建议（S11–S12）

### S11 · 选择器的"well-typed"校验（编译器层）
**依据**：JSONPath RFC 9535 §2.4.3 明确区分 `$[?length(@) < 3]`（合法，`@` 是 singular query）与 `$[?length(@.*) < 3]`（**不合法**，`@.*` 返回 nodelist 而非单值）。规范原文要求实现做 **well-typedness 检查**。

**这条对我们的价值**：它证明了"选择器需要类型系统"在标准层面是成立的，不只是一个好主意。

**落到 RSL**：

```jsonc
// 我们的管道类型检查（COMPILER.md §3.2）就是同一件事
{ "$": "css:.titles", "transform": [{ "regex": {"regex": "x"} }] }
//    ^ 返回 NodeList              ^ 要求 Str  → E_TYPE（编译期，带修复建议）
```

**额外收获**：`json:` 实现若直接把 RFC 的 well-typed 规则抄过来，可以免费获得同样的静态检查；同时官方 Compliance Test Suite 里包含 well-typed 的**反例**，可直接作为我们编译器的测试用例。

**成本**：低。**收益**：错配在编译期而非运行期暴露。

### S12 · 预算语义二分：截断 vs 拒绝
**依据**：Jina Reader 同时提供 `x-max-tokens`（截断到上限）与 `x-token-budget`（超出直接拒绝），两者语义不同且**不能混用**。

**我们当前的问题**：`limits.maxBytes` 只说了"最大字节"，但没说超了之后是**截断继续**还是**报错终止**——这两种行为对书源作者意味着完全不同的调试体验。

**落到 RSL**：

```jsonc
"limits": {
  "maxBytes": { "value": 2097152, "on": "reject" },   // reject | truncate
  "maxNodes": 5000,
  "timeoutMs": 20000
}
```

| 预算 | 推荐默认 | 理由 |
|---|---|---|
| `maxBytes` | `reject` | 大响应通常是异常页（登录墙/错误页），截断后解析出垃圾更难排查 |
| `maxNodes` | `truncate` | 超大列表截断比失败有用（如 5000 章之后的目录） |
| `timeoutMs` | `reject` | 超时没有"部分结果"的合理语义 |
| `maxPages` | `truncate` | 翻到上限是有意义的"取了前 N 页" |

**收益**：行为可预期，且默认值有明确理由。**成本**：极低（改一行 Schema）。

---

## 七、这轮调研里最有价值的三个"反直觉"发现

1. **Trafilatura 官方评测承认纯规则不如混合**——"Rule-based approaches obtain balanced results despite a lack of precision. Combined with an algorithmic approach they perform significantly better." 这对我们意义重大：**规则引擎再完美，也需要算法兜底**，这是设计前提而不是附加功能。

2. **JSONPath 官方标准里就有类型检查**（well-typedness）。说明"声明式选择器需要编译期类型系统"不是我们的原创顾虑，而是标准社区已经验证过的共识。我们的 `E_TYPE` 设计方向是对的。

3. **Playwright 自己把布局选择器标成了 deprecated**——"matching based on layout may produce unexpected results... a different element could be matched when layout changes by one pixel"。这给我们的启示是：**看起来聪明的选择器抽象会随时间变成负担**，宁可要朴素、可预期的东西。这也反过来印证了我们拒绝 Legado 私有方言的判断。
