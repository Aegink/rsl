# RSL ⇄ Legado 内核逐层对比（v3）

> 状态：**分析文档，不入规范图**。它记录的是"我们凭什么这么设计"，不是契约。
> 契约在 `RSL-1.0.md`；缺口台账在 `GAP-AUDIT.md`。
> 面向使用者的对比（写法/能力/迁移）见 [`docs/comparison-vs-legado.md`](../docs/comparison-vs-legado.md)。

**对比口径**

| 侧 | 对象 | 性质 |
|---|---|---|
| 阅读 | Legado，Rimchars Archive v15，commit `e111bf6`（3.26.09242230） | 可跑的现网内核 |
| RSL | 本仓库 v2.0（`spec/RSL-1.0.md` + `crates/` 参考实现） | 设计 **+ 已落地内核**（2026-11） |

> **实现状态注记（v4）**：v3 写作时 RSL 是"零实现的设计"，因此本文只比分层、做法、代价、缺口，
> 不比"更快更稳"。该口径已随内核交付作废——RSL 现有 57 项双引擎测试、fuzz 713 万轮零 panic、
> 10,671 真实源压测与 QuickJS/Boa 基准（2.0–3.4×），实测数据汇总见
> [`docs/comparison-vs-legado.md` §7](../docs/comparison-vs-legado.md)。
> 本文的分层与源码证据仍然全部有效。

---

## 0. 依据与本版的更正

### 0.1 上一版（v1）错在哪：8 条认账

v1 基于一个 **legado-E 切片**（只含 analyzeRule/entities/webBook 三个目录），不是完整仓库。由此产生的错误：

| # | v1 的说法 | 事实 | 证据 |
|---|---|---|---|
| 1 | 内核 8,673 行 | ≈**27,400 行** / 196 文件（全仓 1,742 `.kt` / 387,291 行） | 全仓统计 |
| 2 | 无 `Mode.WebJs` | **有** | `AnalyzeRule.kt:546-567`、`AppPattern.kt:10` |
| 3 | 并发翻页乱序 | **目录/正文保序**；乱序的是**搜索** | `FlowExtensions.kt:110-126`、`SearchModel.kt:59-70` |
| 4 | 去重按对象 | **按 url**（`BookChapter.equals` 只比 url） | `BookChapter.kt:111-118` |
| 5 | `charset` 兼作响应解码 | **完全不参与** | `OkHttpUtils.kt:57` 不传 encode |
| 6 | Rhino 无沙箱 | **有**：黑名单 + 只读包装 + 禁隐式全局 | `RhinoClassShutter.kt:46-179`、`SharedJsScope.kt:76` |
| 7 | 绑定 4 套 | **93 处注入 / 13 文件**，`java` 在 7 个上下文是不同对象 | 见 §6 |
| 8 | 宿主 API 67 个 | `JsExtensions` **120 fun**（49 个 `@JavascriptInterface`）+ `JsEncodeUtils` 30 + `RegexJsExtensions` 6 | `JsExtensions.kt:95` 等 |

### 0.2 本版（v3）的读取范围

v2 读了 11 层、33 个 Kotlin 文件。v3 在此之上补齐三块此前只有粗结论的区域：

| 本轮新增区域 | 读的文件 | 关键产出 |
|---|---|---|
| 搜索与发现编排 | `SearchModel.kt`、`BookList.kt`、`FlowExtensions.kt`、`ExploreFragment.kt`、`ExploreShowViewModel.kt`、`BookSourceExtensions.kt`、`ExploreKind.kt`、`SearchBookMergeUtils.kt` | 双并发数字、4 桶重排、三层三键去重、无分页源无限翻页 |
| 登录与凭据 | `ui/login/` 全部、`RowUi.kt`、`FlexChildStyle.kt`、`BaseSource.kt`、`BookSource.kt`、`CookieStore.kt`、`CookieManager.kt`、`HttpHelper.kt` | 5 种控件、两条登录路径、`loginCheckJs` 实为响应改写器、无自动重登 |
| 数据模型与多类型源 | `BookSource.kt`、`BookSourceType.kt`、`BookSourceExtensions.kt`、`rule/*.kt`、`BookContent.kt`、`BookInfo.kt` | 32 字段 / 64 叶子 / 90 可填项、5 种源类型、8 级前缀优先级、4 个死字段 |

### 0.3 本版把 v2 的 4 处粗说法改精确

| v2 说法 | v3 精确化 |
|---|---|
| "搜索并发靠 `flatMapMerge`" | 有**两个不同数字**：Flow 并发 = `AppConfig.threadCount`（默认 **16**），执行线程池 = `min(threadCount, 9)` = **9**。同时在飞 16 个源，非挂起段只有 9 线程。 |
| "搜索到达序 + 重排" | 重排**不在解析层**，在 UI 数据层 `SearchModel.mergeItems`，每收到一个源就整体重算一次。 |
| "去重" | 不是一层，是**三层三键**：源内按 `bookUrl`（`LinkedHashSet`）→ 跨源按 `name+author` 折叠 → DB 按 `bookUrl` 主键 `REPLACE`。**三层键互不相同**。 |
| "字段多" | 精确：`BookSource` **32 个声明字段**（6 个规则对象 + 26 标量），规则对象展开 **64 个叶子**，合计 **90 个可填项**；其中 **4 个是死字段**。 |

---

## 1. 规模与分层

### 1.1 阅读（Rimchars Archive v15）

| 层 | 目录 | 行数 | 文件 |
|---|---|---|---|
| 规则求值 | `model/analyzeRule/` | 3,370 | 10 |
| 阶段编排 | `model/webBook/` | 1,825 | 6 |
| 数据模型 | `data/entities/` | 4,837 | 75 |
| 请求与传输 | `help/http/` | 3,747 | 22 |
| 书籍/内容处理 | `help/book/` | 7,453 | 38 |
| 书源管理 | `help/source/` | 597 | 5 |
| JS 引擎 | `modules/rhino/` | 2,651 | 31 |
| 宿主 API | `JsExtensions` + `JsEncodeUtils` + `RegexJsExtensions` | 1,905 | 3 |
| 缓存/限流/调试 | `CacheManager` 等 | 994 | 6 |
| **内核合计** | | **≈27,400** | **196** |

