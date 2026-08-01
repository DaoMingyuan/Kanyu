# 更新日志 (Changelog)

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- **kanyu-core / CLI / MCP**：KMZ 支持（kml 的 zip 容器，zip crate
  default-features=false + deflate 纯 Rust 后端）——`.kmz` 内存解包
  （doc.kml 优先、否则首个 .kml 条目，多 KML 条目取首个注释即契约），
  zip 损坏/无 .kml 条目中文结构化错误；`Layer::to_kmz_bytes`
  （doc.kml 单条目 deflate）。CLI/MCP export `-f kmz` 分流
  （kmz 为 kml 容器变体非独立格式条目，按 format 值区分 kml/kmz）。
  format.rs kml 条目 note：KMZ 📋→✅。

## [0.12.0] - 2026-08-03

**AI 代理远程热加载基因：kanyu_system_hotload 实质化 + gene_run/gene_list。**

### 新增

- **kanyu-mcp**：MCP 基因热加载接线（`kanyu_system_hotload` 从 planned
  变为真实工具，AI 代理可远程加载并执行 WASM 基因）——
  KanyuServer 持有基因注册表（内存态 `Arc<Mutex<GeneRegistryState>>`，
  Clone 共享，与 TaskManager 同模式；重启即丢）：
  - `kanyu_system_hotload(wasm_path)`：编译校验 + 实例化 + 元数据校验
    （hotload 即"验证"职责，**校验失败绝不注册**），返回 gene_id/meta，
    重名覆盖并返回 `replaced: true`；
  - `kanyu_gene_run(gene_id, path)`：已注册基因在数据文件上沙箱执行
    （FeatureCollection 进/出），未知 gene_id 中文错误提示先 hotload；
    加入任务化白名单（`task: true` 可异步执行，与分析工具同待遇）；
  - `kanyu_gene_list()`：注册表快照（gene_id/version/capabilities）。
  introspect：`kanyu_system_hotload` planned→stable；新增 gene 组
  （`kanyu_gene_run`/`kanyu_gene_list` stable）。v0.1 基因调用锁内串行化
  （注释即契约；按名细粒度锁 📋）。

## [0.11.0] - 2026-08-03

**Phase 5 魂启幕：WASM 基因系统宿主（wasmtime + WIT 组件模型，fuel 沙箱）。**

### 新增

- **kanyu-gene / CLI**：Phase 5「魂」启幕——WASM 基因系统宿主（总规 §4.5
  "以 WASM 为基因"落地，新 crate `kanyu-gene`）：wasmtime 47 组件模型
  + WIT 强类型 ABI（`wit/gene.wit`：`meta() -> string`、
  `run(string) -> result<string, string>`，FeatureCollection JSON 进/出）；
  沙箱无 WASI 导入（纯计算）+ fuel 配额（10 亿/次执行，耗尽即 trap；
  无 IO 挂起故不设墙钟超时，注释即契约）。`GeneHost::load`（编译校验 +
  实例化 + meta() 元数据校验）与 `run`（每次执行重置 fuel），
  LoadFailed/MetaInvalid/Trap/Timeout/ResultInvalid 五类中文结构化错误。
  样板分析基因 `attr_scaler`（真 Rust guest：wit-bindgen 0.60
  `generate!`/`export!`，height ×2；wasm32-unknown-unknown 核心模块 +
  `wasm-tools component new` 组件化，fixture 提交于 testdata/）。
  CLI 新命令组 `kanyu gene info/run`。
  introspect：kanyu-gene 模块 planned→incubating。
  **MSRV 1.88 → 1.94**（wasmtime 47 要求）。
  MCP 热加载接线（kanyu_system_hotload 实质化）与 libredwg-wasm 基因 📋。

## [0.10.0] - 2026-08-02

**属性驱动符号化：graduated 分级设色 / categorical 分类符号（总规 §3.4 落地）。**

### 新增

- **kanyu-render / CLI / MCP**：属性驱动符号化（总规 §3.4 符号系统第一块，
  裁决 #17：并入 `kanyu_render_map` 的 `style` 参数而非独立工具）——
  `StyleRule` 两型（JSON `type` 判别）：`graduated`（数值字段分档，
  `stops: [[阈值, "#RRGGBB"], …]` 严格升序，取最后满足 值≥阈值 的档，
  恰等阈值取该档、低于首档走默认）与 `categorical`（字符串字段类别映射，
  无匹配取 `default`）。字段缺失/类型不符的要素走主题默认样式（不产生
  脏样式）；样式决策仍集中于 `style_for`/`effective_style` 一处；
  命中色按几何类型派生（面=该色 20% 透明填充+同色描边、线=该色描边、
  点=该色填充）。空 stops/非升序/坏 hex 报中文错误并指出出错项。
  CLI `render map --style/--style-file`（互斥中文报错）；MCP `style`
  参数 JSON 原样透传。introspect：移除 `kanyu_render_symbolize`
  （`kanyu_render_camera` 保持 planned）。无 style 调用行为完全不变。

