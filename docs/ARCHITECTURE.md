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
│  ├─ kanyu-render  眼睛（wgpu GPU 渲染）             📋  │
│  ├─ kanyu-edit    手（DCEL 拓扑编辑）               📋  │
│  ├─ kanyu-gene    基因（wasmtime 插件）             📋  │
│  └─ kanyu-shell   壳层（桌面 UI）                   📋  │
└─────────────────────────────────────────────────────────┘
```

关键约束：**Object-Level 不感知 Meta-Level 的存在**。内核只提供确定性能力；
智能完全由 MCP 客户端（Claude、Codex、Kimi 等）携带。这意味着换掉 LLM 不需要改内核。

## 2. crate 划分与依赖方向

| crate | 角色 | 状态 | 依赖的兄弟 crate |
|---|---|---|---|
| `kanyu-core` | 数据心脏：格式注册表、图层模型、空间分析（buffer/overlay/topology）、投影/度量（reproject/measure）、AGENTS.md 语义、系统自省 | ✅ stable | 无 |
| `kanyu-cli` | 脊髓：`kanyu` 命令行（clap derive） | ✅ stable | kanyu-core, kanyu-mcp |
| `kanyu-mcp` | 神经接口：MCP Server（rmcp，stdio） | ✅ incubating | kanyu-core |
| `kanyu-render` | 眼睛：wgpu 渲染管线，GeoArrow→SSBO 直通 | 📋 planned | kanyu-core |
| `kanyu-edit` | 手：DCEL 增量拓扑编辑，Undo/Redo | 📋 planned | kanyu-core |
| `kanyu-gene` | 基因：WASM 插件系统（wasmtime 沙箱 + 热加载） | 📋 planned | kanyu-core |
| `kanyu-shell` | 壳层：桌面 UI（egui/slint 方向） | 📋 planned | kanyu-core |

依赖规则（编译期强制，review 时核对）：

- `kanyu-core` **不依赖任何兄弟 crate**，是依赖图的根。所有能力下沉到 core，
  cli/mcp 只是"薄壳"：解析参数 → 调 core → 格式化输出。
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
每条记录还带 `driver`（`native` / `gdal-bridge` / `libredwg-wasm` 等）与 `note` 备注。
v0.1 内置 17 种格式，与总规附录 A.1 一一对应（有单元测试保证不漏）。

**能力矩阵如何驱动决策**：CLI 与 MCP 不写任何格式特判，一律走注册表：

```
kanyu data export buildings.geojson -f dwg --out out.dwg
  │
  ├─ FormatRegistry::require("dwg", "write")
  │    ├─ by_id("dwg") 未命中 → KanyuError::UnknownFormat
  │    └─ write 为 None      → KanyuError::UnsupportedOperation
  │         （例：-f wfs 在此被拒绝，WFS 只读）
  ├─ require 通过（dwg.write = Partial，可用）
  └─ driver = "libredwg-wasm" 非原生且未启用
       → 结构化错误："格式 'dwg' 的原生导出尚未启用（driver: libredwg-wasm）。
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
| 内核零 C 依赖 | ✅ | 默认构建不链接任何 C/C++ 库，消除整条 FFI 攻击面 |
| WASM 沙箱 | 📋 | 插件（"基因"）在 wasmtime + WIT 组件模型沙箱中运行，无宿主任意权限 |
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
| DWG | LibreDWG 直链 + ODA 转换 | **LibreDWG 编译为 WASM，wasmtime 沙箱只读运行**；写仅 ≤r2004 可靠，现代 DWG 写出以 DXF 导出替代；ODA SDK 为商业可选插件 | LibreDWG 为 GPLv3+ 且 2026 年披露多个 CVE，沙箱隔离崩溃与许可证风险；DXF 0.6（IxMilia）为原生读写 |
| MCP | 手写 MCP Server | **官方 rmcp 3.x SDK** | 协议升级（tasks、streamable HTTP）由上游跟进；注意 MCP 工具名只允许 `[a-zA-Z0-9_-]`，总规的 `kanyu.data.load` 落地为 `kanyu_data_load` |
| 插件 | WASM（wasmtime） | 维持 **wasmtime + WIT 组件模型** | 沙箱安全、热重载、多语言基因 |

> 注：格式注册表中 DWG 一行的能力矩阵已按实现路线诚实标注（read: Full、
> write: Partial、driver: `libredwg-wasm`）；总规附录 A 描述的是目标能力，
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

## 9. 演进路线

| 阶段 | 目标 | 状态 |
|---|---|---|
| Phase 1 地基 | 纯 Rust 内核：GeoArrow 内存模型、FGB/GeoParquet/Shapefile/DXF 原生 I/O | 🚧 进行中 |
| Phase 2 视界 | wgpu GPU 渲染管线，GeoArrow→SSBO 直通，MLT 瓦片 | 📋 规划 |
| Phase 3 手 | DCEL 增量拓扑编辑，Undo/Redo，属性表 | 📋 规划 |
| Phase 4 脑 | LLM 融合、MCP tasks 长任务、代码生成→WASM 沙箱流水线 | 📋 规划 |
| Phase 5 魂 | A/B 测试框架、基因市场、知识库 RAG、自我迭代闭环 | 📋 规划 |

当前 v0.1.0 已交付：kanyu-core 四大模块、kanyu CLI 全部子命令、
kanyu-mcp stdio Server（6 个工具）。细分状态以 `kanyu introspect` 输出为准。