全仓 1,742 `.kt` / 387,291 行 / 40 MB。UI（`ui/`）约 21 万行，**不参与本对比**。

**但登录编排是例外**：`ui/login/SourceLoginDialog.kt` **816 行**，登录的表单构建、`loginUi` 解析、`reLoginView` 增量重绘、凭据写入全在这一层。也就是说**阅读的登录逻辑没法从 UI 里剥出来**——这是分层问题，不是规模问题。

### 1.2 RSL（参考实现，v3 写作时为设计估算）

12 模块：载体/选择器/请求/变量/阶段/脚本/预算/能力/元信息/错误/执行/工具链。估算 ≈12,000 行 Rust。比例 **1 : 2.3**。

### 1.3 表达力的数量对照

| 指标 | 阅读 | RSL |
|---|---|---|
| 书源可填项 | 90（32 字段 + 64 规则叶子） | 见 schema 23 个 `$defs` |
| 规则字符串语法 | 8 级前缀 + 6 种串内语法 + URL 侧 3 种 | 0 种（显式命名空间） |
| 阶段数 | 5（搜索/发现/详情/目录/正文） | 7 + `hostStages` |
| 源类型 | 5（文本/音频/图片/文件/视频），rss 是**另一个实体** | 6 载体（含 rss） |
| 死字段 | **4** | 0（schema 校验） |

---

## 2. 逐层对比

### 2.1 入口与宿主耦合

**阅读**：`WebBook` 是静态门面，5 个 `*Await` 函数，全部返回/接受可变实体 `Book`/`BookChapter`/`SearchBook`。宿主直接读写这些字段，没有"阶段输出"这一层。

**RSL**：阶段只通过 `out` 契约输出保留字段，宿主读 `out` 不读引擎内部。

**判定**：RSL 强。阅读的耦合点在于"宿主能改中间态"，一旦宿主改坏，源无法自证。

### 2.2 规则字符串 → 执行计划

**阅读**：规则是**纯 `String?`**，没有任何编译期结构。前缀判定是 `when` 链，**8 级优先级**（`AnalyzeRule.kt:546-575` 先切 JS/WebJs，`:602-635` 再判）：

```
@js: / <js>  →  @webjs:  →  @CSS:  →  @@  →  @XPath:  →  @Json:  →  isJSON|$.|$[  →  以 / 开头  →  默认 CSS
```

`isJSON` 是**运行时实例字段**（`:100-103`），同一实例里一旦置位，后续规则全部按 JSON 走。

**RSL**：规则在编译期定成 `Selector` 结构体，命名空间显式，无优先级链。

**判定**：RSL 强。8 级链 + 实例粘滞状态是"同一行规则换个上下文就变义"的根因。

### 2.3 选择器与组合符

**阅读**：三套索引方言 + 6 个硬编码取值词 + `&&`/`||`/`%%` 三种组合（`%%` 在 5 处重复实现）。`$n` 双义：列表下标，**只有出现在 replacement 段才是捕获组**（`AnalyzeRule.kt:722-731,494,503`）。

**RSL**：`range`/`pick`/`exclude` 三字段 + `fallback` + `transform` 管道；`match` 与 `regex.replace` 分开。

**判定**：RSL 强，唯一劣势是 `%%`（K1）。

### 2.4 请求与编码链

**阅读**：URL 四段字符串改写（`AnalyzeUrl.kt:159-298`）+ 两次 JS 钩子。`charset` 三态只管**请求**编码，响应解码完全不看它（`OkHttpUtils.kt:57`）。`type != null` → hex，且**绕过限流**（`:441-443`）。`origin` 是死字段（`:786-962`，全仓无 `getOrigin()` 调用）。

**RSL**：`request` 结构体 + `encode`/`charset` 分离 + 六级强制解码顺序。

**判定**：RSL 强。阅读的 `charset` 命名与其实际作用相反，是最容易踩的坑。

### 2.5 Cookie 与登录态

**阅读**：
- Cookie 键 = **eTLD+1 域名**（`CookieStore.kt:26-35`），**跨源共享**；超过 4096 条**随机删键**（`:78-83`）。
- WebView cookie 用**源 key 当伪域名**（`BackstageWebView.kt:215-222`）——跨域登录页会记错域。
- 登录凭据 AES/ECB/PKCS5，密钥 = **ANDROID_ID 前 16 字节**（`BaseSource.kt:172,223`）。
- **换账号不清 cookie**；唯一清理入口是菜单"删除登录头"。

**RSL**：隔离键 = `源 id` + 站点域名 + 登录名；`vars.secret` 不落日志。

**判定**：RSL 强。阅读的"按域名共享 + 按源存凭据"是两个互相矛盾的隔离维度。

### 2.6 状态与作用域

**阅读**：变量四级回退（`AnalyzeRule.kt:799-823`）；`RuleData` 大变量读不回（`RuleData.kt:19-21`）；源变量不存在 `BookSource` 字段里，而在 `CacheManager` 的 `sourceVariable_<url>`（`BaseSource.kt:242-269`），**参与发现分类缓存键**（`BookSourceExtensions.kt:31-48`）却**不参与正文缓存文件名**（`BookChapter.kt:124-134`）。

**RSL**：六层状态模型（§5.1），每层写明"可写者"，`vars.persist` 独立于 `cache`。

**判定**：RSL 强。阅读的源变量"一半是变量一半是缓存键"。

### 2.7 阶段编排

**阅读**：5 个阶段硬编码在 `WebBook` 里，`loginCheckJs` **五个阶段全都跑**（`WebBook.kt:71-95,147-171,223-247,320-344,422-449`）。详情→目录在 `bookUrl == tocUrl` 时**复用响应**省一次请求（`:303-311`）。

**RSL**：7 阶段 + `hostStages`；`toc` 无条件发请求（K5/A34）。

**判定**：RSL 的编排更干净，但**阅读多一个真实优化**（A34 值得抄）。

### 2.8 脚本沙箱

**阅读**：三件套——`RhinoClassShutter` 黑名单（`RhinoClassShutter.kt:46-179`）、`ProtectedNativeJavaClass` + `ReadOnlyJavaObject` 只读包装、`scope.preventExtensions()` 禁隐式全局（`SharedJsScope.kt:76`）。**无 wall-clock 超时**（`RhinoScriptEngine.kt:324-344`，`timeout` 在模块内 0 命中），只有协作式取消。支持**远程 jsLib**（下载 + 按 MD5 缓存，`SharedJsScope.kt:41-68`）。

