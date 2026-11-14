---
id: rsl-compiler
type: module-design
status: draft
title: rsl-core 编译器设计
parent: rsl-arch
tags: [rsl, compiler]
---

# rsl-core 编译器设计

> 配套：[RSL-1.0.md](RSL-1.0.md)（语言契约 **v2.0**）｜[EXPRESSIONS.md](EXPRESSIONS.md)（`expr:` 子语言）
> 目标：把合法 JSON 编译成可执行 AST，并在**加载时**拦住所有可静态发现的错误。与规范冲突时以规范为准。

## 1. 为什么必须有编译器

JSON Schema 只能描述形状。下面这些错误 Schema 永远拦不住，但恰恰是书源最常见的故障：

```jsonc
"name":    "li.title"                          // 缺命名空间：CSS 还是 XPath？
"content": {"$":"css:#txt","transform":[{"regex":{"regex":"x"}}]}   // node[] 上做正则
"url":     {"$":"var:book.urll"}               // 拼写错误，运行时才空
"chapter": {"$":"re:(a)\\1"}                   // 反向引用，RE2 不支持
"toc":     {"out":{"map":{"title":{"$":"var:x","transform":[{"js":{"fn":"lib:fetch"}}]}}}}  // 逐章规则里发请求
"login":   {"steps":[{"id":"a","next":{"to":"snp"}}]}                // 步骤图指向不存在的 step
"merge":   {"key":["name","authro"]}                                 // 折叠键在 out.map 里不存在
```

Legado 把前四类全推到运行时，失败返回空串；第五类在现网造成过「807 章目录跑 19 秒超时」。RSL 要求它们在 `compile()` 阶段就报错。v2.0 新增的两类（步骤图悬空、合并键悬空）是同一类病的变体：**结构合法但语义悬空**——Schema 看得见 `"to": "snp"` 是个字符串，看不见 `snp` 不存在。

## 2. 编译流水线

```
JSON 文本
  │
  ├─[1] 解析      ── 语法错误 → E_PARSE（含行列号）
  ├─[2] 形状校验  ── 走 JSON Schema → E_SCHEMA
  ├─[3] 反序列化  ── 强类型结构体（serde）→ 未知字段收集为 W_UNKNOWN_FIELD
  ├─[4] 消糖      ── 字符串选择器 → Selector 对象；快捷字段 → transform 步骤
  ├─[5] 语义分析  ── ★ 本文重点 ★
  │     ├─ 命名空间与选择器语法        → E_SELECTOR            §3.1
  │     ├─ 引用图（契约字段/步骤字段） → E_REF                 §3.2
  │     ├─ 管道类型推断（zip/protect） → E_TYPE                §3.3
  │     ├─ 钩子签名与档位（onResponse）→ E_SCRIPT              §3.4
  │     ├─ 正则子集                    → E_REGEX_UNSUPPORTED   §3.5
  │     ├─ 逐章成本白名单              → E_COST                §3.6
  │     ├─ feature / 算法声明          → E_UNSUPPORTED         §3.7
  │     ├─ 步骤图拓扑 + merge 契约     → E_SCHEMA / E_REF      §3.8
  │     └─ 分页 kind 与字段匹配        → E_SCHEMA              §3.8.3
  ├─[6] 降级为 IR ── 统一中间表示
  └─[7] 生成 AST ── 干净、无分支、可直接解释执行
          └→ CompiledSource（可缓存、可序列化）
```

v2.0 的增量全落在 [5]：`login.steps` 步骤图、`merge` 契约、`paginate.enumerate` 互斥、`onResponse` 签名、`zip`/`protect` 类型。**没有一条新增检查需要联网或执行**——这是 `rsl lint` 能在保存时跑的前提（§3.9）。`CompiledSource` 可序列化（bincode/postcard）落盘，二次加载跳过 [1]~[6]，只做反序列化 + 选择器实例化。

## 3. 语义分析细则

### 3.1 命名空间

规范 §3.2 的 **18 个**命名空间，编译器逐一预编译；表外前缀一律非法：

