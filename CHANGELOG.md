# 更新日志 (Changelog)

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- **kanyu-core / CLI / MCP**：空间连接与分区统计（补全矢量分析面）——
  - `analysis::sjoin`：空间连接（`SpatialPredicate::Intersects/Contains/Within`；
    **左连接 + 匹配展开**：保留全部 target 要素、无匹配 join 侧缺省、
    一对多各输出一条；属性合并、键冲突加 `join_` 前缀并附 `join_index`
    溯源；O(n·m)，rstar 加速 📋）。
  - `analysis::zonal_stats`：分区统计（`ZonalStat::Count/Sum/Mean/Min/Max`；
    values 按质心/代表点归属 zones 面要素、一值多区取首个匹配、区外值
    计入 `foreign_members.unzoned_count`；统计列命名 `{field}_{stat}`；
    count 同样要求 field 存在且数值）。
  - CLI：`kanyu analysis sjoin/zonal`；MCP：`kanyu_analysis_sjoin`、
    `kanyu_analysis_zonal_stats`（introspect 新增两项 stable）。

## [0.5.0] - 2026-08-02

**米制分析链路打通：投影变换（proj4rs + EPSG 数据库）与测地线度量。**

### 新增

- **kanyu-core / CLI / MCP**：坐标投影工具 + 测地线度量（新模块 `crs`，
  解决 EPSG:4326 数据无法做米制分析的痛点）——
  - `crs::reproject`：投影变换（proj4rs + crs-definitions 内置 EPSG 数据库；
    `"EPSG:xxxx"`/proj4 定义串/`WGS84` 均接受；逐坐标递归转换全几何类型，
    经纬度自动衔接度/弧度，z 不变；失败坐标报中文错误并指出要素序号）。
  - `crs::measure`：测地线度量（geo crate Karney 2013；`MeasureKind::Length`
    线长与面外环周长（米）、`MeasureKind::Area` 面面积（平方米，含洞扣除，
    `Orient` 归一化绕向防 ESRI 顺时针数据算出补集面积）；输出
    total + 逐要素明细 JSON；Point/无几何为 0）。
  - CLI：`kanyu data reproject`、`kanyu analysis measure`；
    MCP：`kanyu_data_reproject`、`kanyu_analysis_measure`
    （introspect 新增两项 stable）。
  - buffer/measure 文档中的"需先投影"警示句统一指向 reproject 工具。

## [0.4.0] - 2026-08-02

**分析内核落地：总规 §4.2.2 的 MCP 分析工具组兑现（裁决 #16 调序前置）。**

### 新增

- **kanyu-core / CLI / MCP**：空间分析工具组（geo crate，总规 §4.2.2 落地，
  裁决 #16 分析内核优先于 UI 壳层）——
  - `analysis::buffer`：缓冲区分析（圆角连接/端帽，segments 控制每象限
    分段数；属性随行；不可转换要素跳过并计入 `foreign_members.skipped`）。
  - `analysis::overlay`：叠加分析（BooleanOps/i_overlay：
    union/intersection/difference/xor；仅面要素，非面报错指出序号；
    target×overlay 逐要素对布尔、未做跨对融合；属性合并、键冲突加
    `overlay_` 前缀；Difference 为连续差、仅带 target 属性）。
  - `analysis::topology_check`：拓扑检查（NoOverlap：面要素两两交集
    面积 > 1e-10 判违规；`TopologyReport` 结构化报告；O(n²) 朴素实现，
    rstar 加速 📋）。
  - CLI 新命令组 `kanyu analysis buffer/overlay/topology`；
    MCP 新工具 `kanyu_analysis_buffer/overlay/topology`（introspect
    三项状态 planned→stable）。
  - 单位警示：distance/面积均为数据 CRS 单位，EPSG:4326 下是度；
    米制分析需先投影（proj4rs 📋）。

## [0.3.0] - 2026-08-02

**GeoArrow 成为内核血液：`Layer` 内存模型迁移至 RecordBatch 列式零拷贝。**