**RSL**：`lib.pure`/`lib.ctx` 两档 + 白名单式 `ctx` + `libSteps`/`scriptTimeoutMs`；禁止远程 jsLib。

**判定**：RSL 强（超时与白名单），但阅读的沙箱**不是没有**，v1 说"无沙箱"是错的。

### 2.9 后处理与净化：两层 vs 一层

**阅读**：
1. 源级：`HtmlFormatter.formatKeepImg`（`BookContent.kt:239`），漫画走这条。
2. **用户级**：`ContentProcessor`（`ContentProcessor.kt:196-339`），用户自定义替换规则，单条 **3000ms 超时后自动禁用**。

**RSL**：只有源级 `content.transform`。

**判定**：**阅读强**。用户偏好不该由源承担，RSL 把这块推给宿主（A36），但没写"宿主必做"。

### 2.10 缓存与限流

**阅读**：三层（`CacheManager.kt:18-45`）——内存 `LruCache(50MB)` **不校验 TTL**（`:99-101,108-110`）；磁盘 Room `caches(key,value,deadline)`；二进制 `ACache`。**键空间不隔离**：`v_<key>_<name>`、`sourceVariable_<key>`、`loginHeader_<key>`、`userInfo_<key>` 与普通缓存**同表同 LRU**，只靠前缀区分；`clearSourceVariables()` 只清内存不清 DB（`:57-67`）。

限流：`"N/M"` = M 毫秒内 N 次，`"N"` = N 毫秒内 1 次，key = 源 key，**阻塞等待**（`ConcurrentRateLimiter.kt:21-31,103-121`）；`type` 路径可绕过。

**RSL**：`cache.<k>{key,ttl,ttlPartial}` 维度化 + `vars.persist` + `token` 三层分离；`http.rate{windowMs,max}` + `maxParallel`，不可绕过。

**判定**：RSL 强。"一个 KV 装一切 + 内存层不看 TTL"是真实的正确性风险。

### 2.11 错误与诊断

**阅读**：`Debug.log(sourceUrl, msg, print, isHtml, showTime, state)`（`Debug.kt:34-71`）：`[mm:ss.SSS]` 时间戳 + `state` 整数（1/10/20/30/40 分阶段）+ 按源过滤 + `withDebugSource` 作用域 + `isChecking`/`debugTimeMap` 计时。错误只有测试模式的 `-1..-7` 伪码（`AnalyzeUrl.kt:539-551`）；其余靠 `getString` 返回 `""`（`:357`）与 `isUrl` 兜底 `baseUrl`（`:364`）。

**RSL**：25 个结构化错误码 + `severity`/`path`/`hint`/`degraded`；`--trace` 输出含 `bridge` 跨桥计数。

**判定**：RSL 强。但阅读的 `Debug.log` 在**现场排查**上更贴脸（原文直接打出来），RSL 的 trace 只有设计。

### 2.12 搜索与发现编排

这是 v3 新增的一层，也是此前只有粗结论的地方。

**并发**：`flatMapMerge` 封装为 `mapParallelSafe`（`SearchModel.kt:89-98`，`FlowExtensions.kt:58-70`）。**两个数字**：

| 限制 | 值 | 证据 |
|---|---|---|
| Flow 并发（同时在飞的源） | `AppConfig.threadCount`，默认 **16** | `AppConfig.kt:2059-2060` |
| 执行线程池（非挂起段） | `min(threadCount, MAX_THREAD)` = `min(16, 9)` = **9** | `AppConst.kt:26` |
| 单源超时 | **30s** | `SearchModel.kt:90` |
| 整体超时 | **无** | `SearchModel.kt:76-115` 全程无 timeout |

**单源失败被吞掉**（`FlowExtensions.kt:64-68` 的 `catch (_: Throwable)`），该源当次静默 0 条。UI 只看到"结果少了一个源"，没有任何提示。重试**只在 HTTP 层**：次数来自源 URL 的 `retry` 选项（`AnalyzeUrl.kt:268,798,859-863`，默认 0），且 `OkHttpUtils.kt:36-42` 的 `for (i in 0..retry)` **只对非 2xx 重试，无退避**。

**重排**：不在解析层，在 UI 数据层 `SearchModel.mergeItems`（`:99-107`），**每收到一个源就整体重算一次**。算法是 **4 桶 + 桶内合并 + 桶间拼接**：

| 桶 | 判据 | 排序 |
|---|---|---|
| `equal` | `name==key \|\| author==key` | `sortByDescending { origins.size }` |
| `tags` | `kind.contains(key)` | 同上 |
| `contains` | `name.contains(key) \|\| author.contains(key)` | 同上 |
| `other` | 其余 | 不排序，`precision` 时才拼到末尾 |

合并键 = `name + author`，命中则 `pBook.addOrigin(nBook.origin)`（`:142-143` 等 4 处）。**排序键只有 `origins.size`（命中源数）**，没有相似度、没有来源权重、没有响应时间。`BookSourcePart.weight` 在 `model/webBook` 下 **0 引用**——字段存在但没人用。

**去重三层三键**：

| 层 | 键 | 证据 |
|---|---|---|
| 源内解析 | `bookUrl`（`SearchBook.equals`） | `BookList.kt:142-144`、`SearchBook.kt:65-67` |
| 跨源 UI | `name + author` 折叠成一条，累加 `origins` | `SearchModel.kt:138-186` |
| DB | `bookUrl` 主键 + `REPLACE` | `SearchBook.kt:21,31-32`、`SearchBookDao.kt:96` |

**事实缺口**：只在**同桶内**比较。同一本书若在 A 源落 `equal`、B 源落 `tags`，会保留两条。发现/套件侧又是另一套键 `"$sourceKey|$bookUrl"`（`SearchBookMergeUtils.kt:83-91`）。

**分页**：`searchPage` 全局共用，**无固定页大小**（由各源返回条数决定）。页码经 `AnalyzeUrl(page=)`（`WebBook.kt:62-70`）注入 JS 的 `page` 绑定（`AnalyzeUrl.kt:382`）与 URL 的 `<1,2,3>` 列表替换（`:218-228`，`<(.*?)>` 正则，超界取 `pages.last()`）。