| 命名空间 | 输入作用域 → 编译产物 |
|---|---|
| `css:` / `xpath:` / `json:` | HTML 节点 / HTML 节点 / JSON 值 → 对应引擎预编译（level 见 `requires.selectorLevel`） |
| `re:` | 字符串 → 预编译 RE2（§3.5） |
| `text` / `owntext` / `html` / `tag` | 节点 → 直接文本 / 自身文本 / innerHTML / 标签名 |
| `attr:<name>` | 节点 → 属性值 |
| `const:<json>` | 常量（JSON 解析失败当字符串） |
| `var:<path>` / `let:<name>` / `session:<key>` | 上下文（§3.2）/ 阶段内 `let` / 会话变量 |
| `token:<name>` / `cookie:<name>` | 登录令牌 / Cookie 值（只读） |
| `lib:<fn>` / `expr:<表达式>` | `lib.pure` 纯函数 / 受限表达式（EXPRESSIONS.md） |

| 检查 | 错误码 |
|---|---|
| 缺 `:` 且不是无参命名空间（`text` `owntext` `html` `tag`），或前缀未知 | `E_SELECTOR`，错误里列全 18 个 |
| `attr:` `var:` `let:` `session:` `token:` `cookie:` `lib:` `expr:` 参数为空 | `E_SELECTOR` |
| `var:` 路径不在引用图 / `let:` 未在当前阶段先声明（含循环引用） | `E_REF` |
| `token:` / `cookie:` 但源未声明 `login` / `cookie` | `E_REF` |
| `lib:` 函数不存在或档位不符（§3.4） | `E_SCRIPT` |
| `expr:` 编译失败 / CSS·XPath·JSONPath 语法错误 | 见 `EXPRESSIONS.md` §7 ／ `E_SELECTOR` |
| `${...}` 模板骨架错误（未闭合 / 空 / 未知命名空间 / 嵌套） | `E_TEMPLATE`；模板里的变量仍走 `E_REF`，表达式仍走 `E_EXPR_SYNTAX` |

### 3.2 引用图

逐条 `var:` 引用查表。`book.*` / `chapter.*` / `login.*` 的字段集合来自契约声明，**不是通配**：

| 作用域 | 可见变量 |
|---|---|
| 全局 | `vars.*` ∪ `{baseUrl, url, response, charset}` |
| `search` | + `kw`；有 `paginate` 时 + `page` |
| `explore` | + `group` `filter.<key>`；有 `paginate` 或 `groups` 时 + `page` |
| `detail` | + `book.*`（= `search`/`explore.out.map` 声明字段 + 保留字段） |
| `toc` | + `book.*`；有 `paginate` 时 + `page` |
| `content` | + `book.*`, `chapter.*`（= `toc.out.map` 声明字段 + 保留字段） |
| `login.<stepId>` | + `login.<field>`（= 该步 `fields[].key`，**仅该步可见**） |
| 有 `script` 钩子的阶段 | + `scriptResult`（`before` / `onResponse` 之后可见，类型 `any`） |
| `hostStages.<id>` | 同五阶段规则；`out.map` 声明什么就可见什么 |

三条硬规则：

1. `var:scriptResult` 只在阶段确实有 `script` 块时进图，否则引用它 → `E_REF`（运行期只会得到空串，最难查的一类 bug）。类型 `any`，使用处必须显式 `transform` 收敛。
2. `var:login.<field>` 的 `<field>` 取自**当前步骤**的 `fields[].key`；跨步骤引用（第 1 步读第 2 步的 `code`）→ `E_REF`。
3. 引擎**不做**"第一个结果自动提升"（规范 §5.4）——宿主必须显式传入选中项，因此 `var:book.tocUrl` 的拼写错误在编译期就是 `E_REF`，不是运行期空串。

### 3.3 管道类型推断

类型格：`Str | StrList | Node | NodeList | Json | JsonList | Bytes | Num | BlockList`。每个步骤在**父作用域**声明的输入类型上求值（v2.0 新增两步标 ★）：

