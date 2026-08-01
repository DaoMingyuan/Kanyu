//! `kanyu` CLI 的命令结构定义（clap derive）。
//!
//! 设计对齐 docs/CLI.md；每个顶层子命令对应内核的一组能力。

use clap::{Parser, Subcommand, ValueEnum};

/// 堪舆 (Kanyu) —— AI 原生地理空间操作系统。
#[derive(Parser, Debug)]
#[command(
    name = "kanyu",
    version,
    about = "堪舆 (Kanyu) — AI-native geospatial operating system",
    long_about = "以天地为盘，以数据为爻，以 AI 为神。\n\nkanyu 是堪舆系统的脊髓：数据、分析、自省、插件与 MCP 的统一入口。",
    propagate_version = true
)]
pub struct Cli {
    /// 以 JSON 输出（AI 代理与脚本默认开启）。
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// 顶层命令。
#[derive(Subcommand, Debug)]
pub enum Command {
    /// 数据操作：加载、查询、导出、检视。
    #[command(subcommand)]
    Data(DataCommand),

    /// 空间分析：缓冲、叠加、拓扑检查。
    #[command(subcommand)]
    Analysis(AnalysisCommand),

    /// 地图渲染：离屏出图（PNG/SVG）。
    #[command(subcommand)]
    Render(RenderCommand),

    /// 系统自省 —— AI 读取自身（源码树、能力矩阵、工具清单）。
    Introspect,

    /// AGENTS.md 项目语义文件：生成与校验。
    #[command(subcommand)]
    Agents(AgentsCommand),

    /// MCP 神经接口：启动 MCP Server，供 AI 代理接入。
    #[command(subcommand)]
    Mcp(McpCommand),
}

/// `kanyu data ...`
#[derive(Subcommand, Debug)]
pub enum DataCommand {
    /// 检视数据文件：格式探测、要素数、字段清单。
    Info {
        /// 数据文件路径。
        file: String,
    },
    /// 加载数据文件到会话（打印图层概要）。
    Load {
        /// 数据文件路径。
        file: String,
        /// 图层别名（默认取文件名主干）。
        #[arg(long = "as")]
        alias: Option<String>,
        /// 坐标系声明（如 EPSG:4326）。
        #[arg(long)]
        crs: Option<String>,
    },
    /// 对数据执行属性查询，如 --filter="height > 50"。
    Query {
        /// 数据文件路径。
        file: String,
        /// 过滤表达式："field op value"，op ∈ == != > >= < <=。
        #[arg(long)]
        filter: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 导出为目标格式（-f dwg 等；受格式能力矩阵约束）。
    Export {
        /// 数据文件路径。
        file: String,
        /// 目标格式短名（见 `kanyu introspect` 的格式矩阵）。
        #[arg(long, short = 'f')]
        format: String,
        /// 输出路径。
        #[arg(long)]
        out: String,
        /// 保留符号化映射。
        #[arg(long)]
        symbol_mapping: bool,
    },
    /// 投影变换（--from/--to 为 EPSG:xxxx 或 proj4 定义串；内置 EPSG 数据库）。
    Reproject {
        /// 数据文件路径。
        file: String,
        /// 源 CRS（如 EPSG:4326）。
        #[arg(long)]
        from: String,
        /// 目标 CRS（如 EPSG:3857）。
        #[arg(long)]
        to: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
}

/// `kanyu analysis ...`
#[derive(Subcommand, Debug)]
pub enum AnalysisCommand {
    /// 缓冲区分析（distance 单位为数据 CRS 单位；EPSG:4326 下是度，米制缓冲需先投影）。
    Buffer {
        /// 数据文件路径。
        file: String,
        /// 缓冲距离。
        #[arg(long)]
        distance: f64,
        /// 圆弧拟合的每象限分段数（默认 8）。
        #[arg(long, default_value_t = 8)]
        segments: usize,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 叠加分析（仅 Polygon/MultiPolygon 面要素）。
    Overlay {
        /// 目标图层文件。
        target: String,
        /// 叠加图层文件。
        overlay: String,
        /// 操作：union/intersection/difference/xor。
        #[arg(long)]
        operation: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 拓扑检查（--json 输出 TopologyReport）。
    Topology {
        /// 数据文件路径。
        file: String,
        /// 规则（逗号分隔，支持 no_overlap）。
        #[arg(long)]
        rules: String,
    },
    /// 测地线度量（Karney 2013；长度米/面积平方米；--json 输出明细）。
    Measure {
        /// 数据文件路径。
        file: String,
        /// 度量类型：length/area。
        #[arg(long)]
        kind: String,
    },
    /// 空间连接（左连接 + 匹配展开；一对多匹配各输出一条）。
    Sjoin {
        /// 目标图层文件。
        target: String,
        /// 连接图层文件。
        join: String,
        /// 空间谓词：intersects/contains/within。
        #[arg(long)]
        predicate: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 分区统计（values 按质心/代表点归属 zones 面要素）。
    Zonal {
        /// 分区图层文件（仅面要素）。
        zones: String,
        /// 数值图层文件。
        values: String,
        /// 数值字段名。
        #[arg(long)]
        field: String,
        /// 统计项（逗号分隔：count,sum,mean,min,max）。
        #[arg(long)]
        stats: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
}

/// `kanyu render ...`
#[derive(Subcommand, Debug)]
pub enum RenderCommand {
    /// 渲染数据文件为地图图片（输出格式按 --out 扩展名判定：png/svg）。
    Map {
        /// 数据文件路径。
        file: String,
        /// 输出路径（.png 或 .svg）。
        #[arg(long)]
        out: String,
        /// 图片宽度（像素，默认 800）。
        #[arg(long, default_value_t = 800)]
        width: u32,
        /// 图片高度（像素，默认 600）。
        #[arg(long, default_value_t = 600)]
        height: u32,
        /// 主题：light（晨山）/dark（夜观星）。
        #[arg(long, default_value = "light")]
        theme: String,
    },
}

/// `kanyu agents ...`
#[derive(Subcommand, Debug)]
pub enum AgentsCommand {
    /// 在指定目录生成 AGENTS.md 项目语义模板。
    Init {
        /// 项目目录（默认当前目录）。
        #[arg(long, default_value = ".")]
        project: String,
        /// 项目名（默认取目录名）。
        #[arg(long)]
        name: Option<String>,
        /// 坐标参考系（默认 EPSG:4326）。
        #[arg(long, default_value = "EPSG:4326")]
        crs: String,
        /// 覆盖已存在的 AGENTS.md。
        #[arg(long)]
        force: bool,
    },
    /// 校验 AGENTS.md 完整性（元数据/图层语义/业务规则）。
    Validate {
        /// AGENTS.md 路径（默认 ./AGENTS.md）。
        #[arg(long, default_value = "AGENTS.md")]
        path: String,
    },
}

/// `kanyu mcp ...`
#[derive(Subcommand, Debug)]
pub enum McpCommand {
    /// 启动 MCP Server。
    Serve {
        /// 传输方式：stdio（本地 AI 助手）或 http（streamable HTTP，远程代理）。
        #[arg(long, value_enum, default_value_t = Transport::Stdio)]
        transport: Transport,
        /// HTTP 模式监听端口（绑定 127.0.0.1）。
        #[arg(long, default_value_t = 3000)]
        port: u16,
    },
}

/// MCP 传输方式。
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// 标准输入输出（本地 AI 助手默认）。
    Stdio,
    /// streamable HTTP（远程 AI 代理；官方已取代旧 SSE）。
    Http,
}
