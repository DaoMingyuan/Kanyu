//! MCP Server：工具路由与 stdio 服务。

use kanyu_core::{agents, introspect, tooldef, toolrun, FormatRegistry, KanyuError, Layer};
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    service::{MaybeSendFuture, RequestContext},
    transport::stdio,
    ErrorData as McpError, Json, RoleServer, ServerHandler, ServiceExt,
};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

/// 可任务化的分析工具白名单（SEP-2663 试点：`tools/call` 的 arguments 带
/// `"task": true` 时异步执行；其余工具忽略该键走同步路由）。
/// `kanyu_skill_run` 同待遇（技能执行可能耗时）；`kanyu_toolbox_run`
/// 同待遇（工具箱统一执行入口，大数据量工具可能耗时）。
const TASK_ELIGIBLE: [&str; 7] = [
    "kanyu_analysis_buffer",
    "kanyu_analysis_overlay",
    "kanyu_analysis_sjoin",
    "kanyu_analysis_zonal_stats",
    "kanyu_analysis_topology",
    "kanyu_skill_run",
    "kanyu_toolbox_run",
];

/// 任务结果保留时长（10 分钟；TaskManager 惰性 TTL 清扫，重启即丢——
/// 内存态注册表，见 docs/MCP.md §2）。
const TASK_TTL_MS: u64 = 600_000;

/// WASM 技能注册表（内存态，Clone 共享；重启即丢——与任务管理器同生命周期）。
type GeneRegistry = std::sync::Arc<std::sync::Mutex<SkillRegistryState>>;

/// 注册表状态：宿主 + 已注册技能（skill_id = meta.name）。
struct SkillRegistryState {
    host: kanyu_skill::SkillHost,
    skills: std::collections::HashMap<String, kanyu_skill::Skill>,
}

/// 堪舆 MCP Server。工具调用无状态（每次调用重新加载执行）；
/// 另持有 SEP-2663 任务管理器与 WASM 技能注册表（均内存态，Clone 共享）。
#[derive(Clone)]
pub struct KanyuServer {
    /// rmcp 工具路由表（由 #[tool_router] 宏生成并填充）。
    tool_router: ToolRouter<Self>,
    /// SEP-2663 任务管理器：spawn/TTL/协作取消（rmcp 3.1 内置）。
    task_manager: rmcp::task_manager::TaskManager,
    /// WASM 技能注册表：hotload 校验注册 / skill_run 沙箱执行
    /// （v0.1 技能调用在锁内串行化，注释即契约；按-names 细粒度锁 📋）。
    skills: GeneRegistry,
}

/// `kanyu_data_load` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataLoadReq {
    /// 数据文件路径（格式自动探测）。
    pub path: String,
    /// 图层别名（默认取文件名主干）。
    pub alias: Option<String>,
    /// 坐标系声明（如 EPSG:4326）。
    pub crs: Option<String>,
}

/// `kanyu_data_query` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataQueryReq {
    /// 数据文件路径。
    pub path: String,
    /// 过滤表达式："field op value"，op ∈ == != > >= < <=。例：height > 50。
    pub filter: String,
}

/// `kanyu_data_export` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataExportReq {
    /// 数据文件路径。
    pub path: String,
    /// 目标格式短名（geojson/dxf/fgb/...，受格式能力矩阵约束）。
    pub format: String,
    /// 输出路径。
    pub out: String,
}

/// `kanyu_agents_validate` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentsValidateReq {
    /// AGENTS.md 路径。
    pub path: String,
    /// 校验上下文（可选）。缺省 → 自动判定（元数据 `data-layer` 行 → crs 占位）；
    /// `geo` → 地理项目（数据层语义表必填）；`code-repo` → 钉死软件仓库语境
    /// （免检数据层），等价于 AGENTS.md「校验契约」零参/`--check-code-repo` 形态。
    #[serde(default, alias = "context")]
    pub ctx: Option<String>,
}

/// `kanyu_agents_init` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AgentsInitReq {
    /// 项目目录。
    pub project: String,
    /// 项目名（默认取目录名）。
    pub name: Option<String>,
    /// 坐标参考系（默认 EPSG:4326）。
    pub crs: Option<String>,
    /// 模板种类（可选）。缺省 → 地理项目（`geo`，含数据层语义表）；`code-repo` →
    /// 软件/代码仓库模板（`data-layer: 否`，免数据层语义表），与 CLI
    /// `init <dir> --code-repo` 一致。
    #[serde(default, alias = "kind")]
    pub ctx: Option<String>,
}

/// `kanyu_analysis_buffer` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisBufferReq {
    /// 数据文件路径。
    pub path: String,
    /// 缓冲距离（数据 CRS 单位；EPSG:4326 下是度，米制缓冲需先投影）。
    pub distance: f64,
    /// 圆弧拟合的每象限分段数（默认 8）。
    pub segments: Option<usize>,
}

/// `kanyu_analysis_overlay` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisOverlayReq {
    /// 目标图层文件路径。
    pub target: String,
    /// 叠加图层文件路径。
    pub overlay: String,
    /// 操作：union/intersection/difference/xor。
    pub operation: String,
}

/// `kanyu_analysis_topology` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisTopologyReq {
    /// 数据文件路径。
    pub path: String,
    /// 规则清单（支持 no_overlap）。
    pub rules: Vec<String>,
}

/// `kanyu_data_reproject` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataReprojectReq {
    /// 数据文件路径。
    pub path: String,
    /// 源 CRS（"EPSG:xxxx" 或 proj4 定义串；内置 EPSG 数据库）。
    pub from: String,
    /// 目标 CRS（同 from 格式）。
    pub to: String,
    /// 输出路径（可选；缺省返回转换后的 FeatureCollection）。
    pub out: Option<String>,
}

/// `kanyu_analysis_measure` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisMeasureReq {
    /// 数据文件路径。
    pub path: String,
    /// 度量类型：length（米）/ area（平方米）。
    pub kind: String,
}

/// `kanyu_analysis_sjoin` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisSjoinReq {
    /// 目标图层文件路径。
    pub target: String,
    /// 连接图层文件路径。
    pub join: String,
    /// 空间谓词：intersects/contains/within。
    pub predicate: String,
}

/// `kanyu_analysis_zonal_stats` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisZonalStatsReq {
    /// 分区图层文件路径（仅面要素）。
    pub zones: String,
    /// 数值图层文件路径。
    pub values: String,
    /// 数值字段名。
    pub field: String,
    /// 统计项清单（count/sum/mean/min/max）。
    pub stats: Vec<String>,
}

/// `kanyu_render_map` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderMapReq {
    /// 数据文件路径。
    pub path: String,
    /// 输出格式：png（base64 图片回传）/ svg（源码文本回传）。
    pub format: String,
    /// 图片宽度（像素，默认 800）。
    pub width: Option<u32>,
    /// 图片高度（像素，默认 600）。
    pub height: Option<u32>,
    /// 主题：light（晨山）/ dark（夜观星）。
    pub theme: Option<String>,
    /// 属性驱动样式规则（可选 JSON 对象，缺省走主题默认样式）：
    /// {"type":"graduated","field":"height","stops":[[阈值,"#RRGGBB"],…]}（数值分档，取最后满足 值≥阈值 的档，阈值严格升序）或
    /// {"type":"categorical","field":"usage","colors":{"类别":"#RRGGBB"},"default":"#RRGGBB"}（字符串类别映射）。
    pub style: Option<serde_json::Value>,
}

/// `kanyu_system_hotload` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SystemHotloadReq {
    /// WASM 技能文件路径（.wasm 组件；加载校验失败绝不注册——hotload 即"验证"职责）。
    pub wasm_path: String,
}

/// `kanyu_render_parcel_map` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderParcelMapReq {
    /// 宗地数据文件路径（面要素；GeoJSON/SHP/宗地 TXT/DXF/kdb 等注册格式，
    /// 多面要素缺省取面积最大者，可用 index 指定）。
    pub path: String,
    /// 输出格式：svg（源码文本回传）/ png（base64 图片回传）。
    pub format: String,
    /// 可选落盘路径（.svg/.png；给定则同时写文件）。
    pub out: Option<String>,
    /// 宗地代码（缺省取属性 parcel_id/ZDDM/zddm）。
    pub parcel_code: Option<String>,
    /// 土地权利人（缺省取属性 owner/QLRMC/parcel_name）。
    pub owner: Option<String>,
    /// 所在图幅号（缺省取属性 map_sheet/TFH）。
    pub map_sheet: Option<String>,
    /// 宗地面积（㎡；缺省取属性 area/ZDMJ，再无按几何现算）。
    pub area: Option<f64>,
    /// 地类编码（缺省取属性 parcel_use/YT）。
    pub land_use: Option<String>,
    /// 左侧竖排单位名（如 XXX自然资源局）。
    pub unit_name: Option<String>,
    /// 左下测绘说明（如「2026年08月解析法测绘界址点」）。
    pub survey_note: Option<String>,
    /// 制图者。
    pub drawer: Option<String>,
    /// 审核者。
    pub reviewer: Option<String>,
    /// 制图日期。
    pub draw_date: Option<String>,
    /// 审核日期。
    pub review_date: Option<String>,
    /// 东至注记（邻宗地；缺省取属性 ZDSZD；`\n` 分行）。
    pub sizhi_e: Option<String>,
    /// 南至注记（缺省取属性 ZDSZN）。
    pub sizhi_s: Option<String>,
    /// 西至注记（缺省取属性 ZDSZX）。
    pub sizhi_w: Option<String>,
    /// 北至注记（缺省取属性 ZDSZB）。
    pub sizhi_n: Option<String>,
    /// 相邻道路线文件路径（任意注册格式线要素；路名取属性
    /// name/NAME/road_name/道路名称/DLMC；按地图框裁剪，路名沿线）。
    pub roads: Option<String>,
    /// 比例尺分母（缺省自动适配取整百）。
    pub scale: Option<u32>,
    /// PNG 分辨率 dpi（默认 150，SVG 忽略）。
    pub dpi: Option<f64>,
    /// 面要素序号（缺省面积最大者；指定后按文档序第 N 个，0 起）。
    pub index: Option<usize>,
}

/// `kanyu_render_parcel_dxf` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderParcelDxfReq {
    /// 宗地数据文件路径（同 kanyu_render_parcel_map）。
    pub path: String,
    /// 输出路径（.dxf，必填落盘）。
    pub out: String,
    /// 宗地代码（分式分子取末 7 位；缺省取属性 parcel_id/ZDDM/zddm）。
    pub parcel_code: Option<String>,
    /// 地类编码（分式分母；缺省取属性 parcel_use/YT）。
    pub land_use: Option<String>,
    /// 土地权利人（ZJ 注记；缺省取属性 owner/QLRMC/parcel_name）。
    pub owner: Option<String>,
    /// 出图比例尺分母（纸面毫米要素换算模型单位；默认 1000）。
    pub scale: Option<u32>,
    /// 不挂 SOUTH 编码 XDATA（默认 false=挂载 302001/302002）。
    pub no_xdata: Option<bool>,
    /// 面要素序号（缺省面积最大者；指定后按文档序第 N 个，0 起）。
    pub index: Option<usize>,
}

/// `kanyu_data_kdb_pack` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataKdbPackReq {
    /// 输入数据文件（任意注册格式，可多个；图层名=文件主干，重名中文报错）。
    pub files: Vec<String>,
    /// 输出路径（.kdb，KDB v2 多图层容器）。
    pub out: String,
}