### 新增

- **kanyu-core**：`Layer::batch()` 零拷贝访问底层 GeoArrow RecordBatch
  （WKB 几何列 `geometry` 携带 geoarrow.wkb 扩展元数据 + 类型化属性列
  Int64/Float64/Boolean/Utf8，arrow 58 / geoarrow-schema 0.8）。
- **kanyu-core**：`summary()`/`query()` 改为直接在 RecordBatch 列上求值
  （几何类型读 WKB 头部类型码；谓词逐行求值后经 arrow take 取子集），
  谓词语义与 GeoJSON 载体时代逐比特一致。

### 变更（Breaking）

- **kanyu-core**：`Layer` 内部载体从 `geojson::FeatureCollection` 迁移为
  GeoArrow `RecordBatch`（总规"以 GeoArrow 为血液"落地）。格式解析器在边界
  统一 `collection_to_batch` 入列，导出时 `batch_to_collection` 转回——
  全部格式 I/O 代码零改动，对外行为不变（36 项既有测试全绿）。
  **Breaking**：`Layer::collection()` 签名由
  `fn collection(&self) -> &geojson::FeatureCollection`（只读借用）改为
  `fn collection(&self) -> geojson::FeatureCollection`（按需转换的拥有值）；
  调用方传引用处需加 `&`（CLI/MCP 已适配）。

## [0.2.0] - 2026-08-02

**Phase 1 里程碑：主流矢量文件格式免 GDAL 读写全部达成。**

### 新增

- **kanyu-core / CLI / MCP**：KML 原生读写（kml crate，免 GDAL）——
  Document/Folder 嵌套展平取全部 Placemark，Point/LineString/LinearRing/
  Polygon（含内环洞）/MultiGeometry（同类合并 Multi*、异类 GeometryCollection）
  映射；name/description/ExtendedData（Data/SimpleData/SchemaData）写为同名属性
  （值按 CSV 规则数值化）。写出全六类型（Multi*→MultiGeometry），属性除
  name/description 外入 ExtendedData/SimpleData。KMZ（zip 容器）返回待集成
  结构化错误。`Layer::to_kml_string`、CLI/MCP export `-f kml` 可用。
  至此 Phase 1 主流矢量文件格式（shp/geojson/fgb/geoparquet/dxf/kml/csv）
  免 GDAL 读写全部达成；gpkg/spatialite/postgis/wfs 按总规裁决 #15
  走可选 feature 插件路线。
- **kanyu-core / CLI / MCP**：DXF 原生读写（dxf crate，免 GDAL）——
  POINT/LINE/LWPOLYLINE/POLYLINE 映射（闭合折线→Polygon），CIRCLE/ARC 按 64 分段
  折线近似，其余实体跳过不报错；要素带 `layer`（图层名）与非 ByLayer 时的
  `color_index`（ACI）属性。写出为 R2000 LWPOLYLINE（Polygon 仅外环、洞舍弃、
  属性/XDATA 📋、z 丢弃），统一落图层 "0"。`Layer::to_dxf_string`、
  CLI/MCP export `-f dxf` 可用。
- **kanyu-core / CLI / MCP**：GeoParquet 原生读写（geoparquet crate，免 GDAL）——
  几何列按 GeoParquet 1.x 规范 WKB 编码（列名 `geometry`，自研小端 2D WKB
  编解码，六类基础几何 + GeometryCollection 可往返），geo 元数据/geometry_types/
  bbox 由 geoparquet crate 编码器生成；属性列 schema 推断规则与 FGB 一致，
  Arrow 列类型原生映射 JSON 类型（不支持的列类型读侧报中文错误，拒绝静默丢列）。
  `Layer::to_geoparquet_bytes`、CLI/MCP export `-f geoparquet` 可用。
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

[Unreleased]: https://github.com/DaoMingyuan/Kanyu/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.5.0
[0.4.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.4.0
[0.3.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.3.0
[0.2.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.2.0
[0.1.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.1.0
