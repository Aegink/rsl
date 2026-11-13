---
id: rsl-migration
type: guide
status: draft
title: Legado 书源 → RSL 迁移手册
parent: rsl-language
tags: [rsl, migration, legado, convert]
version: "2.0"
---

# Legado → RSL 迁移手册 v2.0

> 本文只讲一件事：把存量 Legado 书源转成 `RSL-1.0.md` v2.0 的源。
> **Legado 侧语义以 `KERNEL-COMPARE.md` 为准**（含源码证据），**RSL 侧字段名以 `RSL-1.0.md` 为准**。两边冲突时不信任何第三方文档。
> 工具：`rsl convert <legado.json>`（`RSL-1.0.md` §14）。

## 1. 先读这一页

`rsl convert` 的输出是**草稿**，不是成品。它产三样东西：

| 产物 | 内容 |
|---|---|
| `<id>.rsl.json` | 尽力转换后的源；能过 `rsl lint` 的形状检查 |
| `<id>.migration.json` | 不可转换/降级清单，逐条带 Legado 原字段与原因 |
| `<id>.checklist.md` | 本文 §6 的检查清单，已勾选"转换器已处理"的项 |

**为什么必须人工复核**：Legado 的规则是**纯 `String?`**（`RSL-1.0.md` §0.1、`KERNEL-COMPARE.md` §2.2），没有任何编译期结构；8 级前缀链 + 运行期 `isJSON` 粘滞状态意味着"同一行字符串换个上下文就变义"。转换器只能按最常见的解释翻，翻错的位置**不会报错，只会静默给错数据**——所以每一条被改写的规则都要 `rsl explain` 一次。

**一定转不了的五大类**（逐条清单见 §5）：

1. **让源操作宿主 UI 的东西**：`loginUi` 的 `button`/`toggle`/`select`、`reLoginView`、`java.open()`。
2. **远程代码**：远程 `jsLib`（§16 明确不采纳：源必须自包含、可审计、可离线）。
3. **伪协议与越界能力**：`data:` URI、`upload()` multipart、`java.*`（`JsExtensions` 120 个函数）。
4. **死字段**：`exploreScreen`、`ruleReview`（`KERNEL-COMPARE.md` §2.14 源码可证无读取点）。
5. **带宿主状态的隐式语义**：WebView 登录路径的 cookie 抓取、`loginCheckJs` 里"500 重抛"、逐章 `formatJs`。

### 1.1 转换器**不做**的判断（它没有依据，所以留给你）

| 它不会判断 | 后果 | 你怎么办 |
|---|---|---|
| 这条源是不是"列表页即详情页" | `search` 与 `detail` 各写一遍，或漏 `tocUrl` | 按 §2 表 #21 核 `bookUrlPattern` |
| `@js:` 里那些 `java.*` 调用原本想做什么 | 转不过去的脚本被整段丢弃 | 读 `migration.json` 里"被丢弃脚本"的条目，逐个人工重写 |
| 站点是不是 GBK/Big5 | 编码靠启发式，可能选错 | §6 第 4 条双向验证 |
| 登录走"表单"还是"WebView"路径 | 选错 → 永远 `E_LOGIN_REQUIRED` | 看 `loginUi` 空不空（§2.13：两条路径二选一） |
| 分页的终止条件 | 可能无限翻页 | 每条 `paginate` 手补 `max` + `stopIf` |
| `weight` / `customOrder` 是否真被用过 | 可能只是历史遗留 | 只在多源合并场景保留 `merge.weight` |

> **`customOrder` / `respondTime` 的明确去向**：RSL **没有**对应字段，也不打算有——章节排序用 `toc.order`，响应耗时是宿主遥测（不是源属性）。转换器必须把它们放进 `migration.json` 的 `dropped` 数组并写明原因，**不得静默丢弃**（`T5.6` 验收项）。

### 1.2 复核流程（每个源都走一遍，别跳）

1. `rsl lint <src>` — 先让形状对（§13.3 二十条）。
2. `rsl compile <src>` — 引用图/类型/成本诊断全在这里（`E_REF` `E_TYPE` `E_COST`）。
3. `rsl record <src> search --kw <真实关键词>` — 把真实响应录成 fixture，后续调试不再依赖站点在线。
4. 逐阶段 `rsl run <src> <stage> --trace` — 只信 `matched` 与 `bridge` 两个数字。
5. 对着 `migration.json` 一条条过：每条"不可转"要么补实现，要么在 `x-legado.*` 留档并在 `meta.comment` 写明能力缺失。
6. 把第 4 步的响应固化成 `rsl test` 用例，最后才 `rsl pack`。

## 2. 字段映射总表（24 个）

