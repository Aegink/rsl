---
id: rsl-language
type: module-design
status: draft
title: RSL 语言契约（规范正文）
parent: rsl-arch
tags: [rsl, spec, language]
---

# RSL — Reader Source Language

## 阅读源描述语言 · 标准规范 v2.0

| | |
|---|---|
| 状态 | Draft（可评审；本轮为整体重写，非增量修订） |
| 版本 | 2.0.0-draft.1 |
| 日期 | 2026-11-21 |
| 参考实现 | `rsl-core`（Rust，编译到 iOS / Android / WASM / CLI） |
| 配套 | `CONCEPTS.md`（先读这个）、`REFERENCE.md`（字段速查）、`PATTERNS.md`（任务配方）、`ERRORS.md`（错误全书）、`MIGRATION.md`（Legado 翻译）、`COMPILER.md`、`EXPRESSIONS.md` |

**这份文档是什么**：契约。只写"合法 / 不合法 / 引擎必须怎么做"，不写教学。
**想知道某个任务怎么写** → `PATTERNS.md`。**想查字段** → `REFERENCE.md`。**报错了** → `ERRORS.md`。

### v2.0 相对 v1.1 的破坏性变更

| 变更 | 原因 |
|---|---|
| `rsl` 升到 `"2.0"` | `login` 从"单步表单"改为"多步序列"，`fields` 由必填变可选 |
| `login.fields`/`submit` → `login.steps[]` | 真实站点普遍是"账号密码 → 验证码 → 短信码" |
| 新增 `merge` 顶层块 | 多源合并语义此前完全空白 |
| 新增 `script.onResponse` | `script.after` 此前未定义作用对象，导致响应改写能力不可移植 |

---

## 0. 定位

### 0.1 与 Legado 的关系

**RSL 不兼容 Legado，也不打算兼容。** 兼容意味着永久背负"前缀猜测 + 私有方言 + Java 对象注入"。存量源通过一次性转换器迁移（`rsl convert`），转换器是工具，不是内核依赖。

| 维度 | Legado | RSL |
|---|---|---|
| 选择器 | 8 级前缀猜测（`/`→XPath、`$.`→JSONPath、`@@`→去前缀） | 显式命名空间，无默认解释 |
| 正则 | 出现 `##` 即切换整条链语义 | 独立 `re:` + `transform.regex` |
| 索引/切片 | 两套私有方言 `[!2][0:10]` | `range` / `pick` / `exclude` 独立字段 |
| 请求 | URL 尾部拼 `,{...}` JSON | 结构化 `request` 对象 |
| 阶段通信 | 可变 `Book` 字段 + `put()` 全局 map | `out` 契约 + 显式注入 |
| 脚本 | 全字段可写 JS，作用域塞 `java`（93 处注入） | 两档 `lib` + 三个钩子 + 受控 `ctx` |
| 登录 | `loginUi` 5 种控件 + `reLoginView` 增量重绘 | `login.steps[]` 数据声明，UI 归宿主 |
| 多源合并 | 4 桶 + `origins.size`，**跨桶不比较** | `merge` 契约，**强制全局比较** |
| 失败 | 静默空串 | 结构化错误码 + 路径 + 建议 |
| 逐章成本 | 靠经验避免 | **编译期禁止**（`E_COST`） |

完整逐层对比（58 条差异、源码行号）见 `KERNEL-COMPARE.md`。

### 0.2 设计公理

1. **规则是数据。** 可 diff、可 lint、可回归、可被程序断言。
2. **没有隐式通道。** 阶段之间只通过 `out` 契约传递；不存在"第一个结果自动变成 `var:book`"。
3. **失败必须可区分。** "确实为空""可降级""失败"是三种结果，三种错误码。
4. **跨端一致优先于方便。** 任何依赖宿主语言运行时的能力都不得进标准。
5. **能力按需付费。** 90% 的源不需要登录/WebView/媒体解密；它们必须是可选块，不出现就不占认知。
6. **凡是被证明有用的工程优化，必须进标准。** 详情→目录响应复用、占位保护、页码枚举——这些不是"实现细节"，是作者会反复重新发明的东西。

---

## 1. 术语

| 术语 | 定义 |
|---|---|
| **源（Source）** | 一个 RSL JSON 文档，描述如何从**一个站点**取书 |
| **阶段（Stage）** | 数据获取环节。固定五个：`search` `explore` `detail` `toc` `content`；另有宿主扩展 `hostStages` |
| **选择器（Selector）** | `命名空间:表达式` 形式的取值指令，如 `css:.title` |
| **管道（Pipeline）** | 选择器之后有序的纯函数变换列表 |
| **契约（Contract）** | 阶段 `out` 声明的输出结构，含保留字段表 |
| **上下文（Context）** | 由契约与宿主注入的只读变量（`var:` 命名空间） |
| **会话（Session）** | 单次运行内可读写的临时状态（`session:`） |
| **预算（Budget）** | 单次执行的资源上限（字节/节点/时长/步数/并发） |
| **逐章规则（Per-item rule）** | 对列表每一项各求值一次的规则，成本乘以项数 |
| **折叠（Merge）** | 多源搜索结果按 `merge.key` 合并为一条，累加来源 |

---

# 第一部分 · 数据模型

## 2. 文档结构

### 2.1 顶层块

```jsonc
{
  "rsl": "2.0",                      // 必填，MAJOR.MINOR
  "id": "example.com",               // 必填，唯一标识，建议域名
  "name": "示例站",                   // 必填，显示名
  "type": "novel",                   // 必填，源级默认载体，§2.3
  "baseUrl": "https://example.com",  // 必填，相对地址基准
  "match": { "detail": "re:^https://example\\.com/book/\\d+" },  // 可选，URL 归属识别
  "lang": "zh",                      // 可选
  "version": "2.0.0",                // 可选，源自身版本

  "requires":   { ... },   // §9.1  能力声明
  "http":       { ... },   // §4.1  全局请求
  "vars":       { ... },   // §5.2  源配置
  "cookie":     { ... },   // §4.6  显式初始 Cookie（一般不用，登录产出的 Cookie 走 token 商店）
  "cache":      { ... },   // §4.6
  "media":      { ... },   // §4.6
  "login":      { ... },   // §4.5
  "merge":      { ... },   // §12.4 多源合并（宿主执行，源声明）
  "lib":        { ... },   // §7.1
  "stages":     { ... },   // §6
  "hostStages": { ... },   // §6.8
  "limits":     { ... },   // §8.1
  "meta":       { ... }    // §10
}
```

### 2.2 版本协商

- `rsl` 格式 `^\d+\.\d+$`。
- **MAJOR 不等** → `E_VERSION_UNSUPPORTED`，明确提示所需版本；这不是语法错误。
- **同 MAJOR 内的未知字段** → 忽略并记 `W_UNKNOWN_FIELD`，**绝不拒绝加载**。Schema 根级必须 `additionalProperties: true`，否则"新增字段可忽略"的承诺立刻失效。
- 私有扩展字段以 `x-` 开头，可出现在**任意层级**（根 / 阶段 / 选择器 / 管道步骤）。

### 2.3 载体类型

源级 `type` 只是默认值。**条目级 `type` 优先**——一个源里不同书可能是不同载体（聚合源常态）。

```jsonc
"type": "novel",                                  // 源级默认
"stages": { "search": { "out": { "list": "json:$.data[*]", "map": {
  "name": "json:$.title",
  "type": { "$": "json:$.media_kind",             // 条目级覆盖
            "typeMap": { "1": "comic", "2": "audio", "4": "video" } }
}}}}
```

枚举：`novel` `comic` `audio` `video` `file` `rss`。

**映射链（唯一真源）**：`源级 type` → `条目 type`（可覆盖）→ `宿主渲染类型`。宿主渲染类型由宿主自行映射，源不参与；源里不得出现宿主枚举值。这条规则来自现网"`module` / `tab_type` / `book.type` 三套枚举互串"的历史事故。

> Legado 的 rss 是**独立实体 `RssSource`**，不在 `BookSourceType` 里；RSL 把 rss 并进载体枚举，是**故意不同**——同一种"取列表 → 取条目 → 取正文"的心智模型，不该分两套引擎。

---

# 第二部分 · 取值

## 3. 选择器

### 3.1 通用形式

选择器可以是**字符串**（快捷形式）或**对象**（完整形式）：

