# kanyu-core Rust API 参考（API）

> 版本：v0.1.0 ｜ 本文与 `cargo doc --no-deps -p kanyu-core` 生成的文档同源，
> 侧重"类型地图 + 用法示例"。集成指南见 [SDK.md](SDK.md)；架构背景见 [ARCHITECTURE.md](ARCHITECTURE.md)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [crate 总览](#1-crate-总览)
2. [format —— 格式注册表](#2-format--格式注册表)
3. [layer —— 图层内存模型](#3-layer--图层内存模型)
4. [agents —— AGENTS.md 语义](#4-agents--agentsmd-语义)
5. [introspect —— 系统自省](#5-introspect--系统自省)
6. [error —— 错误处理约定](#6-error--错误处理约定)

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

v0.1 以 GeoJSON `FeatureCollection` 为原生载体；后续迁移 GeoArrow `RecordBatch`
时本模块 API 不变（迁移设计见 [ARCHITECTURE.md](ARCHITECTURE.md#3-数据流)）。

### `Layer`

| 方法 | 签名 | 说明 |
|---|---|---|
| `load` | `fn load(id: impl Into<String>, path: &str) -> Result<Self>` | 加载图层。格式自动探测；v0.1 原生支持 geojson、csv/tsv（坐标列自动识别 lon/lat/x/y/经度/纬度；xlsx 暂返回 `UnsupportedOperation`）与 shp（读取：Point/MultiPoint/Polyline/Polygon 含洞，dbase 属性类型化），桥接驱动格式返回 `UnsupportedOperation`；无法探测返回 `UnknownFormat` |
| `id` | `fn id(&self) -> &str` | 图层标识 |
| `len` | `fn len(&self) -> usize` | 要素数量 |
| `is_empty` | `fn is_empty(&self) -> bool` | 是否空图层 |
| `summary` | `fn summary(&self) -> LayerSummary` | 概要信息 |
| `collection` | `fn collection(&self) -> &geojson::FeatureCollection` | 底层要素集合（只读） |
| `query` | `fn query(&self, expression: &str) -> Result<geojson::FeatureCollection>` | 属性查询，语法见下 |
| `to_geojson_string` | `fn to_geojson_string(collection: &geojson::FeatureCollection) -> String` | 关联函数：集合 → GeoJSON 字符串 |
| `to_csv_string` | `fn to_csv_string(collection: &geojson::FeatureCollection) -> Result<String>` | 关联函数：集合 → CSV 字符串（`x,y` 坐标列仅 Point 取值，后接属性字段并集） |

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

## 4. agents —— AGENTS.md 语义

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

## 5. introspect —— 系统自省

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

## 6. error —— 错误处理约定

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
