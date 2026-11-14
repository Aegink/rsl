---
id: rsl-reference
type: reference
status: draft
title: RSL 字段速查（REFERENCE）
parent: rsl-language
tags: [rsl, spec, reference]
---

# RSL 字段速查 · REFERENCE v2.0

| | |
|---|---|
| 版本 | 2.0（对应 `RSL-1.0.md` 2.0.0-draft.1） |
| 定位 | 字段速查手册。全书只有表格，无教学 |
| 权威 | `RSL-1.0.md`。本文件与之冲突时以规范正文为准 |
| 配套 | `CONCEPTS.md`（先读）、`PATTERNS.md`（配方）、`ERRORS.md`（错误处置） |

## 1. 本文怎么用

1. 只查字段名/类型/取值范围 → 本文件；判断合法与非法 → `RSL-1.0.md`。
2. 每节末行 `> 见 RSL-1.0.md §x.y` 指向权威小节，标号即规范正文的节号。
3. 列「必需」中的 **条件** 表示由互斥规则或阶段类型决定，看该行的小节号。

### 1.1 30 秒速查（最常用 12 个字段）

| # | 字段 | 写在哪 | 一句话 | 例 |
|---|---|---|---|---|
| 1 | `$` | 选择器 | 底层选择器，省略等价 `text` | `"$": "css:.title"` |
| 2 | `attr` | 选择器 | 取属性值，等价 `attr:<name>` | `"attr": "href"` |
| 3 | `transform` | 选择器 / 任意规则 | 有序纯函数管道 | `[{"trim": true}]` |
| 4 | `fallback` | 选择器 | 依次尝试，第一个非空生效 | `["css:h1", "css:.name"]` |
| 5 | `out.list` | 阶段 | 列表选择器（列表型契约必填） | `"list": "css:#list li"` |
| 6 | `out.map` | 阶段 | 每个列表项的子结构提取 | `"map": {"name": "css:a"}` |
| 7 | `request.url` | 阶段 / login | 请求地址，可用 `var:` | `"url": "var:baseUrl/search"` |
| 8 | `request.params` | 阶段 | 查询参数，唯一放 `expr:` 算术的位置 | `"pn": "expr:page - 1"` |
| 9 | `paginate.kind` | 阶段 | 分页方式：`page\|next\|cursor\|enumerate` | `"kind": "page"` |
| 10 | `let` | 阶段 | 阶段内有序中间值（多步请求/签名） | `"ts": "expr:now()"` |
| 11 | `session:` / `var:` | 选择器 | 会话变量 / 上下文变量 | `"$": "session:token"` |
| 12 | `limits.maxPages` | 顶层 | 分页页数上限 | `{"value": 50, "on": "truncate"}` |

> 见 RSL-1.0.md §0（定位）、§2.1（顶层块）、§3.1（选择器）

## 2. 顶层块索引

| 块 | 类型 | 必需 | 一句话 | 小节 |
|---|---|---|---|---|
| `rsl` | string | 是 | 格式 `MAJOR.MINOR`，当前 `"2.0"` | §2.1 §2.2 |
| `id` | string | 是 | 唯一标识，建议域名 | §2.1 |
| `name` | string | 是 | 显示名 | §2.1 |
| `type` | enum | 是 | 源级默认载体；条目级可覆盖 | §2.1 §2.3 |
| `baseUrl` | string | 是 | 相对地址基准 | §2.1 |
| `match` | object | 否 | URL 归属识别，如 `{"detail": "re:^https://…"}` | §2.1 |
| `lang` | string | 否 | 语言标记 | §2.1 |
| `version` | string | 否 | 源自身版本（与 `rsl` 无关） | §2.1 |
| `requires` | object | 否 | 能力声明（引擎版本 / feature / 算法） | §9.1 |
| `http` | object | 否 | 全局请求配置 | §4.1 |
| `vars` | object | 否 | 源配置：常量与可配置项 | §5.2 |
| `cookie` | object | 否 | Cookie 访问策略（规范未给字段表，见 §4.5 只读访问与登录态隔离） | §4.5 |
| `cache` | object | 否 | 命名缓存条目（请求 + 取值 + TTL + 维度键） | §4.6 |
| `media` | object | 否 | 媒体请求头 / 解密契约 / 缓存 / 上限 | §4.6 |
| `login` | object | 否 | 登录声明（单步或多步） | §4.5 |
| `merge` | object | 否 | 多源合并契约（宿主执行，源声明） | §12.4 |
| `lib` | object | 否 | 两档脚本：`pure` / `ctx` | §7.1 |
| `stages` | object | 是 | 五阶段声明容器 | §6 |
| `hostStages` | object | 否 | 宿主扩展阶段（书架/历史/评论等） | §6.8 |
| `limits` | object | 否 | 预算：字节/节点/时长/页数/并发/步数 | §8.1 |
| `meta` | object | 否 | 元信息：作者/许可/变更记录 | §10 |

载体枚举（`type`）：`novel` `comic` `audio` `video` `file` `rss`。映射链：源级 `type` → 条目 `type` → 宿主渲染类型（源不参与最后一步）。

### 2.1 `requires` 字段

| 字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `engine` | string | 否 | 引擎版本范围，如 `">=2.0 <3.0"` |
| `features` | string[] | 否 | `js` `time` `random` `crypto` `font` `webview` `detect` `xpath` `jsonpath` `css` `captcha` `tls` |
| `crypto` | string[] | 否 | 具体算法按名声明，如 `aes-cbc` `hmac-sha256` `md5` |
| `selectorLevel` | object | 否 | `{css: "4", xpath: "1.0", jsonpath: "9535"}` |

### 2.2 `merge` 字段

| 字段 | 类型 | 默认 | 一句话 |
|---|---|---|---|
| `key` | string[] | `[name, author]` | 折叠键 |
| `normalize` | string[] | `[trim]` | 折叠前归一化：`trim` `t2s` `lower` `stripPunct` |
| `crossBucket` | bool | `true` | 跨档（exact/contains/other）也比较 |
| `order` | string[] | `[exact, contains, weight, sources]` | 档位优先级 |
| `weight` | number | `1.0` | 本源权重，参与同分排序 |
| `minScore` | number | `0` | 低于此分丢弃 |