```jsonc
"css:.title"                              // 快捷
{ "$": "css:.title", "attr": "href" }     // 完整
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `$` | string | 必需的底层选择器；省略时等价 `"text"` |
| `attr` | string | 取属性值（等价 `attr:<name>` 命名空间） |
| `range` | string | **数组**切片：`"0:10"` `"2:"` `"-5:"` `"-1:0:-1"` `"0:20:2"` |
| `pick` | int[] | 按下标取值，支持负索引 |
| `exclude` | int[] | 排除下标，支持负索引 |
| `exists` | bool | 只判命中，返回布尔 |
| `where` | object | 条件过滤（§3.5） |
| `map` | object | 对每项执行子结构提取（§6.4） |
| `blocks` | object | 拆成「文本/图片」混合数组（§6.6） |
| `paragraphs` | `{split?: "br"\|"block"\|"line"\|"auto", minLength?: int}` | 拆成纯文本段落数组；缺省 `split: "auto"`、`minLength: 1` |
| `strip` | Selector[] | 先删除匹配子节点（正文去广告） |
| `protect` | string[] \| Selector | 管道执行期间保护匹配片段（§3.6.2） |
| `transform` | array | 管道（§3.6） |
| `fallback` | Selector[] | 依次尝试（§3.7） |
| `flags` | string | 正则 flag：`m` `s` `i`（仅 `re:` 有效） |
| `typeMap` | object | 取值 → 载体的映射表（仅 `type` 字段有效） |
| `optional` | bool | 缺失不报错，默认 `false` |
| `default` | any | 缺失时的默认值 |
| `limit` | int | 命中项上限 |

**`range` 语法**：`start:end[:step]`，语义与 Python 切片一致（`end` 不含）。
负索引从末尾计；`step` 为负时反向遍历，此时 `start` 应大于 `end`：

```jsonc
{"$": "css:li", "range": "0:10"}       // 前 10 项
{"$": "css:li", "range": "-5:"}        // 末 5 项
{"$": "css:li", "range": "-1:0:-1"}    // 全量倒序（对应 Legado `[-1:0]`）
{"$": "css:li", "range": "0:20:2"}     // 隔一取一
```

**互斥规则**（编译期强制，违反 → `E_SCHEMA`）：

- `range` 与 `pick` 互斥；`exclude` 可与二者之一共存（先取子集再排除）。
- `blocks` 与 `paragraphs` 互斥。
- `exists: true` 时其余取值字段无意义，忽略。
- `typeMap` 只允许出现在契约保留字段 `type` 上。
- `protect` 出现在选择器层时，作用于整个 `transform`；出现在 `transform` 内时，从该步开始生效。
- `absolute` 是**选择器层字段**（等价于管道第一步 `{"absolute": true}`），与 `transform` 里的同名步骤语义一致；两者同给时以管道内的为准。
- 选择器对象顶层可直接写**条件字段** `eq` `ne` `gt` `lt` `exists` `contains`，语义与 `where` 的运算符相同，用于 `ok` / `when` 这类断言位置；条件字段与取值字段同时出现时，先取值再判断。

### 3.2 命名空间

**不存在"无前缀默认解释"。** 这是 RSL 相对 Legado 最重要的改进：`/` 既可能是路径也可能是正则的一部分，显式前缀让编译器能在**加载时**判语义。

| 命名空间 | 语法 | 输入作用域 |
|---|---|---|
| `css:` | `css:div[itemprop=name]` | HTML 节点 |
| `xpath:` | `xpath://div[@id="list"]//a` | HTML 节点 |
| `json:` | `json:$.data.list[*]` | JSON 值 |
| `re:` | `re:第(\d+)章\s*(.+)` | 字符串 |
| `text` | `text` | 节点 → 直接文本（含子节点文本） |
| `owntext` | `owntext` | 节点 → 仅自身文本节点 |
| `html` | `html` | 节点 → innerHTML |
| `tag` | `tag` | 节点 → 标签名 |
| `attr:<name>` | `attr:href` | 节点 → 属性值 |
| `const:<json>` | `const:"固定值"` | 常量；按 JSON 解析，失败当字符串 |
| `var:<path>` | `var:book.tocUrl` | 上下文变量（§5.4） |
| `let:<name>` | `let:sign` | 阶段内 `let` 中间值 |
| `session:<key>` | `session:token` | 会话变量（§5.5） |
| `token:<name>` | `token:auth` | 登录产出令牌 |
| `cookie:<name>` | `cookie:sessionid` | Cookie 值（只读，§4.5） |
| `cache:<key>` | `cache:apiToken` | 读 `cache` 块的派生结果（只读，§4.6） |
| `lib:<fn>` | `lib:cleanTitle` | 调用 `lib.pure` 纯函数 |
| `expr:<表达式>` | `expr:page - 1` | 受限表达式（§4.4） |

### 3.3 作用域与解析顺序

- 阶段入口的作用域是**响应体**：HTML → Document 根；JSON → 解析后的值。若声明了 `parse`，作用域是 `parse` 的输出。
- `map` / `where` 内部的作用域是**单个匹配项**。
- 字符串规则等价于 `{ "$": "<该字符串>" }`。
- 多命中按文档顺序返回；`attr` / `text` 作用于每个命中项（输出 `str[]`）。
- 节点输入上直接套字符串型步骤（如 `regex`）是类型错误 → `E_TYPE`。

### 3.4 正则的跨实现一致性

`re:` 限定为 **RE2 子集**，保证 Rust / Swift / JS / Kotlin 行为一致：

- 支持：字符类、量词、分组、命名分组、非捕获组 `(?:)`、锚点、`\d\w\s\b`、惰性量词。
- **不支持**（`E_REGEX_UNSUPPORTED`）：反向引用 `\1`、前后向断言 `(?=)` `(?<=)`、递归。
- 需要"第 n 个匹配"用 `transform.match`；需要反向引用用 `transform.replace` 的 `$1`。
- 默认 `multiLine: false`、`dotAll: false`，可用 `flags` 覆盖。

### 3.5 条件过滤 `where`

独立字段，不是 CSS 伪类——避免方言污染：

```jsonc
{ "$": "css:#list li", "where": { "has": "css:a[href]" } }
{ "$": "css:#list li", "where": { "not": { "has": "css:.vip" } } }
{ "$": "css:tr.price", "where": { "text": "re:\\d+", "ne": "0" } }
```

运算符：`has`（含匹配子元素）、`not`、`eq` `ne` `gt` `lt`（比较当前取值）、`text`（取文本后匹配）。
多键同时存在 = **AND**；需要 OR 用 `fallback`。

### 3.6 管道 `transform`

每一步 `T -> T`，输入输出为 `str | str[] | node | node[] | json | bytes`。

| 步骤 | 参数 | 类型 |
|---|---|---|
| `trim` | `true` 或 `["\u00a0"]` | `Str\|Node` → 同 |
| `replace` | `[[from,to],...]`，`from` 可带 `re:` 前缀 | 同上 |
| `regex` | `{"regex":"…","replace":"$1","all":false}` | `Str` → `Str` |
| `match` | `{"regex":"…","n":2}` | `Str` → `Str` |
| `prepend` / `append` | string | `Str` → `Str` |
| `absolute` | `true` 或 `{"base":"var:baseUrl"}` | `Str` → `Str` |
| `urlencode` / `urldecode` | `{"charset":"gbk"}` | `Str` → `Str` |
| `base64` | `"encode"` / `"decode"` | `Str\|Bytes` → 同 |
| `hex` | `"encode"` / `"decode"` | 同上 |
| `crypto` | `{"alg":"aes-cbc","mode":"decrypt","key":"var:k","iv":"var:iv"}` | `Str\|Bytes` → 同 |
| `font` | `{"map":"lib:glyph","ref":"var:normalFont"}` | `Str` → `Str` |
| `html` | `true` | `Str` → `Str`（反转义实体） |
| `json` | `true` | `Str` → `Json` |
| `date` | `{"from":"…","to":"yyyy-MM-dd","tz":8}` | `Str` → `Str` |
| `number` | `true` 或 `{"unit":true}` | `Str` → `Num` |
| `numChapter` | `true` | `Str` → `Str`（`"第一百二十三章"` → `123`） |
| `cjkNumber` | `"to"` / `"from"` | `Str` → `Str` |
| `t2s` / `s2t` | `true` | `Str` → `Str` |
| `slice` | string（**字符串**切片，与选择器 `range` 区分） | `Str` → `Str` |
| `charset` | `"gbk"` | `Bytes` → `Str` |
| `join` | string | `Str[]` → `Str` |
| `split` | string | `Str` → `Str[]` |
| `zip` | `{"with":"css:.price","sep":" | "}` | `Str[]` → `Str[]` |
| `protect` | `["<usehtml>"]` 或 Selector | 任意 → 同（§3.6.2） |
| `unique` / `sort` / `dropEmpty` | — | 列表操作 |
| `js` | `"lib:fn"` 或 `{"fn":"lib:fn","ctx":true}` | 任意 → 任意 |

**类型检查**：编译期已知输入是 `node[]` 而步骤要求 `Str`（如 `regex`）→ `E_TYPE`，并建议先加 `{"$":"text"}`。

#### 3.6.1 `zip`：交叉合并

对应 Legado 的 `%%`。按下标把两个列表配成一条：

```jsonc
{ "$": "css:#list li", "transform": [
  { "zip": { "with": "css:.price", "sep": " · " } }
]}
// ["书A", "书B"] zip ["12元", "8元"] → ["书A · 12元", "书B · 8元"]
```

规则：`with` 在该步骤的**父作用域**求值（不是当前项）；长度不等时以短者为准，多余项丢弃并记 `W_ZIP_LENGTH`。

#### 3.6.2 `protect`：占位保护

有些片段**不允许被后续规则碰到**——典型是站点自带的 `<usehtml>` 标记（里面是可以直接当 HTML 渲染的内容）。如果直接跑正则替换/净化，标记会被吃掉。

```jsonc
{ "$": "css:#content", "protect": ["<usehtml>", "</usehtml>"], "transform": [
  { "replace": [["广告", ""]] }
]}
```

语义：管道入口把命中片段替换成不可猜的占位符（`\u0000p0\u0000`），管道出口原样还原。占位符不参与任何匹配、不计入长度、不进入 `slice`/`regex` 的输入。

也接受选择器形式，用于"整块节点保护"：

```jsonc
{ "$": "css:#content", "protect": { "$": "css:pre.code" } }
```

保护失败（占位符在管道中被误删）→ `E_PROTECT_LOST`，附带原占位符位置。**绝不静默丢弃原文。**

### 3.7 回退 `fallback`

```jsonc
"name": { "fallback": ["css:h1.title", "css:.book-name", "json:$.title"] }
```

按顺序尝试，第一个**非空且无错误**的结果生效。全部失败 → `optional:true` 则取 `default`，否则 `E_NO_MATCH`。

### 3.8 空结果与校验

列表型契约里，"接口正常返回但没有内容"是合法结果，不是错误：

```jsonc
"out": {
  "emptyOk": true,                       // 空列表合法 → 返回 []，不报错
  "verify": { "contains": "var:kw" },    // 结果必须含关键词，否则 E_VERIFY
  "list": "json:$.data[*]",
  "map": { ... }
}
```

- `emptyOk: false`（默认）：列表为空 → `E_EMPTY`（info 级，非错误级）。
- `verify`：对结果做完整性校验，用于识别"HTTP 200 但返回了错误页/别的内容"。

---

# 第三部分 · 取数

## 4. 请求层

### 4.1 `http` 全局块

