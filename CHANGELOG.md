# 更新日志 (Changelog)

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- **kanyu-core / CLI / MCP**：FlatGeobuf 原生读写（flatgeobuf crate，免 GDAL）——
  读取逐要素转换（几何经 geozero `ToJson`、属性经自研 PropertyProcessor），
  FGB 列类型原生映射为 JSON 类型；
  写出列 schema 自动推断（String→String、整数→Long、浮点→Double、Bool→Bool，
  混合类型列退化为 String），单一几何类型按声明写出、混合几何按 Unknown 异构声明，
  Hilbert 空间索引 + EPSG:4326 CRS 声明。`Layer::to_fgb_bytes`、
  CLI/MCP export `-f fgb` 可用。
- **kanyu-core**：Shapefile 原生读取（shapefile crate，免 GDAL）——Point/MultiPoint/
  Polyline/Polygon（含 M/Z 变体与带洞多边形），dbase 属性类型化为 JSON
  （Numeric/Float→Number、Character→String、Logical→Bool、Date/DateTime/Memo→String、
  空值跳过）；缺少 .dbf 侧边文件时返回中文结构化错误。写出待 Phase 1 后续。
- **kanyu-core**：CSV/TSV 原生加载，坐标列自动识别（lon/lat、longitude/latitude、
  x/y、经度/纬度），其余列自动作为属性（数值单元格转为 JSON 数值）。
  xlsx 暂返回结构化错误提示（待 calamine 集成）。
- **kanyu-core / CLI / MCP**：CSV 导出（`x,y` 坐标列 + 属性字段并集），
  加载→查询→导出 CSV 闭环。

### 修正

- `kanyu introspect` 工具清单改为真实 MCP 表面名（`kanyu_data_load` 等），
  并补登 agents 组两个已实现工具——自省必须描述现实。
- MCP `ServerInfo` 上报为 `kanyu-mcp` + 内核版本（此前为 SDK 默认值）。
- DWG 能力矩阵诚实化：write/edit 降为 Partial，driver 改为 `libredwg-wasm`，
  note 对齐总规 §6.1 裁决（沙箱只读、写 ≤r2004、现代版本以 DXF 替代）。
- `agents validate --json` 输出改为 pretty JSON，与其他 `--json` 命令一致。
- workspace MSRV 声明修正为 1.88（与 rmcp 3.x 要求一致）。
- 文档：新增 ARCHITECTURE/API/SDK/MCP/CLI 五份接口文档；
  README 架构图补 kanyu-shell 模块。

## [0.1.0] - 2026-08-01

首个公开版本：堪舆的地基。

### 新增

- **kanyu-core**：统一格式注册表（17 种格式的读/写/编辑/符号/布局能力矩阵）、
  图层模型（GeoJSON 原生加载、`LayerSummary` 概要、属性谓词查询
  `field op value`）、`AGENTS.md` 语义文件解析/校验/模板生成、系统自省报告。
- **kanyu-cli**：`kanyu data info/load/query/export`、`kanyu introspect`、
  `kanyu agents init/validate`、`kanyu mcp serve`，全局 `--json` 输出。
- **kanyu-mcp**：基于官方 `rmcp` 3.x 的 stdio MCP Server，6 个确定性工具
  （`kanyu_data_load/query/export`、`kanyu_system_introspect`、
  `kanyu_agents_init/validate`），结构化 JSON 输出。
- **文档**：总规、架构、API、SDK、MCP、CLI 全套文档；开源社区文件齐备。
- 双许可 MIT OR Apache-2.0。

### 已知限制

- 原生 I/O 仅 GeoJSON；FlatGeobuf/GeoParquet/Shapefile/DXF 在 Phase 1 后续迭代。
- 分析/渲染/编辑工具组（buffer/overlay/topology/render）为规划状态。
- MCP 仅 stdio 传输；streamable HTTP 与 MCP tasks 长任务待 v0.2。

[Unreleased]: https://github.com/DaoMingyuan/Kanyu/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.1.0