**源不支持分页时**：URL 里没有 `<...>` / `{{page}}` → 每页请求同一 URL，结果折叠进同一条，列表不增长；但 `hasMore = hasMore || items.isNotEmpty()`（`:103`）**恒为 true** → UI 无限触底翻页。**无检测、无截断**。

**发现**：字段名是 `exploreUrl`（`BookSource.kt:82`，全仓 `findUrl` **0 命中**）。展开在 `BookSourceExtensions.kt:58-127`：`@js:`/`<js>` → 执行 JS（只绑 `infoMap`）；JSON 数组 → `ExploreKind` 列表；否则按 `(&&|\n)+` 切 `title::url`。失败塞一个 `ExploreKind("ERROR:…")`。

**数据结构是扁平的 `List<ExploreKind>`，没有树**。`ExploreKind(title,url,type,action,chars,default,viewName,style)`，`type ∈ {url,text,button,toggle,select}`（`ExploreKind.kt:6-26`）。所谓"多级"靠 UI 约定：现代页把"无 url/action 且整行宽"的 kind 当分组标题（`ExploreFragment.kt:2884-2887`、判定 `:2951-2957`）。子分类 = 用该 kind 的 url 再发一次列表请求，无递归预取。

**书单**：与普通书源同一套规则接口（`BookListRule.kt:6-17`），`SearchRule`/`ExploreRule` 都实现它。**发现规则 `bookList` 为空时整条回退到 `searchRule`**（`BookList.kt:86`，不是逐字段回退）。

**JS 变量按阶段不同**（三张表，`key`/`page`/`infoMap` 只在 URL 阶段有）：

| 阶段 | `java` 是谁 | 独有绑定 |
|---|---|---|
| 搜索/发现 URL（`AnalyzeUrl.evalJS`） | `AnalyzeUrl` | **`page` `key` `infoMap`** |
| 列表字段规则（`AnalyzeRule.evalJS`） | `AnalyzeRule` | `result`(=item) `src`(=item) |
| 详情/目录/正文 | `AnalyzeRule` | `chapter` `src`(=正文) |

**RSL**：§12.2 只说"`let.parallel` 与宿主 `searchMany` 都受 `limits.maxParallel` 约束"；§6.3 的 `explore` 是**显式嵌套 `groups`**（有二级 `groups` 的项本身不发起请求）+ `groupsFrom` 动态抓分类 + `filters` 数据选择。

**判定**：
- **发现结构 RSL 强**——显式树 vs 扁平列表 + UI 约定。
- **搜索合并语义 RSL 缺**——RSL 根本没定义"多源结果怎么合"，这是宿主职责但规范必须写清（K11）。
- **分页 RSL 强**——`stopIf: {empty:"out.list"}` + `max` 天然避免"无分页源无限翻页"。
- **重试 RSL 强**——`retry{times,backoff,on}` vs 阅读的"只非 2xx、无退避"。
- **暂停/恢复阅读有，RSL 无**（`workingState.first { it }`，`SearchModel.kt:80-86`）。

### 2.13 登录与凭据

**`loginUi` 的完整 schema**：`RowUi(name, type, action, chars, default, viewName, style)`（`RowUi.kt:3-22`），控件类型**只有 5 种**：`text` / `password` / `button` / `toggle` / `select`。

**没有 step / group 字段**。分组靠 `FlexChildStyle.layout_wrapBefore / flexBasisPercent / flexGrow`（`FlexChildStyle.kt:6-14`）；**多步骤靠 JS 调 `java.reLoginView(deltaUp)` 增量重绘**——按 name 对齐、id = index+1000。这是"先账号密码 → 出验证码 → 出短信码"的真实实现方式。`viewName` 三态：null → 用 name；`'...'`（长度 3..19 且首尾单引号）→ 字面量；其他 → 当 JS 求值（失败显示 `"null"`/`"err"`）。

**两条登录路径，二选一**（`SourceLoginActivity.kt:26-34`）：

| 路径 | 触发 | 行为 | 差异 |
|---|---|---|---|
| 表单 | `loginUi` 非空 | 建控件 → 控件 action 执行 `loginUrl+action` JS → OK 时 AES 存 `loginInfo` → 执行 `loginUrl + if(typeof login=='function'){login.apply(this)}` | 写 `loginInfo` |
| WebView | `loginUi` 为空 | `loginUrl` 当 **URL** 加载（不走 `getLoginJs` 的剥离）→ `onPageStarted/Finished` 抓 `CookieManager.getCookie(url)` 存 `CookieStore` | **不执行 `loginCheckJs`、不写 `loginInfo`** |

**`loginCheckJs` 的名字是错的**：它不是"校验"，是**响应改写器**——`evalJS(checkJs, result = StrResponse) as StrResponse`。请求异常时以 `getErrStrResponse` 为 result；若返回码 500 则重抛原异常。

**没有自动重登**：没有任何"返回 false 就重登"的机制，登录只能手动触发（22 处 `startActivity<SourceLoginActivity>`、JS `java.open("login")`、`java.login()`）。

**凭据存储**：`caches` 表，键 `userInfo_<sourceKey>`，值 = AES(Base64)，密钥 = **ANDROID_ID 前 16 字节**（hutool 默认 AES/ECB/PKCS5Padding，`BaseSource.kt:172,223`）。多用户隔离**只按 sourceKey**；同域不同源**共享 cookie、不共享 loginInfo**。

**Cookie 域模型**：键 = eTLD+1（IP 用 host）；`cookies(url, cookie)` 表 + 内存 `<domain>_cookie`；会话 cookie 在 `<domain>_session_cookie`（**仅内存**）；请求前 `setCookie` 把 DB cookie merge 进 `Cookie` 头（URL option 优先）；>4096 随机丢键。**WebView → OkHttp** 靠页面回调抓取；**OkHttp → WebView** 只有 `applyToWebView`，**仅 2 处调用**，登录 WebView 启动时**不回填**。

**与请求层的耦合**：登录请求走 `java.ajax/connect` → `AnalyzeUrl(source=书源)` → 同一 OkHttp 客户端族，**受限流**（`skipRateLimit=false`）、用源级 `dnsScope`。登录头仅在 `hasLoginHeader=true` 时合并。