```jsonc
"http": {
  "headers": { "User-Agent": "Mozilla/5.0 …", "Referer": "var:baseUrl" },
  "timeout": 15000,
  "retry": { "times": 2, "backoff": "exponential", "on": [429, 500, 502, 503, 504] },
  "rate": { "windowMs": 60000, "max": 20 },      // 或简写 "rps": 2
  "redirect": { "max": 5 },
  "cookies": "session",                          // none | session | persistent
  "proxy": "none",                               // none | system | "socks5://…"
  "dns": { "example.com": "1.2.3.4" },           // 可选，域名固定解析
  "tls": { "fingerprint": "chrome" }             // 可选，需 features: ["tls"]
}
```

`retry` 与 Legado 的差别（重要）：Legado 只在**非 2xx** 时重试、**无退避**、次数来自 URL 选项。RSL 的 `on` 显式列出重试状态码，`backoff` 支持 `none | linear | exponential`，且连接失败（`E_NET`）也计入重试。
**字符串插值 `${...}`**：`url`、`params` 的值、`headers` 的值、`body.data` 的值都支持在普通字符串里写 `${ns:expr}`，按 §3.2 的命名空间求值后转成字符串拼接。它只是**拼接**，不能执行语句、不能有副作用：

插值里的错误按**原因**分流，不新造语义：模板本身坏了（`${` 未闭合、`${}` 空、未知命名空间、嵌套）→ `E_TEMPLATE`；引用的变量未定义 → `E_REF`；`expr:` 写错 → `E_EXPR_SYNTAX`。

```jsonc
"headers": { "Authorization": "Bearer ${token:auth}" },
"url":     "https://example.com/list/${expr:page}.html"
```

等价于 `"Bearer " + token:auth`。整个字符串就是 `${...}` 时结果保留原类型；混在文本里时按 `str()` 转换。

### 4.2 阶段 `request`

```jsonc
"request": {
  "url": "https://example.com/search",
  "method": "GET",                        // GET | POST | PUT | DELETE | HEAD
  "params": { "q": "var:kw", "pn": "expr:page - 1" },
  "body": { "type": "form", "data": { "k": "var:kw" } },   // form | json | raw | multipart
  "headers": { "X-Requested-With": "XMLHttpRequest" },
  "charset": "gbk",                       // 响应解码，§4.3
  "encode": { "params": "gbk", "body": "gbk" },            // 请求编码，§4.3
  "response": "html",                     // html | json | text | bytes
  "engine": "auto",                       // http | webview | auto，§9.3
  "webJs": "lib:extractDom",              // WebView 内执行的 JS（须有返回值）
  "sniff": "re:\\.(m3u8|mp4)(\\?|$)",     // 资源嗅探正则
  "autoUpgradeIf": { "$": "css:#app > .skeleton", "exists": true },
  "skip": false,                          // true = 不发起请求（§4.2.1）
  "retry": { "times": 1 },                // 覆盖全局
  "timeout": 20000
}
```

#### 4.2.1 不发起请求的阶段

三种情况不会真的请求 `baseUrl`，但只有第一种必须显式 `skip: true`：

```jsonc
// ① 数据由 lib.ctx 脚本自行取回
"content": { "request": { "skip": true }, "script": { "onResponse": "lib:fetch" },
             "out": { "content": { "$": "var:scriptResult" } } }

// ② 正文就是章节 URL 本身（纯图片站、直链站）—— 见 §6.6.4
"content": { "request": { "skip": true }, "direct": true,
             "out": { "content": { "$": "var:chapter.url" } } }

// ③ 整个阶段的数据来自上一阶段的复用 —— 见 §6.5.2
"toc": { "reuseDetailHtml": true, "out": { ... } }
```

### 4.3 编码：解码与编码是两个字段

历史上这两个语义被同一个 `charset` 承担，导致 GBK 站的搜索关键词永远乱码。RSL 分开：

| 字段 | 方向 | 典型值 |
|---|---|---|
| `charset` | **响应**字节 → 字符串 | `gbk`、`big5`、`utf-8` |
| `encode.params` | 请求参数 → 字节 | `gbk` |
| `encode.body` | 请求体 → 字节 | `gbk` |

响应解码的判定顺序见 §6.7。

### 4.4 表达式 `expr:`

唯一允许算术的位置。完整语法见 `EXPRESSIONS.md`，摘要：

| 类别 | 支持 |
|---|---|
| 字面量 | 整数、浮点、字符串、布尔、null |
| 运算符 | `+ - * / %`、`== != < <= > >=`、`&& \|\| !`、三元 |
| 纯函数 | `min max abs round ceil floor len upper lower trim replace split join contains num str` |
| 需声明 | `now()`→`time`；`md5/sha1/sha256/hmac/hex/base64/urlEncode`→`crypto`；`rand()`→`random` |
| 变量 | 当前阶段可见变量（编译期查引用图，未定义 → `E_REF`） |
| 禁止 | 赋值、自定义函数、正则字面量、属性访问、任意 JS |

```jsonc
"params": {
  "pn":   "expr:page - 1",
  "size": "expr:min(page * 20, 100)",
  "sign": "expr:md5(var:kw + str(let:ts) + 'salt')"   // 需 requires.crypto: ["md5"]
}
```

### 4.5 登录与登录态

#### 4.5.1 单步登录（最常见）

```jsonc
"login": {
  "type": "form",                                  // form | cookie | token | qr
  "fields": [
    { "key": "username", "label": "账号" },
    { "key": "password", "label": "密码", "secret": true }
  ],
  "submit": {
    "request": { "url": "https://example.com/api/login", "method": "POST",
                 "body": { "type": "form", "data": { "u": "var:login.username", "p": "var:login.password" } } },
    "ok":    { "$": "json:$.code", "eq": 0 },
    "token": { "$": "json:$.data.token" },          // 存入 token 商店
    "cookie": { "$": "cookie:sessionid" }           // 可选：显式声明要保存的 cookie
  },
  "check": {
    "request": { "url": "https://example.com/api/me" },
    "ok": { "$": "json:$.uid", "exists": true }
  },
  "headers": { "Authorization": "Bearer ${token:auth}" }
}
```

#### 4.5.2 多步登录（验证码 / 短信码 / 二次验证）

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
      // 条件跳转：需要短信验证就进 sms，否则直接结束
      "next": { "when": { "$": "json:$.need_sms", "eq": true }, "to": "sms" }
    },
    { "id": "sms",
      "fields": [ { "key": "code", "label": "短信验证码", "secret": true } ],
      "submit": { "request": { "url": "/api/verify", "method": "POST",
                               "body": { "type": "json", "data": { "code": "var:login.code" } } },
                  "ok": { "$": "json:$.code", "eq": 0 },
                 "token": { "$": "json:$.token" } }
      // 无 next ⇒ 登录流程结束（不要再写 {"to":"done"}，那会指向不存在的步骤 → E_REF）
    }
  ]
}
```

规则：

- `steps` 与顶层 `fields`/`submit` **互斥**；二者都给 → `E_SCHEMA`。
- 每步的 `next` 只能是 `{"to": "<stepId>"}` 或 `{"when": …, "to": …}`；无 `next` 视为结束。
- `next.to` 指向不存在的 step → `E_REF`。
- 步骤图必须是 DAG（无环），环 → `E_SCHEMA`。
- 每步执行完可产出 `token` / `cookie`，后续步骤与规则可见。
- 宿主渲染每一组 `fields`；步骤之间的推进由引擎驱动，宿主只负责"渲染当前步 + 提交值"。
- `steps[].id` 必须**唯一**且非空；重复 id → `E_SCHEMA`（否则 `next.to` 不可判定）。
- `next.when` 在**该步提交请求成功后**求值，可见变量 = 该步响应的根作用域 + `var:login.*` + `token:*` + `cookie:*`。

> 为什么不做 Legado 的 `reLoginView` 增量重绘：那是"让源用 JS 操作宿主 UI"。RSL 把同一需求表达为**数据**（步骤 + 跳转条件），宿主怎么渲染都行，且编译期能校验步骤图。

#### 4.5.3 登录态

**Cookie 访问（只读）**：规则里用 `cookie:<name>` 选择器或 `expr:cookie('sessionid')`；脚本里用 `ctx.cookie.get/set/remove`。

**登录态隔离键** = `源 id` + 站点域名 + 登录名。多源不串号，同名新源不继承旧源 Cookie，**换账号天然隔离**。

**失效检测**：`check` 在 `requiresLogin` 的阶段执行前跑一次；失败 → `E_LOGIN_REQUIRED`。宿主可配置缓存 `check` 结果，但不得超过 `limits.loginCheckTtlMs`（默认 300000）。

**未登录或失效** → `E_LOGIN_REQUIRED`，宿主据此弹登录 UI。**遇到人机验证** → `E_CHALLENGE`，宿主决定是否接管。

### 4.6 `cache` / `media`

```jsonc
"cache": {
  "apiToken": { "request": { "url": "https://example.com/token" },
                "take": { "$": "json:$.t" }, "ttl": 3600 },
  "richToc":  { "request": { "url": "var:api/rich" },
                "take": { "$": "json:$.data" },
                "key": "var:book.id + let:mode",     // 维度化键，防串书
                "ttl": 604800, "ttlPartial": 300 }   // 部分结果只短缓存
}
```

```jsonc
"media": {
  "headers":  { "Referer": "var:baseUrl" },          // 全局默认
  "decode":   { "in": "bytes", "out": "bytes", "fn": "lib:decryptImage" },
  "cache":    { "ttl": 86400 },
  "maxBytes": 20971520
}
```

`decode` 的 `in`/`out` 是**契约**，不是注释：引擎必须把原始字节交给 `lib:decryptImage` 并把返回字节当作图像数据。

**`cookie` 顶层块**（一般不用，只在需要预置 Cookie 时写）：

| 字段 | 类型 | 说明 |
|---|---|---|
| `initial` | `{name: value}` | 首次运行前写入的 Cookie（不覆盖登录产出的值） |
| `persist` | bool | 是否写入持久存储，默认 `true` |
| `domain` | string | 覆盖 Cookie 归属域；缺省用 `baseUrl` 的 eTLD+1 |

登录产出的 Cookie 不进这个块——它由 `login.steps[].submit.cookie` 声明，按登录态隔离键存储（§4.5.3）。

**逐资源请求头**：媒体数组的每一项都可带自己的 `headers`（防盗链几乎必需）：

```jsonc
"out": { "images": [ { "url": "var:u", "headers": { "Referer": "var:baseUrl" } } ] }
```

### 4.7 分页

```jsonc
"paginate": {
  "kind": "page",                  // page | next | cursor | enumerate
  "pageVar": "page",               // kind=page：注入 var:page
  "next": "css:a.next",            // kind=next：选择器 → 下一页 URL
  "cursor": {                      // kind=cursor：从响应取游标
    "take": "json:$.data.next_offset",
    "into": "params.offset"
  },
  "enumerate": [1, 2, 3, 4, 5],    // kind=enumerate：按值重复请求（§4.7.1）
  "into": "params.pn",             // enumerate：每个值注入哪里
  "carry": ["search_id", "passback"],   // 原样透传给后续页的字段
  "max": 50,
  "stopIf": { "empty": "out.list", "noNew": true }
}
```

- `kind: "cursor"` 覆盖"接口给游标"这类站点，`carry` 解决"除 offset 外还要透传会话字段"。
- 四个 kind 都受 `limits.maxPages` 约束。
- 目录分页（`toc`）额外允许 `next` 返回 URL 数组（一次给出全部后续页）。
- **`stopIf.noNew: true`**：一页结果与已收集结果**完全无新增项**时停止。这条专治 Legado 的经典缺陷——源不支持分页时 `hasMore` 恒真，UI 无限触底。

#### 4.7.1 `kind: "enumerate"`：页码枚举

对应 Legado 的 `<1,2,3>`。某些站点的 URL 要求一次给出多个页码，或者需要用固定的一组页码遍历：

```jsonc
// 写法 A：逐值请求，结果拼接
"paginate": { "kind": "enumerate", "enumerate": [1,2,3,4,5], "into": "params.pn" }

