# kanyu-core Rust API 参考（API）

> 版本：v0.1.0 ｜ 本文与 `cargo doc --no-deps -p kanyu-core` 生成的文档同源，
> 侧重"类型地图 + 用法示例"。集成指南见 [SDK.md](SDK.md)；架构背景见 [ARCHITECTURE.md](ARCHITECTURE.md)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [crate 总览](#1-crate-总览)
2. [format —— 格式注册表](#2-format--格式注册表)
3. [layer —— 图层内存模型](#3-layer--图层内存模型)
4. [analysis —— 空间分析内核](#4-analysis--空间分析内核)
5. [crs —— 坐标参考系工具](#5-crs--坐标参考系工具)
6. [agents —— AGENTS.md 语义](#6-agents--agentsmd-语义)
7. [introspect —— 系统自省](#7-introspect--系统自省)
8. [error —— 错误处理约定](#8-error--错误处理约定)
9. [kanyu-render —— 离屏地图渲染](#9-kanyu-render--离屏地图渲染)
10. [kanyu-skill —— WASM 技能系统宿主](#10-kanyu-skill--wasm-技能系统宿主)
11. [dwg —— DWG 原生读取](#11-dwg--dwg-原生读取acadrust-自持补丁层)
12. [kanyu-shell —— 桌面壳层 UI](#12-kanyu-shell--桌面壳层-ui)
13. [kdb —— 堪舆数据库](#13-kdb--堪舆数据库kanyudbkdb)
14. [project —— 堪舆工程](#14-project--堪舆工程kyu)
15. [geoprocess —— QGIS 核心算法移植](#15-geoprocess--qgis-核心算法移植)
16. [parcel —— 宗地 TXT](#16-parcel--宗地-txt界址点坐标)
17. [kanyu-py —— Python 桥接](#17-kanyu-py--python-桥接pyo3)

## 1. crate 总览

`kanyu-core` 是堪舆内核，纯 Rust、零 C 依赖，不依赖任何兄弟 crate。
根模块再导出常用类型，并提供两个常量：

| 项 | 类型 / 值 | 说明 |
|---|---|---|
| `kanyu_core::VERSION` | `&str`（= `CARGO_PKG_VERSION`） | 内核版本号，与 workspace 一致（`0.1.0`） |
| `kanyu_core::CODENAME` | `&str`（= `"kanyu-spirit"`） | 内核代号"堪舆灵" |
| `kanyu_core::Result<T>` | `std::result::Result<T, KanyuError>` | 统一结果类型 |
| 再导出 | `KanyuError`, `FormatCapabilities`, `FormatRegistry`, `Layer`, `LayerSummary` | `use kanyu_core::{...}` 直接可用 |

模块划分：`format`（能力矩阵）、`layer`（图层模型）、`agents`（项目语义）、
`introspect`（系统自省）、`error`（统一错误）。

> 兄弟 crate `kanyu-render`（离屏地图渲染）见 §9。

## 2. format —— 格式注册表

对应 [MASTERPLAN.md](MASTERPLAN.md) 附录 A。AI 与 CLI 通过注册表决策，
格式知识不散落各处（决策流程见 [ARCHITECTURE.md](ARCHITECTURE.md#4-格式注册表设计)）。

### `Support`

```rust
pub enum Support { Full, Partial, None }
impl Support { pub fn usable(self) -> bool } // Full 或 Partial 即 true
```

单项能力支持级别。`Serialize` 为小写（`"full"` / `"partial"` / `"none"`）。

### `FormatCapabilities`

一种格式的能力画像。所有字段为 `&'static`，整体 `Clone + Serialize`：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | `&'static str` | 格式短名，如 `shp`、`dwg` |
| `name` | `&'static str` | 显示名，如 `ESRI Shapefile` |
| `extensions` | `&'static [&'static str]` | 关联扩展名（小写、不含点） |
| `read` / `write` / `edit` | `Support` | 读取 / 写入 / 编辑 |
| `symbol` | `Support` | 符号化保留 |
| `layout` | `Support` | 布局保留 |
| `driver` | `&'static str` | 实现驱动：`native` / `gdal-bridge` / `libredwg` |
| `note` | `&'static str` | 备注 |

### `FormatRegistry`

内置 17 种格式（与总规附录 A.1 对齐，有测试保证）。

| 方法 | 签名 | 说明 |
|---|---|---|
| `builtin` | `fn builtin() -> Self` | 构造内置注册表；`Default` 等同 |
| `by_id` | `fn by_id(&self, id: &str) -> Option<&FormatCapabilities>` | 按短名查询，大小写不敏感 |
| `detect` | `fn detect(&self, path: &str) -> Option<&FormatCapabilities>` | 按扩展名探测（`data.SHP` → `shp`） |
| `all` | `fn all(&self) -> &[FormatCapabilities]` | 全部格式 |
| `require` | `fn require(&self, format_id: &str, operation: &str) -> Result<&FormatCapabilities>` | 断言支持某操作（`"read"`/`"write"`/`"edit"`；其他操作名恒通过），否则返回结构化错误 |

```rust
use kanyu_core::{FormatRegistry, KanyuError};

let reg = FormatRegistry::builtin();
assert_eq!(reg.detect("roads.GEOJSON").unwrap().id, "geojson");
assert!(reg.require("wfs", "read").is_ok());
let err = reg.require("wfs", "write").unwrap_err();   // WFS 只读
assert!(matches!(err, KanyuError::UnsupportedOperation { .. }));
```

## 3. layer —— 图层内存模型

以 GeoArrow `RecordBatch` 为原生载体（WKB 几何列 `geometry` + 类型化属性列
Int64/Float64/Boolean/Utf8，arrow 58 / geoarrow-schema 0.8）。各格式解析器在边界
统一转为 FeatureCollection 后一次性入列，导出时按需转回（数据流见
[ARCHITECTURE.md](ARCHITECTURE.md#3-数据流)）。

### `Layer`

| 方法 | 签名 | 说明 |
|---|---|---|
| `load` | `fn load(id: impl Into<String>, path: &str) -> Result<Self>` | 加载图层。格式自动探测；v0.1 原生支持 geojson、csv/tsv/xlsx（坐标列自动识别 lon/lat/x/y/经度/纬度；xlsx 经 calamine 读首个 worksheet，原生类型化，写出 📋）、shp（读取：Point/MultiPoint/Polyline/Polygon 含洞，dbase 属性类型化）、fgb（读写）、geoparquet（读写，WKB 几何编码）、dxf（读写：POINT/LINE/LWPOLYLINE/POLYLINE/CIRCLE/ARC，图层→layer 属性）与 kml/kmz（读写；KMZ 为 zip 容器变体），桥接驱动格式返回 `UnsupportedOperation`；无法探测返回 `UnknownFormat` |
| `id` | `fn id(&self) -> &str` | 图层标识 |
| `len` | `fn len(&self) -> usize` | 要素数量（batch 行数） |
| `is_empty` | `fn is_empty(&self) -> bool` | 是否空图层 |
| `summary` | `fn summary(&self) -> LayerSummary` | 概要信息（直接在 batch 上统计：几何类型读 WKB 头部类型码，字段取属性列名） |
| `batch` | `fn batch(&self) -> &arrow_array::RecordBatch` | 零拷贝访问底层 GeoArrow RecordBatch（WKB 几何列 + 类型化属性列） |
| `collection` | `fn collection(&self) -> geojson::FeatureCollection` | 要素集合（按需从 batch 转换的**拥有值**；⚠️ 签名自 v0.1 早期变更：原为 `&FeatureCollection` 只读借用） |
| `query` | `fn query(&self, expression: &str) -> Result<geojson::FeatureCollection>` | 属性查询，语法见下（直接在 batch 列上求值，命中行经 arrow take 取子集；谓词语义不变） |
| `to_geojson_string` | `fn to_geojson_string(collection: &geojson::FeatureCollection) -> String` | 关联函数：集合 → GeoJSON 字符串 |
| `to_csv_string` | `fn to_csv_string(collection: &geojson::FeatureCollection) -> Result<String>` | 关联函数：集合 → CSV 字符串（`x,y` 坐标列仅 Point 取值，后接属性字段并集） |
| `to_fgb_bytes` | `fn to_fgb_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>>` | 关联函数：集合 → FlatGeobuf 字节串（列 schema 自动推断：String→String、整数→Long、浮点→Double、Bool→Bool，混合类型列退化为 String；单一几何类型按声明写出，混合几何按 Unknown 异构声明；Hilbert 空间索引，CRS 声明 EPSG:4326） |
| `to_geoparquet_bytes` | `fn to_geoparquet_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>>` | 关联函数：集合 → GeoParquet 字节串（几何列 `geometry` 按 GeoParquet 1.x 规范 WKB 编码，geo 元数据/geometry_types/bbox 由 geoparquet crate 生成；属性列 schema 推断规则同 `to_fgb_bytes`） |
| `to_dxf_string` | `fn to_dxf_string(collection: &geojson::FeatureCollection) -> Result<String>` | 关联函数：集合 → DXF 字符串（R2000；Point/MultiPoint→POINT，LineString/MultiLineString→开放 LWPOLYLINE，Polygon/MultiPolygon→闭合 LWPOLYLINE 仅外环、洞舍弃；统一图层 "0"；properties/XDATA 写出 📋；z 丢弃） |
| `to_kml_string` | `fn to_kml_string(collection: &geojson::FeatureCollection) -> Result<String>` | 关联函数：集合 → KML 字符串（KML 2.2；每要素一个 Placemark，全六类型、Multi*→MultiGeometry，Polygon 含洞保留为内环；`name`/`description` 写为同名字段，其余属性入 ExtendedData/SimpleData；z 丢弃） |
| `to_kmz_bytes` | `fn to_kmz_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>>` | 关联函数：集合 → KMZ 字节串（zip 容器 deflate 压缩，doc.kml 单条目，内容同 `to_kml_string`） |
| `write_shp` | `fn write_shp(collection: &geojson::FeatureCollection, base: &str) -> Result<()>` | 关联函数：集合 → Shapefile 三件套（base.shp/.shx/.dbf，base 为去扩展名路径）。单一几何类型校验（GeometryCollection 展平；混合报中文错误提示先 `data query` 拆分）；Polygon/MultiPolygon 外环+洞（自动整向）；dbase 字段名 10 字节截断（字符边界，冲突加 `_N` 序号）、String→Character(254 截断)、整数→Numeric(18,0)、浮点→Numeric(18,6)、Bool→Logical，空值跳过 |

**查询表达式**：`"field op value"`，`op ∈ == != > >= < <=`。
右值解析顺序：数值 → 布尔 → 字符串（可带单/双引号）。
数值字段按 `f64` 数值比较，其余按字符串比较（`>` 等为字典序）。
字段缺失的要素不匹配。表达式无法解析时返回 `KanyuError::InvalidQuery`。

```rust
use kanyu_core::Layer;

let layer = Layer::load("buildings", "examples/buildings.geojson")?;
let high = layer.query("height > 50")?;          // 2 个要素
let homes = layer.query("usage == residential")?; // 1 个要素
println!("{}", Layer::to_geojson_string(&high));
```

### `LayerSummary`

`Clone + Serialize`，CLI `data info` 与 MCP `kanyu_data_load` 的返回形状：

| 字段 | 类型 | 说明 |
|---|---|---|
| `id` | `String` | 图层标识 |
| `format` | `String` | 来源格式短名 |
| `feature_count` | `usize` | 要素数量 |
| `geometry_types` | `Vec<String>` | 几何类型集合（去重、排序），如 `["LineString", "Point"]` |
| `fields` | `Vec<String>` | 属性字段名集合（去重、排序） |

## 4. analysis —— 空间分析内核

对应 [MASTERPLAN.md](MASTERPLAN.md) §4.2.2（裁决 #16：分析内核优先于 UI 壳层）。
三个函数均以 GeoJSON `FeatureCollection` 为边界格式，结果可直接走任意
`Layer::to_*` 序列化器。**单位警示**：距离/面积以数据 CRS 单位计，
EPSG:4326 下是度而非米；米制分析请先用 [`crs::reproject`](#5-crs--坐标参考系工具)
投影到米制 CRS，或用 `crs::measure` 做测地线度量。

| 函数 | 签名 | 说明 |
|---|---|---|
| `buffer` | `fn buffer(collection: &geojson::FeatureCollection, distance: f64, segments: usize) -> Result<geojson::FeatureCollection>` | 缓冲区分析：结果为 Polygon/MultiPolygon，属性随行；`segments` 为每象限圆弧分段数（≥1，对应圆角连接角 `π/2 / segments`）；几何缺失/不可转换的要素跳过并计入返回集合 `foreign_members.skipped` |
| `overlay` | `fn overlay(target: &geojson::FeatureCollection, overlay: &geojson::FeatureCollection, op: OverlayOp) -> Result<geojson::FeatureCollection>` | 叠加分析：仅 Polygon/MultiPolygon（其余类型报中文错误并指出要素序号）。Union/Intersection/Xor 为 target×overlay 逐要素对布尔（未做跨对融合 dissolve）；Difference 为每 target 连续减全部 overlay。属性：target + overlay（键冲突加 `overlay_` 前缀；Difference 仅 target 属性） |
| `topology_check` | `fn topology_check(collection: &geojson::FeatureCollection, rules: &[TopologyRule]) -> Result<TopologyReport>` | 拓扑检查：NoOverlap 对面要素两两判定（intersects 粗筛 + 交集面积 > 1e-10 确认）；非面要素跳过；O(n²) 朴素实现（rstar 加速 📋） |
| `sjoin` | `fn sjoin(target: &geojson::FeatureCollection, join: &geojson::FeatureCollection, predicate: SpatialPredicate) -> Result<geojson::FeatureCollection>` | 空间连接：**左连接 + 匹配展开**（与 GeoPandas 默认 inner 不同）：保留全部 target 要素、无匹配 join 侧缺省、一对多各输出一条；属性合并、键冲突加 `join_` 前缀并附 `join_index`（join 要素序号）；无几何 target 按无匹配处理；O(n·m)（rstar 📋） |
| `zonal_stats` | `fn zonal_stats(zones: &geojson::FeatureCollection, values: &geojson::FeatureCollection, field: &str, stats: &[ZonalStat]) -> Result<geojson::FeatureCollection>` | 分区统计：zones 仅面要素；values 按代表点归属（Point 直接取坐标，其余用 Centroid 质心）；一值多区计入**首个**匹配区；区外值计入 `foreign_members.unzoned_count`；统计列命名 `{field}_{stat}` 小写；`field` 必须存在且可转 f64（count 同样只统计数值要素）；某区无有效值时 count 写 0、其余列缺省 |

### `OverlayOp` / `TopologyRule` / `SpatialPredicate` / `ZonalStat`

| 类型 | 变体 | FromStr |
|---|---|---|
| `OverlayOp` | `Union` / `Intersection` / `Difference` / `Xor` | `union` / `intersection` / `difference` / `xor`（大小写不敏感，未知值报中文错误） |
| `TopologyRule` | `NoOverlap` | `no_overlap`（同上） |
| `SpatialPredicate` | `Intersects` / `Contains` / `Within` | `intersects` / `contains` / `within`（同上；Contains=target 包含 join，Within=join 包含 target） |
| `ZonalStat` | `Count` / `Sum` / `Mean` / `Min` / `Max` | `count` / `sum` / `mean` / `min` / `max`（同上） |

### `TopologyReport` / `TopologyViolation`

`Clone + Serialize`，MCP `kanyu_analysis_topology` 与 CLI `analysis topology --json` 的返回形状：

| 字段 | 类型 | 说明 |
|---|---|---|
| `rule` | `String` | 执行的规则（逗号分隔） |
| `feature_count` | `usize` | 输入要素总数 |
| `violation_count` | `usize` | 违规条数（= `violations.len()`） |
| `violations` | `Vec<TopologyViolation>` | 违规明细：`feature_a`/`feature_b`（输入集合中的要素序号，0 起）+ `note`（如重叠面积） |

## 5. crs —— 坐标参考系工具

投影变换与测地线度量（EPSG:4326 数据做米制分析的配套能力）。
投影基于 proj4rs（纯 Rust PROJ 改写）+ crs-definitions **内置 EPSG 数据库**；
度量基于 geo crate（Karney 2013，WGS84 椭球）。

| 函数 | 签名 | 说明 |
|---|---|---|
| `reproject` | `fn reproject(collection: &geojson::FeatureCollection, from: &str, to: &str) -> Result<geojson::FeatureCollection>` | 投影变换：逐坐标递归转换全几何类型；经纬度自动衔接度/弧度（GeoJSON 度 ↔ PROJ 弧度）；z 不变；`from == to` 原样返回；失败坐标（NaN/越界）报中文错误并指出要素序号 |
| `measure` | `fn measure(collection: &geojson::FeatureCollection, kind: MeasureKind) -> Result<serde_json::Value>` | 测地线度量：Length 取线长与面外环周长（米）、Area 取面面积（平方米，含洞扣除，`Orient` 归一化绕向）；Point/无几何为 0。输出 `{"kind", "unit": "m"\|"m²", "total", "per_feature": [{"index", "value"}]}` |
| `search_crs` | `fn search_crs(query: &str, limit: usize) -> Vec<CrsInfo>` | EPSG 全库检索（7507 条，代码域 2000..=32766）：代码子串或名称（大小写不敏感）匹配，代码升序；空查询返回常用精选（4326/3857/4490/4526/4527/4610/4214 等） |
| `crs_info` | `fn crs_info(code: u32) -> Option<CrsInfo>` | 按代码查 EPSG 条目；库中不存在返回 None |

### `CrsInfo` / `CrsKind`

`CrsInfo { code: u32, name: String, kind: CrsKind, unit: String }`（serde Serialize）。
名称取自 WKT 首段引号串；`kind` ∈ `Geographic`\|`Projected`\|`Other`（按 proj4 串
`+proj=` 判定）；`unit` 中文友好：地理 CRS 为 "度"，投影 CRS 解析 `+units=`/
`+to_meter=`（"米"/"千米"/"英尺"/"美制英尺" 等）。

**轴序约定**：proj4rs 为 PROJ4 风格改写，EPSG 定义不携带官方轴序——本模块
输入/输出一律 GIS 序（经度在前、纬度在后），与 GeoJSON 一致；EPSG:4490 与
EPSG:4326 对同一 (lon, lat) 点转换结果一致（差异 < 1mm）。

### `MeasureKind`

| 变体 | FromStr | 单位 |
|---|---|---|
| `Length` | `length` | m（测地线长度） |
| `Area` | `area` | m²（测地线面积） |

### CRS 定义接受形式

| 形式 | 示例 | 说明 |
|---|---|---|
| EPSG 码 | `EPSG:4326`、`EPSG:3857`、`EPSG:4490` | 内置 EPSG 数据库（crs-definitions crate，覆盖常用 CGCS2000/高斯克吕格带等全量条目） |
| proj4 定义串 | `+proj=merc +datum=WGS84` | `+` 开头直接解析 |
| 快捷方式 | `WGS84` | 等价 `+proj=longlat +ellps=WGS84` |

无法解析时报中文错误并列出上述接受形式。

## 6. agents —— AGENTS.md 语义

对应 [MASTERPLAN.md](MASTERPLAN.md) §4.3.2；设计理念见
[ARCHITECTURE.md](ARCHITECTURE.md#5-agentsmd-语义层设计)。

### `AgentsMd`

```rust
pub struct AgentsMd {
    pub meta: ProjectMeta,              // 项目元数据
    pub layers: Vec<LayerSemantics>,    // 数据层语义表
    pub business_rules: Vec<String>,    // 业务规则（编号列表原文）
    pub custom_tools: Vec<String>,      // 自定义工具名列表
}
```

| 方法 | 签名 | 说明 |
|---|---|---|
| `parse` | `fn parse(markdown: &str) -> Result<Self>` | 容错解析：缺省段落不报错 |
| `validate` | `fn validate(&self) -> Vec<String>` | 完整性校验，返回问题清单（空 = 通过）。检查：缺 `name`、缺 `crs`（强制项）、语义表为空、图层未声明关键字段 |

自由函数：

| 函数 | 签名 | 说明 |
|---|---|---|
| `agents::template` | `fn template(project_name: &str, crs: &str) -> String` | 生成 AGENTS.md 模板（`kanyu agents init` 使用；可被 `parse` 往返解析） |
| `agents::load` | `fn load(path: &str) -> Result<AgentsMd>` | 读文件并解析；IO 失败返回 `KanyuError::AgentsMd` |

### `ProjectMeta`

`Default + Clone + Serialize`，全部字段可选：

| 字段 | 类型 | 来源语法 |
|---|---|---|
| `name` | `Option<String>` | `- **name**: 朝阳区城市更新规划` |
| `crs` | `Option<String>` | `- **crs**: EPSG:4526` |
| `extent` | `Option<Vec<f64>>` | `- **extent**: [116.2, 39.8, 116.6, 40.0]`（恰为 4 个数才接受） |
| `author` / `created` | `Option<String>` | `- **author**: ...` / `- **created**: ...` |

### `LayerSemantics`

数据层语义表的一行（五列 Markdown 表格，表头/分隔行自动跳过）：

| 字段 | 类型 | 列 |
|---|---|---|
| `layer` | `String` | 图层名 |
| `geometry` | `String` | 几何类型 |
| `semantics` | `String` | 业务语义 |
| `key_fields` | `String` | 关键字段 |
| `rules` | `String` | 业务规则 |

```rust
let doc = kanyu_core::agents::load("AGENTS.md")?;
let issues = doc.validate();
if issues.is_empty() {
    println!("{}：{} 个图层", doc.meta.name.unwrap(), doc.layers.len());
}
```

## 7. introspect —— 系统自省

`kanyu introspect` 与 MCP `kanyu_system_introspect` 的内核实现，
是自迭代闭环 Phase 1（观察）的入口。

| 函数 | 签名 | 说明 |
|---|---|---|
| `introspect::report` | `fn report() -> Introspection` | 生成自省报告 |
| `introspect::modules` | `fn modules() -> Vec<ModuleInfo>` | 内核模块清单（单一事实来源） |
| `introspect::tools` | `fn tools() -> Vec<ToolInfo>` | 工具清单（含 planned） |

### `Introspection`

| 字段 | 类型 | 说明 |
|---|---|---|
| `version` / `codename` | `&'static str` | 即 `VERSION` / `CODENAME` |
| `manifesto` | `&'static str` | 架构宣言："以天地为盘，以数据为爻，以 AI 为神，重构地理空间之道。" |
| `modules` | `Vec<ModuleInfo>` | 模块清单 |
| `formats` | `Vec<FormatCapabilities>` | 完整格式能力矩阵（即 `FormatRegistry::builtin().all()`） |
| `tools` | `Vec<ToolInfo>` | 工具清单 |

### `ModuleInfo` / `ToolInfo`

| 结构 | 字段 | 说明 |
|---|---|---|
| `ModuleInfo` | `name`, `role`, `status`: 均 `&'static str` | `status` ∈ `stable` / `incubating` / `planned` |
| `ToolInfo` | `name`, `group`, `status`: 均 `&'static str` | `group` ∈ `data` / `agents` / `analysis` / `render` / `system`；`name` 即真实 MCP 工具名（下划线式，如 `kanyu_data_load`），与总规点式逻辑命名的映射见 [MCP.md](MCP.md#命名规范) |

## 8. error —— 错误处理约定

所有公共 API 返回 `kanyu_core::Result<T>`；错误为单一枚举 `KanyuError`（thiserror 派生，
`Display` 面向最终用户）。CLI 把它转成非零退出码，MCP 把它转成 `internal_error`
（见 [CLI.md](CLI.md#全局约定) 与 [MCP.md](MCP.md#错误处理)）。

| 变体 | 形状 | 触发场景 |
|---|---|---|
| `UnknownFormat` | `UnknownFormat(String)` | `FormatRegistry::by_id` / `detect` 未命中；`require` 查询未知格式 |
| `UnsupportedOperation` | `{ format: String, operation: String }` | `require` 断言失败（如对 `wfs` 写）；或格式能力在册但驱动未启用（如 v0.1 加载 shp、导出 dwg） |
| `Io` | `Io(#[from] std::io::Error)` | 文件读写失败（`Layer::load`、导出写盘） |
| `GeoJson` | `GeoJson(String)` | GeoJSON 文本解析失败；`impl From<geojson::Error>` 自动转换 |
| `AgentsMd` | `AgentsMd(String)` | `agents::load` 读取失败 |
| `InvalidQuery` | `InvalidQuery(String)` | 查询表达式不含合法 `field op value` 结构 |
| `Other` | `Other(String)` | 兜底 |

```rust
use kanyu_core::{KanyuError, Layer};

match Layer::load("roads", "roads.shp") {
    Err(KanyuError::UnsupportedOperation { format, operation }) => {
        eprintln!("{format} 暂不支持 {operation}：等待 bridge feature 启用");
    }
    other => { let _ = other?; }
}
```

## 9. kanyu-render —— 离屏地图渲染

兄弟 crate（依赖 kanyu-core；CLI/MCP 依赖它出图）。SVG 纯字符串零依赖；
PNG 经 tiny-skia 纯 Rust CPU 光栅化。色彩取自
[MASTERPLAN.md](MASTERPLAN.md) §1.2（晨山/夜观星），点/线/面样式集中于
`style_for` 单一事实来源。wgpu 实时渲染管线属交互壳层 kanyu-shell，不在此 crate。

### `RenderOptions`

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `width` | `u32` | 800 | 输出宽度（像素，1–8192） |
| `height` | `u32` | 600 | 输出高度（像素，1–8192） |
| `padding` | `f64` | 20.0 | 四周边距（像素） |
| `theme` | `Theme` | `Light` | `Light` 晨山 / `Dark` 夜观星（`FromStr`：`light`/`dark`） |
| `background` | `Option<String>` | `None` | 自定义背景色 `#RRGGBB`（缺省用主题画布色） |
| `style` | `Option<StyleRule>` | `None` | 属性驱动样式规则（缺省走主题默认样式，行为与旧版一致） |
| `viewport` | `Option<[f64; 4]>` | `None` | 显式视口 `[minx, miny, maxx, maxy]`（数据坐标；给出时跳过集合 bbox 自动适配直接以该范围等比缩放居中——kanyu-shell 的缩放/平移即每帧传入变化后的视口；非有限或倒置报中文错误） |

### `StyleRule`

JSON `type` 判别（`serde(tag = "type", rename_all = "lowercase")`），
总规 §3.4 符号定义子集：

| 变体 | 字段 | 语义 |
|---|---|---|
| `Graduated` | `field: String`；`stops: Vec<(f64, String)>` | 数值分档：`stops` 为 `[[阈值, "#RRGGBB"], …]`（须非空且阈值**严格升序**）；取**最后一个满足 `值 ≥ 阈值` 的档**（恰等阈值取该档、超最大档取末档、低于首档走默认样式）；字段缺失/非数值走默认 |
| `Categorical` | `field: String`；`colors: HashMap<String, String>`；`default: Option<String>` | 字符串类别映射：命中取对应色，无匹配取 `default`（亦缺省则走默认）；字段缺失/非字符串走默认 |

`validate()` 语义校验（render 入口统一调用）：空 stops、非升序、坏 hex
均报中文错误并指出出错项。`color_for(properties) -> Option<String>`
返回命中色 `#RRGGBB` 原样；未命中为 None。命中色按几何类型派生：
面=该色 20% 透明填充 + 同色描边、线=该色描边、点=该色填充。

完整示例：

```json
{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}
{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E","residential":"#D4A843"},"default":"#888888"}
```

### 函数

| 函数 | 签名 | 说明 |
|---|---|---|
| `render_svg` | `fn render_svg(collection: &geojson::FeatureCollection, opts: &RenderOptions) -> Result<String, RenderError>` | 渲染为 SVG 字符串（viewBox + 背景 rect + 注释头；面 `fill-rule="evenodd"` 出洞） |
| `render_png` | `fn render_png(collection: &geojson::FeatureCollection, opts: &RenderOptions) -> Result<Vec<u8>, RenderError>` | 渲染为 PNG 字节串（tiny-skia Pixmap → `encode_png`） |

视口变换：集合 bbox 等比缩放、居中、padding、y 轴翻转；空集合仅背景、
单点给 0.001° 默认视野（不除零）；经度跨度 >350° 或含 NaN/Inf 坐标报
中文错误。绘制顺序：面 → 线 → 点（点最上层）；Multi* 逐部件、
GeometryCollection 递归；无几何要素跳过。

### `RenderError`

| 变体 | 触发 |
|---|---|
| `InvalidSize(u32, u32)` | 宽高为 0 或 > 8192 |
| `InvalidExtent(String)` | 经度跨度 >350°、NaN/Inf 坐标 |
| `Encode(String)` | PNG 编码失败 |
| `InvalidColor(String)` | 主题名/颜色值非法 |
| `InvalidStyle(String)` | 样式规则非法（空 stops、非升序、坏颜色值；消息指出出错项） |

## 10. kanyu-skill —— WASM 技能系统宿主

兄弟 crate（CLI 依赖它执行技能；MCP 热加载接线 📋）。总规 §4.5"以 WASM
为技能"落地：wasmtime 47 组件模型 + WIT 强类型 ABI + fuel 配额沙箱。
ABI 定义见 [`crates/kanyu-skill/wit/skill.wit`](../crates/kanyu-skill/wit/skill.wit)：
`meta() -> string`（元数据 JSON）与
`run(input: string) -> result<string, string>`（FeatureCollection JSON 进/出）。

### `SkillHost` / `Gene` / `SkillMeta`

| 项 | 签名 | 说明 |
|---|---|---|
| `SkillHost::new` | `fn new() -> Result<SkillHost, SkillError>` | 构造宿主（`Config::consume_fuel(true)`） |
| `SkillHost::load` | `fn load(&self, path: &str) -> Result<Gene, SkillError>` | 编译校验 → 实例化 → 调 `meta()` 取元数据并校验（无效 wasm/接口不匹配/元数据非法各报中文错误） |
| `SkillHost::run` | `fn run(&self, gene: &Gene, input: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection, SkillError>` | 沙箱执行：每次重置 fuel（10 亿），trap/配额/结果非法独立报错 |
| `Gene::meta` | `fn meta(&self) -> &SkillMeta` | 元数据 |

### `SkillMeta`

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | `String` | 技能名（非空校验） |
| `version` | `String` | 版本号（非空校验） |
| `capabilities` | `Vec<String>` | 能力清单（如 `["analyzer"]`） |

### `SkillError`

| 变体 | 触发 |
|---|---|
| `LoadFailed { path, reason }` | wasm 编译失败 / 实例化失败（WIT 接口不匹配） |
| `MetaInvalid(String)` | `meta()` 调用失败 / 非合法 JSON / 必填字段缺失 |
| `Trap(String)` | 执行期 trap（含技能主动返回的业务错误） |
| `Timeout(String)` | fuel 耗尽（疑似死循环或过重计算） |
| `ResultInvalid(String)` | 返回值非合法 FeatureCollection |

**沙箱模型**：组件无 WASI 导入（纯计算，无文件/网络/环境访问）；
fuel 配额覆盖纯计算死循环；组件无 IO 导入故不设墙钟超时（注释即契约）。

**编写技能**（Rust guest）：参照样板
[`crates/kanyu-skill/testdata/attr_scaler/`](../crates/kanyu-skill/testdata/attr_scaler/)——
`wit-bindgen` `generate!`/`export!` 实现 `kanyu:skill/analyzer` 的 `Guest`
trait，`wasm32-unknown-unknown` 编译后用 `wasm-tools component new` 组件化。

**MCP 接线（已落地）**：`kanyu_system_hotload`（热加载校验注册）、
`kanyu_skill_run`（沙箱执行，可 `task: true` 异步）、`kanyu_skill_list`
（注册表快照）——字段与输出见 [MCP.md](MCP.md#35-kanyu_system_hotload)。

## 11. dwg —— DWG 原生读取（acadrust + 自持补丁层）

DWG（AutoCAD 二进制格式）读取，acadrust 0.4（纯 Rust、MPL-2.0）+ 两层
自持补丁（spike 定稿路线，证据见 [MASTERPLAN.md](MASTERPLAN.md) §6.4
Phase 5 spike 结论）：

1. **AC15 定位 workaround**：acadrust 0.4.1 的 objects 定位推断
   `handles_seeker - aux_header_end` 在"AuxHeader 位于 Handles 之后"的
   合法 R2000 布局下为负，静默空文档。本模块按 ODA 约定以
   `[Classes_end, Handles_start)` 推断 objects 段，直接驱动 acadrust
   底层 pub API（`handle_reader::read_handles` /
   `object_reader::DwgObjectReader` / `DwgDocumentBuilder`）。
   AC15 系（R13/R14/R2000，AC1012/1014/1015）直接走本层；其他版本走
   原生 `DwgReader::read`，空文档回退。
2. **编码层**：`decode_dwg_string` 修复两种乱码形态——GBK 字节
   Latin-1 展开（codepage 转码，CJK 页优先 + GBK 兜底）与 MIF
   `\U+XXXX` 转义未解码。

### `dwg_to_collection` / `DwgStats`

| 项 | 签名 | 说明 |
|---|---|---|
| `dwg_to_collection` | `fn dwg_to_collection(path: &str) -> Result<(geojson::FeatureCollection, DwgStats)>` | DWG → FeatureCollection + 统计。实体映射（z 丢弃）：POINT→Point、LINE→LineString、LWPOLYLINE/POLYLINE（2D/3D 取 xy，闭合→Polygon 单环）、CIRCLE→Polygon(64 段)、ARC→LineString(64 段，弧度)、ELLIPSE→64 段参数方程近似（全角→Polygon、部分弧→LineString）；TEXT/MTEXT→标注要素（插入点 Point）；SPLINE/INSERT/HATCH/DIMENSION 系及其余跳过+按类型计数；退化几何（<2 开放/<3 闭合/半径或轴长≤0/空文本）单独计数（dxf 同口径） |

**标注要素语义**（`feature_kind` 设计，文档即契约）：标注要素带
`feature_kind: "annotation"` 属性，消费者可据此过滤——几何图层语义
（"要素=空间对象"）不被标注污染，需要纯几何时 `query` 过滤即可
（`feature_kind != annotation`）。其余属性：`text`（解码+清洗后）、
`height`、`rotation`（度）、`layer`。

### `clean_mtext` / `ellipse_to_positions`

| 函数 | 签名 | 说明 |
|---|---|---|
| `clean_mtext` | `fn clean_mtext(raw: &str) -> String` | MTEXT 内联格式码最小清洗（保留内容、去控制码）：`\P`→`\n`、`~`/`\~`→空格、`\\`→`\`、`{...}` 分组去括号保内容（嵌套配对）、`\f..\H..\W..\A..\C..\Q..\T..\X` 样式参数码丢弃、`\S上/下;` 堆叠保留 |
| `ellipse_to_positions` | `fn ellipse_to_positions(center: (f64,f64), major: (f64,f64), ratio: f64, start: f64, end: f64, segments: usize) -> Vec<Vec<f64>>` | 椭圆参数方程采样：P(t)=C+R(α)·(a·cos t, b·sin t)，a=|major|、b=a·ratio、α=atan2(major.y, major.x)；弧度制（acadrust 口径） |
| `DwgStats` | `{ version: String, skipped_by_type: BTreeMap<String, usize>, degenerate: usize }` | 读取统计，写入 `foreign_members["kanyu:dwg"]`（与 buffer 的 `skipped` 上报同模式） |

### `decode_dwg_string`

`fn decode_dwg_string(raw: &str, codepage: u16) -> String`：

1. 全 ASCII → 仅 MIF 解码；
2. 含高位字节 → 逐 char 取低 8 位还原字节序列，按 codepage 转码
   （936 GBK / 950 BIG5 / 932 Shift-JIS / 949 EUC-KR，其余 GBK 兜底——
   启发式面向中文图纸，真 Latin-1 文本在非 CJK 页可能误判，rustdoc 即契约）；
3. 统一 MIF `\U+XXXX` → Unicode 字符。

`Layer::load("x.dwg")` 经 `"dwg"` arm 薄调用本模块（
`crate::dwg::dwg_to_collection(path)?.0`）。

## 12. kanyu-shell —— 桌面壳层 UI（v0.3，ArcGIS Pro 式）

二进制 crate（`kanyu-shell`，依赖 kanyu-core + kanyu-render + kanyu-skill，
无库表面；不发布 crates.io）。eframe/egui 0.35 + wgpu 原生窗口（1280×800
初始、800×500 最小，窗口图标 assets/logo-256.png 编译期嵌入；release 构建
为 GUI 子系统，双击不弹控制台）。

### 架构细分（v0.3）

```
┌──────────────────────────────────────────────────┐
│ Ribbon（102px）：品牌 + 七页签（主页/数据/分析/    │
│ 制图/视图/技能/帮助）+ 图标大按钮（图标 20px +    │
│ 标题 11px + 悬停简介浮现卡，三分离）              │
├──────────┬────────────────────────────────────────┤
│ 左侧停靠 │      MapCanvas                          │
│ 目录|图层 │  （地图色彩独立，默认固定晨山，         │
│ （双页签）│   不受界面主题影响）                    │
├──────────┴────────────────────────────────────────┤
│ 底部停靠区（200px）：终端 | AI 对话（双页签）      │
├──────────────────────────────────────────────────┤
│ StatusBar（28px）：状态/坐标/视口宽/地图色彩/要素/版本 │
└──────────────────────────────────────────────────┘
```

- **ui_kit 设计系统**（`src/ui_kit/`）：tokens（间距/圆角/控件高/七级文本）、
  controls（KButton 四变体/KIconButton/KTextInput/KCombo/KCheckbox/
  ribbon_button/tab_strip/tree_row/password_input）、containers（KCard/
  KSectionHeader/KDialogShell/KBadge）、icons（33 枚线性图标，§1.4 风格，
  egui painter 直绘）。铁律（AGENTS.md #8）：先查后用、无则按类新建、样式不出库；
  UI 改动对照 ui_kit「设计审查规范」清单（design-review 技能沉淀）。
- **目录面板（Catalog，`src/catalog.rs`）**：ArcGIS Pro 目录窗格——快捷位置
  （桌面/文档/下载/项目目录/磁盘根）、面包屑、目录优先条目（仅数据文件，
  含 .kdb/.kyu），双击进入目录/打开数据为图层；与图层面板职责分离
  （目录管"找数据"，图层管"数据现场"），左侧停靠区双页签保活。
- **Contents 骨架目录**：根→图层节点（展开箭头+几何图例色块+行尾可见性/
  缩放/移除+选中联动）→几何/字段/格式子节点。
- **独立终端**：命令直达内核（`console.rs`，`ConsoleHost` trait），
  与界面共享数据现场；↑↓ 历史导航。
- **AI 对话面板**（`src/ai.rs`，BitFun 式驱动与设置）：`AiDriver` 可插拔——
  LocalDriver（离线规则引擎：`LocalDriver::parse` 纯函数意图映射：
  缓冲/打开/图层/度量/导出/帮助）+ OpenAiDriver（OpenAI 兼容端点，
  ureq/rustls，系统提示注入数据现场）；`AiSettings`（driver/base_url/
  api_key/model）持久化 `%APPDATA%/kanyu/shell_ai.json`。
- **地图色彩解耦**：`MapThemeMode { FixedLight（默认）, FixedDark, FollowUi }`，
  `as_str()/parse()` 随 `.kyu` 序列化；`effective_map_theme()` 统一画布与
  地图导出取色——界面主题切换永不影响制图输出。
- **工程格式**：「主页 → 打开工程…/保存工程」驱动 `.kyu`（见 §14）。
- 右侧属性面板已于 v0.4 移除（后续按新定制要求重建，见 AI_SYNC 待办）。

### 命令行参数（截图验证模式）

```
kanyu-shell [--load <数据文件>] [--theme light|dark]
kanyu-shell --screenshot <out.png> [--load <file>] [--theme dark] [--delay <秒>]
```

| 参数 | 默认 | 说明 |
|---|---|---|
| `--load <file>` | 无 | 启动即加载（格式自动探测，同 `Layer::load` 支持的扩展名） |
| `--theme light\|dark` | `light` | 初始**界面**主题（不影响地图色彩，见 `MapThemeMode`） |
| `--screenshot <out.png>` | 无 | 截图验证模式：延时后截取**真实窗口全部内容**（egui `ViewportCommand::Screenshot` → `Event::Screenshot` 原生管线，eframe wgpu 交换链读回）落盘 PNG 并退出；失败以码 1 退出 |
| `--delay <秒>` | `2.0` | 截图前等待（等窗口与地图纹理就绪） |
| `--help` / `--version` | — | 用法 / 版本号 |

### 视图数学（`src/view.rs`，纯函数可单测）

核心不变式：`view_bbox` 宽高比恒等于画布像素宽高比（`fit_view` 建立、
`zoom_at` 等比维持、`pan` 平移不改比例），渲染内核收到同比例视口后
letterbox 为零，故 `screen_to_data` 是简单线性映射（y 轴翻转）。

| 函数 | 签名 | 说明 |
|---|---|---|
| `fit_view` | `fn fit_view(extent: BBox, width_px: f64, height_px: f64) -> BBox` | 数据范围以中心为准等比扩边嵌入画布比例；零跨度给 0.001° 默认视野 |
| `zoom_at` | `fn zoom_at(bbox: BBox, anchor: (f64, f64), factor: f64) -> BBox` | 以数据锚点（鼠标处）为不动点缩放；结果经 `clamp_span` 约束到 [1e-7, 340°] 跨度 |
| `pan` | `fn pan(bbox: BBox, dx_px: f64, dy_px: f64, width_px: f64, height_px: f64) -> BBox` | 抓取式平移：内容跟随鼠标，视口反向移动（屏幕 y 向下、数据 y 向上，符号翻转） |
| `screen_to_data` | `fn screen_to_data(sx: f64, sy: f64, bbox: BBox, width_px: f64, height_px: f64) -> (f64, f64)` | 屏幕坐标（画布左上角原点）→ 数据坐标（状态栏坐标与缩放锚点来源） |
| `clamp_span` | `fn clamp_span(bbox: BBox) -> BBox` | 视口跨度等比约束（防越过渲染内核零跨度防护与 >350° 拒绝域） |
| `union` | `fn union(bboxes: impl IntoIterator<Item = BBox>) -> Option<BBox>` | 多图层 bbox 并集（初始视口的数据范围） |

## 13. kdb —— 堪舆数据库（KanyuDB，`.kdb`）

自研存档格式（裁决 #19）：KDB 文件 = Arrow IPC 文件，schema 元数据携带
`kanyu:format="kdb"` / `kanyu:format_version="1"` / `kanyu:producer`。与内存
RecordBatch 同构直通（WKB 几何列 + `geoarrow.wkb` 扩展 + 类型化属性列），
类型保真不经 GeoJSON 中间层；任何 Arrow 工具链（pyarrow/DuckDB/Polars）可读。
v1 约束：单批次。

| API | 签名 | 说明 |
|---|---|---|
| `kdb::batch_to_kdb` | `fn batch_to_kdb(batch: &RecordBatch) -> Result<Vec<u8>>` | RecordBatch → KDB 字节流（注入 kanyu.* 元数据） |
| `kdb::kdb_to_batch` | `fn kdb_to_batch(bytes: &[u8]) -> Result<RecordBatch>` | KDB 字节流 → RecordBatch（校验 `kanyu:format`；多批次/非 KDB 中文报错） |
| `Layer::to_kdb_bytes` | `fn to_kdb_bytes(&self) -> Result<Vec<u8>>` | 图层直接导出 KDB（RecordBatch 直通） |
| `Layer::from_batch` | `fn from_batch(id: impl Into<String>, batch: RecordBatch) -> Layer` | RecordBatch 直构图层（.kdb 读取路径，format 记为 "kdb"） |

转换：`Layer::load("x.kdb")` 与 CLI/MCP `export -f kdb` 接入全格式矩阵
（任意格式 ↔ kdb ↔ 任意格式）。

## 14. project —— 堪舆工程（`.kyu`）

JSON 工程清单（裁决 #19）：`kanyu_project=1` 格式标识 + 项目元数据 +
图层路径引用 + 界面状态（视口/地图色彩/可见性）。不内嵌数据；无来源的
内存图层（分析产出）不入工程。

| API | 签名 | 说明 |
|---|---|---|
| `KanyuProject::new` | `fn new(name: impl Into<String>, crs: impl Into<String>) -> Self` | 新建空工程 |
| `to_json` / `from_json` | `fn to_json(&self) -> Result<String>` / `fn from_json(text: &str) -> Result<Self>` | 序列化/解析（`kanyu_project` 标识与版本校验，高版本拒绝并提示升级堪舆） |
| `save` / `load` | `fn save(&self, path: &str) -> Result<()>` / `fn load(path: &str) -> Result<Self>` | 文件读写 |
| 字段 | `name/crs/created/kanyu_version/viewport: Option<[f64;4]>/map_theme: String/layers: Vec<ProjectLayer>` | `map_theme` ∈ `fixed_light`(默认)/`fixed_dark`/`follow_ui` |
| `ProjectLayer` | `{ id, source, visible, style: Option<serde_json::Value> }` | 图层引用（路径相对工程目录或绝对） |


## 15. geoprocess —— QGIS 核心算法移植

语义对齐 QGIS Processing（rustdoc 即契约）。

| 函数 | 签名要点 | QGIS 对应 |
|---|---|---|
| `dissolve` | `(collection, field: Option<&str>) -> Result<FeatureCollection>` | Dissolve（按字段分组并集；keep-first 属性） |
| `simplify` | `(collection, tolerance: f64)` | Simplify（Douglas-Peucker；退化剔除） |
| `centroid` | `(collection)` | Centroids（属性随行） |
| `convex_hull` | `(collection)` | Convex hull |
| `delete_holes` | `(collection, min_area: Option<f64>)` | Delete holes（None=全删） |
| `explode` | `(collection)` | Multipart to singleparts |
| `stats` | `(collection) -> LayerStats` | 选择集统计（测地线口径；亩/公顷/km²） |
| `boundary` | `(collection)` | Boundary（面→全部环转线；开放线→端点 MultiPoint；闭合线/点跳过） |
| `bounding_boxes` | `(collection)` | Bounding boxes（逐要素最小外接矩形面，属性随行） |
| `merge` | `(collections: &[&FeatureCollection])` | Merge vector layers（多图层要素顺序拼接） |
| `extract_by_attribute` | `(collection, expr: &str)` | Extract by attribute（"field op value"；=/==/!=/>/>=/</<=/contains，语义同 `Layer::query`） |
| `extract_by_location` | `(collection, mask, predicate: &str)` | Extract by location（intersects/contains/within，DE-9IM；非法谓词报错） |
| `count_points_in_polygon` | `(polygons, points)` | Count points in polygon（追加 NUMPOINTS 整数；含边界，MultiPoint 按子点计） |
| `field_stats` | `(collection, field: &str) -> FieldStats` | Basic statistics for fields（count/null_count/min/max/sum/mean/range/总体 stddev） |
| `mean_coordinates` | `(collection, weight_field: Option<&str>)` | Mean coordinate(s)（质心可加权平均，单点要素含 MEAN_X/MEAN_Y） |
| `distance_matrix` | `(a, b) -> DistanceMatrix` | Distance matrix（逐对测地线距离，米；点取坐标、非点取质心；行列取要素 id 或序号，含 min/max/mean） |
| `nearest_neighbor` | `(collection) -> NearestNeighborReport` | Nearest neighbour analysis（观测/期望均值、NNI、min/max；期望=0.5/sqrt(n/A)，A 为外包矩形测地面积；n<2 报错） |
| `multi_ring_buffer` | `(collection, distances: &[f64])` | Multi-ring buffer（逐档差集环，首档为完整缓冲区；属性 RING/DISTANCE；距离须非空、严格递增非负） |
| `variable_buffer` | `(collection, field: &str, segments: u32)` | Variable distance buffer（距离取自数值字段；缺失/非数值/负值跳过并计入 skipped） |
| `split_by_field` | `(collection, field: &str) -> Vec<(String, FeatureCollection)>` | Split vector layer（按字段值分组，值字符串化、缺失归空串组，BTreeMap 字典序） |
| `add_geometry_attributes` | `(collection)` | Add geometry attributes（面追加 AREA_M2/PERIMETER_M、线追加 LENGTH_M，测地线口径；点不追加） |
| `create_grid` | `(extent: [f64;4], cell_size: f64)` | Create grid（矩形格网覆盖范围，末行/列裁剪；属性 ROW/COL 0 起） |
| `points_along_lines` | `(collection, distance: f64)` | Points along geometry（沿线每 distance 米一点，含起点、终点仅整除时含；属性 DISTANCE 里程；非线跳过） |
| `concave_hull` | `(collection, concavity: f64)` | Concave hull（整层点集凹包单面；geo concaveman，concavity 越小越凹、∞ 等价凸包，平面口径） |
| `minimum_rotated_rect` | `(collection)` | Oriented minimum bounding box（逐要素最小旋转矩形面，属性随行；退化要素跳过） |

## 15.2 tooldef / toolrun —— 工具定义注册表与执行（一处声明，三面投影）

QGIS Processing 式工具面的**单一事实来源**（自 kanyu-shell 下沉，纯数据零 UI 依赖）：
shell 工具箱面板已投影消费；kanyu-py SDK 与 MCP 工具面为后续投影位。

| 模块 | API | 说明 |
|---|---|---|
| `tooldef` | `TOOLS: &[ToolDef]`（37 工具 / 5 分类） | ToolDef{id/中文名/分类/说明/参数表 ToolParam{key/label/ParamKind/required/hint/default/help}}；ParamKind = Layer/Number/NumberList/Text/Field(锚点)/Enum/Text 表达式；`#[derive(Serialize)]` 供 SDK JSON 投影 |
| | `find(id) -> Option<&ToolDef>` | 按 id 查工具 |
| `toolrun` | `run_tool(id, values, get_layer) -> Result<ToolOutcome>` | 统一执行：图层经闭包注入；产出 NewLayer/NewLayers/Report（「导出图层」直接经 FormatRegistry + Layer::to_* 写盘） |
| | `validate(def, values)` / `validate_param(p, v)` | 整表/单参数校验（中文错误，内联红字语义） |
| | `parse_number_list` / `parse_extent` / `parse_positive` | 纯解析器（递增非负距离列表 / 范围四数 / 正数） |


## 15.1 attrcalc —— 属性字段计算（字段计算器）

递归下降表达式引擎（纯 Rust 零依赖），壳层属性表面板的字段计算器即由其驱动。

| API | 说明 |
|---|---|
| `calc_field(collection, target, expr) -> Result<FeatureCollection>` | 逐要素求值写入 target 字段（不存在新建/存在覆盖；错误带要素序号） |
| `add_field(collection, name, default)` | 添加字段（全部要素写默认值；已存在报错） |
| `delete_field(collection, name)` | 删除字段（幂等） |
| `rename_field(collection, old, new)` | 重命名字段（旧名不存在/新名冲突报错） |

表达式语法：数值/字符串（`'…'` 或 `"…"`）/布尔/null 字面量；字段引用（裸标识符
含中文，或 `[字段名]`）；几何虚列 `$area`（测地面积 ㎡）/`$length`（测地长度 m）/
`$x`/`$y`（点坐标或质心）；`+ - * / %`（`+` 对字符串为拼接）、比较
`= != < <= > >=`、逻辑 `and or not`、括号；函数 `abs/round(x[,n])/floor/ceil/sqrt/
power/min/max/upper/lower/length/trim/concat/coalesce`。


## 16. parcel —— 宗地 TXT（界址点坐标）

移植自堪舆工具箱 `txt_feature.py`（行为互认）。格式：双段（[属性描述]
key=value + [地块坐标] 说明行与点行），X北Y东测绘惯例（GeoJSON 位置=[Y,X]），
圈号 1=外环/2+=洞。

| API | 说明 |
|---|---|
| `parse_parcel_txt(text) -> Result<ParcelDoc>` | 解析+完整校验（首尾闭合/点数一致/面积非零，中文错误带行号） |
| `parcel_doc_to_collection(&doc) -> FeatureCollection` | 地块 → Polygon（属性 parcel_id/name/use/map_sheet/area） |
| `collection_to_parcel_txt(&collection, decimals, crs) -> Result<String>` | 面要素 → 宗地 TXT（闭合点复用首点编号） |
| `validate_parcel_txt(text) -> Vec<QualityIssue>` | 质检（表头必备项/中文逗号/空格/结构；警告不阻塞） |
| `parse_points_txt(text) -> Result<FeatureCollection>` | 简单点表（name X Y [Z]，# 注释，逗号/空格/Tab）回退解析 |
| `is_parcel_txt(text) -> bool` | 格式嗅探（全角段标兼容） |

## 17. kanyu-py —— Python 桥接（PyO3）

扩展模块 `kanyu`（crate-type cdylib；构建产物 `kanyu.dll` → `python/kanyu/kanyu.pyd`）。
函数面（GeoJSON 文本进出）：`load/query/buffer/overlay/topology/sjoin/
zonal_stats/dissolve/simplify/centroid/convex_hull/delete_holes/explode/
stats/measure/reproject/render_png/render_svg/export/version`。
Python 侧 `Layer` 链式封装与工具箱运行时见 [SDK.md](SDK.md) §4。