**RSL**：`login { type: form|cookie|token|qr, fields, submit{ok,token,cookie}, check, headers }`；`E_LOGIN_REQUIRED` / `E_CHALLENGE`；隔离键 = 源 id + 域名 + 登录名。**§16 明确不采纳 `loginUi`**。

**判定**：
- **隔离与失效 RSL 强**（含登录名的隔离键、`check` 断言 + `E_LOGIN_REQUIRED`）。
- **表达力阅读强**：`reLoginView` 增量重绘能表达**多步骤登录**（验证码/短信码），RSL 的静态 `fields` 不能（K13）。"UI 不进标准"这条原则在这里**有真实代价**，不能一句打发。
- **`loginCheckJs` 与 RSL 的 `check` 不是同一个东西**：前者是响应改写钩子，后者是断言。等价物是 `script.after`，但需确认它能否改**原始响应**（K17）。

### 2.14 数据模型与多类型源

**字段规模（逐个数过）**：`BookSource.kt:32-103` 共 **32 个声明字段**——6 个规则对象（`ruleExplore`/`ruleSearch`/`ruleBookInfo`/`ruleToc`/`ruleContent`/`ruleReview`）+ 26 个标量。规则对象展开 **64 个叶子**（Search 11 / Explore 10 / BookInfo 12 / Toc 10 / Content 11 / Review 10）。合计 **90 个可填项**。

**源类型是 5 种**（`BookSourceType.kt:8-12`）：`0` 文本 / `1` 音频 / `2` 图片 / `3` 文件 / `4` 视频，经 `BookSourceExtensions.kt:160-167` 映射成 `Book.type` 位掩码（`WebBook.kt:204-205,297-298`）。

**载体专属输出**（RSL 的 `out.map.type` + `typeMap` 覆盖了这个，但阅读的做法更"约定"）：

| 类型 | 正文规则返回什么 | 证据 |
|---|---|---|
| 图片(2) | **HTML 片段**，再抽图片 | `BookContent.kt:239`、`BookHelp.kt:239-249` |
| 音频(1) / 视频(4) | **媒体地址字符串**，`BookContent.kt:230` 明确跳过 HTML 格式化 | `BookContent.kt:230` |
| 文件(3) | 不走 `tocUrl`，走 `ruleBookInfo.downloadUrls` | `BookInfo.kt:155,165-175` |

`subContent` 按载体分流：audio → 歌词，video → 弹幕。

**规则是纯 `String?`**，没有带类型标记的 Rule 对象；结构化只在容器层（GSON JSON 列，`BookSource.kt:266-314`）。

**空值兜底是最危险的部分**（22 条空值默认 + 26 条依赖关系，全在报告里带行号）。最关键的 4 条：

| 兜底 | 行为 | 证据 |
|---|---|---|
| 正文规则为空 | **不请求，直接拿 `chapter.url` 当正文** | `WebBook.kt:393-397` |
| 发现 `bookList` 为空 | **整条回退到 `ruleSearch`** | `BookList.kt:86` |
| `bookUrlPattern` | 决定"列表页还是详情页" | `BookList.kt:62-81,100-108` |
| `tocUrl`/`bookUrl` 为空 | 回退 `baseUrl` | 多处 |

**4 个死字段（源码可证）**：`exploreScreen` 无任何读取点；`ruleReview` 的 TypeConverter 恒返回 null/写 `"null"`（`BookSource.kt:308-312`）；`BookListRule.updateTime`、`BookInfoRule.updateTime` 声明但无读取点。

**源变量**：`BookSource` 里**没有** `variable` 字段，只有 `variableComment`；实际存在 `CacheManager` 的 `sourceVariable_<bookSourceUrl>`（`BaseSource.kt:242-269`）。

**章节去重键 = `url`**（`BookChapter.kt:111-118`）；DB 主键 `(url, bookUrl)` + 唯一索引 `(bookUrl, index)`。

**RSL**：6 载体枚举 `novel comic audio video file rss`；`out.map.type` 条目级覆盖 + `typeMap` 映射链；无死字段（schema 校验）。

**判定**：
- **RSL 强**：显式载体覆盖链 + 无死字段 + 无"空规则静默改义"。
- **阅读强**：`bookUrlPattern` 那种"列表页即详情页"的一步到位，RSL 需要 `detail` 与 `search` 各写一遍。
- **注意**：阅读的 rss 是**独立实体 `RssSource`**，不是 `BookSourceType` 的一种；RSL 把 rss 并进载体枚举，这是**故意不同**，不是遗漏。

---

## 3. 差异总表（58 条）

### A. 语言与规则层

| # | 维度 | 阅读（源码事实） | RSL（契约 + 实现） | 判定 |
|---|---|---|---|---|
| 1 | 解析器与 JS 宿主 | 同一对象（`AnalyzeRule:52`） | 分离 | RSL |
| 2 | 规则表示 | 纯 `String?`，90 个可填项 | JSON + 编译期定型 | RSL |
| 3 | 切分器 | `RuleAnalyzer` 378 行 | 不存在 | RSL |
| 4 | 模式判定 | 8 级前缀链 + 运行时 `isJSON` | 显式命名空间 | RSL |
| 5 | 实例级粘滞状态 | `isRegex` 一旦置 true 永久生效 | 无 | RSL |
| 6 | 组合子 | `&&`/`\|\|`/`%%`（5 处重复实现） | `fallback`（缺 `%%`） | 平 |
| 7 | 索引方言 | 两套私有语法 + 5 个前置词 | 三字段 + 命名空间 | RSL |
| 8 | 取值词 | 6 个硬编码 + 属性名（空值静默跳过） | 命名空间 | RSL |
| 9 | `$n` 语义 | 列表下标；replacement 段才是捕获组 | `match` / `regex.replace` | RSL |
| 10 | 死字段 | **4 个** | 0（schema 校验） | RSL |

