---
id: rsl-patterns
type: guide
status: draft
title: RSL 任务配方手册
parent: rsl-language
tags: [rsl, patterns, recipes]
---

# RSL 任务配方手册 · PATTERNS

| | |
|---|---|
| 版本 | 2.0（对应 `RSL-1.0.md` v2.0.0-draft.1） |
| 状态 | 非规范性手册（normative 只有 `RSL-1.0.md`） |
| 用途 | "我要做某件事，怎么写" —— 30 个配方，可直接复制 |
| 不负责 | 字段定义（→ `REFERENCE.md`）、报错修复（→ `ERRORS.md`）、Legado 翻译（→ `MIGRATION.md`） |

**读法**：每个配方 = 场景一句话 + 可复制片段 + 3–5 条注意 + 一行字段索引。片段多为 `stages.<name>` 的主体，粘回完整源时要自己补 `rsl` `id` `name` `type` `baseUrl` 五个必填顶层字段。片段里出现的字段名、枚举值、嵌套层级全部取自 `RSL-1.0.md`；`§x.y` 指规范小节。

### 配方里引用的 REFERENCE.md 章节

| § | 覆盖内容 |
|---|---|
| §1–§3 | 顶层字段（`rsl` `id` `name` `type` `baseUrl` `match` `lang` `version`）、`http` / `cookie` / `cache` / `media`、`vars` / `limits` / `meta` |
| §4 | Selector 全字段：`$` `attr` `range` `pick` `exclude` `exists` `where` `map` `blocks` `paragraphs` `strip` `protect` `transform` `fallback` `flags` `typeMap` `optional` `default` `limit` |
| §5 | `transform` 步骤全表（含 `zip` `protect` `font` `date` `numChapter` `json` `charset`） |
| §6 | `request`：`method` `params` `body` `headers` `charset` `encode` `response` `engine` `webJs` `sniff` `autoUpgradeIf` `skip` `retry` `timeout` |
| §7 | `login`：`type` `fields` `submit` `steps` `next` `check` `headers` |
| §8 | `paginate`：`page` `next` `cursor` `enumerate` `into` `joinWith` `carry` `max` `stopIf` |
| §9–§12 | `search` / `explore`（`filters` `groups` `groupsFrom`）、`detail` / `toc`（`tocUrl` `reuseDetailHtml` `order` `dedupe`）、`content` 与六载体、`hostStages` |
| §13 | `lib`（`pure` / `ctx`）、`script.before` / `onResponse` / `after`、`ctx` API |
| §14 | `merge` 多源合并 |
| §15–§17 | `requires`（`engine` `features` `crypto` `selectorLevel`）、错误码与警告、`rsl test` 与工具链 |

## A 入门（3）

### A1 · 最小 search（HTML）
场景：一个静态 HTML 小说站，只要"搜关键词 → 出书名和详情 URL"。
```jsonc
"stages": {
  "search": {
    "request": { "url": "/search", "params": { "q": "var:kw", "p": "var:page" }, "response": "html" },
    "out": {
      "list": "css:ul.result > li",
      "map": { "name": "css:a.title",
               "url": { "$": "css:a.title", "attr": "href", "transform": [{ "absolute": true }] } },
      "emptyOk": true
    },
    "paginate": { "kind": "page", "pageVar": "page", "max": 20 }
  }
}
```
- `search` 的最小必填三件套是 `out.list` + `map.name` + `map.url`；`detail` `toc` `content` 三阶段必需，`search` 与 `explore` 至少其一（§6）。
- 字符串规则等价于 `{ "$": "<该字符串>" }`（§3.3），但命名空间不能省：`"ul.result > li"` → `E_SELECTOR`。
- `href` 是相对路径时必须 `{ "absolute": true }`，否则宿主回访不到详情页。
- 空列表在"接口正常但没内容"时是合法结果，写 `emptyOk: true`，否则记 `E_EMPTY`（info 级）。

用到的字段：`stages.search.request` `out.list` `out.map` `out.emptyOk` `paginate.kind/pageVar/max`（见 REFERENCE.md §6 §8 §9–§12）

### A2 · search → detail → toc → content 串起来
场景：把四个阶段接成一条能读的链路。
```jsonc
"detail": {
  "request": { "url": "var:book.url", "response": "html" },
  "out": { "name": { "$": "css:h1", "transform": [{ "trim": true }] },
           "author": "css:.info .author",
           "tocUrl": { "$": "css:a.btn-read", "attr": "href", "transform": [{ "absolute": true }], "optional": true } }
},
"toc": {
  "request": { "url": "var:book.tocUrl", "response": "html" },
  "order": "auto", "dedupe": "url",
  "out": { "list": "css:#list li",
           "map": { "title": "css:a",
                    "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] } } }
},
"content": {
  "request": { "url": "var:chapter.url", "response": "html" },
  "out": { "title": { "$": "css:h1", "optional": true },
           "content": { "$": "css:#content", "paragraphs": { "split": "br", "minLength": 1 } } }
}
```
- `var:book` 是宿主显式传入的选中条目，字段由 `search.out.map` 声明；`var:chapter` 同理来自 `toc.out.map`。引擎**不会**把第一个结果自动提升（§5.4），所以 `var:book.tocUrl` 拼错是编译期 `E_REF`。
- `detail.out.tocUrl` 缺省回退 `var:book.url`（§6.4）；详情页与目录页不同是常态，能取就取。
- `content` 的入口是 `var:chapter.url`，不是 `var:book.url`，两个变量不能互换。
- `paragraphs` 与 `blocks` 互斥（§3.1）：纯文本正文用 `paragraphs`，图文混排才用 `blocks`。

用到的字段：`detail.out.tocUrl` `toc.order/dedupe` `content.out.content`（见 REFERENCE.md §9–§12）

