---
id: rsl-errors
type: reference
status: draft
title: RSL 错误全书（错误码 · 症状 · 修法）
parent: rsl-language
tags: [rsl, errors, diagnostics]
version: "2.0"
---

# RSL 错误全书 v2.0

> 契约在 `RSL-1.0.md` §11。本文是它的展开：**每个码什么时候报、错误对象长什么样、怎么修、修不好能不能降级**。
> 对齐版本 `RSL-1.0.md` v2.0.0-draft.1。码集合以 §11.1 / §11.2 为准；§11.1 里合并书写的行（如 `E_REGEX` / `E_REGEX_UNSUPPORTED`）拆开后共 **31 个错误码**，§11.2 共 **5 个警告码**。
> 字段名一律按 `RSL-1.0.md`，本文不引入任何新字段。

## 0. 错误对象

```jsonc
{
  "code": "E_NO_MATCH",
  "severity": "error",              // error | warning | info
  "stage": "toc",
  "path": "stages.toc.out.list",
  "rule": "css:#chapter-list li",
  "message": "选择器命中 0 个元素",
  "hint": "页面疑似改版；同义选择器 css:ul.chapter > li 命中 1200 个",
  "sample": "<div id=\"chap-list\">…",
  "degraded": true,                 // 该字段降级，主结果仍可用
  "retryable": true
}
```

| 字段 | 必填 | 说明 |
|---|---|---|
| `code` | 是 | 本文任一码 |
| `severity` | 是 | `error` / `warning` / `info`；只有 `E_EMPTY` 是 `info`，5 个 `W_` 码是 `warning` |
| `stage` | 运行期必填 | `search` `explore` `detail` `toc` `content`，或 `hostStages.<k>`；编译期/加载期填 `—` |
| `path` | 是 | JSON 路径，如 `stages.toc.out.map.title`、`stages.content.request.charset` |
| `rule` | 否 | 出错的那条规则原文（字符串型选择器或 URL） |
| `message` | 是 | 事实陈述，不写"失败了"三个字就完 |
| `hint` | 是 | 可执行的下一步：改成什么、去哪看 |
| `sample` | 否 | 响应/节点片段；`vars.<k>.secret` 的值永不出现 |
| `degraded` | 否 | 只与该字段相关；主契约字段仍完整 |
| `retryable` | 否 | 同参数重试是否有意义（`E_PARSE` 恒 `false`） |

## 1. 错误码全景表

### 1.1 错误码（30）

| 码 | 时机 | 严重级 | 一句话 | 详见 |
|---|---|---|---|---|
| `E_PARSE` | 编译期 | error | JSON 语法错（带行列号） | §2.1 |
| `E_SCHEMA` | 编译期 | error | 不符合 Schema 或违反互斥规则 | §2.2 |
| `E_ENUM` | 编译期 | error | 枚举值非法 | §2.3 |
| `E_REF` | 编译期 | error | `var:` / `let:` 名字不存在（含契约字段拼写） | §2.4 |
| `E_TYPE` | 编译期 / 运行期 | error | 管道/钩子类型不匹配（运行期仅 `onResponse`） | §2.5 |
| `E_REGEX` | 编译期 | error | 正则语法非法 | §2.6 |
| `E_REGEX_UNSUPPORTED` | 编译期 | error | 超出 RE2 子集（反向引用/断言/递归） | §2.7 |
| `E_SELECTOR` | 编译期 | error | 选择器语法错或缺命名空间 | §2.8 |
| `E_COST` | 编译期 | error | 逐章规则含 IO / 脚本 / 不确定性 | §2.9 |
| `E_EXPR_SYNTAX` | 编译期 | error | `expr:` 语法非法 | §2.10 |
| `E_EXPR_FN` | 编译期 | error | `expr:` 函数未知或 feature 未声明 | §2.11 |
| `E_EXPR_LIMIT` | 编译期 | error | `expr:` 长度/嵌套超限 | §2.12 |
| `E_TEMPLATE` | 编译期 | error | `${...}` 插值本身非法 | §2.13 |
| `E_UNSUPPORTED` | 加载期 | error | feature / 算法不支持（**不含** hostStage） | §3.1 |
| `E_VERSION_UNSUPPORTED` | 加载期 | error | `rsl` 主版本不兼容 | §3.2 |
| `E_NET` | 运行期 | error | DNS / TLS / 连接失败 | §4.1 |
| `E_TIMEOUT` | 运行期 | error | 超时（阶段、脚本、总预算） | §4.2 |
| `E_CANCELLED` | 运行期 | error | 宿主取消 | §4.3 |
| `E_HTTP` | 运行期 | error | 非 2xx | §4.4 |
| `E_CHARSET` | 运行期 | error | 响应解码失败（乱码源头） | §4.5 |
| `E_LOGIN_REQUIRED` | 运行期 | error | 未登录或登录态失效 | §4.6 |
| `E_CHALLENGE` | 运行期 | error | 人机验证，需宿主介入 | §4.7 |
| `E_DECODE` | 运行期 | error | 图片/媒体解密失败 | §4.8 |
| `E_NO_MATCH` | 运行期 | error | 选择器命中 0 且字段必填 | §4.9 |
| `E_EMPTY` | 运行期 | **info** | 列表为空且 `emptyOk` 为假 | §4.10 |
| `E_FIELD_MISSING` | 运行期 | error | 保留字段为空 | §4.11 |
| `E_VERIFY` | 运行期 | error | `verify` 校验失败（200 但内容不对） | §4.12 |
| `E_PROTECT_LOST` | 运行期 | error | `protect` 占位符在管道中丢失 | §4.13 |
| `E_BUDGET` | 运行期 | error | 超预算（`limits.*.on: reject`） | §4.14 |
| `E_SCRIPT` | 运行期 | error | `lib`/脚本异常、`webJs` 空返回 | §4.15 |
| `E_HOST_STAGE` | 运行期 | error | 宿主未实现该扩展阶段 | §4.16 |