| # | Legado 字段 | Legado 真实语义（依据） | RSL 落点 | 转换器行为 |
|---|---|---|---|---|
| 1 | `bookSourceUrl` | 源唯一 key；同时是缓存/凭据键 `userInfo_<sourceKey>`、`sourceVariable_<url>` 的组成（§2.5 §2.6 §2.14） | `id`（清洗成域名式短标识）+ `baseUrl`（origin） | 已填则直取；空则从 `searchUrl` 推断 origin。**原值进 `x-legado.bookSourceUrl`**（`id` 改了就不能改回来） |
| 2 | `bookSourceName` | 显示名 | `name` | 直取 |
| 3 | `bookSourceGroup` | 分组串，常以 `,` / `&&` 分隔（宿主侧概念） | **无对应字段** | 存 `x-legado.bookSourceGroup`，并写一条 `meta.comment`；多组语义（一个源属多组）RSL 不表达 |
| 4 | `bookSourceType` | `0` 文本 / `1` 音频 / `2` 图片 / `3` 文件 / `4` 视频（`BookSourceType.kt:8-12`，§2.14） | `type` | `0→novel` `1→audio` `2→comic` `3→file` `4→video`；rss 走另一实体 `RssSource`，不在本字段 |
| 5 | `bookSourceComment` | 自由文本说明（能力说明/用法） | `meta.comment` | 直取；原文里的"需要登录""按 X 分组"等提示同时用于生成 checklist |
| 6 | `searchUrl` | 含 `,{json}` 请求选项、`{{page}}`、`{{key}}`、`<1,2,3>`、`@js:`（§2.4 §2.12） | `stages.search.request.{url,params,body,headers,charset,encode,engine,webJs}` + `paginate` | 拆四段：URL → `request.url`；`,{}` → 对应 `request` 字段；`{{key}}`→`var:kw`；`{{page}}`→`expr:page`；`<…>`→`paginate.enumerate`。`@js:` → `script.before` 或报告不可转 |
| 7 | `exploreUrl` | 发现分类串：`@js:`/`<js>` → JS 展开；JSON 数组；否则按 `(&&|\n)+` 切 `title::url`（`BookSource.kt:82`，§2.12；注意 `findUrl` 全仓 0 命中） | `stages.explore.groups` / `groupsFrom` | 纯 `title::url` 串 → 扁平 `groups[]`；JSON 数组 → `groups[]`（`type` 是控件语义，见 §5）；`@js:` → `groupsFrom`（需重写选择器）或报告不可转 |
| 8 | `ruleSearch` | SearchRule，11 个叶子：`bookList` + 9 个字段 + `checkKeyWord`（§2.14） | `stages.search.out` | `bookList→out.list`；`name→map.name` `author→map.author` `kind→map.kind` `wordCount→map.wordCount` `lastChapter→map.lastChapter` `intro→map.intro` `coverUrl→map.cover` `bookUrl→map.url`；`checkKeyWord→out.verify.contains`（近似，语义弱） |
| 9 | `ruleExplore` | ExploreRule，10 个叶子（`bookList` + 9 字段，无 `checkKeyWord`） | `stages.explore.out` | 同 `ruleSearch` 的字段映射（无 `verify`） |
| 10 | `ruleBookInfo` | BookInfoRule，12 个叶子（§2.14） | `stages.detail.out` | `name/author/intro/kind/wordCount/lastChapter/updateTime` 同名 → 同名；`coverUrl→cover`；`canReName→canRename`；`tocUrl→tocUrl`；`downloadUrls`（URL 串）→ `downloads`（`{name,url,size?}[]`，需按分隔符切成数组） |
| 11 | `ruleToc` | TocRule，10 个叶子（§2.14） | `stages.toc.out` + `paginate` | `chapterList→out.list`；`chapterName→map.title`；`chapterUrl→map.url`；`isVolume/isVip/isPay/updateTime/wordCount→map.*` 同名；`nextTocUrl→paginate.next`（kind=next，允许多页 URL 数组）；`formatJs` → **不可转**（§5） |
| 12 | `ruleContent` | ContentRule，11 个叶子（§2.14） | `stages.content.out` + `request` | `content→out.content`；`title→out.title`；`nextContentUrl→out.nextPage`；`payAction→out.payAction`；`webJs→request.webJs`；`sourceRegex→request.sniff`；`replaceRegex→transform[{replace}]`；`subContent→out.tracks`（按载体分流，需人工拆）；`imageStyle` → 渲染细节，丢弃并记报告 |
| 13 | `loginUrl` | 表单路径：控件 action 拼在 `loginUrl` 后执行 JS；WebView 路径：当 URL 加载，抓 `CookieManager.getCookie`（§2.13） | `login.steps[].submit.request.url`（表单）或 `login.type: "cookie"`（WebView） | 纯 URL → `steps[0].submit.request.url`；含 JS/`@js:` → 报告不可转。`type` 由"是否有 `loginUi`"反推（**两条路径二选一**，§2.13） |
| 14 | `loginUi` | `RowUi(name,type,action,chars,default,viewName,style)`，控件只有 5 种（§2.13） | `login.fields[]`（`key`/`label`/`secret`） | 只转 `text`/`password`：`name→key`、`name→label`（或 `chars` 有值则用之）、`password→secret: true`；`action`/`viewName`/`style` 丢弃；`button`/`toggle`/`select` → 报告不可转 |
| 15 | `loginCheckJs` | **名字是错的：实为响应改写器**，`evalJS(checkJs, result = StrResponse) as StrResponse`，请求异常时以 `getErrStrResponse` 为 result，返回码 500 则重抛（`WebBook.kt` 5 处，§2.13） | `script.onResponse`（v2.0 新增，作用对象=原始响应） | `evalJS(result=r) as StrResponse` 形状 → 机械改写为 `{ "script": { "onResponse": {"fn":"lib:fixBody","ctx":true} } }` + `lib.pure` 函数；"500 重抛"语义无对应（§5）。**不要**把它映到 `login.check`（§2.13 明确：前者是改写，后者是断言） |
| 16 | `header` | 字符串形式的请求头；可带 `@js:` | `http.headers` | 纯 JSON/`k:v` → `http.headers`；`@js:` → `script.before` 返回 `{headers}`，或报告不可转 |
| 17 | `concurrentRate` | `"N/M"` = M 毫秒内 N 次；`"N"` = N 毫秒内 1 次；key=源 key，**阻塞等待**（`ConcurrentRateLimiter.kt:21-31,103-121`，§2.10） | `http.rate: {windowMs, max}` | `"N/M"→{windowMs:M, max:N}`；`"N"→{windowMs:N, max:1}`；空 → 不写（**不要**编个默认值） |
| 18 | `jsLib` | 远程脚本 URL（可 `@js:` 拼接），下载后按 MD5 缓存（`SharedJsScope.kt:41-68`，§2.8） | `lib.pure` / `lib.ctx`（内联源码） | 远程 URL → **不可转**（§16 明确不采纳），进报告并保留 `x-legado.jsLib` 供人工粘贴；内联源码 → 按有无 IO 分档 |
| 19 | `enabledCookieJar` | 是否启用 CookieJar（布尔） | `http.cookies` | `true → "session"`，`false → "none"`；需要跨重启就人工改 `"persistent"` |
| 20 | `variableComment` | 源变量说明文字；变量本身不在 `BookSource` 里，而在 `CacheManager` 的 `sourceVariable_<bookSourceUrl>`（§2.14） | `vars.<k>.label` / `vars.<k>.value` | 逐行解析"变量名: 说明" → 生成 `vars.<k>`（`persist: true`，`label` 取说明）；解析不出的行 → `meta.comment` |
| 21 | `bookUrlPattern` | 正则，用于判断"这是列表页还是详情页"（`BookList.kt:62-81,100-108`，§2.14 空值兜底表） | `match.detail` | 正则 → `match.detail: "re:…"`。**语义收窄**：RSL 只做 URL 归属识别；Legado 那种"列表页即详情页"的一步到位在 RSL 要 `detail` 与 `search` 各写一遍（§2.14 判定） |
| 22 | `customOrder` | 组内自定义排序（整数） | **无对应字段** | `x-legado.customOrder`；排序归宿主，不进标准 |
| 23 | `weight` | 检索权重（浮点）；`BookSourcePart.weight` 在 `model/webBook` 下 0 引用（§2.12） | `merge.weight` | 直取到 `merge.weight`；若同时有多个源参与合并，人工确认量级（`merge.weight` 参与同档排序） |
| 24 | `respondTime` | 上次响应耗时（毫秒，运行期统计） | **无对应字段** | `x-legado.respondTime`；它是观测值不是源声明，**不得**写进任何规则或 `merge` 打分 |

