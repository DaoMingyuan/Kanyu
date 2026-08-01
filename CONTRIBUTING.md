# 贡献指南 (Contributing to Kanyu)

感谢你对堪舆的兴趣！本指南帮助你高效地参与贡献。

## 行为准则

参与本项目即表示同意遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 开发环境

- Rust **1.88+**（见 `rust-toolchain.toml`），推荐通过 [rustup](https://rustup.rs) 安装。
- 克隆后构建与测试：

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## 提交前检查单

- [ ] `cargo fmt --all` 已运行
- [ ] `cargo clippy -- -D warnings` 零警告
- [ ] `cargo test --workspace` 全绿；新功能附带单元测试
- [ ] 公共 API 有 rustdoc 注释（`///`），中文撰写
- [ ] 若改动了模块清单 / 工具清单 / 格式矩阵，同步更新
      `crates/kanyu-core/src/introspect.rs`（单一事实来源）与 `docs/` 中对应文档
- [ ] CHANGELOG.md 的 `Unreleased` 段记录了你的改动

## 架构约定（重要）

1. **依赖方向**：`kanyu-core` 不依赖任何兄弟 crate；`kanyu-cli` / `kanyu-mcp` 只依赖 `kanyu-core`。
2. **内核零 C 依赖**：GDAL / GEOS / LibreDWG 等只能以可选 feature 或 WASM 插件形式存在，
   默认 `cargo build` 必须在 Windows / macOS / Linux 上开箱通过。
3. **单一事实来源**：模块清单、MCP 工具清单、格式矩阵只定义在 `kanyu-core` 的代码中，
   文档从代码生成或引用，不允许两处手写同一清单。
4. **无冗余文件**：新增文档前先检查 `docs/` 是否已有对应文件可以扩展。

## 提交信息

采用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/)：

```
feat(core): 新增 FlatGeobuf 原生读取
fix(cli): 修复 query 对空属性要素的崩溃
docs(mcp): 补充 kanyu_data_query 输出示例
test(core): 覆盖 AGENTS.md 缺 CRS 的校验分支
```

## Pull Request 流程

1. Fork 并创建特性分支：`git checkout -b feat/xxx`
2. 保持 PR 聚焦单一目的；大改动先开 Issue 讨论
3. 填写 PR 模板，CI 全绿后请求 review

## 报告安全问题

请勿公开开 Issue，按 [SECURITY.md](SECURITY.md) 流程私下报告。
