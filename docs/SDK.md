# 堪舆 SDK 使用指南（SDK）

> 版本：v0.1.0 ｜ 三种集成方式：Rust 库、MCP 客户端、CLI 脚本。
> API 细节见 [API.md](API.md)；MCP 协议细节见 [MCP.md](MCP.md)；CLI 完整参考见 [CLI.md](CLI.md)。

状态标记：✅ 已实现 ｜ 🚧 进行中 ｜ 📋 计划中

## 目录

1. [作为 Rust 库集成](#1-作为-rust-库集成)
2. [作为 MCP 客户端集成](#2-作为-mcp-客户端集成)
3. [作为 CLI 脚本集成](#3-作为-cli-脚本集成)
4. [Python SDK（📋 规划中）](#4-python-sdk--规划中)
5. [WASM 基因 SDK（🚧 v0.1 已落地）](#5-wasm-基因-sdk-v01-已落地最小子集)

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

## 4. Python SDK（📋 规划中）

计划以 PyO3 绑定暴露 `kanyu-core`，设计原则一段话：

> Python 侧只做**零拷贝视图**，不做数据复制：`Layer` 在 Rust 侧持有 GeoArrow
> RecordBatch，Python 通过 Arrow C Data Interface / nanoarrow 直接映射同一块内存
> （呼应总规 §3.1 "Python/Rust 共享同一块物理内存"）。API 与 Rust 侧一一对应
> （`kanyu.Layer.load(...).query("height > 50")`），错误映射为单一
> `kanyu.KanyuError` 层次；不引入 GIL 持有的长临界区，重活在 Rust 侧 `py.allow_threads` 中执行。

## 5. WASM 基因 SDK（🚧 v0.1 已落地最小子集）

堪舆的可扩展功能是 WASM 模块，称为"基因"（[MASTERPLAN.md](MASTERPLAN.md) §4.5），
在 wasmtime 组件模型沙箱中运行（kanyu-gene crate 宿主，见
[API.md](API.md#10-kanyu-gene--wasm-基因系统宿主)）。已定稿的 v0.1 ABI
（[wit/gene.wit](../crates/kanyu-gene/wit/gene.wit)）：

```wit
package kanyu:gene@0.1.0;

interface analyzer {
    /// 基因元数据（JSON：{"name","version","capabilities":[...]}）。
    meta: func() -> string;
    /// FeatureCollection JSON 进/出（或中文错误串）。
    run: func(input: string) -> result<string, string>;
}

world gene {
    export analyzer;
}
```

**Rust guest 编写**：参照样板基因
[`crates/kanyu-gene/testdata/attr_scaler/`](../crates/kanyu-gene/testdata/attr_scaler/)
（`wit-bindgen` `generate!`/`export!` 实现 `exports::kanyu::gene::analyzer::Guest`）：

```bash
cargo build --target wasm32-unknown-unknown --release
wasm-tools component new target/wasm32-unknown-unknown/release/<gene>.wasm -o <gene>.wasm
kanyu gene info <gene>.wasm   # 校验元数据
kanyu gene run <gene>.wasm data.geojson
```

后续迭代方向：GeoArrow RecordBatch 进出（C Data Interface 零拷贝）、
`renderer/io/panel/tool` 基因类型、热加载与 A/B 测试生命周期（总规 §4.5.3）。