> 4 个死字段（`exploreScreen`、`ruleReview`、`BookListRule.updateTime`、`BookInfoRule.updateTime`，§2.14）转换器一律丢弃并进报告——它们能解释"为什么这个源看起来有某能力但从未生效"。

### 2.1 五个必须手工核对的字段（前后对照）

**① `searchUrl` —— 尾缀 `,{}` 与两个编码方向**

```jsonc
// Legado（一段字符串干了四件事）
"searchUrl": "/search.php?q={{key}}&page={{page}},{\"method\":\"POST\",\"body\":\"q={{key}}&p={{page}}\",\"charset\":\"gbk\"}"
```

```jsonc
// RSL：URL / 参数 / 请求编码 / 分页 各归各位
"stages": { "search": {
  "request": {
    "url": "https://example.com/search.php",
    "method": "POST",
    "body": { "type": "form", "data": { "q": "var:kw", "p": "expr:page" } },
    "encode": { "params": "gbk", "body": "gbk" },     // ← Legado 的 charset 搬到这里
    "headers": { "Referer": "var:baseUrl" }
  },
  "out": { "list": "css:.result", "map": { "name": "css:.title", "url": { "$": "css:a", "attr": "href" } } },
  "paginate": { "kind": "page", "pageVar": "page", "max": 50, "stopIf": { "empty": "out.list", "noNew": true } }
}}
```