### B. 请求层

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 11 | URL 构造 | 四段字符串改写 + 两次 JS 钩子 | 模板 + 结构体 | RSL |
| 12 | 请求编码 | `charset` 三态 + 已编码短路 | `encode` | 平 |
| 13 | 响应编码 | BOM→Content-Type→meta→ICU；`charset` 无效 | 六级强制顺序 | RSL |
| 14 | 二进制取数 | `type` → hex（**绕过限流**） | `response:"bytes"` | 平 |
| 15 | 页码枚举 `<1,2,3>` | 有 | 无 | **阅读** |
| 16 | `data:` URI | 有（仅 byte 路径） | 无 | **阅读** |
| 17 | `upload()` multipart | 有 | 无 | **阅读** |
| 18 | `bodyJs` 响应二次处理 | 有 | `script.after` 等价 | 平 |
| 19 | 重试 | 只对非 2xx，**无退避** | `retry{times,backoff,on}` | RSL |
| 20 | 限流 | 阻塞等待，`type` 可绕过 | 声明式，不可绕过 | RSL |

### C. 会话与登录

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 21 | Cookie 隔离 | 按 eTLD+1 域名（**跨源共享**），>4096 随机删键 | 源 id + 域名 + 登录名 | RSL |
| 22 | 凭据加密 | AES/ECB，密钥 = **ANDROID_ID 前 16 字节** | `vars.secret` 不落日志 | RSL |
| 23 | 登录表单表达力 | 5 种控件 + **`reLoginView` 增量重绘**（多步骤），816 行住在 UI 层 | `login.fields` 静态 | **阅读** |
| 24 | 登录态失效 | **无自动重登**，22 处手动入口 | `E_LOGIN_REQUIRED` + `check` | RSL |
| 25 | 登录路径 | `loginUi` 空→WebView，非空→表单；WebView 路径**不写 loginInfo** | `login.type` 四选一 | RSL |
| 26 | `loginCheckJs` 语义 | **响应改写器**（名字是错的），500 重抛 | `check` 是断言 | 平（不同物） |
| 27 | WebView cookie 回填 | 仅 2 处 `applyToWebView`，**登录不回填** | 无 WebView 登录 | RSL |
| 28 | 换账号 | **不清 cookie** | 隔离键含登录名 | RSL |

### D. 状态与脚本

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 29 | JS 变量集 | **三张表**：`key`/`page`/`infoMap` 只在 URL 阶段 | 1 套 `ctx` | RSL |
| 30 | 注入点 | 93 处 / 13 文件 / `java` 在 7 个上下文是不同对象 | 1 套 | RSL |
| 31 | 宿主 API | 120 + 30 + 6 个函数，49 处 `@JavascriptInterface` | 10 项 + `expr:` 函数族 | **阅读**（覆盖广） |
| 32 | 文件系统 | 对脚本开放（安全根 `externalCache.parent`） | 明确禁止 | RSL |
| 33 | 沙箱 | 黑名单 + 只读对象 + 禁隐式全局（`preventExtensions`） | 两档 + 白名单式接口 | RSL |
| 34 | 执行超时 | **无 wall-clock**，仅协作式取消 | `libSteps` + `scriptTimeoutMs` | RSL |
| 35 | 远程 jsLib | 支持（下载 + MD5 缓存） | 无（且不应有） | RSL |

### E. 阶段与编排

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 36 | 阶段通信 | 可变 `Book` 字段 | `out` 契约 | RSL |
| 37 | 详情→目录 HTML 复用 | 有（`bookUrl==tocUrl` 省一次请求） | 无 | **阅读** |
| 38 | 逐章成本 | 4~6 次 `getString`/章，运行时才炸 | `E_COST` 编译期拦 | RSL（未验证） |
| 39 | 正文规则为空 | **直接拿 `chapter.url` 当正文** | 需 `request.skip` + `var:chapter.url` | 平 |
| 40 | 阶段级回退 | 发现 `bookList` 空 → **整条**回退 `searchRule` | 只有选择器级 `fallback` | **阅读** |
| 41 | 搜索并发 | Flow 16 / 线程池 9（两个数字） | `searchMany` + `maxParallel` | 平 |
| 42 | 搜索超时 | 单源 30s，**无整体超时** | `limits` | RSL |
| 43 | 搜索合并 | 4 桶 + `name+author` 折叠 + `origins.size` 降序 | **未定义** | **阅读** |
| 44 | 搜索去重 | 三层三键，**跨桶不比较** | 未定义 | 平（都缺） |
| 45 | 分页终止 | 无页大小；无分页源 `hasMore` 恒 true → **无限触底** | `stopIf.empty` + `max` | RSL |
| 46 | 暂停/恢复 | `workingState.first { it }` | 无 | **阅读** |

### F. 发现与书单

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 47 | 发现结构 | 扁平 `List<ExploreKind>`，**无树**，UI 约定分组 | 显式嵌套 `groups` | RSL |
| 48 | 发现控件 | 5 类型，含 `button`/`toggle`/`select` 行为控件 | `filters` 只覆盖数据面 | **阅读**（`button` 无对应） |
| 49 | 动态分类 | `@js:` + `infoMap` 缓存 | `groupsFrom` | 平 |
| 50 | 书单 | `BookListRule` 复用 search/explore 规则 | **无书单概念** | **阅读** |

### G. 数据与缓存

| # | 维度 | 阅读 | RSL | 判定 |
|---|---|---|---|---|
| 51 | 源类型 | 5 类；rss 是**另一实体** | 6 载体（含 rss） | 平（故意不同） |
| 52 | 载体专属输出 | 漫画 HTML 片段 / 音视频地址串 / 文件 `downloadUrls` | `out.map.type` + `typeMap` | RSL |
| 53 | 源变量 | 无字段，`CacheManager` 的 `sourceVariable_` | `vars.persist` | RSL |
| 54 | 章节去重 | 按 `url`；DB `(url,bookUrl)` + `(bookUrl,index)` | 规范要求保序 + 键 | 平 |
| 55 | 缓存 | 三层，**内存层不看 TTL**，键不隔离 | 分层 + 维度化 + TTL | RSL |
| 56 | 净化层 | **两层**（源级 + 用户级 339 行） | 一层 | **阅读** |
| 57 | 诊断 | `Debug.log` 符号流 + `state` 1/10/20/30/40 | 结构化 trace | RSL |
| 58 | 错误 | `""` 兜底 + `isUrl` 兜底 `baseUrl` + `-1..-7` | 31 个错误码 + 三分语义 | RSL |

**计分**：RSL 强 35 / 阅读强 12 / 平 11（共 58 条）。

---

## 4. RSL 仍缺的能力（K1–K17）

### 4.1 确定要补（有真实收益，语义干净）

