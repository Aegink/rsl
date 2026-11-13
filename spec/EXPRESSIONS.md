---
id: rsl-expr
type: submodule-design
status: draft
title: RSL 表达式语言 expr: 规范
parent: rsl-compiler
tags: [rsl, expressions]
---

# RSL 表达式语言 `expr:` 规范

> 配套：[RSL-1.0.md §4.4](RSL-1.0.md)（语言契约 **v2.0**）｜[COMPILER.md §3.3](COMPILER.md)（类型推断）
> 目标：在"能算数、能签名"与"不越界成 JS"之间取一个**可静态校验、跨端一致**的平衡点。

## 1. 为什么必须有它

现网绝大多数站点分页不是"点下一页"，而是 URL 里的算术；鉴权也不是"点登录"，而是参数里的摘要：

```
/search?q=xx&pn={page-1}
/api/list?offset={page*20}&size=20
/api/search?sign=md5(kw + ts + salt)
```

Legado 用 Rhino 直接求值 `{{page-1}}`——等于把任意 JS 塞进 URL，代价是无法静态校验、无法脱离 JVM、无法保证各端一致。

`expr:` 只做**确定性计算**：相同输入永远得到相同输出，编译期就能知道类型、变量是否存在、用了哪个加密算法。

v2.0 的 `expr:` 文法未变，变的是"它能看见什么"：新增 `var:login.<field>`、`var:scriptResult`（§5.4），并明确了 `paginate.kind: "enumerate"` 阶段没有 `page`（§7.1）。

## 2. 词法

| 记号 | 规则 |
|---|---|
| 数值 | 整数 `[0-9]+`、浮点 `[0-9]+\.[0-9]+` |
| 字符串 | `'...'` 或 `"..."`，支持 `\\ \' \" \n \t` |
| 布尔 / 空 | `true` `false` `null` |
| 标识符 | `[A-Za-z_][A-Za-z0-9_]*`（仅变量名与函数名） |
| 运算符 | `+ - * / % == != < <= > >= && \|\| ! ? : ( ) ,` |
| 空白 | 忽略 |

**禁止**：注释、分号、赋值 `=`、箭头 `=>`、反引号、正则字面量 `/.../`、属性访问 `.`、下标 `[]`。

### 2.1 为什么禁 `.`：`expr:` 读不到 `book.*` / `chapter.*`

`.` 被禁是为了不和 JSONPath 抢语义，代价必须讲清楚：**任何带 `.` 的路径都不能写进表达式**，`var:` / `let:` 形式也不行。

| 想写 | 结果 | 原因 |
|---|---|---|
| `expr:var:book.tocUrl` | `E_EXPR_SYNTAX`（列号指向 `.`） | `.` 不是合法记号 |
| `expr:book.tocUrl` | `E_REF` | `book` 不是标量变量 |
| `expr:var:chapter.url` | `E_EXPR_SYNTAX` | 同上 |
| `expr:var:filter.mode` | `E_EXPR_SYNTAX` | `filter.*` 同样是带 `.` 的路径 |
| `expr:var:kw + '-1'` | ✅ 合法 | `kw` 是标量 |

**正确路径三步**：选择器取出嵌套标量 → 放进阶段 `let` → `expr:` 只用 `let:<name>`。

完整例子（搜索签名要用接口返回的 `uid`，而 `uid` 藏在 JSON 嵌套里）：

```jsonc
"search": {
  "let": {
    "uid":  { "request": { "url": "var:baseUrl/api/user" },
              "take": { "$": "json:$.data.user.uid" } },   // ① take 是选择器：嵌套标量 → let
    "ts":   "expr:now()",                                  // ② 纯计算（需 features:["time"]）
    "sign": "expr:md5(let:uid + str(let:ts) + 'salt')"      // ③ 只用 let:，通篇没有 `.`
  },
  "request": { "url": "var:baseUrl/api/search",
               "params": { "q": "var:kw", "sign": "let:sign" } },
  "out": { "list": "json:$.data.list[*]",
           "map": { "name": "json:$.title", "url": "json:$.url" } }
}
```

契约字段（`var:book.*`）同理，只是不需要请求：`"bid": { "$": "var:book.id" }`。这种"纯选择器作 `let` 项"的写法规范 §6.1.1 未显式列出（只列了 `expr:` / `{request,take}` / `{parallel}` / `lib:` 四类），但实现必须支持，否则上面的链路对契约字段就断了。

## 3. 文法（EBNF）

