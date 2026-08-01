//! # kanyu-mcp —— 堪舆的 MCP 神经接口
//!
//! 基于官方 [`rmcp`](https://crates.io/crates/rmcp) SDK 实现，
//! 将堪舆内核能力暴露为标准 MCP 工具，供任何 MCP 兼容的 AI 代理调用。
//!
//! ## 设计要点
//!
//! - **确定性、可审计**：只暴露声明式工具，拒绝 `execute_code` 式任意代码执行
//!   （与 qgis_mcp 系 Python 薄壳的本质区别，见 docs/MCP.md）。
//! - **工具命名**：MCP 规范限制工具名为 `[a-zA-Z0-9_-]`，
//!   故总规中的 `kanyu.data.load` 落地为 `kanyu_data_load`。
//! - **结构化输出**：所有工具返回携带 CRS / 单位 / 要素数等元数据的 JSON。

mod server;

pub use server::{serve_stdio, KanyuServer};
