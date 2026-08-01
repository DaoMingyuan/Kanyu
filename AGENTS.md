# AGENTS.md —— 堪舆 (Kanyu) 仓库

> 本文件是 AI 代理在本仓库工作的"罗盘"。遵循 [agents.md](https://agents.md) 开放规范。

## 项目元数据

- **name**: 堪舆 (Kanyu) —— AI 原生地理空间操作系统
- **crs**: 不适用（本仓库是软件工程仓库；地理项目的 AGENTS.md 规范见 `docs/MASTERPLAN.md` §4.3.2）
- **author**: 道明远 (DaoMingyuan)
- **created**: 2026-08-01

## 构建与验证命令

```bash
cargo build --workspace          # 构建
cargo test --workspace           # 全部测试（提交前必须通过）
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## 仓库结构

| 路径 | 职责 |
|------|------|
| `crates/kanyu-core/` | 内核：格式注册表、图层模型、AGENTS.md 语义、系统自省 |
| `crates/kanyu-render/` | 眼睛：离屏地图渲染（SVG 零依赖 + tiny-skia PNG，晨山/夜观星主题） |
| `crates/kanyu-cli/` | `kanyu` 命令行（clap derive） |
| `crates/kanyu-mcp/` | MCP Server（rmcp 3.x，stdio + streamable HTTP，SEP-2663 长任务） |
| `docs/` | 总规 + 架构/API/SDK/MCP/CLI 文档 |
| `examples/` | 示例数据（GeoJSON） |
| `tests/` | 跨 crate 集成测试 |

## 不可逾越的约定

1. **依赖方向**：`kanyu-core` 不依赖兄弟 crate；render 依赖 core；cli/mcp 依赖 core+render。
2. **内核零 C 依赖**：GDAL/GEOS/LibreDWG 只能以可选 feature 或 WASM 插件存在；
   默认构建必须在三大桌面平台开箱通过。
3. **单一事实来源**：模块清单、MCP 工具清单、格式矩阵只写在 `kanyu-core` 代码中
   （`format.rs`、`introspect.rs`），文档引用代码而非复制。
4. **无 execute_code**：MCP 接口永不暴露任意代码执行工具（安全基线，见 SECURITY.md）。
5. **无冗余文件**：新文档优先扩展现有文件；不留 `.bak`、临时输出、重复文档。
6. 代码注释与文档用中文；标识符用英文；提交信息用 Conventional Commits。

## AI 工作流

- **改格式矩阵**：编辑 `crates/kanyu-core/src/format.rs` → 同步 `docs/API.md` 与 README 能力表。
- **加 MCP 工具**：在 `crates/kanyu-mcp/src/server.rs` 的 `#[tool_router]` 块中添加 →
  在 `introspect.rs::tools()` 登记 → 更新 `docs/MCP.md`。
- **加 CLI 命令**：`crates/kanyu-cli/src/cli.rs` 定义 + `commands.rs` 实现 → 更新 `docs/CLI.md`。

## 自定义工具

- `kanyu introspect`：输出本仓库内核的模块/工具/格式矩阵（AI 读取自身）。
- `kanyu agents validate --path AGENTS.md`：校验本文件完整性。