```ebnf
expr        := ternary
ternary     := logicOr [ "?" expr ":" expr ]
logicOr     := logicAnd { "||" logicAnd }
logicAnd    := equality { "&&" equality }
equality    := relational { ("==" | "!=") relational }
relational  := additive  { ("<" | "<=" | ">" | ">=") additive }
additive    := multiplicative { ("+" | "-") multiplicative }
multiplicative := unary { ("*" | "/" | "%") unary }
unary       := "!" unary | "-" unary | primary
primary     := NUMBER | STRING | "true" | "false" | "null"
             | IDENT [ "(" [ args ] ")" ]
             | "(" expr ")"
args        := expr { "," expr }
```

优先级由文法层级决定，不需要额外优先级表。

### 3.1 `+` 的双重语义

`+` 是唯一重载运算符，规则**编译期确定**：

| 左操作数 | 结果 |
|---|---|
| 数字 + 数字 | 数值相加 |
| 字符串 + 任意 | 字符串拼接（右操作数按 `str` 转换） |
| 列表 + 列表 | 列表连接 |

编译期能确定类型的直接报 `E_TYPE`（如 `1 + true`）。

## 4. 类型

```
Num | Str | Bool | Null | List
```

隐式转换（仅以下情况允许）：

| 场景 | 转换 |
|---|---|
| `<` `>` 比较 | 两边转 Num；不能转则 `E_TYPE` |
| `==` 比较 | 同类型直接比；Num↔Str 时 Str 尝试转 Num |
| 布尔上下文（`&&` `!` `?:`） | 空串、`0`、`null`、空数组为假 |
| 字符串拼接 | `str()`：Num 去掉多余小数，Bool→`"true"/"false"`，Null→`""` |

**不做**数组与字符串互转（易出歧义），需要时用 `join()` / `split()`。

## 5. 内置函数

### 5.1 纯函数（无需声明）

| 函数 | 签名 | 说明 |
|---|---|---|
| `min(a, b, ...)` / `max(...)` | `Num... -> Num` | 最小/最大 |
| `abs(n)` `round(n)` `ceil(n)` `floor(n)` | `Num -> Num` | 数值 |
| `len(x)` | `Str\|List -> Num` | 长度 |
| `upper(s)` `lower(s)` `trim(s)` | `Str -> Str` | 大小写/去空白 |
| `replace(s, from, to)` | `Str,Str,Str -> Str` | **字面量**替换（正则用管道，见 §10） |
| `split(s, sep)` / `join(list, sep)` | `Str,Str -> List` / `List,Str -> Str` | 切分/连接 |
| `contains(s, sub)` | `Str,Str -> Bool` | 包含 |
| `num(s)` / `str(x)` | `Str -> Num` / `any -> Str` | 互转（`num` 失败为 0） |

本表与规范 §4.4 的"纯函数"行逐项一致，**不许多出**：中文数字转换不在 `expr:` 里，用 `transform.numChapter` / `transform.cjkNumber`（规范 §3.6）。

### 5.2 需声明的函数

| 函数 | 声明 | 说明 |
|---|---|---|
| `now()` | `features:["time"]` | 当前毫秒时间戳 |
| `rand()` / `rand(max)` | `features:["random"]` | 0~1 浮点 / 0~max 整数 |
| `md5(s)` `sha1(s)` `sha256(s)` | `crypto` + `crypto:["md5"]` 等 | 十六进制小写摘要 |
| `hmac(alg, key, s)` | `crypto` + `crypto:["hmac-sha256"]` | HMAC 十六进制 |
| `hexEncode(s)` / `hexDecode(s)` | `crypto:["hex"]` | Hex |
| `base64Encode(s)` / `base64Decode(s)` | `crypto:["base64"]` | Base64 |
| `urlEncode(s, charset?)` | `crypto:["url-encode"]` | URL 编码（支持 `gbk`） |
| `cookie(name)` | 源声明 `login` 或 `cookie` | 读 Cookie 值（规范 §4.5.3） |

**未声明而使用** → 编译期 `E_UNSUPPORTED`，明确告知缺哪个 feature / 算法。

> 为什么按算法声明而不是一个布尔 `crypto`：不同实现（Rust / Swift / WASM）可用算法不同，`aes-gcm` 缺失不该阻止只用 `md5` 的源。

