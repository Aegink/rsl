# 第三方组件与数据声明（THIRD-PARTY-NOTICES）

本仓库基于 **AGPL-3.0-only** 分发。以下第三方组件/数据归属于其各自许可，本项目未复制其源码到本仓库（对比分析以文档形式引用行号证据，未搬运实现）。

## Rust 依赖（crates）

| 组件 | 用途 | 许可 |
|---|---|---|
| serde / serde_json | 序列化 | MIT OR Apache-2.0 |
| rquickjs | QuickJS 引擎绑定（默认脚本引擎） | MIT |
| boa_engine | Boa 引擎（回退脚本引擎） | MIT OR Apache-2.0 |
| ureq | HTTP 传输（CLI net feature） | MIT OR Apache-2.0 |
| scraper / lol_html | HTML/CSS 解析 | MIT OR Apache-2.0 |
| jsonpath-rust | JSONPath（RFC 9535） | MIT OR Apache-2.0 |
| roxmltree | XPath 子集的文档模型 | MIT OR Apache-2.0 |
| aes / cbc / des / hmac / sha2 / md-5 | crypto 家族 | MIT OR Apache-2.0 |
| base64 / hex | 编码 | MIT OR Apache-2.0 |
| encoding_rs | charset 判定六步 | MIT OR Apache-2.0 |
| notify | 文件监听（test --watch / serve --watch） | MIT OR Apache-2.0 |
| bincode | IR 往返 | MIT |
| wasm-bindgen / js-sys | WASM 目标 | MIT OR Apache-2.0 |

完整依赖树以 `Cargo.lock` 为准；各 crate 的许可以其仓库声明为准（MIT / Apache-2.0 双许可为主）。

## 数据资源

| 资源 | 来源 | 许可 |
|---|---|---|
| OpenCC 简繁词表（Characters + Phrases 全表，编译进 `rsl-core/src/data/`） | [OpenCC](https://github.com/BYVoid/OpenCC) | Apache-2.0 |
| RFC 9535（JSONPath） | IETF | 引用规范 |

## 对比分析引用

| 对象 | 性质 | 说明 |
|---|---|---|
| [gedoor/legado（阅读）](https://github.com/gedoor/legado) | Apache-2.0 开源项目 | `spec/KERNEL-COMPARE.md` 对比分析引用其公开源码行号证据，未复制任何实现代码；Legado 源格式映射表见 `spec/MIGRATION.md`（字段名引用属规范文档） |

## 本仓库明确不包含

- 任何真实站点书源数据（含规则文本、签名密钥、设备标识；真实站点调试语料均在 `scratch/`，已被 `.gitignore` 排除且不随发布分发）
- 任何站点的逆向密钥常量（测试中的加密 key 为自造测试值）

## 测试值声明

`crates/rsl-core/tests/webjs_bridge.rs` 中的 AES key（`0011223344556677`）与明文均为**自造测试值**，仅用于加解密往返回归。