> 见 RSL-1.0.md §2.1（顶层块）、§2.3（载体）、§9.1（requires）、§12.4（merge）

## 3. 选择器字段

选择器 = **字符串**（快捷：等价 `{"$": "..."}`）或 **对象**（完整）。

| 字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `$` | string | 是 | 底层选择器；省略时等价 `text` |
| `attr` | string | 否 | 取属性值，等价 `attr:<name>` |
| `range` | string | 否 | **数组**切片 `start:end[:step]`（字符串切片用 `transform.slice`） |
| `pick` | int[] | 否 | 按下标取值，支持负索引 |
| `exclude` | int[] | 否 | 排除下标，支持负索引 |
| `exists` | bool | 否 | 只判命中，返回布尔 |
| `where` | object | 否 | 条件过滤（§3.5） |
| `map` | object | 否 | 对每项执行子结构提取 |
| `blocks` | object | 否 | 拆成「文本/图片」混合数组 |
| `paragraphs` | object | 否 | 拆成纯文本段落数组 |
| `strip` | Selector[] | 否 | 先删除匹配子节点（正文去广告） |
| `protect` | string[] \| Selector | 否 | 管道执行期间保护匹配片段 |
| `transform` | array | 否 | 管道 |
| `fallback` | Selector[] | 否 | 依次尝试 |
| `flags` | string | 否 | 正则 flag `m` `s` `i`（仅 `re:` 有效） |
| `typeMap` | object | 否 | 取值 → 载体映射（仅 `type` 字段有效） |
| `optional` | bool | 否 | 缺失不报错，默认 `false` |
| `default` | any | 否 | 缺失时的默认值 |
| `limit` | int | 否 | 命中项上限 |

### 3.1 `range` 语法

| 写法 | 含义 | 规范例 |
|---|---|---|
| `"0:10"` | 前 10 项 | `{"$": "css:li", "range": "0:10"}` |
| `"-5:"` | 末 5 项 | `{"$": "css:li", "range": "-5:"}` |
| `"-1:0:-1"` | 全量倒序（负步长） | 对应 Legado `[-1:0]` |
| `"0:20:2"` | 隔一取一 | — |

语义与 Python 切片一致（`end` 不含）；负步长时 `start` 应大于 `end`。

### 3.2 互斥规则（编译期强制，违反 → `E_SCHEMA`）

| 组合 | 结论 |
|---|---|
| `range` × `pick` | 互斥 |
| `exclude` × `range` / `pick` | 可共存（先取子集再排除） |
| `blocks` × `paragraphs` | 互斥 |
| `exists: true` × 其余取值字段 | 其余字段忽略 |
| `typeMap` × 非 `type` 字段 | 只允许出现在契约保留字段 `type` 上 |
| `protect` 在选择器层 | 作用于整个 `transform` |
| `protect` 在 `transform` 内 | 从该步开始生效 |

### 3.3 `where` 运算符

| 运算符 | 作用 |
|---|---|
| `has` | 含匹配子元素 |
| `not` | 取反（可包裹 `has`） |
| `eq` `ne` `gt` `lt` | 比较当前取值 |
| `text` | 取文本后匹配 |

多键同时存在 = **AND**；需要 OR 用 `fallback`。

> 见 RSL-1.0.md §3.1（通用形式）、§3.5（where）、§3.6.2（protect）、§3.7（fallback）、§3.8（空结果）

## 4. 命名空间表

**不存在"无前缀默认解释"。** 缺命名空间 → 编译期 `E_SELECTOR`。

| 命名空间 | 语法 | 输入 | 输出 | 可用阶段 |
|---|---|---|---|---|
| `css:` | `css:div[itemprop=name]` | HTML 节点 | node / node[] | 全部（逐章允许） |
| `xpath:` | `xpath://div[@id="list"]//a` | HTML 节点 | node / node[] | 全部（逐章允许） |
| `json:` | `json:$.data.list[*]` | JSON 值 | json / json[] | 全部（逐章允许） |
| `re:` | `re:第(\d+)章\s*(.+)` | 字符串 | str / str[] | 全部（逐章允许） |
| `text` | `text` | 节点 | str（含子节点文本） | 全部（逐章允许） |
| `owntext` | `owntext` | 节点 | str（仅自身文本节点） | 全部（逐章允许） |
| `html` | `html` | 节点 | str（innerHTML） | 全部（逐章允许） |
| `tag` | `tag` | 节点 | str（标签名） | 全部（逐章允许） |
| `attr:<name>` | `attr:href` | 节点 | str | 全部（逐章允许） |
| `const:<json>` | `const:"固定值"` | — | 常量（JSON 解析，失败当字符串） | 全部（逐章允许） |
| `var:<path>` | `var:book.tocUrl` | 上下文 | any | 按变量可用阶段（§13） |
| `let:<name>` | `let:sign` | 阶段内中间值 | any | 声明该 `let` 的阶段 |
| `session:<key>` | `session:token` | 会话 | any | 全部（逐章允许读） |
| `token:<name>` | `token:auth` | 登录产出 | str | 全部；**逐章禁用**（`E_COST`） |
| `cookie:<name>` | `cookie:sessionid` | Cookie（只读） | str | 全部；**逐章禁用**（`E_COST`） |
| `cache:<name>` | `cache:meta` | 命名缓存（分层） | any | 全部（写经 `cache` 块；读任意） |
| `lib:<fn>` | `lib:cleanTitle` | — | any | 全部；逐章仅 `lib.pure` |
| `expr:<表达式>` | `expr:page - 1` | — | any | 全部；逐章禁 `now()` / `rand()` |

