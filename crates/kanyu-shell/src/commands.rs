//! 命令注册表（ArcGIS Pro DAML 范式）：UI 命令声明为数据，Ribbon/QAT/右键菜单
//! 均为注册表的**投影**——文案/图标/简介只在 [`COMMANDS`] 写一次（单一事实来源）。
//!
//! - Ribbon：`ribbon.rs` 的页签→组→命令 id 声明式布局按 id 查表投影成按钮；
//! - QAT：按 id 取高频命令；
//! - 图层右键菜单等零散入口：按 id 取文案，不手抄字符串。
//!
//! 可用条件：[`CommandDef::enabled`] 以 [`AppSnapshot`]（图层数/有无选中）求值，
//! Ribbon 按条件置灰。刻意保持轻量（函数指针 + 小快照），不做完整 condition 系统。

use crate::dock::PanelId;
use crate::ribbon::RibbonAction;
use crate::ui_kit::Icon;

/// 应用快照（命令可用条件求值输入；app 每帧构造）。
#[derive(Clone, Copy, Debug, Default)]
pub struct AppSnapshot {
    /// 已加载图层数。
    pub layer_count: usize,
    /// 是否有选中图层。
    pub has_selection: bool,
}

/// 恒可用。
fn always(_: &AppSnapshot) -> bool {
    true
}
/// 有图层可用。
fn has_layers(s: &AppSnapshot) -> bool {
    s.layer_count > 0
}
/// 有选中图层可用。
fn has_selection(s: &AppSnapshot) -> bool {
    s.has_selection
}

/// 命令定义（DAML `<button>` 的 Rust 形态）。
pub struct CommandDef {
    /// 命令 id（英文，布局/投影引用键）。
    pub id: &'static str,
    /// 按钮标题（中文）。
    pub title: &'static str,
    /// 图标。
    pub icon: Icon,
    /// 悬停简介卡标题。
    pub desc_title: &'static str,
    /// 悬停简介卡正文。
    pub desc_body: &'static str,
    /// 执行动作。
    pub action: RibbonAction,
    /// 可用条件（快照求值；false 时 Ribbon 置灰）。
    pub enabled: fn(&AppSnapshot) -> bool,
}

const fn cmd(
    id: &'static str,
    title: &'static str,
    icon: Icon,
    desc_title: &'static str,
    desc_body: &'static str,
    action: RibbonAction,
    enabled: fn(&AppSnapshot) -> bool,
) -> CommandDef {
    CommandDef {
        id,
        title,
        icon,
        desc_title,
        desc_body,
        action,
        enabled,
    }
}

