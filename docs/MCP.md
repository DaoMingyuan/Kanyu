# 堪舆 MCP 接口文档（MCP）

> 版本：v0.1.0 ｜ 基于官方 [rmcp](https://crates.io/crates/rmcp) 3.x SDK ｜ 传输：stdio（✅）+ streamable HTTP（✅）
>
> 客户端接入示例见 [SDK.md](SDK.md#2-作为-mcp-客户端集成)；同名 CLI 命令见
> [CLI.md](CLI.md#kanyu-mcp-serve)；安全与命名背景见 [ARCHITECTURE.md](ARCHITECTURE.md#6-安全模型)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [快速开始](#1-快速开始)
2. [协议细节](#2-协议细节)
3. [长任务（SEP-2663）](#21-长任务sep-2663-)
4. [工具参考（✅）](#3-工具参考-)
5. [命名规范](#4-命名规范)
6. [设计原则](#5-设计原则)
7. [错误处理](#6-错误处理)
8. [规划中的能力](#7-规划中的能力)
9. [与现有 GIS MCP 项目的差异](#8-与现有-gis-mcp-项目的差异)

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

远程接入（streamable HTTP）：

```bash
kanyu mcp serve --transport http --port 3000
# kanyu-mcp streamable HTTP 监听 http://127.0.0.1:3000/mcp
# （⚠️ 无鉴权/TLS，远程暴露请自行加反代；Ctrl-C 停止）
```

支持 HTTP 传输的客户端将 URL 指向 `http://127.0.0.1:3000/mcp` 即可。
⚠️ 服务绑定 localhost 且**无鉴权/TLS（📋）**；暴露到局域网/公网前
必须自行加反向代理（nginx/caddy）与鉴权。

之后即可用自然语言驱动："*加载 buildings.geojson，找出所有高于 50 米的建筑并导出*"。
Windows 下 `kanyu` 不在 PATH 时用绝对路径（如 `target/release/kanyu.exe`）。

## 2. 协议细节

| 项 | 值 |
|---|---|
| 传输 | stdio（JSON-RPC 2.0，每行一条消息）；streamable HTTP（`http://127.0.0.1:<port>/mcp`，POST=JSON-RPC、GET=SSE 流、DELETE=会话终止；`Mcp-Session-Id` 头管理会话，内存会话存储） |
| 协议版本 | `2025-06-18`（initialize 握手协商） |
| SDK | rmcp 3.x（`ServerInfo.name` 为 `"kanyu-mcp"`，version 随内核版本） |
| Server capabilities | `tools` + `extensions["io.modelcontextprotocol/tasks"]`（SEP-2663 长任务 ✅；resources/prompts 📋） |
| Server instructions | "堪舆 (Kanyu) GIS 内核：data/agents/analysis/render/system 五组工具。所有结果为结构化 JSON 并携带 CRS/单位元数据；不提供任意代码执行。" |

streamable HTTP 实测（`kanyu mcp serve --transport http --port 39178`）：

```bash
$ curl -i -X POST http://127.0.0.1:39178/mcp \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"8"}}}'
HTTP/1.1 200 OK
content-type: text/event-stream
mcp-session-id: 39cd01a2-c69c-410a-9ab2-85107970dc78
…
data: {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-03-26",
      "capabilities":{"tools":{}},"serverInfo":{"name":"kanyu-mcp","version":"0.6.0"},…}}
# 后续请求带 Mcp-Session-Id 头；notifications/initialized → 202；
# tools/list 返回全部 19 个工具；tools/call 正常返回结构化结果；
# capabilities 另含 resources 与 prompts（§4A/§4B）。
```

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

## 2.1 长任务（SEP-2663，✅）

耗时分析工具支持协议级异步执行（`io.modelcontextprotocol/tasks`，
即原 SEP-1686 的现行编号；rmcp 3.1 服务端内置 TaskManager）。

**客户端前提**：initialize 的 `capabilities` 声明扩展
`"extensions": {"io.modelcontextprotocol/tasks": {}}`
（服务端未检测到该声明时拒绝返回任务句柄）。

**任务化触发**：白名单分析工具（`kanyu_analysis_buffer` / `overlay` /
`sjoin` / `zonal_stats` / `topology` / `kanyu_skill_run` /
`kanyu_toolbox_run`）的 `tools/call` arguments 增加
`"task": true`（非白名单工具带此键返回中文错误；其余工具忽略该键走同步路由）。

**生命周期**（协议方法，非工具）：

```jsonc
// → tools/call（带 "task": true）
// ← 立即返回任务句柄（resultType "task"）
{"jsonrpc":"2.0","id":2,"result":{
  "resultType":"task","taskId":"<uuid>","status":"working",
  "ttlMs":600000,"pollIntervalMs":1000}}
// → tasks/get（按 pollIntervalMs 轮询）
{"jsonrpc":"2.0","id":3,"method":"tasks/get","params":{"taskId":"<uuid>"}}
// ← 进行中：{"status":"working"}；完成：
{"jsonrpc":"2.0","id":3,"result":{
  "resultType":"complete","taskId":"<uuid>","status":"completed",
  "result":{"structuredContent":{"feature_count":1,"collection":{…}},…}}}
// tasks/cancel：协作取消（运行中的操作自行决定终态，可能仍 completed）
```

**结果形状**：`tasks/get` 完成时的 `result` 即该工具同步调用的完整
`CallToolResult`（structuredContent + content 双通道，与 §2 一致）。
**保留与持久性**：任务结果为内存态，TTL 10 分钟（惰性 TTL 清扫），
重启即丢；任务执行在 blocking 线程池（不阻塞 stdio 调度线程）。
未知 `taskId` 返回结构化错误。

## 3. 工具参考（✅）

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
  "extent": [116.39, 39.9, 116.41, 39.92],
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

> 将数据文件导出为目标格式（当前原生支持 geojson/csv/shp/fgb/geoparquet/dxf/kml/kmz/kdb；其余格式受格式能力矩阵与驱动状态约束）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `format` | string | 是 | 目标格式短名（`geojson`/`csv`/`fgb`/`geoparquet`/`dxf`/`kml`/`kmz`/`shp`/`kdb`，受能力矩阵约束；`fgb`/`geoparquet`/`kdb` 为二进制写出，`kdb` 经 RecordBatch 直通类型保真；`dxf` 暂不写出属性） |
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

### 3.5 `kanyu_system_hotload`

> 热加载 WASM 技能到内存注册表（wasmtime 沙箱：无 WASI 导入纯计算 + fuel 配额 10 亿；**校验失败绝不注册**——hotload 即"验证"职责）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `wasm_path` | string | 是 | WASM 技能文件路径（.wasm 组件，接口须为 `kanyu:skill@0.1.0/analyzer`） |

输出（`structuredContent`）：

```json
{"skill_id": "attr_scaler", "replaced": false,
 "meta": {"name": "attr_scaler", "version": "0.1.0", "capabilities": ["analyzer"]}}
```

`skill_id` 取技能的 `meta.name`；重名覆盖旧注册并返回 `"replaced": true`。
注册表为内存态，**重启即丢**（需重新 hotload）。

### 3.6 `kanyu_skill_run`

> 对已注册技能在数据文件上沙箱执行（FeatureCollection 进/出；arguments 带 `"task": true` 可异步执行，见 §2.1）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `skill_id` | string | 是 | 已注册的技能标识（`kanyu_system_hotload` 返回） |
| `path` | string | 是 | 数据文件路径 |

输出：`{"feature_count": 4, "collection": {…}}`。
未知 `skill_id` 返回中文错误（提示先 `kanyu_system_hotload`，或 `kanyu_skill_list` 查看）。

### 3.7 `kanyu_skill_list`

> 列出内存注册表中的全部 WASM 技能（快照）。

无输入参数。输出：

```json
{"genes": [{"skill_id": "attr_scaler", "version": "0.1.0", "capabilities": ["analyzer"]}]}
```

### 3.8 `kanyu_agents_validate`

> 校验 AGENTS.md 项目语义文件：元数据（name/crs）、图层语义、业务规则完整性。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | AGENTS.md 路径 |

输出：`{"valid": true, "issues": []}`（`issues` 为中文问题清单，空 = 通过；
校验规则见 [ARCHITECTURE.md](ARCHITECTURE.md#5-agentsmd-语义层设计)）。

### 3.9 `kanyu_agents_init`

> 在指定项目目录生成 AGENTS.md 语义模板（图层/CRS/业务规则，供 AI 理解项目）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `project` | string | 是 | 项目目录 |
| `name` | string \| null | 否 | 项目名（默认取目录名） |
| `crs` | string \| null | 否 | 坐标参考系（默认 `EPSG:4326`） |

输出：`{"created": "<project>/AGENTS.md"}`。目标已存在时返回 `invalid_params`
错误（"…已存在"），不会静默覆盖。

### 3.10 `kanyu_analysis_buffer`

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

### 3.11 `kanyu_analysis_overlay`

> 叠加分析（仅 Polygon/MultiPolygon 面要素；target×overlay 逐要素对布尔、未做跨对融合；非面要素返回中文错误并指出序号）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `target` | string | 是 | 目标图层文件路径 |
| `overlay` | string | 是 | 叠加图层文件路径 |
| `operation` | string | 是 | `union` / `intersection` / `difference` / `xor` |

输出：`{"feature_count": 1, "collection": {…}}`；结果属性 = target 属性 +
overlay 属性（键冲突加 `overlay_` 前缀；difference 仅 target 属性）。

### 3.12 `kanyu_analysis_topology`

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

### 3.13 `kanyu_data_reproject`

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

### 3.14 `kanyu_analysis_measure`

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

### 3.15 `kanyu_analysis_sjoin`

> 空间连接（左连接 + 匹配展开：保留全部 target 要素、一对多匹配各输出一条；属性合并、键冲突加 `join_` 前缀并附 `join_index`）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `target` | string | 是 | 目标图层文件路径 |
| `join` | string | 是 | 连接图层文件路径 |
| `predicate` | string | 是 | `intersects` / `contains` / `within`（contains=target 包含 join，within=join 包含 target） |

输出：`{"feature_count": 4, "collection": {…}}`；无匹配的 target 要素
join 侧属性与 `join_index` 缺省。

### 3.16 `kanyu_analysis_zonal_stats`

> 分区统计（values 按质心/代表点归属 zones 面要素，一值多区取首个匹配；zones 追加 `{field}_{stat}` 统计列）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `zones` | string | 是 | 分区图层文件路径（仅面要素） |
| `values` | string | 是 | 数值图层文件路径 |
| `field` | string | 是 | 数值字段名（缺失或非数值时报中文错误） |
| `stats` | string[] | 是 | 统计项清单（`count`/`sum`/`mean`/`min`/`max`） |

输出：

```json
{"feature_count": 2, "unzoned_count": 1,
 "collection": {"type":"FeatureCollection","features":[{"type":"Feature","geometry":{…},
  "properties":{"name":"z1","height_count":2,"height_sum":30.0,"height_mean":15.0}}]}}
```

### 3.17 `kanyu_render_map`

> 离屏渲染数据文件为地图图片（AI 代理可直接"看见"数据；晨山/夜观星主题，见 [MASTERPLAN.md](MASTERPLAN.md) §1.2）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 数据文件路径 |
| `format` | string | 是 | `png`（base64 图片回传）/ `svg`（源码文本回传） |
| `width` | integer \| null | 否 | 图片宽度（像素，默认 800） |
| `height` | integer \| null | 否 | 图片高度（像素，默认 600） |
| `theme` | string \| null | 否 | `light`（晨山，默认）/ `dark`（夜观星） |
| `style` | object \| null | 否 | 属性驱动样式规则（缺省走主题默认样式）：`{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}` 数值分档（取最后满足 值≥阈值 的档，阈值严格升序），或 `{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E"},"default":"#888888"}` 类别映射 |

返回（不写文件，直接回传内容）：

- `format=png`：`content` 两块——`image`（base64 `image/png`）+ `text`
  （摘要 JSON 字符串）；客户端可直接展示图片。
- `format=svg`：`content` 两块——`text`（SVG 源码）+ `text`（摘要 JSON）。
- `structuredContent` 恒为摘要：
  `{"feature_count": 4, "bbox": null, "width": 800, "height": 600, "theme": "light", "format": "png"}`。

### 3.18 QGIS 核心算法工具组（v0.16 移植）

均为"文件进、结果出"形态（`path` 必填），返回
`{"feature_count": n, "collection": {...}}`（stats/validate 例外）：

| 工具 | 额外输入 | 说明 |
|---|---|---|
| `kanyu_analysis_dissolve` | `field`（可空） | 融合：按字段分组并集（keep-first 属性） |
| `kanyu_analysis_simplify` | `tolerance`（必填） | 道格拉斯简化（退化剔除） |
| `kanyu_analysis_centroid` | — | 质心（属性随行） |
| `kanyu_analysis_convex_hull` | — | 凸包 |
| `kanyu_analysis_delete_holes` | `min_area`（可空） | 删洞（缺省全删） |
| `kanyu_analysis_explode` | — | 多部件炸开（属性复制） |
| `kanyu_analysis_stats` | — | 图层统计 JSON（测地线口径；亩/公顷/km²） |
| `kanyu_data_validate` | — | 宗地 TXT 质检：`{"valid": bool, "issue_count": n, "issues": [...]}`（警告不影响 valid） |

### 3.19 `kanyu_toolbox_list`

> 工具箱注册表发现入口——`kanyu-core::tooldef` 同一注册表的 MCP 投影
> （壳层工具箱 / kanyu-py SDK / MCP 三面一处声明，消除漂移）。

无输入。返回 `{"count": 37, "tools": [ToolDef, …]}`；每个 ToolDef 含
`id` / `name`（中文名）/ `category` / `desc` / `params`（`key`/`label`/
`kind`/`required`/`hint`/`default`/`help`）/ `report`（true=报告类输出终端文本，
false=产出新图层）。

### 3.20 `kanyu_toolbox_run`

> 按注册表统一执行工具箱工具（toolrun 下沉入口）；arguments 带
> `"task": true` 可任务化执行（§2.1 白名单成员）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `tool_id` | string | 是 | 工具 id（见 `kanyu_toolbox_list`） |
| `params` | string[] | 否 | 参数值，按注册表参数序对齐；空串或缺位 = 取参数 `default`；枚举参数取**中文标签**（如 `"相交"`）；`LinearUnit` 参数取 `数值\|单位`（如 `"100\|米"`、`"0.1\|度"`，米/千米换算为米、度直通 CRS 单位） |
| `layers` | object[] | 否 | 图层注入清单 `[{"id": "pts", "path": "a.geojson"}]`；`Layer` 类参数按 id 引用（文件路径加载，格式自动探测，与 `kanyu_data_load` 同一加载器） |

返回（结构化 JSON）：

- 新图层类：`{"type": "new_layer", "verb": "缓冲区", "layers": {"buf_pts": {…GeoJSON…}}}`
  （新图层命名为 `前缀_源图层id`，由调用方结算落层；多产出如分割矢量图层为
  `"type": "new_layers"` 同形多键）；
- 报告类：`{"type": "report", "report": "…文本…"}`；
- 错误为中文结构化错误（未知工具 / 图层不存在 / 参数个数与校验失败等）。

### 3.21 不动产制图工具组（v0.23 不动产制图）

CLI `kanyu render parcel-map` / `render parcel-dxf` / `data kdb-pack` 的
MCP 同源投影（kanyu-core `cartography`/`cass`/`kdb` + kanyu-render `parcelmap`）。

#### `kanyu_render_parcel_map`

> 宗地图出图（GB/T 42547-2023《地籍调查规程》图 L.3 版式；勘测定界图
> 注记契约排版——边长中点法线、点号角平分线朝外、SAT 避让，残余压盖
> 诚实回报于 `overlap_count`）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 宗地数据文件路径（面要素；多面缺省取面积最大者） |
| `format` | string | 是 | `svg`（源码文本回传）/ `png`（base64 图片回传） |
| `out` | string \| null | 否 | 同时落盘路径（`.svg`/`.png`） |
| `parcel_code` | string \| null | 否 | 宗地代码（缺省取属性 `parcel_id/ZDDM/zddm`） |
| `owner` | string \| null | 否 | 土地权利人（缺省 `owner/QLRMC/parcel_name`） |
| `map_sheet` | string \| null | 否 | 所在图幅号（缺省 `map_sheet/TFH`） |
| `area` | number \| null | 否 | 宗地面积㎡（缺省 `area/ZDMJ`，再无现算） |
| `land_use` | string \| null | 否 | 地类编码（缺省 `parcel_use/YT`） |
| `unit_name` | string \| null | 否 | 左侧竖排单位名 |
| `survey_note` | string \| null | 否 | 左下测绘说明 |
| `drawer` / `reviewer` | string \| null | 否 | 制图者 / 审核者 |
| `draw_date` / `review_date` | string \| null | 否 | 制图 / 审核日期 |
| `scale` | integer \| null | 否 | 比例尺分母（缺省自动取整百） |
| `dpi` | number \| null | 否 | PNG 分辨率（默认 150，SVG 忽略） |
| `index` | integer \| null | 否 | 面要素文档序序号（0 起；缺省面积最大者） |
| `sizhi_e` / `sizhi_s` / `sizhi_w` / `sizhi_n` | string \| null | 否 | 四至/邻宗地注记（缺省取属性 `ZDSZD/ZDSZN/ZDSZX/ZDSZB`；`\n` 分行） |
| `roads` | string \| null | 否 | 相邻道路线文件路径（线要素；路名取属性 `name/NAME/road_name/道路名称/DLMC`） |

返回：`format=png` → `content` = `image`（base64）+ `text`（摘要 JSON）；
`format=svg` → `content` = `text`（SVG 源码）+ `text`（摘要 JSON）；
`structuredContent`：`{"scale": 700, "label_count": 18, "overlap_count": 0, "format": "png", "out": null}`。

#### `kanyu_render_parcel_dxf`

> 宗地 CASS 兼容 DXF 导出（南方 CASS 联动：ZD/JZX/JZD/ZJ 分层 +
> SOUTH 编码 XDATA 302001/302002，CASS 直接打开编辑且可被堪舆回读）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `path` | string | 是 | 宗地数据文件路径（同上） |
| `out` | string | 是 | 输出路径（`.dxf` 落盘） |
| `parcel_code` / `land_use` / `owner` | string \| null | 否 | 覆盖属性拾取（分式/注记） |
| `scale` | integer \| null | 否 | 出图比例尺分母（默认 1000） |
| `no_xdata` | boolean \| null | 否 | 不挂 SOUTH XDATA（默认 false=挂载） |
| `index` | integer \| null | 否 | 面要素文档序序号（0 起） |

返回：`{"out": "…", "boundary_points": 9, "boundary_lines": 9, "xdata": true, "scale": 1000, "bytes": 26731}`。

#### `kanyu_data_kdb_pack`

> 多图层打包为堪舆数据库（KDB v2 zip 容器：每输入文件成为一个命名图层，
> 图层名=文件主干；面向不动产登记数据库标准多表形态单文件建库）。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `files` | string[] | 是 | 输入数据文件（任意注册格式，可多个；重名主干报错） |
| `out` | string | 是 | 输出路径（`.kdb`） |

返回：`{"out": "…", "format": "kdb", "format_version": "2", "layer_count": 3, "layers": [{"name": "…", "rows": n}]}`。
读取侧：`kanyu_data_load` 对 v2 容器取清单首图层。

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
| —（analysis 组扩展） | `kanyu_analysis_sjoin` / `kanyu_analysis_zonal_stats` | ✅ |
| `kanyu.render.camera` | `kanyu_render_*` | 📋 |
| —（render 组首工具；符号化并入其 style 参数，裁决 #17） | `kanyu_render_map` | ✅ |
| —（不动产制图组，v0.23） | `kanyu_render_parcel_map` / `kanyu_render_parcel_dxf` | ✅ |
| —（不动产建库，KDB v2） | `kanyu_data_kdb_pack` | ✅ |
| `kanyu.system.generate` | `kanyu_system_*` | 📋 |
| `kanyu.system.hotload` | `kanyu_system_hotload` | ✅ |
| —（gene 组，Phase 5 落地） | `kanyu_skill_run` / `kanyu_skill_list` | ✅ |
| —（toolbox 组，tooldef 注册表投影） | `kanyu_toolbox_list` / `kanyu_toolbox_run` | ✅ |

即：点号映射为下划线，分组（data/analysis/render/system/agents）保留。

## 4A. Resources（只读资源，✅）

`resources/list` 返回静态资源，`resources/templates/list` 返回 URI 模板，
`resources/read` 读取（内容均为 `application/json` 文本）：

| URI | 内容 |
|---|---|
| `kanyu://formats` | 格式注册表能力矩阵（FormatRegistry::builtin，读写/符号化/驱动列） |
| `kanyu://tools` | MCP 工具清单（introspect 单一事实来源，名称/分组/状态） |
| `kanyu://crs/{code}`（模板） | EPSG 条目：`{"code","name","kind","unit","proj4"}`（名称/类型/单位/proj4 定义） |

错误均为中文 `invalid_params`：未知 URI / 代码非数值 / 代码不在内置库
（7507 条）。`kanyu://layer/{path}` **暂缓**：文件路径入 URI 需要百分号
编码与路径穿越约束（授权根外拒绝），其安全权衡待与资源订阅一并裁决；
图层数据当前经工具参数（path）通道已完备。

## 4B. Prompts（中文分析流模板，✅）

`prompts/list` 返回模板清单，`prompts/get` 以 arguments 做 `{参数}` 占位
替换（缺必填参数返回中文 `invalid_params`），消息引用真实工具名：

| name | 参数（必填） | 编排 |
|---|---|---|
| `data_health_check` | `path` | 数据体检：`kanyu_data_load` → `kanyu_analysis_topology` → `kanyu_analysis_stats`（TXT 再 `kanyu_data_validate`） |
| `buffer_analysis` | `path`、`distance` | 缓冲区分析流：概要 → 经纬度先 `kanyu_data_reproject` → `kanyu_analysis_buffer` → `kanyu_data_export` |
| `crs_transform` | `path`、`from`、`to` | 坐标系转换流：读 `kanyu://crs/{to}` → `kanyu_data_reproject` → 重载核对坐标量级 |

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
| analysis 工具组扩展 | 📋 | MCP tasks 长任务；buffer/overlay/topology（§3.10–3.12）、reproject/measure（§3.13–3.14）与 sjoin/zonal_stats（§3.15–3.16）已 ✅ |
| render 工具组 | 📋 | `kanyu_render_symbolize` / `camera`（随 kanyu-render/wgpu 落地） |
| system 工具组扩展 | 📋 | `kanyu_system_generate` / `hotload`（代码生成→WASM 沙箱流水线，须人类审核） |
| MCP tasks | ✅ | SEP-2663（`io.modelcontextprotocol/tasks`，原 SEP-1686 现行编号）协议级长任务，见 §2.1 |
| resources | 📋 | `layer://<id>`、`crs://EPSG/4326` 等资源只读暴露 |
| prompts | 📋 | 常用工作流提示模板（制图/分析/导出） |
| SSE / streamable HTTP | ✅ | `kanyu mcp serve --transport http`（§1/§2；官方 streamable HTTP 已取代旧 SSE）；tasks 长任务 📋 |

## 8. 与现有 GIS MCP 项目的差异

| | qgis_mcp 等 Python 薄壳 | gis-mcp 类库 | **堪舆 kanyu-mcp** |
|---|---|---|---|
| 内核 | QGIS（C++/Qt）进程外挂 | Python 库（shapely/geopandas） | 纯 Rust 内核，零 C 依赖 |
| 数据进出 | WKT 字符串 / 文件路径约定 | WKT / GeoJSON 文本 | **结构化 JSON（structuredContent）+ GeoJSON 对象** |
| 任意代码执行 | 常见 `execute_code` 工具（安全隐患） | 部分存在 | **无**，声明式工具白名单 |
| 元数据 | 坐标系/单位靠 prompt 约定 | 同左 | **CRS/单位随结果返回**，AGENTS.md 项目级强制 |
| 能力协商 | 无 | 无 | **格式能力矩阵**驱动，失败返回结构化错误 |
| 运行时 | Python + GIL + QGIS 依赖 | Python + GIL | 单二进制 stdio，亚秒启动 |
| 长任务 | 无 | 无 | MCP tasks（SEP-2663）✅ 协议级 |