`expr:` 摘要：支持 `+ - * / %`、比较、`&& \|\| !`、三元；纯函数 `min max abs round ceil floor len upper lower trim replace split join contains num str`；需声明 `now()`→`time`、`md5/sha1/sha256/hmac/hex/base64/urlEncode`→`crypto`、`rand()`→`random`。禁止赋值、自定义函数、正则字面量、属性访问、任意 JS。

> 见 RSL-1.0.md §3.2（命名空间）、§3.3（作用域）、§4.4（expr:）、§6.5.1（逐章白名单）

## 5. 管道步骤完整表

每步 `T → T`；可用类型 `str | str[] | node | node[] | json | bytes`。

| 步骤 | 参数 | 输入 → 输出 | 示例 |
|---|---|---|---|
| `trim` | `true` 或 `["\u00a0"]` | `Str\|Node` → 同 | `{"trim": true}` |
| `replace` | `[[from,to],…]`，`from` 可带 `re:` | `Str\|Node` → 同 | `{"replace": [["广告",""]]}` |
| `regex` | `{"regex","replace","all"}` | `Str` → `Str` | `{"regex": {"regex":"第(\\d+)章","replace":"$1"}}` |
| `match` | `{"regex","n"}` | `Str` → `Str` | `{"match": {"regex":"\\d+","n":2}}` |
| `prepend` | string | `Str` → `Str` | `{"prepend": "https://example.com"}` |
| `append` | string | `Str` → `Str` | `{"append": ".html"}` |
| `absolute` | `true` 或 `{"base"}` | `Str` → `Str` | `{"absolute": {"base": "var:baseUrl"}}` |
| `urlencode` | `{"charset"}` | `Str` → `Str` | `{"urlencode": {"charset": "gbk"}}` |
| `urldecode` | `{"charset"}` | `Str` → `Str` | `{"urldecode": true}` |
| `base64` | `"encode"` / `"decode"` | `Str\|Bytes` → 同 | `{"base64": "decode"}` |
| `hex` | `"encode"` / `"decode"` | `Str\|Bytes` → 同 | `{"hex": "decode"}` |
| `crypto` | `{"alg","mode","key","iv"}` | `Str\|Bytes` → 同 | `{"crypto": {"alg":"aes-cbc","mode":"decrypt","key":"var:k","iv":"var:iv"}}` |
| `font` | `{"map","ref"}` | `Str` → `Str` | `{"font": {"map":"lib:glyph","ref":"var:normalFont"}}` |
| `html` | `true` | `Str` → `Str`（反转义实体） | `{"html": true}` |
| `json` | `true` | `Str` → `Json` | `{"json": true}` |
| `date` | `{"from","to","tz"}` | `Str` → `Str` | `{"date": {"from":"x","to":"yyyy-MM-dd","tz":8}}` |
| `number` | `true` 或 `{"unit"}` | `Str` → `Num` | `{"number": {"unit": true}}` |
| `numChapter` | `true` | `Str` → `Str`（"第一百二十三章"→`123`） | `{"numChapter": true}` |
| `cjkNumber` | `"to"` / `"from"` | `Str` → `Str` | `{"cjkNumber": "to"}` |
| `t2s` | `true` | `Str` → `Str` | `{"t2s": true}` |
| `s2t` | `true` | `Str` → `Str` | `{"s2t": true}` |
| `slice` | string（**字符串**切片） | `Str` → `Str` | `{"slice": "0:100"}` |
| `charset` | `"gbk"` | `Bytes` → `Str` | `{"charset": "gbk"}` |
| `join` | string | `Str[]` → `Str` | `{"join": "\n"}` |
| `split` | string | `Str` → `Str[]` | `{"split": "\n"}` |
| `zip` | `{"with","sep"}` | `Str[]` → `Str[]` | `{"zip": {"with":"css:.price","sep":" · "}}` |
| `protect` | `[…占位串]` 或 Selector | 任意 → 同 | `{"protect": ["<usehtml>"]}` |
| `unique` | — | 列表 → 列表 | `{"unique": true}` |
| `sort` | — | 列表 → 列表 | `{"sort": true}` |
| `dropEmpty` | — | 列表 → 列表 | `{"dropEmpty": true}` |
| `js` | `"lib:fn"` 或 `{"fn","ctx"}` | 任意 → 任意 | `{"js": {"fn":"lib:sign","ctx":true}}` |

### 5.1 `zip` 与 `protect`（v2.0 新增）

| 步骤 | 语义要点 |
|---|---|
| `zip` | 对应 Legado `%%`；`with` 在**父作用域**求值（不是当前项）；按下标配成一条；长度不等以短者为准，多余丢弃并记 `W_ZIP_LENGTH` |
| `protect` | 管道入口把命中片段替换为 `\u0000p0\u0000` 占位符，出口原样还原；占位符不参与匹配、不计长度、不进 `slice`/`regex` 输入；丢失 → `E_PROTECT_LOST` |

类型不匹配（如已知 `node[]` 上做 `regex`）→ 编译期 `E_TYPE`，提示先加 `{"$": "text"}`。

> 见 RSL-1.0.md §3.6（管道）、§3.6.1（zip）、§3.6.2（protect）、§3.3（类型错误）

## 6. 请求字段

### 6.1 `http` 全局块

| 字段 | 类型 | 一句话 |
|---|---|---|
| `headers` | object | 全局请求头；值可用 `var:` |
| `timeout` | int(ms) | 单请求超时 |
| `retry` | object | `{times, backoff, on}` |
| `rate` | object | `{windowMs, max}`，或简写 `"rps": 2` |
| `redirect` | object | `{max}` 最大跳转 |
| `cookies` | enum | `none` \| `session` \| `persistent` |
| `proxy` | enum/string | `none` \| `system` \| `"socks5://…"` |
| `dns` | object | 域名固定解析，如 `{"example.com": "1.2.3.4"}` |
| `tls` | object | `{"fingerprint": "chrome"}`，需 `features: ["tls"]` |

### 6.2 阶段 `request` 块