### 1.2 警告码（5）

| 码 | 严重级 | 一句话 | 详见 |
|---|---|---|---|
| `W_UNKNOWN_FIELD` | warning | 非 `x-` 前缀的未知字段被忽略 | §5.1 |
| `W_BUDGET_TRUNCATED` | warning | 预算 `truncate` 生效，结果被截断 | §5.2 |
| `W_DEGRADED` | warning | 增强字段降级，主结果保留 | §5.3 |
| `W_DEPRECATED` | warning | 用了将被移除的写法 | §5.4 |
| `W_ZIP_LENGTH` | warning | `transform.zip` 两列长度不等 | §5.5 |

## 2. 编译期错误

### 2.1 `E_PARSE` · JSON 语法错误
**什么时候报**：文件整体无法解析——尾逗号、单引号、未转义反斜杠、BOM、`//` 注释。此时其余诊断一律不出（先修这个）。
**错误对象**：`{"code":"E_PARSE","severity":"error","stage":"—","path":"stages.search.out.list","rule":null,"message":"expecting ',' or '}' at line 42 column 8","hint":"第 41 行末尾多了逗号"}`
**怎么修**：① 用 `rsl lint` 或编辑器 JSONC 校验到行列号再改；② 正则里的 `\d` 写成 `"\\d"`（错误 `"re:第(\d+)章"` → 正确 `"re:第(\\d+)章"`）；③ 删注释，说明写进 `meta.comment` 或 `x-note`；④ 文件存成无 BOM 的 UTF-8。
**修不好怎么办**：不可降级。`E_PARSE` 是门禁，任何 `optional` / `emptyOk` / `onError` 都不生效。

### 2.2 `E_SCHEMA` · 结构或互斥规则冲突
**什么时候报**：缺必填（`rsl` `id` `name` `type` `baseUrl`、`out.list`、`map.name` `map.url`），或违反 §3.1/§4.5/§4.7/§6.6.4 的互斥：`range`+`pick`、`blocks`+`paragraphs`、`login.steps`+顶层 `fields`、`direct`+非 `skip` 的 `request`、`pageVar`+`enumerate`、步骤图成环、`reuseDetailHtml` 但无 `detail`、`search` 与 `explore` 全缺。
**错误对象**：`{"code":"E_SCHEMA","severity":"error","stage":"—","path":"stages.toc.paginate","rule":null,"message":"paginate.pageVar 与 paginate.enumerate 互斥","hint":"保留 pageVar 删掉 enumerate，或改用 kind:\"enumerate\" 并去掉 pageVar"}`
**怎么修**：① 互斥二选一（错误 `{"range":"0:10","pick":[0,1]}` → 正确 `{"range":"0:10"}`；错误 `{"blocks":{...},"paragraphs":{...}}` → 只留一个）；② 单步登录用 `fields`+`submit`，多步用 `steps[]`，不要同时写；③ `direct: true` 时把 `request` 整体删掉或只留 `skip: true`；④ 步骤图改成无环，且 `next.to` 只能指向**真实存在的 step id**；末步省略 `next` 即结束。
**修不好怎么办**：不可降级——这是加载前的形状问题。可临时把整块可疑配置挪到 `x-legado.*` 保底不再参与求值。

### 2.3 `E_ENUM` · 枚举值非法
**什么时候报**：`type`（`novel comic audio video file rss`）、`paginate.kind`（`page next cursor enumerate`）、`request.method`（`GET POST PUT DELETE HEAD`）、`request.response`（`html json text bytes`）、`request.engine`（`http webview auto`）、`tracks.kind`/`format`、`onError`（`abort skip fallbackTo:<stage>`）、`limits.*.on`（`reject truncate`）、阶段键名拼错（`stages.tocs`）。
**错误对象**：`{"code":"E_ENUM","severity":"error","stage":"—","path":"stages.content.request.response","rule":null,"message":"\"htm\" 不是合法枚举值","hint":"合法值：html | json | text | bytes"}`
**怎么修**：① 按 `hint` 里的枚举替换；② 阶段名只能是 `search` `explore` `detail` `toc` `content` 或 `hostStages.<k>`；③ Legado 的 `bookSourceType` 数字不要手抄进 `type`（`2` → `comic`，见 `MIGRATION.md` §2）；④ 载体要按条目覆盖时用 `out.map.type` + `typeMap`，源级 `type` 仍须合法。
**修不好怎么办**：不可降级。非标准的自造值请放 `x-` 字段，引擎忽略但保留。

### 2.4 `E_REF` · 引用未定义
**什么时候报**：`var:book.tocUrl` 拼错、`let:` 在同阶段未先声明、`let` 循环引用、`merge.key` 指向 `search.out.map` 不存在的字段、`login.steps[].next.to` 指向不存在的 step、`vars` 键与内置上下文键（`baseUrl url response charset kw page group filter login book chapter`）重名。
**错误对象**：`{"code":"E_REF","severity":"error","stage":"—","path":"stages.toc.out.map.url","rule":"var:book.tocurl","message":"var:book.tocurl 未定义","hint":"detail.out 声明的是 tocUrl（大小写敏感）"}`
**怎么修**：① 逐字对齐 `out.map` 的键名（错误 `var:book.tocurl` → 正确 `var:book.tocUrl`）；② `let` 声明顺序提前；③ `merge.key` 改成实际存在的字段（错误 `"key":["bookName"]` → 正确 `"key":["name","author"]`）；④ `vars` 里的键改名（错误 `"page":{...}` → 正确 `"startPage":{...}`）。
**修不好怎么办**：不可降级。`optional: true` 只解决"命中为空"，不解决"名字不存在"——名字错永远是 `E_REF`，必须在源里改。

