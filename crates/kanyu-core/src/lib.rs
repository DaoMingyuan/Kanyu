//! # kanyu-core —— 堪舆内核
//!
//! 堪舆 (Kanyu) 的数据心脏。本 crate 提供：
//!
//! - [`format`](mod@format)：统一格式注册表与能力矩阵（读/写/编辑/符号/布局）。
//! - [`layer`]：图层内存模型，GeoArrow RecordBatch 载体与属性查询。
//! - [`analysis`]：空间分析内核（buffer/overlay/topology，geo crate）。
//! - [`crs`]：坐标参考系工具（投影变换/测地线度量，proj4rs + geo geodesic）。
//! - [`agents`]：`AGENTS.md` 项目语义文件解析，AI 理解项目的"罗盘"。
//! - [`introspect`]：系统自省，输出架构、能力与格式矩阵（供 AI 读取自身）。
//!
//! 设计原则：**纯 Rust 内核，零 C 依赖**。GDAL / LibreDWG 等 C/C++ 桥接
//! 以可选 feature 形式存在，不进入默认构建，保证任何平台 `cargo build` 即可用。

pub mod agents;
pub mod analysis;
pub mod attrcalc;
pub mod bench;
pub mod cartography;
pub mod cass;
pub mod crs;
pub mod dwg;
pub mod error;
pub mod format;
pub mod geoprocess;
pub mod introspect;
pub mod kdb;
pub mod layer;
pub mod parcel;
pub mod project;
pub mod tooldef;
pub mod toolrun;

pub use error::{KanyuError, Result};
pub use format::{FormatCapabilities, FormatRegistry};
pub use layer::{Layer, LayerSummary};

/// 内核版本号（与 workspace 版本一致）。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 内核代号：堪舆灵 (Kanyu Spirit)。
pub const CODENAME: &str = "kanyu-spirit";
