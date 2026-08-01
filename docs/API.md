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
10. [kanyu-gene —— WASM 基因系统宿主](#10-kanyu-gene--wasm-基因系统宿主)

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

## 10. kanyu-gene —— WASM 基因系统宿主

兄弟 crate（CLI 依赖它执行基因；MCP 热加载接线 📋）。总规 §4.5"以 WASM
为基因"落地：wasmtime 47 组件模型 + WIT 强类型 ABI + fuel 配额沙箱。
ABI 定义见 [`crates/kanyu-gene/wit/gene.wit`](../crates/kanyu-gene/wit/gene.wit)：
`meta() -> string`（元数据 JSON）与
`run(input: string) -> result<string, string>`（FeatureCollection JSON 进/出）。

### `GeneHost` / `Gene` / `GeneMeta`

| 项 | 签名 | 说明 |
|---|---|---|
| `GeneHost::new` | `fn new() -> Result<GeneHost, GeneError>` | 构造宿主（`Config::consume_fuel(true)`） |
| `GeneHost::load` | `fn load(&self, path: &str) -> Result<Gene, GeneError>` | 编译校验 → 实例化 → 调 `meta()` 取元数据并校验（无效 wasm/接口不匹配/元数据非法各报中文错误） |
| `GeneHost::run` | `fn run(&self, gene: &Gene, input: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection, GeneError>` | 沙箱执行：每次重置 fuel（10 亿），trap/配额/结果非法独立报错 |
| `Gene::meta` | `fn meta(&self) -> &GeneMeta` | 元数据 |

### `GeneMeta`

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | `String` | 基因名（非空校验） |
| `version` | `String` | 版本号（非空校验） |
| `capabilities` | `Vec<String>` | 能力清单（如 `["analyzer"]`） |

### `GeneError`

| 变体 | 触发 |
|---|---|
| `LoadFailed { path, reason }` | wasm 编译失败 / 实例化失败（WIT 接口不匹配） |
| `MetaInvalid(String)` | `meta()` 调用失败 / 非合法 JSON / 必填字段缺失 |
| `Trap(String)` | 执行期 trap（含基因主动返回的业务错误） |
| `Timeout(String)` | fuel 耗尽（疑似死循环或过重计算） |
| `ResultInvalid(String)` | 返回值非合法 FeatureCollection |

**沙箱模型**：组件无 WASI 导入（纯计算，无文件/网络/环境访问）；
fuel 配额覆盖纯计算死循环；组件无 IO 导入故不设墙钟超时（注释即契约）。

**编写基因**（Rust guest）：参照样板
[`crates/kanyu-gene/testdata/attr_scaler/`](../crates/kanyu-gene/testdata/attr_scaler/)——
`wit-bindgen` `generate!`/`export!` 实现 `kanyu:gene/analyzer` 的 `Guest`
trait，`wasm32-unknown-unknown` 编译后用 `wasm-tools component new` 组件化。

**MCP 接线（已落地）**：`kanyu_system_hotload`（热加载校验注册）、
`kanyu_gene_run`（沙箱执行，可 `task: true` 异步）、`kanyu_gene_list`
（注册表快照）——字段与输出见 [MCP.md](MCP.md#35-kanyu_system_hotload)。