### 2.5 `E_TYPE` · 类型不匹配
**什么时候报**：`node[]` 上直接跑字符串步骤（`regex` `slice` `t2s` `html`）、`bytes` 上跑 `regex`、`json` 上跑 `css:`、`onResponse` 返回类型与输入不一致、选择器的 `attr` 用在 JSON 作用域。
**错误对象**：`{"code":"E_TYPE","severity":"error","stage":"—","path":"stages.toc.out.map.title","rule":"css:a","message":"步骤 regex 需要 Str，输入是 node[]","hint":"先加 {\"$\":\"text\"} 再 regex"}`
**怎么修**：① 先降维再变换（错误 `{"$":"css:a","transform":[{"regex":{...}}]}` → 正确 `{"$":"css:a","transform":[{"$":"text"},{"regex":{...}}]}`）；② `response: "bytes"` 时先 `transform.charset` 再跑文本步骤；③ 管道里 `json:true` 之后只能用 `json:` 类取值；④ `onResponse` 进出同型（`str` 进 `str` 出，`bytes` 进 `bytes` 出）。
**修不好怎么办**：不可降级；但可把该字段改成 `optional: true` 直接跳过它（主契约字段不可跳）。

### 2.6 `E_REGEX` · 正则语法错误
**什么时候报**：`re:` / `transform.regex` / `transform.match` / `verify.contains` / `match.detail` / `sniff` 里的正则括号不配、量词错、字符类未闭合、`\` 结尾。
**错误对象**：`{"code":"E_REGEX","severity":"error","stage":"—","path":"stages.search.out.list","rule":"re:第(\\d+章","message":"unclosed group at offset 3","hint":"补右括号：re:第(\\d+)章"}`
**怎么修**：① 补括号/方括号；② JSON 里反斜杠双写；③ 用 `rsl explain <src> <path>` 单测这一条；④ 复杂的先拆成两步 `match` 再 `replace`。
**修不好怎么办**：不可降级。改不了正则就退回选择器路径（`css:`/`xpath:`）取同一字段。

### 2.7 `E_REGEX_UNSUPPORTED` · 超出 RE2 子集
**什么时候报**：用了反向引用 `\1`、前后向断言 `(?=)` `(?<=)` `(?!)`、递归 `(?R)`。§3.4 明确不支持，因为 Rust/Swift/JS/Kotlin 行为不一致。
**错误对象**：`{"code":"E_REGEX_UNSUPPORTED","severity":"error","stage":"—","path":"stages.content.out.content","rule":"re:(?<=id=)\\\\d+","message":"lookbehind 不受支持","hint":"改用 transform.regex 的捕获组 $1"}`
**怎么修**：① 反向引用改 `transform.replace` 的 `$1`；② 断言改"先匹配整段再取组"（错误 `(?<=id=)\d+` → 正确 `id=(\d+)` + `{"regex":{...,"replace":"$1"}}`）；③ "第 n 个匹配"用 `transform.match` 的 `n`；④ 必须用方言特性时把这段挪到 `lib.pure` 里的 JS 正则。
**修不好怎么办**：不可降级；`lib.pure` 内联脚本是唯一出口（仍是 `lib.pure` 档，不得有 IO）。

### 2.8 `E_SELECTOR` · 缺命名空间或选择器语法错
**什么时候报**：裸字符串被当规则用（`"div.title"`）、旧方言前缀（`@@`、`@CSS:`、`$.`）、`css:` 里写 XPath、`json:` 里写 CSS、`attr` 无属性名。
**错误对象**：`{"code":"E_SELECTOR","severity":"error","stage":"—","path":"stages.detail.out.name","rule":".book-name","message":"选择器缺命名空间","hint":"改为 css:.book-name"}`
**怎么修**：① 加命名空间（错误 `.book-name` → 正确 `css:.book-name`；错误 `/html/body//a` → 正确 `xpath:/html/body//a`；错误 `$.data.list` → 正确 `json:$.data.list`）；② `@@` 的语义是"取纯文本"，改 `text`；③ `@CSS:`/`@XPath:`/`@Json:` 改成小写命名空间；④ 属性用 `attr:href` 或 `{"$":"css:a","attr":"href"}`。
**修不好怎么办**：不可降级。抓不到结构的字段要么换命名空间，要么承认该字段增强属性（`optional: true`）。

### 2.9 `E_COST` · 逐章规则含禁止能力
**什么时候报**：`toc.out.map.*` 里出现 `script`、`lib.ctx`、`cache`、`http`、`expr:now()`、`expr:rand()`、`cookie:`、`token:`，或 `transform.js` 调 `lib.ctx` 档函数。807 章 × 3 字段 ≈ 4800 次跨桥 ≈ 19 秒，编译期必须拦。
**错误对象**：`{"code":"E_COST","severity":"error","stage":"toc","path":"stages.toc.out.map.updateTime","rule":"lib:nowText","message":"逐章规则不得调用 lib.ctx","hint":"把该字段的计算挪到 list 选择器的父规则里一次算完，或改用 transform.date"}`
**怎么修**：① IO/时间/随机挪到 `stages.toc.let` 或 `script.before`，逐章只读 `let:`；② `lib.ctx` 函数改成 `lib.pure`（签名、格式化都是纯函数）；③ `expr:now()` 换成 `var:` 注入的固定值；④ 逐章只留白名单内的选择器与管道（§6.5.1）。
**修不好怎么办**：不可降级。真要逐章发请求就改 `hostStages` 或宿主批量接口，不要塞回 `toc.out.map`。

