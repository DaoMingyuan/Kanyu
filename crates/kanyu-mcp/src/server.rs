//! MCP Server：工具路由与 stdio 服务。

use kanyu_core::{agents, introspect, FormatRegistry, KanyuError, Layer};
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
const TASK_ELIGIBLE: [&str; 5] = [
    "kanyu_analysis_buffer",
    "kanyu_analysis_overlay",
    "kanyu_analysis_sjoin",
    "kanyu_analysis_zonal_stats",
    "kanyu_analysis_topology",
];

/// 任务结果保留时长（10 分钟；TaskManager 惰性 TTL 清扫，重启即丢——
/// 内存态注册表，见 docs/MCP.md §2）。
const TASK_TTL_MS: u64 = 600_000;

/// 堪舆 MCP Server。工具调用无状态（每次调用重新加载执行）；
/// 另持有 SEP-2663 任务管理器（内存态，Clone 共享）支撑长任务。
#[derive(Clone)]
pub struct KanyuServer {
    /// rmcp 工具路由表（由 #[tool_router] 宏生成并填充）。
    tool_router: ToolRouter<Self>,
    /// SEP-2663 任务管理器：spawn/TTL/协作取消（rmcp 3.1 内置）。
    task_manager: rmcp::task_manager::TaskManager,
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

#[tool_router]
impl KanyuServer {
    /// 构造 Server 并注册全部工具。
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            task_manager: rmcp::task_manager::TaskManager::new(),
        }
    }

    /// 加载地理数据文件，返回图层概要（要素数/几何类型/字段/CRS）。
    #[tool(
        name = "kanyu_data_load",
        description = "加载地理数据文件到内存，返回图层概要（要素数、几何类型、字段清单）"
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
        description = "将数据文件导出为目标格式（当前原生支持 geojson/csv/fgb/geoparquet/dxf/kml；其余格式受格式能力矩阵与驱动状态约束）"
    )]
    async fn data_export(
        &self,
        Parameters(req): Parameters<DataExportReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let registry = FormatRegistry::builtin();
        let caps = registry.require(&req.format, "write").map_err(to_mcp)?;
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

    /// 校验 AGENTS.md 项目语义文件完整性。
    #[tool(
        name = "kanyu_agents_validate",
        description = "校验 AGENTS.md 项目语义文件：元数据（name/crs）、图层语义、业务规则完整性"
    )]
    async fn agents_validate(
        &self,
        Parameters(req): Parameters<AgentsValidateReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let doc = agents::load(&req.path).map_err(to_mcp)?;
        let issues = doc.validate();
        Ok(Json(serde_json::json!({
            "valid": issues.is_empty(),
            "issues": issues,
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
        let crs = req.crs.unwrap_or_else(|| "EPSG:4326".to_string());
        std::fs::write(&path, agents::template(&name, &crs)).map_err(to_mcp)?;
        Ok(Json(
            serde_json::json!({ "created": path.display().to_string() }),
        ))
    }
}

#[tool_handler]
impl ServerHandler for KanyuServer {
    /// tools/call 分发：白名单分析工具（[`TASK_ELIGIBLE`]）的 arguments 带
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
            let task = spawn_analysis_task(&self.task_manager, name, args);
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
                .build(),
        )
        .with_instructions(
            "堪舆 (Kanyu) GIS 内核：data/agents/analysis/render/system 五组工具 + SEP-2663 长任务。\
                 所有结果为结构化 JSON 并携带 CRS/单位元数据；不提供任意代码执行。",
        );
        info.server_info.name = "kanyu-mcp".to_string();
        info.server_info.version = env!("CARGO_PKG_VERSION").to_string();
        info
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
}
