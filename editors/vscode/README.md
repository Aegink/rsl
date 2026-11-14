# rsl-vscode — RSL 书源语言扩展

RSL（`.rsl.json`）的 VS Code 支持：**诊断 / hover / 补全 / 定义 / 快速修复**，
全部能力来自 [rsl-lsp](../../crates/rsl-lsp)（一语言服务多 IDE，LSP-for-DSL 模式）。

## 功能

| 能力 | 说明 |
|---|---|
| 实时诊断 | 编译期 36 码体系直推编辑器（E_REF 笔误定位到 JSON key） |
| hover | `var:book.url` 显示来源契约；命名空间显示语义文档 |
| 补全 | 18 命名空间（`css:`/`json:`/…）+ transform 内 31 管道步骤键 |
| 定义跳转 | `var:x` → 声明处 |
| 快速修复 | 确定性 fix（如 `cs:` → `css:`、`var:book.urll` → `var:book.url`），应用后诊断消失 |
| Schema 校验 | `schema/rsl.schema.json` 全量 $defs（jsonValidation 内建） |
| snippets | 五阶段骨架 / 分页四模式 / 登录（表单 + WebView）/ 合并 / 缓存 |

## 安装（开发态）

```bash
cargo build -p rsl-lsp          # 产出 target/debug/rsl-lsp(.exe)
cd editors/vscode
npm test                        # 协议冒烟 6/6（零依赖，直接驱动真实 server）
```

VS Code 内：把本目录放入 `%USERPROFILE%\.vscode\extensions\rsl-spec.rsl-vscode-2.0.0`
（或 `code --install-extension` 打包后的 .vsix，打包需 `npx @vscode/vsce`）。

设置项：`rsl.lspPath`（默认 `rsl-lsp` 取 PATH；开发时可指
`../../target/debug/rsl-lsp.exe` 的绝对路径）。

## 协议测试（本目录验收）

`npm test` = `node test/protocol-smoke.js`：
用与 extension.js 相同的传输层（`lib/rpc.js`，零依赖手写 JSON-RPC）
驱动真实 rsl-lsp 二进制，覆盖 6 场景：

1. initialize 能力宣告
2. didOpen → E_REF 诊断（带 fix data）
3. codeAction → 快速修复 offer
4. 应用 edit → didChange → 诊断消失
5. 命名空间补全 18 项
6. shutdown 干净退出

## 手工验收清单（H1，需桌面环境）

> 2026-11-14 实测（自动化键鼠 + 截屏，VS Code + --extensionDevelopmentPath，
> 演示文件 编辑器内置示例源 含 `var:book.urll` 笔误）：

- [x] 打开源 → 诊断出现：E_REF **精确定位到笔误 token**（行 45 列 17，问题面板含 hint）
      ——[`docs/images/20261114_vscode_h1_诊断.png`](../docs/images/20261114_vscode_h1_诊断.png)
- [x] 快速修复灯泡 → "替换为 var:book.url（最近似可见引用）" 菜单
      ——[`docs/images/20261114_vscode_h1_快速修复菜单.png`](../docs/images/20261114_vscode_h1_快速修复菜单.png)
- [x] 应用 → **0 错误 0 警告**（诊断消失）
      ——[`docs/images/20261114_vscode_h1_快速修复应用后.png`](../docs/images/20261114_vscode_h1_快速修复应用后.png)
- [x] 选择器字符串内 `:` 触发 → 18 命名空间补全（含语义文档）
      ——[`docs/images/20261114_vscode_h1_补全.png`](../docs/images/20261114_vscode_h1_补全.png)
- [ ] hover 显示来源契约 / transform 内步骤键补全（同类机制，协议冒烟已覆盖；截图留待）
- [ ] snippets：`rsl-source` 前缀展开五阶段骨架，保存后过 `rsl lint`（留待）

## 边界

- 传输层手写（无 vscode-languageclient 依赖）：与 rsl-lsp 的 server 端同风格，
  帧协议 = LSP 3.17 基本子集（Content-Length + JSON-RPC）。
- `language-configuration.json` 按 JSON 语义配置（注释/括号/缩进规则）。