/// `kanyu_render_sea_map` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RenderSeaMapReq {
    /// 宗海数据文件路径（面要素；GeoJSON/SHP/宗地 TXT/DXF/kdb 等注册格式，
    /// 多面要素缺省取面积最大者，可用 index 指定）。
    pub path: String,
    /// 图种：boundary（宗海界址图 L.7，默认）/ location（宗海位置图 L.6）/
    /// layout（宗海平面布置图 L.8）。
    pub kind: Option<String>,
    /// 输出格式：svg（源码文本回传）/ png（base64 图片回传）。
    pub format: String,
    /// 可选落盘路径（.svg/.png；给定则同时写文件）。
    pub out: Option<String>,
    /// 项目名称（标题前缀；缺省取属性 project_name/XMMC）。
    pub project_name: Option<String>,
    /// 宗海代码（缺省取属性 sea_code/ZHDM）。
    pub sea_code: Option<String>,
    /// 源坐标系（EPSG:xxxx 或纯数字；界址点坐标表经此反算 CGCS2000 经纬度
    /// 度分秒；默认 EPSG:4527；仅 boundary 图种使用）。
    pub source_epsg: Option<String>,
    /// 测绘单位。
    pub survey_unit: Option<String>,
    /// 测量员。
    pub surveyor: Option<String>,
    /// 绘图员。
    pub drawer: Option<String>,
    /// 绘制日期。
    pub draw_date: Option<String>,
    /// 检查人。
    pub inspector: Option<String>,
    /// 审核人。
    pub reviewer: Option<String>,
    /// 比例尺分母（缺省自动适配取整百）。
    pub scale: Option<u32>,
    /// PNG 分辨率 dpi（默认 150，SVG 忽略）。
    pub dpi: Option<f64>,
    /// 面要素序号（缺省面积最大者；指定后按文档序第 N 个，0 起）。
    pub index: Option<usize>,
}

/// `kanyu_skill_run` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SkillRunReq {
    /// 已注册的技能标识（技能的 meta.name，见 kanyu_system_hotload 返回的 skill_id）。
    pub skill_id: String,
    /// 数据文件路径。
    pub path: String,
}

/// 单文件分析工具的统一输入（多数算法只需路径）。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisFileReq {
    /// 数据文件路径。
    pub path: String,
}

/// `kanyu_analysis_dissolve` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisDissolveReq {
    /// 数据文件路径。
    pub path: String,
    /// 分组字段（缺省全图融合）。
    pub field: Option<String>,
}

/// `kanyu_analysis_simplify` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisSimplifyReq {
    /// 数据文件路径。
    pub path: String,
    /// 简化容差（CRS 单位）。
    pub tolerance: f64,
}

/// `kanyu_analysis_delete_holes` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AnalysisDeleteHolesReq {
    /// 数据文件路径。
    pub path: String,
    /// 洞面积阈值（CRS 平面单位；缺省全删）。
    pub min_area: Option<f64>,
}

/// `kanyu_data_validate` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DataValidateReq {
    /// 数据文件路径（当前支持 .txt 宗地/点表格式）。
    pub path: String,
}

/// `kanyu_toolbox_run` 的图层注入项（id → 文件路径；Layer 类参数引用 id）。
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ToolboxLayerRef {
    /// 图层 id（params 中 Layer 类参数按此 id 引用）。
    pub id: String,
    /// 数据文件路径（格式自动探测，与 kanyu_data_load 同一加载器）。
    pub path: String,
}

/// `kanyu_toolbox_run` 输入。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolboxRunReq {
    /// 工具 id（见 kanyu_toolbox_list 返回的注册表）。
    pub tool_id: String,
    /// 参数值（按注册表参数序；空串或缺位 = 取参数默认值；
    /// 枚举参数取中文标签，如 "相交"）。
    #[serde(default)]
    pub params: Vec<String>,
    /// 图层注入清单（Layer 类参数引用其 id）。
    #[serde(default)]
    pub layers: Vec<ToolboxLayerRef>,
}