### 2.10 `E_EXPR_SYNTAX` · `expr:` 语法非法
**什么时候报**：赋值、自定义函数、正则字面量、属性访问、任意 JS、括号不配、`{{ }}` 里跑副作用。
**错误对象**：`{"code":"E_EXPR_SYNTAX","severity":"error","stage":"—","path":"stages.search.request.params.pn","rule":"expr:page -= 1","message":"赋值不允许","hint":"expr:page - 1"}`
**怎么修**：① 去副作用（错误 `expr:page -= 1` → 正确 `expr:page - 1`）；② 属性访问改选择器（错误 `expr:book.url` → 正确 `var:book.url`）；③ 复杂逻辑进 `lib.pure`，`expr:` 只留算术；④ 运算只用 §4.4 白名单内的运算符。
**修不好怎么办**：不可降级；`lib.pure` + `let:` 是替代路线。

### 2.11 `E_EXPR_FN` · 函数未知或 feature 未声明
**什么时候报**：`expr:` 调了不存在的函数，或调了 `now`/`rand`/`md5`/`sha256`/`hmac`/`hex`/`base64`/`urlEncode` 但 `requires.features` / `requires.crypto` 没声明对应项。
**错误对象**：`{"code":"E_EXPR_FN","severity":"error","stage":"—","path":"stages.search.let.sign","rule":"expr:md5(var:kw + let:ts)","message":"crypto 算法 md5 未声明","hint":"requires.crypto 加 \"md5\"（features 也要含 crypto）"}`
**怎么修**：① 声明（`"requires":{"features":["crypto"],"crypto":["md5"]}`）；② 函数名拼对（`urlencode` → `urlEncode`，`length` → `len`）；③ `now()` 加 `time`、`rand()` 加 `random`；④ 算法名按名声明，别写"随便传"。
**修不好怎么办**：不可降级；缺 feature 会在宿主侧变 `E_UNSUPPORTED`，宁可编译期就报。

### 2.12 `E_EXPR_LIMIT` · 表达式超限
**什么时候报**：单条 `expr:` 过长或嵌套过深（防生成式源与不可读表达式）。
**错误对象**：`{"code":"E_EXPR_LIMIT","severity":"error","stage":"—","path":"stages.search.request.params.sign","rule":"expr:md5(...)...","message":"表达式嵌套深度 42 超上限","hint":"拆成 let 中间值"}`
**怎么修**：① 拆成多个 `let`（每步一句）；② 循环/分支多就搬进 `lib.pure`；③ 只保留真正需要算术的部分；④ 用 `let:` 引用中间结果。
**修不好怎么办**：不可降级；这是可读性与可移植性门禁，不接受"机器生成"当理由。

### 2.13 `E_TEMPLATE` · `${...}` 插值非法

**什么时候报**：`url` / `params` / `headers` / `body.data` 的字符串里写了 `${`，但：未闭合；`${}` 为空；命名空间不认识（`${foo:x}`）；模板嵌套（`${var:${x}}`）。

> 边界：变量不存在仍是 `E_REF`，`expr:` 写错仍是 `E_EXPR_SYNTAX`。`E_TEMPLATE` **只管模板骨架本身**。

**错误对象**：`{"code":"E_TEMPLATE","severity":"error","stage":"search","path":"stages.search.request.url","rule":"https://x/list/${expr:page.html","message":"${ 未闭合","hint":"补上 }；若本意是字面量 $，写 \\$"}`}

**怎么修**：① 补 `}`（错误 `${expr:page` → 正确 `${expr:page}`）；② 删空模板（`${}` → 直接写字面量）；③ 命名空间写全（`${page}` → `${expr:page}`）；④ 不要嵌套，需要嵌套就先用 `let` 算好。

**修不好怎么办**：不可降级——这是 URL/请求头拼错的源头，放过去只会得到 404 或登录墙。

## 3. 加载期错误

### 3.1 `E_UNSUPPORTED` · 能力 / 算法缺失
**什么时候报**：`requires.features`（`js time random crypto font webview detect captcha xpath jsonpath css tls`）、`requires.crypto` 的算法、`requires.selectorLevel` 里声明了宿主给不出的能力。**不含 `hostStages`**——那是 `E_HOST_STAGE`（§4.16）的专属职责，两者不重叠。
**错误对象**：`{"code":"E_UNSUPPORTED","severity":"error","stage":"content","path":"stages.content.request.engine","rule":null,"message":"宿主无 webview 能力","hint":"requires.features 去掉 \"webview\"，或给 engine 改 \"http\""}`
**怎么修**：① 删掉不必要的 `requires` 声明；② 给等价 HTTP 路径（`engine: "http"`）；③ 字体反爬无 `font` 就别声明 `transform.font`；④ 需要宿主能力就换宿主，不要在源里假装它有。
**修不好怎么办**：不可降级——`features` 缺失是硬失败，源侧不能自救；若只是某个扩展阶段不可用，那本来就不该走本码（见 `E_HOST_STAGE`）。

### 3.2 `E_VERSION_UNSUPPORTED` · 主版本不兼容
**什么时候报**：`rsl` 的 MAJOR 与引擎不同（`"1.x"`、`"3.0"`）。同 MAJOR 的未知字段不会触发本码，只记 `W_UNKNOWN_FIELD`。
**错误对象**：`{"code":"E_VERSION_UNSUPPORTED","severity":"error","stage":"—","path":"rsl","rule":null,"message":"需要 rsl 3.x，本引擎支持 2.x","hint":"用 rsl convert 重新转换或降到 2.x"}`
**怎么修**：① 把 `rsl` 改成本引擎支持的 MAJOR；② 升级宿主；③ 多版本并存时按 `requires.engine` 声明 `>=2.0 <3.0`。
**修不好怎么办**：不可降级——不解析未知主版本的文档，避免"看起来能跑"。

## 4. 运行期错误