核对点：`charset` 是不是被**误当成响应解码**（Legado 里它只管请求，§2.4）；`{{page}}` 前 1 还是前 0（§2.12 由 `AnalyzeUrl` 注入）。

**② `ruleToc` —— 逐章字段必须过 `E_COST` 白名单**

```jsonc
// Legado
"ruleToc": { "chapterList": "#list dd", "chapterName": "a@text", "chapterUrl": "a@href",
             "isVolume": ".vol", "isVip": "i.vip", "nextTocUrl": "#next@href" }
```

```jsonc
// RSL
"stages": { "toc": {
  "order": "auto",
  "dedupe": "url",
  "out": {
    "list": "css:#list dd",
    "map": {
      "title":     { "$": "css:a", "transform": [{ "trim": true }] },
      "url":       { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] },
      "isVolume":  { "$": "css:.vol", "exists": true },
      "isVip":     { "$": "css:i.vip", "exists": true }
    }
  },
  "paginate": { "kind": "next", "next": "css:#next", "max": 50 }
}}
```

核对点：`a@href` 的 `@` 是 Legado 的属性取值词（§2.3），在 RSL 里等价 `attr: "href"` 或 `attr:href`；`nextTocUrl` 若是 `##` 正则抽出来的数组，`paginate.next` 允许直接给 URL 数组（`RSL-1.0.md` §4.7）。

**③ `ruleContent` —— 正文规则为空**是**语义变更**，不是"没填"

```jsonc
// Legado：content 为空 → 引擎不请求，直接把 chapter.url 当正文（WebBook.kt:393-397）
"ruleContent": { "content": "", "nextContentUrl": "#next@href" }
```

```jsonc
// RSL：必须显式声明
"stages": { "content": {
  "direct": true,                                  // 不请求，正文就是 var:chapter.url
  "out": { "content": { "$": "var:chapter.url" } }
}}
```

核对点：`direct: true` 只对"URL 本身就是正文/资源"的站点成立。如果原站其实是"详情页里有正文"，那 Legado 的空规则一直是错的，别照抄。

**④ `loginUi` —— 只转输入控件，行为控件进报告**

```jsonc
// Legado
"loginUrl": "/login.php",
"loginUi": "[{\"name\":\"用户名\",\"type\":\"text\"},{\"name\":\"密码\",\"type\":\"password\"}]"
```

```jsonc
// RSL
"login": {
  "type": "form",
  "fields": [
    { "key": "username", "label": "用户名" },
    { "key": "password", "label": "密码", "secret": true }
  ],
  "submit": {
    "request": { "url": "/login.php", "method": "POST",
                 "body": { "type": "form", "data": { "u": "var:login.username", "p": "var:login.password" } } },
    "ok": { "$": "json:$.uid", "exists": true }      // 未登录时不存在
  }
}
```

核对点：`key` 是**自造标识**（Legado 的 `name` 是中文标签，直接当 `key` 会让 `var:login.*` 不可读）；`ok` 千万别选"未登录也返回 0"的字段（那是 `E_LOGIN_REQUIRED` 死循环的经典成因，见 `ERRORS.md` §4.6）。

**⑤ 源级字段：`header` / `concurrentRate` / `enabledCookieJar` / `bookUrlPattern` / `weight`**

```jsonc
// Legado
"header": "{\"User-Agent\":\"Mozilla/5.0 …\",\"Referer\":\"https://example.com\"}",
"concurrentRate": "20/60000",
"enabledCookieJar": true,
"bookUrlPattern": "^https://example\\.com/book/\\d+$",
"weight": 30
```

```jsonc
// RSL
{
  "id": "example.com",
  "baseUrl": "https://example.com",
  "match": { "detail": "re:^https://example\\.com/book/\\d+$" },
  "http": {
    "headers": { "User-Agent": "Mozilla/5.0 …", "Referer": "var:baseUrl" },
    "rate": { "windowMs": 60000, "max": 20 },
    "cookies": "session"
  },
  "merge": { "weight": 30 },
  "x-legado": { "concurrentRate": "20/60000", "customOrder": 3, "respondTime": 812 }
}
```

核对点：`concurrentRate: "20/60000"` 是"60000 毫秒内 20 次"（§2.10），**不是** 20 rps；`weight` 只影响合并排序，不影响取数；`respondTime` 是观测值，不许进规则。
## 3. 规则语法翻译表

Legado 的规则字符串有 8 级前缀 + 6 种串内语法（`KERNEL-COMPARE.md` §2.2 §2.3）。逐条对应：