#[tool_router]
impl KanyuServer {
    /// 构造 Server 并注册全部工具。
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            task_manager: rmcp::task_manager::TaskManager::new(),
            skills: std::sync::Arc::new(std::sync::Mutex::new(SkillRegistryState {
                host: kanyu_skill::SkillHost::new().expect("wasmtime 引擎初始化失败"),
                skills: std::collections::HashMap::new(),
            })),
        }
    }

    /// 加载地理数据文件，返回图层概要（要素数/几何类型/字段/CRS）。
    #[tool(
        name = "kanyu_data_load",
        description = "加载地理数据文件到内存，返回图层概要（要素数、几何类型、范围、字段清单）"
    )]
    async fn data_load(
        &self,
        Parameters(req): Parameters<DataLoadReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let alias = req.alias.unwrap_or_else(|| stem_of(&req.path));
        let layer = Layer::load(&alias, &req.path).map_err(to_mcp)?;
        let mut value = serde_json::to_value(layer.summary()).map_err(to_mcp)?;
        if let Some(crs) = req.crs {
            value["crs"] = serde_json::Value::String(crs);
        }
        Ok(Json(value))
    }

    /// 对数据执行属性查询，返回 GeoJSON FeatureCollection。
    #[tool(
        name = "kanyu_data_query",
        description = "对数据文件执行属性过滤查询（如 height > 50），返回 GeoJSON FeatureCollection 与要素数"
    )]
    async fn data_query(
        &self,
        Parameters(req): Parameters<DataQueryReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = layer.query(&req.filter).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 将数据导出为目标格式。
    #[tool(
        name = "kanyu_data_export",
        description = "将数据文件导出为目标格式（当前原生支持 geojson/csv/shp/fgb/geoparquet/dxf/kml/kmz；其余格式受格式能力矩阵与驱动状态约束）"
    )]
    async fn data_export(
        &self,
        Parameters(req): Parameters<DataExportReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let registry = FormatRegistry::builtin();
        // kmz 是 kml 的 zip 容器变体（非独立格式条目）：按 kml 校验后分流。
        let caps = if req.format == "kmz" {
            registry.require("kml", "write").map_err(to_mcp)?
        } else {
            registry.require(&req.format, "write").map_err(to_mcp)?
        };
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let bytes: Vec<u8> = match caps.id {
            "geojson" => Layer::to_geojson_string(&layer.collection()).into_bytes(),
            "csv" => Layer::to_csv_string(&layer.collection())
                .map_err(to_mcp)?
                .into_bytes(),
            "fgb" => Layer::to_fgb_bytes(&layer.collection()).map_err(to_mcp)?,
            "geoparquet" => Layer::to_geoparquet_bytes(&layer.collection()).map_err(to_mcp)?,
            "dxf" => Layer::to_dxf_string(&layer.collection())
                .map_err(to_mcp)?
                .into_bytes(),
            "kml" if req.format == "kmz" => {
                Layer::to_kmz_bytes(&layer.collection()).map_err(to_mcp)?
            }
            "kdb" => layer.to_kdb_bytes().map_err(to_mcp)?,
            "txt" => {
                kanyu_core::parcel::collection_to_parcel_txt(&layer.collection(), 4, "EPSG:4326")
                    .map_err(to_mcp)?
                    .into_bytes()
            }
            "dat" => Layer::to_cass_dat_string(&layer.collection(), 3)
                .map_err(to_mcp)?
                .into_bytes(),
            "shp" => {
                // shp 为三件套（base.shp/.shx/.dbf）：不能走字节流写文件，直接落盘。
                let base = req
                    .out
                    .strip_suffix(".shp")
                    .or_else(|| req.out.strip_suffix(".SHP"))
                    .unwrap_or(&req.out);
                Layer::write_shp(&layer.collection(), base).map_err(to_mcp)?;
                return Ok(Json(serde_json::json!({
                    "exported": layer.len(),
                    "format": "shp",
                    "out": format!("{base}.shp/.shx/.dbf"),
                })));
            }
            "kml" => Layer::to_kml_string(&layer.collection())
                .map_err(to_mcp)?
                .into_bytes(),
            _ => {
                return Err(to_mcp(kanyu_core::KanyuError::UnsupportedOperation {
                    format: caps.id.to_string(),
                    operation: format!("native-export (driver {} not enabled)", caps.driver),
                }))
            }
        };
        std::fs::write(&req.out, bytes).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "exported": layer.len(),
            "format": caps.id,
            "out": req.out,
        })))
    }

    /// 缓冲区分析。
    #[tool(
        name = "kanyu_analysis_buffer",
        description = "对数据文件做缓冲区分析（distance 单位为数据 CRS 单位；EPSG:4326 下是度而非米，米制缓冲需先投影），属性随行，返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_buffer(
        &self,
        Parameters(req): Parameters<AnalysisBufferReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        analysis_buffer_sync(req).map(Json).map_err(to_mcp)
    }

    /// 叠加分析。
    #[tool(
        name = "kanyu_analysis_overlay",
        description = "叠加分析（仅 Polygon/MultiPolygon 面要素；operation ∈ union/intersection/difference/xor；逐要素对布尔、未做跨对融合），返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_overlay(
        &self,
        Parameters(req): Parameters<AnalysisOverlayReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        analysis_overlay_sync(req).map(Json).map_err(to_mcp)
    }

    /// 拓扑检查。
    #[tool(
        name = "kanyu_analysis_topology",
        description = "拓扑检查（rules 支持 no_overlap：面要素两两交集面积 > 1e-10 判违规），返回违规报告（规则、要素数、违规条数与明细）"
    )]
    async fn analysis_topology(
        &self,
        Parameters(req): Parameters<AnalysisTopologyReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        analysis_topology_sync(req).map(Json).map_err(to_mcp)
    }

    /// 投影变换。
    #[tool(
        name = "kanyu_data_reproject",
        description = "坐标投影变换（from/to 为 \"EPSG:xxxx\" 或 proj4 定义串，内置 EPSG 数据库；经纬度自动衔接度/弧度，z 不变），out 缺省返回 FeatureCollection"
    )]
    async fn data_reproject(
        &self,
        Parameters(req): Parameters<DataReprojectReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result =
            kanyu_core::crs::reproject(&layer.collection(), &req.from, &req.to).map_err(to_mcp)?;
        match req.out {
            Some(out) => {
                std::fs::write(&out, Layer::to_geojson_string(&result)).map_err(to_mcp)?;
                Ok(Json(serde_json::json!({
                    "reprojected": result.features.len(),
                    "out": out,
                })))
            }
            None => Ok(Json(serde_json::json!({
                "feature_count": result.features.len(),
                "collection": result,
            }))),
        }
    }

    /// 测地线度量。
    #[tool(
        name = "kanyu_analysis_measure",
        description = "测地线度量（Karney 2013；kind=length 长度米 / area 面积平方米；输入应为经纬度数据如 EPSG:4326），返回 total 与逐要素明细"
    )]
    async fn analysis_measure(
        &self,
        Parameters(req): Parameters<AnalysisMeasureReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let kind: kanyu_core::crs::MeasureKind = req.kind.parse().map_err(to_mcp)?;
        let report = kanyu_core::crs::measure(&layer.collection(), kind).map_err(to_mcp)?;
        Ok(Json(report))
    }

    /// 空间连接。
    #[tool(
        name = "kanyu_analysis_sjoin",
        description = "空间连接（左连接 + 匹配展开：保留全部 target 要素，一对多匹配各输出一条；属性合并、键冲突加 join_ 前缀并附 join_index；predicate ∈ intersects/contains/within），返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_sjoin(
        &self,
        Parameters(req): Parameters<AnalysisSjoinReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        analysis_sjoin_sync(req).map(Json).map_err(to_mcp)
    }

    /// 分区统计。
    #[tool(
        name = "kanyu_analysis_zonal_stats",
        description = "分区统计（values 按质心/代表点归属 zones 面要素，一值多区取首个匹配；zones 追加 {field}_{stat} 统计列；区外值计入 unzoned_count），返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_zonal_stats(
        &self,
        Parameters(req): Parameters<AnalysisZonalStatsReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        analysis_zonal_stats_sync(req).map(Json).map_err(to_mcp)
    }

    /// 离屏地图渲染。
    #[tool(
        name = "kanyu_render_map",
        description = "离屏渲染数据文件为地图图片（format=png 时 content 携带 base64 image/png，format=svg 时携带 SVG 源码文本；structuredContent 携带要素数/bbox/尺寸/主题/格式摘要；主题为晨山 light 或夜观星 dark；可选 style 属性驱动样式：{\"type\":\"graduated\",\"field\":..,\"stops\":[[阈值,\"#RRGGBB\"],..]} 数值分档或 {\"type\":\"categorical\",\"field\":..,\"colors\":{..},\"default\":..} 类别映射，缺省走主题默认样式）"
    )]
    async fn render_map(
        &self,
        Parameters(req): Parameters<RenderMapReq>,
    ) -> Result<CallToolResult, McpError> {
        use base64::Engine as _;

        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let style_rule: Option<kanyu_render::StyleRule> = match req.style {
            Some(v) => Some(serde_json::from_value(v).map_err(|e| {
                McpError::invalid_params(format!("样式规则 JSON 解析失败: {e}"), None)
            })?),
            None => None,
        };
        let opts = kanyu_render::RenderOptions {
            width: req.width.unwrap_or(800),
            height: req.height.unwrap_or(600),
            theme: req
                .theme
                .as_deref()
                .unwrap_or("light")
                .parse()
                .map_err(to_mcp)?,
            style: style_rule,
            ..Default::default()
        };
        let collection = layer.collection();
        let bbox = collection.bbox.clone().map(serde_json::Value::from);
        let summary = serde_json::json!({
            "feature_count": layer.len(),
            "bbox": bbox,
            "width": opts.width,
            "height": opts.height,
            "theme": opts.theme.name(),
            "format": req.format.to_ascii_lowercase(),
        });

        let mut result = match req.format.to_ascii_lowercase().as_str() {
            "png" => {
                let bytes = kanyu_render::render_png(&collection, &opts).map_err(to_mcp)?;
                CallToolResult::success(vec![
                    ContentBlock::image(
                        base64::engine::general_purpose::STANDARD.encode(&bytes),
                        "image/png",
                    ),
                    ContentBlock::text(summary.to_string()),
                ])
            }
            "svg" => {
                let svg = kanyu_render::render_svg(&collection, &opts).map_err(to_mcp)?;
                CallToolResult::success(vec![
                    ContentBlock::text(svg),
                    ContentBlock::text(summary.to_string()),
                ])
            }
            other => {
                return Err(McpError::invalid_params(
                    format!("未知渲染格式 '{other}'（支持 png/svg）"),
                    None,
                ));
            }
        };
        result.structured_content = Some(summary);
        Ok(result)
    }

    /// 宗地图出图（GB/T 42547 图 L.3 版式）。
    #[tool(
        name = "kanyu_render_parcel_map",
        description = "宗地图出图（GB/T 42547-2023《地籍调查规程》图 L.3 版式：界址点 Ø2.0mm 符号 + 0.3mm 红界址线 + J 点号/边长注记（勘测定界图注记契约排版，残余压盖诚实回报）+ 界址点坐标表（长表自动折列）+ 宗地号/地类编码分式 + 整百比例尺自动求解 + 「北」指北针 + 签注栏；format=png 时 content 携带 base64 image/png，format=svg 时携带 SVG 源码文本；可选 out 同时落盘；structuredContent 携带比例尺/注记数/残余压盖数）"
    )]
    async fn render_parcel_map(
        &self,
        Parameters(req): Parameters<RenderParcelMapReq>,
    ) -> Result<CallToolResult, McpError> {
        use base64::Engine as _;
        use kanyu_render::parcelmap::{
            render_parcel_map_png, render_parcel_map_svg, ParcelMapData, ParcelMapSpec,
        };

        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let (boundary, props) =
            kanyu_core::cartography::boundary_from_collection(&layer.collection(), req.index)
                .map_err(to_mcp)?;
        let prop = |keys: &[&str]| kanyu_core::cartography::feature_prop_str(&props, keys);
        let spec = ParcelMapSpec {
            parcel_code: req
                .parcel_code
                .or_else(|| prop(&["parcel_id", "ZDDM", "zddm"]))
                .unwrap_or_default(),
            owner: req
                .owner
                .or_else(|| prop(&["owner", "QLRMC", "parcel_name"]))
                .unwrap_or_default(),
            map_sheet: req
                .map_sheet
                .or_else(|| prop(&["map_sheet", "TFH"]))
                .unwrap_or_default(),
            area_sqm: req
                .area
                .or_else(|| kanyu_core::cartography::feature_prop_f64(&props, &["area", "ZDMJ"])),
            land_use: req
                .land_use
                .or_else(|| prop(&["parcel_use", "YT"]))
                .unwrap_or_default(),
            unit_name: req.unit_name.unwrap_or_default(),
            survey_note: req.survey_note.unwrap_or_default(),
            drawer: req.drawer.unwrap_or_default(),
            reviewer: req.reviewer.unwrap_or_default(),
            draw_date: req.draw_date.unwrap_or_default(),
            review_date: req.review_date.unwrap_or_default(),
            sizhi_e: req
                .sizhi_e
                .or_else(|| prop(&["ZDSZD", "zdszd"]))
                .unwrap_or_default(),
            sizhi_s: req
                .sizhi_s
                .or_else(|| prop(&["ZDSZN", "zdszn"]))
                .unwrap_or_default(),
            sizhi_w: req
                .sizhi_w
                .or_else(|| prop(&["ZDSZX", "zdszx"]))
                .unwrap_or_default(),
            sizhi_n: req
                .sizhi_n
                .or_else(|| prop(&["ZDSZB", "zdszb"]))
                .unwrap_or_default(),
            roads: match &req.roads {
                Some(path) => {
                    let road_layer = Layer::load(stem_of(path), path).map_err(to_mcp)?;
                    kanyu_render::parcelmap::roads_from_collection(
                        &road_layer.collection(),
                        &["name", "NAME", "road_name", "道路名称", "DLMC", "dlmc"],
                    )
                }
                None => Vec::new(),
            },
            scale: req.scale,
            dpi: req.dpi.unwrap_or(150.0),
            ..Default::default()
        };
        let fmt = req.format.to_ascii_lowercase();
        let output = match fmt.as_str() {
            "png" => render_parcel_map_png(&boundary, &spec).map_err(to_mcp)?,
            "svg" => render_parcel_map_svg(&boundary, &spec).map_err(to_mcp)?,
            other => {
                return Err(McpError::invalid_params(
                    format!("未知输出格式 '{other}'（支持 svg/png）"),
                    None,
                ));
            }
        };
        let overlaps = output
            .diagnostics
            .iter()
            .filter(|d| d.contains("overlap=true"))
            .count();
        let summary = serde_json::json!({
            "scale": output.scale,
            "label_count": output.diagnostics.len(),
            "overlap_count": overlaps,
            "format": fmt,
            "out": req.out,
        });
        if let Some(out) = &req.out {
            match &output.data {
                ParcelMapData::Svg(text) => std::fs::write(out, text).map_err(to_mcp)?,
                ParcelMapData::Png(bytes) => std::fs::write(out, bytes).map_err(to_mcp)?,
            }
        }
        let mut result = match &output.data {
            ParcelMapData::Png(bytes) => CallToolResult::success(vec![
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                    "image/png",
                ),
                ContentBlock::text(summary.to_string()),
            ]),
            ParcelMapData::Svg(svg) => CallToolResult::success(vec![
                ContentBlock::text(svg.clone()),
                ContentBlock::text(summary.to_string()),
            ]),
        };
        result.structured_content = Some(summary);
        Ok(result)
    }

    /// 宗地 CASS 兼容 DXF 导出（南方 CASS 联动）。
    #[tool(
        name = "kanyu_render_parcel_dxf",
        description = "宗地成果 CASS 兼容 DXF 导出（南方 CASS 联动：AC1024 DXF——ZD 宗地面 / JZX 界址线（逐边 + 边长注记，编码 302002）/ JZD 界址点（Ø2.0mm CIRCLE + 点号注记，编码 302001）/ ZJ 分式与权利人注记；编码挂 SOUTH XDATA，CASS 直接打开编辑且可被堪舆回读；注记位置经勘测定界图注记契约排版；out 必填落盘，返回界址点/界址线数与字节数）"
    )]
    async fn render_parcel_dxf(
        &self,
        Parameters(req): Parameters<RenderParcelDxfReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        use kanyu_core::cartography::{generate_boundary_lines, generate_boundary_points};

        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let (boundary, props) =
            kanyu_core::cartography::boundary_from_collection(&layer.collection(), req.index)
                .map_err(to_mcp)?;
        let prop = |keys: &[&str]| kanyu_core::cartography::feature_prop_str(&props, keys);
        let points = generate_boundary_points(&boundary, "J");
        let lines = generate_boundary_lines(&boundary, &points);
        let spec = kanyu_core::cass::CassDxfSpec {
            scale: req.scale.unwrap_or(1000),
            parcel_code: prop(&["parcel_id", "ZDDM", "zddm"]).unwrap_or_default(),
            land_use: prop(&["parcel_use", "YT"]).unwrap_or_default(),
            owner: prop(&["owner", "QLRMC", "parcel_name"]).unwrap_or_default(),
            xdata: !req.no_xdata.unwrap_or(false),
        };
        // CLI 旗标可覆盖属性拾取（MCP 侧 req 覆盖优先）
        let spec = kanyu_core::cass::CassDxfSpec {
            parcel_code: req.parcel_code.unwrap_or(spec.parcel_code),
            land_use: req.land_use.unwrap_or(spec.land_use),
            owner: req.owner.unwrap_or(spec.owner),
            ..spec
        };
        let text = kanyu_core::cass::parcel_to_cass_dxf(&boundary, &points, &lines, &spec)
            .map_err(to_mcp)?;
        let bytes = text.len();
        std::fs::write(&req.out, &text).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "out": req.out,
            "boundary_points": points.len(),
            "boundary_lines": lines.len(),
            "xdata": spec.xdata,
            "scale": spec.scale,
            "bytes": bytes,
        })))
    }

    /// 多图层打包为堪舆数据库（KDB v2）。
    #[tool(
        name = "kanyu_data_kdb_pack",
        description = "多图层打包为堪舆数据库（KDB v2 zip 容器：每输入文件成为一个命名图层，图层名=文件主干，重名报错；面向不动产登记数据库标准多表形态单文件建库——ZDJBXX/JZD/JZX… 一库全收，类型保真 RecordBatch 直通；读取侧 kanyu_data_load 取清单首图层）"
    )]
    async fn data_kdb_pack(
        &self,
        Parameters(req): Parameters<DataKdbPackReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        if req.files.is_empty() {
            return Err(McpError::invalid_params(
                "kdb-pack 至少需要一个输入文件".to_string(),
                None,
            ));
        }
        let mut layers: Vec<kanyu_core::kdb::KdbLayer> = Vec::new();
        for f in &req.files {
            let stem = stem_of(f);
            if layers.iter().any(|l| l.name == stem) {
                return Err(McpError::invalid_params(
                    format!("图层名重复（{stem}）：请重命名输入文件之一"),
                    None,
                ));
            }
            let layer = Layer::load(stem.clone(), f).map_err(to_mcp)?;
            layers.push(kanyu_core::kdb::KdbLayer {
                name: stem,
                batch: layer.batch().clone(),
            });
        }
        let summary: Vec<serde_json::Value> = layers
            .iter()
            .map(|l| {
                serde_json::json!({
                    "name": l.name,
                    "rows": l.batch.num_rows(),
                })
            })
            .collect();
        let bytes = kanyu_core::kdb::layers_to_kdb(&layers).map_err(to_mcp)?;
        std::fs::write(&req.out, &bytes).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "out": req.out,
            "format": "kdb",
            "format_version": "2",
            "layer_count": layers.len(),
            "layers": summary,
        })))
    }

    /// 宗海图件出图（宗海界址图 L.7 / 宗海位置图 L.6 / 宗海平面布置图 L.8）。
    #[tool(
        name = "kanyu_render_sea_map",
        description = "宗海图件出图（GB/T 42547-2023 图 L.6/L.7/L.8 版式，A4 横：自适应经纬网图廓（度分秒注记）+ 宗海图斑 + 红界址线 + 网格签注表 + 指北针；kind=boundary（宗海界址图，默认，含界址点编号及坐标表【北纬|东经度分秒，source_epsg 反算】与点号边长注记）/ location（宗海位置图）/ layout（宗海平面布置图）；format=png 时 content 携带 base64 image/png，format=svg 时携带 SVG 源码文本；可选 out 同时落盘；structuredContent 携带图种/比例尺/注记数/残余压盖数）"
    )]
    async fn render_sea_map(
        &self,
        Parameters(req): Parameters<RenderSeaMapReq>,
    ) -> Result<CallToolResult, McpError> {
        use base64::Engine as _;
        use kanyu_render::parcelmap::ParcelMapData;
        use kanyu_render::seamap::{
            render_sea_boundary_map_png, render_sea_boundary_map_svg, SeaBoundaryMapSpec,
            SeaMapKind,
        };

        let kind = match req.kind.as_deref().unwrap_or("boundary") {
            "boundary" => SeaMapKind::BoundaryMap,
            "location" => SeaMapKind::LocationMap,
            "layout" => SeaMapKind::LayoutMap,
            other => {
                return Err(McpError::invalid_params(
                    format!("未知图种 '{other}'（支持 boundary/location/layout）"),
                    None,
                ));
            }
        };
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let (boundary, props) =
            kanyu_core::cartography::boundary_from_collection(&layer.collection(), req.index)
                .map_err(to_mcp)?;
        let prop = |keys: &[&str]| kanyu_core::cartography::feature_prop_str(&props, keys);
        let raw_epsg = req.source_epsg.unwrap_or_else(|| "EPSG:4527".to_string());
        // 纯数字代码（如 4527）规范化为 EPSG:xxxx
        let source_epsg = if raw_epsg.chars().all(|c| c.is_ascii_digit()) {
            format!("EPSG:{raw_epsg}")
        } else {
            raw_epsg
        };
        let spec = SeaBoundaryMapSpec {
            kind,
            project_name: prop(&["project_name", "XMMC", "xmmc"]).unwrap_or_default(),
            sea_code: prop(&["sea_code", "ZHDM", "zhdm"]).unwrap_or_default(),
            source_epsg,
            survey_unit: req.survey_unit.unwrap_or_default(),
            surveyor: req.surveyor.unwrap_or_default(),
            drawer: req.drawer.unwrap_or_default(),
            draw_date: req.draw_date.unwrap_or_default(),
            inspector: req.inspector.unwrap_or_default(),
            reviewer: req.reviewer.unwrap_or_default(),
            scale: req.scale,
            dpi: req.dpi.unwrap_or(150.0),
        };
        // 参数覆盖属性拾取（req.project_name/sea_code 优先）
        let spec = SeaBoundaryMapSpec {
            project_name: req.project_name.unwrap_or(spec.project_name),
            sea_code: req.sea_code.unwrap_or(spec.sea_code),
            ..spec
        };
        let fmt = req.format.to_ascii_lowercase();
        let output = match fmt.as_str() {
            "png" => render_sea_boundary_map_png(&boundary, &spec).map_err(to_mcp)?,
            "svg" => render_sea_boundary_map_svg(&boundary, &spec).map_err(to_mcp)?,
            other => {
                return Err(McpError::invalid_params(
                    format!("未知输出格式 '{other}'（支持 svg/png）"),
                    None,
                ));
            }
        };
        let overlaps = output
            .diagnostics
            .iter()
            .filter(|d| d.contains("overlap=true"))
            .count();
        let summary = serde_json::json!({
            "kind": kind.title_suffix(),
            "scale": output.scale,
            "label_count": output.diagnostics.len(),
            "overlap_count": overlaps,
            "format": fmt,
            "out": req.out,
        });
        if let Some(out) = &req.out {
            match &output.data {
                ParcelMapData::Svg(text) => std::fs::write(out, text).map_err(to_mcp)?,
                ParcelMapData::Png(bytes) => std::fs::write(out, bytes).map_err(to_mcp)?,
            }
        }
        let mut result = match &output.data {
            ParcelMapData::Png(bytes) => CallToolResult::success(vec![
                ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                    "image/png",
                ),
                ContentBlock::text(summary.to_string()),
            ]),
            ParcelMapData::Svg(svg) => CallToolResult::success(vec![
                ContentBlock::text(svg.clone()),
                ContentBlock::text(summary.to_string()),
            ]),
        };
        result.structured_content = Some(summary);
        Ok(result)
    }

    /// 系统自省：架构、模块、格式矩阵、工具清单。
    #[tool(
        name = "kanyu_system_introspect",
        description = "系统自省：返回堪舆内核版本、模块清单、格式能力矩阵与 MCP 工具清单（AI 读取自身）"
    )]
    async fn system_introspect(&self) -> Result<Json<serde_json::Value>, McpError> {
        Ok(Json(
            serde_json::to_value(introspect::report()).map_err(to_mcp)?,
        ))
    }

    /// 热加载 WASM 技能。
    #[tool(
        name = "kanyu_system_hotload",
        description = "热加载 WASM 技能到内存注册表：编译校验 + 实例化 + 元数据校验（wasmtime 沙箱，无 WASI 导入纯计算 + fuel 配额；校验失败绝不注册），返回 skill_id 与元数据；重名覆盖旧注册"
    )]
    async fn system_hotload(
        &self,
        Parameters(req): Parameters<SystemHotloadReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let mut state = self.skills.lock().map_err(|_| to_mcp("技能注册表锁中毒"))?;
        let skill = state.host.load(&req.wasm_path).map_err(to_mcp)?;
        let skill_id = skill.meta().name.clone();
        let meta = skill.meta().clone();
        // 重名覆盖旧注册（调用方经 replaced 感知）。
        let replaced = state.skills.insert(skill_id.clone(), skill).is_some();
        Ok(Json(serde_json::json!({
            "skill_id": skill_id,
            "replaced": replaced,
            "meta": meta,
        })))
    }

    /// 列出已注册技能。
    #[tool(
        name = "kanyu_skill_list",
        description = "列出内存注册表中的全部 WASM 技能（skill_id/version/capabilities 快照；重启即丢，需重新 hotload）"
    )]
    async fn skill_list(&self) -> Result<Json<serde_json::Value>, McpError> {
        let state = self.skills.lock().map_err(|_| to_mcp("技能注册表锁中毒"))?;
        let mut skills: Vec<serde_json::Value> = state
            .skills
            .values()
            .map(|g| {
                serde_json::json!({
                    "skill_id": g.meta().name,
                    "version": g.meta().version,
                    "capabilities": g.meta().capabilities,
                })
            })
            .collect();
        skills.sort_by(|a, b| a["skill_id"].to_string().cmp(&b["skill_id"].to_string()));
        Ok(Json(serde_json::json!({ "skills": skills })))
    }

    /// 执行已注册技能。
    #[tool(
        name = "kanyu_skill_run",
        description = "对已注册技能在数据文件上沙箱执行（FeatureCollection 进/出；fuel 配额 10 亿；未知 skill_id 报错提示先 kanyu_system_hotload；arguments 带 task:true 可异步执行），返回 GeoJSON FeatureCollection"
    )]
    async fn skill_run(
        &self,
        Parameters(req): Parameters<SkillRunReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let mut state = self.skills.lock().map_err(|_| to_mcp("技能注册表锁中毒"))?;
        skill_run_sync(&mut state, req).map(Json).map_err(to_mcp)
    }

    /// 校验 AGENTS.md 项目语义文件完整性。
    #[tool(
        name = "kanyu_agents_validate",
        description = "校验 AGENTS.md 项目语义文件：元数据（name/crs）、图层语义、业务规则完整性。校验契约：零参自动裁决（AGENTS.md 的 data-layer 元数据行优先，未声明时回退 crs 占位）；ctx=code-repo 钉死软件仓库语境（免检数据层）、ctx=geo 钉死地理项目（数据层必填）"
    )]
    async fn agents_validate(
        &self,
        Parameters(req): Parameters<AgentsValidateReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let doc = agents::load(&req.path).map_err(to_mcp)?;
        // 与 AGENTS.md「校验契约」零参 `validate` 一致：由文档自裁决——
        // 显式 `- **data-layer**: 是/否` 元数据行最高优先；未声明时回退 crs
        // 占位（真实编码 → 地理、必填；`不适用`/`N/A` 或缺失 → 代码仓库、免检）。
        // 调用方再传 `ctx`：`code-repo`/`code_repo` → 钉死软件仓库（免检）；
        // `geo` → 钉死地理项目（必填）。
        let (validate_code_repo, ctx_note) = match req.ctx.as_deref() {
            Some(code)
                if code.eq_ignore_ascii_case("code-repo")
                    || code.eq_ignore_ascii_case("code_repo") =>
            {
                (
                    true,
                    "code-repo → 软件仓库语境（数据层免检，校验契约钉死）".to_string(),
                )
            }
            Some("geo") => (
                false,
                "geo → 地理项目（数据层语义表必填，校验契约钉死）".to_string(),
            ),
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!("无法识别的校验上下文模式 {other:?}（支持缺省/code-repo/geo）"),
                    None,
                ));
            }
            None => (
                false,
                "auto（零参：由 AGENTS.md 自身按 data-layer 元数据 + crs 占位自动裁决）"
                    .to_string(),
            ),
        };
        let pin = if validate_code_repo {
            Some(false)
        } else {
            None
        };
        let issues = doc.validate(pin);
        Ok(Json(serde_json::json!({
            "valid": issues.is_empty(),
            "issues": issues,
            "context": ctx_note,
        })))
    }

    /// 生成 AGENTS.md 项目语义模板。
    #[tool(
        name = "kanyu_agents_init",
        description = "在指定项目目录生成 AGENTS.md 语义模板（图层/CRS/业务规则，供 AI 理解项目）"
    )]
    async fn agents_init(
        &self,
        Parameters(req): Parameters<AgentsInitReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let dir = std::path::Path::new(&req.project);
        let name = req.name.unwrap_or_else(|| {
            dir.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("kanyu-project")
                .to_string()
        });
        let path = dir.join("AGENTS.md");
        if path.exists() {
            return Err(McpError::invalid_params(
                format!("{} 已存在", path.display()),
                None,
            ));
        }
        std::fs::create_dir_all(dir).map_err(to_mcp)?;
        // template 的 `is_geo`：仅当显式指定 code-repo（或 code_repo）时 → false
        // （免数据层语义表）；geo / 空 / 未指定 / 无法识别 → 一律缺省按地理
        // 项目处理（true，含语义表）。
        let is_geo = !matches!(
            req.ctx.as_deref(),
            Some(code) if code.eq_ignore_ascii_case("code-repo") || code.eq_ignore_ascii_case("code_repo")
        );
        let crs = req.crs.unwrap_or_else(|| "EPSG:4326".to_string());
        std::fs::write(&path, agents::template(&name, &crs, is_geo)).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "created": path.display().to_string(),
            "template": if is_geo { "geo" } else { "code-repo" },
        })))
    }

    /// 融合（QGIS Dissolve 移植）。
    #[tool(
        name = "kanyu_analysis_dissolve",
        description = "融合（QGIS Dissolve）：按字段分组对面要素做布尔并集；属性取组字段值+组内首要素其余属性。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_dissolve(
        &self,
        Parameters(req): Parameters<AnalysisDissolveReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = kanyu_core::geoprocess::dissolve(&layer.collection(), req.field.as_deref())
            .map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 道格拉斯简化（QGIS Simplify 移植）。
    #[tool(
        name = "kanyu_analysis_simplify",
        description = "道格拉斯-普克简化（QGIS Simplify）：tolerance 为 CRS 单位；简化后退化要素剔除。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_simplify(
        &self,
        Parameters(req): Parameters<AnalysisSimplifyReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result =
            kanyu_core::geoprocess::simplify(&layer.collection(), req.tolerance).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 质心（QGIS Centroids 移植）。
    #[tool(
        name = "kanyu_analysis_centroid",
        description = "质心提取（QGIS Centroids）：逐要素质心点，属性随行。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_centroid(
        &self,
        Parameters(req): Parameters<AnalysisFileReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = kanyu_core::geoprocess::centroid(&layer.collection()).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 凸包（QGIS Convex hull 移植）。
    #[tool(
        name = "kanyu_analysis_convex_hull",
        description = "凸包（QGIS Convex hull）：逐要素凸包面。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_convex_hull(
        &self,
        Parameters(req): Parameters<AnalysisFileReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = kanyu_core::geoprocess::convex_hull(&layer.collection()).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 删洞（QGIS Delete holes 移植）。
    #[tool(
        name = "kanyu_analysis_delete_holes",
        description = "删洞（QGIS Delete holes）：min_area 缺省删全部洞，否则仅删面积 < min_area 的洞（CRS 平面单位）。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_delete_holes(
        &self,
        Parameters(req): Parameters<AnalysisDeleteHolesReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = kanyu_core::geoprocess::delete_holes(&layer.collection(), req.min_area)
            .map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 多部件炸开（QGIS Multipart to singleparts 移植）。
    #[tool(
        name = "kanyu_analysis_explode",
        description = "多部件炸开（QGIS Multipart to singleparts）：Multi* → 单部件逐要素，属性复制。返回 GeoJSON FeatureCollection"
    )]
    async fn analysis_explode(
        &self,
        Parameters(req): Parameters<AnalysisFileReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let result = kanyu_core::geoprocess::explode(&layer.collection()).map_err(to_mcp)?;
        Ok(Json(serde_json::json!({
            "feature_count": result.features.len(),
            "collection": result,
        })))
    }

    /// 图层统计（选择集统计移植）。
    #[tool(
        name = "kanyu_analysis_stats",
        description = "图层统计（测地线口径）：要素计数按几何类型 + 总长度（米/千米）+ 总面积（平方米/公顷/亩/平方千米）+ 总周长"
    )]
    async fn analysis_stats(
        &self,
        Parameters(req): Parameters<AnalysisFileReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let report = kanyu_core::geoprocess::stats(&layer.collection()).map_err(to_mcp)?;
        Ok(Json(serde_json::to_value(report).map_err(to_mcp)?))
    }

    /// 数据质检（宗地 TXT）。
    #[tool(
        name = "kanyu_data_validate",
        description = "数据质检（宗地 TXT：表头必备项缺失/空值、中文逗号、空格、闭合环与点数规则），返回问题清单（空=通过）"
    )]
    async fn data_validate(
        &self,
        Parameters(req): Parameters<DataValidateReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let text = std::fs::read_to_string(&req.path).map_err(to_mcp)?;
        let issues = kanyu_core::parcel::validate_parcel_txt(&text);
        Ok(Json(serde_json::json!({
            "valid": issues.iter().all(|i| i.level != "错误"),
            "issue_count": issues.len(),
            "issues": issues,
        })))
    }

    /// 工具箱注册表（tooldef 单一事实来源投影）。
    #[tool(
        name = "kanyu_toolbox_list",
        description = "列出内核工具箱注册表（37 个工具：id/中文名/分类/参数表/是否报告类；与壳层工具箱同一 tooldef 注册表，供 AI 代理发现工具面）"
    )]
    async fn toolbox_list(&self) -> Result<Json<serde_json::Value>, McpError> {
        Ok(Json(serde_json::json!({
            "count": tooldef::TOOLS.len(),
            "tools": serde_json::to_value(tooldef::TOOLS).map_err(to_mcp)?,
        })))
    }

    /// 工具箱统一执行。
    #[tool(
        name = "kanyu_toolbox_run",
        description = "按注册表统一执行工具箱工具：tool_id 见 kanyu_toolbox_list；params 按注册表参数序（空串或缺位取参数默认值，枚举参数取中文标签如 \"相交\"）；layers 为 [{id, path}] 图层注入清单（Layer 类参数引用 id）。产出新图层类工具返回 {\"type\":\"new_layer\"|\"new_layers\",\"verb\",\"layers\":{名称: GeoJSON}}（调用方命名落层），报告类返回 {\"type\":\"report\",\"report\":文本}；arguments 带 task:true 可异步执行"
    )]
    async fn toolbox_run(
        &self,
        Parameters(req): Parameters<ToolboxRunReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        toolbox_run_sync(req).map(Json).map_err(to_mcp)
    }
}

