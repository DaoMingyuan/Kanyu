# 堪舆 MCP 接口文档（MCP）

> 版本：v0.1.0 ｜ 基于官方 [rmcp](https://crates.io/crates/rmcp) 3.x SDK ｜ 传输：stdio（✅）
>
> 客户端接入示例见 [SDK.md](SDK.md#2-作为-mcp-客户端集成)；同名 CLI 命令见
> [CLI.md](CLI.md#kanyu-mcp-serve)；安全与命名背景见 [ARCHITECTURE.md](ARCHITECTURE.md#6-安全模型)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [快速开始](#1-快速开始)
2. [协议细节](#2-协议细节)
3. [工具参考（6 个，✅）](#3-工具参考6-个-)
4. [命名规范](#4-命名规范)
5. [设计原则](#5-设计原则)
6. [错误处理](#6-错误处理)
7. [规划中的能力](#7-规划中的能力)
8. [与现有 GIS MCP 项目的差异](#8-与现有-gis-mcp-项目的差异)

## 1. 快速开始

```bash
# 构建后直接启动（stdio 为默认传输）
kanyu mcp serve
# 等价于
kanyu mcp serve --transport stdio
```

客户端配置（Claude Desktop / Codex / Kimi 等 MCP 兼容客户端）：

```json
{
  "mcpServers": {
    "kanyu": {
      "command": "kanyu",
      "args": ["mcp", "serve", "--transport", "stdio"]
    }
  }
}
```

之后即可用自然语言驱动："*加载 buildings.geojson，找出所有高于 50 米的建筑并导出*"。
Windows 下 `kanyu` 不在 PATH 时用绝对路径（如 `target/release/kanyu.exe`）。

## 2. 协议细节

| 项 | 值 |
|---|---|
| 传输 | stdio（JSON-RPC 2.0，每行一条消息） |
| 协议版本 | `2025-06-18`（initialize 握手协商） |
| SDK | rmcp 3.x（`ServerInfo.name` 为 `"kanyu-mcp"`，version 随内核版本） |
| Server capabilities | 仅 `tools`（resources/prompts 📋） |
| Server instructions | "堪舆 (Kanyu) GIS 内核：data/agents/analysis/render/system 五组工具。所有结果为结构化 JSON 并携带 CRS/单位元数据；不提供任意代码执行。" |

initialize 握手（实测）：

```jsonc
// → 客户端
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}
// ← 服务器
{"jsonrpc":"2.0","id":1,"result":{
  "protocolVersion":"2025-06-18",
  "capabilities":{"tools":{}},
  "serverInfo":{"name":"kanyu-mcp","version":"0.1.0"},
  "instructions":"堪舆 (Kanyu) GIS 内核：…不提供任意代码执行。"}}
// → 客户端
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

**结构化输出（structuredContent）**：每个工具按 MCP 2025-06-18 规范返回
`structuredContent`（JSON 对象），同时在 `content[0].text` 携带同一对象的
JSON 字符串副本，兼容旧客户端：

```jsonc
// tools/call kanyu_data_query 的 result（实测，已折叠）
{
  "content": [{"type":"text","text":"{\"feature_count\":2,\"collection\":{…}}"}],
  "structuredContent": {"feature_count":2, "collection":{…}},
  "isError": false
}
```

所有工具**无状态**：每次调用重新加载文件、执行、返回。读操作天然幂等。

## 3. 工具参考（6 个，✅）

`tools/list` 按字母序返回；每个工具带中文 description 与 schemars 生成的
inputSchema（JSON Schema draft 2020-12；可选字段类型为 `["string","null"]` 且不列入 required）。

### 3.1 `kanyu_data_load`

> 加载地理数据文件到内存，返回图层概要（要素数、几何类型、字段清单）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径（格式自动探测） |
| `alias` | string \| null | 否 | 图层别名（默认取文件名主干） |
| `crs` | string \| null | 否 | 坐标系声明（如 `EPSG:4326`）；提供时随结果返回 |

输出（`structuredContent`，即 `LayerSummary` + 可选 `crs`）：

```json
{
  "id": "buildings",
  "format": "geojson",
  "feature_count": 4,
  "geometry_types": ["LineString", "Point"],
  "fields": ["grade", "height", "name", "usage", "width"],
  "crs": "EPSG:4326"
}
```

### 3.2 `kanyu_data_query`

> 对数据文件执行属性过滤查询（如 height > 50），返回 GeoJSON FeatureCollection 与要素数。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `filter` | string | 是 | 过滤表达式 `"field op value"`，`op ∈ == != > >= < <=`，数值/字符串比较（语法见 [API.md](API.md#3-layer--图层内存模型)） |

输出（实测形状）：

```json
{
  "feature_count": 2,
  "collection": {
    "type": "FeatureCollection",
    "features": [
      {"type":"Feature","geometry":{"type":"Point","coordinates":[116.3914,39.9072]},
       "properties":{"height":88.5,"name":"示例大厦A","usage":"office"}}
    ]
  }
}
```

### 3.3 `kanyu_data_export`

> 将数据文件导出为目标格式（当前原生支持 geojson/csv/fgb/geoparquet/dxf/kml；其余格式受格式能力矩阵与驱动状态约束）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `format` | string | 是 | 目标格式短名（`geojson`/`csv`/`fgb`/`geoparquet`/`dxf`/`kml`/…，受能力矩阵约束；`fgb`/`geoparquet` 为二进制写出，列 schema 自动推断；`dxf` 暂不写出属性） |
| `out` | string | 是 | 输出路径 |

输出（导出成功时）：

```json
{"exported": 4, "format": "geojson", "out": "deliver.geojson"}
```

驱动未启用的格式返回结构化错误（决策路径见
[ARCHITECTURE.md](ARCHITECTURE.md#4-格式注册表设计)）：
`format 'dwg' does not support operation 'native-export (driver libredwg-wasm not enabled)'`。

### 3.4 `kanyu_system_introspect`

> 系统自省：返回堪舆内核版本、模块清单、格式能力矩阵与 MCP 工具清单（AI 读取自身）。

无输入参数。输出即 `Introspection` 报告（字段见 [API.md](API.md#5-introspect--系统自省)）：

```jsonc
{
  "version": "0.1.0",
  "codename": "kanyu-spirit",
  "manifesto": "以天地为盘，以数据为爻，以 AI 为神，重构地理空间之道。",
  "modules": [{"name":"kanyu-core","role":"数据心脏：…","status":"stable"}, …],
  "formats": [{"id":"geojson","read":"full","write":"full","symbol":"partial",…}, …],
  "tools":   [{"name":"kanyu_data_load","group":"data","status":"stable"}, …]
}
```

> 注意：`tools` 清单中的 `name` 即真实 MCP 工具名（下划线式，见 §4），
> 并包含 📋 planned 工具（尚不可调用，用于能力宣告）。

### 3.5 `kanyu_agents_validate`

> 校验 AGENTS.md 项目语义文件：元数据（name/crs）、图层语义、业务规则完整性。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | AGENTS.md 路径 |

输出：`{"valid": true, "issues": []}`（`issues` 为中文问题清单，空 = 通过；
校验规则见 [ARCHITECTURE.md](ARCHITECTURE.md#5-agentsmd-语义层设计)）。

### 3.6 `kanyu_agents_init`

> 在指定项目目录生成 AGENTS.md 语义模板（图层/CRS/业务规则，供 AI 理解项目）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `project` | string | 是 | 项目目录 |
| `name` | string \| null | 否 | 项目名（默认取目录名） |
| `crs` | string \| null | 否 | 坐标参考系（默认 `EPSG:4326`） |

输出：`{"created": "<project>/AGENTS.md"}`。目标已存在时返回 `invalid_params`
错误（"…已存在"），不会静默覆盖。

### 3.7 `kanyu_analysis_buffer`

> 对数据文件做缓冲区分析（distance 单位为数据 CRS 单位；EPSG:4326 下是度而非米，米制缓冲需先投影），属性随行。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `distance` | number | 是 | 缓冲距离（数据 CRS 单位） |
| `segments` | integer \| null | 否 | 圆弧拟合的每象限分段数（默认 8） |

输出（`structuredContent`）：

```json
{
  "feature_count": 4,
  "skipped": 0,
  "collection": {"type": "FeatureCollection", "features": [{"type":"Feature","geometry":{"type":"MultiPolygon","coordinates":[…]},"properties":{"name":"示例大厦A",…}}]}
}
```

### 3.8 `kanyu_analysis_overlay`

> 叠加分析（仅 Polygon/MultiPolygon 面要素；target×overlay 逐要素对布尔、未做跨对融合；非面要素返回中文错误并指出序号）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `target` | string | 是 | 目标图层文件路径 |
| `overlay` | string | 是 | 叠加图层文件路径 |
| `operation` | string | 是 | `union` / `intersection` / `difference` / `xor` |

输出：`{"feature_count": 1, "collection": {…}}`；结果属性 = target 属性 +
overlay 属性（键冲突加 `overlay_` 前缀；difference 仅 target 属性）。

### 3.9 `kanyu_analysis_topology`

> 拓扑检查（面要素两两交集面积 > 1e-10 判违规；非面要素跳过）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `rules` | string[] | 是 | 规则清单（当前支持 `no_overlap`） |

输出（`TopologyReport`，字段见 [API.md](API.md#4-analysis--空间分析内核)）：

```json
{"rule":"no_overlap","feature_count":2,"violation_count":1,
 "violations":[{"feature_a":0,"feature_b":1,"note":"面要素重叠，交集面积 4.000000"}]}
```

### 3.10 `kanyu_data_reproject`

> 坐标投影变换（内置 EPSG 数据库；经纬度自动衔接度/弧度，z 不变）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `from` | string | 是 | 源 CRS（`EPSG:xxxx` / proj4 定义串 / `WGS84`） |
| `to` | string | 是 | 目标 CRS（同 from 格式） |
| `out` | string \| null | 否 | 输出路径；缺省返回转换后的 FeatureCollection |

输出（out 缺省时）：

```json
{"feature_count": 4, "collection": {"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[12956631.38,4852465.99]},"properties":{…}}]}}
```

提供 `out` 时：`{"reprojected": 4, "out": "bj3857.geojson"}`。

### 3.11 `kanyu_analysis_measure`

> 测地线度量（Karney 2013，WGS84 椭球；输入应为经纬度数据如 EPSG:4326，投影数据请先 `kanyu_data_reproject` 回地理 CRS）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `kind` | string | 是 | `length`（米）/ `area`（平方米） |

输出：

```json
{"kind":"length","unit":"m","total":2802.82,
 "per_feature":[{"index":0,"value":0.0},{"index":3,"value":2802.82},…]}
```

## 4. 命名规范

MCP 规范限制工具名为 `[a-zA-Z0-9_-]`（不允许点号）。因此总规
[MASTERPLAN.md](MASTERPLAN.md) §4.2 的点式命名按下述规则落地：

| 总规逻辑命名 | MCP 实际工具名（introspect 清单同名） | 状态 |
|---|---|---|
| `kanyu.data.load` | `kanyu_data_load` | ✅ |
| `kanyu.data.query` | `kanyu_data_query` | ✅ |
| `kanyu.data.export` | `kanyu_data_export` | ✅ |
| —（data 组扩展，裁决后增） | `kanyu_data_reproject` | ✅ |
| `kanyu.system.introspect` | `kanyu_system_introspect` | ✅ |
| —（agents 组，总规后增） | `kanyu_agents_validate` | ✅ |
| — | `kanyu_agents_init` | ✅ |
| `kanyu.analysis.buffer/overlay/topology` | `kanyu_analysis_*` | ✅ |
| —（analysis 组扩展） | `kanyu_analysis_measure` | ✅ |
| `kanyu.render.symbolize/camera` | `kanyu_render_*` | 📋 |
| `kanyu.system.generate/hotload` | `kanyu_system_*` | 📋 |

即：点号映射为下划线，分组（data/analysis/render/system/agents）保留。

## 5. 设计原则

1. **确定性**：同一输入恒得同一输出；无隐藏会话状态，可审计、可回放。
2. **无 `execute_code`**：只暴露声明式工具，拒绝任意代码执行——这是与
   qgis_mcp 系 Python 薄壳的本质区别（见 §8）。
3. **CRS/单位元数据随行**：`kanyu_data_load` 的 `crs` 声明随结果返回；
   AGENTS.md 把坐标系列为项目强制项，AI 不会"猜坐标系"。
4. **能力矩阵驱动**：导出等操作先过 `FormatRegistry::require`，失败即返回
   结构化错误，AI 可据此选择替代格式，而不是盲试。

## 6. 错误处理

内核 `KanyuError` 映射为 MCP 协议错误（`internal_error`，message 为中文/英文错误描述）；
参数层面错误（如 `agents_init` 目标已存在）映射为 `invalid_params`。
`isError: true` 仅在工具执行失败时出现；握手/路由错误走 JSON-RPC error 帧。

## 7. 规划中的能力

| 能力 | 状态 | 说明 |
|---|---|---|
| analysis 工具组扩展 | 📋 | sjoin / zonal_stats / MCP tasks 长任务；buffer/overlay/topology（§3.7–3.9）与 reproject/measure（§3.10–3.11）已 ✅ |
| render 工具组 | 📋 | `kanyu_render_symbolize` / `camera`（随 kanyu-render/wgpu 落地） |
| system 工具组扩展 | 📋 | `kanyu_system_generate` / `hotload`（代码生成→WASM 沙箱流水线，须人类审核） |
| MCP tasks | 📋 | SEP-1686 长任务：大文件导入、批量导出等异步化，进度可查询 |
| resources | 📋 | `layer://<id>`、`crs://EPSG/4326` 等资源只读暴露 |
| prompts | 📋 | 常用工作流提示模板（制图/分析/导出） |
| SSE / streamable HTTP | 📋 | `kanyu mcp serve --transport sse` 当前返回提示，v0.2 由 rmcp streamable HTTP 提供 |

## 8. 与现有 GIS MCP 项目的差异

| | qgis_mcp 等 Python 薄壳 | gis-mcp 类库 | **堪舆 kanyu-mcp** |
|---|---|---|---|
| 内核 | QGIS（C++/Qt）进程外挂 | Python 库（shapely/geopandas） | 纯 Rust 内核，零 C 依赖 |
| 数据进出 | WKT 字符串 / 文件路径约定 | WKT / GeoJSON 文本 | **结构化 JSON（structuredContent）+ GeoJSON 对象** |
| 任意代码执行 | 常见 `execute_code` 工具（安全隐患） | 部分存在 | **无**，声明式工具白名单 |
| 元数据 | 坐标系/单位靠 prompt 约定 | 同左 | **CRS/单位随结果返回**，AGENTS.md 项目级强制 |
| 能力协商 | 无 | 无 | **格式能力矩阵**驱动，失败返回结构化错误 |
| 运行时 | Python + GIL + QGIS 依赖 | Python + GIL | 单二进制 stdio，亚秒启动 |
| 长任务 | 无 | 无 | MCP tasks（SEP-1686）📋 |