| 步骤 | 签名 | 备注 |
|---|---|---|
| `trim` | `Str\|StrList\|Node\|NodeList → 同` | |
| `regex` / `match` | `Str → Str` | 输入 `StrList` → `E_TYPE`，先 `join` 或对项求值 |
| `replace` | `Str\|StrList\|Node → 同` | `from` 带 `re:` 时按 RE2 预编译 |
| `prepend` / `append` / `absolute` / `urlencode` / `urldecode` | `Str → Str` | `absolute` 基准默认 `var:baseUrl` |
| `base64` / `hex` / `crypto` | `Str\|Bytes → 同` | 算法需 `requires.crypto` 声明 |
| `font` / `html` / `slice` | `Str → Str` | `slice` 是**字符串**切片，与选择器 `range` 无关 |
| `json` / `number` | `Str → Json` / `Str → Num` | |
| `date` / `numChapter` / `cjkNumber` / `t2s` / `s2t` | `Str → Str` | |
| `charset` | `Bytes → Str` | 仅 `response:"bytes"` 时有意义 |
| `join` / `split` | `StrList → Str` / `Str → StrList` | |
| ★ `zip` | `StrList → StrList` | 第二输入来自**父作用域**（见下） |
| ★ `protect` | `任意 → 同` | 占位符进出，类型不变 |
| `unique` / `sort` / `dropEmpty` | `List → List` | |
| `paragraphs` / `blocks` / `strip` | `Node\|NodeList → StrList` / `→ BlockList` / `→ 同` | |
| `js` | 由 `lib` 签名声明（§3.4） | `ctx:true` 时降级为 `任意 → 任意` |

选择器给出输入类型：`css:`/`xpath:` → `NodeList`；`text`/`html`/`tag`/`attr:` → `Str`（多命中 `StrList`）；`json:` → `Json|JsonList`；`re:` → `StrList`；`const:` → 由字面量决定；`var:`/`let:`/`session:`/`token:`/`cookie:` → `Str`（契约声明为数组时 `StrList`）；`expr:` → 由表达式类型决定。不匹配 → `E_TYPE`，hint 给"先加 `{"$":"text"}` 或改用 `paragraphs`"这类改法。

两条 v2.0 专属规则：

- **`zip`**：`with` 不是当前步的输入，而在**该步的父作用域**求值（`css:.price` 与 `css:#list li` 同级），必须解析出 `StrList`；解析出标量 → `E_TYPE`。两侧长度不等是**运行期**警告 `W_ZIP_LENGTH`，编译期不报（长度由数据决定）。
- **`protect`**：在选择器层作用于整个 `transform`，在管道内从该步起生效；两者类型不变，无需额外推断。编译期只做保守检查（能否被后续 `regex`/`slice` 吃掉），无法判定时放行，运行期由 `E_PROTECT_LOST` 兜底。

### 3.4 lib 签名、档位与钩子签名

编译器**不做完整 JS 解析**，只做轻量扫描 `function NAME(`：

- `pure` 里的函数 → 可被 `transform.js`、`lib:` 选择器、`let` 调用，写法 `{"js":"lib:f"}` 或 `{"$":"lib:f"}`；签名默认 `(any...) -> any`。
- `ctx` 里的函数 → 首个形参必须是 `ctx`，只能写成 `{"js":{"fn":"lib:f","ctx":true}}`，且只能从 `script.*` 调用；用 `lib:` 选择器调它 → `E_SCRIPT`（提示"ctx 函数只能从 script 钩子调用"）。

#### 3.4.1 三个钩子的签名约束

| 钩子 | 期望签名 | 检查 | 违规码 |
|---|---|---|---|
| `before` | `(ctx) → {url?, headers?, body?}` | 返回字段白名单；`url` 必须能静态判为字符串 | `E_TYPE` |
| `onResponse` | `(ctx, str\|bytes) → 同类型` | 形参 2 个（编译期可查）；**输出类型必须等于输入类型**（运行期判） | `E_TYPE`（编译期）/ `E_TYPE`（运行期） |
| `after` | `(ctx, out) → out` | 返回值必须仍是该阶段契约形状（缺 `map.name` 之类） | `E_TYPE` |

`onResponse` 的两条额外硬约束（规范 §7.5）：① **禁止 `ctx.http`**——轻量扫描该函数体（含可到达的同档函数）是否出现 `ctx.http`，出现即 `E_SCRIPT`，否则形成"响应 → 请求 → 响应"的递归；② **禁止换类型**——`response:"bytes"` 的阶段返回 `str` 会让后续 `charset` 失去输入，报 `E_TYPE` 并提示对齐。**返回类型只能运行期判**（JS 无静态类型），这是 `E_TYPE` 唯一允许出现在运行期的场景，规范 §11.1 已注明。