#[tool_handler]
impl ServerHandler for KanyuServer {
    /// tools/call 分发：白名单分析工具（`TASK_ELIGIBLE`）的 arguments 带
    /// `"task": true` 时按 SEP-2663 任务化执行（rmcp TaskManager spawn，
    /// 客户端须已声明 tasks 扩展能力，否则路由层拒绝 CreateTaskResult）；
    /// 其余走同步工具路由。
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let wants_task = request
            .arguments
            .as_ref()
            .and_then(|a| a.get("task"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if wants_task {
            let name = request.name.to_string();
            if !TASK_ELIGIBLE.contains(&name.as_str()) {
                return Err(McpError::invalid_params(
                    format!(
                        "工具 '{name}' 不支持任务化执行（task:true 仅支持 {}）",
                        TASK_ELIGIBLE.join("/")
                    ),
                    None,
                ));
            }
            let mut args = request.arguments.clone().unwrap_or_default();
            args.remove("task");
            let task = if name == "kanyu_skill_run" {
                // 技能执行需要注册表（call_analysis_sync 为无状态分发，不含此臂）。
                spawn_skill_run_task(&self.task_manager, self.skills.clone(), args)
            } else {
                spawn_analysis_task(&self.task_manager, name, args)
            };
            return Ok(CallToolResponse::Task(CreateTaskResult::new(task)));
        }

        // 同步路径（与 #[tool_router] 生成分发一致）。
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    /// SEP-2663 `tasks/get`：委托 TaskManager（含 TTL 惰性清扫）。
    fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetTaskResult, McpError>> + MaybeSendFuture + '_
    {
        let _ = context;
        let result = self.task_manager.get_task(&request.task_id);
        async move { result.map(GetTaskResult::new) }
    }

    /// SEP-2663 `tasks/update`：委托 TaskManager（本批任务无输入请求，仅透传）。
    fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), McpError>> + MaybeSendFuture + '_ {
        let _ = context;
        let result = self
            .task_manager
            .update_task(&request.task_id, request.input_responses);
        async move { result }
    }

