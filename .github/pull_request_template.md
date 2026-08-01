## 变更说明

<!-- 这个 PR 做了什么？为什么？关联 Issue： Fixes # -->

## 检查单

- [ ] `cargo fmt --all` / `cargo clippy -- -D warnings` / `cargo test --workspace` 全绿
- [ ] 新功能包含测试
- [ ] 公共 API 有 rustdoc 注释
- [ ] 已同步更新 `introspect.rs` 单一事实来源与 `docs/` 对应文档（如涉及）
- [ ] CHANGELOG.md `Unreleased` 段已更新
- [ ] 未引入 C 依赖到内核默认构建（GDAL/GEOS/LibreDWG 仅可选 feature 或 WASM 插件）