### 4.1 `E_NET` · 网络层失败
**什么时候报**：DNS 解析失败、TLS 握手/证书失败、连接被拒、代理不可用、`http.dns` 固定解析失效。
**错误对象**：`{"code":"E_NET","severity":"error","stage":"search","path":"stages.search.request.url","rule":"https://example.com/search?q=…","message":"TLS handshake failed","hint":"检查 tls.fingerprint 与代理；retry 会把连接失败计入重试"}`
**怎么修**：① `http.retry.on` 覆盖连接失败（本码本身就计入重试）；② 换 `proxy`（`none` / `system` / `"socks5://…"`）；③ 域名被污染时用 `http.dns`；④ 缩短 `timeout` 先排除假死。
**修不好怎么办**：可降级——`onError: "skip"`（该阶段跳过）或 `onError: "fallbackTo:<stage>"`；搜索侧由宿主决定是否隐藏该源。

### 4.2 `E_TIMEOUT` · 超时
**什么时候报**：`stages.<s>.timeoutMs` / `limits.timeoutMs` / `limits.scriptTimeoutMs` 触发；逐章过重导致的目录超时也走这里。注意它**不是** `E_BUDGET`（§8.4 明确区分）。
**错误对象**：`{"code":"E_TIMEOUT","severity":"error","stage":"toc","path":"stages.toc","rule":null,"message":"阶段超时 20000ms","hint":"检查 --trace 里 bridge 最高的字段，把它挪到 list 父规则"}`
**怎么修**：① `rsl run --trace` 找 `bridge` 最大的字段；② 缩小 `range` / `limit`；③ 去掉逐章脚本（否则先报 `E_COST`）；④ 调 `timeoutMs` 与 `limits.maxNodes`。
**修不好怎么办**：可降级——`onError: "skip"` 或 `truncate` 型预算（配 `W_BUDGET_TRUNCATED`）；已取到的部分结果保留。

### 4.3 `E_CANCELLED` · 宿主取消
**什么时候报**：用户切页、宿主停止搜索、整体取消。不是源的缺陷。
**错误对象**：`{"code":"E_CANCELLED","severity":"error","stage":"search","path":"—","rule":null,"message":"宿主取消","hint":"无需修复；已取得结果标记 partial: true"}`
**怎么修**：① 不要在源里加任何"重试逻辑"；② 需要可续跑就用 `paginate.cursor` + `carry`，不要依赖会话；③ 报告里保留 `partial`。
**修不好怎么办**：天然可降级（`partial: true`）。若某源总是被取消，问题在它太慢——回到 `E_COST` / `E_TIMEOUT`。

### 4.4 `E_HTTP` · 非 2xx
**什么时候报**：4xx/5xx、302 到错误页、被 WAF 拦成 403。`hint` 必须带状态码与片段。
**错误对象**：`{"code":"E_HTTP","severity":"error","stage":"detail","path":"stages.detail.request","rule":"https://example.com/book/12","message":"HTTP 403","hint":"补 Referer/UA 或先登录；retry.on 未含 403，未重试"}`
**怎么修**：① `http.headers` 补 `User-Agent` `Referer`；② 把状态码加进 `http.retry.on`；③ 401/403 且站点需登录 → 走 `login` 并依赖 `check`；④ 接口要求 POST/JSON 时改 `request.method` 与 `body.type`。
**修不好怎么办**：可降级——`onError: "skip"`；列表型阶段配 `emptyOk: true` 把"无结果"与"被封"区分开（`E_EMPTY` 是 info，`E_HTTP` 是 error）。

### 4.5 `E_CHARSET` · 响应解码失败
**什么时候报**：按 §6.7 六级顺序都解不出文本、`request.charset` 写了不存在的编码、`transform.charset` 用错方向。**"正文乱码"多半不是本码，而是编码判定错了但没报错。**
**错误对象**：`{"code":"E_CHARSET","severity":"error","stage":"content","path":"stages.content.request.charset","rule":null,"message":"gb2312 解码在字节 0xD8 处失败","hint":"前 64 字节：d8 a1 b2 …；试试 gbk"}`
**怎么修**：① 显式写 `request.charset`（`gbk` / `big5` / `utf-8`）；② 记住方向：`charset` 只管**响应解码**，请求编码用 `encode.params` / `encode.body`（错误：GBK 站把关键词塞进 `charset` → 正确：`encode.params: "gbk"`）；③ 二进制站点用 `response: "bytes"` + `transform.charset`；④ 需要自动探测才声明 `features: ["detect"]`。
**修不好怎么办**：可降级为 `degraded: true` 的字段级失败，但正文失败等于阶段失败——把 `charset` 修对是唯一出路；`onError: "skip"` 只是不弹错误。

### 4.6 `E_LOGIN_REQUIRED` · 未登录/失效
**什么时候报**：`requiresLogin` 的阶段执行前 `check` 失败；`check` 判定未登录；`token:` 过期。宿主据此弹登录 UI。
**错误对象**：`{"code":"E_LOGIN_REQUIRED","severity":"error","stage":"content","path":"login.check","rule":"json:$.uid","message":"check 未通过（uid 不存在）","hint":"先用宿主登录界面完成登录，再跑 rsl run <src> content；注意隔离键含登录名"}`
**怎么修**：① `login.check` 的 `ok` 选一个只有登录后存在的字段（错误：`{"$\":"json:$.code","eq":0}` 在未登录也返回 0 → 正确：`{"$\":"json:$.uid","exists":true}`）；② 多步登录写 `login.steps[]` + `next.when`；③ 规则里读 Cookie 只能 `cookie:<name>`（只读），写入走 `login.submit.cookie`；④ 换账号不串号靠隔离键（源 id + 域名 + 登录名），别自己拼 Cookie 头。
**修不好怎么办**：不可降级——这是业务前置条件。宿主可缓存 `check` 结果，但不得超过 `limits.loginCheckTtlMs`（默认 300000）。