    /// SEP-2663 `tasks/cancel`：委托 TaskManager（协作取消）。
    fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<(), McpError>> + MaybeSendFuture + '_ {
        let _ = context;
        let result = self.task_manager.cancel_task(&request.task_id);
        async move { result }
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_instructions(
            "堪舆 (Kanyu) GIS 内核：data/agents/analysis/render/system/toolbox 工具组 + SEP-2663 长任务；\
                 只读资源（kanyu://formats、kanyu://tools、kanyu://crs/{code}）与中文分析流 prompts。\
                 所有结果为结构化 JSON 并携带 CRS/单位元数据；不提供任意代码执行。",
        );
        info.server_info.name = "kanyu-mcp".to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info
    }

    /// MCP resources 清单（静态资源；crs 走模板，见 list_resource_templates）。
    fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, McpError>> + MaybeSendFuture + '_
    {
        let _ = (request, context);
        std::future::ready(Ok(ListResourcesResult {
            resources: vec![
                Resource::new("kanyu://formats", "kanyu_formats")
                    .with_description("格式注册表能力矩阵（JSON）")
                    .with_mime_type("application/json"),
                Resource::new("kanyu://tools", "kanyu_tools")
                    .with_description("MCP 工具清单（introspect 单一事实来源，JSON）")
                    .with_mime_type("application/json"),
            ],
            ..Default::default()
        }))
    }

    /// MCP resources 模板清单（kanyu://crs/{code}）。
    fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, McpError>>
           + MaybeSendFuture
           + '_ {
        let _ = (request, context);
        std::future::ready(Ok(ListResourceTemplatesResult {
            resource_templates: vec![ResourceTemplate::new("kanyu://crs/{code}", "kanyu_crs")
                .with_description("EPSG 条目信息（代码/名称/类型/单位/proj4 定义，JSON）")
                .with_mime_type("application/json")],
            ..Default::default()
        }))
    }

    /// MCP resources 读取。
    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResponse, McpError>> + MaybeSendFuture + '_
    {
        let _ = context;
        let result = read_resource_sync(&request.uri);
        async move { result }
    }

    /// MCP prompts 清单（中文分析流模板）。
    fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListPromptsResult, McpError>> + MaybeSendFuture + '_
    {
        let _ = (request, context);
        std::future::ready(Ok(list_prompts_sync()))
    }

    /// MCP prompts 获取（参数经 arguments 模板替换）。
    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<GetPromptResponse, McpError>> + MaybeSendFuture + '_
    {
        let _ = context;
        let result = get_prompt_sync(&request.name, request.arguments.as_ref());
        async move { result }
    }
}

impl Default for KanyuServer {
    fn default() -> Self {
        Self::new()
    }
}

/// 以 stdio 传输启动 MCP Server（阻塞直至会话结束）。
pub fn serve_stdio() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let service = KanyuServer::new().serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}

/// 以 streamable HTTP 传输启动 MCP Server（阻塞）。
///
/// 绑定 `127.0.0.1:{port}`，endpoint 为 `/mcp`（rmcp tower service 按
/// HTTP 方法分派 POST/GET/DELETE，不区分路径）。KanyuServer 无状态，
/// 按 rmcp service factory 模式每会话一个实例（LocalSessionManager
/// 内存会话存储）。
///
/// ⚠️ 安全边界：无鉴权/TLS（📋），远程暴露请自行加反向代理与鉴权；
/// MCP tasks（SEP-1686 长任务）📋。
pub fn serve_http(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let service = StreamableHttpService::new(
            || Ok(KanyuServer::new()),
            std::sync::Arc::new(LocalSessionManager::default()),
            StreamableHttpServerConfig::default(),
        );
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        eprintln!(
            "kanyu-mcp streamable HTTP 监听 http://127.0.0.1:{port}/mcp \
             （⚠️ 无鉴权/TLS，远程暴露请自行加反代；Ctrl-C 停止）"
        );
        loop {
            let (stream, _) = listener.accept().await?;
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = hyper_util::service::TowerToHyperService::new(service.clone());
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    })
}

/// 内核错误 → MCP 错误。
fn to_mcp(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

// ===== 分析工具同步实现（MCP 工具薄壳与 SEP-2663 任务化路径共享） =====

fn analysis_buffer_sync(req: AnalysisBufferReq) -> kanyu_core::Result<serde_json::Value> {
    let layer = Layer::load(stem_of(&req.path), &req.path)?;
    let result =
        kanyu_core::analysis::buffer(&layer.collection(), req.distance, req.segments.unwrap_or(8))?;
    let skipped = result
        .foreign_members
        .as_ref()
        .and_then(|m| m.get("skipped"))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::from(0));
    Ok(serde_json::json!({
        "feature_count": result.features.len(),
        "skipped": skipped,
        "collection": result,
    }))
}

fn analysis_overlay_sync(req: AnalysisOverlayReq) -> kanyu_core::Result<serde_json::Value> {
    let target = Layer::load(stem_of(&req.target), &req.target)?;
    let overlay_layer = Layer::load(stem_of(&req.overlay), &req.overlay)?;
    let op: kanyu_core::analysis::OverlayOp = req.operation.parse()?;
    let result =
        kanyu_core::analysis::overlay(&target.collection(), &overlay_layer.collection(), op)?;
    Ok(serde_json::json!({
        "feature_count": result.features.len(),
        "collection": result,
    }))
}

fn analysis_sjoin_sync(req: AnalysisSjoinReq) -> kanyu_core::Result<serde_json::Value> {
    let target = Layer::load(stem_of(&req.target), &req.target)?;
    let join = Layer::load(stem_of(&req.join), &req.join)?;
    let predicate: kanyu_core::analysis::SpatialPredicate = req.predicate.parse()?;
    let result = kanyu_core::analysis::sjoin(&target.collection(), &join.collection(), predicate)?;
    Ok(serde_json::json!({
        "feature_count": result.features.len(),
        "collection": result,
    }))
}