| # | 缺口 | 阅读实现 | 建议 |
|---|---|---|---|
| K5 | **详情→目录 HTML 复用** | `WebBook.kt:303-311` | `toc.reuseDetailHtml: true`（= A34，建议进 1.1） |
| K11 | **多源搜索结果合并语义** | `SearchModel.kt:99-107,126-194` | 规范写清宿主必须做什么：折叠键、排序键、跨桶比较。否则三端各写一套 |
| K13 | **多步骤登录** | `reLoginView(deltaUp)` 增量重绘 | `login.steps[]`，每步一组 `fields` + `when` |

### 4.2 有等价写法，但规范没写明

| # | 缺口 | 阅读实现 | 现状 |
|---|---|---|---|
| K2 | `<1,2,3>` 页码枚举 | `AnalyzeUrl.kt:218-228` | 只能用 `expr:` 手算 |
| K6 | `<usehtml>` 占位保护 | `BookContent.kt:232-245` | `transform` 是纯函数链，无"这段原文不许被碰到"的表达（= A35） |
| K10 | 反向切片 `[-1:0]` | `AnalyzeByJSoup.kt:370` | `range:"-1:0"` 结构上支持，正文没写负步长（= A37） |
| K14 | 纯 URL 正文 | `WebBook.kt:393-397` | `request.skip` + `out.content: "var:chapter.url"` 可行，需写进 §4.2.1 示例 |
| K17 | 改写原始响应 | `loginCheckJs` / `bodyJs` | `script.after` 是否作用于**原始字节**待确认 |

### 4.3 明确不做（记录理由，防反复提议）

| # | 缺口 | 阅读实现 | 为什么不抄 |
|---|---|---|---|
| K1 | `%%` 交叉合并 | `AnalyzeByJSoup.kt:110-117` | 按下标 zip 交错，语义只对"两个等长列表拼一条"成立；RSL 用 `transform.zip` 表达更明确，但**优先级低** |
| K3 | `data:` URI | `AnalyzeUrl.kt:645-682` | §16 已列反例：上下文进 URL 不可校验、不可 diff |
| K4 | `upload()` multipart | `AnalyzeUrl.kt:693-708` | 写进非目标（源不是上传工具） |
| K7 | 用户级净化层 | `ContentProcessor.kt:196-339` | 用户偏好不是源能力。但**必须写清"宿主必做"**（= A36） |
| K8 | `formatJs` 逐章格式化 | `BookChapterList.kt:139-157` | 与 `E_COST` 直接冲突；给阶段级批量后处理出口 |
| K9 | `title` 拆 `imgUrl` | `BookContent.kt:176-194` | `toc.out.map.cover` 已覆盖结果，写法待定 |
| K12 | 阶段级回退 | `BookList.kt:86` | 阅读是"整条回退"，会静默换义；RSL 要回退就该显式写两个阶段 |
| K15 | 发现行为控件 `button` | `ExploreKind.type=button` | 属宿主交互，用 `hostStages` 表达 |
| K16 | 书单源 | `BookListRule.kt:6-17` | 书单是宿主聚合概念，不是源类型 |

---

## 5. RSL 侧未验证的假设

| # | 假设 | 风险 |
|---|---|---|
| A1 | `E_COST` 能静态判定逐章跨桥 | `lib.pure` 循环只计步数；阅读的逐章成本来自 `setContent` + 6 次 `getString`，RSL 需要等价的可判据 |
| A2 | `lib.pure`/`lib.ctx` 二分够用 | 阅读 93 处注入里，多数既非纯函数也不该有 IO |
| A3 | `hostStages` 覆盖书架/推荐/段评 | 无宿主实现过 |
| A4 | `requires.crypto` 算法清单三端可给 | 阅读是"算法名随便传"（`JsEncodeUtils.kt:460`），RSL 按名声明，需核对各端库 |
| A5 | 编译期引用图覆盖 `var:` 拼写 | `out` 允许自由字段，仍可能漏 |
| A6 | 三端输出逐字节一致 | 依赖 QuickJS 版本与浮点行为 |
| A7 | `login.fields` 静态表单覆盖真实站点 | 多步骤/验证码/短信码是常态，K13 未落地前这条不成立 |
| A8 | `explore.groups` 显式树能表达"运营位/轮播/多 tab" | 阅读用扁平列表 + UI 约定绕过去了，RSL 的树更严格，可能装不下不规则布局 |
| A9 | 多源合并交给宿主不会各写一套 | 阅读自己就写了 4 桶算法；不写进规范，三端必然分叉（K11） |
| A10 | `script.after` 能改原始响应 | 若只作用于解析结果，`loginCheckJs`/`bodyJs` 的等价物不存在（K17） |

---

## 6. 源码证据索引

路径相对 `F:\legado-rimchars\app\src\main\java\io\legado\app\`。

### 规则求值

| 结论 | 证据 |
|---|---|
| 解析器即 JS 宿主 | `model/analyzeRule/AnalyzeRule.kt:52-56,831` |
| 8 级前缀判定 | `AnalyzeRule.kt:546-575,602-635` |
| `isJSON` 运行时决定引擎 | `AnalyzeRule.kt:100-103` |
| `isRegex` 粘滞 | `AnalyzeRule.kt:538-544` |
| `##` 四段 / `$n` 双义 | `AnalyzeRule.kt:763-773,722-731,494,503` |
| `getElements` 不展开 `$n` | `AnalyzeRule.kt:419-422` |
| 变量四级回退 | `AnalyzeRule.kt:799-823` |
| `RuleData` 大变量读不回 | `data/entities/rule/RuleData.kt:19-21` |
| `Mode.WebJs` | `AnalyzeRule.kt:546-567`、`AppPattern.kt:10` |
| 重入（30 分钟超时） | `AnalyzeRule.kt:912-951` |
| 切分器 | `model/analyzeRule/RuleAnalyzer.kt:165-298` |
| 三套索引方言 / 6 取值词 | `AnalyzeByJSoup.kt:283-292,406-511,229-279` |
| `%%` 上界=首分支 | `AnalyzeByJSoup.kt:110-117` |
| 反向切片 `[-1:0]` | `AnalyzeByJSoup.kt:370` |

### 请求与传输