### A3 · JSON 接口站
场景：搜索走 `GET /api/search?q=`，返回 `{"data":{"items":[…]}}`。
```jsonc
"search": {
  "request": { "url": "var:api/search", "params": { "q": "var:kw" }, "response": "json" },
  "out": {
    "list": "json:$.data.items[*]",
    "map": { "name": "json:$.title",
             "url":  { "$": "expr:'https://example.com/book/' + var:item.id" },
             "type": { "$": "json:$.kind", "typeMap": { "1": "comic", "2": "audio", "4": "video" } } },
    "emptyOk": true
  }
}
```
- `response: "json"` 让阶段入口作用域直接是解析后的 JSON 值；漏写就会拿 HTML 字符串跑 `json:`，得 `E_NO_MATCH`。
- `map` 内部作用域是单个匹配项，项内字段用 `var:item.*` 读（§3.3）；`json:$.title` 也是相对该项求值。
- 拼 URL 只能靠 `expr:`（唯一允许算术与 `+` 拼接的位置，§4.4），不要在 `json:` 里写拼接。
- `typeMap` 只允许出现在保留字段 `type` 上（§3.1），其余字段带 `typeMap` → `E_SCHEMA`。

用到的字段：`request.response` `out.list` `map.type` `typeMap` `expr:` `var:item`（见 REFERENCE.md §4 §6 §9–§12）

## B 取数（9）

### B1 · 单步登录
场景：账号 + 密码换 token，之后每个请求带 `Authorization`。
```jsonc
"login": {
  "type": "form",
  "fields": [ { "key": "username", "label": "账号" },
              { "key": "password", "label": "密码", "secret": true } ],
  "submit": {
    "request": { "url": "/api/login", "method": "POST",
                 "body": { "type": "form", "data": { "u": "var:login.username", "p": "var:login.password" } } },
    "ok": { "$": "json:$.code", "eq": 0 },
    "token": { "$": "json:$.data.token" },
    "cookie": { "$": "cookie:sessionid" }
  },
  "check": { "request": { "url": "/api/me" }, "ok": { "$": "json:$.uid", "exists": true } },
  "headers": { "Authorization": "Bearer ${token:auth}" }
}
```
- `type` ∈ `form` `cookie` `token` `qr`（§4.5.1）；`fields` 的 `key` 通过 `var:login.<key>` 读回（§5.3）。
- `ok` 是登录成功判定；`token` 进 token 商店（用 `token:<name>` 读），`cookie` 声明要保存哪一个 Cookie。
- 登录态隔离键 = 源 `id` + 站点域名 + 登录名（§4.5.3）：换账号天然隔离，同名新源不继承旧 Cookie。
- `check` 在 `requiresLogin` 的阶段执行前跑；失败 → `E_LOGIN_REQUIRED`，宿主据此弹登录 UI。缓存不得超过 `limits.loginCheckTtlMs`（默认 300000）。

用到的字段：`login.type/fields/submit/check/headers`、`var:login.*`、`token:`（见 REFERENCE.md §7）

### B2 · 两步登录（`login.steps` + `next.when`）
场景：账号密码之后**可能**要短信验证码，由响应里的 `need_sms` 决定。
```jsonc
"login": {
  "type": "form",
  "steps": [
    { "id": "cred",
      "fields": [ { "key": "username", "label": "账号" },
                  { "key": "password", "label": "密码", "secret": true } ],
      "submit": { "request": { "url": "/api/login", "method": "POST",
                               "body": { "type": "json", "data": { "u": "var:login.username",
                                                                   "p": "var:login.password" } } },
                  "ok": { "$": "json:$.code", "eq": 0 } },
      "next": { "when": { "$": "json:$.need_sms", "eq": true }, "to": "sms" } },
    { "id": "sms",
      "fields": [ { "key": "code", "label": "短信验证码", "secret": true } ],
      "submit": { "request": { "url": "/api/verify", "method": "POST",
                               "body": { "type": "json", "data": { "code": "var:login.code" } } },
                  "ok": { "$": "json:$.code", "eq": 0 },
                  "token": { "$": "json:$.token" } } }
  ]
}
```
- `steps` 与顶层 `fields`/`submit` **互斥**，二者都给 → `E_SCHEMA`（§4.5.2、lint 9）。
- `next` 只有两种形态：`{ "to": "<stepId>" }` 或 `{ "when": …, "to": … }`；**无 `next` 即结束**——末步别写 `{ "to": "done" }`，除非真有 `id: "done"` 的步骤，否则 `E_REF`。
- 步骤图必须是 DAG，有环 → `E_SCHEMA`（lint 10）。
- 宿主只负责"渲染当前步 `fields` + 提交值"，步骤推进由引擎驱动；不要试图用 JS 操作登录 UI（§16 明确不采纳）。

用到的字段：`login.steps[]`（`id` `fields` `submit` `next.when` `next.to`）（见 REFERENCE.md §7）

### B3 · 搜索签名：`let` + `expr:md5`
场景：接口要 `ts` + `sign=md5(kw+ts+salt)`，签名放在 POST body 里。
```jsonc
"let": { "ts":   "expr:now()",
         "sign": "expr:md5(var:kw + str(let:ts) + var:appKey)" },
"request": {
  "url": "var:line/api/search", "method": "POST",
  "body": { "type": "json", "data": { "q": "var:kw", "ts": "let:ts", "sign": "let:sign" } },
  "response": "json"
}
```
- `let` 按声明顺序求值，后项可引用前项（`let:ts`）；循环引用 → 编译期 `E_REF`（§6.1.1）。
- `now()` 需 `features: ["time"]`；`md5` 需 `features` 含 `crypto` 且 `crypto: ["md5"]`（§4.4、§9.1）。
- `let` 的值可以是表达式、`{ request, take }` 单次请求、`{ parallel: [...], limit }` 并发批次、`lib:` 调用——"先取 token 再签名"就靠这个。
- 签名进 `body.data`，不要拼进 URL：URL 会进缓存键与日志，`secret` 的语义就废了（§5.2）。