// 写法 B：整组一次性塞进 URL 的一个参数（等价 Legado 的 <1,2,3>）
"paginate": { "kind": "enumerate", "enumerate": {"from": 1, "to": 5},
              "joinWith": ",", "into": "params.pages" }   // → pages=1,2,3,4,5
```

- `enumerate` 可以是字面数组，或 `{"from":1,"to":5[,"step":1]}`。
- 给了 `joinWith` → **只发一次请求**，值拼成字符串注入；不给 → **每个值发一次请求**，结果按值顺序拼接。
- 与 `page` 互斥：同一阶段不能既有 `pageVar` 又有 `enumerate` → `E_SCHEMA`。
- `enumerate` 为 `{from,to,step}` 时：`step` 必须 > 0，`from` 必须 ≤ `to`，否则 `E_SCHEMA`（不做自动纠正，避免静默改义）。
- `into` 必填；指向不存在的参数路径 → `E_REF`。

### 4.8 WebView 与资源嗅探

- `engine: "webview"` 用宿主 WebView 渲染后取 DOM；`engine: "auto"` 先 HTTP，命中 `autoUpgradeIf` 再升级一次。
- `webJs` 在页面内执行，**必须返回非空值**，否则 `E_SCRIPT`（避免历史上"空返回导致死循环"）。
- `sniff` 用于音视频源：从页面网络请求里匹配资源 URL，命中结果注入 `var:sniffed`（`str[]`），规则里用 `var:sniffed` 或 `var:sniffed[0]` 取用。
- WebView 是**并列的传输方式**，规则层与 HTTP 完全一致。缺能力 → `E_UNSUPPORTED`（`features: ["webview"]`）。

---

# 第四部分 · 状态

## 5. 变量与状态

### 5.1 六层状态模型

真实源的绝大多数 bug 来自"状态放错了层"。RSL 把层写死：

| 层 | 命名空间 | 生命周期 | 可写者 | 典型内容 |
|---|---|---|---|---|
| 源配置 | `var:` | 跟随源，用户可编辑 | 用户 / `script` | 线路、模式、开关、手动 Token |
| 登录令牌 | `token:` | 跟随登录态 | `login` | `auth`、`sessionid` |
| 会话 | `session:` | 单次运行 | `script` / `let` | 本次请求的临时值 |
| 缓存 | `cache:` | 按 TTL | `script` / `cache` 块 | 派生结果、索引 |
| 阶段内 | `let:` | 单个阶段 | 阶段 `let` | 签名、时间戳 |
| 上下文 | `var:` | 宿主注入 | 宿主 / 契约 | `book` `chapter` `kw` `page` `group` |

**规则**：用户设置先写源配置层；派生结果只进缓存层；会话层不得冒充持久配置（"重启后开关失效"就是这么来的）。

### 5.2 `vars`：常量与可配置项

```jsonc
"vars": {
  "ua": "Mozilla/5.0 …",                                  // 简写 = 常量
  "line": {                                               // 完整形式 = 可配置
    "value": "https://line1.example.com",
    "label": "线路",
    "options": ["https://line1.example.com", "https://line2.example.com"],
    "persist": true,
    "secret": false
  },
  "manualToken": { "value": "", "label": "手动 Token", "persist": true, "secret": true }
}
```

- `persist: true` → 用户修改后持久化，但**不随源文件导出**。
- `secret: true` → 永不进入日志、导出、错误信息、`--trace` 输出。
- 默认值优先级：用户持久值 > `value`。
- `vars` 的键名**不得**与内置上下文键冲突（`baseUrl` `url` `response` `charset` `kw` `page` `group` `filter` `login` `book` `chapter`），冲突 → 编译期 `E_REF`。

### 5.3 内置变量

| 变量 | 类型 | 可用阶段 |
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
| `var:scriptResult` | any | 有 `script` 钩子的阶段；值为**最后一个执行的钩子**的返回值（`after` > `onResponse` > `before`，后写覆盖前写） |
| `var:sniffed` | `str[]` | 有 `sniff` 的 content 阶段 |

### 5.4 契约注入（显式，无隐式提升）

- **detail 的 `var:book`** = 宿主显式传入的选中条目，其字段由 `search`/`explore` 的 `out.map` 声明。
- **content 的 `var:chapter`** = 宿主显式传入的选中章节，字段由 `toc` 的 `out.map` 声明。
- **引擎不会**"把第一个结果项自动变成 `var:book`"。宿主必须传，源必须声明。

这条规则是硬约束：它让 `var:book.tocUrl` 的拼写错误在**编译期**变成 `E_REF`。

### 5.5 会话

```jsonc
"script": { "before": { "fn": "lib:prepare", "ctx": true } }
```

脚本内：`ctx.session.get(k)` / `ctx.session.set(k, v)`。规则内读：`session:<key>`。

会话不跨运行持久；需要持久就写 `cache` 或源配置。

---

# 第五部分 · 阶段

## 6. 阶段

| 阶段 | 必需性 | 说明 |
|---|---|---|
| `detail` `toc` `content` | 必需 | 取书主链路 |
| `search` | 至少一个入口 | 关键词检索 |
| `explore` | 至少一个入口 | 分类浏览 |

`search` 与 `explore` **至少要有其一**，都没有 → `E_SCHEMA`；未知阶段键 → `E_ENUM`。

### 6.1 通用结构

```jsonc
"search": {
  "let":      { ... },            // 可选：有序中间值（§6.1.1）
  "request":  { ... },            // 可选；缺省用 baseUrl + 阶段默认路径
  "parse":    { ... },            // 可选：预处理响应根（§6.1.2）
  "out":      { ... },            // 契约
  "paginate": { ... },            // 可选
  "script":   { "before": …, "onResponse": …, "after": … },   // 可选钩子（§7.5）
  "onError":  "abort",            // abort | skip | fallbackTo:<stage>
  "timeoutMs": 20000              // 覆盖 limits.timeoutMs
}
```

#### 6.1.1 阶段内 `let`（多步请求）

```jsonc
"let": {
  "ts":    "expr:now()",
  "token": { "request": { "url": "https://example.com/api/token" },
             "take": { "$": "json:$.token" } },
  "docs":  { "parallel": [ { "request": {"url":"var:api/a"} },
                           { "request": {"url":"var:api/b"} } ], "limit": 2 },
  "sign":  "expr:md5(var:kw + let:ts + 'secret')"
}
```

- 按声明顺序求值，后项可引用前项（`let:`）。
- 每项可以是：`expr:` 表达式 / 纯选择器（如 `var:book.*`、`json:…`，`take` 可省略）/ `{request, take}` 单次请求 / `{parallel: [...], limit}` 并发批次 / `lib:` 调用。
- 并发批次受 `limits.maxParallel` 与 `http.rate` 双重约束。
- 循环引用 → 编译期 `E_REF`。

#### 6.1.2 `parse`：预处理响应根

用于"JSON 藏在 `<script>` 里"或"接口外层套了 HTML 注释"这类站点：

```jsonc
"parse": { "$": "css:script#__NEXT_DATA__", "transform": [{ "json": true }] }
```

`parse` 的输出成为该阶段**所有规则的根作用域**；`var:response` 仍保留原始响应。

### 6.2 `search` 契约

| 保留字段 | 类型 | 说明 |
|---|---|---|
| `list` | Selector | 列表选择器（必填） |
| `map.name` | Str | 书名（必填） |
| `map.url` | Str | 详情 URL（必填） |
| `map.author` `map.intro` `map.kind` `map.cover` `map.wordCount` `map.lastChapter` `map.updateTime` | Str | 可选 |
| `map.type` | 载体 | 条目级载体覆盖（§2.3） |
| `emptyOk` | bool | 空列表是否合法 |
| `verify` | object | 结果校验 |

输入：`var:kw`、`var:page`。

### 6.3 `explore` 契约

```jsonc
"explore": {
  "filters": [
    { "key": "mode", "label": "模式", "persist": true, "default": "novel",
      "options": [ {"value":"novel","label":"小说"}, {"value":"comic","label":"漫画"} ] }
  ],
  "groups": [
    { "title": "玄幻", "when": { "mode": "novel" }, "params": { "type": "1" } },
    { "title": "最新", "url": "https://example.com/latest/${expr:page}.html" },
    { "title": "排行", "groups": [
        { "title": "月票", "params": { "type": "3", "sort": "vote" } },
        { "title": "收藏", "params": { "type": "3", "sort": "fav" } }
    ]}
  ],
  "groupsFrom": { "$": "css:.nav a", "map": { "title": "text",
                  "params": { "type": "attr:data-cat" } } },
  "request": { "url": "https://example.com/list", "params": { "p": "var:page" } },
  "out": { "list": "css:.item", "map": { ... } },
  "paginate": { "kind": "page", "max": 20 }
}
```

规则：

- `groups` 的字段**深度合并**到阶段 `request`（group 优先）。
- `when` 按 `filters` 当前值过滤——这是**数据选择声明**，不是控件编程。
- 有二级 `groups` 的项本身不发起请求，只作分组标题。
- `filters` 的值通过 `var:filter.<key>` 读取，可 `persist` 持久化。
- `groupsFrom` 支持从站点动态抓分类。
- `groups` 与 `groupsFrom` 可同时存在：`groupsFrom` 的结果追加在 `groups` 之后。

### 6.4 `detail` 契约

| 保留字段 | 类型 | 说明 |
|---|---|---|
| `name` `author` `intro` `kind` `cover` `wordCount` `lastChapter` `updateTime` | Str | 元数据 |
| `tocUrl` | Str | 目录页地址；缺省回退 `var:book.url` |
| `type` | 载体 | 本书实际载体（覆盖源级） |
| `canRename` | bool | 允许用详情页书名覆盖搜索页 |
| `downloads` | `{name,url,size?}[]` | 文件型源的下载列表 |
| `extra` | object | 宿主展示用的自由字段（不进契约检查） |

`tocUrl` 是本阶段最关键的输出：目录 URL 与详情 URL 不同是常态。

### 6.5 `toc` 契约

```jsonc
"toc": {
  "order": "auto",                 // auto | asc | desc
  "dedupe": "url",                 // url | title | none
  "reuseDetailHtml": false,        // true：复用 detail 的响应（§6.5.2）
  "out": {
    "list": "css:#list li",
    "map": {
      "title":     { "$": "css:a" },
      "url":       { "$": "css:a", "attr": "href", "absolute": true },
      "isVolume":  { "$": "css:.vol", "exists": true },
      "isVip":     { "$": "css:i.vip", "exists": true },
      "isPay":     { "$": "css:i.pay", "exists": true },
      "wordCount": { "$": "css:.wc", "transform": [{"number": true}] },
      "updateTime":{ "$": "css:.t", "transform": [{"date": {"from":"x","to":"yyyy-MM-dd"}}] },
      "cover":     { "$": "css:img", "attr": "src", "optional": true }
    }
  }
}
```

**保留字段的引擎行为**（规范强制，不是约定）：

| 字段 | 行为 |
|---|---|
| `url` | 必需，正文入口 |
| `isVolume: true` | 只作分卷标记，**不请求正文** |
| `isPay: true` | 不请求正文，宿主显示"需购买" |
| `isVip: true` | 标记受限章节，宿主决定提示方式 |
| `cover` | 章节附加图（视频封面等） |
| `order: "auto"` | 比较首尾标题的章节序号自动判正倒序 |

#### 6.5.1 逐章规则的硬约束（`E_COST`）

`toc.out.map.*` 是**逐章求值**的：成本 = 章数 × 字段数 × 每条规则的跨桥次数。实测过 807 章 × 3 字段 ≈ 4800 次跨桥 ≈ 19 秒，直接撞穿宿主预算。

因此**编译期强制**：逐章规则只允许

- 选择器：`css:` `xpath:` `json:` `re:` `text` `owntext` `html` `tag` `attr:` `const:` `var:` `let:` `session:`
- 管道：`trim` `replace` `regex` `match` `prepend` `append` `absolute` `urlencode` `urldecode` `base64` `hex` `html` `json` `date` `number` `numChapter` `cjkNumber` `t2s` `s2t` `slice` `join` `split` `zip` `protect` `font` `unique` `sort` `dropEmpty` `js(lib.pure)`

禁止：`script`、`lib.ctx`、`cache`、`http`、`expr:now()`、`expr:rand()`、`cookie:`、`token:`。
- **`expr:` 允许**，但只能用确定性函数：`now()` / `rand()` / `cookie()` / `token:` 一律 → `E_COST`。

违反 → `E_COST`，并给出"把该字段的计算挪到 `list` 选择器的父规则里一次算完"的提示。

#### 6.5.2 `reuseDetailHtml`：详情与目录同页

大量站点把书籍信息和目录放在同一个页面：`detail` 请求后，`toc` 的目标 URL 就是刚请求过的那个。

```jsonc
"toc": { "reuseDetailHtml": true, "out": { "list": "css:#list dd", "map": { ... } } }
```

语义（规范强制）：

1. 若 `reuseDetailHtml: true` 且本阶段实际 URL 与 `detail` 阶段实际 URL **相同**，则**不发起请求**，直接把 detail 的响应体作为本阶段根作用域。
2. URL 不同则正常请求，行为不变。
3. `detail` 未执行（宿主跳过）时，本项无效，正常请求。
4. 复用不计入 `http.rate` 限流（没有请求发生）。

这条省掉的是**每一次目录刷新的一次请求**——它是真实站点里最常见的优化，不该让每个作者自己想办法。

### 6.6 `content` 契约（六载体）

`type` 决定 `out` 使用哪些保留字段：

| `type` | 保留输出字段 | 说明 |
|---|---|---|
| `novel` | `content`（`Block[]` 或 `Str[]`） | 段落/图文混排 |
| `comic` | `images`（`{url,headers?}[]`） | 图片按序，**不得混入非图片资源** |
| `audio` | `audio`（`{url,duration?,tone?,variants?}`；`url` 是选择器，其余是字面值） | 直链 |
| `video` | `video`（`{url,format,variants?,headers?}`） | mp4 / m3u8 |
| `file` | `files`（`{name,url,size?}[]`） | 下载型 |
| `rss` | `feed`（见 §6.6.2） | 订阅 |

所有载体共用：

| 字段 | 说明 |
|---|---|
| `title` | 章节标题（站点只在正文页给标题时用） |
| `tracks` | 第二轨道（§6.6.1） |
| `nextPage` | 同章续页 URL，引擎自动循环拼接（受 `limits.maxPages`） |
| `payAction` | 需购买时的宿主动作（URL 或 `lib:` 调用） |

#### 6.6.1 `tracks`：弹幕 / 歌词 / 字幕

第二轨道统一为一种结构，不再靠"副文"字段按载体分流：

```jsonc
"tracks": [
  { "kind": "lyric",   "format": "lrc",       "url": "var:lrcUrl" },
  { "kind": "danmaku", "format": "bili-xml",  "text": "var:dmText" },
  { "kind": "subtitle","format": "vtt",       "url": "var:vttUrl" }
]
```

`kind` ∈ `lyric` `danmaku` `subtitle`；`format` ∈ `lrc` `bili-xml` `ass` `vtt` `text`。
`url` 与 `text` 二选一：给 `url` 时引擎拉取内容；给 `text` 时直接用。
轨道失败**不得影响主内容**——返回空轨道，不报错。

#### 6.6.2 `feed`（rss）

```jsonc
"feed": {
  "items": "xpath://item",
  "map": { "title": "xpath:title", "link": "xpath:link",
           "date": "xpath:pubDate", "desc": "xpath:description",
           "image": "xpath:enclosure/@url" }
}
```

#### 6.6.3 载体容器契约

- `comic` 的 `images` 里每一项都会被当作页面资源；混入评论横幅/SVG 会导致整话渲染失败。评论入口必须走 `hostStages` 或宿主按钮，不得伪装成图片。
- `audio` / `video` 的 `url` 必须是播放器可消费的地址；错误 JSON、提示文本不得冒充媒体地址。
- `novel` 的 `content` 允许图文混排（`Block[]`），渲染层据 `type` 决定段落或图片。

#### 6.6.4 `direct`：正文就是 URL

纯图片站、直链站、`chapter.url` 本身就是资源的站点，不需要请求正文页：

```jsonc
"content": {
  "request": { "skip": true },
  "direct": true,
  "out": { "content": { "$": "var:chapter.url" } }
}
```

- `direct: true` ⇒ 引擎**不请求**，`out.content` 直接取 `var:chapter.url`；`request.skip` 可省略。
- 载体为 `comic`/`audio`/`video`/`file` 时，`direct` 把该 URL 当作单个媒体项，自动生成对应数组。
- `direct` 与 `request` 同时给非 `skip` 的配置 → `E_SCHEMA`（不能既请求又直取）。

> 对应 Legado 的"正文规则为空则直接用章节 URL"——但那边是**隐式兜底**（空规则静默改义），这里是**显式声明**。

### 6.7 编码判定顺序（规范强制）

```
1. request.charset（显式声明）
2. HTTP Content-Type 的 charset=
3. <meta charset> / <?xml encoding=?>
4. BOM（UTF-8/16/32）
5. 自动探测（仅当 requires.features 含 "detect"）
6. utf-8（兜底）
```

`response: "bytes"` 时不做解码，`transform.charset` 才有意义。探测失败 → `E_CHARSET`，附带前 64 字节十六进制。

### 6.8 宿主扩展阶段 `hostStages`

内核只调度五个阶段，但书架、历史、推荐、作者页、购买、评论是真实需求。标准用 `hostStages` 给它们契约，而不假装不存在：

```jsonc
"hostStages": {
  "shelf": {
    "requiresLogin": true,
    "request": { "url": "var:api/shelf", "params": { "page": "var:page" } },
    "out": { "emptyOk": true, "list": "json:$.data[*]", "map": { "name": "...", "url": "..." } },
    "paginate": { "kind": "page", "max": 20 }
  }
}
```

- 宿主声明自己支持哪些扩展阶段（`engine.capabilities.hostStages`）；源声明的阶段宿主不支持 → **`E_HOST_STAGE`**（只禁用该入口，不影响五阶段）。这是唯一"加载成功但某个入口不可用"的情况，因此**不复用** `E_UNSUPPORTED`（后者表示整份源缺能力）。
- 扩展阶段复用同一套 `request`/`out`/`paginate`/`script` 机制，不引入新语法。
- `requiresLogin: true` → 未登录直接 `E_LOGIN_REQUIRED`。

---

# 第六部分 · 脚本与预算

## 7. 脚本

### 7.1 两档 `lib`

```jsonc
"lib": {
  "pure": "function cleanTitle(s){ return s.replace(/[《》\\[\\]]/g,'').trim() }\nfunction glyph(ch){ ... }",
  "ctx":  "function sign(ctx, s){ return md5(ctx.vars.salt + s) }\nfunction fetchContent(ctx){ return ctx.http.get(ctx.var('api') + '/c').json().data }"
}
```

| 档 | 能做什么 | 调用位置 | 限制 |
|---|---|---|---|
| `pure` | 纯计算：字符串、数组、正则、JSON、数学、`expr:` 可用函数 | `transform.js`、`lib:fn` 选择器、`let` | 无 IO、无时间、无随机、无上下文写 |
| `ctx` | 读写会话与上下文、发请求、读缓存、读源配置 | `script.*`、`transform.js`（`ctx:true`） | 禁文件系统、禁反射、受预算 |

**为什么分两档**：真实源 90% 的逻辑（签名、解密、归一化、配置读）既不是纯函数也不是阶段钩子。只给纯函数会逼作者把一切塞进 `after`，结果比 Legado 更难维护；而把 IO 全放开又会重蹈"脚本逃生舱"。

### 7.2 `ctx` 的受控接口

| 接口 | 说明 |
|---|---|
| `ctx.var(name)` | 读上下文（只读；写上下文一律走 `ctx.session.set`） |
| `ctx.config(key)` | 读源配置（`vars.<key>.persist` 的用户值） |
| `ctx.http.get(url, opts)` / `.post(...)` | 发请求，受 `http.rate` 与预算 |
| `ctx.cache.get(k)` / `.set(k, v, ttl)` | 缓存 |
| `ctx.session.get(k)` / `.set(k, v)` | 会话 |
| `ctx.cookie.get/set/remove` | Cookie |
| `ctx.log(msg)` | 写调试轨迹 |
| `ctx.crypto` | 与 `expr:` 同一套加密函数 |
| `ctx.now()` / `ctx.rand()` | 需 `time` / `random` feature |

**明确不提供**：`java.*`、`org.*`、`globalThis.fetch`、`require`、`process`、文件系统、任意反射、`eval`。

### 7.3 确定性约束

- 同一输入 + 同一 `limits` ⇒ 同一输出。步数计量（`libSteps`）优先于墙钟。
- `now()` / `rand()` 只能出现在声明了 `time` / `random` feature 的源里；用于逐章规则 → `E_COST`。
- 脚本抛异常 → `E_SCRIPT`，附 `lib` 函数名与行号，**不静默吞掉**。

### 7.4 作用域

- `pure` 函数之间可互相调用；无 `this`、无全局变量、无隐式注入。
- `ctx` 函数的第一个参数固定是 `ctx`；不允许依赖 `this`。
- 规则字段不执行任意 JS：唯一入口是 `lib:` 与 `script`。

### 7.5 三个钩子（作用对象必须明确）

| 钩子 | 时机 | 输入 | 输出 | 能改什么 |
|---|---|---|---|---|
| `script.before` | 请求**发出前** | `ctx` | 可选：`{url, headers, body}` | 请求本身 |
| `script.onResponse` | 响应**解码后、`parse` 前** | `ctx`, `body`（`str` 或 `bytes`） | `str` \| `bytes` | **原始响应内容** |
| `script.after` | `out` **求值后** | `ctx`, `out` | `out` | 已解析的契约结果 |

```jsonc
"script": {
  "before":     { "fn": "lib:prepare", "ctx": true },
  "onResponse": { "fn": "lib:fixHtml", "ctx": true },
  "after":      { "fn": "lib:postProcess", "ctx": true }
}
```

**`onResponse` 是 v2.0 新增的**，专门解决一类此前无解的需求：站点返回的 HTML 被包裹/截断/需要按规则改写，而这些改写必须在**解析之前**发生。Legado 用 `loginCheckJs`（名字是错的，实际是响应改写器）和 `bodyJs` 做这件事，RSL 把它显式化为一个钩子。

- `onResponse` 的返回值类型必须与输入一致（`str` 进 `str` 出，`bytes` 进 `bytes` 出），否则 `E_TYPE`。
- `onResponse` 里禁止调用 `ctx.http`（会形成请求递归）→ `E_SCRIPT`。
- `script` 出现在逐章规则里 → `E_COST`。

---

## 8. 预算与成本

### 8.1 `limits`

```jsonc
"limits": {
  "maxBytes":        { "value": 2097152, "on": "reject" },
  "maxNodes":        { "value": 5000,    "on": "truncate" },
  "timeoutMs":       20000,
  "maxRedirects":    5,
  "maxPages":        { "value": 50, "on": "truncate" },
  "maxParallel":     4,
  "libSteps":        1000000,
  "scriptTimeoutMs": 3000,
  "mediaBytes":      20971520,
  "loginCheckTtlMs": 300000
}
```

### 8.2 `on` 的超限行为

| 值 | 行为 |
|---|---|
| `reject` | 立即终止，返回 `E_BUDGET`，不带结果 |
| `truncate` | 截断到上限后继续，附 `W_BUDGET_TRUNCATED` 与部分结果 |

默认建议：`maxBytes: reject`（超大响应通常是登录墙/错误页）、`maxNodes: truncate`、`timeoutMs: reject`、`maxPages: truncate`。

### 8.3 成本模型（`E_COST`）

```
阶段耗时 ≈ 项数 × 逐字段规则数 × 每条规则的跨桥次数 × 单次跨桥成本
```

编译器对逐章规则做静态判定（§6.5.1）。这不是性能建议，是**加载期错误**——把 19 秒的目录超时消灭在编译期。

### 8.4 确定性计量

`libSteps` 按语句/循环步数计，与机器快慢无关。墙钟超时只作最后兜底，超时错误码是 `E_TIMEOUT`，不是 `E_BUDGET`。

---

# 第七部分 · 能力与降级

## 9. 能力声明与降级

### 9.1 `requires`

```jsonc
"requires": {
  "engine": ">=2.0 <3.0",
  "features": ["js", "time", "crypto", "font", "webview"],
  "crypto": ["aes-cbc", "hmac-sha256", "md5"],
  "selectorLevel": { "css": "4", "xpath": "1.0", "jsonpath": "9535" }
}
```

| feature | 含义 |
|---|---|
| `js` | 有 JS 沙箱（`lib`/`script`） |
| `time` / `random` | `now()` / `rand()` |
| `crypto` | 加密原语；具体算法在 `crypto` 数组里按名声明 |
| `font` | 字体解析与字形映射 |
| `webview` | WebView 取数与嗅探 |
| `detect` | 编码自动探测 |
| `xpath` / `jsonpath` / `css` | 对应选择器后端 |
| `captcha` | 验证码宿主接管（`E_CHALLENGE`） |
| `tls` | TLS 指纹模拟（`http.tls.fingerprint`） |

缺失 → `E_UNSUPPORTED`，错误信息必须点名缺哪个 feature / 哪个算法。

### 9.2 选择器能力等级

| 命名空间 | level | 必须支持 |
|---|---|---|
| `css` | `3` | 全部 CSS3 选择器 |
| `css` | `4` | 3 的全部 + `:is()` `:where()` `:not(列表)` `:has()` `:nth-child(An+B of S)` `:empty` `:root` `[attr=v i]` |
| `xpath` | `1.0` | XPath 1.0 全部 |
| `json` | `9535` | RFC 9535 全部，含过滤表达式 |

`:is()` / `:where()` / `:not()` / `:has()` 的参数按 `<forgiving-selector-list>` 处理：其中某条解析失败只忽略该条。

### 9.3 渲染降级链

`http`（默认，最快）→ `webview` → `auto`（先 HTTP，命中 `autoUpgradeIf` 再升级一次）。不要为了少数站点让所有请求付渲染成本。

### 9.4 扩展字段

`x-` 前缀，可出现在任意层级；引擎忽略但保留（保证往返不丢信息）。

---

# 第八部分 · 错误与执行

## 10. 元信息 `meta`

```jsonc
"meta": {
  "author": "…", "homepage": "…", "license": "MIT",
  "updated": "2026-11-21",
  "comment": "…",
  "changelog": [{ "version": "2.0.0", "date": "…", "note": "适配站点改版" }]
}
```

## 11. 错误模型

所有错误都是结构化的，永不返回裸空串。

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

### 11.1 错误码

| 码 | 时机 | 说明 |
|---|---|---|
| `E_PARSE` | 编译期 | JSON 语法错误（含行列号） |
| `E_SCHEMA` | 编译期 | 不符合 Schema / 互斥规则冲突 |
| `E_ENUM` | 编译期 | 枚举值非法 |
| `E_REF` | 编译期 | `var:` / `let:` 引用未定义（含契约字段拼写） |
| `E_TYPE` | 编译期 / 运行期 | 管道步骤类型不匹配；运行期仅用于 `script.onResponse` 返回值与输入不一致 |
| `E_REGEX` / `E_REGEX_UNSUPPORTED` | 编译期 | 正则非法 / 超出 RE2 子集 |
| `E_SELECTOR` | 编译期 | 选择器语法错误或缺命名空间 |
| `E_COST` | 编译期 | 逐章规则含 IO/脚本/不确定性 |
| `E_EXPR_SYNTAX` / `E_EXPR_FN` / `E_EXPR_LIMIT` | 编译期 | `expr:` 语法 / 函数 / 长度超限 |
| `E_TEMPLATE` | 编译期 | `${...}` 插值本身非法（未闭合 / 空 / 未知命名空间 / 嵌套） |
| `E_UNSUPPORTED` | 加载期 | `features` / `crypto` 算法缺失（**不含** hostStage，见 `E_HOST_STAGE`） |
| `E_VERSION_UNSUPPORTED` | 加载期 | `rsl` 主版本不兼容 |
| `E_NET` | 运行期 | DNS/TLS/超时/连接失败 |
| `E_TIMEOUT` / `E_CANCELLED` | 运行期 | 超时 / 宿主取消 |
| `E_HTTP` | 运行期 | 非 2xx（附状态码与片段） |
| `E_CHARSET` | 运行期 | 解码失败 |
| `E_LOGIN_REQUIRED` | 运行期 | 未登录或失效 |
| `E_CHALLENGE` | 运行期 | 人机验证，需宿主介入 |
| `E_DECODE` | 运行期 | 图片/媒体解密失败 |
| `E_NO_MATCH` | 运行期 | 选择器命中 0 且字段必填 |
| `E_EMPTY` | 运行期 | 列表为空且 `emptyOk` 为假（info 级） |
| `E_FIELD_MISSING` | 运行期 | 保留字段为空 |
| `E_VERIFY` | 运行期 | `verify` 校验失败 |
| `E_PROTECT_LOST` | 运行期 | `protect` 占位符在管道中丢失 |
| `E_BUDGET` | 运行期 | 超预算 |
| `E_SCRIPT` | 运行期 | lib/脚本异常 |
| `E_HOST_STAGE` | 运行期 | 宿主未实现该扩展阶段 |

> 共 **31 个错误码**：上表按语义合并了同名族（`E_REGEX`/`E_REGEX_UNSUPPORTED`、`E_EXPR_*` 三兄弟、`E_TIMEOUT`/`E_CANCELLED` 各占一行）。逐码处置见 `ERRORS.md`。

**每个错误码的"怎么修"见 `ERRORS.md`。**

### 11.2 警告

`W_UNKNOWN_FIELD`、`W_BUDGET_TRUNCATED`、`W_DEGRADED`、`W_DEPRECATED`、`W_ZIP_LENGTH`。警告不阻断执行，进 `--trace` 输出。`W_DEPRECATED` 只在引擎内置的弃用表命中时触发（当前表为空；新增弃用项必须在变更记录里同步登记）。

### 11.3 降级语义

"合法空""可降级""失败"三者必须分开：

| 情况 | 表现 |
|---|---|
| 成功 | 有必需字段且业务状态正确 |
| 合法空 | `emptyOk: true` → 返回 `[]`，不发 `E_EMPTY`（`E_EMPTY` 只在 `emptyOk: false` 时以 info 级出现） |
| 可降级 | 增强字段失败 → 主结果保留，附 `degraded: true` 与 `W_DEGRADED` |
| 失败 | 无法完成核心功能 → 对应错误码 |

富目录失败要保留普通目录；推荐为空要隐藏入口；歌词失败要保留音频；弹幕失败返回空轨道。**增强失败不得清除已取得的核心结果。**

## 12. 执行语义

### 12.1 调度

```
load → compile(一次) → [search] → select(book) → [detail] → [toc] → select(chapter) → [content]
```

- 编译只做一次，`CompiledSource` 可序列化（bincode/postcard）落盘缓存，二次加载跳过解析与语义分析。
- 阶段之间无共享可变全局；上下文由契约注入，可快照、可重放。
- **`detail` 与 `toc` 的响应可复用**（§6.5.2）；复用不产生新请求。

### 12.2 并发与限流

- 引擎保证对同一源遵守 `http.rate`。
- `let.parallel` 与宿主 `searchMany` 都受 `limits.maxParallel` 约束。
- 每个请求独立预算，互不影响。
- **单请求超时默认 20s**（`limits.timeoutMs` 未设时取 §8.1 的 20000），**整体搜索不设总超时**——单个源慢不应该拖死全部；由宿主决定何时取消。
- **保序**：目录、正文的并发结果必须按源顺序拼接，不得按到达顺序。搜索不受此约束（由 §12.4 合并语义处理）。

### 12.3 缓存分层

| 层 | 内容 | TTL |
|---|---|---|
| 编译缓存 | AST | 进程内 / 落盘 |
| 响应缓存 | HTTP（源 + URL + 可变参数哈希） | 源声明 |
| 解析缓存 | 正文（章节 URL → 块） | 永久（本地库） |
| 目录缓存 | 章节列表 | 可配置；部分结果用 `ttlPartial` |

缓存键必须带维度（`book.id` / `mode` / 源身份），否则会串书。**不同语义的键不得共享同一命名空间**——源变量、登录令牌、用户信息必须是独立的存储，不能靠前缀挤在同一张 KV 表里。

### 12.4 多源合并契约（`merge`）

搜索是**多源并行**的，但"多个源返回同一本书怎么办"此前完全没有定义。RSL 把它写成契约：

```jsonc
"merge": {
  "key": ["name", "author"],        // 折叠键，默认 [name, author]
  "normalize": ["trim", "t2s"],     // 折叠前归一化，默认 [trim]
  "crossBucket": true,              // 强制全局比较（默认 true）
  "order": ["exact", "contains", "other"],
  "weight": 1.0,                    // 本源权重，参与同分排序
  "minScore": 0                     // 低于此分的条目丢弃
}
```

**这是宿主职责，但语义必须统一**——否则三端各写一套，同一份源在三个 App 里给出三种结果。

规则：

1. **归一化**：每条候选先按 `normalize` 处理 `key` 字段（`trim` / `t2s` / `lower` / `stripPunct`）。
2. **匹配分档**（按顺序判定，命中即停）：
   - `exact`：`key` 全部字段**相等**
   - `contains`：`key` 全部字段**互相包含**（`a.contains(b) || b.contains(a)`）
   - 其余归 `other`
3. **折叠**：同一档内 `key` 相等的条目合并为一条，`origins` 累加来源，`weight` 取最大值。
4. **跨档比较**：`crossBucket: true` 时，**不同档之间也要比较 `key`**——这是 v2.0 相对 Legado 的关键修正。Legado 只在同桶内比较，同一本书落不同桶会重复出现两条。
5. **排序**：先按 `order` 里列出的档位顺序分组（未列出的档位排在最后），同档内按 `sources`（命中源数）降序，再按 `weight` 降序，最后按名称字典序（保证确定性）。
6. **分数**：`score = bucketRank × 100 + sources × 10 + weight`（`bucketRank` = 该条目在 `order` 中的下标，越小越靠前），`score < minScore` 的条目丢弃。

宿主可以**忽略** `merge` 块（用自己的策略），但一旦声明就必须按此语义执行；`rsl test` 的合并断言以本契约的默认值为准。

### 12.5 取消与恢复

- 宿主取消 → `E_CANCELLED`，已取得的结果保留并标记 `partial: true`。
- 搜索可暂停/恢复：暂停后不再启动新源，已在飞的请求跑完；恢复后继续。暂停状态不写入磁盘。

---

# 第九部分 · 测试、工具链与边界

## 13. 测试与调试

### 13.1 结构化轨迹

`rsl run --trace` 输出 JSON 数组，每条：

```jsonc
{ "stage": "toc", "step": "out.map.title", "rule": "css:a",
  "matched": 1200, "ms": 3, "bridge": 0, "sample": "第一章 风起" }