| 字段 | 类型 | 一句话 |
|---|---|---|
| `url` | string | 请求地址 |
| `method` | enum | `GET` \| `POST` \| `PUT` \| `DELETE` \| `HEAD` |
| `params` | object | 查询参数；可用 `var:` / `expr:` |
| `body` | object | `{type, data}` |
| `body.type` | enum | `form` \| `json` \| `raw` \| `multipart` |
| `body.data` | object | 表单/JSON 字段值 |
| `headers` | object | 本请求头 |
| `charset` | string | **响应**解码字符集 |
| `encode` | object | 请求编码：`{params, body}` |
| `response` | enum | `html` \| `json` \| `text` \| `bytes` |
| `engine` | enum | `http` \| `webview` \| `auto` |
| `webJs` | string | WebView 内执行的 JS，**必须有非空返回**，否则 `E_SCRIPT` |
| `sniff` | string | 资源嗅探正则（返回 `str[]`） |
| `autoUpgradeIf` | Selector | 命中则从 HTTP 升级到 WebView（`engine: auto`） |
| `skip` | bool | `true` = 不发起请求 |
| `retry` | object | 覆盖全局 `retry` |
| `timeout` | int(ms) | 覆盖全局 `timeout` |

### 6.3 `retry`

| 字段 | 类型 | 取值 | 一句话 |
|---|---|---|---|
| `times` | int | ≥0 | 重试次数 |
| `backoff` | enum | `none` \| `linear` \| `exponential` | 退避策略（`none` 以外为 RSL 相对 Legado 的新增） |
| `on` | int[] | 如 `[429,500,502,503,504]` | 显式列出触发重试的状态码；连接失败 `E_NET` 也计入重试 |

### 6.4 解码 × 编码（两个字段，方向相反）

| 字段 | 方向 | 典型值 |
|---|---|---|
| `request.charset` | **响应**字节 → 字符串 | `gbk` `big5` `utf-8` |
| `request.encode.params` | 请求参数 → 字节 | `gbk` |
| `request.encode.body` | 请求体 → 字节 | `gbk` |

响应解码判定顺序（规范强制）：`request.charset` → HTTP `Content-Type: charset=` → `<meta charset>` / `<?xml encoding=?>` → BOM → 自动探测（需 `features:["detect"]`）→ `utf-8`。`response: "bytes"` 时不解码，`transform.charset` 才有意义；探测失败 → `E_CHARSET`。

### 6.5 不发起请求的三情形（只有 ① 须 `skip: true`）

| # | 情形 | 声明 |
|---|---|---|
| ① | 数据由 `lib.ctx` 脚本自行取回 | `request.skip: true` + `script.onResponse` |
| ② | 正文就是章节 URL | `request.skip: true` + `direct: true`（`direct` 时 `skip` 可省略） |
| ③ | 数据来自上一阶段复用 | `toc.reuseDetailHtml: true` |

> 见 RSL-1.0.md §4.1（http）、§4.2（request）、§4.2.1（skip）、§4.3（编码）、§4.8（WebView/嗅探）、§6.7（解码顺序）

## 7. 分页矩阵

`paginate` 一次性声明；`kind` 与字段必须匹配（lint #6，`E_SCHEMA`）。

| 字段 | `page` | `next` | `cursor` | `enumerate` | 说明 |
|---|---|---|---|---|---|
| `kind` | ✅ | ✅ | ✅ | ✅ | 必填，四选一 |
| `pageVar` | ✅ 专用 | ✗ | ✗ | ✗ 互斥 | 注入 `var:page` 的变量名 |
| `next` | ✗ | ✅ 专用 | ✗ | ✗ | 选择器 → 下一页 URL（`toc` 可返回 URL 数组） |
| `cursor.take` | ✗ | ✗ | ✅ 专用 | ✗ | 从响应取游标 |
| `cursor.into` | ✗ | ✗ | ✅ 专用 | ✗ | 游标注入位置，如 `params.offset` |
| `enumerate` | ✗ | ✗ | ✗ | ✅ 专用 | 字面数组或 `{from,to[,step]}` |
| `into` | ✗ | ✗ | ✗ | ✅ 专用 | 每个值注入位置，如 `params.pn` |
| `joinWith` | ✗ | ✗ | ✗ | ✅ 可选 | 给了 → 只发一次请求，值拼成字符串 |
| `carry` | ✅ | ✅ | ✅ | ✅ | 原样透传给后续页的字段（如 `search_id` `passback`） |
| `max` | ✅ | ✅ | ✅ | ✅ | 页数上限，另受 `limits.maxPages` 约束 |
| `stopIf.empty` | ✅ | ✅ | ✅ | ✅ | 取值是路径（如 `out.list`），该路径结果为空即停 |
| `stopIf.noNew` | ✅ | ✅ | ✅ | ✅ | 与已收集结果完全无新增项即停 |

| 互斥 / 规则 | 结论 |
|---|---|
| `pageVar` × `enumerate` | 互斥 → `E_SCHEMA`（lint #7） |
| `enumerate` 无 `joinWith` | 每个值各发一次请求，结果按值顺序拼接 |
| `enumerate` 有 `joinWith` | 只发一次请求，如 `pages=1,2,3,4,5` |
| 四 kind × `limits.maxPages` | 全部受约束 |
| `toc` × `next` | 额外允许返回 URL 数组（一次给出全部后续页） |

> 见 RSL-1.0.md §4.7（分页）、§4.7.1（enumerate）、§13.3（lint #6 #7）

## 8. 登录

### 8.1 `login.type`

| 值 | 含义 |
|---|---|
| `form` | 表单/接口提交（最常见） |
| `cookie` | 用户提供 Cookie |
| `token` | 用户提供 Token |
| `qr` | 扫码 |

### 8.2 单步形态

