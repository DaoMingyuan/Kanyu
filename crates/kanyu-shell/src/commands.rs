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

/// 全局快捷键（纯函数判定；app ui() 顶层调用，`wants_keyboard_input` 守卫在调用方——
/// 焦点在文本框时不拦截）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppShortcut {
    /// Ctrl+Z（编辑会话撤销）。
    Undo,
    /// Ctrl+Y 或 Ctrl+Shift+Z（重做）。
    Redo,
    /// Ctrl+S（保存工程对话框）。
    SaveProject,
}

/// 快捷键匹配（command = Ctrl/Cmd 修饰键；egui modifiers.command 跨平台）。
pub fn match_shortcut(key: eframe::egui::Key, command: bool, shift: bool) -> Option<AppShortcut> {
    if !command {
        return None;
    }
    match key {
        eframe::egui::Key::Z if shift => Some(AppShortcut::Redo),
        eframe::egui::Key::Z => Some(AppShortcut::Undo),
        eframe::egui::Key::Y => Some(AppShortcut::Redo),
        eframe::egui::Key::S => Some(AppShortcut::SaveProject),
        _ => None,
    }
}

/// 应用快照（命令可用条件求值输入；app 每帧构造）。
#[derive(Clone, Copy, Debug, Default)]
pub struct AppSnapshot {
    /// 已加载图层数。
    pub layer_count: usize,
    /// 是否有选中图层。
    pub has_selection: bool,
    /// 编辑会话进行中。
    pub editing: bool,
    /// 可撤销。
    pub can_undo: bool,
    /// 可重做。
    pub can_redo: bool,
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
/// 有选中图层且未在编辑（开始编辑条件）。
fn can_start_edit(s: &AppSnapshot) -> bool {
    s.has_selection && !s.editing
}
/// 编辑会话中可用。
fn editing(s: &AppSnapshot) -> bool {
    s.editing
}
/// 可撤销。
fn can_undo(s: &AppSnapshot) -> bool {
    s.can_undo
}
/// 可重做。
fn can_redo(s: &AppSnapshot) -> bool {
    s.can_redo
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
        "把当前图层、可见性、视口、地图色彩保存为 .kyu 工程文件（快捷键 Ctrl+S）",
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
        "new_frame_2d",
        "新建二维地图框",
        Icon::Image,
        "新建二维地图框",
        "创建二维地图框并激活（绑定自有图层集；目录「地图框」可管理）",
        RibbonAction::NewFrame2D,
        always,
    ),
    cmd(
        "new_frame_3d",
        "新建三维场景",
        Icon::Compass,
        "新建三维场景",
        "创建三维场景框并激活（实验性软件 3D；二维/三维分开建立）",
        RibbonAction::NewFrame3D,
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
    // —— 编辑 ——
    cmd(
        "start_edit",
        "开始编辑",
        Icon::Play,
        "开始编辑会话",
        "以当前选中图层为目标开启编辑（同一时刻仅一个图层可编辑）",
        RibbonAction::StartEdit,
        can_start_edit,
    ),
    cmd(
        "save_edit",
        "保存编辑",
        Icon::Export,
        "保存编辑",
        "结束编辑会话（编辑即时生效；落盘请用「导出图层」）",
        RibbonAction::SaveEdit,
        editing,
    ),
    cmd(
        "discard_edit",
        "放弃编辑",
        Icon::Close,
        "放弃编辑",
        "逐条撤销到会话起点并结束会话",
        RibbonAction::DiscardEdit,
        editing,
    ),
    cmd(
        "edit_select",
        "选择",
        Icon::Info,
        "选择工具",
        "点击画布要素选中（空白处取消）",
        RibbonAction::SetEditTool(crate::edit::EditTool::Select),
        editing,
    ),
    cmd(
        "edit_vertex",
        "顶点编辑",
        Icon::Field,
        "顶点编辑工具",
        "拖拽顶点句柄移动顶点（MoveVertex 命令入历史）",
        RibbonAction::SetEditTool(crate::edit::EditTool::Vertex),
        editing,
    ),
    cmd(
        "edit_move",
        "移动要素",
        Icon::Reset,
        "移动要素工具",
        "拖动要素整体平移",
        RibbonAction::SetEditTool(crate::edit::EditTool::Move),
        editing,
    ),
    cmd(
        "edit_add_point",
        "添加点",
        Icon::Example,
        "添加点工具",
        "点击画布插入点要素（InsertFeature 命令入历史）",
        RibbonAction::SetEditTool(crate::edit::EditTool::AddPoint),
        editing,
    ),
    cmd(
        "edit_add_line",
        "添加线",
        Icon::Ruler,
        "添加线工具",
        "单击加顶点，双击/Enter 完成（≥2 点），Backspace 撤点，Esc 放弃",
        RibbonAction::SetEditTool(crate::edit::EditTool::AddLine),
        editing,
    ),
    cmd(
        "edit_add_polygon",
        "添加面",
        Icon::Grid,
        "添加面工具",
        "单击加顶点，双击/Enter 完成并自动闭合（≥3 点），Backspace 撤点，Esc 放弃",
        RibbonAction::SetEditTool(crate::edit::EditTool::AddPolygon),
        editing,
    ),
    cmd(
        "edit_add_hole",
        "添加洞",
        Icon::Overlay,
        "添加洞工具",
        "面图层：在目标面内绘制闭合环（完全在面内才生效），MultiPolygon 按子面定位",
        RibbonAction::SetEditTool(crate::edit::EditTool::AddHole),
        editing,
    ),
    cmd(
        "edit_split",
        "分割要素",
        Icon::Grid,
        "分割要素工具",
        "绘制切割线（双击完成）切分面要素；单击线要素在点击处打断为两段（一次撤销）",
        RibbonAction::SetEditTool(crate::edit::EditTool::Split),
        editing,
    ),
    cmd(
        "edit_snap",
        "捕捉",
        Icon::Topology,
        "顶点捕捉开关",
        "绘制/顶点编辑时吸附 10px 内既有顶点（默认开）",
        RibbonAction::ToggleEditSnap,
        editing,
    ),
    cmd(
        "edit_topo",
        "拓扑编辑",
        Icon::Link,
        "拓扑编辑开关",
        "开时顶点拖拽联动全部共享该顶点的要素（当前图层范围，一次撤销；默认关）",
        RibbonAction::ToggleEditTopo,
        editing,
    ),
    cmd(
        "edit_delete",
        "删除要素",
        Icon::Close,
        "删除要素",
        "点选要素后按 Delete 键或再次点击删除",
        RibbonAction::SetEditTool(crate::edit::EditTool::Delete),
        editing,
    ),
    cmd(
        "undo",
        "撤销",
        Icon::Reset,
        "撤销",
        "撤销一步编辑（编辑会话历史；快捷键 Ctrl+Z）",
        RibbonAction::Undo,
        can_undo,
    ),
    cmd(
        "redo",
        "重做",
        Icon::Play,
        "重做",
        "重做一步编辑（快捷键 Ctrl+Y / Ctrl+Shift+Z）",
        RibbonAction::Redo,
        can_redo,
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
    fn shortcut_matching() {
        use eframe::egui::Key;
        assert_eq!(match_shortcut(Key::Z, true, false), Some(AppShortcut::Undo));
        assert_eq!(match_shortcut(Key::Z, true, true), Some(AppShortcut::Redo));
        assert_eq!(match_shortcut(Key::Y, true, false), Some(AppShortcut::Redo));
        assert_eq!(
            match_shortcut(Key::S, true, false),
            Some(AppShortcut::SaveProject)
        );
        // 无修饰键/未登记键不命中。
        assert_eq!(match_shortcut(Key::Z, false, false), None);
        assert_eq!(match_shortcut(Key::A, true, false), None);
    }

    #[test]
    fn enabled_predicates() {
        let empty = AppSnapshot::default();
        let with_layers = AppSnapshot {
            layer_count: 2,
            has_selection: false,
            ..Default::default()
        };
        let with_sel = AppSnapshot {
            layer_count: 2,
            has_selection: true,
            ..Default::default()
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