fn analysis_zonal_stats_sync(req: AnalysisZonalStatsReq) -> kanyu_core::Result<serde_json::Value> {
    let zones = Layer::load(stem_of(&req.zones), &req.zones)?;
    let values = Layer::load(stem_of(&req.values), &req.values)?;
    let stats: Vec<kanyu_core::analysis::ZonalStat> = req
        .stats
        .iter()
        .map(|s| s.parse())
        .collect::<std::result::Result<_, _>>()?;
    let result = kanyu_core::analysis::zonal_stats(
        &zones.collection(),
        &values.collection(),
        &req.field,
        &stats,
    )?;
    let unzoned = result
        .foreign_members
        .as_ref()
        .and_then(|m| m.get("unzoned_count"))
        .cloned()
        .unwrap_or_else(|| serde_json::Value::from(0));
    Ok(serde_json::json!({
        "feature_count": result.features.len(),
        "unzoned_count": unzoned,
        "collection": result,
    }))
}

fn analysis_topology_sync(req: AnalysisTopologyReq) -> kanyu_core::Result<serde_json::Value> {
    let layer = Layer::load(stem_of(&req.path), &req.path)?;
    let rules: Vec<kanyu_core::analysis::TopologyRule> = req
        .rules
        .iter()
        .map(|s| s.parse())
        .collect::<std::result::Result<_, _>>()?;
    let report = kanyu_core::analysis::topology_check(&layer.collection(), &rules)?;
    serde_json::to_value(report).map_err(|e| KanyuError::Other(format!("报告序列化失败: {e}")))
}