用到的字段：`stages.*.let` `request.body.type/data` `expr:` `requires.features/crypto`（见 REFERENCE.md §6 §15–§17）

### B4 · 分页三写法（page / next / cursor）
场景：三种常见翻页形态，按站点选一种，不要混。
```jsonc
// ① page：引擎注入 var:page，规则自己拼参数
"paginate": { "kind": "page", "pageVar": "page", "max": 20 }
// ② next：从响应里取"下一页"URL
"paginate": { "kind": "next", "next": "css:a.next-page", "max": 50 }
// ③ cursor：接口给游标，写回请求的某个位置
"paginate": { "kind": "cursor",
              "cursor": { "take": "json:$.data.next_offset", "into": "params.offset" },
              "carry": ["search_id"], "max": 20 }
```
- `kind` ∈ `page` `next` `cursor` `enumerate`，字段必须与 kind 匹配，否则 `E_SCHEMA`（lint 6）；`pageVar` 与 `enumerate` 不能同时出现（lint 7）。
- `cursor.into` 可指向 `params.x`，也可指向 `body.data.x`（POST 接口常见）；`carry` 透传游标之外的会话字段（§4.7）。
- 目录分页（`toc`）额外允许 `next` 返回 **URL 数组**，一次给出全部后续页（§4.7）。
- 四个 kind 都受 `limits.maxPages` 约束；"没有更多"要靠 `stopIf`（见 F2 第 14 条），否则无限触底。

用到的字段：`paginate.kind/pageVar/next/cursor.take/cursor.into/carry/max`（见 REFERENCE.md §8）

### B5 · `paginate.enumerate`（含 `joinWith` 一次拼 URL）
场景：站点要求一次给出多个页码，或用一组固定页码遍历。
```jsonc
// 写法 A：逐值请求，结果按值顺序拼接
"paginate": { "kind": "enumerate", "enumerate": [1, 2, 3, 4, 5], "into": "params.pn" }
// 写法 B：只发一次请求，把整组页码拼进一个参数（等价 Legado 的 <1,2,3>）
"paginate": { "kind": "enumerate", "enumerate": { "from": 1, "to": 5 },
              "joinWith": ",", "into": "params.pages" }   // → pages=1,2,3,4,5
```
- `enumerate` 可以是字面数组，或 `{ "from": 1, "to": 5 }`（可带 `"step": 1`）（§4.7.1）。
- 给 `joinWith` ⇒ **只发一次请求**；不给 ⇒ 每个值发一次请求，结果按值顺序拼接（保序由引擎负责）。
- 与 `page` 互斥：同一阶段既有 `pageVar` 又有 `enumerate` → `E_SCHEMA`。
- `into` 写的是请求里的位置（`params.pn` / `body.data.pn`），不是变量名。

用到的字段：`paginate.enumerate/joinWith/into`（见 REFERENCE.md §8）

### B6 · `toc.reuseDetailHtml`（详情与目录同页）
场景：`detail` 请求的那一页下面就是完整目录，不想再请求一次。
```jsonc
"detail": { "request": { "url": "var:book.url", "response": "html" },
            "out": { "name": "css:h1", "tocUrl": "var:book.url" } },
"toc": {
  "reuseDetailHtml": true,
  "out": { "list": "css:#list dd",
           "map": { "title": "css:a",
                    "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] } } }
}
```
- 语义是"**URL 相同**才复用"：本阶段实际 URL 与 `detail` 实际 URL 一致 → 不发请求，直接把 detail 响应体当本阶段根作用域；不同则照常请求（§6.5.2）。
- 所以要么让 `toc.request.url` 与 detail 一致，要么干脆省略 `toc.request`（此时按 `var:book.tocUrl` 缺省回退 `var:book.url`）。
- 复用不产生请求，因此不计入 `http.rate` 限流；`detail` 被宿主跳过时本项无效，正常请求。
- 源里必须有 `detail` 阶段，否则 lint 17 报 `E_SCHEMA`。

用到的字段：`toc.reuseDetailHtml` `detail.out.tocUrl`（见 REFERENCE.md §9–§12）

### B7 · `content.direct`：正文就是 URL
场景：纯图片站/直链站，`chapter.url` 本身就是资源，不需要请求正文页。
```jsonc
"content": { "request": { "skip": true }, "direct": true,
             "out": { "content": "var:chapter.url" } }
```
- `direct: true` ⇒ 引擎**不请求**，`out.content` 直接取 `var:chapter.url`；此时 `request.skip` 可省略（§6.6.4）。
- 载体为 `comic` / `audio` / `video` / `file` 时，`direct` 把该 URL 当作**单个媒体项**，自动生成对应的 `images` / `audio` / `video` / `files` 数组。
- `direct` 与**非 `skip`** 的 `request` 同时给 → `E_SCHEMA`（lint 8）：不能既请求又直取。
- 不写 `direct` 又想不请求，必须显式 `request.skip: true`，否则引擎会去请求 `baseUrl` 拼出的地址（§4.2.1）。

用到的字段：`content.direct` `request.skip` `out.content`（见 REFERENCE.md §6 §9–§12）

