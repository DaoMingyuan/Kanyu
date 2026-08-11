//! 工具箱面板（QGIS Processing 工具箱范式）——壳层投影。
//!
//! 类型与 37 工具注册表已下沉 `kanyu_core::tooldef`（单一事实来源，
//! 供 shell 工具箱 / kanyu-py SDK / MCP 三面投影），执行在
//! `kanyu_core::toolrun`；本模块只保留 egui 呈现：
//!
//! - [`params`]：每种参数类型一个独立组件（统一契约：`&mut 值 + 校验错误 +
//!   Response`），供任意对话框组合调用；
//! - [`dialog`]：ArcGIS Pro 式工具参数对话框（参数帮助区 + 内联校验 +
//!   「运行」按校验置灰）；
//! - [`panel`]：工具箱面板（最近使用 / 收藏 / 分类树 / 筛选）。
//!
//! 加工具 = `kanyu-core/src/tooldef.rs` 注册表加一行 + `toolrun.rs` 加分支。

mod dialog;
mod panel;
mod params;

pub use dialog::{run_dialog, DialogOutcome, ToolRunState};
pub use kanyu_core::tooldef::{find, ParamKind, ToolCategory, ToolDef, ToolParam, TOOLS};
pub use kanyu_core::toolrun::{run_tool, ToolOutcome};
pub use panel::ToolboxPanel;

#[cfg(test)]
mod tests {
    //! 壳层投影接线守护：注册表经 core 路径可达（详细语义测试在 core 侧）。

    #[test]
    fn registry_reachable_via_core() {
        assert_eq!(super::TOOLS.len(), 37);
        assert_eq!(super::find("buffer").unwrap().name, "缓冲区");
    }
}
