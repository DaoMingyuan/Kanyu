//! 系统自省：AI 读取自身。
//!
//! `kanyu introspect` 的内核实现。输出当前架构、模块清单、
//! 格式能力矩阵与 MCP 工具清单——这是自迭代闭环 Phase 1（观察）的入口。

use serde::Serialize;

use crate::format::{FormatCapabilities, FormatRegistry};
use crate::{CODENAME, VERSION};

/// 一个已注册内核模块的描述。
#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    /// 模块名。
    pub name: &'static str,
    /// 职责。
    pub role: &'static str,
    /// 状态（stable / incubating / planned）。
    pub status: &'static str,
}

/// MCP 工具清单中的一项（名称 + 分组）。
#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    /// 工具全名（真实 MCP 表面名），如 `kanyu_data_load`。
    pub name: &'static str,
    /// 分组：data / agents / analysis / render / system。
    pub group: &'static str,
    /// 状态。
    pub status: &'static str,
}

/// 系统自省报告。
#[derive(Debug, Serialize)]
pub struct Introspection {
    /// 内核版本。
    pub version: &'static str,
    /// 内核代号。
    pub codename: &'static str,
    /// 架构宣言。
    pub manifesto: &'static str,
    /// 内核模块清单。
    pub modules: Vec<ModuleInfo>,
    /// 格式能力矩阵。
    pub formats: Vec<FormatCapabilities>,
    /// MCP 工具清单。
    pub tools: Vec<ToolInfo>,
}

/// 内核模块清单（单一事实来源；docs/ARCHITECTURE.md 由此生成）。
pub fn modules() -> Vec<ModuleInfo> {
    vec![
        ModuleInfo {
            name: "kanyu-core",
            role: "数据心脏：格式注册表、图层模型、AGENTS.md 语义、系统自省",
            status: "stable",
        },
        ModuleInfo {
            name: "kanyu-cli",
            role: "脊髓：`kanyu` 命令行，数据/分析/自省/插件/MCP 入口",
            status: "stable",
        },
        ModuleInfo {
            name: "kanyu-mcp",
            role: "神经接口：MCP Server，向 AI 暴露全部内核能力",
            status: "incubating",
        },
        ModuleInfo {
            name: "kanyu-render",
            role: "眼睛：GPU 渲染管线（wgpu），GeoArrow→SSBO 直通",
            status: "planned",
        },
        ModuleInfo {
            name: "kanyu-edit",
            role: "手：DCEL 增量拓扑编辑内核，Undo/Redo",
            status: "planned",
        },
        ModuleInfo {
            name: "kanyu-gene",
            role: "基因：WASM 插件系统（wasmtime 沙箱 + 热加载）",
            status: "planned",
        },
        ModuleInfo {
            name: "kanyu-shell",
            role: "壳层：桌面 UI（TitleBar/StatusBar/面板系统）",
            status: "planned",
        },
    ]
}

/// MCP 工具清单（单一事实来源；docs/MCP.md 由此生成）。
///
/// 命名为真实 MCP 表面名：协议限制工具名为 `[a-zA-Z0-9_-]`，
/// 总规中的点式逻辑名（如 `kanyu.data.load`）落地为下划线式。
pub fn tools() -> Vec<ToolInfo> {
    vec![
        // 数据工具
        ToolInfo {
            name: "kanyu_data_load",
            group: "data",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_data_query",
            group: "data",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_data_export",
            group: "data",
            status: "stable",
        },
        // 项目语义工具
        ToolInfo {
            name: "kanyu_agents_init",
            group: "agents",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_agents_validate",
            group: "agents",
            status: "stable",
        },
        // 空间分析工具
        ToolInfo {
            name: "kanyu_analysis_buffer",
            group: "analysis",
            status: "planned",
        },
        ToolInfo {
            name: "kanyu_analysis_overlay",
            group: "analysis",
            status: "planned",
        },
        ToolInfo {
            name: "kanyu_analysis_topology",
            group: "analysis",
            status: "planned",
        },
        // 渲染工具
        ToolInfo {
            name: "kanyu_render_symbolize",
            group: "render",
            status: "planned",
        },
        ToolInfo {
            name: "kanyu_render_camera",
            group: "render",
            status: "planned",
        },
        // 系统工具
        ToolInfo {
            name: "kanyu_system_introspect",
            group: "system",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_system_generate",
            group: "system",
            status: "planned",
        },
        ToolInfo {
            name: "kanyu_system_hotload",
            group: "system",
            status: "planned",
        },
    ]
}

/// 生成自省报告。
pub fn report() -> Introspection {
    Introspection {
        version: VERSION,
        codename: CODENAME,
        manifesto: "以天地为盘，以数据为爻，以 AI 为神，重构地理空间之道。",
        modules: modules(),
        formats: FormatRegistry::builtin().all().to_vec(),
        tools: tools(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_serializable_and_complete() {
        let r = report();
        let json = serde_json::to_string_pretty(&r).unwrap();
        assert!(json.contains("kanyu-core"));
        assert!(json.contains("kanyu_data_load"));
        assert!(!r.formats.is_empty());
    }
}
