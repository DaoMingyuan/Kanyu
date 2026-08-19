# AGENTS.md —— 堪舆 (Kanyu) 仓库

> 本文件是 AI 代理在本仓库工作的"罗盘"。遵循 [agents.md](https://agents.md) 开放规范。
>
> **⚠️ 强制前置**：开始任何工作前，先阅读并遵守根目录 **[AI_SYNC.md](AI_SYNC.md)**
> （长久性联动机制：开工登记、收工回记、状态快照、自我迭代边界）。

## 项目元数据

- **name**: 堪舆 (Kanyu) —— AI 原生地理空间操作系统
- **crs**: 不适用（本仓库是软件工程仓库，无地理投影；地理项目的 crs 规范见 docs/MASTERPLAN.md §4.3.2）
- **data-layer**: 否（本仓库为纯软件工程，无 GIS 地理数据层，校验对数据层语义表与图层关键字段均免检；地理项目的 AGENTS.md 规范见 docs/MASTERPLAN.md §4.3.2，本行把适用边界显式化为元数据，kanyu agents validate 依「显式声明优先于 crs 占位」即免检，详见本文件「校验契约」节）
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
| `crates/kanyu-shell/` | 桌面壳层（egui）v0.9：声明式命令注册表（`commands.rs`，DAML 范式投影）/ Ribbon 功能区（动画）/ `dock.rs` 三区停靠（跨区回流+滚动契约）/ Contents 图层树（`toc.rs` 复选框+分组+符号化分类展开行）/ 图层属性页 + `symbology.rs` 符号化（单色/唯一值/分级，按层渲染，入 .kyu）/ 中央视图停靠区（`mapview.rs` 页签吸附+纯白画布）+ `scene3d.rs` 实验 3D / `catalog.rs` 工程目录五分类 / `toolbox/`（参数类型对齐 ArcGIS Python 工具箱规范，注册表在 `core::tooldef`）/ `attrtable.rs` 属性表（字段计算器）/ `edit.rs` 编辑会话（顶点/绘制/单元格）/ `services.rs` 服务链接（WFS 发现/WMS 底图）/ `layoutview.rs` 布局页签 / `settings.rs`（坐标系全库/渲染/界面缩放）/ 终端 / AI 对话 / **`src/ui_kit/` = UI 组件规范库**（WCAG 对比度/24px 目标/tokens::state 状态色强制，截图验证模式） |
| `crates/kanyu-edit/` | 编辑内核：Undo/Redo 框架（命令逆操作双栈）+ 基础编辑命令（GeomPath 三级定位） |
| `crates/kanyu-mcp/` | MCP Server（rmcp 3.x，stdio + streamable HTTP，SEP-2663 长任务） |
| `docs/` | 总规 + 架构/API/SDK/MCP/CLI/GIS_MODE 文档 |
| `dsh/` | 堪舆 GIS × DeepSeek Harness 组件源：`plugin/`（Host+Client 双半，七大能力 + 9 个 kanyu_* 动态工具）/ `pkg/`（常驻静态插件适配器，装本机 web profile）/ `skills/`（WASM 技能 guest 源 + 组件化 .wasm）/ `presets/kanyu-gis/`（GIS 模式 preset + 领域技能）/ `tools/verify_preset.mjs` / `tools/test_plugin.mjs`（269 断言本地测试器）/ `sync-preset.sh`（同步本机安装区）；手册见 `docs/GIS_MODE.md` |
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
- **加工具箱工具**：`crates/kanyu-core/src/tooldef.rs` 的 `TOOLS` 注册表加 `ToolDef`
  （分类/中文名/参数表）→ 内核算法在 kanyu-core（`analysis`/`geoprocess`/`crs`）→
  `toolrun.rs` 的 `run_tool` 加分支 → 壳层/CLI/Python 自动可见 → 截图验证
  （可用隐藏参数 `--tool-demo` 预设参数对话框）。

- `kanyu introspect`：输出本仓库内核的模块/工具/格式矩阵（AI 读取自身）。
- `kanyu agents validate --path AGENTS.md`：校验本文件完整性。
- `kanyu agents validate --path AGENTS.md --code-repo`：按**软件工程仓库**语境
  校验（钉死免检数据层语义表）；等价于上方元数据中 `data-layer: 否` 的显式宣言的
  CLI 直连形态。
  **更正（2026-08-15 复核）**：早期版误写为「`--path AGENTS.md --code-repo`」——
  `--code-repo`（`--check-code-repo` 别名）为**无值旗标**，`validate` 的用法
  `agents validate [OPTIONS]` 不含位置参数，照旧命令原样复跑会报
  `unexpected argument 'AGENTS.md'`（exit 2）。本仓库 AGENTS.md 的权威路径即仓库根的
  `./AGENTS.md`，故**零路径参数** `kanyu agents validate --code-repo`（在仓库根目录
  执行）即可精确定位本文件并按代码仓库语境免检，无需重复 `--path`。零参
  `validate`（不带任何旗标）则走下文「校验契约」的自动裁决，二者择一。
- `kanyu agents validate`（`--check-code-repo` 旗标）：校验时**以显式 `data-layer`
  元数据行为最高优先语境裁决**（见下「校验契约」），零参即可让地理项目与代码
  仓库各自通过，无需调用方再选语境。
- `kanyu agents init <目录> --geo [crs]` / `--code-repo [crs]`：在目标目录生成
  AGENTS.md 模板并打印校验（`--geo` 含数据层语义表骨架；`--code-repo` 为
  软工程式骨架 + `crs` 占位 + `data-layer: 否`）。

## 校验契约

`kanyu agents validate` 对「数据层语义表必填」与「每层关键字段非空」的语境裁决，
由 `AgentsMd::resolve_data_layer` 按**优先级**执行：

1. **元数据行** `- **data-layer**: 是/否` 最高优先（`是` → 必填；`否` → 免检）。
   这是**代码/软件仓库**声明「无 GIS 数据层、校验免检」的权威途径——见本文件
   元数据的 `- **data-layer**: 否（…）` 行。
2. 未显式声明时，回退 **crs 占位**（`resolve_crs`）：真实编码 → 地理项目 →
   语义表必填；`不适用`/`N/A` 占位或缺失 → 代码仓库 → 免检（**软告警**，不阻断
   通过）。

零参 `validate` / `--check-code-repo` 据此自动裁决，地理与代码两类仓库**均可一次
通过、零手工**；仍可用 `validate_code_repo`（`--code-repo`）显式钉死代码仓库语境
。本仓库 `AGENTS.md` 已写 `data-layer: 否`，故**校验必不失败**（即便 crs 行仍含
`不适用` 占位，也无矛盾）。