| 字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `type` | enum | 是 | 见 §8.1 |
| `fields[]` | array | 条件 | 宿主渲染的输入项；`steps` 存在时禁止 |
| `fields[].key` | string | 是 | 字段名，规则内读 `var:login.<key>` |
| `fields[].label` | string | 是 | 显示标签 |
| `fields[].secret` | bool | 否 | 密文输入（密码/验证码） |
| `submit` | object | 条件 | 提交：`{request, ok, token, cookie}` |
| `submit.request` | object | 是 | 标准 `request` 块 |
| `submit.ok` | Selector | 是 | 成功判定 |
| `submit.token` | Selector | 否 | 取出令牌存入 token 商店 |
| `submit.cookie` | Selector | 否 | 显式声明要保存的 cookie |
| `check` | object | 否 | 登录态校验：`{request, ok}`，进 `requiresLogin` 阶段前跑一次 |
| `headers` | object | 否 | 登录后附加头，如 `{"Authorization": "Bearer ${token:auth}"}` |

### 8.3 多步形态

| 字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `steps[]` | array | 是 | 步骤序列；与顶层 `fields`/`submit` 互斥 |
| `steps[].id` | string | 是 | 步骤标识，`next.to` 指向它 |
| `steps[].fields[]` | array | 是 | 本步输入项（同单步 `fields` 结构） |
| `steps[].submit` | object | 是 | 本步提交（同单步 `submit`） |
| `steps[].next` | object | 否 | 无 `next` 视为结束 |
| `steps[].next.to` | string | 条件 | 跳转目标步骤 id |
| `steps[].next.when` | Selector | 否 | 条件跳转（如 `json:$.need_sms`）；无 `when` 则无条件跳转 |

| 规则 | 结论 |
|---|---|
| `steps` × 顶层 `fields`/`submit` | 互斥，二者都给 → `E_SCHEMA`（lint #9） |
| `next` 取值 | 只能 `{"to"}` 或 `{"when","to"}` |
| `next.to` 指向不存在 step | `E_REF`（lint #10） |
| 步骤图 | 必须 DAG，有环 → `E_SCHEMA` |
| 每步产出 | 可产 `token` / `cookie`，后续步骤与规则可见 |
| 宿主职责 | 只渲染当前步 `fields` + 提交值；推进由引擎驱动 |

### 8.4 登录态

| 项 | 规定 |
|---|---|
| Cookie 只读访问 | 规则 `cookie:<name>` / `expr:cookie('sessionid')`；脚本 `ctx.cookie.get/set/remove` |
| 隔离键 | 源 `id` + 站点域名 + 登录名 |
| 失效检测 | `check` 在 `requiresLogin` 阶段前跑；失败 → `E_LOGIN_REQUIRED` |
| 缓存上限 | 不超过 `limits.loginCheckTtlMs`（默认 300000） |
| 人机验证 | → `E_CHALLENGE`，宿主决定是否接管 |

> 见 RSL-1.0.md §4.5.1（单步）、§4.5.2（多步）、§4.5.3（登录态）、§13.3（lint #9 #10）

## 9. 阶段通用字段

### 9.1 所有阶段

| 字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `let` | object | 否 | 有序中间值（多步请求 / 签名），后项可引用前项 |
| `request` | object | 否 | 缺省用 `baseUrl` + 阶段默认路径 |
| `parse` | Selector | 否 | 预处理响应根；其输出成为本阶段所有规则的根作用域 |
| `out` | object | 是 | 契约（含保留字段表） |
| `paginate` | object | 否 | 分页声明 |
| `script` | object | 否 | `{before, onResponse, after}` 三钩子 |
| `onError` | enum | 否 | `abort` \| `skip` \| `fallbackTo:<stage>` |
| `timeoutMs` | int | 否 | 覆盖 `limits.timeoutMs` |
| `requiresLogin` | bool | 否 | 未登录直接 `E_LOGIN_REQUIRED` |

### 9.2 `let` 每项可取的形态

| 形态 | 写法 | 说明 |
|---|---|---|
| 表达式 | `"ts": "expr:now()"` | `expr:` 表达式 |
| 单次请求 | `{"request": {...}, "take": Selector}` | 取回后按 `take` 取值 |
| 并发批次 | `{"parallel": [...], "limit": 2}` | 受 `limits.maxParallel` 与 `http.rate` 双重约束 |
| 纯函数 | `"lib:fn"` | 调 `lib.pure` |

循环引用 → 编译期 `E_REF`。

### 9.3 阶段归属

| 阶段 | 必需性 | 专属字段 |
|---|---|---|
| `detail` `toc` `content` | 必需 | — |
| `search` | 与 `explore` 至少其一 | — |
| `explore` | 与 `search` 至少其一 | `filters[]` `groups[]` `groupsFrom` |
| `hostStages.<name>` | 否 | `requiresLogin`（宿主不支持该阶段 → `E_HOST_STAGE`，只禁用该入口） |

### 9.4 `explore` 专属

| 字段 | 类型 | 一句话 |
|---|---|---|
| `filters[]` | array | `{key, label, persist, default, options[{value,label}]}`；规则内读 `var:filter.<key>` |
| `groups[]` | array | `{title, when?, params?, url?, groups?}`；字段**深度合并**到阶段 `request`（group 优先） |
| `groups[].when` | object | 按 `filters` 当前值过滤（数据选择声明，非控件编程） |
| `groupsFrom` | Selector | 从站点动态抓分类；结果追加在 `groups` 之后 |
| 二级 `groups` | — | 该项本身不发请求，只作分组标题 |

### 9.5 `toc` 专属

| 字段 | 类型 | 取值 | 一句话 |
|---|---|---|---|
| `order` | enum | `auto` \| `asc` \| `desc` | `auto` 比较首尾标题章节号判定正倒序 |
| `dedupe` | enum | `url` \| `title` \| `none` | 目录去重维度 |
| `reuseDetailHtml` | bool | 默认 `false` | 本阶段 URL 与 `detail` 实际 URL 相同时不发请求，复用响应（lint #17） |

`reuseDetailHtml` 语义：URL 相同 → 不请求、直接用 detail 响应体作根作用域；URL 不同 → 正常请求；`detail` 未执行 → 本项无效；复用不计入 `http.rate`。