```

`bridge` 是该条规则的跨桥次数——它就是目录超时的量化指标。

### 13.2 测试格式

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
      - count("list") == 20
      - "list[0].map.name" == "斗破苍穹"
      - "list[0].map.url" =~ ^https://example\.com/book/\d+$
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

**证据等级**：低等级通过不得写成高等级可用。

### 13.3 `rsl lint` 的规则清单

Lint 不联网，只做静态检查。以下规则**必须全部实现**（每条都能被单测覆盖）：

| # | 规则 | 错误码 |
|---|---|---|
| 1 | 选择器必须有命名空间 | `E_SELECTOR` |
| 2 | `var:` 引用必须在引用图内 | `E_REF` |
| 3 | `let:` 必须在同阶段先声明 | `E_REF` |
| 4 | `range` 与 `pick` 互斥 | `E_SCHEMA` |
| 5 | `blocks` 与 `paragraphs` 互斥 | `E_SCHEMA` |
| 6 | `paginate.kind` 与字段匹配 | `E_SCHEMA` |
| 7 | `pageVar` 与 `enumerate` 互斥 | `E_SCHEMA` |
| 8 | `direct` 与非 `skip` 的 `request` 互斥 | `E_SCHEMA` |
| 9 | `login.steps` 与顶层 `fields` 互斥 | `E_SCHEMA` |
| 10 | `login.steps[].next.to` 必须存在；`steps[].id` 必须唯一 | 不存在 → `E_REF`；重复 → `E_SCHEMA` |
| 11 | 管道类型推断 | `E_TYPE` |
| 12 | 逐章规则白名单 | `E_COST` |
| 13 | 正则 RE2 子集 | `E_REGEX_UNSUPPORTED` |
| 14 | `features` / `crypto` 声明 | `E_UNSUPPORTED` |
| 15 | `lib:` 函数存在性与档位 | `E_SCRIPT` |
| 16 | `onResponse` 不得调 `ctx.http` | `E_SCRIPT` |
| 17 | `reuseDetailHtml` 必须有 `detail` 阶段 | `E_SCHEMA` |
| 18 | `merge.key` 字段必须存在于 `search.out.map` | `E_REF` |
| 19 | 保留字段类型 | `E_SCHEMA` |
| 20 | `x-` 前缀之外的未知字段 | `W_UNKNOWN_FIELD` |

## 14. 工具链

| 命令 | 作用 |
|---|---|
| `rsl lint <src>` | 只做静态检查（不联网），跑 §13.3 全部 20 条 |
| `rsl compile <src>` | 编译并输出 IR 摘要与诊断 |
| `rsl run <src> <stage> [--kw …] [--trace]` | 执行单个阶段 |
| `rsl record <src> <stage>` | 录制真实响应为 fixture |
| `rsl test <src>` | 跑测试用例 |
| `rsl build <dir>` | 多文件源装配（`src/` → `dist/`），确定性构建 |
| `rsl pack <src>` | 打包分发产物（含校验和与签名） |
| `rsl convert <legado.json>` | 存量源迁移 + 不可转换报告（见 `MIGRATION.md`） |
| `rsl explain <src> <path>` | 解释某条规则为何命中/未命中 |

`build` 的验收要求：同一份 `src/` 产物**逐字节一致**（固定基准时间戳），且产物必须过语法门禁。

## 15. 与 Legado 的能力对照

| 能力 | Legado | RSL |
|---|---|---|
| 选择器 | 8 级前缀猜测 | 显式命名空间 |
| 正则 | `##` 全局开关 | `re:` + `transform.regex` |
| 索引/切片 | `[!2][0:10]` 私有方言 | `range`（含负步长）/ `pick` / `exclude` |
| 请求 | URL 尾部 `,{}` | 结构化 `request` |
| 请求参数编码 | `charset`（三态，只作用于请求） | `encode.params` |
| 响应编码 | `charset`（实际对响应无效） | `charset` + 强制判定顺序 |
| 登录 | `loginUrl` + `loginUi` + `checkJs` | `login.steps[]` + 宿主 UI |
| 登录失效 | 无自动重登，22 处手动入口 | `check` + `E_LOGIN_REQUIRED` |
| Cookie | 按 eTLD+1 跨源共享 | 源 id + 域名 + 登录名 |
| 多步请求 | `@put/@get` | `let`（含并发批次） |
| 分页 | `{{page}}` + `<1,2,3>` | `paginate`（page / next / cursor / enumerate） |
| 分页终止 | `hasMore` 恒真，无限触底 | `stopIf.empty` + `stopIf.noNew` |
| 详情→目录复用 | `bookUrl==tocUrl` 时复用 | `toc.reuseDetailHtml` |
| 纯 URL 正文 | 正文规则空 → 隐式兜底 | `content.direct: true` 显式声明 |
| 占位保护 | `<usehtml>` 硬编码 | `protect`（通用） |
| 响应改写 | `loginCheckJs` / `bodyJs`（名字与作用不符） | `script.onResponse` |
| 多源合并 | 4 桶，跨桶不比较 | `merge` 契约，`crossBucket` 强制全局 |
| 目录语义 | `isVip/isPay/isVolume` | 同名字段 + 引擎强制行为 |
| 逐章成本 | 靠经验 | `E_COST` 编译期禁止 |
| 字体反爬 | `queryTTF/replaceFont` | `transform.font` + `features:["font"]` |
| 加解密 | 30 个 `java.*` 函数 | `expr:` / `transform.crypto`，按算法声明 |
| WebView | `webView` + `webJs` + `sourceRegex` | `engine` + `webJs` + `sniff` |
| 媒体解密 | `imageDecode` / `coverDecodeJs` | `media.decode`（bytes 契约） |
| 弹幕/歌词 | `subContent` 按载体分流 | `tracks`（一种结构） |
| 评论/段评 | `customButton` + WebView 硬凑 | `hostStages` 契约 |
| 发现筛选 | `ExploreKind` 控件编程（5 种控件） | `filters` 数据选择 |
| 发现分类树 | 扁平列表 + UI 约定 | 显式嵌套 `groups` |
| 源配置 | `source.getVariable()` | `vars.<k>.persist` + `secret` |
| 状态分层 | 五套变量域混用 | 六层模型（§5.1） |
| 失败 | 静默空串 | 结构化错误码 + 降级标记 |
| 测试 | 无 | 内置格式 + 证据等级 + `lint` 20 条 |
| 多端 | 各端重写 | Rust 一次实现 |