/// 命令注册表（Ribbon 全部按钮 + QAT + 右键菜单共享入口）。
pub const COMMANDS: &[CommandDef] = &[
    // —— 主页：工程 ——
    cmd(
        "open_data",
        "打开数据…",
        Icon::Folder,
        "打开地理数据文件",
        "支持 shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/xlsx/kdb；也可直接拖文件入窗",
        RibbonAction::OpenData,
        always,
    ),
    cmd(
        "open_example",
        "打开示例",
        Icon::Example,
        "加载内置示例",
        "examples/buildings.geojson（3 建筑点 + 1 道路线）",
        RibbonAction::OpenExample,
        always,
    ),
    cmd(
        "open_project",
        "打开工程…",
        Icon::Export,
        "打开堪舆工程 (.kyu)",
        "恢复图层清单、可见性、视口与地图色彩设置",
        RibbonAction::OpenProject,
        always,
    ),
    cmd(
        "save_project",
        "保存工程",
        Icon::Info,
        "保存堪舆工程 (.kyu)",
        "把当前图层、可见性、视口、地图色彩保存为 .kyu 工程文件",
        RibbonAction::SaveProject,
        always,
    ),
    // —— 主页：窗口 ——
    cmd(
        "save_screenshot",
        "保存截图",
        Icon::Camera,
        "窗口截图",
        "把当前整个窗口保存为 PNG（含 Ribbon 与面板）",
        RibbonAction::SaveScreenshot,
        always,
    ),
    cmd(
        "toggle_theme",
        "切换主题",
        Icon::Sun,
        "切换界面主题",
        "晨山 / 夜观星；只改变界面，不改变地图色彩（见「视图 → 地图色彩」）",
        RibbonAction::ToggleTheme,
        always,
    ),
    cmd(
        "settings",
        "设置…",
        Icon::Settings,
        "设置",
        "工程坐标系选择与渲染设置（独立设置对话框，不占功能区）",
        RibbonAction::SettingsDialog,
        always,
    ),
    // —— 主页：关于 / 帮助页 ——
    cmd(
        "about",
        "关于堪舆",
        Icon::Info,
        "关于堪舆",
        "版本、架构、许可证",
        RibbonAction::About,
        always,
    ),
    // —— 数据 ——
    cmd(
        "layer_info",
        "图层概要",
        Icon::Info,
        "图层概要",
        "当前图层的要素数/几何类型/字段（输出到终端）",
        RibbonAction::LayerInfo,
        has_selection,
    ),
    cmd(
        "query",
        "属性查询…",
        Icon::Funnel,
        "属性查询",
        "如 height > 50；结果存为新图层",
        RibbonAction::QueryDialog,
        has_layers,
    ),
    cmd(
        "export_layer",
        "导出图层…",
        Icon::Export,
        "导出图层",
        "geojson/csv/fgb/parquet/dxf/kml/kmz/shp/kdb 全格式互转",
        RibbonAction::ExportDialog,
        has_layers,
    ),
    cmd(
        "reproject",
        "投影变换…",
        Icon::Compass,
        "投影变换",
        "EPSG 全库（如 4326 → 3857），结果存为新图层",
        RibbonAction::ReprojectDialog,
        has_layers,
    ),
    // —— 分析 ——
    cmd(
        "buffer",
        "缓冲区…",
        Icon::Buffer,
        "缓冲区分析",
        "按距离生成缓冲区（结果存为新图层；米制请先投影）",
        RibbonAction::BufferDialog,
        has_layers,
    ),
    cmd(
        "overlay",
        "叠加分析…",
        Icon::Overlay,
        "叠加分析",
        "union/intersection/difference/xor（两个面图层）",
        RibbonAction::OverlayDialog,
        has_layers,
    ),
    cmd(
        "topology",
        "拓扑检查",
        Icon::Topology,
        "拓扑检查",
        "no_overlap 规则（结果输出到终端）",
        RibbonAction::Topology,
        has_selection,
    ),
    cmd(
        "sjoin",
        "空间连接…",
        Icon::Link,
        "空间连接",
        "按空间谓词合并两图层属性（左连接 + explode）",
        RibbonAction::SjoinDialog,
        has_layers,
    ),
    cmd(
        "zonal",
        "分区统计…",
        Icon::Grid,
        "分区统计",
        "面要素分区统计数值字段（count/sum/mean/min/max）",
        RibbonAction::ZonalDialog,
        has_layers,
    ),
    cmd(
        "measure",
        "测地度量…",
        Icon::Ruler,
        "测地线度量",
        "Karney 2013 测地线长度/面积（米/平方米）",
        RibbonAction::MeasureDialog,
        has_layers,
    ),
    // —— 制图 ——
    cmd(
        "export_map",
        "导出地图…",
        Icon::Export,
        "导出地图",
        "当前视图导出为 PNG / SVG（尺寸/样式在「设置 → 渲染」，色彩由「视图 → 地图色彩」决定）",
        RibbonAction::ExportMapDialog,
        has_layers,
    ),
    // —— 视图：相机 ——
    cmd(
        "zoom_fit",
        "缩放到图层",
        Icon::ZoomFit,
        "缩放到图层",
        "全部可见图层的数据范围",
        RibbonAction::ZoomToFit,
        has_layers,
    ),
    cmd(
        "reset_view",
        "复位视图",
        Icon::Reset,
        "复位视图",
        "恢复初始相机",
        RibbonAction::ResetView,
        always,
    ),
    cmd(
        "new_map_view",
        "新建地图视图",
        Icon::Image,
        "新建地图视图",
        "打开浮动地图视图窗口（独立视口，可切换实验性三维场景）",
        RibbonAction::NewMapView,
        always,
    ),
    // —— 视图：面板 ——
    cmd(
        "panel_catalog",
        "目录",
        Icon::Folder,
        "目录面板",
        "显示/关闭目录面板（页签可拖动改停靠、拖到画布变浮动窗）",
        RibbonAction::TogglePanel(PanelId::Catalog),
        always,
    ),
    cmd(
        "panel_layers",
        "图层",
        Icon::Layers,
        "图层面板",
        "显示/关闭图层面板（Contents 目录树）",
        RibbonAction::TogglePanel(PanelId::Layers),
        always,
    ),
    cmd(
        "panel_toolbox",
        "工具箱",
        Icon::Toolbox,
        "工具箱面板",
        "显示/关闭工具箱面板（QGIS Processing 式算法清单）",
        RibbonAction::TogglePanel(PanelId::Toolbox),
        always,
    ),
    cmd(
        "panel_attrtable",
        "属性表",
        Icon::Field,
        "属性表面板",
        "显示/关闭属性表面板（字段表格 + 字段计算器）",
        RibbonAction::TogglePanel(PanelId::AttrTable),
        always,
    ),
    cmd(
        "panel_console",
        "终端",
        Icon::PanelBottom,
        "终端面板",
        "显示/关闭终端面板（命令直达内核）",
        RibbonAction::TogglePanel(PanelId::Console),
        always,
    ),
    cmd(
        "panel_aichat",
        "AI 对话",
        Icon::Chat,
        "AI 对话面板",
        "显示/关闭 AI 对话面板（自然语言驱动分析）",
        RibbonAction::TogglePanel(PanelId::AiChat),
        always,
    ),
    // —— 视图：地图色彩 ——
    cmd(
        "map_theme",
        "地图色彩",
        Icon::Sun,
        "地图色彩模式",
        "固定晨山 → 固定夜观星 → 跟随界面（默认固定晨山，保证制图输出正确）",
        RibbonAction::CycleMapTheme,
        always,
    ),
    // —— 技能 ——
    cmd(
        "skill_hotload",
        "热加载…",
        Icon::Skill,
        "热加载技能",
        "加载并校验 .wasm 技能（wasmtime 沙箱 + fuel 配额）",
        RibbonAction::SkillHotload,
        always,
    ),
    cmd(
        "skill_list",
        "技能清单",
        Icon::List,
        "技能清单",
        "已注册技能（输出到终端）",
        RibbonAction::SkillList,
        always,
    ),
    cmd(
        "skill_run",
        "运行技能…",
        Icon::Play,
        "运行技能",
        "在选定图层上执行技能，结果存为新图层",
        RibbonAction::SkillRunDialog,
        has_layers,
    ),
    // —— 帮助 ——
    cmd(
        "help",
        "命令速查",
        Icon::Help,
        "命令速查",
        "终端命令速查（输出到终端）",
        RibbonAction::ShowHelp,
        always,
    ),
];