### 9.6 `content` 专属

| 字段 | 类型 | 一句话 |
|---|---|---|
| `direct` | bool | `true` = 不请求，`out.content` 直接取 `var:chapter.url`；`request.skip` 可省略 |
| `direct` × `request` | — | 同时给非 `skip` 的 `request` → `E_SCHEMA`（lint #8） |
| `direct` × 非 `novel` 载体 | — | 该 URL 当作单个媒体项，自动生成对应数组 |

> 见 RSL-1.0.md §6.1（通用结构）、§6.1.1（let）、§6.1.2（parse）、§6.3（explore）、§6.5.1（E_COST）、§6.5.2（reuseDetailHtml）、§6.6.4（direct）、§6.8（hostStages）

## 10. 契约保留字段

### 10.1 `search`

| 保留字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `list` | Selector | 是 | 列表选择器 |
| `map.name` | Str | 是 | 书名 |
| `map.url` | Str | 是 | 详情 URL |
| `map.author` `map.intro` `map.kind` `map.cover` `map.wordCount` `map.lastChapter` `map.updateTime` | Str | 否 | 元数据 |
| `map.type` | 载体 | 否 | 条目级载体覆盖（唯一可用 `typeMap` 的地方） |
| `emptyOk` | bool | 否 | 空列表是否合法 |
| `verify` | object | 否 | 结果校验，如 `{"contains": "var:kw"}` |

输入：`var:kw`、`var:page`。

### 10.2 `detail`

| 保留字段 | 类型 | 必需 | 一句话 |
|---|---|---|---|
| `name` `author` `intro` `kind` `cover` `wordCount` `lastChapter` `updateTime` | Str | 否 | 元数据 |
| `tocUrl` | Str | 关键 | 目录页地址；缺省回退 `var:book.url` |
| `type` | 载体 | 否 | 本书实际载体（覆盖源级） |
| `canRename` | bool | 否 | 允许用详情页书名覆盖搜索页 |
| `downloads` | `{name,url,size?}[]` | 否 | 文件型源下载列表 |
| `extra` | object | 否 | 宿主展示用的自由字段（不进契约检查） |

### 10.3 `toc`

| 保留字段 | 类型 | 引擎行为（规范强制） |
|---|---|---|
| `map.url` | Str | 必需，正文入口 |
| `map.title` | Str | 章节标题 |
| `map.isVolume` | bool | `true` = 分卷标记，**不请求正文** |
| `map.isPay` | bool | `true` = 不请求正文，宿主显示"需购买" |
| `map.isVip` | bool | `true` = 标记受限章节，宿主决定提示方式 |
| `map.cover` | Str | 章节附加图（视频封面等） |
| `map.wordCount` | Str | 章节字数（示例用 `transform.number`） |
| `map.updateTime` | Str | 更新时间（示例用 `transform.date`） |

### 10.4 `content`（按载体分列）

| 载体 `type` | 保留输出字段 | 结构 |
|---|---|---|
| `novel` | `content` | `Block[]` 或 `Str[]`，允许图文混排 |
| `comic` | `images` | `{url, headers?}[]`，按序，**不得混入非图片资源** |
| `audio` | `audio` | `{url, duration?, tone?, variants?}`，直链 |
| `video` | `video` | `{url, format, variants?, headers?}`，mp4 / m3u8 |
| `file` | `files` | `{name, url, size?}[]` |
| `rss` | `feed` | 见 §10.6 |

所有载体共用：

| 字段 | 类型 | 一句话 |
|---|---|---|
| `title` | Str | 章节标题（站点只在正文页给标题时用） |
| `tracks` | array | 第二轨道（§10.5） |
| `nextPage` | Str | 同章续页 URL，引擎自动循环拼接（受 `limits.maxPages`） |
| `payAction` | Str \| `lib:` | 需购买时的宿主动作 |

### 10.5 `tracks`（弹幕 / 歌词 / 字幕）

| 字段 | 取值 | 一句话 |
|---|---|---|
| `kind` | `lyric` \| `danmaku` \| `subtitle` | 轨道类型 |
| `format` | `lrc` \| `bili-xml` \| `ass` \| `vtt` \| `text` | 轨道格式 |
| `url` | Str | 给 `url` 时引擎拉取 |
| `text` | Str | 给 `text` 时直接用（与 `url` 二选一） |
| 失败语义 | — | 轨道失败不得影响主内容；返回空轨道，不报错 |

### 10.6 `feed`（rss）

| 字段 | 类型 | 一句话 |
|---|---|---|
| `feed.items` | Selector | 条目列表 |
| `feed.map.title` | Selector | 标题 |
| `feed.map.link` | Selector | 链接 |
| `feed.map.date` | Selector | 日期 |
| `feed.map.desc` | Selector | 描述 |
| `feed.map.image` | Selector | 封面（如 `xpath:enclosure/@url`） |

> 见 RSL-1.0.md §6.2（search）、§6.4（detail）、§6.5（toc）、§6.6（content 六载体）、§6.6.1（tracks）、§6.6.2（feed）、§6.6.3（容器契约）

## 11. 脚本

### 11.1 两档 `lib`

| 档 | 能做什么 | 调用位置 | 限制 |
|---|---|---|---|
| `pure` | 纯计算：字符串、数组、正则、JSON、数学、`expr:` 可用函数 | `transform.js`、`lib:fn` 选择器、`let` | 无 IO、无时间、无随机、无上下文写 |
| `ctx` | 读写会话与上下文、发请求、读缓存、读源配置 | `script.*`、`transform.js`（`ctx: true`） | 禁文件系统、禁反射、受预算 |

作用域：`pure` 之间可互调，无 `this`、无全局变量、无隐式注入；`ctx` 函数第一个参数固定是 `ctx`；规则字段不执行任意 JS（唯一入口 `lib:` 与 `script`）。

### 11.2 三个钩子