**v2.0 一致性校正**（以规范 §4.4 为准，只删不增）：v1.1 表里的 `nowDate`、`sha512`、`aesCbcDecrypt`、`aesCbcEncrypt`、`aesGcmDecrypt`、`desDecrypt`、`tripleDesDecrypt` **已移除**——`expr:` 只承担摘要与编码；对称加密/解密走 `transform.crypto`（规范 §3.6），日期格式化走 `transform.date`。需要"读 Cookie 再参与计算"才保留 `cookie(name)`，它由规范 §4.5.3 特批。

### 5.3 刻意不提供

| 不提供 | 原因 |
|---|---|
| 文件/网络 IO | 纯计算；IO 走 `let` / `request` / `ctx.http` |
| 循环 / 递归 | 保证可终止，防止拖垮 UI 线程 |
| 自定义函数 | 需要逻辑就写 `lib` |
| `eval` / `Function` | 沙箱安全底线 |
| 属性访问 `.` / 下标 `[]` | 避免与 JSONPath 混淆（§2.1） |

### 5.4 表达式里能用的变量

| 写法 | 例子 | 可用阶段 |
|---|---|---|
| `var:<标量>` | `expr:var:page - 1` | 同规范 §5.3（见下表） |
| `let:<name>` | `expr:md5(let:ts + 'salt')` | 同阶段 `let` 中**已声明的前项** |
| `session:<key>` | `expr:session:uid == '' ? 1 : 2` | 全部阶段（单次运行内，规范 §5.5） |
| 裸内置名（简写） | `expr:page - 1` ≡ `expr:var:page - 1` | 同该变量的可用阶段 |
| ❌ 带 `.` 的路径 | `expr:var:book.id` | 编译期 `E_EXPR_SYNTAX`（§2.1） |
| ❌ `token:` / `cookie:` 命名空间 | `expr:token:auth` | 无表达式形式：Cookie 用 `cookie(name)`，令牌或契约字段先落 `let` |

各 `var:` 变量的可见范围（与规范 §5.3 一一对应）：

| 变量 | 类型 | 可用阶段 | 能否进 `expr:` |
|---|---|---|---|
| `var:baseUrl` `var:url` `var:charset` | string | 全部 | ✅ |
| `var:response` | string \| bytes | 全部 | ✅（只判空/长度） |
| `var:kw` | string | search | ✅ |
| `var:page` | int | 有 `paginate` 或 `groups` 的阶段 | ✅（`enumerate` 阶段除外，§7.1） |
| `var:login.<field>` | string | login 步骤 | ❌ 带 `.`（`expr:var:login.username` 非法），先落 `let` 再用 |
| `var:scriptResult` | any | 有 `script` 钩子的阶段 | ✅（用 `str`/`len` 收敛） |
| `var:group` / `var:filter.<key>` / `var:book.*` / `var:chapter.*` | object / string | 见规范 §5.3 | ❌ 带 `.`，走 §2.1 三步 |

## 6. 求值示例

```jsonc
"params": {
  "pn":    "expr:page - 1",
  "size":  "expr:page * 20",
  "last":  "expr:max(1, page - 1)",
  "q":     "expr:urlEncode(var:kw, 'gbk')",
  "label": "expr:'第' + page + '页'",
  "sign":  "expr:md5(var:kw + str(let:ts) + 'salt')"
}
```

`page=3` 时 `'第' + page + '页'` → `"第3页"`（Num 自动 `str`）。

## 7. 编译期检查清单

| 检查 | 错误码 |
|---|---|
| 词法/文法错误（含非法 `.`、`[]`） | `E_EXPR_SYNTAX`（带列号） |
| 引用变量在当前阶段不可见 | `E_REF` |
| 函数名不存在 | `E_EXPR_FN` |
| 使用未声明的 feature / 算法 | `E_UNSUPPORTED` |
| 静态可判定的类型错误（`1 + true`） | `E_TYPE` |
| 表达式超过 512 字符 / 嵌套超 32 层 | `E_EXPR_LIMIT` |
| 逐章规则中使用 `now()` / `rand()` / `cookie()` | `E_COST` |
| `enumerate` 阶段引用 `page` | `E_REF`（§7.1） |

编译产物：AST 直接编译为闭包树（`fn(&Ctx) -> Value`），执行期零解析开销。

### 7.1 与 `paginate.kind: "enumerate"` 的交互

`enumerate` 与 `page` 互斥（规范 §4.7.1），这条约束会一路传导到表达式：

