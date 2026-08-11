# 堪舆 SDK 使用指南（SDK）

> 版本：v0.1.0 ｜ 三种集成方式：Rust 库、MCP 客户端、CLI 脚本。
> API 细节见 [API.md](API.md)；MCP 协议细节见 [MCP.md](MCP.md)；CLI 完整参考见 [CLI.md](CLI.md)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [作为 Rust 库集成](#1-作为-rust-库集成)
2. [作为 MCP 客户端集成](#2-作为-mcp-客户端集成)
3. [作为 CLI 脚本集成](#3-作为-cli-脚本集成)
4. [Python SDK](#4-python-sdk-v016-已落地)
5. [WASM 技能 SDK（🚧 v0.1 已落地）](#5-wasm-技能-sdk-v01-已落地最小子集)

## 1. 作为 Rust 库集成

`kanyu-core` 是纯 Rust 库，零 C 依赖，`cargo build` 即可用于任何平台。

```toml
# Cargo.toml
[dependencies]
kanyu-core = { path = "crates/kanyu-core", version = "0.1.0" }
# 或直接从仓库引用：
# kanyu-core = { git = "https://github.com/DaoMingyuan/Kanyu" }
```

### 示例 1：格式探测 + 能力断言

在动手前询问注册表"这个文件是什么、能不能做这件事"：

```rust
use kanyu_core::{FormatRegistry, KanyuError};

fn main() -> kanyu_core::Result<()> {
    let reg = FormatRegistry::builtin();

    let caps = reg.detect("survey.dwg").expect("无法识别格式");
    println!("{} — driver: {}", caps.name, caps.driver);

    // 断言可写；WFS 这类只读格式会在这里被拦下
    if let Err(KanyuError::UnsupportedOperation { format, operation }) =
        reg.require("wfs", "write")
    {
        eprintln!("{format} 不支持 {operation}，换用 gpkg/geojson");
    }
    Ok(())
}
```

### 示例 2：加载 + 查询 + 导出 GeoJSON

```rust
use kanyu_core::Layer;

fn main() -> kanyu_core::Result<()> {
    let layer = Layer::load("buildings", "examples/buildings.geojson")?;
    let s = layer.summary();
    println!("{}：{} 个要素，字段 {:?}", s.id, s.feature_count, s.fields);

    let high_rise = layer.query("height > 50")?;       // 命中 2 个要素
    let text = Layer::to_geojson_string(&high_rise);
    std::fs::write("high_rise.geojson", text)?;
    Ok(())
}
```

### 示例 3：解析 + 校验 AGENTS.md

```rust
use kanyu_core::agents;

fn main() -> kanyu_core::Result<()> {
    // 没有项目罗盘？先生成一份模板（可被 parse 往返解析）
    let text = agents::template("城市更新规划", "EPSG:4526");
    std::fs::write("AGENTS.md", &text)?;

    let doc = agents::load("AGENTS.md")?;
    let issues = doc.validate();
    if issues.is_empty() {
        println!("罗盘就绪：crs = {:?}", doc.meta.crs);
    } else {
        for i in issues { eprintln!("✗ {i}"); }
    }
    Ok(())
}
```

三个示例与 kanyu-core 单元测试同源（crates/kanyu-core/src 各模块 `#[cfg(test)]`），
可对照 [API.md](API.md) 查阅每个签名。

## 2. 作为 MCP 客户端集成

堪舆以 stdio MCP Server 暴露全部工具（✅，清单见 [MCP.md §3](MCP.md#3-工具参考-)）。启动命令即 `kanyu mcp serve`；
协议握手与输出形状见 [MCP.md](MCP.md)。

### Claude Desktop 配置

通用 `mcpServers` JSON 配置见 [MCP.md §1](MCP.md#1-快速开始)（Claude Desktop 直接采用该格式）。

### Codex 配置（config.toml）

```toml
[mcp_servers.kanyu]
command = "kanyu"
args = ["mcp", "serve", "--transport", "stdio"]
```

> Windows 下若 `kanyu` 不在 PATH，将 `command` 写为绝对路径
> （如 `E:/BaiduSyncdisk/堪舆GIS/target/release/kanyu.exe`）。

### 工具调用序列

```
客户端 (LLM)                kanyu-mcp (stdio)
    │  initialize (protocolVersion 2025-06-18)   │
    │ ─────────────────────────────────────────▶ │
    │  ◀──── InitializeResult + instructions     │
    │  notifications/initialized                 │
    │ ─────────────────────────────────────────▶ │
    │  tools/list                                │
    │ ─────────────────────────────────────────▶ │
    │  ◀──── 工具清单 (inputSchema, 中文描述)      │
    │                                            │
    │  "找出高于 50 米的建筑并导出"                │
    │  tools/call kanyu_data_query               │
    │    {path, filter: "height > 50"}           │
    │ ─────────────────────────────────────────▶ │
    │  ◀──── structuredContent:                  │
    │        {feature_count: 2, collection:{…}}  │
    │  tools/call kanyu_data_export              │
    │    {path, format: "geojson", out}          │
    │ ─────────────────────────────────────────▶ │
    │  ◀──── {exported: 4, format, out}          │
```

每次 `tools/call` 无状态：内核重新加载文件、执行、返回结构化 JSON。
AI 不需要会话概念，失败重试天然幂等（读操作）。

## 3. 作为 CLI 脚本集成

CLI 是"AI 代理与 shell 脚本的第一等入口"，全局约定（细节见 [CLI.md](CLI.md#全局约定)）：

| 约定 | 内容 |
|---|---|
| `--json` | 全局标志；机器可读输出到 **stdout**，人读信息/进度到 **stderr** |
| 退出码 | `0` 成功；非 `0` 失败（错误描述在 stderr，含结构化错误如 `UnsupportedOperation`） |
| 默认输出 | 不加 `--json` 时为人读文本，适合终端检视 |

```bash
#!/usr/bin/env bash
set -euo pipefail

# 管线示例：仅当数据有效时才继续（失败即非零退出，set -e 拦截）
kanyu data info examples/buildings.geojson --json > summary.json

COUNT=$(kanyu data info examples/buildings.geojson --json | jq .feature_count)
if [ "$COUNT" -gt 0 ]; then
  kanyu data query examples/buildings.geojson \
    --filter "height > 50" --output high_rise.geojson   # 进度走 stderr，不污染管道
  kanyu data export high_rise.geojson -f geojson --out deliver.geojson
fi

# 项目罗盘校验可放进 CI
kanyu agents validate --path ./AGENTS.md --json | jq -e '.valid'
```

错误处理示例：导出未启用驱动的格式会得到非零退出与可解析的 stderr 信息——

```bash
$ kanyu data export examples/buildings.geojson -f dwg --out out.dwg
Error: 格式 'dwg' 的原生导出尚未启用（driver: libredwg-wasm）。
桥接/插件驱动将在对应阶段就绪后开放，见 docs/MASTERPLAN.md 第五部分。
$ echo $?
1
```

## 4. Python SDK（v0.16 已落地）

`kanyu-py`（PyO3 扩展模块）把 Rust 内核全量暴露给 Python——**数据契约为
GeoJSON 文本**（跨语言最稳、零宿主类型纠缠；Arrow C Data Interface 零拷贝
路线留作后续优化项，见总规裁决 #20）。

### 安装（当前为源码构建）

```bash
cargo build --release -p kanyu-py
cp target/release/kanyu.dll python/kanyu/kanyu.pyd   # Windows
# Linux/macOS 为 libkanyu.so → kanyu.so
$env:PYTHONPATH = "<repo>/python"                     # 或 pip 安装发布后免设
```

### 用法

```python
import kanyu

fc = kanyu.load("buildings.geojson")          # 全格式（shp/fgb/parquet/dxf/dwg/kml/kdb/txt…）
high = kanyu.query(fc, "height > 50")          # 属性查询
buf = kanyu.buffer(high, 500.0)                # 缓冲区（CRS 单位；米制先 reproject）
rp = kanyu.reproject(fc, "EPSG:4326", "EPSG:3857")
ds = kanyu.dissolve(fc, field="usage")         # QGIS 语义融合
s = kanyu.stats(fc)                            # 图层统计 JSON（含亩/公顷）
png = kanyu.render_png(fc, 1200, 800, "light") # PNG 字节
kanyu.export(buf, "out.kdb", "kdb")            # 导出（含自研 .kdb）
```

链式封装（`kanyu.Layer`）：

```python
kanyu.Layer.load("buildings.geojson").query("height > 50").buffer(500).export("out.fgb", "fgb")
```

### Python 工具箱（ArcGIS Pro .pyt 式样）

工具箱是一个 `.py` 文件（约定见 `python/kanyu/toolbox.py`）：`Toolbox` 子类 +
内嵌 `Tool` 子类（`name/label/description/params` + `execute(args)`），
由 CLI 驱动：

```bash
kanyu toolbox list examples/planning_tools.py
kanyu toolbox run examples/planning_tools.py buffer500 --param input=a.geojson --param distance=500
```

工具内直接 `import kanyu` 调用 Rust 内核；`kanyu toolbox` 经 JSON over stdout
与 Python 通信（参数自动类型化；`KANYU_PYTHON` 环境变量可指定包路径）。
行业工具（规划统计、属性面积图等）由此用 Python 快速迭代，内核保持稳定。

## 5. WASM 技能 SDK（🚧 v0.1 已落地最小子集）

堪舆的可扩展功能是 WASM 模块，称为"技能"（[MASTERPLAN.md](MASTERPLAN.md) §4.5），
在 wasmtime 组件模型沙箱中运行（kanyu-skill crate 宿主，见
[API.md](API.md#10-kanyu-skill--wasm-技能系统宿主)）。已定稿的 v0.1 ABI
（[wit/skill.wit](../crates/kanyu-skill/wit/skill.wit)）：

```wit
package kanyu:skill@0.1.0;

interface analyzer {
    /// 技能元数据（JSON：{"name","version","capabilities":[...]}）。
    meta: func() -> string;
    /// FeatureCollection JSON 进/出（或中文错误串）。
    run: func(input: string) -> result<string, string>;
}

world gene {
    export analyzer;
}
```

**Rust guest 编写**：参照样板技能
[`crates/kanyu-skill/testdata/attr_scaler/`](../crates/kanyu-skill/testdata/attr_scaler/)
（`wit-bindgen` `generate!`/`export!` 实现 `exports::kanyu::gene::analyzer::Guest`）：

```bash
cargo build --target wasm32-unknown-unknown --release
wasm-tools component new target/wasm32-unknown-unknown/release/<gene>.wasm -o <gene>.wasm
kanyu gene info <gene>.wasm   # 校验元数据
kanyu gene run <gene>.wasm data.geojson
```

后续迭代方向：GeoArrow RecordBatch 进出（C Data Interface 零拷贝）、
`renderer/io/panel/tool` 技能类型、热加载与 A/B 测试生命周期（总规 §4.5.3）。
