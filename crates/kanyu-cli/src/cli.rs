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
    /// Box 收纳：parcel-map 参数组使 RenderCommand 体积远超其他子命令
    /// （clippy large_enum_variant），堆间址保持 Command 枚举紧凑。
    #[command(subcommand)]
    Render(Box<RenderCommand>),

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

    /// 坐标参考系（CRS）：EPSG 全库检索与条目检视（内置 EPSG 数据库 7507 条）。
    #[command(subcommand)]
    Crs(CrsCommand),

    /// 工具箱注册表（QGIS Processing 式，core::tooldef 单一事实来源）：
    /// 列出/执行内核工具，与壳层工具箱面板、MCP 工具面共用同一注册表。
    #[command(subcommand)]
    Tool(ToolCommand),
}

/// `kanyu tool ...`
#[derive(Subcommand, Debug)]
pub enum ToolCommand {
    /// 列出工具注册表（id/中文名/分类/说明；--json 输出含参数表全量定义，
    /// 供 AI 代理与 DSH 组件发现）。
    List,
    /// 执行注册表工具：Layer 类参数值给数据文件路径（多图层参数逗号/换行
    /// 分隔多个路径），枚举参数给内核值或中文标签均可。
    Run {
        /// 工具 id（`kanyu tool list` 查看）。
        id: String,
        /// 参数（k=v 形式，键为参数 key，可多个；缺省取参数默认值）。
        #[arg(long = "param")]
        params: Vec<String>,
        /// 结果输出路径（GeoJSON；多产出工具视作输出目录，逐组一个文件；
        /// 缺省打印到 stdout）。工具声明了输出文件参数（OutFile）时由内核
        /// 直接按扩展名格式写盘，与本旗标无关。
        #[arg(long)]
        output: Option<String>,
    },
}