## [0.9.0] - 2026-08-02

**Phase 2 视界启幕：离屏地图渲染 render_map（PNG/SVG 双通道），AI 代理能"看见"数据。**

### 新增

- **kanyu-render / CLI / MCP**：Phase 2「视界」启幕——离屏地图渲染
  `kanyu_render_map`（新 crate `kanyu-render`，依赖方向更新为
  core ← render ← cli/mcp）：SVG 纯字符串零依赖生成（同时兑现注册表
  svg 导出 write:Full 承诺）、PNG 经 tiny-skia 纯 Rust CPU 光栅化
  （CI 无 GPU 依赖；wgpu 实时管线留给交互壳层 kanyu-shell）。
  色彩取自总规 §1.2：晨山（米白 #F0EDE8/墨黑/远黛青 #2D6A5E）与
  夜观星（极暗 #0D0F12/月白/青玉 #4DB8A8）双主题，点/线/面样式
  集中于 `style_for` 单一事实来源；bbox 等比自适应（y 翻转、居中、
  padding，单点/空集合退化安全）。CLI 新命令组 `kanyu render map`
  （格式按扩展名判定）；MCP `kanyu_render_map`——PNG 时 content 携带
  base64 image/png + 摘要文本，SVG 时携带 SVG 源码，structuredContent
  恒带 feature_count/bbox/尺寸/主题/格式摘要（AI 代理可直接"看见"数据）。
  introspect：kanyu-render 模块 planned→incubating，新增
  `kanyu_render_map`（render 组 stable）。

## [0.8.0] - 2026-08-02

**长任务一等特性：SEP-2663 协议级 MCP tasks，分析工具异步化。Phase 1.5 收官。**

### 新增

- **kanyu-mcp**：长任务能力（总规"MCP tasks 为一等特性"承诺落地）——
  调研确认 rmcp 3.1 服务端已完整实现 **SEP-2663**
  （`io.modelcontextprotocol/tasks`，即原 SEP-1686 的现行编号），
  走**协议级路线**：`ServerCapabilities` 声明 tasks 扩展；白名单分析工具
  （buffer/overlay/sjoin/zonal_stats/topology）的 `tools/call` arguments 带
  `"task": true` 时任务化执行（rmcp TaskManager spawn，阻塞内核调用进
  blocking 线程池），立即返回 `resultType:"task"` 任务句柄；客户端经协议方法
  `tasks/get`（轮询）/ `tasks/cancel`（协作取消）/ `tasks/update` 驱动
  生命周期。任务结果保留 10 分钟（TaskManager TTL 惰性清扫，内存态、
  重启即丢）。5 个分析工具的内核调用提取为同步函数供同步/任务两路径共享；
  stdio 与 HTTP 两传输同权。未知 taskId / 非白名单工具返回结构化/中文错误。

## [0.7.0] - 2026-08-02

**远程 AI 代理接入：MCP streamable HTTP 传输。**

### 新增

- **kanyu-mcp / CLI**：MCP streamable HTTP 传输（裁决 #11：官方 streamable
  HTTP 已取代旧 SSE；裁决 #16 第二承诺）——`kanyu mcp serve --transport http
  --port <port>` 绑定 `127.0.0.1`，endpoint `/mcp`（POST=JSON-RPC、
  GET=SSE 流、DELETE=会话终止；`Mcp-Session-Id` 头 + LocalSessionManager
  内存会话存储；service factory 模式每会话一个无状态 KanyuServer 实例）。
  ⚠️ 无鉴权/TLS（📋），远程暴露需自行加反代与鉴权（eprintln 与文档双警示）；
  MCP tasks（SEP-1686 长任务）📋。

### 变更（Breaking）

- **CLI**：`kanyu mcp serve --transport` 的取值 `sse` 改为 `http`
  （旧 `sse` 值不再接受，clap 直接报无效值；此前 `sse` 仅返回
  "待 v0.2" 错误提示，无实际行为损失）。

## [0.6.0] - 2026-08-02

**矢量分析面补全：空间连接 sjoin 与分区统计 zonal_stats。**

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

[Unreleased]: https://github.com/DaoMingyuan/Kanyu/compare/v0.12.0...HEAD
[0.12.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.12.0
[0.11.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.11.0
[0.10.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.10.0
[0.9.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.9.0
[0.8.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.8.0
[0.7.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.7.0
[0.6.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.6.0
[0.5.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.5.0
[0.4.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.4.0
[0.3.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.3.0
[0.2.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.2.0
[0.1.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.1.0