| # | Legado 写法 | 前缀链里的位置 | RSL 写法 | 备注 |
|---|---|---|---|---|
| 1 | `@js:` / `<js>…</js>` | 第 1 级 | `lib:<fn>`（取值）/ `transform: [{"js": "lib:fn"}]`（变换）/ `script.before\|onResponse\|after`（阶段钩子） | JS 里用 `java.*` 的一律不可转（§5）；纯字符串处理 → `lib.pure` |
| 2 | `@webjs:` | 第 2 级 | `request.webJs: "lib:fn"` + `request.engine: "webview"` | `webJs` 必须返回非空，否则 `E_SCRIPT`（§4.8） |
| 3 | `@CSS:` / 无前缀 | 第 3 / 末级（默认 CSS） | `css:<sel>` | 必须显式加 `css:`；裸选择器 → `E_SELECTOR` |
| 4 | `@@` | 第 4 级（去前缀，取纯文本） | `text` 或 `owntext` | `@@` 的"取文本"语义 → `text`；要去广告子节点先 `strip` 或 `owntext` |
| 5 | `@XPath:` | 第 5 级 | `xpath:<expr>` | 直译 |
| 6 | `@Json:` | 第 6 级 | `json:<path>` | 直译；RFC 9535 子集 |
| 7 | `isJSON` / `$.` / `$[` | 第 7 级（**运行时粘滞**：一旦置位，整个实例后续规则都按 JSON 走，§2.2） | `json:$.…` | 粘滞是 Legado 的坑：转换时**逐条**判断，不要整段搬 |
| 8 | 以 `/` 开头 | 第 8 级 | `xpath:/…` | 直译；`//a` 也要补 `xpath:` |
| 9 | `规则##正则##替换##` | 串内语法：出现 `##` 即切换整条链语义（§0.1 §2.3） | `transform: [{"regex": {"regex":"…","replace":"…","all":…}}]` | 第二段是正则、第三段是替换（可含 `$1`）；第四段编码"是否只替换首个" → `all`（`##` 收尾=只替换首个 → `all:false`）。`$n` 出现在替换段才是捕获组（`AnalyzeRule.kt:722-731`） |
| 10 | `&&` | 串内组合：串联（前一条结果作为下一条输入） | `transform` 数组顺序 + 选择器嵌套 | 例：`css:.box&&text` → `{"$\":"css:.box","transform":[{"$\":"text"}]}`。**跨阶段**的 `&&`（详情+目录同页）不能这样翻，改用 `toc.reuseDetailHtml` |
| 11 | `\|\|` | 串内组合：并联（依次尝试） | `fallback: [sel1, sel2, …]` | 语义对齐；空结果与报错都算失败（§3.7） |
| 12 | `%%` | 串内组合：交叉合并（按下标配对，上界取首分支，`AnalyzeByJSoup.kt:110-117`） | `transform: [{"zip": {"with": "<sel>", "sep": " · "}}]` | 长度不等丢多余项 + `W_ZIP_LENGTH`；`with` 在**父作用域**求值，注意别把父作用域写错 |
| 13 | `[!2]` | 索引方言：排除下标 2 | `exclude: [2]` | 负索引同义 |
| 14 | `[0:10]` / `[-1:0]` | 索引方言：切片（`[-1:0]` 为反向） | `range: "0:10"` / `range: "-1:0:-1"` | 负步长要写成 `start:end:step`，`range` 与 `pick` 互斥 |
| 15 | `[2]`（列表下标） | 索引方言：取第 n 项；`$n` 在非替换段就是下标 | `pick: [2]`（或 `range` 单点） | `$n` 双义是本手册最容易翻错的地方：**先判断它在取值段还是替换段** |
| 16 | `$1` / `$2` | 在 `##…##` 替换段 = 捕获组 | 保留在 `transform.regex.replace` 里 | 不需要改写；但 RE2 不支持反向引用 `\1`（→ `E_REGEX_UNSUPPORTED`），一律改成 `$1` 形式 |
| 17 | `{{page}}` | URL 模板 | `expr:page`（URL 内）或 `paginate.pageVar: "page"` + `params.pn: "expr:page"` | 页码从 1 起算的假设必须核对；Legado 的 `page` 由 `AnalyzeUrl` 注入（§2.12） |
| 18 | `{{key}}` | URL 模板 | `var:kw`（`request.params` / `url`） | 关键词需转码时用 `encode.params`，**不要**用 `charset`（§4.3） |
| 19 | `<1,2,3>` | URL 模板：页码枚举（`AnalyzeUrl.kt:218-228`，超界取 `pages.last()`） | `paginate: {"kind":"enumerate","enumerate":[1,2,3],"into":"params.pn"}`（逐值请求）或加 `joinWith: ","`（一次请求） | RSL 超界不取最后一页，而是按 `enumerate` 实际值发；上限受 `limits.maxPages` |
| 20 | `,{json}` URL 尾缀 | 请求选项：`method/body/charset/headers/retry/webView/js` 等 | `request` 结构化字段 | `charset` **只管请求编码** → `encode.params` / `encode.body`；`retry` → `request.retry` 或 `http.retry`；`webView` → `engine`；`body` → `body: {type,data}` |
| 21 | `@put:{k:v}` / `@get:{k}` | 规则内变量（全局 map） | `let`（阶段内）/ `session:<key>`（跨阶段，需 `script` 写） | `@put` 属跨阶段可变状态，安全做法是写 `let` 并让下游用 `let:`；必须跨阶段才用 `session` + `script.before`（§5.5） |
| 22 | `<js>…</js>`（整段规则） | JS 求值 | `lib.pure` 函数 + `lib:` 选择器 | 含 `java.*`/`result`/`src` 隐式绑定的不可转（§2.12 三张变量表，`RSL-1.0.md` 只有一套 `ctx`） |