### B8 · 先拿 token 再请求
场景：每个阶段开始前要先换一个临时 token，再带着它发正式请求。
```jsonc
"let": { "token": { "request": { "url": "var:api/token", "params": { "key": "var:appKey" } },
                    "take": "json:$.data.token" } },
"request": { "url": "var:api/search",
             "headers": { "X-Token": "let:token" },
             "params": { "q": "var:kw" },
             "response": "json" }
```
- `let` 项写成 `{ request, take }` 就是"阶段内的一次子请求"，`take` 是取值选择器，结果用 `let:<name>` 引用（§6.1.1）。
- 需要**跨阶段/跨运行**复用改用 `cache` 块 `{ "request": …, "take": …, "ttl": 3600 }`（§4.6）；带维度的键（如 `"key": "var:book.id"`）防串书（§12.3）。
- 多个子请求互不依赖时写 `{ "parallel": [ … ], "limit": 2 }`，受 `limits.maxParallel` 与 `http.rate` 双重约束（§6.1.1、§12.2）。
- `secret: true` 的源配置（`vars.appKey`）永不进入日志、导出、错误信息与 `--trace` 输出（§5.2）。

用到的字段：`let.<name>.request/take` `request.headers` `vars.*.secret` `cache.*.ttl`（见 REFERENCE.md §1–§3 §13）

### B9 · GBK 站：`charset` + `encode`
场景：老站点响应是 GBK，搜索关键词也必须按 GBK 编码发出去。
```jsonc
"request": {
  "url": "/search", "params": { "q": "var:kw" },
  "charset": "gbk",                 // 响应字节 → 字符串
  "encode": { "params": "gbk" },    // 请求参数 → 字节
  "response": "html"
}
```
- 两个方向是两个字段（§4.3）：`charset` 管**响应解码**，`encode.params` / `encode.body` 管**请求编码**。只写 `charset` 是历史 bug 的原始形态——关键词照样乱码。
- 响应解码顺序强制：`request.charset` → Content-Type → `<meta charset>` → BOM → 自动探测（仅 `features: ["detect"]`）→ utf-8 兜底（§6.7）。
- `response: "bytes"` 时不做解码，改用管道 `{ "charset": "gbk" }` 解码（§6.7、§3.6）。
- 解码**之后**再做 `re:` 匹配；`flags`（`m` `s` `i`）只对 `re:` 有效（§3.1）。

用到的字段：`request.charset` `request.encode.params/body` `transform.charset` `requires.features["detect"]`（见 REFERENCE.md §5 §6）

## C 解析（8）

### C1 · 正文删广告：`strip`
场景：正文容器里混着脚本、样式、广告块和居中的二维码。
```jsonc
"out": {
  "title": { "$": "css:h1.chapter-title", "transform": [{ "trim": true }] },
  "content": {
    "$": "css:div#content",
    "strip": ["css:script", "css:style", "css:.ad", "css:div[align=center]"],
    "paragraphs": { "split": "br", "minLength": 1 },
    "transform": [{ "replace": [["&nbsp;", " "]] }, { "html": true }]
  }
}
```
- `strip` 是 `Selector[]`，语义是"**先删除匹配的子节点**再取值"（§3.1），比事后正则干净。
- `strip` 之后 `paragraphs` 按 `<br>` 切段；`paragraphs` 与 `blocks` 互斥（lint 5）。
- 管道顺序有意义：先 `replace` 去实体残留，再 `html: true` 反转义（§3.6）。反过来会把实体再吃一遍。
- 正文分页用 `nextPage`（同章续页 URL，引擎自动循环拼接，受 `limits.maxPages`），别把续页塞进 `toc`（§6.6）。

用到的字段：`out.content.strip` `out.content.paragraphs` `out.nextPage` `transform.html`（见 REFERENCE.md §4 §5 §9–§12）

### C2 · `transform.replace` + `protect` 保护 `<usehtml>`
场景：正文里带站点自己的 `<usehtml>` 标记（内容可直接当 HTML 渲染），清洗正则会把它吃掉。
```jsonc
"content": { "$": "css:#content",
             "protect": ["<usehtml>", "</usehtml>"],
             "transform": [ { "replace": [["广告", ""], ["re:\\s*\\n\\s*", "\n"]] }, { "trim": true } ] }
```
- `protect` 在**选择器层**时作用于整个 `transform`；在 `transform` 内时从该步开始生效（§3.1）。
- 管道入口把命中片段换成不可猜的占位符，出口原样还原；占位符不参与匹配、不计入长度、不进入 `slice` / `regex` 的输入（§3.6.2）。
- 占位符在管道中被误删 → `E_PROTECT_LOST`，附原占位符位置；**绝不静默丢弃原文**。
- 保护整块节点用选择器形式：`"protect": { "$": "css:pre.code" }`。

用到的字段：`protect`（选择器层 / 管道步骤）、`transform.replace`（见 REFERENCE.md §4 §5）

### C3 · 中文数字 `numChapter`
场景：标题是"第一百二十三章 风起"，宿主要数字排序。
```jsonc
"out": { "list": "css:#list li",
         "map": { "title": { "$": "css:a", "transform": [{ "trim": true }, { "numChapter": true }] } } }
```
- `numChapter: true` 把 `"第一百二十三章"` 归一成 `123`（§3.6）；`cjkNumber` 是通用版（`"to"` / `"from"`），不限于章节。
- 两者都在逐章规则白名单里（§6.5.1），可以安全放在 `toc.out.map`。
- `toc.order: "auto"` 靠比较首尾标题的章节序号判正倒序；序号没归一化时 auto 会判错，先规范化再交给它（§6.5）。
- 已归一成数字后不要再叠加 `number`：那是另一套语义，会把中间串重算一遍。

用到的字段：`transform.numChapter` `transform.cjkNumber` `toc.order`（见 REFERENCE.md §5 §9–§12）