### 4.7 `E_CHALLENGE` · 人机验证
**什么时候报**：响应出现验证码/滑块/五秒盾特征，且 `requires.features` 含 `captcha` 时由宿主接管；宿主无该能力则先报 `E_UNSUPPORTED`。
**错误对象**：`{"code":"E_CHALLENGE","severity":"error","stage":"search","path":"stages.search.request","rule":null,"message":"检测到滑块验证","hint":"宿主接管；engine 可用 auto 先试 autoUpgradeIf"}`
**怎么修**：① 降请求频率（`http.rate.windowMs/max`）；② 补 `http.headers` 与 `tls.fingerprint`；③ `engine: "auto"` + `autoUpgradeIf` 让 WebView 走一次；④ 登录后再请求。
**修不好怎么办**：源侧无法降级，必须宿主介入。绝不写"绕验证码"的规则——那是 `E_UNSUPPORTED` 的语义错误。

### 4.8 `E_DECODE` · 媒体/图片解密失败
**什么时候报**：`media.decode.fn` 返回类型不符、密钥错、字体/图片被加固、`transform.crypto` 算法与站点不符。
**错误对象**：`{"code":"E_DECODE","severity":"error","stage":"content","path":"media.decode","rule":"lib:decryptImage","message":"返回长度 0","hint":"确认 in/out 都是 bytes；确认 requires.crypto 已声明 aes-cbc"}`
**怎么修**：① 守住 `media.decode` 的 `in`/`out` 契约（字节进、字节出）；② 算法与 key/iv 对齐（`transform.crypto.alg`）；③ 图片防盗链补逐资源 `headers`（`{"url":"var:u","headers":{"Referer":"var:baseUrl"}}`）；④ 用 `rsl run <src> content --trace` 只调一次 `lib:decryptImage` 验证。
**修不好怎么办**：可降级——单图失败标 `degraded: true` 继续渲染其余图，**不得**让整话渲染失败；`tracks` 失败一律空轨道。

### 4.9 `E_NO_MATCH` · 命中 0 且必填
**什么时候报**：必填字段（`map.name`、`map.url`、`out.content`）选择器命中 0；`fallback` 全空且无 `optional`。
**错误对象**：`{"code":"E_NO_MATCH","severity":"error","stage":"toc","path":"stages.toc.out.list","rule":"css:#chapter-list li","message":"选择器命中 0 个元素","hint":"同义选择器 css:ul.chapter > li 命中 1200 个"}`
**怎么修**：① 按 `hint` 换选择器（`rsl explain` 可对比多条）；② 加 `fallback`：`{"fallback":["css:h1.title","css:.book-name"]}`；③ 增强字段加 `optional: true` + `default`；④ 页面是 SPA 空壳时先 `script.onResponse` 或 `engine: "auto"`。
**修不好怎么办**：可降级——`optional: true` 取 `default`；列表型阶段用 `emptyOk: true` 区分"合法空"与"改版"。必填的 `url`/`content` 不能降级。

### 4.10 `E_EMPTY` · 列表为空（info）
**什么时候报**：列表型契约（`search` / `explore` / `toc` / `hostStages.<k>`）返回 0 条且 `emptyOk` 为假（默认）。**它是 info，不是失败。**
**错误对象**：`{"code":"E_EMPTY","severity":"info","stage":"search","path":"stages.search.out.list","rule":"css:.result","message":"列表为空","hint":"接口正常但无结果；确认 var:kw 已注入，或把 emptyOk 设为 true"}`
**怎么修**：① 确认 `var:kw` / `var:page` 真的进了请求（看 `--trace` 的 url）；② 换 `list` 选择器；③ 接口型站点检查 `out.list` 的 `json:` 路径；④ 确认不是 `verify` 该管的错误页（那会报 `E_VERIFY`）。
**修不好怎么办**：可降级——`emptyOk: true` 直接返回 `[]`；宿主按"无结果"处理，不弹错。

### 4.11 `E_FIELD_MISSING` · 保留字段为空
**什么时候报**：契约要求字段有值但取到空（`detail.out.name`、`toc.out.map.title`、`content.out.content`），常见于 `map` 键名与选择器错配、`text` 取到空节点。
**错误对象**：`{"code":"E_FIELD_MISSING","severity":"error","stage":"detail","path":"stages.detail.out.author","rule":"css:.author","message":"命中节点但文本为空","hint":"作者在 css:.info span:nth-child(2)"}`
**怎么修**：① 用 `owntext` 代替 `text` 排除子节点干扰；② 补 `transform: [{"trim": true}]` 与 `dropEmpty`；③ `map` 键名与 §6.2/§6.4/§6.5 保留字段逐字对齐；④ 需要兜底就 `default`。
**修不好怎么办**：可降级——增强字段 `optional: true`；`search.out.map.url`、`toc.out.map.url`、`content.out.content` 是硬字段，不可降级。

### 4.12 `E_VERIFY` · `verify` 校验失败
**什么时候报**：`out.verify` 不满足（典型：HTTP 200 但返回"访问频繁"/登录页/空壳）。它比 `E_EMPTY` 严重：`E_EMPTY` 是"确实没内容"。
**错误对象**：`{"code":"E_VERIFY","severity":"error","stage":"search","path":"stages.search.out.verify","rule":"{\"contains\":\"var:kw\"}","message":"结果不含关键词","hint":"疑似风控页；先看 sample，再补 headers 或降频"}`
**怎么修**：① `verify.contains` 用 `var:kw` 或站点固定文案；② 多字段 AND 收紧（错误 `{"contains":"书"}` 太松 → 正确 `{"contains":"var:kw"}`）；③ 先修 403/风控（`E_HTTP` / `E_CHALLENGE`）；④ 确属正常空结果时改 `emptyOk: true` 并放宽 `verify`。
**修不好怎么办**：可降级——放宽或删除 `verify`（会退回"空列表也算成功"），或 `onError: "skip"` 让该源静默退出。