`script` 落在 `toc.out.map` 等逐章子树时，判 `E_COST`（§3.6）优先于本节。

### 3.5 正则子集

用 RE2 解析器编译一次。不支持构造（反向引用 `\1`、环视 `(?=)(?<=)`、递归）→ `E_REGEX_UNSUPPORTED`，并给等效改写（"第 n 个匹配" → `transform.match`；反向引用 → `transform.replace` 的 `$1`）。默认 `multiLine` / `dotAll` 为 `false`，由选择器 `flags` 覆盖。

### 3.6 逐章成本分析（`E_COST`）

判定对象：`toc.out.map.*`（逐章求值）。白名单与规范 §6.5.1 逐字对齐：

| 类别 | 允许 |
|---|---|
| 选择器 | `css:` `xpath:` `json:` `re:` `text` `owntext` `html` `tag` `attr:` `const:` `var:` `let:` `session:` |
| 管道 | `trim` `replace` `regex` `match` `prepend` `append` `absolute` `urlencode` `urldecode` `base64` `hex` `html` `json` `date` `number` `numChapter` `cjkNumber` `t2s` `s2t` `slice` `join` `split` **`zip`** **`protect`** **`font`** `unique` `sort` `dropEmpty` `js(lib.pure)` |
| 禁止 | `script`（三个钩子）、`lib.ctx`、`cache`、`http`、`expr:now()`、`expr:rand()`、`cookie:`、`token:` |

`zip`、`protect`、`font` 是白名单成员：`zip` 的 `with` 只在父作用域求值一次（规范 §3.6.1），成本不随章数增长；`protect` 是纯字符串替换 + 还原，无 IO、无不确定性；`font` 是纯映射，与已在白名单的 `js(lib.pure)` 同构——禁掉它们只会逼作者退回 `js`，反而更贵。

实现方式：把该子树编译为受限 IR，落到 `Step::Call{tier:Ctx}` / `Stage::Script` / `Step::Http` / `Step::Cache`，或 `expr:` 含 `now()`·`rand()`·`cookie()`、命名空间为 `token:`·`cookie:` 的引用，各记一次违规：

```
✗ E_COST at stages.toc.out.map.updateTime
   逐章规则禁止调用 lib.ctx；该规则会被求值 1200 次
   hint: 把时间格式化挪到 list 选择器的父规则一次算完
```

`toc.out.list`（每阶段一次）不受此约束。

### 3.7 feature 与算法声明

| 使用 | 必须声明 |
|---|---|
| `lib` / `script` | `features: ["js"]` |
| `expr:now()` / `date` 的 `tz`；`expr:rand()` | `features: ["time"]`；`features: ["random"]` |
| `transform.crypto` / `expr:md5…` | `features: ["crypto"]` + `crypto: [算法名]` |
| `transform.font` | `features: ["font"]` |
| `request.engine: webview` / `webJs` / `sniff` | `features: ["webview"]` |
| `charset` 探测兜底 | `features: ["detect"]` |
| `hostStages.<id>` | 宿主必须声明支持该 id；缺失报 **`E_HOST_STAGE`**（只禁用该入口，不是 `E_UNSUPPORTED`） |

缺声明 → `E_UNSUPPORTED`，错误信息点名缺哪个 feature / 算法。**唯一例外**：`hostStages.<id>` 缺失报 `E_HOST_STAGE`（只禁用该入口，不使整源失效）。

### 3.8 步骤图与跨块契约校验（v2.0 新增）

三处"跨块引用"必须在编译期闭合，否则只能在运行期得到难以定位的失败。

#### 3.8.1 `login.steps` 步骤图

| 检查 | 判据 | 错误码 |
|---|---|---|
| `steps` 与顶层 `fields`/`submit` 互斥 | 二者同时出现（规范 §4.5.2） | `E_SCHEMA` |
| `steps[].id` 必填且唯一 | 缺失/重复使 `next.to` 不可判定 | `E_SCHEMA` |
| `next.to` 存在 | 目标 id 必须落在 `steps[].id` 集合内 | `E_REF`（列出全部合法 id + 最近似建议） |
| 图无环 | 以 step 为点、`next.to` 为边跑 Kahn 拓扑排序；排序后仍有剩余节点 ⇒ 有环 | `E_SCHEMA`（错误里给环路径 `cred → sms → cred`） |
| `next.when` 可判定 | `when` 引用的 `var:` 必须在该步 `ok` 之后可见 | `E_REF` |

