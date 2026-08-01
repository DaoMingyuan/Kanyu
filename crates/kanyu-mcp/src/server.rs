//! MCP Server：工具路由与 stdio 服务。

use kanyu_core::{agents, introspect, FormatRegistry, Layer};
use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    transport::stdio,
    ErrorData as McpError, Json, ServerHandler, ServiceExt,
};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

/// 堪舆 MCP Server。无状态工具集：每次调用直接驱动内核。
#[derive(Clone)]
pub struct KanyuServer {
    /// rmcp 工具路由表（由 #[tool_router] 宏生成并填充）。
    #[allow(dead_code)] // 经宏生成的 dispatch 代码间接持有，静态分析不可见。
    tool_router: ToolRouter<Self>,
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

#[tool_router]
impl KanyuServer {
    /// 构造 Server 并注册全部工具。
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
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
        description = "将数据文件导出为目标格式（当前原生支持 geojson/csv/fgb/geoparquet/dxf；其余格式受格式能力矩阵与驱动状态约束）"
    )]
    async fn data_export(
        &self,
        Parameters(req): Parameters<DataExportReq>,
    ) -> Result<Json<serde_json::Value>, McpError> {
        let registry = FormatRegistry::builtin();
        let caps = registry.require(&req.format, "write").map_err(to_mcp)?;
        let layer = Layer::load(stem_of(&req.path), &req.path).map_err(to_mcp)?;
        let bytes: Vec<u8> = match caps.id {
            "geojson" => Layer::to_geojson_string(layer.collection()).into_bytes(),
            "csv" => Layer::to_csv_string(layer.collection())
                .map_err(to_mcp)?
                .into_bytes(),
            "fgb" => Layer::to_fgb_bytes(layer.collection()).map_err(to_mcp)?,
            "geoparquet" => Layer::to_geoparquet_bytes(layer.collection()).map_err(to_mcp)?,
            "dxf" => Layer::to_dxf_string(layer.collection())
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
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "堪舆 (Kanyu) GIS 内核：data/agents/analysis/render/system 五组工具。\
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

/// 内核错误 → MCP 错误。
fn to_mcp(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// 取文件名主干作为默认图层名。
fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string()
}