### C4 · 时间格式 `transform.date`
场景：`updateTime` 是 `1700000000` 时间戳，或 `2026年11月21日` 这种中文日期。
```jsonc
// ① Unix 时间戳
"updateTime": { "$": "css:.t", "transform": [{ "date": { "from": "x", "to": "yyyy-MM-dd" } }], "optional": true }
// ② 自定义格式 → 标准格式
"updateTime": { "$": "css:.t", "optional": true,
                "transform": [{ "date": { "from": "yyyy年MM月dd日", "to": "yyyy-MM-dd", "tz": 8 } }] }
```
- `date` 参数是 `{ from, to, tz }`，`from: "x"` 表示 Unix 时间戳（§3.6、§6.5 示例）。
- `to` 是**输出**格式，必须显式给；`tz` 是时区偏移（小时），站点时间无时区标注时靠它定正。
- `date` 在逐章白名单内，可以放进 `toc.out.map`（§6.5.1）。
- 时间是增强字段：`optional: true` + 转换失败保留原串，别让整章失败；增强失败只该 `degraded`，不该清空主结果（§11.3）。

用到的字段：`transform.date` `out.updateTime` `optional`（见 REFERENCE.md §4 §5）

### C5 · 字体反爬 `transform.font`
场景：正文汉字用自定义字体渲染，靠字形映射表还原。
```jsonc
"lib": { "pure": "function glyph(ch){ /* 查字表 → 真实字符 */ return ch; }" },
"requires": { "features": ["js", "font"] },
"out": { "content": { "$": "css:#content",
                      "transform": [ { "trim": true },
                                     { "font": { "map": "lib:glyph", "ref": "var:normalFont" } } ] } }
```
- `font` 的两个参数是 `map`（`lib:glyph` 纯函数，逐字映射）与 `ref`（正常字体基准，通常是 `vars` 里的配置项）（§3.6）。
- 必须有 `features: ["font"]`，否则加载期 `E_UNSUPPORTED`（§9.1）；缺 `js` 则 `lib:` 也无法解析。
- **`font` 在逐章白名单里**（§6.5.1）：它是纯映射，与 `js(lib.pure)` 同构，`toc.out.map.title` 里可以直接用。注意成本仍是"字符数 × 跨桥次数"，1200 章 × 20 字的量级可接受；若站点整本都混淆，优先在 `content` 阶段映射。
- 映射表放 `lib.pure`（无 IO、无时间、无随机），保证同输入同输出（§7.1、§7.3）。

用到的字段：`transform.font` `lib.pure` `requires.features["font"]`（见 REFERENCE.md §5 §13 §15–§17）

### C6 · 两列表拼一条 `transform.zip`
场景：书名在一列、价格在另一列，要合成"书A · 12元"。
```jsonc
"out": { "list": { "$": "css:#list li",
                   "transform": [ { "zip": { "with": "css:.price", "sep": " · " } } ] },
         "map": { "name": "text" } }
```
- `zip` 对应 Legado 的 `%%`：按下标把两个列表配成一条（§3.6.1）。
- `with` 在该步骤的**父作用域**求值（不是当前项）——`css:.price` 是"整页的价格列"，不是"这一项里的价格"。
- 长度不等以短者为准，多余项丢弃并记 `W_ZIP_LENGTH`；长度对不上先查选择器，别急着改 `zip`。
- 输入须是 `Str[]`；`zip` 在逐章白名单内，可用于 `toc.out.map`（§6.5.1）。

用到的字段：`transform.zip.with/sep` `W_ZIP_LENGTH`（见 REFERENCE.md §5 §15–§17）

### C7 · 末 5 章与倒序（`range` 负索引 / 负步长）
场景：源站目录最新在前，只要最后 5 章，或把整条目录倒过来。
```jsonc
"list": { "$": "css:#list li", "range": "-5:" }        // 末 5 项
"list": { "$": "css:#list li", "range": "-1:0:-1" }    // 全量倒序（对应 Legado 的 [-1:0]）
"list": { "$": "css:#list li", "range": "0:20:2" }     // 隔一取一
```
- `range` 语法是 `start:end[:step]`，语义同 Python 切片，**`end` 不含**（§3.1）。
- `step` 为负时反向遍历，此时 `start` 应大于 `end`；写成 `"0:-1:-1"` 会得到空结果。
- `range` 与 `pick` 互斥（lint 4）；`exclude` 可与二者之一共存——先取子集再排除。
- 选择器的 `range` 切**数组**，`transform.slice` 切**字符串**（§3.6）；要整本升序优先用 `toc.order`，`range` 倒序不改 URL 顺序。

用到的字段：`range` `pick` `exclude` `transform.slice` `toc.order`（见 REFERENCE.md §4 §9–§12）

### C8 · 列表项嵌 JSON（`parse` + `transform.json`）
场景：整份数据藏在 `__NEXT_DATA__` 里，或单个条目塞了一段 JSON 字符串。
```jsonc
"parse": { "$": "css:script#__NEXT_DATA__", "transform": [{ "json": true }] },
"out": { "list": "json:$.props.pageProps.books[*]",
         "map": { "name": "json:$.title",
                  "url":  { "$": "expr:'/book/' + var:item.id", "transform": [{ "absolute": true }] } } }
```
- `parse` 的输出成为该阶段**所有规则的根作用域**；`var:response` 仍保留原始响应（§6.1.2），要在脚本里看原文时用它。
- `transform.json` 是 `Str → Json`；`json:$.` 走 RFC 9535（含过滤表达式），需 `selectorLevel.jsonpath`（§9.2）。
- `parse` 只能是阶段级字段，不能写进 `map`；单个字段要转 JSON 就用管道 `json`，例如 `{ "$": "css:.meta", "attr": "data-json", "transform": [{ "json": true }] }`。
- `parse` 里先 `css:` 取到节点再 `json` 化——对节点直接跑 `json` 是类型错误 `E_TYPE`（§3.6 类型检查）。

用到的字段：`stages.*.parse` `transform.json` `selectorLevel.jsonpath`（见 REFERENCE.md §5 §6 §15–§17）