### 3.1 组合规则例题（串内语法叠在一起时）

**例 1：`||` + `##`（回退 + 整链切换正则）**

```jsonc
// Legado
"content": "#content@html||.article@html##(ads|AD)\\d*##"
```

```jsonc
// RSL：replace 要**分别**挂到每个分支上（## 是整链语义，fallback 不继承）
"out": { "content": { "fallback": [
  { "$": "css:#content", "transform": [{ "replace": [["re:(ads|AD)\\d*", ""]] }] },
  { "$": "css:.article", "transform": [{ "replace": [["re:(ads|AD)\\d*", ""]] }] }
] } }
```

**例 2：`%%`（交叉合并）**

```jsonc
// Legado：书名列表与价格列表按标交叉
"ruleSearch": { "name": "#list li@text%%#list li .price@text" }
```

```jsonc
// RSL：with 在**父作用域**求值（不是当前项）
"map": { "name": { "$": "css:#list li", "transform": [{ "zip": { "with": "css:#list li .price", "sep": " · " } }] } }
```

**例 3：`[!0][0:10]`（排除 + 切片）**

```jsonc
// Legado
"chapterList": "#list li[!0][0:10]"
```

```jsonc
// RSL：先取子集，再排除（exclude 作用于 range/pick 的结果）
"list": { "$": "css:#list li", "range": "0:10", "exclude": [0] }
```

核对点：Legado 的 `[!0]` 与 `[0:10]` 谁先谁后决定了结果集，转换后**必须**用 `rsl explain` 数一遍条数。

**例 4：`<1,2,3>` + `{{page}}` 混用**

```jsonc
// Legado：一个 URL 里同时有枚举和页码
"searchUrl": "/api/list?pages=<1,2,3,4,5>&p={{page}}"
```

```jsonc
// RSL：一条阶段只能有一个分页驱动（pageVar 与 enumerate 互斥，§4.7.1）
"paginate": { "kind": "enumerate", "enumerate": { "from": 1, "to": 5 },
              "joinWith": ",", "into": "params.pages" }
// {{page}} 与枚举无法共存 → 只保留一个分页驱动，另一个值的取值方式要人工决策
```

**例 5：`@put` / `@get`（跨规则变量）**

```jsonc
// Legado：详情阶段 @put 一个 id，正文阶段 @get 出来拼 URL
"ruleBookInfo": { "name": ".name@text@put:{bid:bookUrl}" }
"ruleContent": { "content": "@get:{bid}##(\\d+)##https://example.com/api/c/$1" }
```

```jsonc
// RSL：阶段内用 let，跨阶段用 session（只有 script 能写）
"stages": {
  "detail": { "let": { "bid": "var:book.url" },
              "out": { "name": "css:.name", "tocUrl": "expr:let:bid" } },   // 借契约已有字段带过阶段边界
  "content": {
    "request": { "url": "expr:var:book.tocUrl" },
    "out": { "content": "css:.read-content" }
  }
}
```

核对点：`@put` 写的是**全局可变 map**，RSL 没有这个层。能被 `out` 契约带过去的就走 `out`（`var:book.*` / `var:chapter.*`）；必须跨阶段又不在契约里的才用 `script.before` + `ctx.session.set`。

## 4. 行为差异表（转换后必然不同）