拓扑排序顺带产出**执行序**：IR 里 `steps` 存为邻接表 + 入度表，运行期按入度推进；无 `next` 的线性序列零额外开销。

#### 3.8.2 `merge` 契约

| 检查 | 判据 | 错误码 |
|---|---|---|
| `merge.key` 字段存在 | 每个键必须是 `stages.search.out.map` 的声明字段（默认 `["name","author"]`，两者都要在） | `E_REF` |
| `normalize` / `order` 枚举 | ⊆ `trim` `t2s` `lower` `stripPunct` ／ ⊆ `exact` `contains` `weight` `sources` | `E_ENUM` |

源没有 `search` 阶段时跳过 `key` 校验（`merge` 只约束搜索合并）；`explore` 结果的合并策略归宿主。`crossBucket` 缺省 `true`（规范 §12.4 强制全局比较）。

#### 3.8.3 分页与直取

| 检查 | 判据 | 错误码 |
|---|---|---|
| `paginate.kind` 与字段匹配 | `page`→`pageVar`；`next`→`next`；`cursor`→`cursor.take`+`into`；`enumerate`→`enumerate`+`into` | `E_SCHEMA` |
| `pageVar` 与 `enumerate` 互斥 | 同阶段同时出现（规范 §4.7.1） | `E_SCHEMA` |
| `enumerate` 形状 | 字面数组或 `{from,to[,step]}`；有 `joinWith` ⇒ 单请求，否则逐值请求 | `E_SCHEMA` |
| `direct` 与非 `skip` 的 `request` 互斥 | `direct: true` 且 `request` 含除 `skip` 之外的键 | `E_SCHEMA` |
| `reuseDetailHtml` 需要 `detail` | `stages.detail` 不存在 | `E_SCHEMA` |

实现提醒：`direct` 与 `{request:{skip:true}}` **可以同时出现**（规范 §6.6.4 原文示例），判据是"是否存在非 `skip` 的键"，别写成"`request` 存在即报错"。

### 3.9 `rsl lint` 20 条 ↔ 编译阶段

`rsl lint` 不联网，跑的就是 [5] 这套分析器；20 条规则**不应有第二份实现**。归属表：

| # | lint 规则 | 阶段 | 小节 | 错误码 |
|---|---|---|---|---|
| 1 | 选择器必须有命名空间 | [4]+[5] | §3.1 | `E_SELECTOR` |
| 2 | `var:` 引用必须在引用图内 | [5] | §3.2 | `E_REF` |
| 3 | `let:` 必须在同阶段先声明 | [5] | §3.1 | `E_REF` |
| 4 | `range` 与 `pick` 互斥 | [5] | §3.1 | `E_SCHEMA` |
| 5 | `blocks` 与 `paragraphs` 互斥 | [5] | §3.1 | `E_SCHEMA` |
| 6 | `paginate.kind` 与字段匹配 | [5] | §3.8.3 | `E_SCHEMA` |
| 7 | `pageVar` 与 `enumerate` 互斥 | [5] | §3.8.3 | `E_SCHEMA` |
| 8 | `direct` 与非 `skip` 的 `request` 互斥 | [5] | §3.8.3 | `E_SCHEMA` |
| 9 | `login.steps` 与顶层 `fields` 互斥 | [5] | §3.8.1 | `E_SCHEMA` |
| 10 | `login.steps[].next.to` 存在且无环 | [5] | §3.8.1 | `E_REF` / `E_SCHEMA` |
| 11 | 管道类型推断 | [5] | §3.3 | `E_TYPE` |
| 12 | 逐章规则白名单 | [5] | §3.6 | `E_COST` |
| 13 | 正则 RE2 子集 | [5] | §3.5 | `E_REGEX_UNSUPPORTED` |
| 14 | `features` / `crypto` 声明 | [5] | §3.7 | `E_UNSUPPORTED` |
| 15 | `lib:` 函数存在性与档位 | [5] | §3.4 | `E_SCRIPT` |
| 16 | `onResponse` 不得调 `ctx.http` | [5] | §3.4.1 | `E_SCRIPT` |
| 17 | `reuseDetailHtml` 必须有 `detail` | [5] | §3.8.3 | `E_SCHEMA` |
| 18 | `merge.key` 字段必须存在于 `search.out.map` | [5] | §3.8.2 | `E_REF` |
| 19 | 保留字段类型 | [2]+[5] | §3.3 | `E_SCHEMA` |
| 20 | `x-` 之外的未知字段 | [3] | §2 | `W_UNKNOWN_FIELD` |

