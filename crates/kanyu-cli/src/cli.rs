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

    /// WASM 技能：检视与执行插件（wasmtime 沙箱）。
    #[command(subcommand)]
    Skill(SkillCommand),

    /// 系统自省 —— AI 读取自身（源码树、能力矩阵、工具清单）。
    Introspect,

    /// AGENTS.md 项目语义文件：生成与校验。
    #[command(subcommand)]
    Agents(AgentsCommand),

    /// MCP 神经接口：启动 MCP Server，供 AI 代理接入。
    #[command(subcommand)]
    Mcp(McpCommand),

    /// Python 工具箱（ArcGIS .pyt 式）：列出/执行 Python 编写的工具。
    #[command(subcommand)]
    Toolbox(ToolboxCommand),
}

/// `kanyu toolbox ...`
#[derive(Subcommand, Debug)]
pub enum ToolboxCommand {
    /// 列出工具箱文件中的工具清单。
    List {
        /// 工具箱 .py 文件路径。
        file: String,
    },
    /// 执行工具箱中的工具。
    Run {
        /// 工具箱 .py 文件路径。
        file: String,
        /// 工具名（toolbox list 查看）。
        tool: String,
        /// 参数（k=v 形式，可多个）。
        #[arg(long = "param")]
        params: Vec<String>,
    },
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
    /// 数据质检（宗地 TXT：表头必备项/中文逗号/闭合环等规则；--json 输出问题清单）。
    Validate {
        /// 数据文件路径（当前支持 .txt 宗地/点表格式）。
        file: String,
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
    /// 融合（QGIS Dissolve：按字段分组并集；属性取组字段值+组内首要素）。
    Dissolve {
        /// 数据文件路径。
        file: String,
        /// 分组字段（缺省全图融合）。
        #[arg(long)]
        field: Option<String>,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 道格拉斯简化（tolerance 为 CRS 单位）。
    Simplify {
        /// 数据文件路径。
        file: String,
        /// 简化容差。
        #[arg(long)]
        tolerance: f64,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 质心（逐要素 Point，属性随行）。
    Centroid {
        /// 数据文件路径。
        file: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 凸包（逐要素 Polygon）。
    Convexhull {
        /// 数据文件路径。
        file: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 删洞（--min-area 保留面积≥阈值的洞，缺省全删）。
    Deleteholes {
        /// 数据文件路径。
        file: String,
        /// 洞面积阈值（CRS 平面单位）。
        #[arg(long)]
        min_area: Option<f64>,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 多部件炸开（QGIS Multipart to singleparts）。
    Explode {
        /// 数据文件路径。
        file: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 图层统计（测地线口径；面积含亩/公顷/平方千米）。
    Stats {
        /// 数据文件路径。
        file: String,
    },
    /// 性能基准（ARCHITECTURE §8）：对加载解析/buffer/overlay/sjoin/render_png
    /// 各场景计时（每项 3 次取中位数；场景由内核 bench 生成器确定性产出，
    /// 大文件落 target/bench/ 不入仓库）。
    Bench {
        /// 基准规模（混合数据集要素数，默认 10000）。
        #[arg(long, default_value_t = 10000)]
        size: usize,
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
        /// 属性驱动样式规则（内联 JSON；与 --style-file 二选一）。
        #[arg(long)]
        style: Option<String>,
        /// 样式规则 JSON 文件路径（与 --style 二选一）。
        #[arg(long)]
        style_file: Option<String>,
    },
}

/// `kanyu skill ...`
#[derive(Subcommand, Debug)]
pub enum SkillCommand {
    /// 检视 WASM 技能元数据（name/version/capabilities）。
    Info {
        /// 技能文件路径（.wasm 组件）。
        plugin: String,
    },
    /// 在数据上执行分析技能（FeatureCollection 进/出）。
    Run {
        /// 技能文件路径（.wasm 组件）。
        plugin: String,
        /// 数据文件路径。
        file: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
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