| # | 维度 | Legado 行为（证据） | RSL 行为 | 迁移动作 |
|---|---|---|---|---|
| 1 | Cookie 隔离 | 键 = eTLD+1 **跨源共享**，>4096 条随机删键；换账号**不清 cookie**（`CookieStore.kt:26-35,78-83`，§2.5） | 隔离键 = 源 `id` + 域名 + 登录名（§4.5.3） | 多个源共用同一站点的，转完后**每个源各登一次**；原先靠"共享 cookie 顺带生效"的源会变未登录（→ `E_LOGIN_REQUIRED`） |
| 2 | 正文规则为空 | **不请求，直接拿 `chapter.url` 当正文**（`WebBook.kt:393-397`，§2.14） | 空规则 = 必填字段缺失 → `E_FIELD_MISSING`；直取要显式 `request.skip: true` + `direct: true`（§6.6.4） | 转换器对空 `ruleContent.content` 自动补 `direct: true`；**人工确认**该 URL 真是正文而不是详情页 |
| 3 | 详情→目录复用 | `bookUrl == tocUrl` 时复用响应省一次请求（`WebBook.kt:303-311`，§2.7） | `toc.reuseDetailHtml: true`，且必须**实际 URL 相同**才复用（§6.5.2） | 探测 `ruleBookInfo.tocUrl` 是否存在：不存在 → 加 `reuseDetailHtml: true`；存在 → 正常请求 |
| 4 | 多源合并 | 4 桶（`equal/tags/contains/other`）+ `name+author` 折叠，**跨桶不比较**，排序键只有 `origins.size`（`SearchModel.kt:99-194`，§2.12） | `merge` 契约，`crossBucket` 默认 `true` **强制全局比较**，排序含档位+源数+`weight`（§12.4） | 同一本书可能从"重复两条"变"合并一条"；书名去重更狠，需检查 `merge.key` 是否把不同版本合并了 |
| 5 | 登录多步骤 | 靠 JS 调 `java.reLoginView(deltaUp)` 增量重绘实现"账号密码 → 验证码 → 短信码"（`SourceLoginJsExtensions.kt:49-132`，§2.13） | `login.steps[]` 数据声明 + `next.when` 条件跳转，UI 归宿主（§4.5.2） | 多步登录**必须人工重建**为步骤图；`reLoginView` 的 JS 一行都不可直译 |
| 6 | `charset` 对响应 | **完全不参与**响应解码（`OkHttpUtils.kt:57`，§2.4）；`charset` 三态只管请求编码 | `charset` = 响应解码；请求编码是 `encode.params` / `encode.body`（§4.3） | 原来的 `charset` 值搬到 `encode.*`；GBK/Big5 站若出现乱码，补 `request.charset` 并核 §6.7 六级顺序 |
| 7 | 逐章成本 | 逐章 4~6 次 `getString`，运行时才炸；无静态检查（§2.14 `BookChapterList.kt:235,238-241`） | `E_COST` 编译期禁止逐章 IO/脚本/不确定性（§6.5.1） | 逐章字段里出现的 `@js:`/`<js>`/`java.*` 全部要搬到 `toc.let` 或 `lib.pure`，否则**加载就失败** |
| 8 | 分页终止 | 无页大小；无分页源 `hasMore` 恒 true → UI 无限触底（`SearchModel.kt:103`，§2.12） | `paginate.max` + `stopIf: {empty, noNew}`（§4.7） | 给所有无分页源显式写 `paginate.max`；`noNew: true` 专治"每页返回同一批" |
| 9 | 空 URL 兜底 | `tocUrl`/`bookUrl` 为空 → 回退 `baseUrl`（§2.14） | `tocUrl` 缺省回退 `var:book.url`（§6.4），不会凭空拼 `baseUrl` | 原源靠"空值→baseUrl"能跑的，转换后会 404/空 `E_NO_MATCH`；把真 URL 补进 `detail.out.tocUrl` |
| 10 | 发现规则回退 | 发现 `bookList` 为空 → **整条**回退 `ruleSearch`（`BookList.kt:86`，§2.12） | 只有选择器级 `fallback`；阶段级回退是**静默换义**，明确不采纳（§16） | 把被复用的搜索规则**显式复制**进 `stages.explore.out`；否则发现页整页 0 条 |
| 11 | 源变量存放 | 变量不在 `BookSource` 里，在 `CacheManager` 的 `sourceVariable_<url>`；**参与发现分类缓存键**却不参与正文缓存文件名（§2.6 §2.14） | 六层状态模型：`vars.persist`（配置）/ `cache`（派生）/ `session`（单次）（§5.1） | `variableComment` 里的变量逐个落到 `vars.<k>`；派生结果改 `cache.<k>` 并带维度化 `key` |
| 12 | 章节去重 | 按 `url`（`BookChapter.equals`），DB 主键 `(url,bookUrl)` + 唯一索引 `(bookUrl,index)`（§2.14） | `toc.dedupe: "url"`（可选 `title`/`none`），保序（§6.5） | 默认 `dedupe: "url"` 与原行为一致；卷标题多条目站点要检查 `isVolume` 是否误判 |

## 5. 一定转不了的清单（转换器会拒绝，别手工硬凑）