完整逐层对比（58 条差异、源码行号、12 处 Legado 更强的地方）见 `KERNEL-COMPARE.md`。

## 16. 明确不采纳的设计

以下写法在 RSL 里**永远不合法**，理由记录在此以免被反复提议：

| 反例 | 它解决的问题 | 为什么不采纳 |
|---|---|---|
| `data:name;base64,<json>,{opts}` 伪协议 | 跨阶段传结构化上下文 | 上下文进 URL 后不可校验、不可 diff；RSL 用 `out` 契约传 |
| `{{ }}` 里执行任意表达式/副作用 | 动态值注入 | 模板里跑副作用，求值时机未定义；RSL 只允许 `var:`/`expr:` |
| `##正则##替换##` 尾符号编码操作类型 | 一次替换 vs 全文替换 | 用 `transform.regex.all` 布尔字段表达 |
| `@put/@get` 规则内变量 | 跨规则传值 | 用 `let` + `session` |
| 单源内聚合多站点 | 一站式搜索 | 一个源 = 一个站点；聚合是宿主职责 |
| 任意自定义 UI 控件（`loginUi` 的 5 种控件 / `push`） | 面板、开关、线路切换 | UI 不进标准；用 `vars.persist` + `filters` + `login.steps` 表达**数据** |
| 让源用 JS 操作宿主 UI（`reLoginView`） | 多步骤登录 | 用 `login.steps[]` 的步骤图表达；UI 渲染归宿主 |
| 全字段可写 JS，作用域注入宿主对象 | 极限适配 | 绑定宿主语言；RSL 只给 `lib` 两档 + 受控 `ctx` |
| 逐章规则里发请求/读配置 | 少写几个字段 | 成本乘以章数，实测 19 秒超时；编译期禁止 |
| 远程 `jsLib`（源引用外部脚本 URL） | 复用脚本 | 源必须自包含、可审计、可离线；远程脚本让"这份源做什么"不可判定 |
| `upload()` 表单上传 | 上传书籍 | 源是取数工具，不是上传工具 |
| 阶段级"整条回退"（发现规则空 → 用搜索规则） | 少写一段 | 静默换义；要复用就显式写两个阶段或 `x-` 扩展 |