| 钩子 | 时机 | 输入 | 输出 | 能改什么 |
|---|---|---|---|---|
| `script.before` | 请求**发出前** | `ctx` | 可选 `{url, headers, body}` | 请求本身 |
| `script.onResponse` | 响应**解码后、`parse` 前** | `ctx`, `body`（`str` 或 `bytes`） | `str` \| `bytes` | **原始响应内容** |
| `script.after` | `out` **求值后** | `ctx`, `out` | `out` | 已解析的契约结果 |

`onResponse` 补充约束：返回类型必须与输入一致（否则 `E_TYPE`）；禁调 `ctx.http`（请求递归 → `E_SCRIPT`，lint #16）；`script` 出现在逐章规则 → `E_COST`。

### 11.3 `ctx` 的受控接口

`lib.ctx` 与 `script.*` 可用；规范 §7.2 表共 9 行，展开为下列 11 个具名方法。

| # | 接口 | 说明 |
|---|---|---|
| 1 | `ctx.var(name)` | 读上下文 |
| 2 | `ctx.set(name, v)` | 写会话 |
| 3 | `ctx.config(key)` | 读源配置（`vars.<key>.persist` 的用户值） |
| 4 | `ctx.http.get(url, opts)` | 发请求，受 `http.rate` 与预算 |
| 5 | `ctx.http.post(url, opts)` | 同上 |
| 6 | `ctx.cache.get(k)` | 读缓存 |
| 7 | `ctx.cache.set(k, v, ttl)` | 写缓存 |
| 8 | `ctx.session.get(k)` / `.set(k, v)` | 会话读写 |
| 9 | `ctx.cookie.get` / `set` / `remove` | Cookie |
| 10 | `ctx.log(msg)` | 写调试轨迹 |
| 11 | `ctx.crypto` / `ctx.now()` / `ctx.rand()` | 加密（同 `expr:` 函数集）；`now`/`rand` 需 `time`/`random` feature |

**明确不提供**：`java.*`、`org.*`、`globalThis.fetch`、`require`、`process`、文件系统、任意反射、`eval`。

确定性：同一输入 + 同一 `limits` ⇒ 同一输出；`libSteps` 计量优先于墙钟；脚本异常 → `E_SCRIPT`（附函数名与行号，不静默吞掉）。

> 见 RSL-1.0.md §7.1（两档 lib）、§7.2（ctx 接口）、§7.3（确定性）、§7.4（作用域）、§7.5（三钩子）

## 12. `limits` 与超限行为

| 字段 | 示例值 | 一句话 | 默认 `on` 建议 |
|---|---|---|---|
| `maxBytes` | `{"value": 2097152, "on": "reject"}` | 单响应字节上限 | `reject`（超大响应常是登录墙/错误页） |
| `maxNodes` | `{"value": 5000, "on": "truncate"}` | 解析节点上限 | `truncate` |
| `timeoutMs` | `20000` | 单请求超时 | `reject` |
| `maxRedirects` | `5` | 跳转次数 | — |
| `maxPages` | `{"value": 50, "on": "truncate"}` | 分页页数上限 | `truncate` |
| `maxParallel` | `4` | 并发上限（`let.parallel`、宿主 `searchMany`） | — |
| `libSteps` | `1000000` | 脚本步数上限（与机器快慢无关） | — |
| `scriptTimeoutMs` | `3000` | 脚本墙钟兜底 | — |
| `mediaBytes` | `20971520` | 媒体资源字节上限 | — |
| `loginCheckTtlMs` | `300000` | 登录 `check` 结果缓存上限 | — |

| `on` 值 | 行为 |
|---|---|
| `reject` | 立即终止，返回 `E_BUDGET`，不带结果 |
| `truncate` | 截断到上限后继续，附 `W_BUDGET_TRUNCATED` 与部分结果 |

成本模型：`阶段耗时 ≈ 项数 × 逐字段规则数 × 每条规则的跨桥次数 × 单次跨桥成本`；逐章规则静态判定见 §6.5.1（`E_COST` 是加载期错误）。墙钟超时的错误码是 `E_TIMEOUT`，不是 `E_BUDGET`。

> 见 RSL-1.0.md §8.1（limits）、§8.2（on）、§8.3（成本模型）、§8.4（确定性计量）

## 13. 错误码总表

详细处置（怎么修、怎么复现、怎么断言）见 `ERRORS.md`。