第 1、19 条在 [2] 能被 Schema 部分覆盖（形状），但只有 [5] 能给出 hint；**错误码以 [5] 为准**，避免同一个源两套错误码。

## 4. 中间表示（IR）

```rust
pub enum Step {
    Select { ns, expr, flags },  Attr { name },  Range { start, end, step },
    Pick { idx },  Exclude { idx },  Where { op },  Map { fields },
    Blocks { mode, min_len },  Paragraphs { split, min_len },
    Trim { chars },  Regex { re, repl, all },  Replace { pairs },  Abs { base },
    Crypto { alg, mode, key, iv },  Font { map, reference },
    Zip { with: Box<Pipeline>, parent: ScopeId, sep: String },   // v2.0：with 在父作用域求值
    Protect { target: ProtectTarget },                          // v2.0：Literal(Vec<String>) | Nodes(Pipeline)
    Call { fn_name, tier, args },  Expr { tree: ExprNode },
}
```

**每个 `Pipeline` 是 `Vec<Step>`**，执行就是顺序 reduce：`fn eval(pipeline: &Pipeline, input: Value, ctx: &Ctx) -> Result<Value>`。`Zip` 携带 `parent: ScopeId`——"`with` 在父作用域求值"就靠它（编译期算好作用域，运行期不回查）；`login.steps` 在 IR 里是 `Vec<StepNode{ id, fields, submit, next }>` + 拓扑序索引表（§3.8.1）。解释器核心循环约 300 行，其余都是各步骤实现——这是控制内核规模的关键。

## 5. 模块拆分与规模

| 模块 | 职责 | 预估行数 |
|---|---|---|
| `model` | serde 结构、Schema 映射 | 1100 |
| `sugar` | 字符串选择器 → 对象、快捷字段展开 | 350 |
| `analyze` | 命名空间/类型/引用/正则/成本/feature/步骤图/merge | **2000** |
| `ir` | IR 定义与降级 | 600 |
| `expr` | `expr:` 词法、Pratt、闭包树 | 450 |
| `selector/*` | `css`(400) `xpath`(400) `json`(350) `regex`(150) `font`(350) | 1650 |
| `exec` | 管道执行器（含 zip / protect） | 800 |
| `http` | 传输抽象 + 重试/限流/cookie/dns/proxy | 1100 |
| `webview` | WebView 端口与嗅探（宿主注入） | 300 |
| `stage` | 五阶段调度、分页（page/next/cursor/enumerate）、步骤推进、上下文、预算 | **1250** |
| `script` | QuickJS 绑定、pure/ctx 两档、确定性计量 | 800 |
| `crypto` | 摘要/对称/HMAC/编码 | 500 |
| `error` | 错误类型与诊断 | 500 |
| `testkit` | fixture 录制、断言、证据等级 | 700 |
| `cli` | 命令行 | 500 |
| **合计** | | **≈ 12600 行** |

**两处上调的理由**（不是通胀，是 v2.0 新增的静态义务）：

- `analyze` 1800 → 2000：新增步骤图拓扑排序、`merge.key` 存在性、分页/`direct` 互斥、`onResponse` 签名与 `ctx.http` 扫描、`zip`/`protect` 类型规则——五类检查；且 §3.9 要求 lint 20 条"每条都能被单测覆盖"，分析器是唯一实现，错误消息构造与表驱动框架都算它的行数。
- `stage` 1100 → 1250：分页从 3 种 kind 变 4 种（`enumerate` 有"拼一次"与"逐值发一次"两条路径），`content.direct` 要按六载体自动生成数组，`reuseDetailHtml` 要跨阶段复用响应并对 `http.rate` 豁免，`login.steps` 要一个步骤推进器（宿主只渲染 `fields` 并回传值）。