## D 载体（4）

### D1 · 漫画 `images` + 逐图 headers
场景：接口给图片数组，CDN 校验 Referer，图片还做了字节级异或。
```jsonc
"media": { "decode": { "in": "bytes", "out": "bytes", "fn": "lib:decryptImage" },
           "cache": { "ttl": 604800 }, "maxBytes": 31457280 },
"out": { "title":  { "$": "json:$.data.title", "optional": true },
         "images": { "$": "json:$.data.images[*]", "transform": [ { "js": "lib:toImages" } ] },
         "tracks": [] }
// lib.pure: function toImages(a){ return a.map(function(u){ return {
//   url: u, headers: { Referer: 'https://example.com/' } }; }); }
```
- `images` 每项是 `{ url, headers? }`；逐资源 `headers` 是防盗链的必需项（§4.6）。
- `media.decode` 的 `in`/`out` 是**契约**：引擎必须把原始字节交给 `lib:decryptImage`，并把返回字节当作图像数据（§4.6）；解密失败 → `E_DECODE`。
- `images` 里每一项都会被当作页面资源，混入评论横幅 / SVG / 占位图会导致整话渲染失败（§6.6.3）；评论入口走 `hostStages` 或宿主按钮。
- `maxBytes`（媒体单文件上限）与 `limits.mediaBytes` 取小值；`cache.ttl` 按图片是否稳定给，天天换的签名 URL 别缓存。

用到的字段：`out.images[]` `media.decode/cache/maxBytes` `limits.mediaBytes`（见 REFERENCE.md §1–§3 §9–§12）

### D2 · 音频 `audio` + 歌词 `tracks`
场景：有声书/音乐站，音频直链 + LRC 歌词。
```jsonc
"out": {
  "audio": { "url": { "$": "json:$.data.audio", "transform": [{ "absolute": true }] },
             "duration": { "$": "json:$.data.duration", "optional": true } },
  "tracks": [ { "kind": "lyric",   "format": "lrc",  "url": "json:$.data.lrc" },
              { "kind": "danmaku", "format": "text", "text": "var:dmText" } ]
}
```
- `audio` 结构是 `{ url, duration?, tone?, variants? }`（§6.6）；`url` 必须是播放器能直接消费的地址，错误 JSON/提示文本不得冒充媒体地址（§6.6.3）。
- `tracks` 统一一种结构：`kind` ∈ `lyric` `danmaku` `subtitle`；`format` ∈ `lrc` `bili-xml` `ass` `vtt` `text`（§6.6.1）。
- `url` 与 `text` **二选一**：给 `url` 引擎去拉内容，给 `text` 直接用。
- 轨道失败**不得影响主内容**——返回空轨道、不报错（§6.6.1）；没有轨道就写 `"tracks": []`，别省这个字段。

用到的字段：`out.audio` `out.tracks[].kind/format/url/text`（见 REFERENCE.md §9–§12）

### D3 · 视频 `sniff` m3u8 + `variants`
场景：播放页地址不算数，真正的 m3u8 只出现在页面网络请求里。
```jsonc
"content": {
  "request": { "url": "var:chapter.url", "response": "html",
               "engine": "auto",
               "autoUpgradeIf": { "$": "css:#app > .skeleton", "exists": true },
               "sniff": "re:\\.(m3u8|mp4)(\\?|$)" },
  "out": {
    "video": { "url": { "$": "var:playUrl" },
               "format": "const:\"m3u8\"",
               "variants": { "$": "json:$.data.play.urls[*]", "optional": true },
               "headers": { "Referer": "var:baseUrl" } },
    "tracks": [ { "kind": "danmaku", "format": "bili-xml", "url": "json:$.data.danmaku" } ]
  }
}
```
- `sniff` 是资源嗅探正则，从页面网络请求里匹配资源 URL，返回 `str[]`（§4.8）；`format` 取 `mp4` / `m3u8`，与 `video` 契约一致。
- 规范只说了 `sniff` 返回 `str[]`，**未定义注入点**（§4.8 / §5.3 无对应变量名），所以本片段用 `var:playUrl` 占位——实际可引用的名字以 `REFERENCE.md` 与实现为准，别自己发明字段。
- `variants` 是同一视频的多码路；只给一个地址时省略它，让播放器按 `format` 处理。
- `headers` 是逐资源请求头（结构与 `images` 的项相同，§4.6）；`engine: "auto"` = 先 HTTP，命中 `autoUpgradeIf` 再升级一次（§9.3），需 `features: ["webview"]`。

用到的字段：`request.sniff/engine/autoUpgradeIf` `out.video.url/format/variants/headers`（见 REFERENCE.md §6 §9–§12）

### D4 · RSS `feed`
场景：订阅型源，一条 feed 就是一份"章节列表"。
```jsonc
"type": "rss",
"content": {
  "request": { "url": "var:chapter.url", "response": "html" },
  "out": { "feed": {
    "items": "xpath://item",
    "map": { "title": "xpath:title", "link": "xpath:link",
             "date": "xpath:pubDate", "desc": "xpath:description",
             "image": "xpath:enclosure/@url" } } }
}
```
- `type: "rss"` 是载体枚举的一种（`novel` `comic` `audio` `video` `file` `rss`）；条目级 `type` 可覆盖源级（§2.3）。
- `feed` 的键固定为 `items` + `map`，`map` 只有 `title` `link` `date` `desc` `image` 五个键（§6.6.2）——不是 `list`，别照搬 `search`。
- 用 `xpath:` 需 `selectorLevel.xpath` 或 `features: ["xpath"]`（§9.1、§9.2）。
- rss 与其它载体共用同一套"取列表 → 取条目 → 取正文"心智，不另设引擎（§2.3 注）；不存在的命名空间（如 `atom:`）不要写。