| 码 | 时机 | 一句话 | 典型触发 |
|---|---|---|---|
| `E_PARSE` | 编译期 | JSON 语法错误（含行列号） | 源文件多/少一个逗号 |
| `E_SCHEMA` | 编译期 | 不符合 Schema / 互斥规则冲突 | `range` + `pick` 同给；`steps` 与 `fields` 同给 |
| `E_ENUM` | 编译期 | 枚举值非法 | 未知阶段键；`paginate.kind` 拼错 |
| `E_REF` | 编译期 | `var:` / `let:` 引用未定义（含契约字段拼写） | `var:book.tocUrl` 拼错；`next.to` 指向不存在的 step |
| `E_TYPE` | 编译期 / 运行期 | 管道步骤类型不匹配（运行期仅 `script.onResponse` 返回值） | `node[]` 上直接做 `regex` |
| `E_REGEX` / `E_REGEX_UNSUPPORTED` | 编译期 | 正则非法 / 超出 RE2 子集 | 用 `(?=)`、`\1` 反向引用 |
| `E_SELECTOR` | 编译期 | 选择器语法错误或缺命名空间 | `div.title` 未加 `css:` |
| `E_COST` | 编译期 | 逐章规则含 IO/脚本/不确定性 | `toc.out.map.*` 里用 `script` / `cookie:` / `expr:now()` |
| `E_EXPR_SYNTAX` / `E_EXPR_FN` / `E_EXPR_LIMIT` | 编译期 | `expr:` 语法 / 函数 / 长度超限 | 在 `expr:` 里写赋值或自定义函数 |
| `E_TEMPLATE` | 编译期 | `${...}` 插值本身非法 | `${expr:page` 未闭合；`${page}` 缺命名空间 |
| `E_UNSUPPORTED` | 加载期 | feature / 算法不支持（**不含** hostStage） | 用 `webview` 但宿主无该能力且未声明 |
| `E_VERSION_UNSUPPORTED` | 加载期 | `rsl` 主版本不兼容 | `"rsl": "3.0"` |
| `E_NET` | 运行期 | DNS/TLS/超时/连接失败 | 域名解析失败 |
| `E_TIMEOUT` / `E_CANCELLED` | 运行期 | 超时 / 宿主取消 | 墙钟兜底超时；用户中止（结果标 `partial: true`） |
| `E_HTTP` | 运行期 | 非 2xx（附状态码与片段） | 403 登录墙、404 |
| `E_CHARSET` | 运行期 | 解码失败 | GBK 站按 utf-8 强解 |
| `E_LOGIN_REQUIRED` | 运行期 | 未登录或失效 | `check` 失败、`requiresLogin` 阶段未登录 |
| `E_CHALLENGE` | 运行期 | 人机验证，需宿主介入 | 命中验证码页 |
| `E_DECODE` | 运行期 | 图片/媒体解密失败 | `media.decode` 的 `lib:fn` 返回非字节 |
| `E_NO_MATCH` | 运行期 | 选择器命中 0 且字段必填 | `fallback` 全部失败且无 `optional` |
| `E_EMPTY` | 运行期 | 列表为空且 `emptyOk` 为假（info 级） | 搜索接口正常返回但无结果 |
| `E_FIELD_MISSING` | 运行期 | 保留字段为空 | `detail` 未产出 `name`，`toc` 未产出 `url` |
| `E_VERIFY` | 运行期 | `verify` 校验失败 | HTTP 200 但返回错误页/别的内容 |
| `E_PROTECT_LOST` | 运行期 | `protect` 占位符在管道中丢失 | 正则 `replace` 吃掉 `\u0000p0\u0000` |
| `E_BUDGET` | 运行期 | 超预算 | `on: reject` 的 `maxBytes` 被撞穿 |
| `E_SCRIPT` | 运行期 | lib/脚本异常 | `lib` 抛异常；`webJs` 返回空；`onResponse` 调 `ctx.http` |
| `E_HOST_STAGE` | 运行期 | 宿主未实现该扩展阶段 | 源声明 `hostStages.comments` 而宿主无此实现 |

错误对象形状：`code` `severity`(`error|warning|info`) `stage` `path` `rule` `message` `hint` `sample` `degraded` `retryable`。永不返回裸空串。

> 见 RSL-1.0.md §11（错误模型）、§11.1（错误码）、§11.3（降级语义）

## 14. 警告码

| 码 | 一句话 | 典型触发 |
|---|---|---|
| `W_UNKNOWN_FIELD` | 未知字段，忽略但不拒绝加载 | 同 MAJOR 内出现新字段；非 `x-` 前缀的扩展字段 |
| `W_BUDGET_TRUNCATED` | 预算截断，附部分结果 | `maxNodes` / `maxPages` 的 `on: truncate` 生效 |
| `W_DEGRADED` | 增强字段失败，主结果保留 | 富目录失败、推荐为空、歌词失败、弹幕失败 |
| `W_DEPRECATED` | 使用了已弃用写法 | — |
| `W_ZIP_LENGTH` | `zip` 两列表长度不等，多余项丢弃 | 书名 20 项、价格 15 项 |

警告不阻断执行，进 `--trace` 输出。

> 见 RSL-1.0.md §11.2（警告）、§11.3（降级语义）、§3.6.1（W_ZIP_LENGTH）

## 15. 变量命名空间速查

| 命名空间 | 生命周期 | 可写者 | 能出现在哪些阶段 |
|---|---|---|---|
| `var:` | 跟随源（用户可编辑）+ 宿主注入的上下文 | 用户 / `script` / 宿主 / 契约 | 全部；具体键受 §5.3 可用阶段约束（如 `var:kw` 仅 search、`var:chapter.*` 仅 content） |
| `let:` | 单个阶段 | 阶段 `let` | 仅声明它的阶段；逐章规则允许（lint #3：必须同阶段先声明） |
| `session:` | 单次运行 | `script` / `let` | 全部；逐章只允许读 |
| `token:` | 跟随登录态 | `login` | 全部；**逐章禁用** |
| `cookie:` | 跟随 Cookie 存储 | 宿主 / `login` | 全部；**逐章禁用**（只读） |

### 15.1 内置 `var:` 键

| 键 | 类型 | 可用阶段 |
|---|---|---|
| `var:baseUrl` | string | 全部 |
| `var:url` | string | 全部（当前请求 URL） |
| `var:response` | string \| bytes | 全部 |
| `var:charset` | string | 全部 |
| `var:kw` | string | search |
| `var:page` | int | 有 `paginate` 或 `groups` 的阶段 |
| `var:group` | object | explore |
| `var:filter.<key>` | string | explore |
| `var:login.<field>` | string | login 步骤 |
| `var:book.*` | object | detail / toc / content |
| `var:chapter.*` | object | content |
| `var:scriptResult` | any | 有 `script` 钩子的阶段（钩子返回值） |

### 15.2 内置键冲突

`vars` 的键名不得与内置上下文键冲突：`baseUrl` `url` `response` `charset` `kw` `page` `group` `filter` `login` `book` `chapter`——冲突 → 编译期 `E_REF`。

变量不可冒充：`persist: true` → 用户修改后持久化但**不随源文件导出**；`secret: true` → 永不进入日志、导出、错误信息、`--trace` 输出。

> 见 RSL-1.0.md §5.1（六层状态）、§5.2（vars）、§5.3（内置变量）、§5.4（契约注入）、§5.5（会话）、§6.5.1（逐章限制）

---

| 版本 | 日期 | 说明 |
|---|---|---|
| 2.0 | 2026-11-21 | 首版，对齐 `RSL-1.0.md` 2.0.0-draft.1 整体重写 |