合计：v1.1 表内明细求和为 12250（表下标称 ≈12000 是四舍五入的近似），v2.0 为 12600，净增 350 行 ≈ 2.9%。对比 Legado 的 ≈27,400 行内核 Kotlin（Legado 全仓 139k 行）——约 9.1%，差距不是功能少，而是没有 UI 层、没有 Android 生态适配、没有为兼容历史格式付出的开销。

### 依赖选型

| 用途 | crate | 备注 |
|---|---|---|
| HTTP / HTML / XML / JSON / 正则 | `ureq` 或 `reqwest`；`scraper`；`sxd-document`+`sxd-xpath`；`serde_json_path`；`regex` | 全纯 Rust；`regex` 天然满足 §3.5 的 RE2 语义；`tokio` 可选，仅 CLI/服务端 |
| JS / 编码 / 字体 | `rquickjs`；`encoding_rs`；`ttf-parser` | 可编译到 WASM/iOS/Android；GBK/GB18030 必需；字形映射 |
| 加密 | `md-5` `sha2` `aes` `hmac` `des` | 与 `requires.crypto` 一一对应 |

**交叉编译目标**：iOS `aarch64-apple-ios` → staticlib → Swift Package；Android `aarch64-linux-android` → `.so` → JNI；WASM `wasm32-unknown-unknown` → wasm-bindgen。

## 6. 为什么给 Swift 一个薄壳

```
Swift App → import RSLKit（Swift Package，手写约 400 行）
          → C ABI（rsl.h，cbindgen 生成）
          → librsl_core.a（Rust 编译产物）
```

`RSLKit` 只做三件事：内存生命周期、`JSON <-> Swift 结构体`、`async/await` 桥。**所有规则逻辑在 Rust 里，Swift 侧改不到**——这是"多端一致"的前提。

v2.0 的 `login.steps` 让边界更干净：宿主只负责"渲染当前步的 `fields` + 回传值"，步骤推进（含 `next.when` 判定）全在 Rust；登录态隔离键（源 id + 域名 + 登录名）由宿主保管，内核只按键取值。不想引入 Rust 工具链的退路是 WASM + JavaScriptCore，代价是 2~3 倍解析性能与一层沙箱通信开销。

## 7. 验收标准

1. `rsl compile examples/*.rsl.json` → 输出 IR 摘要，0 错误。
2. 下列负例全部在**编译期**报错，错误码与 hint 与规范一致：

| 负例 | 期望 |
|---|---|
| `"name": "li.title"`（缺命名空间） | `E_SELECTOR` |
| `node[]` 上做 `regex` | `E_TYPE` |
| `var:book.urll` | `E_REF` |
| `re:(a)\1` | `E_REGEX_UNSUPPORTED` |
| 逐章规则里调 `lib.ctx` | `E_COST` |
| 用 `md5` 但未声明 `crypto: ["md5"]` | `E_UNSUPPORTED` |
| `lib:` 选择器调 ctx 函数 | `E_SCRIPT` |
| `stages` 缺 `toc` | `E_SCHEMA` |
| `login.steps` 有环（`cred → sms → cred`） | `E_SCHEMA`，hint 给出环路径 |
| `login.steps[].next.to` 指向不存在的 id | `E_REF` |
| `direct: true` 与 `request: {url: …}` 同给 | `E_SCHEMA` |
| `paginate.pageVar` 与 `paginate.enumerate` 同给 | `E_SCHEMA` |
| `onResponse` 里调 `ctx.http` | `E_SCRIPT` |
| `merge.key` 指向 `search.out.map` 里没有的字段 | `E_REF` |
| `zip` 的 `with` 在父作用域解析出标量 | `E_TYPE` |

3. **运行期**负例（fixture 级，不属于编译期，但必须在 `rsl test` 里钉住）：`protect` 占位符丢失（管道里的 `replace`/`slice` 把它吃掉）→ `E_PROTECT_LOST` 且原文一个字不丢；`zip` 两侧长度不等 → `W_ZIP_LENGTH`，结果以短者为准。
4. `rsl run <src> search --kw 测试` 对真实站点返回 ≥1 条结果。
5. 编译产物可 bincode 序列化，二次加载耗时 < 首编的 5%。
6. 500KB HTML、`maxNodes=5000` 下单次解析 < 150ms（iPhone 13 量级）。
7. 同一份源在三端（CLI / WASM / iOS）跑同一 fixture，输出逐字节一致。
