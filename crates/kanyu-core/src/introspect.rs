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
    /// 分组：data / agents / analysis / render / system / gene / toolbox。
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
            role: "数据心脏：格式注册表、图层模型、空间分析、投影/度量、AGENTS.md 语义、系统自省",
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
            role: "眼睛：离屏地图渲染（tiny-skia PNG + SVG，晨山/夜观星主题，属性驱动符号化），wgpu 实时管线待壳层",
            status: "incubating",
        },
        ModuleInfo {
            name: "kanyu-edit",
            role: "手：编辑内核（Undo/Redo：命令逆操作双栈 + Delta 快照 v2 双通道 + 事务原子提交；基础编辑命令：顶点移动/要素平移/删除/插入/属性更新）；DCEL 拓扑待后续",
            status: "incubating",
        },
        ModuleInfo {
            name: "kanyu-skill",
            role: "技能：WASM 插件宿主（wasmtime 沙箱 + WIT 组件模型 ABI + fuel 配额）+ MCP 热加载（hotload/skill_run/skill_list）",
            status: "incubating",
        },
        ModuleInfo {
            name: "kanyu-shell",
            role: "壳层：egui 桌面 UI（TitleBar/图层面板/MapCanvas/StatusBar/双主题）",
            status: "incubating",
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
        ToolInfo {
            name: "kanyu_data_reproject",
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
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_overlay",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_topology",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_measure",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_sjoin",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_zonal_stats",
            group: "analysis",
            status: "stable",
        },
        // QGIS 核心算法移植（geoprocess 模块）
        ToolInfo {
            name: "kanyu_analysis_dissolve",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_simplify",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_centroid",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_convex_hull",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_delete_holes",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_explode",
            group: "analysis",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_analysis_stats",
            group: "analysis",
            status: "stable",
        },
        // 数据质检
        ToolInfo {
            name: "kanyu_data_validate",
            group: "data",
            status: "stable",
        },
        // 渲染工具
        ToolInfo {
            name: "kanyu_render_map",
            group: "render",
            status: "stable",
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
            status: "stable",
        },
        // 技能工具
        ToolInfo {
            name: "kanyu_skill_run",
            group: "gene",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_skill_list",
            group: "gene",
            status: "stable",
        },
        // 工具箱工具（core::tooldef 注册表投影——壳层/MCP 同一事实来源）
        ToolInfo {
            name: "kanyu_toolbox_list",
            group: "toolbox",
            status: "stable",
        },
        ToolInfo {
            name: "kanyu_toolbox_run",
            group: "toolbox",
            status: "stable",
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