### 4.13 `E_PROTECT_LOST` · 占位符丢失
**什么时候报**：`protect` 的片段在管道中被误删（例如 `replace` 把整段含占位符的文本清空）。规范禁止静默丢原文，所以直接报错。
**错误对象**：`{"code":"E_PROTECT_LOST","severity":"error","stage":"content","path":"stages.content.out.content","rule":"protect:[\"<usehtml>\",\"</usehtml>\"]","message":"占位符 \\u0000p1\\u0000 未还原（位置 2048）","hint":"该位置被 replace 覆盖；把 replace 的匹配范围收窄"}`
**怎么修**：① `protect` 的字符串与原文逐字一致（不要漏 `</usehtml>`）；② 不要用"整块替换"的 `replace`；③ 把 `protect` 放在选择器层（作用于整个 `transform`），或放到误伤步骤**之前**；④ 节点级保护用 `protect: {"$":"css:pre.code"}`。
**修不好怎么办**：不可降级。丢原文是数据损坏，`optional` 也不能掩盖。

### 4.14 `E_BUDGET` · 超预算
**什么时候报**：`limits.maxBytes` / `maxNodes` / `maxPages` / `libSteps` / `mediaBytes` 触顶且该项 `on: "reject"`。`on: "truncate"` 时改报 `W_BUDGET_TRUNCATED` 并给部分结果。
**错误对象**：`{"code":"E_BUDGET","severity":"error","stage":"toc","path":"limits.maxBytes","rule":null,"message":"响应 3.1MB 超过 maxBytes 2MB","hint":"超大响应通常是登录墙/错误页；先看 sample"}`
**怎么修**：① 判断是不是错误页（`E_VERIFY`/`E_LOGIN_REQUIRED` 往往同时成立）；② 提高对应 `limits.*.value`；③ 改 `on: "truncate"` 接受部分结果；④ 缩小 `range` / `maxNodes` 的作用面。
**修不好怎么办**：可降级——`on: "truncate"` + `degraded: true`；`reject` 项（`maxBytes`、`timeoutMs`）不能降级。

### 4.15 `E_SCRIPT` · 脚本异常
**什么时候报**：`lib` 抛异常或语法错、`webJs` 返回空、`script.onResponse` 里调 `ctx.http`、`libSteps`/`scriptTimeoutMs` 耗尽、`lib:` 函数档位错（逐章调 `lib.ctx` 应先报 `E_COST`）。
**错误对象**：`{"code":"E_SCRIPT","severity":"error","stage":"content","path":"stages.content.script.onResponse","rule":"lib:fixHtml","message":"lib.pure.fixHtml 行 3: TypeError","hint":"onResponse 不得调用 ctx.http；返回值必须与输入同型"}`
**怎么修**：① 按 `message` 里的函数名+行号定位；② `onResponse` 只做字符串/字节变换，取数用 `let` 或 `script.before`；③ `webJs` 必须有非空返回；④ 逐章字段只允许 `lib.pure`。
**修不好怎么办**：可降级——非必需钩子（`after` 的数据增强）失败 → `degraded: true` + `W_DEGRADED`；`onResponse` 失败通常等价阶段失败。

### 4.16 `E_HOST_STAGE` · 宿主未实现该扩展阶段
**什么时候报**：源声明了 `hostStages.<k>`，但宿主 `engine.capabilities.hostStages` 里没有它。**这是本码的唯一用途**：加载成功、只有该入口不可用（`E_UNSUPPORTED` 已明确不含 hostStage）。
**错误对象**：`{"code":"E_HOST_STAGE","severity":"error","stage":"hostStages.shelf","path":"hostStages.shelf","rule":null,"message":"宿主未实现 shelf 阶段","hint":"宿主 engine.capabilities.hostStages 未含 shelf；仅该入口禁用"}`
**怎么修**：① 从 `hostStages` 删掉该键；② 用 `x-` 保留定义供支持的宿主使用；③ 把该能力改成宿主按钮（评论/书架不应伪装成图片或章节）。
**修不好怎么办**：可降级——只禁用该入口，五阶段不受影响（§6.8 明确）。

## 5. 警告码

### 5.1 `W_UNKNOWN_FIELD` · 未知字段被忽略
**什么时候报**：出现 `x-` 前缀之外的未知字段。§2.2 承诺"绝不拒绝加载"，所以只警告。
**错误对象**：`{"code":"W_UNKNOWN_FIELD","severity":"warning","stage":"—","path":"stages.search.out.map.price","rule":null,"message":"未知字段 price 已忽略","hint":"自定义字段请写成 x-price"}`
**怎么修**：① 改成 `x-` 前缀；② 拼写检查（`ou` → `out`）；③ 确认不是 1.x 旧字段名，按 `MIGRATION.md` 映射。
**修不好怎么办**：可忽略。它能解释"我明明写了却没生效"这类问题。

### 5.2 `W_BUDGET_TRUNCATED` · 预算截断
**什么时候报**：某项 `limits.*.on: "truncate"` 生效（`maxNodes`、`maxPages` 最常见）。
**错误对象**：`{"code":"W_BUDGET_TRUNCATED","severity":"warning","stage":"toc","path":"limits.maxPages","rule":null,"message":"到 50 页上限仍可翻页","hint":"目录确实很长就提高 maxPages；否则查 paginate.stopIf"}`
**怎么修**：① 提高 `limits.maxPages.value`；② 配 `paginate.stopIf`（`empty` / `noNew`）；③ 检查 `next` 选择器是否总命中同一页；④ 接受部分结果并在 UI 标"未完整"。
**修不好怎么办**：可接受——它是"结果不全"的唯一诚实信号，别静默吞。

