//! 统一错误类型。

use thiserror::Error;

/// 堪舆内核错误。
#[derive(Debug, Error)]
pub enum KanyuError {
    /// 无法识别的数据格式。
    #[error("unrecognized format: {0}")]
    UnknownFormat(String),

    /// 目标格式不支持请求的操作（读/写/编辑等）。
    #[error("format '{format}' does not support operation '{operation}'")]
    UnsupportedOperation {
        /// 格式名（如 `dwg`）。
        format: String,
        /// 操作名（如 `write`）。
        operation: String,
    },

    /// 数据 I/O 失败。
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// GeoJSON 解析失败。
    #[error("geojson error: {0}")]
    GeoJson(String),

    /// AGENTS.md 语义文件解析失败。
    #[error("agents.md parse error: {0}")]
    AgentsMd(String),

    /// 查询表达式无效。
    #[error("invalid query: {0}")]
    InvalidQuery(String),

    /// 其他错误。
    #[error("{0}")]
    Other(String),
}

impl From<geojson::Error> for KanyuError {
    fn from(e: geojson::Error) -> Self {
        KanyuError::GeoJson(e.to_string())
    }
}

/// 内核统一结果类型。
pub type Result<T> = std::result::Result<T, KanyuError>;