| 情况 | 结果 |
|---|---|
| 阶段同时有 `pageVar` 与 `enumerate` | 编译期 `E_SCHEMA`（lint #7），先于表达式检查 |
| `enumerate` 阶段写 `expr:page - 1` 或 `var:page` | 编译期 `E_REF`：该阶段 `page` 不在引用图内 |
| `enumerate` 的值怎么进请求 | 只能靠 `into`（如 `params.pn`）；表达式拿不到"当前是第几个值" |
| 需要按序号算值 | 用 `joinWith` 一次拼好；或在 `let` 里预置常数列表后逐值使用 |

一句话：**`expr:` 永远只读"已注入的变量"，不会因为分页方式不同而隐式多出一个 `page`。**

## 8. 实现要点（Rust）

```rust
pub enum Expr {
    Num(f64), Str(String), Bool(bool), Null,
    Var(VarRef), Bin(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnOp, Box<Expr>), Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(FnId, Vec<Expr>),
}
pub fn compile(src: &str, scope: &Scope, decl: &Declared) -> Result<Expr, ExprError>;
pub fn eval(e: &Expr, ctx: &Ctx) -> Value;
```

- Pratt 解析器约 250 行，求值器约 200 行；变量用索引（`VarRef(u16)`）而非字符串查找；`FnId` 在编译期解析到具体实现，未声明算法当场被拒。总计实现 + 测试约 850 行（去掉 AES/DES 与 `nowDate` 后比 v1.1 略小）。
- 相比引入 JS 引擎（QuickJS 最小配置约 1.2MB 二进制），这个代价是值得的：`expr:` 覆盖 95% 的算术与摘要需求，零依赖、零沙箱风险。

## 9. 与 JS / 管道的边界

| 需求 | 用什么 |
|---|---|
| `page - 1`、`ceil(total/20)`、字符串拼接 | `expr:` |
| md5 / hmac 签名 | `expr:` + `requires.crypto` |
| AES / DES 解密、编码转换 | 管道 `crypto` / `base64` / `hex` / `charset` |
| 复杂文本清洗（多分支、正则链） | `lib.pure` |
| 字符串截取、数组下标、正则替换、时间格式化 | 管道 `slice` / 选择器 `pick`·`range` / `regex` / `date`（§10） |
| 需要发请求才能算的值 / 需要循环处理列表 | 阶段 `let` + `{request, take}`，或 `lib.ctx` / `lib.pure` |
| **解析前改写响应** | `script.onResponse`（`(ctx, str\|bytes) → 同类型`，禁 `ctx.http`；规范 §7.5） |

一句话判据：**一次求值能算完、无副作用、无循环 → `expr:`；否则 → 管道或 `lib`。**

## 10. 常见错误与替代写法

| # | 需求 | ❌ 错误写法 | ✅ 替代 |
|---|---|---|---|
| 1 | 字符串截取 | `expr:substr(var:kw, 0, 10)`（无此函数） | 管道 `{"slice": "0:10"}`（**字符串**切片）；或 `{"regex": {"regex": "^.{0,10}"}}` |
| 2 | 数组取下标 | `expr:var:list[0]`、`expr:var:list.0` | 选择器 `{"$": "json:$.list[0]"}`；已有列表用 `{"pick": [0]}` 或 `{"range": "0:1"}` |
| 3 | 正则替换 | `expr:replace(var:kw, '/a/g', 'b')` | `{"regex": {"regex": "a", "replace": "b", "all": true}}`；`replace()` 只做字面量替换 |
| 4 | 时间格式化 | `expr:nowDate('yyyy-MM-dd')`（已移除） | `{"date": {"from": "x", "to": "yyyy-MM-dd", "tz": 8}}`（需 `features:["time"]`） |
| 5 | URL 拼接 | `expr:'https://x/book/' + let:id + '?p=' + page` 手拼查询串 | `request.params` + `{"absolute": true}`；相对链接交给 `transform.absolute`，别手写 `encode` |
| 6 | JSON 取字段 | `expr:var:book.title`、`expr:book.title` | §2.1 三步：选择器 → `let` → `expr:let:title`；或直接在 `out.map` 里写 `json:$.title` |
| 7 | 条件默认值 | `expr:len(var:kw) > 0 ? var:kw : '全部'`（能做，但空串/未命中分不清） | 选择器 `default` / `optional: true` / `fallback`；"空列表合法"用 `out.emptyOk` 表达 |
| 8 | 多分支判断 | 嵌套三元 `a ? 1 : b ? 2 : c ? 3 : 4` | 写 `lib.pure` 函数；字段映射用选择器 `typeMap`；候选取值用 `fallback` |