### 5.3 `W_DEGRADED` · 增强字段降级
**什么时候报**：增强字段失败但主结果保留（富目录、推荐、歌词、弹幕、封面），错误对象带 `degraded: true`。
**错误对象**：`{"code":"W_DEGRADED","severity":"warning","stage":"toc","path":"stages.toc.out.map.wordCount","rule":"css:.wc","message":"字数取值失败，已降级","hint":"可选字段；不影响章节列表"}`
**怎么修**：① 给该字段 `optional: true` + `default`；② 修选择器；③ 富目录失败时保证普通目录仍输出（§11.3）。
**修不好怎么办**：可忽略——降级方向是单向的：只能丢增强字段，不能丢主契约字段。

### 5.4 `W_DEPRECATED` · 即将移除的写法
**什么时候报**：用到引擎弃用表里的写法。注意：**v2.0 的 §11.2 只列了这个码，没有给弃用清单**——具体条目由 `COMPILER.md` / 引擎弃用表定义，本文不臆造。
**错误对象**：`{"code":"W_DEPRECATED","severity":"warning","stage":"—","path":"stages.toc","rule":null,"message":"该写法将在下一 MAJOR 移除（具体条目见引擎弃用表）","hint":"按引擎弃用表给出的新写法迁移"}`
**怎么修**：① 按 `hint` 迁移；② 一次只迁一处并跑 `rsl lint`；③ 迁移后跑回归用例（`rsl test`）。
**修不好怎么办**：可忽略一个 MAJOR 周期；跨 MAJOR 会变成 `E_SCHEMA` 或 `E_VERSION_UNSUPPORTED`。

### 5.5 `W_ZIP_LENGTH` · `zip` 两列长度不等
**什么时候报**：`transform.zip` 的 `with` 命中数与当前列表长度不同，多余项被丢弃（§3.6.1）。
**错误对象**：`{"code":"W_ZIP_LENGTH","severity":"warning","stage":"search","path":"stages.search.out.map.name","rule":"zip with css:.price","message":"18 vs 16，已丢弃 2 项","hint":"两列可能来自不同容器；确认 with 在父作用域求值"}`
**怎么修**：① 把 `with` 改到同一父容器下（列表项数与价格数必须一一对应）；② 用 `fallback` 替代 `zip` 取单一字段；③ 让站点对齐后再 `zip`，别靠顺序猜。
**修不好怎么办**：可接受但应显式记录——长度不等说明来源结构不可靠，通常接着会出现错位的书名/价格。

## 6. 错误处理的四条铁律

1. **永不返回裸空串。** 每个失败都必须有 `code` + `severity` + `path` + `hint`；`hint` 必须能回答"改成什么"。`sample` 与 `rule` 按 `vars.secret` 脱敏。
2. **三态必须分开。** 合法空（`emptyOk: true` → 返回 `[]`，**不发错误**）、可降级（`degraded: true` + `W_DEGRADED`）、失败（`error`）。`E_EMPTY`（info）只在 `emptyOk: false` 时出现。把"失败"伪装成"空"是最贵的 bug。
3. **编译期能判的绝不留给运行期。** 命名空间、引用图、管道类型、RE2 子集、逐章白名单（`E_SELECTOR` / `E_REF` / `E_TYPE` / `E_REGEX_UNSUPPORTED` / `E_COST`）都在加载时判死；Schema 只管形状，语义靠编译器（§13.3 的 20 条 lint）。
4. **降级单向，增强失败不得清空核心结果。** 富目录失败保留普通目录，歌词失败保留音频，弹幕失败返回空轨道；反过来（用增强失败去清空 `out.list` / `out.content`）一律是源的 bug。

## 7. 按症状查（反向索引）

| 症状 | 先看 | 再看 |
|---|---|---|
| 搜索 0 条但没有任何报错 | `E_EMPTY`（info 被 UI 吞了）§4.10 | `emptyOk` 默认值；`--trace` 里 url 是否带 `var:kw` |
| 搜索"少了一个源"，无提示 | `E_NET` §4.1 / `E_HTTP` §4.4 / `E_TIMEOUT` §4.2 | 单源失败是否被 `onError: "skip"` 静默；`W_DEGRADED` |
| 目录只出一半 / 章节数不对 | `W_BUDGET_TRUNCATED` §5.2 + `limits.maxPages` | `paginate.stopIf`、`next` 选择器是否总命中同一页、`order`/`dedupe` |
| 目录很慢直到超时 | `E_TIMEOUT` §4.2 | `E_COST` §2.9；`--trace` 的 `bridge` 最高字段 |
| 正文乱码 | `E_CHARSET` §4.5 | 方向错：`charset`（响应）vs `encode.params`（请求）；§6.7 判定顺序 |
| 登录成功后规则仍取不到数据 | `E_LOGIN_REQUIRED` §4.6 | `check.ok` 是否选了登录后才存在的字段；`cookie:` 只读；隔离键含登录名 |
| 出现验证码/滑块 | `E_CHALLENGE` §4.7 | 宿主 `captcha` 能力；`http.rate` 太激进 |
| 编译通过、运行却说字段/变量不存在 | `E_NO_MATCH` §4.9 / `E_FIELD_MISSING` §4.11 | `E_REF` **只**在编译期发生；`out.map` 键名与 §5.4 显式注入 |
| 翻页翻不完 / 无限触底 | `W_BUDGET_TRUNCATED` §5.2 | `paginate.stopIf.noNew`、`max`；Legado 的 `hasMore` 恒真习惯 |
| 图片/视频一话全黑 | `E_DECODE` §4.8 | `media.decode` 的 `in`/`out` 契约；逐资源 `headers` |
| 脚本改动没生效或报异常 | `E_SCRIPT` §4.15 | `script.onResponse` 是否在 `parse` 之前；返回类型必须同型 |
| 页面是空壳，规则全 0 命中 | `E_NO_MATCH` §4.9 | `engine: "auto"` + `autoUpgradeIf`；`E_UNSUPPORTED`（缺 `webview`） |
