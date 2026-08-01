# kanyu CLI 参考（CLI）

> 版本：v0.1.0 ｜ `kanyu` 是堪舆系统的脊髓：数据、自省、AGENTS.md 与 MCP 的统一入口。
> 库 API 见 [API.md](API.md)；脚本集成约定见 [SDK.md](SDK.md#3-作为-cli-脚本集成)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [安装](#1-安装)
2. [全局约定](#2-全局约定)
3. [kanyu data](#3-kanyu-data)
4. [kanyu introspect](#4-kanyu-introspect)
5. [kanyu agents](#5-kanyu-agents)
6. [kanyu mcp serve](#6-kanyu-mcp-serve)
7. [计划中的命令（📋）](#7-计划中的命令-)

## 1. 安装

```bash
# 需要 Rust 1.88+（rust-toolchain.toml 锁定）
cargo install --path crates/kanyu-cli     # 安装到 ~/.cargo/bin/kanyu
# 或仅构建：
cargo build --release                     # 产物 target/release/kanyu（Windows 为 kanyu.exe）
```

以下示例均以仓库根目录为工作目录，使用 `target/debug/kanyu.exe` 与示例数据
`examples/buildings.geojson`（4 个要素：3 个 Point 建筑 `height/usage` + 1 个 LineString 道路）。

## 2. 全局约定

| 约定 | 内容 |
|---|---|
| `--json` | 全局标志，可置于任意子命令后；机器可读 JSON 输出到 **stdout** |
| stdout / stderr 分工 | stdout 只放机器数据；进度、提示、错误等人读信息走 **stderr** |
| 退出码 | `0` 成功；非 `0` 失败（stderr 含错误描述，格式类错误为结构化错误文案） |

## 3. kanyu data

数据操作：加载、查询、导出、检视。

### 3.1 `kanyu data info <file>` ✅

检视数据文件：格式探测、要素数、字段清单。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径（位置参数） |

```bash
$ ./target/debug/kanyu.exe data info examples/buildings.geojson
图层:      buildings
格式:      geojson
要素数:    4
几何类型:  LineString, Point
字段:      grade, height, name, usage, width
```

```bash
$ ./target/debug/kanyu.exe data info examples/buildings.geojson --json
{
  "id": "buildings",
  "format": "geojson",
  "feature_count": 4,
  "geometry_types": ["LineString", "Point"],
  "fields": ["grade", "height", "name", "usage", "width"]
}
```

（`--json` 输出为 `LayerSummary`，字段定义见 [API.md](API.md#layersummary)。）

### 3.2 `kanyu data load <file>` ✅

加载数据文件到会话（打印图层概要）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--as <alias>` | 图层别名（默认取文件名主干） |
| `--crs <crs>` | 坐标系声明（如 `EPSG:4326`） |

```bash
$ ./target/debug/kanyu.exe data load examples/buildings.geojson --as buildings --crs EPSG:4326
已加载图层 'buildings'：4 个要素（geojson），CRS 声明 EPSG:4326
```

加 `--json` 时输出 `LayerSummary`（同 `data info --json`）。

### 3.3 `kanyu data query <file> --filter <expr>` ✅

对数据执行属性查询。表达式 `"field op value"`，`op ∈ == != > >= < <=`
（数值字段按数值比较，其余按字符串；语法见 [API.md](API.md#3-layer--图层内存模型)）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--filter <expr>` | 过滤表达式（必填） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe data query examples/buildings.geojson --filter "height > 50"
{"type":"FeatureCollection","features":[
  {"type":"Feature","geometry":{"type":"Point","coordinates":[116.3914,39.9072]},
   "properties":{"height":88.5,"name":"示例大厦A","usage":"office"}},
  {"type":"Feature","geometry":{"type":"Point","coordinates":[116.4025,39.9155]},
   "properties":{"height":120.0,"name":"示例大厦C","usage":"office"}}]}
```

```bash
$ ./target/debug/kanyu.exe data query examples/buildings.geojson --filter "usage == residential" --output homes.geojson
已写出 1 个要素 → homes.geojson        # 该提示在 stderr
```

> 注意：`query` 的 stdout 恒为 GeoJSON 文本（一行），不受 `--json` 影响。

### 3.4 `kanyu data export <file> -f <format> --out <path>` ✅（原生支持 geojson / csv / fgb / geoparquet / dxf）

导出为目标格式，受格式能力矩阵约束（决策路径见
[ARCHITECTURE.md](ARCHITECTURE.md#4-格式注册表设计)）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `-f, --format <fmt>` | 目标格式短名（见 `kanyu introspect` 的格式矩阵） |
| `--out <path>` | 输出路径（必填） |
| `--symbol-mapping` | 保留符号化映射；目标格式 `symbol` 为 None 时直接报错 |

```bash
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f geojson --out deliver.geojson
已导出 4 个要素 → deliver.geojson (geojson)      # stderr
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f csv --out deliver.csv
已导出 4 个要素 → deliver.csv (csv)              # x,y 坐标列仅 Point 取值
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f fgb --out deliver.fgb
已导出 4 个要素 → deliver.fgb (fgb)              # 二进制，列 schema 自动推断，混合几何按 Unknown 异构声明
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f geoparquet --out deliver.parquet
已导出 4 个要素 → deliver.parquet (geoparquet)   # 二进制，WKB 几何编码 + geo 元数据
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f dxf --out deliver.dxf
已导出 4 个要素 → deliver.dxf (dxf)              # R2000 LWPOLYLINE/POINT，统一图层 "0"，属性暂不写出
$ echo $?
0
```

能力矩阵允许但驱动未启用时，返回结构化错误（退出码 1）：

```bash
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f dwg --out out.dwg
Error: 格式 'dwg' 的原生导出尚未启用（driver: libredwg-wasm）。
桥接/插件驱动将在对应阶段就绪后开放，见 docs/MASTERPLAN.md 第五部分。
```

矩阵不允许时更早被拦截：`kanyu data export ... -f wfs ...` →
`Error: format 'wfs' does not support operation 'write'`。

## 4. kanyu introspect ✅

系统自省 —— AI 读取自身（模块清单、能力矩阵、工具清单）。
输出即 `Introspection` 报告（字段见 [API.md](API.md#5-introspect--系统自省)）。

```bash
$ ./target/debug/kanyu.exe introspect
堪舆内核 v0.1.0 (kanyu-spirit)
以天地为盘，以数据为爻，以 AI 为神，重构地理空间之道。

模块:
  kanyu-core     [stable] 数据心脏：格式注册表、图层模型、AGENTS.md 语义、系统自省
  kanyu-cli      [stable] 脊髓：`kanyu` 命令行，数据/分析/自省/插件/MCP 入口
  kanyu-mcp      [incubating] 神经接口：MCP Server，向 AI 暴露全部内核能力
  kanyu-render   [planned] 眼睛：GPU 渲染管线（wgpu），GeoArrow→SSBO 直通
  kanyu-edit     [planned] 手：DCEL 增量拓扑编辑内核，Undo/Redo
  kanyu-gene     [planned] 基因：WASM 插件系统（wasmtime 沙箱 + 热加载）
  kanyu-shell    [planned] 壳层：桌面 UI（TitleBar/StatusBar/面板系统）

MCP 工具:
  kanyu_data_load              data      [stable]
  kanyu_data_query             data      [stable]
  kanyu_data_export            data      [stable]
  kanyu_agents_init            agents    [stable]
  kanyu_analysis_buffer        analysis  [planned]
  …
格式矩阵: 17 种格式（详见 --json 或 docs/）
```

`--json` 输出完整报告（含 17 种格式的五级能力矩阵与全部工具清单）。
工具清单中的名称即真实 MCP 工具名（下划线式，映射规则见
[MCP.md](MCP.md#4-命名规范)）。

## 5. kanyu agents

`AGENTS.md` 项目语义文件：生成与校验（语义层设计见
[ARCHITECTURE.md](ARCHITECTURE.md#5-agentsmd-语义层设计)）。

### 5.1 `kanyu agents init` ✅

在指定目录生成 AGENTS.md 模板。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--project <dir>` | `.` | 项目目录 |
| `--name <name>` | 目录名 | 项目名 |
| `--crs <crs>` | `EPSG:4326` | 坐标参考系 |
| `--force` | 关 | 覆盖已存在的 AGENTS.md |

```bash
$ ./target/debug/kanyu.exe agents init --project ./my_project --name 演示项目 --crs EPSG:4526
已生成 ./my_project/AGENTS.md            # stderr
# 已存在且未加 --force 时：Error: ...AGENTS.md 已存在（使用 --force 覆盖），退出码 1
```

### 5.2 `kanyu agents validate` ✅

校验 AGENTS.md 完整性（元数据 name/crs、图层语义、业务规则）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--path <file>` | `AGENTS.md` | 文件路径 |

```bash
$ ./target/debug/kanyu.exe agents validate --path ./my_project/AGENTS.md
AGENTS.md 校验通过：1 个图层，1 条业务规则
```

未通过时逐条问题打到 stderr，退出码 1：

```bash
$ ./target/debug/kanyu.exe agents validate --path ./empty.md
✗ 缺少项目元数据: name
✗ 缺少项目元数据: crs（坐标参考系是堪舆项目的强制项）
✗ 数据层语义表为空：AI 无法理解图层含义
Error: AGENTS.md 校验未通过：3 个问题
```

`--json` 输出（注意：该命令的 JSON 为单行紧凑格式）：

```json
{"document":{"business_rules":[...],"custom_tools":[...],"layers":[...],"meta":{...}},"issues":[],"valid":true}
```

## 6. kanyu mcp serve ✅

启动 MCP Server，供 AI 代理接入（协议与工具详见 [MCP.md](MCP.md)）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--transport <stdio\|sse>` | `stdio` | 传输方式 |
| `--port <port>` | `3000` | SSE 模式监听端口（预留） |

```bash
$ ./target/debug/kanyu.exe mcp serve
kanyu-mcp: MCP server 监听 stdio（initialize / tools/list / tools/call）   # stderr，随后阻塞服务
```

SSE 暂未实现，选择后得到提示（退出码 1）：

```bash
$ ./target/debug/kanyu.exe mcp serve --transport sse --port 3000
Error: SSE 传输（端口 3000）将在 kanyu-mcp v0.2 提供（rmcp streamable HTTP）；当前请使用 --transport=stdio
```

## 7. 计划中的命令（📋）

对应 [MASTERPLAN.md](MASTERPLAN.md) §4.3.1，随各 Phase 落地：

| 命令 | 状态 | 说明 |
|---|---|---|
| `kanyu analysis buffer/topology ...` | 📋 | 空间分析（geo crate：buffer/布尔/拓扑检查） |
| `kanyu codegen --prompt ... --target rust` | 📋 | AI 代码生成（Phase 4，须人类审核） |
| `kanyu plugin build/load <wasm>` | 📋 | WASM 基因构建与热加载（wasmtime 沙箱） |
| `kanyu benchmark --plugin ... --metric fps` | 📋 | A/B 基准对比 |
| `kanyu regression test --suite ...` | 📋 | 回归测试套件，性能阈值断言 |

当前 v0.1.0 命令全集即本文 §3–§6；以 `kanyu --help` 与 `kanyu introspect` 输出为准。
