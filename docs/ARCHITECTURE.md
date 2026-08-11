# 堪舆架构（ARCHITECTURE）

> 版本：v0.1.0 ｜ 内核代号：kanyu-spirit ｜ 宣言：以天地为盘，以数据为爻，以 AI 为神。
>
> 本文描述堪舆的整体架构与设计裁决。接口细节见 [API.md](API.md)、[SDK.md](SDK.md)、
> [MCP.md](MCP.md)、[CLI.md](CLI.md)；原始愿景见 [MASTERPLAN.md](MASTERPLAN.md)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [三层元架构](#1-三层元架构)
2. [crate 划分与依赖方向](#2-crate-划分与依赖方向)
3. [数据流](#3-数据流)
4. [格式注册表设计](#4-格式注册表设计)
5. [AGENTS.md 语义层设计](#5-agentsmd-语义层设计)
6. [安全模型](#6-安全模型)
7. [技术选型裁决](#7-技术选型裁决)
8. [性能目标](#8-性能目标)
9. [演进路线](#9-演进路线)

## 1. 三层元架构

堪舆的 AI 不是外挂插件，而是系统的**元级意识层**，代号"堪舆灵"（受 GOLEM 元架构启发，
见 [MASTERPLAN.md](MASTERPLAN.md) §4.1）。系统分为三层，层间通信协议唯一：

```
┌─────────────────────────────────────────────────────────┐
│  Meta-Level    堪舆灵 (Kanyu Spirit)                     │
│  ├─ 意图理解 / 空间推理 / 代码生成 / 性能诊断             │
│  └─ 伦理约束：不能修改自己的目标函数（见 §6）             │
├─────────────────────────────────────────────────────────┤
│  Interface     kanyu-mcp —— MCP 神经接口（rmcp 3.x）     │
│  └─ AI 与内核之间的唯一通信协议，只暴露声明式工具         │
├─────────────────────────────────────────────────────────┤
│  Object-Level  堪舆内核                                  │
│  ├─ kanyu-core    数据心脏（格式矩阵/图层/AGENTS.md） ✅  │
│  ├─ kanyu-cli     脊髓（kanyu 命令行）              ✅  │
│  ├─ kanyu-render  眼睛（离屏渲染 tiny-skia+SVG） 🚧  │
│  ├─ kanyu-edit    手（Undo/Redo + 基础编辑命令）   🚧  │
│  ├─ kanyu-skill   技能（wasmtime 插件宿主）       🚧  │
│  ├─ kanyu-py      Python 桥接（PyO3 + 工具箱）    ✅  │
│  └─ kanyu-shell   壳层（egui 桌面 UI）            🚧  │
└─────────────────────────────────────────────────────────┘
```

关键约束：**Object-Level 不感知 Meta-Level 的存在**。内核只提供确定性能力；
智能完全由 MCP 客户端（Claude、Codex、Kimi 等）携带。这意味着换掉 LLM 不需要改内核。

## 2. crate 划分与依赖方向

| crate | 角色 | 状态 | 依赖的兄弟 crate |
|---|---|---|---|
| `kanyu-core` | 数据心脏：格式注册表（19 格式含自研 .kdb/.txt）、图层模型、空间分析（buffer/overlay/topology/sjoin/zonal）、QGIS 核心算法（geoprocess 一批：dissolve/simplify/centroid/convex_hull/delete_holes/explode/stats；二批：boundary/bounding_boxes/merge/extract_by_attribute/extract_by_location/count_points_in_polygon/field_stats/mean_coordinates；三批：distance_matrix/nearest_neighbor/multi_ring_buffer/variable_buffer/split_by_field/add_geometry_attributes/create_grid/points_along_lines/concave_hull/minimum_rotated_rect）、投影/度量、宗地 TXT（parcel）、字段计算器表达式引擎（attrcalc）、工具箱声明与统一执行（tooldef/toolrun，壳层/CLI/Python 三面投影）、CRS 全库检索（7507 条）、AGENTS.md 语义、系统自省 | ✅ stable | 无 |
| `kanyu-py` | Python 桥接：PyO3 扩展模块 `kanyu`（GeoJSON 文本契约全量暴露内核）+ .pyt 式工具箱运行时 | ✅ stable | kanyu-core, kanyu-render |
| `kanyu-cli` | 脊髓：`kanyu` 命令行（clap derive） | ✅ stable | kanyu-core, kanyu-mcp, kanyu-render |
| `kanyu-mcp` | 神经接口：MCP Server（rmcp，stdio + streamable HTTP，SEP-2663 长任务） | ✅ incubating | kanyu-core, kanyu-render |
| `kanyu-render` | 眼睛：离屏地图渲染（SVG 零依赖 + tiny-skia PNG 光栅化，晨山/夜观星主题，属性驱动符号化 graduated/categorical）；wgpu 实时管线待壳层 | 🚧 incubating | kanyu-core |
| `kanyu-edit` | 手：Undo/Redo 框架（命令逆操作双栈 + 容量淘汰）+ 基础编辑命令（顶点移动/要素平移/删除/插入/属性更新，GeomPath 三级定位）；DCEL 增量拓扑待后续 | 🚧 incubating | kanyu-core |
| `kanyu-skill` | 技能：WASM 插件宿主（wasmtime 沙箱 + WIT 组件模型 ABI + fuel 配额）；MCP 热加载接线 📋 | 🚧 incubating | kanyu-core |
| `kanyu-shell` | 壳层：egui 深度桌面 UI（v0.8，ArcGIS Pro SDK 范式）——`commands.rs` 声明式命令注册表（DAML 思路：Ribbon/QAT/右键菜单统一投影，条件置灰）、Contents 图层面板（`toc.rs` 复选框/嵌套分组/符号化分类展开行/属性页四页签）、`dock.rs` 停靠系统（六面板三区拖拽停靠/浮动/关闭，跨区回流自适配+滚动契约）、中央视图停靠区（地图框页签吸附/浮动互转，画布纯白）、`symbology.rs` 符号化（单色/唯一值/分级设色，按层渲染叠图，入 .kyu）、`catalog.rs` 工程目录五分类（地图框/布局框/数据库/服务链接/本机数据）、`toolbox/` 工具箱（参数类型系统对齐 ArcGIS Python 工具箱规范：多值图层/线性单位/坐标系/校验分级/统一骨架/后台线程+进度模态可取消/终端三级日志）、`attrtable.rs` 属性表+字段计算器、`mapview.rs` 多地图视图 + `scene3d.rs` 实验性 3D 棱柱场景、`settings.rs`（坐标系全库搜索/渲染/界面缩放）、palette 语义色全套 + tokens::state 状态色派生 + WCAG 2.2 对比度强制、内置终端 + AI 对话 | 🚧 incubating | kanyu-core, kanyu-render, kanyu-skill |

依赖规则（编译期强制，review 时核对）：

- `kanyu-core` **不依赖任何兄弟 crate**，是依赖图的根。所有能力下沉到 core，
  cli/mcp 只是"薄壳"：解析参数 → 调 core → 格式化输出。
- `kanyu-render`/`kanyu-skill`/`kanyu-py` 只依赖 core（kanyu-py 另依赖 render 用于渲染函数）；
  cli/mcp 依赖 core+render+skill；shell 依赖 core+render+skill。
- 兄弟 crate 之间禁止横向依赖（如 mcp 不得依赖 cli）。
- 该清单的单一事实来源是 `introspect::modules()`（kanyu-core/src/introspect.rs），
  `kanyu introspect` 与 `kanyu_system_introspect` 工具的输出即由此生成。

## 3. 数据流

```
磁盘文件 ──→ 格式探测(FormatRegistry::detect) ──→ 解析器 ──→ 统一内存模型
                                                              │
                                  ┌───────────────────────────┼───────────────────┐
                                  ↓                           ↓                   ↓
                            属性查询 Layer::query        导出编码器           MCP 工具
                            ("field op value")      (GeoArrow → 目标格式)  (结构化 JSON)
```

| 阶段 | v0.1（✅ 现状） | v0.2（📋 目标） |
|---|---|---|
| 内存模型 | GeoArrow `RecordBatch`（WKB 几何列 + 类型化属性列，arrow 58 / geoarrow-schema 0.8）✅ | geoarrow-array 原生几何列（去 WKB 编解码），列式零拷贝 |
| 原生解析 | geojson | flatgeobuf 6.x、geoparquet 0.8、shapefile 0.9、geojson、kml 0.14、dxf 0.6（IxMilia） |
| 统一 I/O 抽象 | 逐格式 match | 候选 geozero 0.15 |
| 桥接格式 | 返回结构化错误 | GDAL/LibreDWG 以可选插件/WASM 沙箱接入 |

`Layer` 的公共 API（`load/summary/query/collection/batch`）面向该演进设计：
各格式解析器在边界统一转为 FeatureCollection 后一次性入列（`collection_to_batch`），
导出时按需转回（`batch_to_collection`），格式代码零感知；`batch()` 提供零拷贝访问。
后续迁移 geoarrow-array 原生几何列后，渲染器可直接映射 SSBO，分析引擎可走 Arrow
向量化，符合 [MASTERPLAN.md](MASTERPLAN.md) §3.1。

## 4. 格式注册表设计

`FormatRegistry`（crates/kanyu-core/src/format.rs）是格式知识的**唯一载体**，
对应总规附录 A 的格式支持矩阵。每种格式声明五项能力的支持级别：

| 能力 | 含义 |
|---|---|
| `read` / `write` / `edit` | 读取 / 写入 / 编辑 |
| `symbol` | 符号化保留（导出后样式不丢） |
| `layout` | 布局保留（打印布局 → Paper Space 等） |

支持级别为三态枚举 `Support`：`Full` / `Partial` / `None`（`usable()` = 非 None）。
每条记录还带 `driver`（`native` / `gdal-bridge` / `acadrust` / `libredwg-wasm` 等）与 `note` 备注。
v0.1 内置 17 种格式；v0.3 起第 18 种为自研 **堪舆数据库 `.kdb`**（KanyuDB，裁决 #19：
Arrow IPC + `kanyu.*` 元数据，RecordBatch 直通类型保真），与总规附录 A.1 一一对应
（有单元测试保证不漏）。

**能力矩阵如何驱动决策**：CLI 与 MCP 不写任何格式特判，一律走注册表：

```
kanyu data export buildings.geojson -f dwg --out out.dwg
  │
  ├─ FormatRegistry::require("dwg", "write")
  │    ├─ by_id("dwg") 未命中 → KanyuError::UnknownFormat
  │    └─ write 为 None      → KanyuError::UnsupportedOperation
  │         （例：-f wfs 在此被拒绝，WFS 只读）
  ├─ require 通过（dwg.write = Partial，可用）
  └─ driver = "acadrust" 导出侧未启用
       → 结构化错误："格式 'dwg' 的原生导出尚未启用（driver: acadrust）。
          桥接/插件驱动将在对应阶段就绪后开放"
```

即：**能力矩阵决定"理论上能不能"，driver 状态决定"现在能不能"**。
两种失败都返回结构化错误而非崩溃，AI 代理可据此换用 DXF 等替代格式
（DWG 写出策略见 §7 裁决表）。完整格式矩阵可用 `kanyu introspect --json` 或
`kanyu_system_introspect` 工具查看。

## 5. AGENTS.md 语义层设计

每个堪舆项目根目录的 `AGENTS.md` 是 AI 理解项目的"罗盘"（[MASTERPLAN.md](MASTERPLAN.md) §4.3.2）。
`kanyu-core::agents` 将其解析为机器可读对象，供 CLI/MCP 消费：

| 章节（`## ` 二级标题） | 解析结果 | 语法 |
|---|---|---|
| 项目元数据 | `ProjectMeta`（name/crs/extent/author/created） | `- **key**: value` |
| 数据层语义 | `Vec<LayerSemantics>`（图层/类型/语义/关键字段/业务规则） | 五列 Markdown 表格 |
| 业务规则 | `Vec<String>` | 编号列表 `1. ...` |
| 自定义工具 | `Vec<String>`（工具名） | `` - `name`: 说明 `` |

设计要点：

- **容错解析，严格校验分离**：`parse` 对缺失段落不报错；完整性由 `validate()`
  检查（缺 name、缺 crs、语义表为空、图层未声明关键字段均列为问题）。
  CRS 是堪舆项目的强制项。
- **模板可往返**：`agents::template()` 生成的模板能被 `parse` 无损解析（有测试保证），
  `kanyu agents init` 即用它生成项目罗盘。
- 生成与校验的入口见 [CLI.md](CLI.md#kanyu-agents) 与 [MCP.md](MCP.md)；Rust API 见 [API.md](API.md#agents)。

## 6. 安全模型

| 机制 | 状态 | 说明 |
|---|---|---|
| 无 `execute_code` | ✅ | 只暴露声明式工具；ServerInfo instructions 明确声明"不提供任意代码执行"。与 qgis_mcp 系 Python 薄壳的本质区别（对比见 [MCP.md](MCP.md#与现有-gis-mcp-项目的差异)） |
| 确定性输出 | ✅ | 工具返回结构化 JSON（`structuredContent`），携带 CRS/单位/要素数元数据，可审计、可回放 |
| 长任务隔离 | ✅ | SEP-2663 任务执行在 blocking 线程池（不阻塞调度线程）；任务注册表为内存态（rmcp TaskManager，TTL 10 分钟惰性清扫，**重启即丢**）；无任务落盘，无持久化副作用 |
| 内核零 C 依赖 | ✅ | 默认构建不链接任何 C/C++ 库，消除整条 FFI 攻击面 |
| WASM 沙箱 | ✅ | 技能在 wasmtime 组件模型沙箱中运行：WIT 强类型 ABI（无 WASI 导入 = 纯计算，无文件/网络/环境访问）+ fuel 配额（10 亿/次执行，耗尽即 trap）；**MCP 远程加载同沙箱**（hotload 校验失败绝不注册；注册表内存态重启即丢） |
| LibreDWG 隔离 | 📋 | LibreDWG（GPLv3+，2026 年披露多个 CVE）编译为 WASM，在 wasmtime 沙箱中**只读**运行，崩溃/越界不殃及内核 |
| 伦理约束 | 📋（随 Phase 4–5） | 堪舆灵**不能修改自己的目标函数**；不能绕过 MCP 直接操作文件系统；代码生成须人类审核方可合并内核（WASM 热加载除外）——[MASTERPLAN.md](MASTERPLAN.md) §4.4 |

## 7. 技术选型裁决

2026-08 调研结论。总规 [MASTERPLAN.md](MASTERPLAN.md) §5.1 为原始计划，本表为**现行方案**：

| 领域 | 原计划（总规 §5.1） | 现行方案 | 理由 |
|---|---|---|---|
| 内核 | geoarrow-rs + GDAL/OGR 桥 | **纯 Rust、零 C 依赖**；GDAL 降为可选插件 | 消除依赖地狱，任何平台 `cargo build` 即可用；GML 等长尾格式由 GDAL 插件覆盖 |
| 渲染引擎 | bgfx + vg-renderer | **wgpu** | 纯 Rust、WebGPU 标准、跨 Vulkan/Metal/D3D12/Web，与内核同语言零 FFI |
| UI 壳层 | Qt6 Widgets（C++20） | **egui / slint / Tauri**（Rust 方向） | 摆脱 C++ 工具链与许可证顾虑；与 wgpu 生态直接互通 |
| 空间计算 | GEOS（FFI） | **geo crate**（布尔/DE-9IM/buffer 内置）+ rstar 索引 + proj4rs 投影；GEOS 降为可选插件 | geo 为纯 Rust 且功能已覆盖核心谓词，避免 libgeos 链接 |
| 内存模型 | GeoArrow | 维持 **GeoArrow**（arrow 58 + geoarrow-schema 0.8，WKB 几何列 + 类型化属性列）✅ 已落地 | 见 §3；geoarrow-array 原生几何列待后续迭代 |
| DWG | LibreDWG 直链 + ODA 转换 | **acadrust 0.4 原生读取（纯 Rust、MPL-2.0）+ 自持补丁层**（AC15 locator workaround + GBK/MIF 编码层；七类几何 + 标注要素化 + ELLIPSE 近似，INSERT/HATCH/SPLINE 📋）；LibreDWG-wasm 降为备选；写仅 ≤r2004 可靠，现代 DWG 写出以 DXF 导出替代 | 裁决 #18 + 2026-08-03 spike（143 个真实 R2000 样本）：acadrust 0.4.1 开箱 0%（AC15 定位缺陷）但底层管线健康，修定位+编码后 52 万实体 100% 可读；纯 Rust 内存安全，GPL/CVE 沙箱理由消失；DXF 0.6（IxMilia）原生读写 |
| MCP | 手写 MCP Server | **官方 rmcp 3.x SDK** | 协议升级（tasks、streamable HTTP）由上游跟进；注意 MCP 工具名只允许 `[a-zA-Z0-9_-]`，总规的 `kanyu.data.load` 落地为 `kanyu_data_load` |
| 插件 | WASM（wasmtime） | 维持 **wasmtime + WIT 组件模型** | 沙箱安全、热重载、多语言技能 |

> 注：格式注册表中 DWG 一行的能力矩阵已按实现路线诚实标注（read: Full、
> write: Partial、driver: `acadrust`）；总规附录 A 描述的是目标能力，
> 两者差异以注册表（代码）为准。

## 8. 性能目标

沿用 [MASTERPLAN.md](MASTERPLAN.md) §5.3（📋 目标值，随 Phase 1–2 验证）：

| 指标 | QGIS 基准 | 堪舆目标 | 提升倍数 |
|---|---|---|---|
| 百万要素渲染 | 5–10 fps | 60 fps | 6–12x |
| 2GB 影像加载 | 30s | 3s | 10x |
| DWG (10MB) 导入 | 15s | 2s | 7.5x |
| 全国路网路径规划 | 分钟级 | 秒级 | 10x+ |
| 内存占用（同等数据） | 100% | 50% | 2x |
| 启动时间 | 8s | 1s | 8x |

### 8.1 首轮实测（2026-08-11）

基准设施：`kanyu-core::bench` 确定性场景生成器（xorshift64*，种子 42，
零依赖）+ `kanyu analysis bench [--size N]`（每项 3 次取中位数，
`--json` 结构化）。实测机：AMD Ryzen 9 9950X（16 核）/ 128GB RAM /
Windows 11 专业工作站版，release 构建（thin LTO）。

| 项目（场景） | 1 万 | 10 万 | 100 万 | 吞吐（100 万档） |
|---|---|---|---|---|
| 加载解析（GeoJSON→Layer） | 32.0 ms | 373.1 ms | 4508.1 ms | 22.2 万要素/秒 |
| buffer（0.01°，segments=8） | 81.6 ms | 816.0 ms | 9285.5 ms | 10.8 万要素/秒 |
| overlay_union（单侧 √N 格） | 26.7 ms（100²对） | 277.5 ms（316²对） | 3289.4 ms（1000²对） | 304 要素/秒（单侧） |
| sjoin（N × 16 格，intersects） | 14.1 ms | 155.9 ms | 1820.4 ms | 54.9 万要素/秒 |
| render_png（800×600 晨山） | 46.9 ms | 369.6 ms | 3785.4 ms | 26.4 万要素/秒 |

差距分析（一句话级）：

- **线性项健康**：加载/buffer/sjoin/render 吞吐跨档近似恒定（线性缩放）；
  buffer 为最重线性项（多边形偏移+布尔成本，每要素 ≈9µs）。
- **overlay 平方项坐实**：耗时随图层规模呈平方增长（×10 规模 → ×10.4/×11.9 耗时，
  即 O(n·m) 朴素笛卡尔积），rstar 空间索引裁剪为既定路线项。
- **渲染口径差异**：render_png 为 CPU 离屏**单帧**口径（100 万要素 ≈3.8s/帧，
  折合 0.26 fps），与 §8「60 fps」实时渲染目标差约 230 倍——该目标对应
  Phase 2 wgpu GPU 管线（GeoArrow→SSBO 直通），CPU 离屏渲染不承担该指标。
- **GeoJSON 文本解析为加载瓶颈**（100 万 ≈4.5s）；FGB/GeoParquet 二进制
  直通（Phase 1 进行中）预期量级改善，下轮实测补二进制格式对照行。

## 9. 演进路线

| 阶段 | 目标 | 状态 |
|---|---|---|
| Phase 1 地基 | 纯 Rust 内核：GeoArrow 内存模型、FGB/GeoParquet/Shapefile/DXF 原生 I/O | 🚧 进行中 |
| Phase 2 视界 | wgpu GPU 渲染管线，GeoArrow→SSBO 直通，MLT 瓦片 | 📋 规划 |
| Phase 3 手 | DCEL 增量拓扑编辑，Undo/Redo，属性表 | 📋 规划 |
| Phase 4 脑 | LLM 融合、MCP tasks 长任务、代码生成→WASM 沙箱流水线 | 📋 规划 |
| Phase 5 魂 | A/B 测试框架、技能市场、知识库 RAG、自我迭代闭环 | 📋 规划 |

当前 v0.1.0 已交付：kanyu-core 四大模块、kanyu CLI 全部子命令、
kanyu-mcp stdio Server（6 个工具）。细分状态以 `kanyu introspect` 输出为准。

### 9.1 近期路线推荐（2026-08-11，v0.20.0 刷新）

v0.18.0–v0.20.0 已推进：壳层 ArcGIS Pro SDK 范式重组（命令注册表/参数组件/统一
对话框骨架/后台执行+进度模态）、属性表与字段计算器、多地图视图（页签吸附/纯白
画布/实验性 3D）、图层符号化系统、目录五分类**全部兑现**（布局框=打印布局 v1、
服务链接=WFS GetFeature）、CRS 全库检索与轴序核验、tooldef/toolrun 下沉内核并
收敛 MCP 面（toolbox_list/toolbox_run）、UI 状态持久化、性能基准首轮实测（§8.1）、
kanyu-edit 首个增量 + 壳层编辑模式（顶点编辑/属性单元格编辑/Undo·Redo 接线）。
下一步推荐：

1. **编辑深化**：壳层编辑模式已闭环（顶点/移动/插入/删除/属性），下一步沿
   「线面要素添加 → GeoArrow Delta 快照 → DCEL 增量拓扑」推进。
2. **§8 性能优化（基准已立）**：overlay 平方项坐实（rstar 索引裁剪优先）、
   GeoJSON 文本加载为瓶颈（FGB/GeoParquet 二进制对照行待补）、60fps 指标归
   Phase 2 wgpu 管线。
3. **服务链接 v2**：WFS GetFeature 已通；GetCapabilities 解析（图层发现）与
   WMS 底图叠加为后续增量。
4. **布局 v2**：PNG 中文字体栈（当前 PNG 省略中文标题，SVG 完整）、布局入 .kyu
   持久化、多地图框混排。
5. **3D 场景真管线化**：scene3d 为 egui painter 实验实现；真 3D（地形/纹理/拾取）
   待 Phase 2 wgpu 管线接入。