/// 按 id 查命令。
pub fn find(id: &str) -> Option<&'static CommandDef> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// QAT 快速访问栏的命令 id（从左到右；主题切换因图标随主题动态，单独处理）。
pub const QAT_COMMANDS: &[&str] = &["save_project", "settings"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(seen.insert(c.id), "命令 id 重复: {}", c.id);
            assert!(
                !c.title.is_empty() && !c.desc_body.is_empty(),
                "{} 缺文案",
                c.id
            );
        }
    }

    #[test]
    fn find_works() {
        assert_eq!(find("buffer").unwrap().action, RibbonAction::BufferDialog);
        assert!(find("nope").is_none());
    }

    #[test]
    fn enabled_predicates() {
        let empty = AppSnapshot::default();
        let with_layers = AppSnapshot {
            layer_count: 2,
            has_selection: false,
        };
        let with_sel = AppSnapshot {
            layer_count: 2,
            has_selection: true,
        };
        let e = |id: &str, s: &AppSnapshot| (find(id).unwrap().enabled)(s);
        assert!(e("open_data", &empty));
        assert!(!e("buffer", &empty)); // 无图层置灰
        assert!(e("buffer", &with_layers));
        assert!(!e("layer_info", &with_layers)); // 无选中置灰
        assert!(e("layer_info", &with_sel));
        assert!(!e("topology", &with_layers));
        assert!(e("topology", &with_sel));
    }

    #[test]
    fn qat_commands_exist() {
        for id in QAT_COMMANDS {
            assert!(find(id).is_some(), "QAT 命令未登记: {id}");
        }
    }
}