用到的字段：`type: "rss"` `out.feed.items/map` `selectorLevel.xpath`（见 REFERENCE.md §1–§3 §9–§12）

## E 宿主与多源（4）

### E1 · `hostStages` 书架
场景：登录后要展示"我的书架"，它不是五阶段之一。
```jsonc
"hostStages": {
  "shelf": {
    "requiresLogin": true,
    "request": { "url": "var:api/shelf", "params": { "page": "var:page" }, "response": "json" },
    "out": { "emptyOk": true, "list": "json:$.data[*]",
             "map": { "name": "json:$.title", "url": { "$": "expr:'/book/' + var:item.id" } } },
    "paginate": { "kind": "page", "max": 20 }
  }
}
```
- `hostStages` 的键由源声明、宿主实现；宿主不支持的阶段 → **`E_HOST_STAGE`**（不是 `E_UNSUPPORTED`），**只禁用该入口，不影响五阶段**（§6.8）。
- 扩展阶段复用同一套 `request` / `out` / `paginate` / `script` 机制，不引入新语法。
- `requiresLogin: true` ⇒ 未登录直接 `E_LOGIN_REQUIRED`，不必自己写 `login.check`。
- 评论、购买、作者页、推荐都放这里，不要伪装成 `images` 或 `content` 里的字段（§6.6.3、§16）。

用到的字段：`hostStages.<name>.requiresLogin/request/out/paginate`（见 REFERENCE.md §9–§12）

### E2 · `merge` 多源合并
场景：三个源都搜到《斗破苍穹》，宿主要折成一条并显示"3 个来源"。
```jsonc
"merge": { "key": ["name", "author"],
           "normalize": ["trim", "t2s"],
           "crossBucket": true,
           "order": ["exact", "contains", "weight", "sources"],
           "weight": 1.0,
           "minScore": 0 }
```
- `merge` 是**宿主执行、源声明**（§12.4）：宿主可以忽略，但一旦声明就必须按此语义执行，`rsl test` 的合并断言也以默认值为准。
- 分档按 `exact`（`key` 全字段相等）→ `contains`（互相包含）→ 其余 `other`；同档内折叠，`origins` 累加、`weight` 取最大。
- `crossBucket: true`（默认）强制**跨档也比较 `key`**——这是相对 Legado"4 桶不跨桶比较"的关键修正，关掉就会出现同一本书两条。
- `score = 档位权重 × 1 + log2(1 + sources) + weight`，低于 `minScore` 丢弃；排序先按 `order` 档位，再 `sources` 降序，再 `weight` 降序。
- `merge.key` 的字段必须存在于 `search.out.map`，否则 lint 18 → `E_REF`。

用到的字段：`merge.key/normalize/crossBucket/order/weight/minScore`（见 REFERENCE.md §14）

### E3 · WebView（`engine: "auto"` + `autoUpgradeIf`）
场景：90% 的页面 HTTP 就够，只有骨架屏那批要渲染。
```jsonc
"request": { "url": "var:chapter.url", "response": "html",
             "engine": "auto",
             "autoUpgradeIf": { "$": "css:#app > .skeleton", "exists": true },
             "webJs": "lib:extractDom" }
```
- `engine` ∈ `http`（默认，最快）| `webview` | `auto`；`auto` 先 HTTP，命中 `autoUpgradeIf` 才升级**一次**（§9.3）。别为少数站点让所有请求付渲染成本。
- `webJs` 在页面内执行且**必须返回非空值**，否则 `E_SCRIPT`（防"空返回导致死循环"，§4.8）；`lib:extractDom` 是 `lib` 里的函数名。
- 需 `features: ["webview"]`，缺失 → `E_UNSUPPORTED`，错误信息必须点名缺哪个 feature（§9.1）。
- WebView 是**并列的传输方式**，规则层（选择器 / 管道 / 契约）与 HTTP 完全一致——不要为一个阶段写两套规则。

用到的字段：`request.engine/autoUpgradeIf/webJs` `requires.features["webview"]`（见 REFERENCE.md §6 §15–§17）

### E4 · `script.onResponse` 改写 HTML
场景：站点返回的 HTML 被包在注释里或标签未闭合，必须在**解析之前**修好。
```jsonc
"lib": { "ctx": "function fixHtml(ctx, body){ return String(body).replace(/<!--\\s*\\/?\\s*wrapper\\s*-->/g, ''); }" },
"stages": { "toc": {
  "request": { "url": "var:book.tocUrl", "response": "html" },
  "script": { "onResponse": { "fn": "lib:fixHtml", "ctx": true } },
  "out": { "list": "css:#list li", "map": { "title": "css:a", "url": "css:a" } }
} }
```
- 时机是**响应解码后、`parse` 前**；输入 `ctx` + `body`，输出 `str` | `bytes`，能改的是原始响应内容（§7.5）。
- 返回值类型必须与输入一致（`str` 进 `str` 出，`bytes` 进 `bytes` 出），否则 `E_TYPE`。
- `onResponse` 里**禁止调用 `ctx.http`**（会形成请求递归）→ `E_SCRIPT`（lint 16）。
- 三个钩子分工：`before` 改请求、`onResponse` 改原始响应、`after` 改已解析的 `out`（§7.5）；`script` 出现在逐章规则里 → `E_COST`。

用到的字段：`script.onResponse` `lib.ctx` `ctx: true`（见 REFERENCE.md §13）

## F 收尾（2）