## 17. 法律与分发边界

1. 本仓库**不包含任何书源、内容或站点信息**，示例域名为虚构地址。
2. 引擎是中立的 HTTP + 解析工具，性质等同 `curl` + 解析库。
3. 不提供公共服务实例、不做内容聚合、不绕付费墙。
4. 源由使用者自行编写与导入，合规责任在使用者。
5. 分发层只发程序，不发源，不发公共实例。

> 背景：Legado 主仓库 2026-05 因侵权被清空，Tachiyomi 扩展体系反复被 DMCA 清洗。架构上内容与软件彻底分离、分发上绝不集中托管源，是这套标准的前提条件，不是可选项。

---

## 附录 A · 示例源

- `examples/novel-html.rsl.json` — HTML 小说站：CSS 选择器、多页目录、正文分页、字体反爬、逐章规则合规示范、`reuseDetailHtml`。
- `examples/comic-api.rsl.json` — JSON 接口漫画站：`let` 签名、`images` 逐图请求头、`media.decode`、`tracks`、`content.direct`。
- `examples/video-login.rsl.json` — 带登录的视频站：`login.steps`、`token:`/`cookie:`、`paginate.cursor`、`sniff`、`variants`、`hostStages`。
- `examples/aggregate-merge.rsl.json` — 聚合源：`merge` 契约、`enumerate` 分页、`protect` 占位保护。