| 结论 | 证据 |
|---|---|
| URL 四段 | `model/analyzeRule/AnalyzeUrl.kt:159-298` |
| `origin` 死字段 | `AnalyzeUrl.kt:786-962`（全仓无 `getOrigin()` 调用） |
| 响应解码链 / `charset` 无效 | `help/http/OkHttpUtils.kt:79-95,57` |
| `<1,2,3>` 枚举 | `AnalyzeUrl.kt:218-228,770` |
| `data:` / upload / type-hex | `AnalyzeUrl.kt:645-682,693-708,441-443` |
| 重试只非 2xx、无退避 | `OkHttpUtils.kt:36-42`、`AnalyzeUrl.kt:268,798,859-863` |
| 限流两语法 + 阻塞 | `help/ConcurrentRateLimiter.kt:21-31,103-121` |
| Cookie 按 eTLD+1 + 随机删键 | `help/http/CookieStore.kt:26-35,78-83` |
| WebView cookie 伪域名 | `ui/book/read/pageDelegate/BackstageWebView.kt:215-222` |
| `applyToWebView` 仅 2 处 | `help/http/CookieManager.kt:41-151` |

### 登录

| 结论 | 证据 |
|---|---|
| 5 种控件 | `data/entities/rule/RowUi.kt:3-22` |
| 分组靠 flex 样式 | `data/entities/rule/FlexChildStyle.kt:6-14` |
| 两条路径二选一 | `ui/login/SourceLoginActivity.kt:26-34` |
| 表单 816 行住 UI | `ui/login/SourceLoginDialog.kt`（全文件） |
| `reLoginView` / `upLoginData` | `ui/login/SourceLoginJsExtensions.kt:49-132` |
| WebView 路径不写 loginInfo | `ui/login/WebViewLoginFragment.kt:71-156` |
| 凭据 AES 密钥=androidId | `data/entities/BaseSource.kt:172,223` |
| `loginCheckJs` 是响应改写器 | `WebBook.kt:71-95` 等 5 处 |

### 阶段编排

| 结论 | 证据 |
|---|---|
| 详情→目录复用 | `model/webBook/WebBook.kt:303-311` |
| 两次 reverse / 去重按 url | `BookChapterList.kt:124-133`、`data/entities/BookChapter.kt:111-118` |
| 逐章 4~6 次求值 | `BookChapterList.kt:235,238-241,274-275` |
| `formatJs` 4 变量 | `BookChapterList.kt:143-147` |
| 正文规则空→url 当正文 | `WebBook.kt:393-397` |
| 并发保序 | `utils/FlowExtensions.kt:110-126` |

### 搜索与发现

| 结论 | 证据 |
|---|---|
| 双并发数字 | `model/webBook/SearchModel.kt:35,46-50`、`AppConfig.kt:2059-2060`、`AppConst.kt:26` |
| 单源 30s / 无整体超时 | `SearchModel.kt:90,76-115` |
| 单源异常被吞 | `FlowExtensions.kt:58-70` |
| 4 桶重排 | `SearchModel.kt:99-107,126-186,189-194` |
| `hasMore` 恒 true | `SearchModel.kt:103` |
| 暂停/恢复 | `SearchModel.kt:80-86,200-206` |
| 源内去重按 bookUrl | `BookList.kt:142-144`、`SearchBook.kt:65-67` |
| `exploreUrl` 字段名（`findUrl` 0 命中） | `data/entities/BookSource.kt:82` |
| 扁平 `ExploreKind` 无树 | `ExploreKind.kt:6-26`、`ExploreFragment.kt:2884-2887,2951-2957` |
| 分类展开与缓存 | `BookSourceExtensions.kt:58-127` |
| 发现回退 searchRule | `BookList.kt:84-88` |
| `infoMap` LruCache(99) | `ExploreAdapter.kt:69`、`ExploreFragment.kt:3412-3416` |

### 数据模型

| 结论 | 证据 |
|---|---|
| 32 字段 / 90 可填项 | `BookSource.kt:32-103` |
| 5 种源类型 | `BookSourceType.kt:8-12`、`BookSourceExtensions.kt:160-167` |
| 漫画正文走 HTML | `BookContent.kt:239`、`help/book/BookHelp.kt:239-249` |
| 音视频跳过格式化 | `BookContent.kt:230` |
| 文件走 downloadUrls | `BookInfo.kt:155,165-175` |
| `bookUrlPattern` 决定列表/详情 | `BookList.kt:62-81,100-108` |
| 4 个死字段 | `exploreScreen` 无读取；`BookSource.kt:308-312`；`BookListRule`/`BookInfoRule.updateTime` 无读取 |
| 源变量在 CacheManager | `BaseSource.kt:242-269` |

### 脚本、缓存、诊断

| 结论 | 证据 |
|---|---|
| 沙箱三件套 | `modules/rhino/RhinoClassShutter.kt:46-179`、`App.kt:309-314`、`NativeBaseSource.kt:10-38` |
| 禁隐式全局 | `model/SharedJsScope.kt:76` |
| 无 wall-clock 超时 | `RhinoScriptEngine.kt:324-344` |
| 远程 jsLib | `SharedJsScope.kt:41-68` |
| 93 处绑定 / 同名不同义 | `AnalyzeUrl.kt:378-394`、`ParagraphRuleProcessor.kt:401-414`、`AnalyzeRule.kt:831-843` 等 13 文件 |
| 宿主 API 120/30/6 | `JsExtensions.kt:95`、`JsEncodeUtils.kt:19`、`RegexJsExtensions.kt:10` |
| 缓存三层 / 内存无 TTL / 键不隔离 | `help/CacheManager.kt:18-45,88-110,57-67` |
| 两层净化 | `BookContent.kt:239`、`help/book/ContentProcessor.kt:196-339` |
| 替换规则 3000ms 超时+自动禁用 | `ContentProcessor.kt:277-301`、`ReplaceRule.kt:56` |
| 调试符号流 | `model/Debug.kt:34-71` |

---

## 7. 这份文档怎么用

1. **改规范前先查 §3**：如果一条设计在 §3 里被判"阅读强"，改之前必须回答"我们为什么愿意付这个代价"。
2. **§4.1 是待办，§4.3 是挡箭牌**：有人提 `data:` URI / `upload()`，直接指向 §4.3。
3. **§5 是诚实清单**：这 10 条都没被任何实现验证过。M1 编译器落地后逐条翻牌。