/// 工具箱统一执行（MCP 薄壳与任务化路径共享）：图层注入走文件路径加载
/// （与 kanyu_data_load 同一加载器），参数按注册表序对齐（空串/缺位取
/// 默认值），产出 ToolOutcome 结算为结构化 JSON。
fn toolbox_run_sync(req: ToolboxRunReq) -> kanyu_core::Result<serde_json::Value> {
    let def = tooldef::find(&req.tool_id)
        .ok_or_else(|| KanyuError::Other(format!("未知工具: {}", req.tool_id)))?;
    if req.params.len() > def.params.len() {
        return Err(KanyuError::Other(format!(
            "参数个数超出注册表（{} 个值 / {} 个参数）",
            req.params.len(),
            def.params.len()
        )));
    }
    // 注册表参数序 → values：空串或缺位取参数默认值。
    let values: Vec<String> = def
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| match req.params.get(i) {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => p.default.to_string(),
        })
        .collect();
    // 图层注入：id → FeatureCollection（文件路径加载，格式自动探测）。
    let mut collections = std::collections::HashMap::new();
    for lref in &req.layers {
        let layer = Layer::load(&lref.id, &lref.path)?;
        collections.insert(lref.id.clone(), layer.collection());
    }
    let outcome = toolrun::run_tool(&req.tool_id, &values, |id| collections.get(id).cloned())
        .map_err(KanyuError::Other)?;
    match outcome {
        toolrun::ToolOutcome::NewLayer {
            collection,
            base,
            verb,
        } => Ok(serde_json::json!({
            "type": "new_layer",
            "verb": verb,
            "layers": { base: collection },
        })),
        toolrun::ToolOutcome::NewLayers { layers, verb } => {
            let map: serde_json::Map<String, serde_json::Value> = layers
                .into_iter()
                .map(|(name, c)| {
                    (
                        name,
                        serde_json::to_value(&c).unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect();
            Ok(serde_json::json!({
                "type": "new_layers",
                "verb": verb,
                "layers": map,
            }))
        }
        toolrun::ToolOutcome::Report(text) => Ok(serde_json::json!({
            "type": "report",
            "report": text,
        })),
    }
}

/// 技能执行（MCP 薄壳与任务化路径共享；持注册表锁内执行——v0.1 串行化，
/// 见 KanyuServer.skills 注释）。
fn skill_run_sync(
    state: &mut SkillRegistryState,
    req: SkillRunReq,
) -> kanyu_core::Result<serde_json::Value> {
    let skill = state.skills.get(&req.skill_id).ok_or_else(|| {
        KanyuError::Other(format!(
            "技能 '{}' 未注册（先用 kanyu_system_hotload 加载，或 kanyu_skill_list 查看已注册技能）",
            req.skill_id
        ))
    })?;
    let layer = Layer::load(stem_of(&req.path), &req.path)?;
    let result = state
        .host
        .run(skill, &layer.collection())
        .map_err(|e| KanyuError::Other(e.to_string()))?;
    Ok(serde_json::json!({
        "feature_count": result.features.len(),
        "collection": result,
    }))
}

/// 任务化路径的同步分发（与同名 MCP 工具共享 sync 实现）。
fn call_analysis_sync(
    name: &str,
    args: serde_json::Map<String, serde_json::Value>,
) -> kanyu_core::Result<serde_json::Value> {
    let v = serde_json::Value::Object(args);
    match name {
        "kanyu_analysis_buffer" => analysis_buffer_sync(from_args(v)?),
        "kanyu_analysis_overlay" => analysis_overlay_sync(from_args(v)?),
        "kanyu_analysis_sjoin" => analysis_sjoin_sync(from_args(v)?),
        "kanyu_analysis_zonal_stats" => analysis_zonal_stats_sync(from_args(v)?),
        "kanyu_analysis_topology" => analysis_topology_sync(from_args(v)?),
        "kanyu_toolbox_run" => toolbox_run_sync(from_args(v)?),
        other => Err(KanyuError::Other(format!(
            "工具 '{other}' 不支持任务化执行"
        ))),
    }
}

fn from_args<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> kanyu_core::Result<T> {
    serde_json::from_value(v).map_err(|e| KanyuError::Other(format!("参数解析失败: {e}")))
}

/// spawn 一个分析任务（call_tool 任务化路径与测试共享）：
/// 内核调用是阻塞 CPU 工作，进 blocking 线程池，不占用
/// current_thread runtime（stdio）的调度线程。
fn spawn_analysis_task(
    manager: &rmcp::task_manager::TaskManager,
    name: String,
    args: serde_json::Map<String, serde_json::Value>,
) -> Task {
    use rmcp::task_manager::{TaskExit, TaskOptions};
    manager.spawn(
        TaskOptions::new().with_ttl_ms(Some(TASK_TTL_MS)),
        move |_ctx| {
            Box::pin(async move {
                match tokio::task::spawn_blocking(move || call_analysis_sync(&name, args)).await {
                    Ok(Ok(value)) => Ok(call_tool_result_from_json(value)),
                    Ok(Err(e)) => Err(TaskExit::Error(McpError::internal_error(
                        e.to_string(),
                        None,
                    ))),
                    Err(join_err) => Err(TaskExit::Error(McpError::internal_error(
                        format!("任务执行线程失败: {join_err}"),
                        None,
                    ))),
                }
            })
        },
    )
}

/// spawn 一个技能执行任务（call_tool 任务化路径专用；与 spawn_analysis_task
/// 同构，但经注册表执行——阻塞在持锁的 skill_run_sync 上）。
fn spawn_skill_run_task(
    manager: &rmcp::task_manager::TaskManager,
    skills: GeneRegistry,
    args: serde_json::Map<String, serde_json::Value>,
) -> Task {
    use rmcp::task_manager::{TaskExit, TaskOptions};
    manager.spawn(
        TaskOptions::new().with_ttl_ms(Some(TASK_TTL_MS)),
        move |_ctx| {
            Box::pin(async move {
                match tokio::task::spawn_blocking(move || {
                    let req: SkillRunReq = from_args(serde_json::Value::Object(args))?;
                    let mut state = skills
                        .lock()
                        .map_err(|_| KanyuError::Other("技能注册表锁中毒".to_string()))?;
                    skill_run_sync(&mut state, req)
                })
                .await
                {
                    Ok(Ok(value)) => Ok(call_tool_result_from_json(value)),
                    Ok(Err(e)) => Err(TaskExit::Error(McpError::internal_error(
                        e.to_string(),
                        None,
                    ))),
                    Err(join_err) => Err(TaskExit::Error(McpError::internal_error(
                        format!("任务执行线程失败: {join_err}"),
                        None,
                    ))),
                }
            })
        },
    )
}

/// 任务结果包装：structuredContent + content[0].text 双通道（与同步路径
/// 经 `Json<T>` 包装的形状一致）。
fn call_tool_result_from_json(value: serde_json::Value) -> CallToolResult {
    let mut result = CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
    if value.is_object() {
        result.structured_content = Some(value);
    }
    result
}

/// 取文件名主干作为默认图层名。
fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string()
}

// ===== MCP resources（只读资源） =====
//
// URI 面：kanyu://formats（格式注册表）、kanyu://tools（工具清单）、
// kanyu://crs/{code}（EPSG 条目，模板）。
// `kanyu://layer/{path}` 本轮**暂缓**：文件路径入 URI 需要百分号编码与
// 路径穿越约束（授权根外拒绝），其安全权衡待与资源订阅（subscribe）
// 一并裁决；图层数据当前经工具参数（path）通道已完备。

/// resources/read 同步实现（handler 薄壳与测试共享）。
fn read_resource_sync(uri: &str) -> Result<ReadResourceResponse, McpError> {
    let json_text = |v: serde_json::Value| -> ResourceContents {
        ResourceContents::text(v.to_string(), uri).with_mime_type("application/json")
    };
    let unknown = || {
        McpError::invalid_params(
            format!("未知资源 URI: '{uri}'（支持 kanyu://formats、kanyu://tools、kanyu://crs/{{code}}）"),
            None,
        )
    };
    if uri == "kanyu://formats" {
        let registry = FormatRegistry::builtin();
        let value = serde_json::to_value(registry.all())
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        return Ok(ReadResourceResult::new(vec![json_text(value)]).into());
    }
    if uri == "kanyu://tools" {
        let value = serde_json::to_value(introspect::tools())
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        return Ok(ReadResourceResult::new(vec![json_text(value)]).into());
    }
    if let Some(code) = uri.strip_prefix("kanyu://crs/") {
        let code: u32 = code
            .parse()
            .map_err(|_| McpError::invalid_params(format!("CRS 代码须为数值: '{code}'"), None))?;
        let info = kanyu_core::crs::crs_info(code).ok_or_else(|| {
            McpError::invalid_params(format!("EPSG:{code} 不在内置库（7507 条）",), None)
        })?;
        let value = serde_json::json!({
            "code": info.code,
            "name": info.name,
            "kind": info.kind,
            "unit": info.unit,
            "proj4": kanyu_core::crs::crs_proj4_def(code),
        });
        return Ok(ReadResourceResult::new(vec![json_text(value)]).into());
    }
    Err(unknown())
}

// ===== MCP prompts（中文分析流模板） =====

/// prompt 声明（名称/描述/参数表/消息模板）。
struct PromptDef {
    name: &'static str,
    description: &'static str,
    /// (参数名, 描述, 是否必填)。
    args: &'static [(&'static str, &'static str, bool)],
    /// 消息模板（`{参数名}` 占位经 arguments 替换）。
    template: &'static str,
}

/// 分析流模板注册表（单一事实来源；新增 prompt 在此加一行）。
const PROMPTS: &[PromptDef] = &[
    PromptDef {
        name: "data_health_check",
        description: "数据体检：加载 → 图层概要 → 拓扑检查 → 图层统计 的完整编排",
        args: &[("path", "数据文件路径", true)],
        template: "请对数据文件 {path} 做数据体检，按序调用：\n\
                   1. kanyu_data_load（path={path}）——取图层概要（要素数/几何类型/字段）；\n\
                   2. kanyu_analysis_topology（path={path}，rules=[\"no_overlap\"]）——面重叠违规检查；\n\
                   3. kanyu_analysis_stats（path={path}）——测地线统计（长度/面积/亩/公顷）；\n\
                   4. 若为宗地 TXT，再调 kanyu_data_validate 做格式质检。\n\
                   汇总为中文体检报告（异常项优先列出）。",
    },
    PromptDef {
        name: "buffer_analysis",
        description: "缓冲区分析流：坐标系判断 →（经纬度先投影）→ 缓冲 → 导出",
        args: &[("path", "数据文件路径", true), ("distance", "缓冲距离（米）", true)],
        template: "请对 {path} 做 {distance} 米缓冲区分析，按序调用：\n\
                   1. kanyu_data_load（path={path}）——确认几何类型与坐标系；\n\
                   2. 若为经纬度（如 EPSG:4326/4490），先 kanyu_data_reproject 投影到米制 CRS\n\
                      （如 EPSG:4526/4527 高斯克吕格带，可经 resources kanyu://crs/{{code}} 查带号）；\n\
                   3. kanyu_analysis_buffer（distance={distance}，米制 CRS 下距离即米）；\n\
                   4. kanyu_data_export 导出结果（geojson/fgb）。\n\
                   注意：经纬度下直接以度为距离单位是常见错误，必须先投影。",
    },
    PromptDef {
        name: "crs_transform",
        description: "坐标系转换流：定义校验 → 投影变换 → 结果抽验",
        args: &[
            ("path", "数据文件路径", true),
            ("from", "源 CRS（如 EPSG:4326）", true),
            ("to", "目标 CRS（如 EPSG:4527）", true),
        ],
        template: "请把 {path} 从 {from} 转换到 {to}，按序调用：\n\
                   1. 先经 resources 读取 kanyu://crs/{to} 确认目标定义（名称/单位/proj4）；\n\
                   2. kanyu_data_reproject（path={path}，from={from}，to={to}，out 指定输出路径）；\n\
                   3. kanyu_data_load（输出路径）核对要素数与坐标量级\n\
                      （度→米转换后坐标应为米级大数，仍在 0–180 说明 from 声明有误）。\n\
                   轴序约定：本内核一律 GIS 序（经度在前），4490 与 4326 同点转换差异 < 1mm。",
    },
];

/// prompts/list 同步实现。
fn list_prompts_sync() -> ListPromptsResult {
    ListPromptsResult {
        prompts: PROMPTS
            .iter()
            .map(|p| {
                Prompt::new(
                    p.name,
                    Some(p.description),
                    Some(
                        p.args
                            .iter()
                            .map(|(name, desc, required)| {
                                PromptArgument::new(*name)
                                    .with_description(*desc)
                                    .with_required(*required)
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
        ..Default::default()
    }
}

/// prompts/get 同步实现：必填参数齐套模板（`{name}` 占位替换）。
fn get_prompt_sync(
    name: &str,
    arguments: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Result<GetPromptResponse, McpError> {
    let Some(def) = PROMPTS.iter().find(|p| p.name == name) else {
        return Err(McpError::invalid_params(
            format!(
                "未知 prompt: '{name}'（支持 {}）",
                PROMPTS.iter().map(|p| p.name).collect::<Vec<_>>().join("/")
            ),
            None,
        ));
    };
    let mut text = def.template.to_string();
    for (arg, desc, required) in def.args {
        let value = arguments
            .and_then(|a| a.get(*arg))
            .and_then(|v| v.as_str().map(str::to_string).or(Some(v.to_string())))
            .filter(|s| !s.trim().is_empty());
        match value {
            Some(v) => text = text.replace(&format!("{{{arg}}}"), &v),
            None if *required => {
                return Err(McpError::invalid_params(
                    format!("prompt '{name}' 缺少必填参数 '{arg}'（{desc}）"),
                    None,
                ))
            }
            None => {}
        }
    }
    let message = PromptMessage::new_text(Role::User, text);
    Ok(GetPromptResult::new(vec![message])
        .with_description(def.description)
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写出 zonal 测试数据（1 区 2 点），返回 (zones, values) 路径。
    fn write_zonal_fixtures(dir: &std::path::Path) -> (String, String) {
        std::fs::create_dir_all(dir).unwrap();
        let zones = dir.join("zones.geojson");
        std::fs::write(
            &zones,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                    [[0,0],[0,4],[4,4],[4,0],[0,0]]]},"properties":{"name":"z1"}}
            ]}"#,
        )
        .unwrap();
        let values = dir.join("values.geojson");
        std::fs::write(
            &values,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
                 "properties":{"height":10}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[2,2]},
                 "properties":{"height":30}}
            ]}"#,
        )
        .unwrap();
        (
            zones.to_str().unwrap().to_string(),
            values.to_str().unwrap().to_string(),
        )
    }

    #[test]
    fn call_analysis_sync_matches_sync_tool_result() {
        let dir = std::env::temp_dir().join("kanyu_mcp_task_sync");
        let (zones, values) = write_zonal_fixtures(&dir);
        let args = serde_json::json!({
            "zones": zones,
            "values": values,
            "field": "height",
            "stats": ["count", "mean"],
        })
        .as_object()
        .unwrap()
        .clone();
        let out = call_analysis_sync("kanyu_analysis_zonal_stats", args).unwrap();
        assert_eq!(out["feature_count"], 1);
        assert_eq!(out["unzoned_count"], 0);
        assert_eq!(
            out["collection"]["features"][0]["properties"]["height_count"],
            serde_json::Value::from(2)
        );
        assert_eq!(
            out["collection"]["features"][0]["properties"]["height_mean"],
            serde_json::Value::from(20.0)
        );
    }

    #[test]
    fn call_analysis_sync_rejects_non_eligible_tool() {
        let err = call_analysis_sync("kanyu_data_load", serde_json::Map::new()).unwrap_err();
        assert!(err.to_string().contains("不支持任务化执行"), "{err}");
    }

    /// attr_scaler fixture 与 buildings 示例的绝对路径（mcp 测试 cwd = crate 根）。
    const GENE_FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../kanyu-skill/testdata/attr_scaler.wasm"
    );
    const BUILDINGS: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/buildings.geojson"
    );

    fn new_registry() -> GeneRegistry {
        std::sync::Arc::new(std::sync::Mutex::new(SkillRegistryState {
            host: kanyu_skill::SkillHost::new().unwrap(),
            skills: std::collections::HashMap::new(),
        }))
    }

    #[test]
    fn hotload_register_then_skill_run_doubles_height() {
        let registry = new_registry();
        // hotload：加载校验 + 注册（复刻 system_hotload 工具的注册语义）。
        let (skill_id, meta) = {
            let mut state = registry.lock().unwrap();
            let skill = state.host.load(GENE_FIXTURE).unwrap();
            let skill_id = skill.meta().name.clone();
            let meta = skill.meta().clone();
            assert!(state.skills.insert(skill_id.clone(), skill).is_none());
            (skill_id, meta)
        };
        assert_eq!(skill_id, "attr_scaler");
        assert_eq!(meta.version, "0.1.0");

        // skill_run：buildings.geojson 的 height 精确翻倍。
        let out = {
            let mut state = registry.lock().unwrap();
            skill_run_sync(
                &mut state,
                SkillRunReq {
                    skill_id: skill_id.clone(),
                    path: BUILDINGS.to_string(),
                },
            )
            .unwrap()
        };
        assert_eq!(out["feature_count"], 4);
        let heights: Vec<f64> = out["collection"]["features"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["properties"]["height"].as_f64())
            .collect();
        assert!(heights.contains(&177.0), "88.5→177.0 应出现: {heights:?}");
        assert!(heights.contains(&66.0), "33→66.0 应出现: {heights:?}");

        // skill_list 快照含注册项（复刻 skill_list 工具的形状）。
        let state = registry.lock().unwrap();
        let skills: Vec<_> = state.skills.values().map(|g| &g.meta().name).collect();
        assert_eq!(skills, vec![&"attr_scaler".to_string()]);
    }

    #[test]
    fn skill_run_unknown_skill_id_gives_chinese_error() {
        let registry = new_registry();
        let mut state = registry.lock().unwrap();
        let err = skill_run_sync(
            &mut state,
            SkillRunReq {
                skill_id: "no_such_gene".to_string(),
                path: BUILDINGS.to_string(),
            },
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("未注册") && err.to_string().contains("hotload"),
            "错误应提示先 hotload: {err}"
        );
    }

    #[test]
    fn hotload_rejects_garbage_wasm_without_registering() {
        let dir = std::env::temp_dir().join("kanyu_mcp_gene_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let bad = dir.join("bad.wasm");
        std::fs::write(&bad, b"not wasm at all").unwrap();
        let registry = new_registry();
        let state = registry.lock().unwrap();
        let err = match state.host.load(bad.to_str().unwrap()) {
            Ok(_) => panic!("垃圾 wasm 应加载失败"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("技能加载失败"), "{err}");
        // 验证职责：加载失败绝不注册（注册表保持空）。
        assert!(state.skills.is_empty());
    }

    #[tokio::test]
    async fn spawned_task_completes_with_sync_result() {
        let dir = std::env::temp_dir().join("kanyu_mcp_task_spawn");
        let (zones, values) = write_zonal_fixtures(&dir);
        let manager = rmcp::task_manager::TaskManager::new();
        let args = serde_json::json!({
            "zones": zones,
            "values": values,
            "field": "height",
            "stats": ["count"],
        })
        .as_object()
        .unwrap()
        .clone();
        let task = spawn_analysis_task(&manager, "kanyu_analysis_zonal_stats".into(), args);
        assert_eq!(task.ttl_ms, Some(TASK_TTL_MS));

        // 轮询到终态（小数据集应秒完成）。
        let mut detailed = None;
        for _ in 0..200 {
            let d = manager.get_task(&task.task_id).unwrap();
            if d.task.status.is_terminal() {
                detailed = Some(d);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let detailed = detailed.expect("任务应在 2s 内到达终态");
        assert_eq!(detailed.task.status, rmcp::model::TaskStatus::Completed);
        let json = serde_json::to_value(&detailed).unwrap();
        assert_eq!(
            json["result"]["structuredContent"]["feature_count"],
            serde_json::Value::from(1)
        );
        assert_eq!(
            json["result"]["structuredContent"]["collection"]["features"][0]["properties"]
                ["height_count"],
            serde_json::Value::from(2)
        );

        // 未知 task_id：错误。
        assert!(manager.get_task("no-such-task").is_err());
    }

    #[tokio::test]
    async fn toolbox_list_returns_full_registry() {
        let server = KanyuServer::new();
        let Json(v) = server.toolbox_list().await.unwrap();
        assert_eq!(v["count"].as_u64().unwrap(), 37, "注册表应为 37 工具");
        let tools = v["tools"].as_array().unwrap();
        let ids: Vec<&str> = tools.iter().filter_map(|t| t["id"].as_str()).collect();
        assert!(ids.contains(&"buffer"), "应含 buffer");
        assert!(ids.contains(&"split_by_field"), "应含第三批工具");
        // 参数表投影可见（key/kind/required）。
        let buf = tools.iter().find(|t| t["id"] == "buffer").unwrap();
        assert_eq!(buf["params"][0]["key"], "layer");
        assert_eq!(buf["params"][0]["required"], true);
    }

    #[test]
    fn toolbox_run_buffer_produces_named_layer() {
        // 输入点图层（2 点），距离走 LinearUnit 线格式「数值|单位」。
        let dir = std::env::temp_dir().join("kanyu_mcp_toolbox_run");
        std::fs::create_dir_all(&dir).unwrap();
        let pts = dir.join("pts.geojson");
        std::fs::write(
            &pts,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{"a":1}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},"properties":{"a":2}}
            ]}"#,
        )
        .unwrap();
        let out = toolbox_run_sync(ToolboxRunReq {
            tool_id: "buffer".to_string(),
            params: vec!["pts".to_string(), "0.1|度".to_string()],
            layers: vec![ToolboxLayerRef {
                id: "pts".to_string(),
                path: pts.to_str().unwrap().to_string(),
            }],
        })
        .unwrap();
        assert_eq!(out["type"], "new_layer");
        assert_eq!(out["verb"], "缓冲区");
        let layers = out["layers"].as_object().unwrap();
        let buf = layers.get("buf_pts").expect("新图层应以 buf_pts 命名");
        let features = buf["features"].as_array().unwrap();
        assert_eq!(features.len(), 2, "缓冲结果要素数与输入一致");
        assert!(
            matches!(
                features[0]["geometry"]["type"].as_str(),
                Some("Polygon") | Some("MultiPolygon")
            ),
            "缓冲产出应为面"
        );
        // 报告类分支：stats。
        let rep = toolbox_run_sync(ToolboxRunReq {
            tool_id: "stats".to_string(),
            params: vec!["pts".to_string()],
            layers: vec![ToolboxLayerRef {
                id: "pts".to_string(),
                path: pts.to_str().unwrap().to_string(),
            }],
        })
        .unwrap();
        assert_eq!(rep["type"], "report");
        assert!(rep["report"].as_str().unwrap().contains("图层统计"));
        // 中文错误：未知工具 / 图层不存在 / 参数超个数。
        let err = toolbox_run_sync(ToolboxRunReq {
            tool_id: "nope".to_string(),
            params: vec![],
            layers: vec![],
        })
        .unwrap_err();
        assert!(err.to_string().contains("未知工具"), "{err}");
        let err = toolbox_run_sync(ToolboxRunReq {
            tool_id: "buffer".to_string(),
            params: vec!["ghost".to_string(), "0.1|度".to_string()],
            layers: vec![],
        })
        .unwrap_err();
        assert!(err.to_string().contains("图层不存在"), "{err}");
        let err = toolbox_run_sync(ToolboxRunReq {
            tool_id: "buffer".to_string(),
            params: vec!["a".to_string(), "0.1|度".to_string(), "extra".to_string()],
            layers: vec![],
        })
        .unwrap_err();
        assert!(err.to_string().contains("参数个数"), "{err}");
    }

    #[test]
    fn toolbox_run_task_path_shares_sync_impl() {
        // 任务化路径（call_analysis_sync）与同步工具共享 toolbox_run_sync。
        let dir = std::env::temp_dir().join("kanyu_mcp_toolbox_task");
        std::fs::create_dir_all(&dir).unwrap();
        let pts = dir.join("p.geojson");
        std::fs::write(
            &pts,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{}}
            ]}"#,
        )
        .unwrap();
        let args = serde_json::json!({
            "tool_id": "centroid",
            "params": ["p"],
            "layers": [{"id": "p", "path": pts.to_str().unwrap()}],
        })
        .as_object()
        .unwrap()
        .clone();
        let out = call_analysis_sync("kanyu_toolbox_run", args).unwrap();
        assert_eq!(out["type"], "new_layer");
        assert!(out["layers"].as_object().unwrap().contains_key("cen_p"));
    }

    // ===== resources / prompts =====

    /// 从 ReadResourceResponse 取首块文本内容。
    fn resource_text(resp: &ReadResourceResponse) -> String {
        let ReadResourceResponse::Complete(result) = resp else {
            panic!("应为 Complete 响应")
        };
        match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            other => panic!("应为文本资源: {other:?}"),
        }
    }

    #[test]
    fn read_resource_static_and_crs_template() {
        // 静态资源：格式矩阵与工具清单。
        let formats = read_resource_sync("kanyu://formats").unwrap();
        let text = resource_text(&formats);
        assert!(
            text.contains("geojson"),
            "格式矩阵应含 geojson: {text:.200}"
        );
        let tools = read_resource_sync("kanyu://tools").unwrap();
        let text = resource_text(&tools);
        assert!(
            text.contains("kanyu_toolbox_run"),
            "工具清单应含 toolbox 工具"
        );
        // crs 模板：4490 命中（名称/类型/单位/proj4 齐套）。
        let crs = read_resource_sync("kanyu://crs/4490").unwrap();
        let text = resource_text(&crs);
        assert!(
            text.contains("China Geodetic Coordinate System 2000"),
            "{text}"
        );
        assert!(text.contains("Geographic"), "{text}");
        assert!(text.contains("度"), "{text}");
        assert!(text.contains("+proj=longlat"), "{text}");
        // 中文错误：未知 URI / 非法代码 / 库中不存在。
        let e = read_resource_sync("kanyu://bogus").unwrap_err();
        assert!(e.message.contains("未知资源 URI"), "{e}");
        let e = read_resource_sync("kanyu://crs/abc").unwrap_err();
        assert!(e.message.contains("须为数值"), "{e}");
        let e = read_resource_sync("kanyu://crs/9999").unwrap_err();
        assert!(e.message.contains("不在内置库"), "{e}");
    }

    #[test]
    fn prompts_list_and_get_with_argument_substitution() {
        // list：三个模板，参数元数据齐套。
        let list = list_prompts_sync();
        assert_eq!(list.prompts.len(), 3);
        let names: Vec<&str> = list.prompts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["data_health_check", "buffer_analysis", "crs_transform"]
        );
        let health = &list.prompts[0];
        let args = health.arguments.as_ref().unwrap();
        assert_eq!(args[0].name, "path");
        assert_eq!(args[0].required, Some(true));
        // get：参数替换进模板，含真实工具名引用。
        let mut arguments = serde_json::Map::new();
        arguments.insert("path".to_string(), serde_json::Value::from("roads.geojson"));
        arguments.insert("distance".to_string(), serde_json::Value::from("500"));
        let resp = get_prompt_sync("buffer_analysis", Some(&arguments)).unwrap();
        let GetPromptResponse::Complete(result) = resp else {
            panic!("应为 Complete 响应")
        };
        let text = format!("{:?}", result.messages[0].content);
        assert!(text.contains("roads.geojson"), "路径应替换: {text:.300}");
        assert!(text.contains("500"), "距离应替换: {text:.300}");
        assert!(!text.contains("{path}"), "占位符应全部替换: {text:.300}");
        assert!(text.contains("kanyu_analysis_buffer"), "应引用真实工具名");
        // 缺必填参数 / 未知 prompt：中文错误。
        let e = get_prompt_sync("buffer_analysis", None).unwrap_err();
        assert!(e.message.contains("缺少必填参数"), "{e}");
        let e = get_prompt_sync("nope", None).unwrap_err();
        assert!(e.message.contains("未知 prompt"), "{e}");
    }
    /// 不动产测试宗地（40m×30m 矩形，投影坐标，带 parcel_* 属性）。
    fn write_parcel_fixture(dir: &std::path::Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let parcel = dir.join("parcel.geojson");
        std::fs::write(
            &parcel,
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                    [[39595000.0,4127000.0],[39595040.0,4127000.0],[39595040.0,4127030.0],[39595000.0,4127030.0],[39595000.0,4127000.0]]]},
                 "properties":{"parcel_id":"371602113005GB00032","parcel_use":"0801","area":1200.0,"owner":"测试权利人"}}
            ]}"#,
        )
        .unwrap();
        parcel.to_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn render_parcel_map_svg_reports_scale_and_zero_overlap() {
        let dir = std::env::temp_dir().join("kanyu_mcp_parcelmap");
        let parcel = write_parcel_fixture(&dir);
        let server = KanyuServer::new();
        let result = server
            .render_parcel_map(Parameters(RenderParcelMapReq {
                path: parcel,
                format: "svg".to_string(),
                out: None,
                parcel_code: None,
                owner: None,
                map_sheet: None,
                area: None,
                land_use: None,
                unit_name: None,
                survey_note: None,
                drawer: None,
                reviewer: None,
                draw_date: None,
                review_date: None,
                sizhi_e: None,
                sizhi_s: None,
                sizhi_w: None,
                sizhi_n: None,
                roads: None,
                scale: None,
                dpi: None,
                index: None,
            }))
            .await
            .unwrap();
        let sc = result.structured_content.unwrap();
        assert!(sc["scale"].as_u64().unwrap() >= 100);
        assert_eq!(sc["overlap_count"].as_u64().unwrap(), 0);
        // 4 点号 + 4 边长
        assert_eq!(sc["label_count"].as_u64().unwrap(), 8);
    }

    #[tokio::test]
    async fn render_parcel_dxf_writes_south_xdata() {
        let dir = std::env::temp_dir().join("kanyu_mcp_parceldxf");
        let parcel = write_parcel_fixture(&dir);
        let out = dir.join("parcel_cass.dxf");
        let server = KanyuServer::new();
        let Json(v) = server
            .render_parcel_dxf(Parameters(RenderParcelDxfReq {
                path: parcel,
                out: out.to_str().unwrap().to_string(),
                parcel_code: None,
                land_use: None,
                owner: None,
                scale: None,
                no_xdata: None,
                index: None,
            }))
            .await
            .unwrap();
        assert_eq!(v["boundary_points"], 4);
        assert_eq!(v["boundary_lines"], 4);
        assert_eq!(v["xdata"], true);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("SOUTH") && text.contains("302001") && text.contains("302002"));
        // 分式分子 = 宗地代码末 7 位
        assert!(text.contains("GB00032"));
    }

    #[tokio::test]
    async fn data_kdb_pack_builds_v2_container() {
        let dir = std::env::temp_dir().join("kanyu_mcp_kdbpack");
        let parcel = write_parcel_fixture(&dir);
        let dat = dir.join("pts.dat");
        std::fs::write(
            &dat,
            "J1,302001,39595462.533,4127300.446,12.5
",
        )
        .unwrap();
        let out = dir.join("db.kdb");
        let server = KanyuServer::new();
        let Json(v) = server
            .data_kdb_pack(Parameters(DataKdbPackReq {
                files: vec![parcel, dat.to_str().unwrap().to_string()],
                out: out.to_str().unwrap().to_string(),
            }))
            .await
            .unwrap();
        assert_eq!(v["format_version"], "2");
        assert_eq!(v["layer_count"], 2);
        assert_eq!(v["layers"][1]["name"], "pts");
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..4], b"PK", "应为 zip 容器");
        let layers = Layer::load_kdb_layers(out.to_str().unwrap()).unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 1);
    }
    #[tokio::test]
    async fn render_sea_map_kinds_render_with_scale() {
        let dir = std::env::temp_dir().join("kanyu_mcp_seamap");
        let parcel = write_parcel_fixture(&dir);
        let server = KanyuServer::new();
        let mk_req = |kind: Option<&str>| RenderSeaMapReq {
            path: parcel.clone(),
            kind: kind.map(str::to_string),
            format: "svg".to_string(),
            out: None,
            project_name: Some("代理围填海项目".to_string()),
            sea_code: None,
            source_epsg: Some("4527".to_string()),
            survey_unit: None,
            surveyor: None,
            drawer: None,
            draw_date: None,
            inspector: None,
            reviewer: None,
            scale: None,
            dpi: None,
            index: None,
        };
        // L.7 界址图（默认 kind + 纯数字 EPSG 规范化）
        let r7 = server
            .render_sea_map(Parameters(mk_req(None)))
            .await
            .unwrap();
        let sc = r7.structured_content.unwrap();
        assert_eq!(sc["kind"], "宗海界址图");
        assert!(sc["scale"].as_u64().unwrap() >= 100);
        assert_eq!(sc["overlap_count"], 0);
        assert!(sc["label_count"].as_u64().unwrap() > 0);
        // L.6 位置图（无注记）
        let r6 = server
            .render_sea_map(Parameters(mk_req(Some("location"))))
            .await
            .unwrap();
        let sc6 = r6.structured_content.unwrap();
        assert_eq!(sc6["kind"], "宗海位置图");
        assert_eq!(sc6["label_count"], 0);
        // L.8 平面布置图
        let r8 = server
            .render_sea_map(Parameters(mk_req(Some("layout"))))
            .await
            .unwrap();
        assert_eq!(r8.structured_content.unwrap()["kind"], "宗海平面布置图");
        // 未知图种：中文错误
        let err = server
            .render_sea_map(Parameters(mk_req(Some("island"))))
            .await
            .unwrap_err();
        assert!(err.message.contains("未知图种"), "{err}");
    }
}