## 附录 B · Schema

`schema/rsl.schema.json`（JSON Schema 2020-12）。根级 `additionalProperties: true`。

## 附录 C · 变更记录

| 版本 | 日期 | 变更 |
|---|---|---|
| 2.0.0-draft.1 | 2026-11-21 | 整体重写：`login.steps[]` 多步登录、`merge` 多源合并契约、`script.onResponse` 响应钩子、`paginate.enumerate`、`toc.reuseDetailHtml`、`content.direct`、`protect`、`transform.zip`、`range` 负步长、`stopIf.noNew`、`E_PROTECT_LOST`、`lint` 20 条清单、`rsl explain` |
| 1.1.0-draft.1 | 2026-11-20 | 载体逐条覆盖、六层状态、`E_COST`、`tracks`、`filters`、`hostStages`、`cursor` 分页、`encode` 分离、`lib` 两档、`emptyOk`/`verify`/降级、`crypto` 按算法声明 |
| 1.0.0-draft.1 | 2026-11-13 | 初稿 |

## 附录 D · Schema 的能力边界

JSON Schema 只校验**形状**，不校验**语义**：

| 负例 | Schema | 谁拦 |
|---|---|---|
| 阶段名拼错 | ✅ | 枚举 |
| 列表型 `out` 缺 `map` | ✅ | 必填 |
| 选择器缺命名空间 | ❌ | 编译器 `E_SELECTOR` |
| `node[]` 上做 `regex` | ❌ | 编译器 `E_TYPE` |
| `var:book.tocUrl` 拼错 | ❌ | 编译器 `E_REF`（引用图） |
| 逐章规则里发请求 | ❌ | 编译器 `E_COST` |
| 用了未声明的 `crypto` 算法 | ❌ | 编译器 `E_UNSUPPORTED` |
| `login.steps` 有环 | ❌ | 编译器 `E_SCHEMA` |
| `merge.key` 指向不存在的字段 | ❌ | 编译器 `E_REF` |

**因此必须有两层**：Schema 层（编辑器友好、秒级、语言无关）与编译器层（引用图、类型推断、成本分析）。只发一个 Schema 不算有标准——Legado 的问题从来不是"不知道格式"，而是"格式里的字符串没人检查"。