| # | 能力 | Legado 证据 | 为什么转不了 / RSL 的替代 |
|---|---|---|---|
| 1 | `loginUi` 的 `button` 控件 | `RowUi.kt:3-22`，§2.13 | 行为控件（点击触发 JS）。RSL 的 `login.fields` 只有输入语义（`key`/`label`/`secret`）。替代：`hostStages` 或宿主按钮 |
| 2 | `loginUi` 的 `toggle` 控件 | 同上 | 开关是**状态**不是输入；RSL 用 `vars.<k>.persist` + `filters` 表达数据，不用控件 |
| 3 | `loginUi` 的 `select` 控件 | 同上 | 同理 → `vars.<k>.options` 或 `explore.filters[].options` |
| 4 | `reLoginView(deltaUp)` 增量重绘 | `SourceLoginJsExtensions.kt:49-132`，§2.13 | "让源用 JS 操作宿主 UI"（§16 明确不采纳）。替代：`login.steps[]` + `next.when`，需人工把每一步拆成字段组 |
| 5 | 远程 `jsLib`（URL 引用外部脚本） | `SharedJsScope.kt:41-68`，§2.8 | 源必须自包含、可审计、可离线（§16）。替代：把脚本内联进 `lib.pure`/`lib.ctx`，人工粘贴并审批 |
| 6 | `upload()` 表单上传 | `AnalyzeUrl.kt:693-708`，§4.3 | 源是取数工具，不是上传工具（§16）。无替代，直接砍掉该功能 |
| 7 | `data:` 伪协议 | `AnalyzeUrl.kt:645-682`，§4.3 | 上下文进 URL 不可校验、不可 diff（§16）。替代：`out` 契约 + `let` |
| 8 | `java.*` 宿主 API（`JsExtensions` 120 fun / 49 个 `@JavascriptInterface`） | `JsExtensions.kt:95`，§0.1 §6 | 绑定宿主语言。替代：`ctx` 10 项接口（`ctx.http` `ctx.cache` `ctx.session` `ctx.cookie` `ctx.now` `ctx.crypto` `ctx.var/set` `ctx.config` `ctx.log`）+ `expr:` 函数族；用到 `java.ajax`/`java.md5Encode`/`java.getCookie` 的脚本要逐行重写 |
| 9 | `exploreScreen` | 无任何读取点（§2.14 死字段） | 死字段，转了也没语义；丢弃并进报告 |
| 10 | `ruleReview` | TypeConverter 恒返回 null / 写 `"null"`（`BookSource.kt:308-312`，§2.14 死字段） | 死字段；段评/评论请用 `hostStages` 重新声明 |
| 11 | `loginCheckJs` 的"500 重抛"语义 | `WebBook.kt:71-95` 等 5 处，§2.13 | `script.onResponse` 只能改写返回内容，不能改变控制流。改写部分可转（→ `onResponse`），重抛部分必须进 `onError`/`check` 体系重设计 |
| 12 | WebView 登录路径（`loginUi` 为空时加载 `loginUrl` 并抓 cookie） | `SourceLoginActivity.kt:26-34`、`WebViewLoginFragment.kt:71-156`，§2.13 | 该路径**不执行 `loginCheckJs`、不写 `loginInfo`**，其副作用无法声明式重建。替代：`login.type: "cookie"`，人工验证 cookie 名与域 |
| 13 | `formatJs` 逐章格式化 | `BookChapterList.kt:139-157`，§4.3 K8 | 与 `E_COST` 直接冲突（逐章脚本）。替代：`lib.pure` + 批量后处理出口，需人工重写 |
| 14 | `customOrder` / `respondTime` 参与逻辑 | §2 表 #22 #24 | 宿主侧排序与观测值，不进标准。若原源靠它们影响取数，逻辑本身就不可移植 |
| 15 | 单源内聚合多站点 | `RSL-1.0.md` §16 | 一个源 = 一个站点；聚合是宿主职责。要拆成多个源 |

## 6. 转换后检查清单（10 条）

1. **`rsl lint` 全绿**：§13.3 的 20 条都要过；特别是 `E_SELECTOR`（每条规则都有命名空间）与 `E_COST`（逐章无 IO/脚本）。
2. **`baseUrl` 与 `id` 人工确认**：`id` 一旦发布不能再改（登录隔离键、缓存键都含它）；`baseUrl` 必须是相对地址基准，不是某个具体页面。
3. **每个阶段先跑 `rsl run <src> <stage> --trace`**：看 `bridge` 是否异常高、`matched` 是否为 0；0 命中字段逐个用 `rsl explain` 对比同义选择器。
4. **编码双向验证**：GBK/Big5 站分别在"请求关键词"和"响应正文"两处验一遍——`encode.params` 与 `request.charset` 是**两个**字段（§4.3）。
5. **登录闭环**：`login.check` 的 `ok` 必须是"未登录时不存在"的字段；跑一次登出→登录→`content`，确认不会拿 `E_LOGIN_REQUIRED` 循环。
6. **目录完整性**：章节数是否等于站点显示数；`isVolume`/`isVip`/`isPay` 标记是否正确（`isVolume: true` 不该被请求正文）；`order`/`dedupe` 是否需要从 `auto` 改成显式值。
7. **分页终止**：每个 `paginate` 都有 `max`；无分页源必须配 `stopIf.noNew`，否则照抄 Legado 会出现"翻不完"（§4.7）。
8. **正文与媒体**：`direct: true` 的源确认 `chapter.url` 真是正文/资源；`comic` 的 `images` 不得混入评论横幅/SVG（§6.6.3）；音视频 `url` 不得是提示文本。
9. **降级路径**：每个增强字段都有 `optional: true` + `default`；增强失败时主契约字段仍输出（§11.3）；`degraded` 字段在 UI 有对应提示。
10. **回归固化**：把上面每一步录成 `rsl test` 用例（L2 用 `fixture`，L3 真实 HTTP），至少覆盖"正常""站点改版应报错""空结果"三种；证据等级低的不许写成高等级可用（§13.2）。