### F1 · `rsl test` 写回归用例（YAML）
场景：站点改版是常态，用固定 fixture 把"正常"和"改版"两条都钉住。
```yaml
source: example.com
cases:
  - name: search 正常
    stage: search
    level: L3                    # L0 静态 / L1 编译 / L2 模拟 / L3 真实 HTTP / L4 WebView / L5 真机
    vars: { kw: 斗破, page: 1 }
    fixture: fixtures/search.html
    assert:
      - count("books") == 20
      - "books[0].name" == "斗破苍穹"
      - "books[0].url =~ ^https://example\\.com/book/\\d+$"
  - name: 站点改版后应报错
    stage: toc
    fixture: fixtures/toc_v2.html
    expect_error: E_NO_MATCH
    at: stages.toc.out.list
  - name: 多源合并去重
    merge: fixtures/merge/*.json
    assert:
      - merged[0].sources == 3
      - count(merged) == 12
```
- `expect_error` + `at` 是改版回归的标准姿势：不仅断言会错，还断言错在哪个路径，避免"错得对"。
- `level` 是**证据等级**，低等级通过不得写成高等级可用（§13.2）；`L2` 用 fixture，别拿 `L3` 的偶发通过当承诺。
- fixture 用 `rsl record <src> <stage>` 录制（§14），别手抄 HTML——手抄的那份永远和线上不一样。
- 多源合并断言以 `merge` 契约默认值为准（§12.4）；`rsl lint` 的 20 条规则都要能被单测覆盖（§13.3）。

用到的字段：`source` `cases[].stage/level/vars/fixture/assert/expect_error/at/merge`（见 REFERENCE.md §15–§17）

### F2 · 常见踩坑 15 条
场景：一张"错误写法 → 正确写法"对照表，写源前扫一眼。

```jsonc
// 综合修正版：把上面 15 条的"正确写法"合成一个小骨架
"search": {
  "request": { "url": "/search", "params": { "q": "var:kw", "p": "var:page" },
               "charset": "gbk", "encode": { "params": "gbk" } },
  "out": { "emptyOk": true, "list": "css:ul > li",
           "map": { "name": "css:a", "url": { "$": "css:a", "attr": "href", "transform": [{ "absolute": true }] } } },
  "paginate": { "kind": "page", "pageVar": "page", "max": 20, "stopIf": { "empty": "out.list", "noNew": true } }
}
```

| # | 错误写法 | 正确写法 | 后果 / 码 |
|---|---|---|---|
| 1 | `"list": "ul.result > li"` | `"list": "css:ul.result > li"` | `E_SELECTOR`（§3.2） |
| 2 | 同一条规则里 `"range"` + `"pick"` | 只留一个；排除用 `"exclude"` | `E_SCHEMA`（§3.1） |
| 3 | 同一条规则里 `"blocks"` + `"paragraphs"` | 纯文本走 `paragraphs`，图文混排才 `blocks` | `E_SCHEMA`（lint 5） |
| 4 | `"pageVar": "page"` + `"enumerate": […]` 同阶段共存 | `pageVar` 与 `enumerate` 互斥，二选一 | `E_SCHEMA`（lint 7） |
| 5 | `"direct": true` + `"request": {"url": …}` | 要直取就写 `{"request":{"skip":true}}` 或省略 `request` | `E_SCHEMA`（lint 8） |
| 6 | `login` 里同时写 `steps` 与 `fields`/`submit` | 单步写 `fields`+`submit`，多步写 `steps` | `E_SCHEMA`（lint 9） |
| 7 | 末步 `"next": {"to": "done"}` 而 `done` 不存在 | 末步省略 `next`，即结束 | `E_REF`（§4.5.2） |
| 8 | 对 `node[]` 直接跑 `{"regex": …}` | 先 `{"$": "text"}` 转字符串 | `E_TYPE`（§3.6） |
| 9 | 只写 `"charset": "gbk"` | 补 `"encode": {"params": "gbk"}` | 关键词乱码（§4.3） |
| 10 | `toc.out.map` 里放 `expr:now()` / `expr:rand()` | 挪到阶段 `let`；逐章规则禁不确定性 | `E_COST`（§6.5.1） |
| 11 | `toc.out.map` 里放 `cookie:` / `token:` / `script` | 挪到阶段级 `request.headers` / `let` / `script` | `E_COST`（§6.5.1） |
| 12 | `"url": "/book/1"` 原样输出相对路径 | `{"$":"css:a","attr":"href","transform":[{"absolute":true}]}` | 宿主回访不到（§3.6） |
| 13 | 列表为空就当成失败 | `"emptyOk": true`，返回 `[]` | `E_EMPTY`（info，§3.8） |
| 14 | 分页没终止条件，UI 无限触底 | `"stopIf": {"empty": "out.list", "noNew": true}` | 无限翻页（§4.7） |
| 15 | 直接 `replace` 清洗带 `<usehtml>` 的正文 | 先 `"protect": ["<usehtml>", "</usehtml>"]` | `E_PROTECT_LOST`（§3.6.2） |

- 前 11 条是**编译期**错误（`rsl lint` 不联网就能全抓）；12–15 是运行期，靠 `rsl test` 的 fixture 兜住。
- 目录类错误的统一修法是"把计算挪到 `list` 选择器的父规则里一次算完"，而不是放宽预算（§6.5.1）。
- 报错时把 `path` / `rule` / `hint` 照抄进 issue，比贴整份源有用（§11）。

用到的字段：全部（见 REFERENCE.md §15–§17；逐条修法见 `ERRORS.md`）

## 相关文档

| 文档 | 什么时候看 |
|---|---|
| `RSL-1.0.md` | 唯一权威：合法 / 不合法 / 引擎必须怎么做 |
| `REFERENCE.md` | 查字段、查枚举、查默认值 |
| `ERRORS.md` | 每个错误码的成因与修法 |
| `EXPRESSIONS.md` | `expr:` 的函数表与限制 |
| `MIGRATION.md` | 从 Legado 源翻译 |
| `examples/*.rsl.json` | 完整可跑的五个示例源 |
