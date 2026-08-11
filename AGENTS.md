# AGENTS.md —— 堪舆 (Kanyu) 仓库

> 本文件是 AI 代理在本仓库工作的"罗盘"。遵循 [agents.md](https://agents.md) 开放规范。
>
> **⚠️ 强制前置**：开始任何工作前，先阅读并遵守根目录 **[AI_SYNC.md](AI_SYNC.md)**
> （长久性联动机制：开工登记、收工回记、状态快照、自我迭代边界）。

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
| `crates/kanyu-skill/` | 技能：WASM 插件宿主（wasmtime 沙箱 + WIT 组件模型 ABI + fuel 配额） |
| `crates/kanyu-cli/` | `kanyu` 命令行（clap derive） |
| `crates/kanyu-shell/` | 桌面壳层（egui）：Ribbon/面板/终端/画布；**`src/ui_kit/` = UI 组件规范库** |
| `crates/kanyu-mcp/` | MCP Server（rmcp 3.x，stdio + streamable HTTP，SEP-2663 长任务） |
| `crates/kanyu-shell/` | 壳层：egui 桌面 UI（TitleBar/图层面板/MapCanvas/StatusBar/双主题，截图验证模式） |
| `docs/` | 总规 + 架构/API/SDK/MCP/CLI 文档 |
| `AI_SYNC.md` | **长久性联动机制**（开工登记/收工回记/状态快照/迭代边界）——先于一切阅读 |
| `examples/` | 示例数据（GeoJSON） |
| `tests/` | 跨 crate 集成测试 |

## 不可逾越的约定

1. **依赖方向**：`kanyu-core` 不依赖兄弟 crate；render/skill 依赖 core；cli/mcp 依赖 core+render+skill；shell 依赖 core+render+skill。
2. **内核零 C 依赖**：GDAL/GEOS/LibreDWG 只能以可选 feature 或 WASM 插件存在；
   默认构建必须在三大桌面平台开箱通过。
3. **单一事实来源**：模块清单、MCP 工具清单、格式矩阵只写在 `kanyu-core` 代码中
   （`format.rs`、`introspect.rs`），文档引用代码而非复制。
4. **无 execute_code**：MCP 接口永不暴露任意代码执行工具（安全基线，见 SECURITY.md）。
5. **无冗余文件**：新文档优先扩展现有文件；不留 `.bak`、临时输出、重复文档。
6. 代码注释与文档用中文；标识符用英文；提交信息用 Conventional Commits。
7. **联动协议**：开工先在 [AI_SYNC.md](AI_SYNC.md) 会签簿登记，收工回记并同步状态快照；
   自我迭代只发生在 GitHub 协作层（提交/PR + CI + 审核），运行时绝不自改内核（§1.3）。
8. **UI 组件铁律**（kanyu-shell）：新界面元素**先查 `crates/kanyu-shell/src/ui_kit/`**
   已有组件再调用；确无组件时按 `ui_kit/mod.rs` 的分类标准（tokens/controls/containers/icons）
   新建可复用组件入 kit，禁止业务代码一次性手搓样式（色值/字号/间距只许出自
   `theme::palette` 与 `ui_kit::tokens`）。UI 改动须对照 `ui_kit/mod.rs`
   「设计审查规范」清单（design-review 技能沉淀：层级/间距/文本/色彩/交互/
   三分离/AI slop 黑名单）。

## AI 工作流

- **改格式矩阵**：编辑 `crates/kanyu-core/src/format.rs` → 同步 `docs/API.md` 与 README 能力表。
- **加 MCP 工具**：在 `crates/kanyu-mcp/src/server.rs` 的 `#[tool_router]` 块中添加 →
  在 `introspect.rs::tools()` 登记 → 更新 `docs/MCP.md`。
- **加 CLI 命令**：`crates/kanyu-cli/src/cli.rs` 定义 + `commands.rs` 实现 → 更新 `docs/CLI.md`。
- **加 UI 图标**：`ui_kit/icons.rs` 的 `Icon` 枚举加变体（含手绘 `draw` 分支）→
  在 `arcgis_resource_name()` 登记 ArcGIS Pro 资源名映射（单一事实来源）→
  调用方一律走 `draw_or_image()`（位图优先、手绘回退双轨）。
  **许可边界**：ArcGIS Pro 位图 PNG 仅存在用户本机 `%LOCALAPPDATA%\Programs\kanyu\icons\`
  （light/dark 双主题），**不得提交进仓库再分发**；仓库内只保留手绘图标与映射表，
  克隆环境自动回退手绘，功能不受影响。

## 自定义工具

- `kanyu introspect`：输出本仓库内核的模块/工具/格式矩阵（AI 读取自身）。
- `kanyu agents validate --path AGENTS.md`：校验本文件完整性。
