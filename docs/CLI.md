# kanyu CLI 参考（CLI）

> 版本：v0.1.0 ｜ `kanyu` 是堪舆系统的脊髓：数据、自省、AGENTS.md 与 MCP 的统一入口。
> 库 API 见 [API.md](API.md)；脚本集成约定见 [SDK.md](SDK.md#3-作为-cli-脚本集成)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [安装](#1-安装)
2. [全局约定](#2-全局约定)
3. [kanyu data](#3-kanyu-data)
4. [kanyu analysis](#4-kanyu-analysis)
   - 4A. [kanyu toolbox](#4a-kanyu-toolbox-python-工具箱arcgis-pyt-式样)
   - 4B. [kanyu crs](#4b-kanyu-crs-坐标参考系epsg-全库)
5. [kanyu render](#5-kanyu-render)
6. [kanyu gene](#6-kanyu-skill)
7. [kanyu introspect](#7-kanyu-introspect)
8. [kanyu agents](#8-kanyu-agents)
9. [kanyu mcp serve](#9-kanyu-mcp-serve)
10. [计划中的命令（📋）](#10-计划中的命令-)

## 1. 安装

```bash
# 需要 Rust 1.94+（rust-toolchain.toml 锁定）
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

检视数据文件：格式探测、要素数、字段清单。**kdb v2 多图层容器**自动展开
图层清单（`format_version: "2"` + 每层要素数/几何类型/范围/字段）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径（位置参数） |

```bash
$ ./target/debug/kanyu.exe data info examples/buildings.geojson
图层:      buildings
格式:      geojson
要素数:    4
几何类型:  LineString, Point
范围:      [116.390000, 39.900000] → [116.410000, 39.920000]
字段:      grade, height, name, usage, width
```

```bash
$ ./target/debug/kanyu.exe data info examples/buildings.geojson --json
{
  "id": "buildings",
  "format": "geojson",
  "feature_count": 4,
  "geometry_types": ["LineString", "Point"],
  "fields": ["grade", "height", "name", "usage", "width"],
  "extent": [116.39, 39.9, 116.41, 39.92]
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

### 3.4 `kanyu data export <file> -f <format> --out <path>` ✅（原生支持 geojson / csv / shp / fgb / geoparquet / dxf / kml / kmz / kdb / dat）

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
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f kml --out deliver.kml
已导出 4 个要素 → deliver.kml (kml)              # Placemark + ExtendedData，含洞 Polygon 保留
$ echo $?
0
```

能力矩阵允许但驱动未启用时，返回结构化错误（退出码 1）：

```bash
$ ./target/debug/kanyu.exe data export examples/buildings.geojson -f dwg --out out.dwg
Error: 格式 'dwg' 的原生导出尚未启用（driver: acadrust）。
桥接/插件驱动将在对应阶段就绪后开放，见 docs/MASTERPLAN.md 第五部分。
```

矩阵不允许时更早被拦截：`kanyu data export ... -f wfs ...` →
`Error: format 'wfs' does not support operation 'write'`。

> 注：DWG **读取**已原生支持（acadrust + 补丁层，六类几何；
> `data info/load/query` 可直接作用于 .dwg 文件）。

### 3.5 `kanyu data reproject <file> --from <crs> --to <crs>` ✅

投影变换（内置 EPSG 数据库；`EPSG:xxxx` / proj4 定义串 / `WGS84` 均接受；
经纬度自动衔接度/弧度，z 不变）。EPSG:4326 数据做米制 buffer 前先用它投影。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--from <crs>` | 源 CRS（如 `EPSG:4326`） |
| `--to <crs>` | 目标 CRS（如 `EPSG:3857`） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe data reproject examples/buildings.geojson --from EPSG:4326 --to EPSG:3857 --output bj3857.geojson
已写出 4 个要素 → bj3857.geojson        # 该提示在 stderr
```

### 3.6 `kanyu data calc <file> --target <field> --expr <expr>` ✅（字段计算器）

attrcalc 内核出口：逐要素求值表达式并写入目标字段（不存在则新建，存在则覆盖）。
支持 `+ - * / %`、比较、`and/or/not`、`round/upper/concat/coalesce` 等函数与
`$area/$length/$x/$y` 几何虚列（对齐壳层属性表字段计算器语义）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--target <field>` | 目标字段（不存在则新建，存在则覆盖） |
| `--expr <expr>` | 表达式（如 `"[height] * 2"` 或 `"$area / 10000"`） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe data calc examples/buildings.geojson --target h2 --expr "[height] * 2" --output calc.geojson
已写出 4 个要素 → calc.geojson          # 该提示在 stderr
```

### 3.7 `kanyu data kdb-pack <file...> --out <path.kdb>` ✅

多图层打包为堪舆数据库（KDB v2 zip 容器）：每输入文件成为一个命名图层
（图层名=文件名主干，重名中文报错），面向《不动产登记数据库标准》的
多表形态单文件建库（ZDJBXX/JZD/JZX… 一库全收）。类型保真（RecordBatch
直通，不经 GeoJSON 中间层）。读取侧：`data info` 自动展开 v2 图层清单；
`Layer::load`/导出/渲染等单图层入口取清单首图层；
`Layer::load_kdb_layers` 取全部图层。

```bash
$ ./target/debug/kanyu.exe data kdb-pack 宗地.txt 界址点.dat --out 不动产库.kdb
已打包 2 图层（宗地, 界址点）→ 不动产库.kdb (kdb v2 多图层容器)   # 该提示在 stderr
$ ./target/debug/kanyu.exe data info 不动产库.kdb
图层:      不动产库
格式:      kdb（v2 多图层容器，2 图层）
  ── 宗地：1 要素（Polygon），……
  ── 界址点：9 要素（Point），……
```

## 4. kanyu analysis ✅

空间分析工具组（geo crate；对应 [MASTERPLAN.md](MASTERPLAN.md) §4.2.2）。
**单位警示**：`--distance` 与面积均为数据 CRS 单位，EPSG:4326 下是度而非米；
米制分析请先用 [`data reproject`](#35-kanyu-data-reproject-file---from-crs---to-crs-)
投影到米制 CRS，或用 [`analysis measure`](#44-kanyu-analysis-measure-file---kind-lengtharea-)
做测地线度量。

### 4.1 `kanyu analysis buffer <file> --distance <f64> [--segments <n>]` ✅

缓冲区分析：逐要素缓冲为面几何，**属性随行**；不可转换要素跳过
（跳过数计入结果 `foreign_members.skipped`）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--distance <f64>` | 缓冲距离（数据 CRS 单位） |
| `--segments <n>` | 圆弧拟合的每象限分段数（默认 8，越大越圆滑） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe analysis buffer examples/buildings.geojson --distance 0.001 --segments 16 --output buf.geojson
已写出 4 个要素 → buf.geojson        # 该提示在 stderr
```

### 4.2 `kanyu analysis overlay <target> <overlay> --operation <op>` ✅

叠加分析（仅 Polygon/MultiPolygon 面要素）：target×overlay 逐要素对布尔
（未做跨对融合 dissolve，语义细节见 [API.md](API.md#4-analysis--空间分析内核)）。
结果属性 = target 属性 + overlay 属性（键冲突加 `overlay_` 前缀）。

| 参数 | 说明 |
|---|---|
| `<target>` | 目标图层文件 |
| `<overlay>` | 叠加图层文件 |
| `--operation <op>` | `union` / `intersection` / `difference` / `xor` |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

### 4.3 `kanyu analysis topology <file> --rules <rules>` ✅

拓扑检查（面要素两两交集面积 > 1e-10 判违规）。人读输出违规摘要，
`--json` 输出 `TopologyReport`（字段见 [API.md](API.md#4-analysis--空间分析内核)）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--rules <rules>` | 规则（逗号分隔，当前支持 `no_overlap`） |

```bash
$ ./target/debug/kanyu.exe analysis topology overlap.geojson --rules no_overlap
拓扑检查发现 1 条违规（规则 no_overlap，2 个要素）:
  要素 0 × 要素 1：面要素重叠，交集面积 4.000000
```

### 4.4 `kanyu analysis measure <file> --kind <length|area>` ✅

测地线度量（Karney 2013，WGS84 椭球；输入应为经纬度数据如 EPSG:4326，
投影数据请先 `data reproject` 回地理 CRS）。人读输出总计与单位，
`--json` 输出逐要素明细（结构见 [API.md](API.md#5-crs--坐标参考系工具)）。

| 参数 | 说明 |
|---|---|
| `<file>` | 数据文件路径 |
| `--kind <kind>` | `length`（线长与面外环周长，米）/ `area`（面面积，平方米） |

```bash
$ ./target/debug/kanyu.exe analysis measure examples/buildings.geojson --kind length
测地线长度总计: 2802.824 m（4 个要素；--json 见逐要素明细）
```

### 4.5 `kanyu analysis sjoin <target> <join> --predicate <pred>` ✅

空间连接（**左连接 + 匹配展开**：保留全部 target 要素、无匹配 join 侧缺省、
一对多匹配各输出一条）。结果属性 = target 属性 + join 属性（键冲突加
`join_` 前缀）+ `join_index`（join 要素序号）。

| 参数 | 说明 |
|---|---|
| `<target>` | 目标图层文件 |
| `<join>` | 连接图层文件 |
| `--predicate <pred>` | `intersects` / `contains` / `within`（contains=target 包含 join，within=join 包含 target） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe analysis sjoin pts.geojson zones.geojson --predicate within --output joined.geojson
已写出 4 个要素 → joined.geojson        # 该提示在 stderr
```

### 4.6 `kanyu analysis zonal <zones> <values> --field <name> --stats <list>` ✅

分区统计：values 按质心/代表点归属 zones 面要素（一值多区取首个匹配），
zones 追加 `{field}_{stat}` 统计列；区外值计入结果
`foreign_members.unzoned_count`。

| 参数 | 说明 |
|---|---|
| `<zones>` | 分区图层文件（仅面要素） |
| `<values>` | 数值图层文件 |
| `--field <name>` | 数值字段名（缺失或非数值时报中文错误） |
| `--stats <list>` | 统计项（逗号分隔：`count,sum,mean,min,max`） |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe analysis zonal zones.geojson pts.geojson --field height --stats count,sum,mean --output zoned.geojson
已写出 2 个要素 → zoned.geojson        # 该提示在 stderr
```

### 4.7 QGIS 核心算法（v0.16 移植，语义对齐 QGIS Processing）✅

| 命令 | 说明 | 关键参数 |
|---|---|---|
| `analysis dissolve <file> [--field <f>]` | 融合：按字段分组并集（属性=组字段值+组内首要素） | `--output` |
| `analysis simplify <file> --tolerance <f64>` | 道格拉斯简化（退化要素剔除） | `--output` |
| `analysis centroid <file>` | 质心（逐要素 Point，属性随行） | `--output` |
| `analysis convexhull <file>` | 凸包（逐要素 Polygon） | `--output` |
| `analysis deleteholes <file> [--min-area <f64>]` | 删洞（无阈值全删；有阈值保留 ≥阈值的洞） | `--output` |
| `analysis explode <file>` | 多部件炸开（Multi*→单部件，属性复制） | `--output` |
| `analysis stats <file>` | 图层统计（测地线口径；面积含亩/公顷/km²） | 人读/`--json` |

```bash
$ ./target/debug/kanyu.exe analysis stats examples/buildings.geojson
图层统计（buildings）:
  要素: 4（点 3 / 线 1 / 面 0 / 其他 0）
  总长度: 2.803 km
  总面积: 0.00 ㎡（0.0000 公顷 / 0.0000 亩 / 0.000000 km²）
```

### 4.8 `kanyu analysis bench [--size N]` ✅（性能基准）

对加载解析 / buffer / overlay(union) / sjoin / render_png 五项场景计时
（每项 3 次取中位数；场景由内核 `bench` 生成器确定性产出（种子 42），
写出 target/bench/ 大文件不入仓库）。overlay 单侧规模为 √N（O(n·m) 朴素
实现的平方项控制），sjoin 连接侧固定 16 格网面。`--json` 输出结构化结果
（要素数/三次耗时/中位毫秒/吞吐要素每秒）。

```bash
$ ./target/release/kanyu.exe analysis bench --size 10000
性能基准（规模 10000 要素，overlay 单侧 100 格，每项 3 次取中位数）:
项目                     要素数          中位耗时          吞吐(要素/秒)
加载解析                 10000        32.0ms            312071
...
```

### 4.9 `kanyu data validate <file>` ✅（宗地 TXT 质检）

表头必备项缺失/空值（错误/警告分级）、中文逗号、空格、闭合环与点数规则
（移植自堪舆工具箱质检）。警告不阻塞退出码；`--json` 输出问题清单。

## 4A. kanyu toolbox ✅（Python 工具箱，ArcGIS .pyt 式样）

```bash
kanyu toolbox list <file.py>                 # 列出工具（名称/参数/说明）
kanyu toolbox run <file.py> <tool> [--param k=v]...   # 执行工具
```

- 工具箱约定：`Toolbox` 子类 + 内嵌 `Tool` 子类（`name/label/params/execute`），
  见 `python/kanyu/toolbox.py` 与示例 `examples/planning_tools.py`；
- 工具内 `import kanyu` 直接调用 Rust 内核（Python SDK 见 [SDK.md](SDK.md) §4）；
- Python 包路径解析：`KANYU_PYTHON` 环境变量 > exe 同级 `python/` > 当前目录 `python/`；
- `--param k=v` 数值/布尔自动类型化；结果 JSON 打印（`--json` 原样透传）。

## 4B. kanyu crs ✅（坐标参考系，EPSG 全库）

直连内核 `kanyu_core::crs`（crs-definitions 内置 EPSG 数据库 7507 条，代码域
2000..=32766；单一事实来源，壳层设置对话框与 DSH 组件 `crs.search` 共用）。

```bash
kanyu crs search [query] [--limit N]     # EPSG 全库检索：代码子串或名称（大小写不敏感）
kanyu crs info <code>                    # 条目详情：名称/类型/单位/proj4 定义串
```

- `search` 空查询返回常用精选（EPSG:4326/3857/4490/4526/4527/4610/4214/32650/4547，
  库中缺失自动跳过）；`--json` 输出 `[{code,name,kind,unit}]`（kind 为
  Geographic/Projected/Other 英文枚举，人机输出为中文「地理/投影坐标系」）。
- `info` 库中不存在的代码中文报错；`--json` 输出单条 `{code,name,kind,unit}`
  （proj4 定义串仅人机输出，原始定义消费方走 `kanyu_core::crs::crs_proj4_def`
  或 MCP `kanyu://crs/{code}` 资源）。

```bash
$ kanyu crs search CGCS2000 --limit 2
EPSG:4490   China Geodetic Coordinate System 2000（地理坐标系，度）
EPSG:4491   CGCS2000 / Gauss-Kruger zone 13（投影坐标系，米）
$ kanyu crs info 4547
EPSG:4547  CGCS2000 / 3-degree Gauss-Kruger CM 114E
类型:    投影坐标系
单位:    米
proj4:   +proj=tmerc +lat_0=0 +lon_0=114 +k=1 +x_0=500000 +y_0=0 +ellps=GRS80 +units=m +no_defs +type=crs
```

## 4C. kanyu tool ✅（工具箱注册表，QGIS Processing 式）

直连内核 `core::tooldef` 注册表（37 工具，与壳层工具箱面板、MCP 工具面、
kanyu-py SDK 同一单一事实来源）+ `toolrun::run_tool` 统一执行入口。

```bash
kanyu tool list                          # 列出注册表（id/中文名/分类/说明）
kanyu tool list --json                   # 全量定义含参数表（AI/组件发现面）
kanyu tool run <id> [--param k=v]... [--output <路径>]
```

- `run` 的 Layer 类参数值给**数据文件路径**（执行前预加载，同名路径只载
  一次）；多图层参数（MultiLayers）逗号或换行分隔多个路径；枚举参数内核值
  与中文标签均可（如 `--param predicate=相交` 等价 `intersect s`）；缺省
  参数取注册表默认值。
- 产出结算：报告类工具（`report: true`，如 `stats`/`topology_check`）打印
  终端报告（`--json` 包装为 `{"tool":..,"report":..}`）；单图层产出走
  `--output`（GeoJSON，缺省打印 stdout）；多产出工具（如 `split_by_field`）
  `--output` 视作**输出目录**，逐组一个 `<目录>/<组名>.geojson`；工具声明
  OutFile 参数时由内核按扩展名格式直接写盘。

```bash
$ kanyu tool run buffer --param layer=examples/buildings.geojson \
    --param "distance=0.001|度" --output buf.geojson
已写出 4 个要素 → buf.geojson
$ kanyu tool run stats --param layer=examples/buildings.geojson
图层统计 examples/buildings.geojson:
{ "feature_count": 4, "points": 3, ... }
```

## 5. kanyu render ✅

离屏地图渲染（kanyu-render crate；色彩系统见 [MASTERPLAN.md](MASTERPLAN.md) §1.2）。

### 5.1 `kanyu render map <file> --out <path>` ✅

渲染数据文件为地图图片，输出格式按 `--out` 扩展名判定（`png`/`svg`，
其他扩展名中文报错）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--out <path>` | （必填） | 输出路径（`.png` 或 `.svg`） |
| `--width <n>` | `800` | 图片宽度（像素） |
| `--height <n>` | `600` | 图片高度（像素） |
| `--theme <light\|dark>` | `light` | 主题：`light` 晨山 / `dark` 夜观星 |
| `--background <#RRGGBB\|none>` | （无） | 背景色覆盖；`none`/`transparent` = 透明背景（不铺画布色，供底图叠加场景） |
| `--style <json>` | （无） | 属性驱动样式规则（内联 JSON；与 `--style-file` 二选一，同给中文报错） |
| `--style-file <path>` | （无） | 样式规则 JSON 文件路径 |

样式 JSON 两种形态（语义见 [API.md](API.md#9-kanyu-render--离屏地图渲染)）：
`{"type":"graduated","field":..,"stops":[[阈值,"#RRGGBB"],…]}` 数值分档、
`{"type":"categorical","field":..,"colors":{..},"default":..}` 类别映射。

```bash
$ ./target/debug/kanyu.exe render map examples/buildings.geojson --out map.png
已渲染 4 个要素 → map.png (png, 800x600, light)        # 该提示在 stderr
$ ./target/debug/kanyu.exe render map examples/buildings.geojson --out styled.png \
    --style '{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}'
已渲染 4 个要素 → styled.png (png, 800x600, light)
# 实测：height=33 的住宅为青绿（低档）、88.5 的大厦A 为琥珀（中档）、120 的大厦C 为赭红（高档）
```

### 5.2 `kanyu render layout <file> --out <path>` ✅

布局排版（kanyu-render `layout` 模块，壳层 layoutview 同源排版器）：A4 页面
+ 标题/图例/比例尺/指北针，内嵌地图渲染。输出格式按 `--out` 扩展名判定
（`svg`/`png`）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--out <path>` | （必填） | 输出路径（`.svg` 或 `.png`） |
| `--title <text>` | `布局` | 图名（页眉标题） |
| `--page <a4l\|a4p>` | `a4l` | 页面：A4 横向 / A4 纵向 |
| `--dpi <n>` | `96` | PNG 分辨率（SVG 忽略） |
| `--no-legend` | （关） | 不画图例 |
| `--no-scalebar` | （关） | 不画比例尺 |
| `--no-north` | （关） | 不画指北针 |
| `--theme <light\|dark>` | `light` | 内嵌地图主题 |
| `--style <json>` / `--style-file <path>` | （无） | 同 `render map`（有样式才出图例） |

比例尺按数据 extent 跨度 ×111320 m/°（赤道近似）取整（`nice_scale`）。

```bash
$ ./target/debug/kanyu.exe render layout examples/buildings.geojson --out layout.svg --title 示例布局
已排版 4 个要素 → layout.svg (layout svg, 1123x794px, 96dpi)   # 该提示在 stderr
```

### 5.3 `kanyu render parcel-map <file> --out <path>` ✅

宗地图出图（GB/T 42547-2023《地籍调查规程》图 L.3 版式；kanyu-render
`parcelmap` 模块 + kanyu-core `cartography` 勘测定界图注记契约排版引擎）：
A4 竖页面 + 标题「宗 地 图」+ 头部信息框（宗地代码/所在图幅号/宗地面积/
土地权利人）+ 地图框（界址点 Ø2.0mm 符号、0.3mm 红界址线、J 点号与边长
注记、宗地号/地类编码分式）+ 界址点坐标表（点号|X|Y|边长，X=纵坐标（北）、
Y=横坐标（东）测绘惯例；**长表自动折列**——超出图框内可用高时 right_to_left
列流分栏，题行仅首列、表头每列重复、面积行仅末列）+ 整百比例尺 +
「北」指北针 + 签注栏。SVG/PNG 双通道边长注记均沿线旋转（PNG 离屏
pixmap 旋转合成，与 SVG `rotate()` 同角）。
输入为面要素（GeoJSON/SHP/宗地 TXT/DXF 等注册格式；多面要素缺省取面积
最大者）。输出格式按 `--out` 扩展名判定（`svg`/`png`）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--out <path>` | （必填） | 输出路径（`.svg` 或 `.png`） |
| `--parcel-code <text>` | 属性 `parcel_id/ZDDM/zddm` | 宗地代码（分式分子取末 7 位） |
| `--owner <text>` | 属性 `owner/QLRMC/parcel_name` | 土地权利人 |
| `--map-sheet <text>` | 属性 `map_sheet/TFH` | 所在图幅号 |
| `--area <㎡>` | 属性 `area/ZDMJ`，再无现算 | 宗地面积 |
| `--land-use <text>` | 属性 `parcel_use/YT` | 地类编码（分式分母） |
| `--unit-name <text>` | （空） | 左侧竖排单位名 |
| `--survey-note <text>` | （空） | 左下测绘说明（如「2026年08月解析法测绘界址点」） |
| `--drawer / --reviewer <text>` | （空） | 制图者 / 审核者 |
| `--draw-date / --review-date <text>` | （空） | 制图 / 审核日期 |
| `--sizhi-e / --sizhi-s / --sizhi-w / --sizhi-n <text>` | 属性 `ZDSZD/ZDSZN/ZDSZX/ZDSZB` | 四至/邻宗地注记（`\n` 分行；主方位最长边外侧，碰撞沿边切向滑移+法向抬升避让） |
| `--roads <file>` | （无） | 相邻道路线文件（线要素；路名取属性 `name/NAME/road_name/道路名称/DLMC`；按地图框裁剪，路名沿最长可见段、角度沿线，可见段过短仅绘线） |
| `--scale <n>` | 自动取整百 | 比例尺分母 |
| `--dpi <n>` | `150` | PNG 分辨率（SVG 忽略） |
| `--index <n>` | 面积最大面要素 | 面要素文档序序号（0 起） |

```bash
$ ./target/debug/kanyu.exe render parcel-map 宗地.dxf --out 宗地图.png \
    --parcel-code 371602113005GB00032 --land-use 0801 --unit-name 滨州市自然资源局
已出宗地图 → 宗地图.png（1:700，注记 18 条，残余压盖 0 条）   # 该提示在 stderr
# 实测：真实 CASS DXF（滨州 GB00032 宗地，9 界址点）渲染 18 注记零压盖
```

### 5.4 `kanyu render parcel-dxf <file> --out <path>` ✅

宗地成果 CASS 兼容 DXF 导出（南方 CASS 联动；kanyu-core `cass` 模块）：
AC1024 DXF——`ZD` 宗地面 / `JZX` 界址线（逐边 + 边长注记，编码 302002）/
`JZD` 界址点（Ø2.0mm CIRCLE + 点号注记，编码 302001）/ `ZJ` 分式与权利人
注记；编码挂 SOUTH XDATA，CASS 直接打开编辑，可再被堪舆回读（闭环）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--out <path>` | （必填） | 输出路径（`.dxf`） |
| `--parcel-code <text>` | 属性 `parcel_id/ZDDM/zddm` | 宗地代码（分式分子取末 7 位） |
| `--land-use <text>` | 属性 `parcel_use/YT` | 地类编码（分式分母） |
| `--owner <text>` | 属性 `owner/QLRMC/parcel_name` | 土地权利人（ZJ 注记） |
| `--scale <n>` | `1000` | 出图比例尺分母（毫米要素换算模型单位） |
| `--no-xdata` | （关） | 不挂 SOUTH 编码 XDATA |
| `--index <n>` | 面积最大面要素 | 面要素文档序序号（0 起） |

```bash
$ ./target/debug/kanyu.exe render parcel-dxf 宗地.dxf --out 宗地_cass.dxf --scale 700 \
    --parcel-code 371602113005GB00032 --land-use 0801
已导出 CASS 兼容 DXF → 宗地_cass.dxf（界址点 9、界址线 9，SOUTH XDATA 开）   # 该提示在 stderr
```

CASS 坐标数据文件（.dat，注册表第 20 格式）走通用导出：
`kanyu data export 点层.geojson -f dat --out 点.dat`（CASS 标准轴序
`点号,编码,Y东,X北[,H]`；.dat 亦可被 `kanyu data info/load` 直接读取）。

### 5.5 `kanyu render sea-boundary-map <file> --out <path>` ✅

宗海界址图出图（GB/T 42547-2023 图 L.7 版式；kanyu-render `seamap`
模块 + `cartography` 排版引擎）：A4 **横向**页面——标题「{项目}宗海界址图」
+ 宗海代码行 + **经纬网图廓**（自适应间隔，度分秒注记顶/底横排、左右竖排）
+ 地图区（宗海图斑 RGB(245,162,122)、0.5mm 红界址线、点号 1,2,3…
（无 J 前缀）、边长注记）+ 右侧**界址点编号及坐标表**（点号|纬度（北纬）|
经度（东经），**度分秒 3 位小数**，末行重复起点闭合，逐点经 `--source-epsg`
反算 EPSG:4490）+ 右下网格签注表（坐标系/高程基准/测绘单位/测量员/绘图员/
绘制日期/检查人/审核人）+ 左下整百比例尺 + 右上指北针。
输出格式按 `--out` 扩展名判定（`svg`/`png`）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--out <path>` | （必填） | 输出路径（`.svg` 或 `.png`） |
| `--project-name <text>` | 属性 `project_name/XMMC` | 项目名称（标题前缀） |
| `--sea-code <text>` | 属性 `sea_code/ZHDM` | 宗海代码（左上「登记时填写或粘贴」） |
| `--source-epsg <code>` | `EPSG:4527` | 源坐标系（坐标表 DMS 反算基准；裸数字自动规范化） |
| `--survey-unit / --surveyor / --drawer` | （空） | 测绘单位 / 测量员 / 绘图员（签注表） |
| `--draw-date <text>` | （空） | 绘制日期 |
| `--inspector / --reviewer <text>` | （空） | 检查人 / 审核人 |
| `--scale <n>` | 自动取整百 | 比例尺分母 |
| `--dpi <n>` | `150` | PNG 分辨率（SVG 忽略） |
| `--index <n>` | 面积最大面要素 | 面要素文档序序号（0 起） |

```bash
$ ./target/debug/kanyu.exe render sea-boundary-map 宗海.dxf --out 宗海界址图.png \
    --project-name 代理围填海项目 --sea-code 371602113005JB00088 --source-epsg 4527
已出宗海界址图 → 宗海界址图.png（1:800，注记 18 条，残余压盖 0 条）   # 该提示在 stderr
# 实测：真实宗地代理（GB00032，EPSG:4527）DMS 坐标表与金样逐行一致
```

## 6. kanyu gene ✅

WASM 技能系统宿主（kanyu-skill crate；ABI 与沙箱模型见
[API.md](API.md#10-kanyu-skill--wasm-技能系统宿主)；MCP 热加载接线 📋）。

### 6.1 `kanyu gene info <plugin.wasm>` ✅

检视技能元数据（加载校验通过即打印）。

```bash
$ ./target/debug/kanyu.exe gene info crates/kanyu-skill/testdata/attr_scaler.wasm
技能:    attr_scaler
版本:    0.1.0
能力:    analyzer
```

`--json` 输出 SkillMeta（`{"name","version","capabilities":[...]}`）。

### 6.2 `kanyu gene run <plugin.wasm> <file>` ✅

在数据上执行分析技能（FeatureCollection 进/出，fuel 配额 10 亿）。

| 参数 | 说明 |
|---|---|
| `<plugin>` | 技能文件路径（.wasm 组件） |
| `<file>` | 数据文件路径 |
| `--output <path>` | 结果输出路径（GeoJSON）；缺省打印到 stdout |

```bash
$ ./target/debug/kanyu.exe gene run crates/kanyu-skill/testdata/attr_scaler.wasm examples/buildings.geojson
{"type":"FeatureCollection","features":[
  {"type":"Feature","geometry":{"type":"Point","coordinates":[116.3914,39.9072]},
   "properties":{"height":177.0,"name":"示例大厦A","usage":"office"}}, …]}
# attr_scaler 把 height 精确 ×2（88.5→177.0、33→66.0），几何与名称不变
```

## 7. kanyu introspect ✅

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
  kanyu-skill     [planned] 技能：WASM 插件系统（wasmtime 沙箱 + 热加载）
  kanyu-shell    [planned] 壳层：桌面 UI（TitleBar/StatusBar/面板系统）

MCP 工具:
  kanyu_data_load              data      [stable]
  kanyu_data_query             data      [stable]
  kanyu_data_export            data      [stable]
  kanyu_agents_init            agents    [stable]
  kanyu_analysis_buffer        analysis  [stable]
  …
格式矩阵: 17 种格式（详见 --json 或 docs/）
```

`--json` 输出完整报告（含 17 种格式的五级能力矩阵与全部工具清单）。
工具清单中的名称即真实 MCP 工具名（下划线式，映射规则见
[MCP.md](MCP.md#4-命名规范)）。

## 8. kanyu agents

`AGENTS.md` 项目语义文件：生成与校验（语义层设计见
[ARCHITECTURE.md](ARCHITECTURE.md#5-agentsmd-语义层设计)）。

### 8.1 `kanyu agents init` ✅

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

### 8.2 `kanyu agents validate` ✅

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

## 9. kanyu mcp serve ✅

启动 MCP Server，供 AI 代理接入（协议与工具详见 [MCP.md](MCP.md)）。

| 参数 | 默认 | 说明 |
|---|---|---|
| `--transport <stdio\|http>` | `stdio` | 传输方式：`stdio` 本地 AI 助手；`http` streamable HTTP（官方已取代旧 SSE；`sse` 值自 v0.6 起不再接受） |
| `--port <port>` | `3000` | HTTP 模式监听端口（绑定 127.0.0.1，endpoint `/mcp`） |

```bash
$ ./target/debug/kanyu.exe mcp serve
kanyu-mcp: MCP server 监听 stdio（initialize / tools/list / tools/call）   # stderr，随后阻塞服务
```

streamable HTTP（远程 AI 代理接入）：

```bash
$ ./target/debug/kanyu.exe mcp serve --transport http --port 3000
kanyu-mcp streamable HTTP 监听 http://127.0.0.1:3000/mcp （⚠️ 无鉴权/TLS，远程暴露请自行加反代；Ctrl-C 停止）
```

⚠️ HTTP 模式无鉴权/TLS（📋）；暴露到局域网/公网前必须自行加反向代理与鉴权。
长任务（SEP-2663 协议级 tasks ✅）：白名单分析工具的 `tools/call` arguments
带 `"task": true` 异步执行，`tasks/get|cancel|update` 驱动生命周期
（语义见 [MCP.md](MCP.md#21-长任务sep-2663-)）。

## 10. 计划中的命令（📋）

对应 [MASTERPLAN.md](MASTERPLAN.md) §4.3.1，随各 Phase 落地：

| 命令 | 状态 | 说明 |
|---|---|---|
| `kanyu codegen --prompt ... --target rust` | 📋 | AI 代码生成（Phase 4，须人类审核） |
| `kanyu plugin build/load <wasm>` | 📋 | WASM 技能构建与热加载（wasmtime 沙箱） |
| `kanyu benchmark --plugin ... --metric fps` | 📋 | A/B 基准对比 |
| `kanyu regression test --suite ...` | 📋 | 回归测试套件，性能阈值断言 |

当前命令全集即本文 §3–§7；以 `kanyu --help` 与 `kanyu introspect` 输出为准。