/// `kanyu crs ...`
#[derive(Subcommand, Debug)]
pub enum CrsCommand {
    /// EPSG 全库检索：按代码子串或名称（大小写不敏感）匹配；空查询返回常用精选。
    Search {
        /// 检索词（代码子串如 "4547"，或名称片段如 "CGCS2000"）；缺省返回常用精选。
        query: Option<String>,
        /// 结果上限（默认 20）。
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// 按代码检视 EPSG 条目详情（名称/类型/单位/proj4 定义串）。
    Info {
        /// EPSG 代码（如 4326、4547；代码域 2000..=32766）。
        code: u32,
    },
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
    /// 字段计算器：按表达式计算/新建目标字段（+-*/%、比较、and/or/not、
    /// round/upper/concat/coalesce 等函数与 $area/$length/$x/$y 几何虚列）。
    Calc {
        /// 数据文件路径。
        file: String,
        /// 目标字段（不存在则新建，存在则覆盖）。
        #[arg(long)]
        target: String,
        /// 表达式（如 `[height] * 2` 或 `$area / 10000`）。
        #[arg(long)]
        expr: String,
        /// 结果输出路径（GeoJSON）；缺省打印到 stdout。
        #[arg(long)]
        output: Option<String>,
    },
    /// 多图层打包为堪舆数据库（KDB v2 zip 容器：每输入文件成为一个命名图层，
    /// 图层名=文件名主干；面向不动产登记数据库标准的多表形态单文件建库）。
    KdbPack {
        /// 输入数据文件（任意注册格式，可多个；同名主干报中文错误）。
        files: Vec<String>,
        /// 输出路径（.kdb）。
        #[arg(long)]
        out: String,
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
/// 宗地图参数组（20+ 出图参数）使 ParcelMap 变体远大于其他变体——
/// CLI 一次性解析、无热路径，尺寸差异可接受，豁免 large_enum_variant。
#[allow(clippy::large_enum_variant)]
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
        /// 背景色（#RRGGBB；缺省用主题画布色；none/transparent = 透明背景，
        /// 供底图叠加场景）。
        #[arg(long)]
        background: Option<String>,
        /// 属性驱动样式规则（内联 JSON；与 --style-file 二选一）。
        #[arg(long)]
        style: Option<String>,
        /// 样式规则 JSON 文件路径（与 --style 二选一）。
        #[arg(long)]
        style_file: Option<String>,
    },
    /// 打印布局排版（ArcGIS Pro Layout 对应物）：纸张 + 标题 + 地图框 +
    /// 图例/比例尺/指北针（输出格式按 --out 扩展名判定：svg 全排版 / png）。
    Layout {
        /// 数据文件路径。
        file: String,
        /// 输出路径（.svg 或 .png）。
        #[arg(long)]
        out: String,
        /// 布局标题（顶部居中；缺省不绘）。
        #[arg(long)]
        title: Option<String>,
        /// 纸张：a4l（A4 横，默认）/a4p（A4 纵）。
        #[arg(long, default_value = "a4l")]
        page: String,
        /// 分辨率 dpi（默认 96）。
        #[arg(long, default_value_t = 96.0)]
        dpi: f64,
        /// 关闭图例。
        #[arg(long)]
        no_legend: bool,
        /// 关闭比例尺。
        #[arg(long)]
        no_scalebar: bool,
        /// 关闭指北针。
        #[arg(long)]
        no_north: bool,
        /// 主题：light（晨山）/dark（夜观星）。
        #[arg(long, default_value = "light")]
        theme: String,
        /// 属性驱动样式规则（内联 JSON；图例行随之分类；与 --style-file 二选一）。
        #[arg(long)]
        style: Option<String>,
        /// 样式规则 JSON 文件路径（与 --style 二选一）。
        #[arg(long)]
        style_file: Option<String>,
    },
    /// 宗地图出图（GB/T 42547-2023《地籍调查规程》图 L.3 版式）：界址点 Ø2.0mm 符号 +
    /// 红界址线 + J 点号/边长注记（勘测定界图注记契约排版）+ 界址点坐标表 +
    /// 比例尺（分母取整百）+ 指北针 + 签注栏（输出格式按 --out 扩展名判定：svg/png）。
    ParcelMap {
        /// 宗地数据文件（面要素；GeoJSON/SHP/宗地 TXT/DXF 等注册格式，
        /// 多面要素缺省取面积最大者，可用 --index 指定）。
        file: String,
        /// 输出路径（.svg 或 .png）。
        #[arg(long)]
        out: String,
        /// 宗地代码（缺省取要素属性 parcel_id/ZDDM/zddm）。
        #[arg(long)]
        parcel_code: Option<String>,
        /// 土地权利人（缺省取属性 owner/QLRMC/parcel_name）。
        #[arg(long)]
        owner: Option<String>,
        /// 所在图幅号（缺省取属性 map_sheet/TFH）。
        #[arg(long)]
        map_sheet: Option<String>,
        /// 宗地面积（㎡；缺省取属性 area/ZDMJ，再无按几何现算）。
        #[arg(long)]
        area: Option<f64>,
        /// 地类编码（缺省取属性 parcel_use/YT）。
        #[arg(long)]
        land_use: Option<String>,
        /// 左侧竖排单位名（如 XXX自然资源局）。
        #[arg(long, default_value = "")]
        unit_name: String,
        /// 测绘说明（左下；如「2026年08月解析法测绘界址点」）。
        #[arg(long, default_value = "")]
        survey_note: String,
        /// 制图者。
        #[arg(long, default_value = "")]
        drawer: String,
        /// 审核者。
        #[arg(long, default_value = "")]
        reviewer: String,
        /// 制图日期。
        #[arg(long, default_value = "")]
        draw_date: String,
        /// 审核日期。
        #[arg(long, default_value = "")]
        review_date: String,
        /// 东至注记（邻宗地；缺省取属性 ZDSZD；`\n` 分行）。
        #[arg(long, default_value = "")]
        sizhi_e: String,
        /// 南至注记（邻宗地；缺省取属性 ZDSZN）。
        #[arg(long, default_value = "")]
        sizhi_s: String,
        /// 西至注记（邻宗地；缺省取属性 ZDSZX）。
        #[arg(long, default_value = "")]
        sizhi_w: String,
        /// 北至注记（邻宗地；缺省取属性 ZDSZB）。
        #[arg(long, default_value = "")]
        sizhi_n: String,
        /// 相邻道路线文件（任意注册格式线要素；路名取属性
        /// name/NAME/road_name/道路名称/DLMC；按地图框裁剪，路名沿线）。
        #[arg(long)]
        roads: Option<String>,
        /// 比例尺分母（缺省自动适配取整百）。
        #[arg(long)]
        scale: Option<u32>,
        /// 分辨率 dpi（默认 150，仅 PNG）。
        #[arg(long, default_value_t = 150.0)]
        dpi: f64,
        /// 面要素序号（缺省取面积最大面要素；指定后按文档序第 N 个，0 起）。
        #[arg(long)]
        index: Option<usize>,
    },
    /// 宗地 CASS 兼容 DXF 导出（南方 CASS 联动）：ZD/JZX/JZD/ZJ 分层 +
    /// SOUTH 编码 XDATA（界址点 302001/界址线 302002），CASS 直接打开编辑。
    ParcelDxf {
        /// 宗地数据文件（面要素；同 parcel-map）。
        file: String,
        /// 输出路径（.dxf）。
        #[arg(long)]
        out: String,
        /// 宗地代码（分式分子取末 7 位；缺省取属性 parcel_id/ZDDM/zddm）。
        #[arg(long)]
        parcel_code: Option<String>,
        /// 地类编码（分式分母；缺省取属性 parcel_use/YT）。
        #[arg(long)]
        land_use: Option<String>,
        /// 土地权利人（ZJ 注记；缺省取属性 owner/QLRMC/parcel_name）。
        #[arg(long)]
        owner: Option<String>,
        /// 出图比例尺分母（纸面毫米要素换算模型单位；默认 1000）。
        #[arg(long, default_value_t = 1000)]
        scale: u32,
        /// 不挂 SOUTH 编码 XDATA。
        #[arg(long)]
        no_xdata: bool,
        /// 面要素序号（缺省取面积最大面要素；指定后按文档序第 N 个，0 起）。
        #[arg(long)]
        index: Option<usize>,
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
        /// 地理项目骨架（含「数据层语义表」章节）。
        #[arg(long)]
        geo: bool,
        /// 软件工程仓库骨架（免数据层语义表，写入 `data-layer: 否`）。
        #[arg(long)]
        code_repo: bool,
    },
    /// 校验 AGENTS.md 完整性（元数据/图层语义/业务规则）。零参自动裁决：
    /// 由 `AgentsMd::resolve_data_layer` 按「`data-layer` 元数据行优先 → crs
    /// 占位回退」判定语境，地理与代码两类仓库均可一次通过。
    Validate {
        /// AGENTS.md 路径（默认 ./AGENTS.md）。
        #[arg(long, default_value = "AGENTS.md")]
        path: String,
        /// 钉死为软件工程/代码仓库语境（免检数据层语义表）——等价于该文件
        /// 元数据 `data-layer: 否` 显式声明的 CLI 直连形态。
        #[arg(long, visible_alias = "code-repo")]
        check_code_repo: bool,
        /// 钉死为地理项目语境（数据层语义表必填）。与 --check-code-repo 互斥。
        #[arg(long)]
        geo: bool,
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
