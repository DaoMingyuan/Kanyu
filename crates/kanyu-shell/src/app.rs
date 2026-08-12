//! 应用主体：Ribbon 功能区 + Contents/属性面板 + 独立终端 + MapCanvas +
//! 双主题 + 截图验证。全部界面由 [`crate::ui_kit`] 组件与各面板模块组合。
//!
//! 布局（ArcGIS Pro 式）：
//! ┌──────────────────────────────┐
//! │ Ribbon（页签 + 命令组，86px） │
//! ├─────────┬────────────┬───────┤
//! │ Contents│  MapCanvas │ 属性  │
//! │ 图层树  │            │ /技能 │
//! ├─────────┴────────────┴───────┤
//! │ 独立终端（可折叠，180px）      │
//! ├──────────────────────────────┤
//! │ StatusBar（28px）            │
//! └──────────────────────────────┘

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui;
use geojson::FeatureCollection;
use kanyu_core::{analysis, crs, Layer};
use kanyu_render::{collection_extent, render_png, render_svg, RenderOptions, StyleRule, Theme};
use kanyu_skill::{Skill, SkillHost};

use crate::canvas::MapCanvas;
use crate::console::{ConsoleHost, ConsolePanel, HELP_TEXT};
use crate::dialogs::{DialogResult, Dialogs};
use crate::panels::{self, LayerView, PanelAction, SkillView};
use crate::ribbon::{Ribbon, RibbonAction};
use crate::toc::{self, TocNode};
use crate::ui_kit::sizes;
use crate::view::{self, BBox};
use crate::ShellArgs;

/// 打开数据对话框的格式过滤器（与内核原生读能力对齐）。
const OPEN_EXTENSIONS: &[&str] = &[
    "shp", "geojson", "fgb", "parquet", "dxf", "dwg", "kml", "kmz", "csv", "tsv", "xlsx",
];

/// 已加载图层（含 UI 态：可见性、目录展开、符号化、来源路径）。
use crate::mapview::LayerEntry;

/// 地图色彩模式（界面主题与地图输出解耦）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MapThemeMode {
    /// 固定晨山（默认，保证制图输出正确）。
    FixedLight,
    /// 固定夜观星。
    FixedDark,
    /// 跟随界面主题。
    FollowUi,
}

impl MapThemeMode {
    /// 状态栏标签。
    pub fn label(self) -> &'static str {
        match self {
            MapThemeMode::FixedLight => "地图: 固定晨山",
            MapThemeMode::FixedDark => "地图: 固定夜观星",
            MapThemeMode::FollowUi => "地图: 跟随界面",
        }
    }
    /// 循环下一个模式。
    pub fn next(self) -> Self {
        match self {
            MapThemeMode::FixedLight => MapThemeMode::FixedDark,
            MapThemeMode::FixedDark => MapThemeMode::FollowUi,
            MapThemeMode::FollowUi => MapThemeMode::FixedLight,
        }
    }
    /// 序列化（.kyu）。
    pub fn as_str(self) -> &'static str {
        match self {
            MapThemeMode::FixedLight => "fixed_light",
            MapThemeMode::FixedDark => "fixed_dark",
            MapThemeMode::FollowUi => "follow_ui",
        }
    }
    /// 反序列化（.kyu；未知值回退默认）。
    pub fn parse(s: &str) -> Self {
        match s {
            "fixed_dark" => MapThemeMode::FixedDark,
            "follow_ui" => MapThemeMode::FollowUi,
            _ => MapThemeMode::FixedLight,
        }
    }
}

/// 截图验证模式状态机：等待 → 已请求 → 收到 `Event::Screenshot` 保存退出。
struct ScreenshotState {
    out_path: String,
    start: Instant,
    delay: Duration,
    requested: bool,
}

/// 重命名/新建组 模态对话框的目标（目录树右键「重命名…」「新建组…」）。
enum RenameTarget {
    /// 重命名图层（改显示名 file_name；id 不动——id 是树节点与操作的稳定身份）。
    Layer(String),
    /// 重命名组（值为组路径）。
    Group(String),
    /// 「移至分组 ▸ 新建组…」：建新组并把该图层移入（值为图层 id）。
    NewGroupForLayer(String),
}

/// 重命名/新建组 模态对话框状态（ui_kit::dialog_shell 采集）。
struct RenameState {
    target: RenameTarget,
    /// 名称输入框当前值。
    name: String,
}

/// 停靠编排输出（各停靠区/浮动窗内容渲染产生的动作，统一在布局后结算）。
#[derive(Default)]
struct DockOutputs {
    catalog: Vec<crate::catalog::CatalogAction>,
    panel: Vec<PanelAction>,
}

/// 图层属性页状态（ArcGIS Pro Layer Properties 范式）。
struct LayerPropsState {
    /// 目标图层 id。
    layer: String,
    /// 当前页（0 常规 / 1 源 / 2 字段 / 3 符号化）。
    page: usize,
    /// 常规：图层名（可改）。
    name: String,
    /// 常规：可见性。
    visible: bool,
    /// 符号化工作副本。
    sym: crate::symbology::LayerSymbology,
    /// 编辑缓冲：单色 hex。
    single_hex: String,
    /// 编辑缓冲：唯一值（值, hex 文本）。
    cat_texts: Vec<(String, String)>,
    /// 编辑缓冲：<其他> hex。
    other_hex: String,
    /// 编辑缓冲：分级断点（逗号分隔文本）。
    breaks_text: String,
    /// 校验错误（红字）。
    err: Option<String>,
}

/// 从符号化模型派生编辑缓冲（打开属性页时调用）。
fn sym_edit_buffers(
    sym: &crate::symbology::LayerSymbology,
) -> (String, Vec<(String, String)>, String, String) {
    use crate::symbology::{hex_of, LayerSymbology};
    match sym {
        LayerSymbology::Single { color } => {
            (hex_of(*color), Vec::new(), "#888888".into(), String::new())
        }
        LayerSymbology::Categorical { colors, other, .. } => (
            String::new(),
            colors
                .iter()
                .map(|(v, c)| (v.clone(), hex_of(*c)))
                .collect(),
            hex_of(*other),
            String::new(),
        ),
        LayerSymbology::Graduated { breaks, .. } => (
            String::new(),
            Vec::new(),
            "#888888".into(),
            breaks
                .iter()
                .map(|b| {
                    if b.fract() == 0.0 {
                        format!("{}", *b as i64)
                    } else {
                        format!("{b}")
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        ),
    }
}

/// 堪舆桌面壳层应用。
pub struct KanyuApp {
    layers: Vec<LayerEntry>,
    /// 目录树（Contents 窗格模型；节点以图层 id 引用 layers，见 toc.rs 约定）。
    toc: Vec<TocNode>,
    /// 属性面板选中的图层索引。
    selected: Option<usize>,
    /// 目录树选中的组路径（与 selected 互斥：点图层清组选，点组清图层选）。
    selected_group: Option<String>,
    /// 重命名/新建组 模态对话框状态。
    rename: Option<RenameState>,
    /// 编辑会话（同一时刻仅一个图层编辑态；kanyu-edit History）。
    edit_session: Option<crate::edit::EditSession>,
    theme: Theme,
    ribbon: Ribbon,
    console: ConsolePanel,
    dialogs: Dialogs,
    canvas: MapCanvas,
    /// 技能宿主与注册表。
    skill_host: SkillHost,
    skills: HashMap<String, Skill>,
    skill_metas: Vec<SkillView>,
    /// 当前视口（数据坐标 bbox；与画布同比例，view.rs 不变式）。
    view_bbox: Option<BBox>,
    needs_fit: bool,
    /// 可见图层合并缓存（仅在加载/可见性/增删时重建，平移缩放不重建）。
    merged: FeatureCollection,
    /// 逐图层渲染缓存（(id, 集合, 符号化)，目录树自下而上序；随 rebuild_merged 重建）。
    render_cache: Vec<(String, FeatureCollection, crate::symbology::LayerSymbology)>,
    data_extent: Option<BBox>,
    /// 地图导出设置（渲染设置对话框采集 → 现由「设置 → 渲染」编辑）。
    map_export_size: (u32, u32),
    map_export_style: Option<StyleRule>,
    /// 工程坐标系（保存进 .kyu；投影变换默认目标；状态栏显示）。
    project_crs: String,
    /// 界面缩放档位（egui zoom_factor；内存态，重启恢复 1.0）。
    ui_zoom: f32,
    /// 工具箱面板状态。
    toolbox: crate::toolbox::ToolboxPanel,
    /// 工具箱参数对话框状态。
    tool_run: Option<crate::toolbox::ToolRunState>,
    /// 工具后台执行句柄（进度模态 + 可终止）。
    tool_progress: Option<crate::toolbox::ToolProgress>,
    /// 设置对话框状态（坐标系/渲染）。
    settings: Option<crate::settings::SettingsDialog>,
    /// 属性表面板状态。
    attrtable: crate::attrtable::AttrTablePanel,
    /// 图层属性对话框状态（多页签：常规/源/字段/符号化）。
    layer_props: Option<LayerPropsState>,
    /// Toast 轻提示队列（右上角，自动消退）。
    toasts: Vec<crate::ui_kit::Toast>,
    /// UI 状态写盘防抖（变化时刻；1s 合并写一次）。
    state_dirty: Option<Instant>,
    /// 工具箱状态版本快照（收藏/最近变更检测）。
    last_toolbox_state: u64,
    /// ui-state.json 路径。
    state_path: std::path::PathBuf,
    /// 地图框清单（frames[0] = 默认主框「地图」，不可删除；休眠框状态驻留自身
    /// site，激活框状态平铺为本结构体的 layers/toc/render_cache/merged/
    /// data_extent/view_bbox/needs_fit/frame_dim/canvas/scene 字段——交换模型
    /// 见 mapview.rs 模块头）。
    frames: Vec<crate::mapview::MapFrame>,
    /// 地图框序号计数（「地图 N」/「场景 N」递增）。
    next_frame_id: usize,
    /// 当前激活地图框（frames 下标；None = 全部关闭，中央显示引导）。
    active_frame: Option<usize>,
    /// 激活框维度（休眠框的维度驻留 site.dim）。
    frame_dim: crate::mapview::ViewDim,
    /// 激活框三维场景态（休眠框的场景态驻留 site.scene）。
    scene: crate::scene3d::Scene3D,
    /// 重命名地图框对话框状态（frames 下标 + 新标题输入）。
    rename_frame_dlg: Option<(usize, String)>,
    /// 当前布局页签（Some 时优先于 active_frame；layouts 下标）。
    active_layout: Option<usize>,
    /// 布局视图清单（内存态；.kyu 持久化列入后续）。
    layouts: Vec<crate::layoutview::LayoutView>,
    /// 布局序号计数（「布局 N」递增）。
    next_layout_id: usize,
    /// 新建布局对话框状态。
    layout_dlg: Option<crate::layoutview::LayoutDialogState>,
    /// 服务链接清单（WFS；持久化入 ui-state.json）。
    services: Vec<crate::services::WfsConnection>,
    /// WMS 底图连接清单（持久化入 ui-state.json 的独立 wms 字段）。
    wms_services: Vec<crate::services::WmsConnection>,
    /// WMS 底图拉取句柄（后台线程 + 每帧轮询）。
    wms_fetch: Option<WmsFetch>,
    /// WMS 请求去抖（同 bbox 1 秒内不重复请求）。
    wms_last: Option<(Instant, [f64; 4])>,
    /// WMS 失败键（失败后视口/尺寸变化才重试，防每帧死循环）。
    wms_failed: Option<([f64; 4], u32, u32)>,
    /// 新建服务链接对话框状态。
    service_dlg: Option<crate::services::ServiceDialogState>,
    /// WFS 后台拉取句柄（进度模态 + 每帧轮询；复用工具运行模式）。
    service_progress: Option<ServiceFetch>,
    /// 渲染内容纪元（rebuild_merged 递增；布局地图缓存据此刻意重合成）。
    render_epoch: u64,
    /// 中央页签条矩形（浮动视图拖入吸附的投放区）。
    view_strip_rect: Option<egui::Rect>,
    /// 正在拖拽的浮动视图（frames 下标）。
    dragging_view: Option<usize>,
    error_msg: Option<String>,
    status: String,
    mouse_data: Option<(f64, f64)>,
    /// 停靠布局状态（每面板所在区/浮动/关闭 + 页签 + 拖拽中）。
    dock: crate::dock::DockState,
    /// 目录面板（Catalog 文件浏览）。
    catalog: crate::catalog::CatalogPanel,
    /// 图层筛选框（图层面板工具栏）。
    layer_filter: String,
    /// AI 对话面板。
    ai_chat: crate::ai::AiChatPanel,
    /// 地图色彩模式（默认固定晨山）。
    map_theme_mode: MapThemeMode,
    /// 终端切主题后置位，下一帧开头统一 apply_theme。
    theme_dirty: bool,
    /// 主题切换交叉淡化（旧主题底色 + 切换时刻；0.2s 渐隐遮罩）。
    theme_fade: Option<(Instant, egui::Color32)>,
    /// 「缩放到指定图层」的一次性适配范围（覆盖 data_extent，消费后清除）。
    fit_extent: Option<BBox>,
    /// 窗口截图（非退出）待保存路径。
    pending_window_shot: Option<String>,
    screenshot: Option<ScreenshotState>,
    /// 位图图标缓存（ArcGIS Pro 本机资源；缺图自动回退手绘线性图标）。
    icon_cache: crate::ui_kit::icons::IconCache,
}

/// WFS 后台拉取句柄（app 持有，每帧轮询；取消 = 析构接收端丢弃结果）。
struct ServiceFetch {
    /// 连接名（结果图层 file_name / 消息用）。
    name: String,
    /// 结果通道。
    rx: std::sync::mpsc::Receiver<Result<FeatureCollection, String>>,
}

/// WMS 底图后台拉取句柄（每帧轮询；视口已变的过期结果直接丢弃）。
struct WmsFetch {
    /// 缓存键（请求时的视口 bbox + 物理像素尺寸）。
    key: ([f64; 4], u32, u32),
    /// 结果通道。
    rx: std::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
}

/// 裸几何 → 无属性要素（编辑插入用）。
fn bare_feature(value: geojson::Value) -> geojson::Feature {
    geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(value)),
        id: None,
        properties: None,
        foreign_members: None,
    }
}

/// 渲染缓存 → 画布切片（目录树自下而上序；与 rebuild_merged 同一顺序约定）。
/// 移自本模块的薄封装：实现见 mapview（地图框绑定图层集后归属彼处）。
use crate::mapview::build_layer_slices;

impl KanyuApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: ShellArgs) -> Self {
        crate::theme::load_cjk_font(&cc.egui_ctx);
        crate::theme::apply_theme(&cc.egui_ctx, args.theme);
        let screenshot = args.screenshot.map(|out_path| ScreenshotState {
            out_path,
            start: Instant::now(),
            delay: Duration::from_secs_f64(args.delay_secs.max(0.0)),
            requested: false,
        });
        let mut app = Self {
            layers: Vec::new(),
            toc: Vec::new(),
            selected: None,
            selected_group: None,
            rename: None,
            edit_session: None,
            theme: args.theme,
            ribbon: Ribbon::default(),
            console: ConsolePanel::default(),
            dialogs: Dialogs::default(),
            canvas: MapCanvas::default(),
            skill_host: SkillHost::new().expect("wasmtime 引擎初始化失败（极少见）"),
            skills: HashMap::new(),
            skill_metas: Vec::new(),
            view_bbox: None,
            needs_fit: false,
            merged: FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
            render_cache: Vec::new(),
            data_extent: None,
            map_export_size: (1200, 800),
            map_export_style: None,
            project_crs: "EPSG:4326".to_string(),
            ui_zoom: 1.0,
            toolbox: crate::toolbox::ToolboxPanel::default(),
            tool_run: None,
            tool_progress: None,
            settings: None,
            attrtable: crate::attrtable::AttrTablePanel::default(),
            layer_props: None,
            toasts: Vec::new(),
            state_dirty: None,
            last_toolbox_state: 0,
            state_path: crate::uistate::state_path(),
            frames: vec![crate::mapview::MapFrame::main()],
            next_frame_id: 1,
            active_frame: Some(0),
            frame_dim: crate::mapview::ViewDim::TwoD,
            scene: crate::scene3d::Scene3D::default(),
            rename_frame_dlg: None,
            active_layout: None,
            layouts: Vec::new(),
            next_layout_id: 1,
            layout_dlg: None,
            services: Vec::new(),
            wms_services: Vec::new(),
            wms_fetch: None,
            wms_last: None,
            wms_failed: None,
            service_dlg: None,
            service_progress: None,
            render_epoch: 1,
            view_strip_rect: None,
            dragging_view: None,
            error_msg: None,
            status: "就绪".to_string(),
            mouse_data: None,
            dock: crate::dock::DockState::default(),
            catalog: crate::catalog::CatalogPanel::default(),
            layer_filter: String::new(),
            ai_chat: crate::ai::AiChatPanel::default(),
            map_theme_mode: MapThemeMode::FixedLight,
            pending_window_shot: None,
            screenshot,
            theme_dirty: false,
            theme_fade: None,
            fit_extent: None,
            icon_cache: crate::ui_kit::icons::IconCache::default(),
        };
        // UI 状态恢复（ui-state.json；坏文件/版本不符自动回退默认）。
        // 先恢复再应用演示参数（演示预设覆盖在恢复态之上）。
        {
            let s = crate::uistate::UiState::load(&app.state_path);
            app.apply_ui_state(&s, &cc.egui_ctx);
        }
        // --dock-demo（隐藏验证参数）：预设「右区停靠 + 浮动窗 + 已关闭」布局。
        if args.dock_demo {
            use crate::dock::{DockZone, PanelId};
            app.dock.dock_to(PanelId::Toolbox, DockZone::Right);
            app.dock.dock_to(PanelId::AiChat, DockZone::Right);
            app.dock.set_active(DockZone::Right, PanelId::Toolbox);
            app.dock.float(PanelId::Console);
            app.dock.close_panel(PanelId::Catalog);
            // 预置工具箱「最近使用/收藏」演示数据（面板区块截图验证）。
            app.toolbox.note_run("centroid");
            app.toolbox.note_run("buffer");
            app.toolbox.toggle_favorite("dissolve");
            app.toolbox.toggle_favorite("buffer");
        }
        // --open-settings / --tool-demo（隐藏验证参数）：预设对话框打开态。
        if args.open_settings {
            app.open_settings();
        }
        if args.tool_demo {
            if let Some(def) = crate::toolbox::find("buffer") {
                let mut st = crate::toolbox::ToolRunState::new(def);
                // 预填「0|米」演示校验分级（警告不阻断运行）。
                if let Some(v) = st.values.get_mut(1) {
                    *v = "0|米".to_string();
                }
                app.tool_run = Some(st);
            }
        }
        // --load 可多次指定；.kyu 走工程恢复，其余走数据加载。
        // frames/layout-bind 演示系列：load[1] 起由演示预置接管（载入新建框）。
        let frames_demo = args.frames_demo || args.frames_demo2 || args.frames_demo3;
        let split_load = frames_demo || args.layout_bind_demo;
        for (i, path) in args.load.iter().enumerate() {
            if split_load && i > 0 {
                continue;
            }
            let p = Path::new(path);
            if p.extension()
                .map(|e| e.eq_ignore_ascii_case("kyu"))
                .unwrap_or(false)
            {
                app.open_project(p);
            } else {
                app.open_file(p);
            }
        }
        // dock-demo 附加：属性表打开态（选中首个图层，截图验证）。
        if args.dock_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                app.dock.open_panel(crate::dock::PanelId::AttrTable);
                app.attrtable.set_layer(id);
            }
        }
        // calc-demo：属性表 + 字段计算器对话框（预填示例与预览，截图验证）。
        if args.calc_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                app.dock.open_panel(crate::dock::PanelId::AttrTable);
                app.attrtable.set_layer(id.clone());
                let preview = crate::attrtable::preview_calc(
                    Some(&first.layer.collection()),
                    "楼高米",
                    "height * 3.5",
                    5,
                );
                app.attrtable.demo_open_calc(preview);
            }
        }
        // view-demo：吸附「地图 2」（二维，自有图层集）+ 浮动「场景 3」（三维）。
        if args.view_demo {
            let v2 = crate::mapview::MapFrame::new(
                app.next_frame_id,
                format!("地图 {}", app.next_frame_id),
                crate::mapview::ViewDim::TwoD,
            );
            app.next_frame_id += 1;
            app.frames.push(v2);
            let mut v3 = crate::mapview::MapFrame::new(
                app.next_frame_id,
                format!("场景 {}", app.next_frame_id),
                crate::mapview::ViewDim::ThreeD,
            );
            app.next_frame_id += 1;
            v3.docked = false;
            app.frames.push(v3);
        }
        // frames-demo 系列公共前置：清掉 ui-state 恢复的额外框（确定性演示），
        // 当前平铺现场（含 load[0]）直接划归主框；load[1] 载入新建三维场景框。
        if frames_demo {
            app.frames.truncate(1); // 丢恢复框（主框休眠位随之废弃）
            app.active_frame = Some(0); // 平铺现场即主框现场
            app.frame_dim = crate::mapview::ViewDim::TwoD; // 主框归位二维
            app.scene = crate::scene3d::Scene3D::default();
            app.next_frame_id = 1;
            if let Some(second) = args.load.get(1).cloned() {
                app.new_frame(crate::mapview::ViewDim::ThreeD);
                app.open_file(Path::new(&second));
            }
        }
        // --frames-demo：双框各自图层集（load[0]→主框，load[1]→三维场景框），
        // 末态激活主框 + 图层面板（切换联动图层面板截图验证）。
        if args.frames_demo {
            app.activate_frame(0);
            app.dock
                .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Layers);
        }
        // --frames-demo2：场景框关闭（目录保留弱色行截图验证关闭≠删除）。
        if args.frames_demo2 {
            app.activate_frame(0);
            app.close_frame(1);
            app.catalog.demo_expand_frames();
            app.dock
                .dock_to(crate::dock::PanelId::Catalog, crate::dock::DockZone::Left);
            app.dock
                .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Catalog);
        }
        // --frames-demo3：末态激活场景框（三维 + 自有图层集截图验证三维独立建立）。
        if args.frames_demo3 {
            app.dock
                .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Layers);
        }
        // --layout-bind-demo：布局绑定场景框——主框 load[0]、场景框 load[1]，
        // 布局「示范区总图」绑定场景框后激活主框（绑定不随激活切换的截图验证）。
        if args.layout_bind_demo {
            app.frames.truncate(1); // 演示确定性（同 frames-demo 归一化）
            app.active_frame = Some(0);
            app.frame_dim = crate::mapview::ViewDim::TwoD;
            app.scene = crate::scene3d::Scene3D::default();
            app.next_frame_id = 1;
            if let Some(second) = args.load.get(1).cloned() {
                app.new_frame(crate::mapview::ViewDim::ThreeD);
                app.open_file(Path::new(&second));
                app.activate_frame(0);
                let scene_title = app.frames[1].title.clone();
                let id = app.next_layout_id;
                app.next_layout_id += 1;
                let mut lv = crate::layoutview::LayoutView::new(
                    id,
                    "示范区总图".to_string(),
                    kanyu_render::layout::LayoutSpec {
                        title: "示范区总图".to_string(),
                        ..Default::default()
                    },
                );
                lv.map = scene_title;
                app.layouts.push(lv);
                app.active_layout = Some(app.layouts.len() - 1);
                app.catalog.demo_expand_layouts();
                app.dock
                    .dock_to(crate::dock::PanelId::Catalog, crate::dock::DockZone::Left);
                app.dock
                    .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Catalog);
            }
        }
        // --zoom：启动缩放档（截图验证等比缩放）。
        if let Some(z) = args.zoom {
            cc.egui_ctx.set_zoom_factor(z);
            app.ui_zoom = z;
        }
        // --dock-demo2：错位停靠（属性表→窄右区、工具箱→宽底区，回流验证）。
        if args.dock_demo2 {
            use crate::dock::{DockZone, PanelId};
            app.dock.dock_to(PanelId::AttrTable, DockZone::Right);
            app.dock.dock_to(PanelId::Toolbox, DockZone::Bottom);
            app.dock.set_active(DockZone::Right, PanelId::AttrTable);
            app.dock.set_active(DockZone::Bottom, PanelId::Toolbox);
            app.dock.float(PanelId::Console);
            app.dock.close_panel(PanelId::Catalog);
            if let Some(first) = app.layers.first() {
                app.attrtable.set_layer(first.layer.id().to_string());
            }
        }
        // --catalog-demo：目录面板深层展开（截图验证滚动）。
        if args.catalog_demo {
            app.catalog.demo_expand_first();
            app.dock
                .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Catalog);
        }
        // --expand-layers：展开全部图层节点（符号化分类行截图验证）。
        if args.expand_layers {
            for e in &mut app.layers {
                e.expanded = true;
            }
        }
        // --layout-demo：创建并激活布局页签（截图验证）。
        if args.layout_demo {
            let id = app.next_layout_id;
            app.next_layout_id += 1;
            let spec = kanyu_render::layout::LayoutSpec {
                title: "示范区总图".to_string(),
                ..Default::default()
            };
            app.layouts.push(crate::layoutview::LayoutView::new(
                id,
                "示范区总图".to_string(),
                spec,
            ));
            app.active_layout = Some(app.layouts.len() - 1);
        }
        // --props-demo：打开首图层属性页（直达符号化页，截图验证）。
        if args.props_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                app.open_layer_props(&id);
                if let Some(st) = &mut app.layer_props {
                    st.page = 3;
                }
            }
        }
        // --edit-demo：编辑会话预设（顶点工具 + 选中要素 #1 高亮，截图验证句柄/高亮）。
        if args.edit_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                let name = first.file_name.clone();
                let n = first.layer.collection().features.len();
                let mut s = crate::edit::EditSession::new(id, name);
                s.tool = crate::edit::EditTool::Vertex;
                s.selected = (n > 1).then_some(1); // 选中次要素（accent 描边高亮）
                app.edit_session = Some(s);
            }
        }
        // --draw-demo：添加面绘制中态（预置 3 顶点 + 演示光标橡皮筋，截图验证）。
        if args.draw_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                let name = first.file_name.clone();
                let mut s = crate::edit::EditSession::new(id, name);
                s.tool = crate::edit::EditTool::AddPolygon;
                if let Some(ext) = app.data_extent {
                    let cx = f64::midpoint(ext[0], ext[2]);
                    let cy = f64::midpoint(ext[1], ext[3]);
                    let sx = (ext[2] - ext[0]).max(1e-9);
                    let sy = (ext[3] - ext[1]).max(1e-9);
                    let mut d = crate::edit::DrawState::new(crate::edit::DrawKind::Polygon);
                    d.add((cx - 0.15 * sx, cy - 0.10 * sy));
                    d.add((cx + 0.05 * sx, cy - 0.20 * sy));
                    d.add((cx + 0.15 * sx, cy + 0.05 * sy));
                    s.drawing = Some(d);
                    // 演示光标（截图无真实悬停时的橡皮筋终点）。
                    app.canvas.demo_sketch_cursor = Some((cx - 0.02 * sx, cy + 0.18 * sy));
                }
                app.edit_session = Some(s);
            }
        }
        // --snap-demo：顶点捕捉指示态（绘制中 + 演示光标贴近既有顶点 → 吸附圆环）。
        if args.snap_demo {
            if let Some(first) = app.layers.first() {
                let id = first.layer.id().to_string();
                let name = first.file_name.clone();
                // 首要素首个顶点（数据坐标；演示光标贴到它附近触发捕捉）。
                let first_vertex = first
                    .layer
                    .collection()
                    .features
                    .first()
                    .and_then(|f| f.geometry.as_ref())
                    .and_then(|g| match &g.value {
                        geojson::Value::Polygon(rings) => {
                            rings.first().and_then(|r| r.first()).cloned()
                        }
                        geojson::Value::LineString(l) => l.first().cloned(),
                        geojson::Value::Point(p) => Some(p.clone()),
                        _ => None,
                    });
                let mut s = crate::edit::EditSession::new(id, name);
                s.tool = crate::edit::EditTool::AddPolygon;
                if let (Some(ext), Some(v0)) = (app.data_extent, first_vertex) {
                    let cx = f64::midpoint(ext[0], ext[2]);
                    let cy = f64::midpoint(ext[1], ext[3]);
                    let sx = (ext[2] - ext[0]).max(1e-9);
                    let sy = (ext[3] - ext[1]).max(1e-9);
                    let mut d = crate::edit::DrawState::new(crate::edit::DrawKind::Polygon);
                    d.add((cx - 0.10 * sx, cy + 0.05 * sy));
                    d.add((cx + 0.02 * sx, cy + 0.12 * sy));
                    s.drawing = Some(d);
                    // 演示光标：既有顶点旁 ~0.4% 跨度（1000px 画布 ≈4px，容差内）。
                    app.canvas.demo_sketch_cursor = Some((v0[0] + 0.004 * sx, v0[1] + 0.004 * sy));
                }
                app.edit_session = Some(s);
            }
        }
        // --service-demo：目录「服务链接」预置 WFS+WMS 演示连接各一并展开分类（截图验证）；
        // --service-dlg-demo 另打开新建对话框（预填图层清单，图层发现截图验证）；
        // --service-edit-demo 打开编辑对话框（WFS 连接回填态截图验证）。
        if args.service_demo || args.service_dlg_demo || args.service_edit_demo {
            // 覆盖式预置（非追加）：持久化恢复后重复演示不累积重复行。
            app.services = vec![crate::services::WfsConnection {
                name: "示例 WFS（演示）".to_string(),
                url: "https://example.com/geoserver/wfs?service=WFS&request=GetFeature\
                      &typeNames=demo:blocks&outputFormat=application/json"
                    .to_string(),
            }];
            app.wms_services = vec![crate::services::WmsConnection {
                name: "示例 WMS 底图（演示）".to_string(),
                url: "https://example.com/geoserver/wms".to_string(),
                layer: "ne:countries".to_string(),
            }];
            app.catalog.demo_expand_services();
            // 演示布局确定性：目录停靠左区并激活；关闭浮动终端避免遮挡。
            app.dock
                .dock_to(crate::dock::PanelId::Catalog, crate::dock::DockZone::Left);
            app.dock
                .set_active(crate::dock::DockZone::Left, crate::dock::PanelId::Catalog);
            app.dock.close_panel(crate::dock::PanelId::Console);
            if args.service_dlg_demo {
                app.service_dlg = Some(crate::services::ServiceDialogState {
                    name: "示范 WFS".to_string(),
                    url: "https://example.com/geoserver/wfs".to_string(),
                    layer: "demo:blocks".to_string(),
                    // 预填图层清单（离线演示图层发现下拉）。
                    caps: vec![
                        crate::services::WfsLayerInfo {
                            name: "demo:blocks".to_string(),
                            title: Some("示范街区".to_string()),
                        },
                        crate::services::WfsLayerInfo {
                            name: "demo:roads".to_string(),
                            title: Some("示范道路".to_string()),
                        },
                    ],
                    caps_note: Some("已发现 2 个图层".to_string()),
                    ..Default::default()
                });
            }
            if args.service_edit_demo {
                // 编辑回填态（WFS 连接：完整 URL 拆解为基址 + 图层）。
                let (base, layer) = crate::services::split_getfeature_url(&app.services[0].url);
                app.service_dlg = Some(crate::services::ServiceDialogState {
                    kind: crate::services::ServiceKind::Wfs,
                    name: app.services[0].name.clone(),
                    url: base,
                    layer,
                    editing: Some(crate::services::ServiceEditTarget {
                        kind: crate::services::ServiceKind::Wfs,
                        index: 0,
                    }),
                    ..Default::default()
                });
            }
        }
        app
    }

    // ===== 图层与数据现场 =====

    /// 应用 UI 状态（ui-state.json 恢复；逐项容错）。
    fn apply_ui_state(&mut self, s: &crate::uistate::UiState, ctx: &egui::Context) {
        use crate::dock::{DockZone, PanelId};
        // 停靠布局。
        for p in &s.panels {
            if let Some(id) = PanelId::from_key(&p.id) {
                self.dock
                    .restore_panel(id, crate::dock::DockState::zone_from_key(&p.zone), p.open);
            }
        }
        for (i, tab) in s.active_tabs.iter().enumerate() {
            if i >= DockZone::DOCKED.len() {
                break;
            }
            if let Some(key) = tab {
                if let Some(id) = PanelId::from_key(key) {
                    self.dock
                        .set_active(crate::dock::DockState::zone_by_index(i), id);
                }
            }
        }
        // 工具箱收藏/最近。
        self.toolbox
            .restore(&s.toolbox_favorites, &s.toolbox_recent);
        self.last_toolbox_state = self.toolbox.state_version();
        // 设置。
        if s.ui_zoom > 0.0 {
            self.ui_zoom = s.ui_zoom;
            ctx.set_zoom_factor(s.ui_zoom);
        }
        if !s.map_theme.is_empty() {
            self.map_theme_mode = MapThemeMode::parse(&s.map_theme);
        }
        if !s.project_crs.is_empty() {
            self.project_crs = s.project_crs.clone();
        }
        // 服务链接清单（先恢复——地图框的 wms_base 按名匹配本机清单）。
        self.services = s.services.clone();
        self.wms_services = s.wms.clone();
        // 地图框清单（不含主框——恒在；恢复为休眠框，图层集由 .kyu 工程恢复，
        // ui-state 只记壳层现场）。
        for v in &s.views {
            let id = self.next_frame_id;
            self.next_frame_id += 1;
            let mut f = crate::mapview::MapFrame::new(
                id,
                v.title.clone(),
                crate::mapview::ViewDim::parse(&v.dim),
            );
            f.docked = v.docked;
            f.open = v.open;
            f.site.view_bbox = v.bbox;
            f.site.needs_fit = v.bbox.is_none();
            f.wms_base = self.restore_wms_base(v.wms_base.as_deref(), &v.title);
            self.frames.push(f);
        }
        if let Some(a) = s.active_view {
            // 清单下标 → frames 下标（+1 跳过主框）。
            let i = a + 1;
            if i < self.frames.len() && self.frames[i].docked && self.frames[i].open {
                self.activate_frame(i);
            }
        }
    }

    /// 采集当前 UI 状态（保存用快照）。
    fn collect_ui_state(&self) -> crate::uistate::UiState {
        use crate::dock::{DockState, PanelId};
        let mut s = crate::uistate::UiState::new();
        s.panels = PanelId::ALL
            .iter()
            .map(|&id| crate::uistate::PanelStateJson {
                id: id.key().to_string(),
                zone: DockState::zone_key(self.dock.zone_of(id)).to_string(),
                open: self.dock.is_open(id),
            })
            .collect();
        s.active_tabs = (0..3)
            .map(|i| {
                self.dock
                    .active_in(DockState::zone_by_index(i))
                    .map(|p| p.key().to_string())
            })
            .collect();
        s.toolbox_favorites = self.toolbox.favorites_snapshot();
        s.toolbox_recent = self.toolbox.recent_snapshot();
        s.ui_zoom = self.ui_zoom;
        s.map_theme = self.map_theme_mode.as_str().to_string();
        s.project_crs = self.project_crs.clone();
        s.views = self
            .frames
            .iter()
            .enumerate()
            .skip(1) // 主框恒在，不入清单
            .map(|(i, f)| {
                let active = self.active_frame == Some(i);
                crate::uistate::ViewStateJson {
                    title: f.title.clone(),
                    dim: (if active { self.frame_dim } else { f.site.dim })
                        .as_str()
                        .to_string(),
                    bbox: if active {
                        self.view_bbox
                    } else {
                        f.site.view_bbox
                    },
                    docked: f.docked,
                    open: f.open,
                    wms_base: f.wms_base.clone(),
                }
            })
            .collect();
        s.active_view = match self.active_frame {
            Some(i) if i > 0 => Some(i - 1), // frames 下标 → 清单下标
            _ => None,
        };
        s.services = self.services.clone();
        s.wms = self.wms_services.clone();
        s
    }

    /// 标记 UI 状态已变更（防抖 1s 合并写盘）。
    fn mark_state_dirty(&mut self) {
        self.state_dirty = Some(Instant::now());
    }

    /// 防抖写盘判定（每帧末尾调用）。
    fn flush_ui_state_if_due(&mut self) {
        if let Some(t) = self.state_dirty {
            if t.elapsed() >= Duration::from_secs(1) {
                self.state_dirty = None;
                self.collect_ui_state().save(&self.state_path);
            }
        }
    }

    // ===== 地图框（交换模型：激活框状态平铺本结构体字段，休眠框驻留自身 site） =====

    /// 确保存在激活框（全部关闭时重开主框——打开文件/结果图层等操作的前提）。
    fn ensure_active_frame(&mut self) {
        if self.active_frame.is_none() {
            self.activate_frame(0); // 主框恒在
        }
    }

    /// 休眠当前激活框：平铺状态逐项搬回框内 site。
    fn park_frame(&mut self, cur: usize) {
        let f = &mut self.frames[cur];
        f.site.layers = std::mem::take(&mut self.layers);
        f.site.toc = std::mem::take(&mut self.toc);
        f.site.render_cache = std::mem::take(&mut self.render_cache);
        f.site.merged = std::mem::replace(
            &mut self.merged,
            FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
        );
        f.site.data_extent = self.data_extent.take();
        f.site.view_bbox = self.view_bbox.take();
        f.site.needs_fit = self.needs_fit;
        f.site.dim = self.frame_dim;
        std::mem::swap(&mut f.site.canvas, &mut self.canvas);
        std::mem::swap(&mut f.site.scene, &mut self.scene);
    }

    /// 取指定框现场到平铺字段（park 的逆操作）。
    fn unpark_frame(&mut self, idx: usize) {
        let f = &mut self.frames[idx];
        self.layers = std::mem::take(&mut f.site.layers);
        self.toc = std::mem::take(&mut f.site.toc);
        self.render_cache = std::mem::take(&mut f.site.render_cache);
        self.merged = std::mem::replace(
            &mut f.site.merged,
            FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
        );
        self.data_extent = f.site.data_extent.take();
        self.view_bbox = f.site.view_bbox.take();
        self.needs_fit = f.site.needs_fit;
        self.frame_dim = f.site.dim;
        std::mem::swap(&mut f.site.canvas, &mut self.canvas);
        std::mem::swap(&mut f.site.scene, &mut self.scene);
    }

    /// 激活地图框（目录行/页签/新建共用；已关闭则重开、浮动则吸附）。
    /// 编辑会话进行中阻止切换（会话图层集属于当前框）。
    fn activate_frame(&mut self, idx: usize) {
        if idx >= self.frames.len() {
            return;
        }
        if self.active_frame == Some(idx) {
            self.frames[idx].open = true;
            self.frames[idx].docked = true;
            self.active_layout = None;
            return;
        }
        if self.edit_session.is_some() {
            self.toast_err("编辑会话进行中——请先保存或放弃编辑再切换地图框");
            return;
        }
        if let Some(cur) = self.active_frame {
            self.park_frame(cur);
        }
        self.active_frame = Some(idx);
        self.active_layout = None;
        self.frames[idx].open = true;
        self.frames[idx].docked = true;
        self.unpark_frame(idx);
        // 选中态指向旧框图层：清空防悬空（属性表面板对未知 id 显示空态）。
        self.selected = None;
        self.selected_group = None;
        // 布局地图缓存按 render_epoch 重合成：换框即换内容，纪元递增防陈旧。
        self.render_epoch += 1;
        self.status = format!("当前地图框 → {}", self.frames[idx].title);
        self.mark_state_dirty();
    }

    /// 关闭地图框（≠ 删除：目录清单保留，双击行重开）。
    fn close_frame(&mut self, idx: usize) {
        if idx >= self.frames.len() {
            return;
        }
        if self.active_frame == Some(idx) && self.edit_session.is_some() {
            self.toast_err("编辑会话进行中——请先保存或放弃编辑再关闭地图框");
            return;
        }
        self.frames[idx].open = false;
        if self.active_frame == Some(idx) {
            self.park_frame(idx);
            self.active_frame = None;
            self.selected = None;
            self.selected_group = None;
        }
        let title = self.frames[idx].title.clone();
        self.console
            .info(format!("已关闭地图框「{title}」（目录中保留，双击行重开）"));
        self.mark_state_dirty();
    }

    /// 删除地图框（仅目录右键；主框不可删——主框唯一特权；最后一框不可删）。
    fn delete_frame(&mut self, idx: usize) {
        if idx >= self.frames.len() {
            return;
        }
        if idx == 0 {
            self.toast_err("默认地图框「地图」不可删除");
            return;
        }
        if self.frames.len() <= 1 {
            self.toast_err("至少保留一个地图框");
            return;
        }
        if self.edit_session.is_some() {
            self.toast_err("编辑会话进行中——请先保存或放弃编辑再删除地图框");
            return;
        }
        let n_layers = if self.active_frame == Some(idx) {
            self.layers.len()
        } else {
            self.frames[idx].site.layers.len()
        };
        let f = self.frames.remove(idx);
        if self.active_frame == Some(idx) {
            // 删的就是激活框：平铺状态随之废弃（unpark 主框时整体覆盖），回落主框。
            self.active_frame = None;
            self.activate_frame(0);
        } else {
            self.active_frame = crate::mapview::adjust_active_after_remove(self.active_frame, idx);
        }
        self.console.info(format!(
            "已删除地图框「{}」（含 {n_layers} 个图层）",
            f.title
        ));
        self.mark_state_dirty();
    }

    /// WMS 底图恢复（.kyu 只存连接名，定义在本机 ui-state——跨机取舍见 project.rs）：
    /// 名在本机 wms 清单中才生效，失配中文提示并忽略。
    fn restore_wms_base(&mut self, name: Option<&str>, frame_title: &str) -> Option<String> {
        let name = name?;
        if self.wms_services.iter().any(|c| c.name == name) {
            Some(name.to_string())
        } else {
            self.console.info(format!(
                "地图框「{frame_title}」的底图连接「{name}」不在本机 WMS 清单——已忽略"
            ));
            None
        }
    }

    /// 新建地图框（二维/三维分开建立，创建即激活）。
    fn new_frame(&mut self, dim: crate::mapview::ViewDim) {
        if self.edit_session.is_some() {
            self.toast_err("编辑会话进行中——请先保存或放弃编辑再新建地图框");
            return;
        }
        let id = self.next_frame_id;
        self.next_frame_id += 1;
        let title = match dim {
            crate::mapview::ViewDim::TwoD => format!("地图 {id}"),
            crate::mapview::ViewDim::ThreeD => format!("场景 {id}"),
        };
        self.frames
            .push(crate::mapview::MapFrame::new(id, title.clone(), dim));
        self.console
            .info(format!("已新建{}「{title}」", dim.create_label()));
        self.activate_frame(self.frames.len() - 1);
    }

    /// 指定地图框的现场部件（图层集/渲染缓存/视口/数据范围；激活框读平铺字段，
    /// 休眠框读自身 site——交换模型）。
    #[allow(clippy::type_complexity)]
    fn frame_parts(
        &self,
        idx: usize,
    ) -> (
        &[LayerEntry],
        &[(String, FeatureCollection, crate::symbology::LayerSymbology)],
        Option<BBox>,
        Option<BBox>,
    ) {
        if self.active_frame == Some(idx) {
            (
                &self.layers,
                &self.render_cache,
                self.view_bbox,
                self.data_extent,
            )
        } else {
            let s = &self.frames[idx].site;
            (&s.layers, &s.render_cache, s.view_bbox, s.data_extent)
        }
    }

    /// 布局内容源（绑定框标题 → 框下标；空串/未匹配 = 跟随激活框）。
    #[allow(clippy::type_complexity)]
    fn layout_frame_parts(
        &self,
        map: &str,
    ) -> (
        &[LayerEntry],
        &[(String, FeatureCollection, crate::symbology::LayerSymbology)],
        Option<BBox>,
        Option<BBox>,
    ) {
        let idx = if map.is_empty() {
            None
        } else {
            self.frames.iter().position(|f| f.title == map)
        };
        let idx = idx.or(self.active_frame).unwrap_or(0);
        self.frame_parts(idx)
    }

    // ===== 图层与数据现场 =====

    /// 生成不重复图层 id。
    fn unique_id(&self, base: &str) -> String {
        if !self.layers.iter().any(|e| e.layer.id() == base) {
            return base.to_string();
        }
        for n in 2.. {
            let candidate = format!("{base}_{n}");
            if !self.layers.iter().any(|e| e.layer.id() == candidate) {
                return candidate;
            }
        }
        unreachable!()
    }

    /// 加载数据文件为一个图层；失败置中文错误模态框。
    fn open_file(&mut self, path: &Path) {
        self.ensure_active_frame(); // 图层归属当前激活地图框
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "layer".to_string());
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| id.clone());
        let path_str = path.to_string_lossy();
        match Layer::load(self.unique_id(&id), &path_str) {
            Ok(layer) => {
                let summary = layer.summary();
                let msg = format!(
                    "已加载 {file_name}（{} 要素，{}）",
                    summary.feature_count, summary.format
                );
                self.status = msg.clone();
                self.console.info(msg.clone());
                self.toast_ok(msg);
                let id = layer.id().to_string();
                let symbology = crate::symbology::default_single(&summary.geometry_types);
                self.layers.push(LayerEntry {
                    layer,
                    summary,
                    visible: true,
                    file_name,
                    expanded: false,
                    source_path: Some(path_str.to_string()),
                    symbology,
                });
                // 新图层插入目录树顶（ArcGIS 约定：最新加载在最上方/最上层）。
                toc::insert_layer_top(&mut self.toc, &id);
                self.selected = Some(self.layers.len() - 1);
                self.rebuild_merged();
                self.needs_fit = true;
            }
            Err(e) => {
                let msg = format!("无法打开 {file_name}: {e}");
                self.console.push(crate::console::LineKind::Err, &msg);
                self.toast_err(&msg);
                self.error_msg = Some(msg);
            }
        }
    }

    /// 从内核结果集合登记新图层（分析/查询/技能产出）。
    fn add_result_layer(
        &mut self,
        base_id: &str,
        collection: FeatureCollection,
        verb: &str,
    ) -> String {
        self.ensure_active_frame(); // 结果图层归属当前激活地图框
        let id = self.unique_id(base_id);
        let n = collection.features.len();
        let layer = Layer::from_collection(id.clone(), collection);
        let summary = layer.summary();
        let symbology = crate::symbology::default_single(&summary.geometry_types);
        self.layers.push(LayerEntry {
            layer,
            summary,
            visible: true,
            file_name: id.clone(),
            expanded: false,
            source_path: None,
            symbology,
        });
        // 结果图层同样插入目录树顶。
        toc::insert_layer_top(&mut self.toc, &id);
        self.selected = Some(self.layers.len() - 1);
        self.rebuild_merged();
        let msg = format!("{verb} → 新图层 {id}（{n} 要素）");
        self.status = msg.clone();
        msg
    }

    /// 重建有效可见图层合并缓存与数据范围。
    ///
    /// 约定（见 toc.rs）：
    /// - **有效可见性** = 图层自身 visible 且所有祖先组 visible；
    /// - **渲染顺序** = 目录树自下而上——树底图层先绘制，树顶图层最后压入
    ///   要素流、绘制在最上层。
    fn rebuild_merged(&mut self) {
        let mut order = toc::visible_draw_order(&self.toc, |id| {
            self.layers
                .iter()
                .find(|e| e.layer.id() == id)
                .map(|e| e.visible)
                .unwrap_or(false)
        });
        // 防御：不在目录树中的可见图层（正常不会发生）补在绘制队列尾部（最上层）。
        for e in &self.layers {
            if e.visible && !order.iter().any(|id| id == e.layer.id()) {
                order.push(e.layer.id().to_string());
            }
        }
        let mut features = Vec::new();
        let mut extents = Vec::new();
        // 逐图层渲染缓存（id + 集合 + 符号化）：画布按层叠图用。
        let mut render_cache = Vec::new();
        for id in &order {
            if let Some(entry) = self.layers.iter().find(|e| e.layer.id() == id) {
                let collection = entry.layer.collection();
                if let Ok(Some(ext)) = collection_extent(&collection) {
                    extents.push(ext);
                }
                render_cache.push((id.clone(), collection.clone(), entry.symbology.clone()));
                features.extend(collection.features);
            }
        }
        self.render_cache = render_cache;
        self.merged = FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        };
        self.data_extent = view::union(extents);
        self.canvas.dirty = true;
        self.render_epoch += 1;
        // 休眠地图框各有自有图层集与渲染缓存，不受本框重建影响（无需同脏）。
    }

    /// 有效可见图层的要素总数（状态栏）。
    fn visible_feature_count(&self) -> usize {
        let order = toc::visible_draw_order(&self.toc, |id| {
            self.layers
                .iter()
                .find(|e| e.layer.id() == id)
                .map(|e| e.visible)
                .unwrap_or(false)
        });
        order
            .iter()
            .filter_map(|id| self.layers.iter().find(|e| e.layer.id() == id))
            .map(|e| e.summary.feature_count)
            .sum()
    }

    fn layer_ids(&self) -> Vec<String> {
        self.layers
            .iter()
            .map(|e| e.layer.id().to_string())
            .collect()
    }

    fn layer_views(&self) -> Vec<LayerView> {
        self.layers
            .iter()
            .map(|e| {
                let single_label = e.summary.geometry_types.join(", ");
                LayerView {
                    id: e.layer.id().to_string(),
                    file_name: e.file_name.clone(),
                    format: e.summary.format.clone(),
                    feature_count: e.summary.feature_count,
                    geometry_types: e.summary.geometry_types.clone(),
                    fields: e.summary.fields.clone(),
                    visible: e.visible,
                    expanded: e.expanded,
                    sym_classes: crate::symbology::class_rows(&e.symbology, &single_label),
                }
            })
            .collect()
    }

    /// 按 id 定位图层下标（layers Vec 增删会使下标漂移，一律现查现用）。
    fn layer_index(&self, id: &str) -> Option<usize> {
        self.layers.iter().position(|e| e.layer.id() == id)
    }

    /// 地图输出的有效主题（界面主题与地图色彩解耦，默认固定晨山）。
    fn effective_map_theme(&self) -> Theme {
        match self.map_theme_mode {
            MapThemeMode::FixedLight => Theme::Light,
            MapThemeMode::FixedDark => Theme::Dark,
            MapThemeMode::FollowUi => self.theme,
        }
    }

    fn find_layer(&self, id: &str) -> Result<usize, String> {
        self.layers
            .iter()
            .position(|e| e.layer.id() == id)
            .ok_or_else(|| format!("图层不存在: {id}（layers 命令查看清单）"))
    }

    fn toggle_theme(&mut self, ctx: &egui::Context) {
        // 交叉淡化：记录旧主题底色（渐隐遮罩用，见 ui() 末尾）。
        self.theme_fade = Some((Instant::now(), crate::theme::palette(self.theme).bg_primary));
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        crate::theme::apply_theme(ctx, self.theme);
        self.canvas.dirty = true;
    }

    /// 保存堪舆工程（.kyu）：地图框清单 + 图层引用（含框归属）+ 可见性 + 视口 + 地图色彩。
    /// 无来源的内存图层（分析产出）不入工程并在终端明示。
    fn save_project(&mut self, path: &Path) {
        let mut project = kanyu_core::project::KanyuProject::new(
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".to_string()),
            &self.project_crs,
        );
        project.viewport = self.view_bbox; // 激活框视口（向后兼容单框语义）
        project.map_theme = self.map_theme_mode.as_str().to_string();
        // 地图框清单（首项恒为主框「地图」）。
        project.frames = self
            .frames
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let active = self.active_frame == Some(i);
                kanyu_core::project::ProjectFrame {
                    title: f.title.clone(),
                    dim: (if active { self.frame_dim } else { f.site.dim })
                        .as_str()
                        .to_string(),
                    viewport: if active {
                        self.view_bbox
                    } else {
                        f.site.view_bbox
                    },
                    open: f.open,
                    docked: f.docked,
                    wms_base: f.wms_base.clone(),
                }
            })
            .collect();
        // 布局清单（含绑定地图框标题）。
        project.layouts = self
            .layouts
            .iter()
            .map(|lv| kanyu_core::project::ProjectLayout {
                title: lv.title.clone(),
                page: match lv.spec.page {
                    kanyu_render::layout::PageSize::A4Landscape => "a4l",
                    kanyu_render::layout::PageSize::A4Portrait => "a4p",
                }
                .to_string(),
                dpi: lv.spec.dpi,
                legend: lv.spec.show_legend,
                scalebar: lv.spec.show_scalebar,
                north: lv.spec.show_north,
                map: (!lv.map.is_empty()).then(|| lv.map.clone()),
            })
            .collect();
        let mut skipped = 0;
        for i in 0..self.frames.len() {
            // 激活框图层读平铺字段，休眠框读自身 site（交换模型）。
            let (entries, toc_ref) = if self.active_frame == Some(i) {
                (&self.layers, &self.toc)
            } else {
                (&self.frames[i].site.layers, &self.frames[i].site.toc)
            };
            for entry in entries {
                match &entry.source_path {
                    // 分组路径：根级图层写 None（不输出 group 键，保持文件干净）；
                    // 符号化 JSON 写入 style（shell 侧模型，core 原样透传）；
                    // 框归属：主框写 None（老工程语义），其余写框标题。
                    Some(src) => project.layers.push(kanyu_core::project::ProjectLayer {
                        id: entry.layer.id().to_string(),
                        source: src.clone(),
                        visible: entry.visible,
                        style: serde_json::to_value(&entry.symbology).ok(),
                        group: toc::group_path_of(toc_ref, entry.layer.id())
                            .filter(|p| !p.is_empty()),
                        map: (i != 0).then(|| self.frames[i].title.clone()),
                    }),
                    None => skipped += 1,
                }
            }
        }
        match project.save(&path.to_string_lossy()) {
            Ok(()) => {
                let mut msg = format!(
                    "工程已保存 → {}（{} 个图层引用）",
                    path.display(),
                    project.layers.len()
                );
                if skipped > 0 {
                    msg.push_str(&format!(
                        "；{skipped} 个内存图层未入工程（可用「导出图层」先存为数据文件）"
                    ));
                }
                self.status = msg.clone();
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Err(e) => {
                self.console
                    .push(crate::console::LineKind::Err, format!("保存工程失败: {e}"));
                self.error_msg = Some(format!("保存工程失败: {e}"));
            }
        }
    }

    /// 打开堪舆工程（.kyu）：恢复地图框清单与图层（按引用加载、按 map 归属各框）、
    /// 可见性、视口、地图色彩。旧工程无 frames/map 字段 → 单主框全收。
    fn open_project(&mut self, path: &Path) {
        let project = match kanyu_core::project::KanyuProject::load(&path.to_string_lossy()) {
            Ok(p) => p,
            Err(e) => {
                self.console
                    .push(crate::console::LineKind::Err, format!("打开工程失败: {e}"));
                self.error_msg = Some(format!("打开工程失败: {e}"));
                return;
            }
        };
        // 清空当前现场再恢复（与"打开工程"语义一致；编辑会话随旧现场废弃）。
        self.frames.clear();
        self.frames.push(crate::mapview::MapFrame::main());
        self.next_frame_id = 1;
        self.active_frame = Some(0);
        self.active_layout = None;
        self.layers.clear();
        self.toc.clear();
        self.selected = None;
        self.selected_group = None;
        self.edit_session = None;
        self.layouts.clear();
        self.next_layout_id = 1;
        self.console.info(format!(
            "打开工程 {}（{} 个图层引用，{} 个地图框）",
            project.name,
            project.layers.len(),
            project.frames.len().max(1)
        ));
        // 地图框清单（首项 = 主框元数据；缺省 = 单主框——旧工程兼容）。
        if let Some(pf0) = project.frames.first() {
            self.frames[0].title = pf0.title.clone();
            self.frames[0].open = pf0.open;
            self.frames[0].docked = pf0.docked;
            self.view_bbox = pf0.viewport.or(project.viewport);
            self.frames[0].wms_base = self.restore_wms_base(pf0.wms_base.as_deref(), &pf0.title);
        } else {
            self.view_bbox = project.viewport;
        }
        for pf in project.frames.iter().skip(1) {
            let id = self.next_frame_id;
            self.next_frame_id += 1;
            let mut f = crate::mapview::MapFrame::new(
                id,
                pf.title.clone(),
                crate::mapview::ViewDim::parse(&pf.dim),
            );
            f.open = pf.open;
            f.docked = pf.docked;
            f.site.view_bbox = pf.viewport;
            f.site.needs_fit = pf.viewport.is_none();
            f.wms_base = self.restore_wms_base(pf.wms_base.as_deref(), &pf.title);
            self.frames.push(f);
        }
        let mut failed = 0;
        for pl in &project.layers {
            // 目标框：map 标题匹配（无匹配/缺省 → 主框）。
            let target = pl
                .map
                .as_deref()
                .and_then(|t| self.frames.iter().position(|f| f.title == t))
                .unwrap_or(0);
            if self.active_frame != Some(target) {
                self.activate_frame(target);
            }
            let before = self.layers.len();
            self.open_file(Path::new(&pl.source));
            if self.layers.len() > before {
                self.layers[before].visible = pl.visible;
                // 按工程记录的分组路径重建组树（缺失组自动逐级创建）。
                if let Some(group) = &pl.group {
                    let id = self.layers[before].layer.id().to_string();
                    toc::insert_layer_into(&mut self.toc, Some(group), &id);
                }
                // 符号化恢复（老工程无 style 字段 → 保持默认单色）。
                if let Some(style) = &pl.style {
                    if let Ok(sym) =
                        serde_json::from_value::<crate::symbology::LayerSymbology>(style.clone())
                    {
                        self.layers[before].symbology = sym;
                    }
                }
            } else {
                failed += 1;
                self.error_msg = None; // 单源失败不阻塞整工程
            }
        }
        // 回落：激活首个打开态框（全关则主框——activate 会重开）。
        let first_open = self.frames.iter().position(|f| f.open).unwrap_or(0);
        if self.active_frame != Some(first_open) {
            self.activate_frame(first_open);
        }
        // 布局清单（绑定框标题失配 → 回退跟随激活框，容错旧引用）。
        for pl in &project.layouts {
            let id = self.next_layout_id;
            self.next_layout_id += 1;
            let spec = kanyu_render::layout::LayoutSpec {
                page: if pl.page == "a4p" {
                    kanyu_render::layout::PageSize::A4Portrait
                } else {
                    kanyu_render::layout::PageSize::A4Landscape
                },
                dpi: pl.dpi,
                title: pl.title.clone(),
                show_legend: pl.legend,
                show_scalebar: pl.scalebar,
                show_north: pl.north,
            };
            let mut lv = crate::layoutview::LayoutView::new(id, pl.title.clone(), spec);
            lv.map = pl.map.clone().unwrap_or_default();
            if !lv.map.is_empty() && !self.frames.iter().any(|f| f.title == lv.map) {
                self.console.info(format!(
                    "布局「{}」绑定的地图框「{}」已不存在——改为跟随当前地图框",
                    lv.title, lv.map
                ));
                lv.map = String::new();
            }
            self.layouts.push(lv);
        }
        self.map_theme_mode = MapThemeMode::parse(&project.map_theme);
        self.project_crs = project.crs.clone();
        if self.view_bbox.is_some() {
            self.canvas.dirty = true;
        } else {
            self.needs_fit = true;
        }
        self.rebuild_merged();
        let msg = if failed > 0 {
            format!(
                "工程 {} 已恢复（{failed} 个数据源加载失败，详见终端）",
                project.name
            )
        } else {
            format!("工程 {} 已恢复", project.name)
        };
        self.status = msg.clone();
        self.console.info(msg);
    }

    // ===== 内核操作（对话框/终端共用） =====

    fn op_query(&mut self, id: &str, expr: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let result = self.layers[idx]
            .layer
            .query(expr)
            .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("q_{id}"), result, &format!("query \"{expr}\"")))
    }

    fn op_buffer(&mut self, id: &str, distance: f64) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let result = analysis::buffer(&self.layers[idx].layer.collection(), distance, 16)
            .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("buf_{id}"), result, &format!("buffer {distance}")))
    }

    fn op_overlay(&mut self, target: &str, overlay: &str, op: &str) -> Result<String, String> {
        let ti = self.find_layer(target)?;
        let oi = self.find_layer(overlay)?;
        let op = op
            .parse::<analysis::OverlayOp>()
            .map_err(|e| e.to_string())?;
        let result = analysis::overlay(
            &self.layers[ti].layer.collection(),
            &self.layers[oi].layer.collection(),
            op,
        )
        .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("ov_{target}"), result, "overlay"))
    }

    fn op_sjoin(&mut self, target: &str, join: &str, predicate: &str) -> Result<String, String> {
        let ti = self.find_layer(target)?;
        let ji = self.find_layer(join)?;
        let pred = predicate
            .parse::<analysis::SpatialPredicate>()
            .map_err(|e| e.to_string())?;
        let result = analysis::sjoin(
            &self.layers[ti].layer.collection(),
            &self.layers[ji].layer.collection(),
            pred,
        )
        .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("sj_{target}"), result, "sjoin"))
    }

    fn op_zonal(
        &mut self,
        zones: &str,
        values: &str,
        field: &str,
        stats: &str,
    ) -> Result<String, String> {
        let zi = self.find_layer(zones)?;
        let vi = self.find_layer(values)?;
        let stats: Result<Vec<_>, _> = stats
            .split(',')
            .map(|s| s.trim().parse::<analysis::ZonalStat>())
            .collect();
        let stats = stats.map_err(|e: kanyu_core::KanyuError| e.to_string())?;
        let result = analysis::zonal_stats(
            &self.layers[zi].layer.collection(),
            &self.layers[vi].layer.collection(),
            field,
            &stats,
        )
        .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("zs_{zones}"), result, "zonal_stats"))
    }

    fn op_measure(&self, id: &str, kind: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let kind = kind
            .parse::<crs::MeasureKind>()
            .map_err(|e| e.to_string())?;
        let report =
            crs::measure(&self.layers[idx].layer.collection(), kind).map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    }

    fn op_topology(&self, id: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let report = analysis::topology_check(
            &self.layers[idx].layer.collection(),
            &[analysis::TopologyRule::NoOverlap],
        )
        .map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    }

    fn op_reproject(&mut self, id: &str, from: &str, to: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let result = crs::reproject(&self.layers[idx].layer.collection(), from, to)
            .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(
            &format!("rp_{id}"),
            result,
            &format!("reproject {from}→{to}"),
        ))
    }

    fn op_export(&self, id: &str, out: &str, fmt: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let entry = &self.layers[idx];
        let collection = entry.layer.collection();
        let registry = kanyu_core::FormatRegistry::builtin();
        let caps = registry.require(fmt, "write").map_err(|e| e.to_string())?;
        match caps.id {
            "geojson" => std::fs::write(out, Layer::to_geojson_string(&collection))
                .map_err(|e| e.to_string())?,
            "csv" => std::fs::write(
                out,
                Layer::to_csv_string(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "fgb" => std::fs::write(
                out,
                Layer::to_fgb_bytes(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "geoparquet" => std::fs::write(
                out,
                Layer::to_geoparquet_bytes(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "dxf" => std::fs::write(
                out,
                Layer::to_dxf_string(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "kml" => std::fs::write(
                out,
                Layer::to_kml_string(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "kmz" => std::fs::write(
                out,
                Layer::to_kmz_bytes(&collection).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?,
            "shp" => Layer::write_shp(&collection, out.trim_end_matches(".shp"))
                .map_err(|e| e.to_string())?,
            other => {
                return Err(format!(
                    "格式 '{other}' 的导出在壳层未启用（driver: {}）",
                    caps.driver
                ))
            }
        }
        Ok(format!(
            "已导出 {} 要素 → {out}（{fmt}）",
            collection.features.len()
        ))
    }

    fn op_skill_run(&mut self, skill_id: &str, layer_id: &str) -> Result<String, String> {
        let idx = self.find_layer(layer_id)?;
        let skill = self
            .skills
            .get(skill_id)
            .ok_or_else(|| format!("技能未注册: {skill_id}（先「技能 → 热加载…」）"))?;
        let result = self
            .skill_host
            .run(skill, &self.layers[idx].layer.collection())
            .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(
            &format!("g_{layer_id}"),
            result,
            &format!("skill {skill_id}"),
        ))
    }

    /// 地图导出（当前视图 + 渲染设置）。
    fn op_export_map(&mut self, out: &str) -> Result<String, String> {
        let (w, h) = self.map_export_size;
        let opts = RenderOptions {
            width: w,
            height: h,
            padding: 20.0,
            theme: self.effective_map_theme(),
            viewport: self.view_bbox,
            style: self.map_export_style.clone(),
            ..Default::default()
        };
        match out.rsplit('.').next() {
            Some("svg") => {
                let svg = render_svg(&self.merged, &opts).map_err(|e| e.to_string())?;
                std::fs::write(out, svg).map_err(|e| e.to_string())?;
            }
            Some("png") => {
                let png = render_png(&self.merged, &opts).map_err(|e| e.to_string())?;
                std::fs::write(out, png).map_err(|e| e.to_string())?;
            }
            _ => return Err(format!("输出路径须以 .png 或 .svg 结尾: {out}")),
        }
        Ok(format!("已导出地图 → {out}（{w}×{h}）"))
    }

    /// 应用编辑动作（命令入会话 History，即时重建图层可见）。
    fn apply_edit_action(&mut self, action: crate::edit::EditAction) {
        use crate::edit::EditAction;
        let Some(mut session) = self.edit_session.take() else {
            return;
        };
        let Some(i) = self.layer_index(&session.target) else {
            self.edit_session = Some(session);
            return;
        };
        let mut coll = self.layers[i].layer.collection();
        let cmd: Option<Box<dyn kanyu_edit::EditCommand>> = match action {
            EditAction::MoveVertex {
                feature,
                path,
                old,
                new,
            } => Some(Box::new(kanyu_edit::MoveVertex {
                index: feature,
                path,
                old_pos: old,
                new_pos: new,
            })),
            EditAction::MoveFeature { feature, dx, dy } => {
                Some(Box::new(kanyu_edit::MoveFeature {
                    index: feature,
                    dx,
                    dy,
                }))
            }
            EditAction::InsertPoint { pos } => {
                let feature = bare_feature(geojson::Value::Point(vec![pos.0, pos.1]));
                Some(Box::new(kanyu_edit::InsertFeature {
                    feature,
                    index: coll.features.len(),
                }))
            }
            EditAction::Select(hit) => {
                session.selected = hit;
                self.edit_session = Some(session);
                return;
            }
            EditAction::DeleteSelected => match session.selected {
                Some(sel) => match kanyu_edit::DeleteFeatures::new(&coll, &[sel]) {
                    Ok(c) => Some(Box::new(c)),
                    Err(e) => {
                        self.toast_err(e.to_string());
                        None
                    }
                },
                None => {
                    self.toast_err("未选中要素（先用选择工具点选）");
                    None
                }
            },
            // —— 线/面绘制（状态留在会话，不进 History；完成才转 InsertFeature）——
            EditAction::DrawAddVertex { pos } => {
                if let Some(kind) = crate::edit::draw_kind_of(session.tool) {
                    let d = session
                        .drawing
                        .get_or_insert_with(|| crate::edit::DrawState::new(kind));
                    d.add(pos);
                    self.status = format!(
                        "绘制中：{} 个顶点（双击/Enter 完成，Backspace 撤点，Esc 放弃）",
                        d.verts.len()
                    );
                }
                self.edit_session = Some(session);
                return;
            }
            EditAction::DrawUndoVertex => {
                if let Some(d) = &mut session.drawing {
                    d.undo();
                    self.status = format!("绘制中：{} 个顶点", d.verts.len());
                }
                self.edit_session = Some(session);
                return;
            }
            EditAction::DrawCancel => {
                if session.drawing.take().is_some() {
                    self.status = "已放弃本次绘制".to_string();
                }
                self.edit_session = Some(session);
                return;
            }
            EditAction::DrawFinish => {
                let Some(d) = session.drawing.take() else {
                    self.edit_session = Some(session);
                    return;
                };
                match d.finish() {
                    Ok(value) => {
                        if session.tool == crate::edit::EditTool::AddHole {
                            // 挖洞：环首顶点定位目标面（MultiPolygon part 定位），
                            // 内核校验「环完全在面内」（失败保留绘制现场）。
                            let geojson::Value::Polygon(rings) = value else {
                                unreachable!("添加洞草图恒为面");
                            };
                            let ring = rings.into_iter().next().expect("面草图有外环");
                            let pt = (ring[0][0], ring[0][1]);
                            match crate::edit::polygon_part_at(&coll, pt) {
                                Some((fi, part)) => {
                                    match kanyu_edit::validate_hole(&coll, fi, part, &ring) {
                                        Ok(()) => Some(Box::new(kanyu_edit::AddHole {
                                            index: fi,
                                            part,
                                            ring,
                                        })),
                                        Err(e) => {
                                            self.toast_err(e.to_string());
                                            session.drawing = Some(d);
                                            self.edit_session = Some(session);
                                            return;
                                        }
                                    }
                                }
                                None => {
                                    self.toast_err("洞环须画在目标面内（未命中任何面要素）");
                                    session.drawing = Some(d);
                                    self.edit_session = Some(session);
                                    return;
                                }
                            }
                        } else {
                            Some(Box::new(kanyu_edit::InsertFeature {
                                feature: bare_feature(value),
                                index: coll.features.len(),
                            }))
                        }
                    }
                    Err(e) => {
                        // 点数不足：中文提示并保留绘制现场（可继续加点）。
                        self.toast_err(&e);
                        session.drawing = Some(d);
                        self.edit_session = Some(session);
                        return;
                    }
                }
            }
        };
        if let Some(cmd) = cmd {
            let desc = cmd.describe();
            match session.history.push(cmd, &mut coll) {
                Ok(()) => {
                    // 命令即时生效：同 id 重建图层 + 刷新概要 + 置脏重渲。
                    let id = self.layers[i].layer.id().to_string();
                    self.layers[i].layer = Layer::from_collection(id, coll);
                    self.layers[i].summary = self.layers[i].layer.summary();
                    session.selected = None;
                    self.rebuild_merged();
                    self.status = format!("已{desc}（{} 步可撤销）", session.history.len());
                }
                Err(e) => {
                    self.toast_err(e.to_string());
                    self.console
                        .push(crate::console::LineKind::Err, e.to_string());
                }
            }
        }
        self.edit_session = Some(session);
    }

    /// 撤销/重做一步。
    fn edit_undo(&mut self, redo: bool) {
        let Some(mut session) = self.edit_session.take() else {
            return;
        };
        let Some(i) = self.layer_index(&session.target) else {
            self.edit_session = Some(session);
            return;
        };
        let mut coll = self.layers[i].layer.collection();
        let result = if redo {
            session.history.redo(&mut coll)
        } else {
            session.history.undo(&mut coll)
        };
        match result {
            Ok(desc) => {
                let id = self.layers[i].layer.id().to_string();
                self.layers[i].layer = Layer::from_collection(id, coll);
                self.layers[i].summary = self.layers[i].layer.summary();
                self.rebuild_merged();
                self.status = format!("已{} {}", if redo { "重做" } else { "撤销" }, desc);
            }
            Err(e) => self.toast_err(e.to_string()),
        }
        self.edit_session = Some(session);
    }

    // ===== 动作分派 =====

    /// 成功轻提示（右上角青条，自动消退）。
    fn toast_ok(&mut self, msg: impl Into<String>) {
        crate::ui_kit::push_toast(&mut self.toasts, crate::ui_kit::ToastKind::Success, msg);
    }

    /// 失败轻提示（朱砂条）。
    fn toast_err(&mut self, msg: impl Into<String>) {
        crate::ui_kit::push_toast(&mut self.toasts, crate::ui_kit::ToastKind::Error, msg);
    }

    /// 打开设置对话框（以工程当前值快照）。
    fn open_settings(&mut self) {
        self.settings = Some(crate::settings::SettingsDialog::open_with(
            &self.project_crs,
            self.map_export_size,
            self.map_theme_mode,
            self.ui_zoom,
        ));
    }

    fn dispatch(&mut self, action: RibbonAction, ctx: &egui::Context) {
        match action {
            RibbonAction::OpenData => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("地理数据", OPEN_EXTENSIONS)
                    .pick_file()
                {
                    self.open_file(&path);
                }
            }
            RibbonAction::OpenExample => {
                self.open_file(Path::new("examples/buildings.geojson"));
            }
            RibbonAction::OpenProject => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("堪舆工程", &["kyu"])
                    .pick_file()
                {
                    self.open_project(&path);
                }
            }
            RibbonAction::SaveProject => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("堪舆工程", &["kyu"])
                    .set_file_name("untitled.kyu")
                    .save_file()
                {
                    self.save_project(&path);
                }
            }
            RibbonAction::SaveScreenshot => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("PNG 图片", &["png"])
                    .set_file_name("kanyu.png")
                    .save_file()
                {
                    self.pending_window_shot = Some(path.to_string_lossy().into_owned());
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
            }
            RibbonAction::ToggleTheme => self.toggle_theme(ctx),
            RibbonAction::LayerInfo => match self.selected {
                Some(i) => {
                    let s = &self.layers[i].summary;
                    self.console.info(format!(
                        "图层 {}: {} 要素 | 几何 {} | 字段 [{}]",
                        s.id,
                        s.feature_count,
                        s.geometry_types.join(", "),
                        s.fields.join(", ")
                    ));
                }
                None => self
                    .console
                    .push(crate::console::LineKind::Err, "未选中图层"),
            },
            RibbonAction::QueryDialog => {
                self.dialogs.query = Some(crate::dialogs::QueryState::default())
            }
            RibbonAction::ExportDialog => {
                self.dialogs.export = Some(crate::dialogs::ExportState::default())
            }
            RibbonAction::ReprojectDialog => {
                // 目标 CRS 默认取工程坐标系（设置页可改）。
                self.dialogs.reproject = Some(crate::dialogs::ReprojectState {
                    from: "EPSG:4326".to_string(),
                    to: self.project_crs.clone(),
                    ..Default::default()
                })
            }
            RibbonAction::BufferDialog => {
                self.dialogs.buffer = Some(crate::dialogs::BufferState::default())
            }
            RibbonAction::OverlayDialog => {
                self.dialogs.overlay = Some(crate::dialogs::OverlayState::default())
            }
            RibbonAction::Topology => match self.selected {
                Some(i) => {
                    let id = self.layers[i].layer.id().to_string();
                    match self.op_topology(&id) {
                        Ok(msg) => self.console.info(msg),
                        Err(e) => self.console.push(crate::console::LineKind::Err, e),
                    }
                }
                None => self
                    .console
                    .push(crate::console::LineKind::Err, "未选中图层"),
            },
            RibbonAction::SjoinDialog => {
                self.dialogs.sjoin = Some(crate::dialogs::SjoinState::default())
            }
            RibbonAction::ZonalDialog => {
                self.dialogs.zonal = Some(crate::dialogs::ZonalState::default())
            }
            RibbonAction::MeasureDialog => {
                self.dialogs.measure = Some(crate::dialogs::MeasureState::default())
            }
            RibbonAction::SettingsDialog => self.open_settings(),
            RibbonAction::ExportMapDialog => {
                self.dialogs.export_map = Some(crate::dialogs::ExportMapState::default())
            }
            RibbonAction::ZoomToFit => self.needs_fit = true,
            RibbonAction::StartEdit => {
                if let Some(i) = self.selected {
                    let (id, name) = (
                        self.layers[i].layer.id().to_string(),
                        self.layers[i].file_name.clone(),
                    );
                    self.edit_session = Some(crate::edit::EditSession::new(id, name.clone()));
                    self.console
                        .info(format!("编辑会话已开始：{name}（编辑页签选择工具）"));
                    self.toast_ok(format!("开始编辑 {name}"));
                }
            }
            RibbonAction::SaveEdit => {
                if let Some(s) = self.edit_session.take() {
                    let msg = format!(
                        "编辑会话已结束：{}（编辑即时生效；落盘请用「数据 → 导出图层…」）",
                        s.target_name
                    );
                    self.console.info(msg.clone());
                    self.toast_ok(msg);
                }
            }
            RibbonAction::DiscardEdit => {
                if let Some(mut s) = self.edit_session.take() {
                    // 逐条逆回到会话起点。
                    if let Some(i) = self.layer_index(&s.target) {
                        let mut coll = self.layers[i].layer.collection();
                        while s.history.can_undo() {
                            if s.history.undo(&mut coll).is_err() {
                                break;
                            }
                        }
                        let id = self.layers[i].layer.id().to_string();
                        self.layers[i].layer = Layer::from_collection(id, coll);
                        self.layers[i].summary = self.layers[i].layer.summary();
                        self.rebuild_merged();
                    }
                    let msg = format!("已放弃编辑：{}", s.target_name);
                    self.console.info(msg.clone());
                    self.toast_ok(msg);
                }
            }
            RibbonAction::SetEditTool(tool) => {
                let Some(target) = self.edit_session.as_ref().map(|s| s.target.clone()) else {
                    return;
                };
                // 点/线/面添加工具与目标图层几何类型匹配判定（不匹配阻止并中文提示）。
                let types = self
                    .layer_index(&target)
                    .map(|i| self.layers[i].summary.geometry_types.clone())
                    .unwrap_or_default();
                if let Err(e) = crate::edit::tool_geometry_match(tool, &types) {
                    self.toast_err(&e);
                    self.console.push(crate::console::LineKind::Err, e);
                    return;
                }
                if let Some(s) = &mut self.edit_session {
                    s.tool = tool;
                    s.drawing = None; // 切换工具放弃未完成绘制
                    self.status = format!("编辑工具 → {}", tool.label());
                }
            }
            RibbonAction::ToggleEditSnap => {
                if let Some(s) = &mut self.edit_session {
                    s.snap = !s.snap;
                    self.status = format!("顶点捕捉 → {}", if s.snap { "开" } else { "关" });
                }
            }
            RibbonAction::Undo => self.edit_undo(false),
            RibbonAction::Redo => self.edit_undo(true),
            RibbonAction::NewFrame2D => self.new_frame(crate::mapview::ViewDim::TwoD),
            RibbonAction::NewFrame3D => self.new_frame(crate::mapview::ViewDim::ThreeD),
            RibbonAction::ResetView => {
                self.view_bbox = None;
                self.needs_fit = true;
                self.canvas.dirty = true;
            }
            RibbonAction::TogglePanel(id) => {
                if self.dock.is_open(id) {
                    self.dock.close_panel(id);
                } else {
                    self.dock.open_panel(id);
                }
                self.mark_state_dirty();
            }
            RibbonAction::CycleMapTheme => {
                self.map_theme_mode = self.map_theme_mode.next();
                self.console
                    .info(format!("地图色彩模式 → {}", self.map_theme_mode.label()));
                self.canvas.dirty = true;
            }
            RibbonAction::SkillHotload => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("WASM 技能", &["wasm"])
                    .pick_file()
                {
                    match self.skill_host.load(&path.to_string_lossy()) {
                        Ok(skill) => {
                            let meta = skill.meta().clone();
                            let replaced = self.skills.contains_key(&meta.name);
                            self.console.info(format!(
                                "技能已注册: {} v{}（能力: {}{}）",
                                meta.name,
                                meta.version,
                                meta.capabilities.join(", "),
                                if replaced { "，覆盖同名" } else { "" }
                            ));
                            self.skill_metas.push(SkillView {
                                id: meta.name.clone(),
                                version: meta.version.clone(),
                                capabilities: meta.capabilities.clone(),
                            });
                            self.skills.insert(meta.name.clone(), skill);
                        }
                        Err(e) => {
                            self.console
                                .push(crate::console::LineKind::Err, format!("技能校验失败: {e}"));
                        }
                    }
                }
            }
            RibbonAction::SkillList => {
                if self.skill_metas.is_empty() {
                    self.console.info("（无已注册技能）");
                } else {
                    for g in &self.skill_metas {
                        self.console.info(format!(
                            "{:<24} v{:<8} [{}]",
                            g.id,
                            g.version,
                            g.capabilities.join(", ")
                        ));
                    }
                }
            }
            RibbonAction::SkillRunDialog => {
                self.dialogs.skill_run = Some(crate::dialogs::SkillRunState::default())
            }
            RibbonAction::ShowHelp => self.console.info(HELP_TEXT),
            RibbonAction::About => self.dialogs.about = true,
        }
    }

    fn dispatch_dialog_result(&mut self, result: DialogResult) {
        let outcome = match result {
            DialogResult::Query { layer, expr } => self.op_query(&layer, &expr),
            DialogResult::Export { layer, out, fmt } => self.op_export(&layer, &out, &fmt),
            DialogResult::Reproject { layer, from, to } => self.op_reproject(&layer, &from, &to),
            DialogResult::Buffer { layer, distance } => self.op_buffer(&layer, distance),
            DialogResult::Overlay {
                target,
                overlay,
                op,
            } => self.op_overlay(&target, &overlay, &op),
            DialogResult::Sjoin {
                target,
                join,
                predicate,
            } => self.op_sjoin(&target, &join, &predicate),
            DialogResult::Zonal {
                zones,
                values,
                field,
                stats,
            } => self.op_zonal(&zones, &values, &field, &stats),
            DialogResult::Measure { layer, kind } => self.op_measure(&layer, &kind),
            DialogResult::ExportMap { out } => self.op_export_map(&out),
            DialogResult::SkillRun { skill_id, layer } => self.op_skill_run(&skill_id, &layer),
            DialogResult::Invalid { reason } => Err(reason),
        };
        match outcome {
            Ok(msg) => {
                self.status = msg.clone();
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Err(e) => {
                self.status = format!("失败: {e}");
                self.console.push(crate::console::LineKind::Err, e.clone());
                self.toast_err(e);
            }
        }
    }

    /// 移除图层（本体 + 目录树节点），修正选中下标并重建合并缓存。
    fn remove_layer_by_id(&mut self, id: &str) {
        if let Some(i) = self.layer_index(id) {
            let name = self.layers[i].file_name.clone();
            self.layers.remove(i);
            toc::remove_layer(&mut self.toc, id);
            match self.selected {
                Some(s) if s == i => self.selected = None,
                Some(s) if s > i => self.selected = Some(s - 1),
                _ => {}
            }
            self.console.info(format!("已移除图层 {name}"));
            self.rebuild_merged();
        }
    }

    /// 移除图层本体（不动目录树——供 RemoveGroup 在摘除组节点后逐图层清理）。
    fn remove_layer_entry(&mut self, id: &str) {
        if let Some(i) = self.layer_index(id) {
            self.layers.remove(i);
            match self.selected {
                Some(s) if s == i => self.selected = None,
                Some(s) if s > i => self.selected = Some(s - 1),
                _ => {}
            }
        }
    }

    /// 应用重命名/新建组对话框结果。
    fn apply_rename(&mut self, state: RenameState) {
        let name = state.name.trim().to_string();
        if name.is_empty() {
            self.console
                .push(crate::console::LineKind::Err, "名称不能为空");
            return;
        }
        match state.target {
            RenameTarget::Layer(id) => {
                if let Some(i) = self.layer_index(&id) {
                    self.layers[i].file_name = name.clone();
                    self.console.info(format!("图层已重命名为 {name}"));
                }
            }
            RenameTarget::Group(path) => match toc::rename_group(&mut self.toc, &path, &name) {
                Ok(new_path) => {
                    if self.selected_group.as_deref() == Some(path.as_str()) {
                        self.selected_group = Some(new_path.clone());
                    }
                    self.console.info(format!("组已重命名为 {new_path}"));
                }
                Err(e) => self.console.push(crate::console::LineKind::Err, e),
            },
            RenameTarget::NewGroupForLayer(id) => {
                match toc::new_group_named(&mut self.toc, None, &name) {
                    Ok(path) => {
                        toc::insert_layer_into(&mut self.toc, Some(&path), &id);
                        self.console
                            .info(format!("已新建组「{path}」并移入图层 {id}"));
                        self.rebuild_merged();
                    }
                    Err(e) => self.console.push(crate::console::LineKind::Err, e),
                }
            }
        }
    }

    fn dispatch_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::SelectLayer(id) => {
                self.selected = self.layer_index(&id);
                self.selected_group = None;
            }
            PanelAction::SetLayerVisible(id, v) => {
                if let Some(i) = self.layer_index(&id) {
                    self.layers[i].visible = v;
                    self.rebuild_merged();
                }
            }
            PanelAction::ToggleLayerVisible(id) => {
                if let Some(i) = self.layer_index(&id) {
                    self.layers[i].visible = !self.layers[i].visible;
                    self.rebuild_merged();
                }
            }
            PanelAction::ZoomToLayer(id) => {
                if let Some(i) = self.layer_index(&id) {
                    if let Ok(Some(ext)) = collection_extent(&self.layers[i].layer.collection()) {
                        self.selected = Some(i);
                        // 下一帧以该图层范围（而非可见并集）做一次性适配。
                        self.fit_extent = Some(ext);
                        self.needs_fit = true;
                    }
                }
            }
            PanelAction::ShowSummary(id) => {
                if let Some(i) = self.layer_index(&id) {
                    let s = &self.layers[i].summary;
                    self.console.info(format!(
                        "图层 {}: {} 要素 | 格式 {} | 几何 {} | 字段 [{}]",
                        s.id,
                        s.feature_count,
                        s.format,
                        s.geometry_types.join(", "),
                        s.fields.join(", ")
                    ));
                }
            }
            PanelAction::OpenAttrTable(id) => {
                if self.layer_index(&id).is_some() {
                    self.selected = self.layer_index(&id);
                    self.dock.open_panel(crate::dock::PanelId::AttrTable);
                    self.attrtable.set_layer(id);
                }
            }
            PanelAction::LayerProperties(id) => {
                self.open_layer_props(&id);
            }
            PanelAction::MoveLayer(id, dir) => {
                // 父列表内位移影响渲染叠置顺序 → 重建合并缓存。
                if toc::move_layer(&mut self.toc, &id, dir) {
                    self.rebuild_merged();
                }
            }
            PanelAction::RenameLayer(id) => {
                if let Some(i) = self.layer_index(&id) {
                    self.rename = Some(RenameState {
                        target: RenameTarget::Layer(id),
                        name: self.layers[i].file_name.clone(),
                    });
                }
            }
            PanelAction::MoveLayerToGroup(id, path) => {
                toc::insert_layer_into(&mut self.toc, path.as_deref(), &id);
                self.rebuild_merged();
            }
            PanelAction::NewGroupForLayer(id) => {
                self.rename = Some(RenameState {
                    target: RenameTarget::NewGroupForLayer(id),
                    name: "新建图层组".to_string(),
                });
            }
            PanelAction::ExportLayer(id) => {
                if let Some(i) = self.layer_index(&id) {
                    self.selected = Some(i);
                    self.dialogs.export = Some(crate::dialogs::ExportState::default());
                }
            }
            PanelAction::RemoveLayer(id) => self.remove_layer_by_id(&id),
            PanelAction::ToggleExpand(id) => {
                if let Some(i) = self.layer_index(&id) {
                    self.layers[i].expanded = !self.layers[i].expanded;
                }
            }
            PanelAction::SelectGroup(path) => {
                self.selected_group = Some(path);
                self.selected = None;
            }
            PanelAction::ToggleGroupExpand(path) => {
                toc::toggle_group_expand(&mut self.toc, &path);
            }
            PanelAction::SetGroupExpanded(path, v) => {
                toc::set_group_expanded(&mut self.toc, &path, v);
            }
            PanelAction::SetGroupVisible(path, on) => {
                let toc = &mut self.toc;
                let layers = &mut self.layers;
                toc::set_group_all(toc, &path, on, |id, v| {
                    if let Some(e) = layers.iter_mut().find(|e| e.layer.id() == id) {
                        e.visible = v;
                    }
                });
                self.rebuild_merged();
            }
            PanelAction::ZoomToGroup(path) => {
                if let Some(g) = toc::find_group(&self.toc, &path) {
                    let ids = toc::group_layer_ids(&g.children);
                    let mut extents = Vec::new();
                    for id in &ids {
                        if let Some(i) = self.layer_index(id) {
                            if let Ok(Some(ext)) =
                                collection_extent(&self.layers[i].layer.collection())
                            {
                                extents.push(ext);
                            }
                        }
                    }
                    if let Some(u) = view::union(extents) {
                        self.fit_extent = Some(u);
                        self.needs_fit = true;
                    }
                }
            }
            PanelAction::RenameGroup(path) => {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                self.rename = Some(RenameState {
                    target: RenameTarget::Group(path),
                    name,
                });
            }
            PanelAction::NewGroup(parent) => {
                let path = toc::new_group(&mut self.toc, parent.as_deref());
                self.console.info(format!("已新建图层组「{path}」"));
            }
            PanelAction::Ungroup(path) => {
                if toc::ungroup(&mut self.toc, &path) {
                    if self.selected_group.as_deref() == Some(path.as_str()) {
                        self.selected_group = None;
                    }
                    self.console
                        .info(format!("已取消分组「{path}」（子项上移一级）"));
                    self.rebuild_merged();
                }
            }
            PanelAction::RemoveGroup(path) => {
                if let Some(ids) = toc::remove_group(&mut self.toc, &path) {
                    let n = ids.len();
                    for id in &ids {
                        self.remove_layer_entry(id);
                    }
                    if self.selected_group.as_deref() == Some(path.as_str()) {
                        self.selected_group = None;
                    }
                    self.console
                        .info(format!("已移除组「{path}」及 {n} 个图层"));
                    self.rebuild_merged();
                }
            }
            PanelAction::SetAllExpanded(expanded) => {
                toc::set_all_expanded(&mut self.toc, expanded);
                for entry in &mut self.layers {
                    entry.expanded = expanded;
                }
            }
            PanelAction::SetAllVisible(v) => {
                toc::set_all_groups_visible(&mut self.toc, v);
                for entry in &mut self.layers {
                    entry.visible = v;
                }
                self.rebuild_merged();
            }
        }
    }

    /// 应用属性表字段操作（attrcalc 写回：重建图层 + 刷新概要 + 合并缓存）。
    fn apply_attr_action(&mut self, action: crate::attrtable::AttrAction) {
        use crate::attrtable::FieldOp;
        // 单元格编辑走 UpdateProperties 命令（入编辑会话历史）。
        if let crate::attrtable::AttrAction::EditCell {
            layer,
            feature,
            field,
            text,
        } = action
        {
            self.apply_cell_edit(&layer, feature, &field, &text);
            return;
        }
        let (layer, op) = match action {
            crate::attrtable::AttrAction::Apply { layer, op } => (layer, op),
            crate::attrtable::AttrAction::EditCell { .. } => unreachable!("上方分支已返回"),
        };
        let Some(i) = self.layer_index(&layer) else {
            return;
        };
        let collection = self.layers[i].layer.collection();
        let result = match &op {
            FieldOp::Add { name, default } => {
                kanyu_core::attrcalc::add_field(&collection, name, default.clone())
            }
            FieldOp::Delete { name } => kanyu_core::attrcalc::delete_field(&collection, name),
            FieldOp::Rename { old, new } => {
                kanyu_core::attrcalc::rename_field(&collection, old, new)
            }
            FieldOp::Calc { target, expr } => {
                kanyu_core::attrcalc::calc_field(&collection, target, expr)
            }
        };
        let verb = match &op {
            FieldOp::Add { .. } => "添加字段",
            FieldOp::Delete { .. } => "删除字段",
            FieldOp::Rename { .. } => "重命名字段",
            FieldOp::Calc { .. } => "字段计算",
        };
        match result {
            Ok(c) => {
                let id = self.layers[i].layer.id().to_string();
                self.layers[i].layer = Layer::from_collection(id, c);
                self.layers[i].summary = self.layers[i].layer.summary();
                self.rebuild_merged();
                let msg = format!("{verb} 已应用到图层 {layer}");
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Err(e) => {
                let msg = format!("{verb} 失败: {e}");
                self.console.push(crate::console::LineKind::Err, &msg);
                self.toast_err(msg);
            }
        }
    }

    /// 打开图层属性页（常规/源/字段/符号化）。
    fn open_layer_props(&mut self, id: &str) {
        if let Some(i) = self.layer_index(id) {
            let e = &self.layers[i];
            let (single_hex, cat_texts, other_hex, breaks_text) = sym_edit_buffers(&e.symbology);
            self.layer_props = Some(LayerPropsState {
                layer: id.to_string(),
                page: 0,
                name: e.file_name.clone(),
                visible: e.visible,
                sym: e.symbology.clone(),
                single_hex,
                cat_texts,
                other_hex,
                breaks_text,
                err: None,
            });
        }
    }

    /// 应用属性页编辑（重命名/可见性/符号化 → 重建渲染缓存）。
    fn apply_layer_props(&mut self, st: &LayerPropsState) -> Result<(), String> {
        use crate::symbology::{parse_hex, LayerSymbology};
        // 1) 解析符号化编辑缓冲（中文错误）。
        let sym = match &st.sym {
            LayerSymbology::Single { .. } => LayerSymbology::Single {
                color: parse_hex(&st.single_hex)
                    .ok_or_else(|| format!("单色颜色值非法: {}", st.single_hex))?,
            },
            LayerSymbology::Categorical { field, .. } => {
                let mut colors = Vec::new();
                for (v, hex) in &st.cat_texts {
                    colors.push((
                        v.clone(),
                        parse_hex(hex).ok_or_else(|| format!("类别「{v}」颜色值非法: {hex}"))?,
                    ));
                }
                LayerSymbology::Categorical {
                    field: field.clone(),
                    colors,
                    other: parse_hex(&st.other_hex)
                        .ok_or_else(|| format!("<其他>颜色值非法: {}", st.other_hex))?,
                }
            }
            LayerSymbology::Graduated { field, ramp, .. } => {
                let mut breaks = Vec::new();
                for part in st
                    .breaks_text
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    breaks.push(
                        part.parse::<f64>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| format!("断点须为数值: {part}"))?,
                    );
                }
                if breaks.is_empty() {
                    return Err("分级断点不能为空".to_string());
                }
                if !breaks.windows(2).all(|w| w[0] < w[1]) {
                    return Err("分级断点须严格升序".to_string());
                }
                LayerSymbology::Graduated {
                    field: field.clone(),
                    breaks,
                    ramp: *ramp,
                }
            }
        };
        let name = st.name.trim().to_string();
        if name.is_empty() {
            return Err("图层名不能为空".to_string());
        }
        // 2) 写回。
        let Some(i) = self.layer_index(&st.layer) else {
            return Err(format!("图层不存在: {}", st.layer));
        };
        self.layers[i].file_name = name;
        self.layers[i].visible = st.visible;
        self.layers[i].symbology = sym;
        self.rebuild_merged();
        let msg = format!("图层「{}」属性已应用", self.layers[i].file_name);
        self.console.info(msg.clone());
        self.toast_ok(msg);
        Ok(())
    }

    /// 图层属性页（多页签模态）。
    fn layer_props_ui(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.layer_props.take() else {
            return;
        };
        let Some(i) = self.layer_index(&st.layer) else {
            return; // 图层已删除：静默关闭
        };
        let (summary, source, crs, field_types, collection) = {
            let e = &self.layers[i];
            (
                e.summary.clone(),
                e.source_path
                    .clone()
                    .unwrap_or_else(|| "（内存图层，无来源）".to_string()),
                self.project_crs.clone(),
                crate::attrtable::infer_field_types(&e.layer.collection()),
                e.layer.collection(),
            )
        };
        let mut open = true;
        // keep=false 时关闭（确定成功/取消/×）；否则状态回填继续显示。
        let mut keep = true;
        egui::Window::new(crate::ui_kit::text::heading(format!(
            "图层属性 — {}",
            summary.id
        )))
        .collapsible(false)
        .resizable(false)
        .default_size([520.0, 420.0])
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                // 左侧导航。
                ui.allocate_ui_with_layout(
                    egui::Vec2::new(90.0, 300.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        for (idx, label) in ["常规", "源", "字段", "符号化"].iter().enumerate()
                        {
                            if ui
                                .selectable_label(st.page == idx, crate::ui_kit::text::body(*label))
                                .clicked()
                            {
                                st.page = idx;
                                st.err = None;
                            }
                            ui.add_space(4.0);
                        }
                    },
                );
                ui.separator();
                // 右侧内容。
                ui.vertical(|ui| {
                    ui.set_max_width(400.0);
                    match st.page {
                        0 => {
                            // 常规：名称/可见性/来源。
                            crate::ui_kit::text_input(
                                ui,
                                "图层名",
                                &mut st.name,
                                "显示在目录树的名称",
                                true,
                            );
                            let mut v = st.visible;
                            if crate::ui_kit::checkbox(ui, &mut v, "可见").changed() {
                                st.visible = v;
                            }
                            ui.label(crate::ui_kit::text::body(format!("来源: {source}")));
                        }
                        1 => {
                            // 源：只读概要。
                            for (k, val) in [
                                ("格式", summary.format.clone()),
                                ("要素数", summary.feature_count.to_string()),
                                ("几何类型", summary.geometry_types.join(", ")),
                                ("工程坐标系", crs.clone()),
                            ] {
                                ui.horizontal(|ui| {
                                    ui.label(crate::ui_kit::text::body(format!("{k}:")));
                                    ui.label(crate::ui_kit::text::body(val));
                                });
                            }
                        }
                        2 => {
                            // 字段：名称 + 推断类型（只读）。
                            for (name, kind) in &field_types {
                                ui.label(crate::ui_kit::text::body(format!("{name}（{kind}）")));
                            }
                        }
                        _ => {
                            // 符号化：方式 + 参数。
                            self.layer_sym_page(ui, &mut st, &summary, &collection);
                        }
                    }
                });
            });
            ui.add_space(8.0);
            ui.separator();
            ui.horizontal(|ui| {
                if crate::ui_kit::button(ui, "应 用", crate::ui_kit::ButtonVariant::Primary, true)
                    .clicked()
                {
                    match self.apply_layer_props(&st) {
                        Ok(()) => st.err = None,
                        Err(e) => st.err = Some(e),
                    }
                }
                if crate::ui_kit::button(ui, "确 定", crate::ui_kit::ButtonVariant::Secondary, true)
                    .clicked()
                {
                    match self.apply_layer_props(&st) {
                        Ok(()) => keep = false,
                        Err(e) => st.err = Some(e),
                    }
                }
                if crate::ui_kit::button(ui, "取 消", crate::ui_kit::ButtonVariant::Subtle, true)
                    .clicked()
                {
                    keep = false;
                }
                if let Some(e) = &st.err {
                    crate::ui_kit::error_caption(ui, e);
                }
            });
        });
        if !open {
            keep = false;
        }
        if keep {
            self.layer_props = Some(st);
        }
    }

    /// 属性页-符号化页内容。
    fn layer_sym_page(
        &mut self,
        ui: &mut egui::Ui,
        st: &mut LayerPropsState,
        summary: &kanyu_core::LayerSummary,
        collection: &FeatureCollection,
    ) {
        use crate::symbology::LayerSymbology;
        let mode = match &st.sym {
            LayerSymbology::Single { .. } => 0,
            LayerSymbology::Categorical { .. } => 1,
            LayerSymbology::Graduated { .. } => 2,
        };
        let mut new_mode = mode;
        // 方式选择（三态互斥）。
        ui.horizontal(|ui| {
            ui.label(crate::ui_kit::text::body("方式:"));
            for (idx, label) in ["单色", "唯一值分类", "分级设色"].iter().enumerate() {
                if ui
                    .selectable_label(mode == idx, crate::ui_kit::text::body(*label))
                    .clicked()
                {
                    new_mode = idx;
                }
            }
        });
        if new_mode != mode {
            // 切换方式：按数据自动生成默认配置。
            let first_field = summary.fields.first().cloned().unwrap_or_default();
            st.sym = match new_mode {
                0 => LayerSymbology::Single {
                    color: crate::symbology::primary_color(&st.sym),
                },
                1 => crate::symbology::auto_categorical(&first_field, collection),
                _ => crate::symbology::auto_graduated(&first_field, collection),
            };
            let (s, c, o, b) = sym_edit_buffers(&st.sym);
            st.single_hex = s;
            st.cat_texts = c;
            st.other_hex = o;
            st.breaks_text = b;
        }
        ui.add_space(4.0);
        match &mut st.sym {
            LayerSymbology::Single { .. } => {
                crate::ui_kit::text_input(
                    ui,
                    "颜色",
                    &mut st.single_hex,
                    "#RRGGBB，如 #2D6A5E",
                    true,
                );
            }
            LayerSymbology::Categorical { field, .. } => {
                crate::ui_kit::combo(ui, "字段", field, &summary.fields, true);
                ui.label(crate::ui_kit::text::body("类别颜色（#RRGGBB）："));
                for (v, hex) in &mut st.cat_texts {
                    ui.horizontal(|ui| {
                        ui.label(crate::ui_kit::text::body(v.clone()));
                        ui.add(
                            egui::TextEdit::singleline(hex)
                                .desired_width(90.0)
                                .font(egui::FontId::monospace(12.0)),
                        );
                        // 即时预览色块。
                        if let Some(c) = crate::symbology::parse_hex(hex) {
                            let (r, _) = ui
                                .allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                r,
                                2.0,
                                egui::Color32::from_rgb(c[0], c[1], c[2]),
                            );
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(crate::ui_kit::text::body("<其他>"));
                    ui.add(
                        egui::TextEdit::singleline(&mut st.other_hex)
                            .desired_width(90.0)
                            .font(egui::FontId::monospace(12.0)),
                    );
                });
            }
            LayerSymbology::Graduated { field, ramp, .. } => {
                crate::ui_kit::combo(ui, "字段", field, &summary.fields, true);
                crate::ui_kit::text_input(
                    ui,
                    "断点",
                    &mut st.breaks_text,
                    "逗号分隔严格升序，如 30,60,120",
                    true,
                );
                let mut label = ramp.label().to_string();
                let labels: Vec<String> = crate::symbology::Ramp::ALL
                    .iter()
                    .map(|r| r.label().to_string())
                    .collect();
                crate::ui_kit::combo(ui, "色带", &mut label, &labels, true);
                for r in crate::symbology::Ramp::ALL {
                    if r.label() == label {
                        *ramp = r;
                    }
                }
            }
        }
    }

    /// 布局页签内容（排版视图 + 导出；内容取绑定地图框图层集——缺省跟随激活框）。
    fn layout_view_content(&mut self, ui: &mut egui::Ui, li: usize, ctx: &egui::Context) {
        let map_bind = self.layouts[li].map.clone();
        let epoch = self.render_epoch;
        let theme = self.effective_map_theme();
        let need = self.layouts[li].epoch != epoch || self.layouts[li].map_png.is_none();
        // 绑定框数据源（不可变借用段：图例 + 视口 + 按需合成地图 PNG）。
        let (legend, span_m, composed) = {
            let (layers, cache, vbbox, dext) = self.layout_frame_parts(&map_bind);
            let legend = Self::layout_legend_of(layers, cache);
            let viewport = vbbox.or(dext);
            let span_m = viewport.map(|b| (b[2] - b[0]).abs() * 111320.0); // 赤道近似（示意级）
            let composed = if need {
                let f = kanyu_render::layout::LayoutFrame::compute(&self.layouts[li].spec);
                crate::canvas::composite_layers_png(
                    &build_layer_slices(cache),
                    f.map[2].round().max(1.0) as u32,
                    f.map[3].round().max(1.0) as u32,
                    viewport,
                    theme,
                )
                .ok()
            } else {
                None
            };
            (legend, span_m, composed)
        };
        let mut lv = self.layouts.remove(li);
        if let Some(png) = composed {
            lv.map_png = Some(png);
            lv.epoch = epoch;
        }
        let map_png = lv.map_png.clone();
        let action = crate::layoutview::layout_ui(ui, &mut lv, &legend, span_m, map_png.as_deref());
        self.layouts.insert(li, lv);
        match action {
            Some(crate::layoutview::LayoutAction::ExportPng) => self.export_layout(li, "png"),
            Some(crate::layoutview::LayoutAction::ExportSvg) => self.export_layout(li, "svg"),
            None => {}
        }
        let _ = ctx;
    }

    /// 布局图例行（指定图层集 × 渲染缓存的符号化分类；绑定框数据源）。
    fn layout_legend_of(
        layers: &[LayerEntry],
        cache: &[(String, FeatureCollection, crate::symbology::LayerSymbology)],
    ) -> Vec<kanyu_render::layout::LegendRow> {
        let mut rows = Vec::new();
        for (id, _, sym) in cache {
            let name = layers
                .iter()
                .find(|e| e.layer.id() == id)
                .map(|e| e.file_name.clone())
                .unwrap_or_else(|| id.clone());
            let single_label = layers
                .iter()
                .find(|e| e.layer.id() == id)
                .map(|e| e.summary.geometry_types.join(", "))
                .unwrap_or_default();
            for (color, label) in crate::symbology::class_rows(sym, &single_label) {
                rows.push(kanyu_render::layout::LegendRow {
                    color,
                    label: format!("{name} · {label}"),
                });
            }
        }
        rows
    }

    /// 布局导出（PNG/SVG 写盘 + toast；内容取绑定地图框图层集——缺省跟随激活框）。
    fn export_layout(&mut self, li: usize, fmt: &str) {
        if li >= self.layouts.len() {
            return;
        }
        let (spec, map_bind, lv_title) = {
            let lv = &self.layouts[li];
            (lv.spec.clone(), lv.map.clone(), lv.title.clone())
        };
        let (default_name, filter_name) = (format!("{lv_title}.{fmt}"), fmt.to_uppercase());
        let Some(path) = rfd::FileDialog::new()
            .add_filter(filter_name, &[fmt])
            .set_file_name(&default_name)
            .save_file()
        else {
            return;
        };
        let f = kanyu_render::layout::LayoutFrame::compute(&spec);
        let (layers, cache, vbbox, dext) = self.layout_frame_parts(&map_bind);
        let viewport = vbbox.or(dext);
        let span_m = viewport.map(|b| (b[2] - b[0]).abs() * 111320.0);
        let scale = span_m.map(|s| {
            let (label, bar_px, _) = kanyu_render::layout::nice_scale(s, f.map[2], spec.dpi);
            (label, bar_px)
        });
        let legend = Self::layout_legend_of(layers, cache);
        // 地图 PNG（打印尺寸合成；SVG 内经 base64 data-URI 内嵌，保持按层符号化）。
        let map_png = crate::canvas::composite_layers_png(
            &build_layer_slices(cache),
            f.map[2].round().max(1.0) as u32,
            f.map[3].round().max(1.0) as u32,
            viewport,
            self.effective_map_theme(),
        );
        let lv_spec = &spec;
        let out_path = path.to_string_lossy().into_owned();
        let result: Result<(), String> = (|| {
            let map_png = map_png?;
            let bytes = match fmt {
                "svg" => {
                    let img = format!(
                        "<image x=\"0\" y=\"0\" width=\"{:.0}\" height=\"{:.0}\" href=\"data:image/png;base64,{}\"/>",
                        f.map[2], f.map[3], base64_encode(&map_png)
                    );
                    kanyu_render::layout::render_layout_svg(
                        lv_spec,
                        &img,
                        &legend,
                        scale.as_ref().map(|(l, b)| (l.as_str(), *b)),
                    )
                    .into_bytes()
                }
                _ => kanyu_render::layout::render_layout_png(
                    lv_spec,
                    &map_png,
                    &legend,
                    scale.as_ref().map(|(l, b)| (l.as_str(), *b)),
                )
                .map_err(|e| e.to_string())?,
            };
            std::fs::write(&out_path, bytes).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => {
                let msg = format!("布局「{lv_title}」已导出 → {out_path}");
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Err(e) => {
                self.console
                    .push(crate::console::LineKind::Err, format!("布局导出失败: {e}"));
                self.toast_err(format!("布局导出失败: {e}"));
            }
        }
    }

    /// 属性表单元格编辑：UpdateProperties 命令入编辑会话（无会话自动开启）。
    fn apply_cell_edit(&mut self, layer: &str, feature: usize, field: &str, text: &str) {
        let Some(i) = self.layer_index(layer) else {
            return;
        };
        let mut coll = self.layers[i].layer.collection();
        let Some(f) = coll.features.get_mut(feature) else {
            return;
        };
        let old = f.properties.clone();
        // 类型按可解析性：数值 > 布尔 > 空 > 文本。
        let t = text.trim();
        let v = if let Ok(n) = t.parse::<f64>() {
            serde_json::Number::from_f64(n)
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        } else if t == "true" || t == "false" {
            serde_json::Value::from(t == "true")
        } else if t.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::from(t)
        };
        f.properties
            .get_or_insert_with(Default::default)
            .insert(field.to_string(), v);
        let new = f.properties.clone();
        // 会话：同图层复用；他图层/无会话则自动开启（注释：替换会丢其历史，会话唯一约束）。
        let mut session = match self.edit_session.take() {
            Some(s) if s.target == layer => s,
            other => {
                if let Some(o) = &other {
                    self.console.info(format!(
                        "编辑会话已切换（放弃 {} 的会话历史）",
                        o.target_name
                    ));
                }
                crate::edit::EditSession::new(layer.to_string(), self.layers[i].file_name.clone())
            }
        };
        let cmd = kanyu_edit::UpdateProperties {
            index: feature,
            old,
            new,
        };
        match session.history.push(Box::new(cmd), &mut coll) {
            Ok(()) => {
                let id = self.layers[i].layer.id().to_string();
                self.layers[i].layer = Layer::from_collection(id, coll);
                self.layers[i].summary = self.layers[i].layer.summary();
                self.rebuild_merged();
                let msg = format!("单元格已更新（{layer}[{feature}].{field}）");
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Err(e) => {
                self.console
                    .push(crate::console::LineKind::Err, e.to_string());
                self.toast_err(e.to_string());
            }
        }
        self.edit_session = Some(session);
    }

    // ===== 停靠区渲染（dock.rs 编排，此处提供内容）=====

    /// 单个停靠区：页签条（拖动/关闭/全部开关）+ 当前页签内容。
    fn dock_zone_content(
        &mut self,
        ui: &mut egui::Ui,
        zone: crate::dock::DockZone,
        views: &[LayerView],
        selected_id: Option<&str>,
        out: &mut DockOutputs,
    ) {
        let panels = self.dock.panels_in(zone);
        let Some(active) = self.dock.active_in(zone) else {
            return;
        };
        ui.add_space(4.0);
        let strip = crate::dock::dock_tab_strip(ui, &panels, active, self.dock.dragging);
        if let Some(id) = strip.activated {
            self.dock.set_active(zone, id);
            self.mark_state_dirty();
        }
        if let Some(id) = strip.closed {
            self.dock.close_panel(id);
            self.console.info(format!("面板「{}」已关闭", id.title()));
            self.mark_state_dirty();
        }
        if let Some(id) = strip.drag_started {
            self.dock.dragging = Some(id);
        }
        if let Some(open) = strip.set_all_open {
            self.dock.set_all_open(open);
            self.mark_state_dirty();
        }
        ui.separator();
        // 当前页签内容（可能刚被关闭/拖走 → active_in 自动回落）。
        if let Some(active) = self.dock.active_in(zone) {
            self.panel_content(ui, active, views, selected_id, out);
        }
    }

    /// 面板内容渲染（注册表的"渲染回调"：新增面板在此加分支）。
    fn panel_content(
        &mut self,
        ui: &mut egui::Ui,
        id: crate::dock::PanelId,
        views: &[LayerView],
        selected_id: Option<&str>,
        out: &mut DockOutputs,
    ) {
        match id {
            crate::dock::PanelId::Catalog => {
                // 地图框分类数据：全部已建框（含已关闭；维度取各框现场）。
                let frame_rows: Vec<crate::catalog::FrameRow> = self
                    .frames
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let active = self.active_frame == Some(i);
                        crate::catalog::FrameRow {
                            index: i,
                            title: f.title.clone(),
                            dim_label: (if active { self.frame_dim } else { f.site.dim }).label(),
                            open: f.open,
                        }
                    })
                    .collect();
                let layout_rows: Vec<crate::catalog::LayoutRow> = self
                    .layouts
                    .iter()
                    .map(|l| crate::catalog::LayoutRow {
                        title: l.title.clone(),
                        open: l.open,
                        map: (!l.map.is_empty()).then(|| l.map.clone()),
                    })
                    .collect();
                let mut catalog = std::mem::take(&mut self.catalog);
                out.catalog.extend(catalog.ui(
                    ui,
                    &mut self.icon_cache,
                    &frame_rows,
                    &layout_rows,
                    &self.services,
                    &self.wms_services,
                ));
                self.catalog = catalog;
            }
            crate::dock::PanelId::Layers => {
                let mut filter = std::mem::take(&mut self.layer_filter);
                out.panel.extend(panels::layers_tree(
                    ui,
                    &self.toc,
                    views,
                    selected_id,
                    self.selected_group.as_deref(),
                    &mut filter,
                    &mut self.icon_cache,
                ));
                self.layer_filter = filter;
            }
            crate::dock::PanelId::Toolbox => {
                if let Some(tool_id) = self.toolbox.ui(ui, &mut self.icon_cache) {
                    if let Some(def) = crate::toolbox::find(tool_id) {
                        let mut st = crate::toolbox::ToolRunState::new(def);
                        // 投影变换的目标 CRS 默认取工程坐标系。
                        if def.id == "reproject" {
                            for (p, v) in def.params.iter().zip(st.values.iter_mut()) {
                                if p.key == "to" && v.is_empty() {
                                    *v = self.project_crs.clone();
                                }
                            }
                        }
                        // 创建网格的范围默认取当前数据范围（可改）。
                        if def.id == "create_grid" {
                            for (p, v) in def.params.iter().zip(st.values.iter_mut()) {
                                if p.key == "extent" && v.is_empty() {
                                    if let Some(b) = self.data_extent {
                                        *v = format!("{},{},{},{}", b[0], b[1], b[2], b[3]);
                                    }
                                }
                            }
                        }
                        self.tool_run = Some(st);
                    }
                }
                // 收藏/最近变更 → ui-state 脏标记（面板内部自管状态）。
                if self.toolbox.state_version() != self.last_toolbox_state {
                    self.last_toolbox_state = self.toolbox.state_version();
                    self.mark_state_dirty();
                }
            }
            crate::dock::PanelId::AttrTable => {
                let layers_meta: Vec<(String, String)> = self
                    .layers
                    .iter()
                    .map(|e| (e.layer.id().to_string(), e.file_name.clone()))
                    .collect();
                let collection = self
                    .attrtable
                    .layer()
                    .and_then(|id| self.layers.iter().find(|e| e.layer.id() == id))
                    .map(|e| e.layer.collection());
                let actions = self.attrtable.ui(ui, &layers_meta, collection.as_ref());
                for a in actions {
                    self.apply_attr_action(a);
                }
            }
            crate::dock::PanelId::Console => {
                let mut console = std::mem::take(&mut self.console);
                console.ui(ui, self);
                self.console = console;
            }
            crate::dock::PanelId::AiChat => {
                let mut ai_chat = std::mem::take(&mut self.ai_chat);
                ai_chat.ui(ui, self);
                self.ai_chat = ai_chat;
            }
        }
    }

    /// 应用工具执行结果（登记图层/报告/导出、终端日志、toast、最近使用）。
    fn apply_tool_outcome(
        &mut self,
        tool_id: &'static str,
        tool_name: &str,
        outcome: Result<crate::toolbox::ToolOutcome, String>,
    ) {
        let mut succeeded = false;
        match outcome {
            Ok(crate::toolbox::ToolOutcome::NewLayer {
                collection,
                base,
                verb,
            }) => {
                succeeded = true;
                let msg = self.add_result_layer(&base, collection, &verb);
                self.status = msg.clone();
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Ok(crate::toolbox::ToolOutcome::NewLayers { layers, verb }) => {
                // 多产出（分割矢量图层）：逐组登记，终端汇报组数。
                succeeded = true;
                let n = layers.len();
                let mut last_msg = String::new();
                for (base, collection) in layers {
                    last_msg = self.add_result_layer(&base, collection, &verb);
                }
                if n > 0 {
                    self.console.info(last_msg);
                }
                let msg = format!("{verb}：共 {n} 组");
                self.status = msg.clone();
                self.console.info(msg.clone());
                self.toast_ok(msg);
            }
            Ok(crate::toolbox::ToolOutcome::Report(text)) => {
                succeeded = true;
                self.console.info(text);
                self.toast_ok(format!("{tool_name} 完成"));
            }
            // 执行失败（AddError 对应）：终端 + toast。
            Err(e) => {
                self.console.push(
                    crate::console::LineKind::Err,
                    format!("工具「{tool_name}」失败: {e}"),
                );
                self.toast_err(format!("工具「{tool_name}」失败: {e}"));
            }
        }
        // 成功执行记入「最近使用」。
        if succeeded {
            self.toolbox.note_run(tool_id);
            self.mark_state_dirty();
        }
    }

    /// 工具后台执行：进度模态（不确定态 + 取消）+ 完成轮询。
    fn tool_progress_ui(&mut self, ctx: &egui::Context) {
        let Some(prog) = &mut self.tool_progress else {
            return;
        };
        // 完成轮询。
        match prog.rx.try_recv() {
            Ok(result) => {
                let prog = self.tool_progress.take().expect("轮询分支已保证存在");
                // 取消的句柄在点取消时已析构，这里恒为未取消。
                match crate::toolbox::completion_action(false, result.is_ok()) {
                    crate::toolbox::CompletionAction::Apply
                    | crate::toolbox::CompletionAction::ReportError => {
                        self.apply_tool_outcome(prog.tool_id, &prog.tool_name.clone(), result);
                    }
                    crate::toolbox::CompletionAction::Discard => {}
                }
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.tool_progress = None;
                return;
            }
        }
        // 进度模态（不确定态 + 可终止）。
        let name = self
            .tool_progress
            .as_ref()
            .map(|p| p.tool_name.clone())
            .unwrap_or_default();
        let mut cancel = false;
        egui::Window::new(crate::ui_kit::text::heading("正在执行"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(crate::ui_kit::text::body(format!("正在执行「{name}」…")));
                });
                ui.add_space(8.0);
                if crate::ui_kit::button(ui, "取 消", crate::ui_kit::ButtonVariant::Secondary, true)
                    .clicked()
                {
                    cancel = true;
                }
            });
        ctx.request_repaint(); // 进度动画/轮询持续推进
        if cancel {
            // 协作式取消简化语义：丢弃结果（接收端析构，线程结果被忽略）。
            let prog = self.tool_progress.take().expect("取消分支已保证存在");
            self.console
                .info(format!("已取消「{}」（后台结果将被丢弃）", prog.tool_name));
        }
    }

    // ===== 服务链接（WFS）=====

    /// 连接服务：后台线程拉取（10s 超时）+ 进度模态（复用工具运行模式，不卡 UI）。
    fn start_service_fetch(&mut self, conn: crate::services::WfsConnection) {
        if self.service_progress.is_some() {
            self.toast_err("已有服务连接进行中，请稍候");
            return;
        }
        self.console
            .info(format!("正在连接「{}」：{}", conn.name, conn.url));
        let (tx, rx) = std::sync::mpsc::channel();
        let name = conn.name.clone();
        // 结果上限 5 万条（超大结果集防御；fetch_wfs 内截断）。
        std::thread::spawn(move || {
            let _ = tx.send(crate::services::fetch_wfs(&conn, 50_000));
        });
        self.service_progress = Some(ServiceFetch { name, rx });
    }

    /// WFS 结果登记为内存图层（file_name=连接名，source_path=None）。
    fn add_service_layer(&mut self, name: &str, collection: FeatureCollection) {
        self.ensure_active_frame(); // 服务图层归属当前激活地图框
        let n = collection.features.len();
        let id = self.unique_id(name);
        let layer = Layer::from_collection(id.clone(), collection);
        let summary = layer.summary();
        let symbology = crate::symbology::default_single(&summary.geometry_types);
        self.layers.push(LayerEntry {
            layer,
            summary,
            visible: true,
            file_name: name.to_string(),
            expanded: false,
            source_path: None,
            symbology,
        });
        // 同 open_file 约定：新图层插入目录树顶并选中。
        toc::insert_layer_top(&mut self.toc, &id);
        self.selected = Some(self.layers.len() - 1);
        self.rebuild_merged();
        self.needs_fit = true;
        let msg = format!("已加载 {name}（{n} 要素，WFS 服务）");
        self.status = msg.clone();
        self.console.info(msg.clone());
        self.toast_ok(msg);
    }

    /// WFS 后台拉取：进度模态（不确定态 + 取消）+ 完成轮询。
    fn service_progress_ui(&mut self, ctx: &egui::Context) {
        let Some(prog) = &mut self.service_progress else {
            return;
        };
        match prog.rx.try_recv() {
            Ok(result) => {
                let prog = self.service_progress.take().expect("轮询分支已保证存在");
                match result {
                    Ok(collection) => self.add_service_layer(&prog.name, collection),
                    Err(e) => {
                        let msg = format!("服务链接「{}」失败: {e}", prog.name);
                        self.console.push(crate::console::LineKind::Err, &msg);
                        self.toast_err(&msg);
                    }
                }
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.service_progress = None;
                return;
            }
        }
        let name = self
            .service_progress
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let mut cancel = false;
        egui::Window::new(crate::ui_kit::text::heading("连接服务"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(crate::ui_kit::text::body(format!("正在连接「{name}」…")));
                });
                ui.add_space(8.0);
                if crate::ui_kit::button(ui, "取 消", crate::ui_kit::ButtonVariant::Secondary, true)
                    .clicked()
                {
                    cancel = true;
                }
            });
        ctx.request_repaint(); // 进度动画/轮询持续推进
        if cancel {
            // 同工具取消语义：丢弃结果（接收端析构，线程结果被忽略）。
            let prog = self.service_progress.take().expect("取消分支已保证存在");
            self.console
                .info(format!("已取消连接「{}」（后台结果将被丢弃）", prog.name));
        }
    }

    // ===== WMS 底图 =====

    /// WMS 底图驱动（每帧：轮询在途结果 + 按需发起新请求）。
    /// 仅作用激活框的二维画布；拉取失败 toast 中文错误，不阻断矢量渲染。
    fn wms_drive(&mut self, ctx: &egui::Context) {
        // 轮询在途结果。
        if let Some(fetch) = &self.wms_fetch {
            match fetch.rx.try_recv() {
                Ok(Ok(png)) => {
                    let key = fetch.key;
                    self.wms_fetch = None;
                    if let Err(e) = self.canvas.set_wms(key, &png, ctx) {
                        self.toast_err(&e);
                    }
                }
                Ok(Err(e)) => {
                    let key = fetch.key;
                    self.wms_fetch = None;
                    self.wms_failed = Some(key); // 视口/尺寸变化才重试（防每帧死循环）
                    self.toast_err(format!("WMS 底图加载失败: {e}"));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.wms_fetch = None;
                }
            }
        }
        let Some(fi) = self.active_frame else {
            return;
        };
        if self.frame_dim != crate::mapview::ViewDim::TwoD {
            return; // 三维场景不铺底图
        }
        let Some(name) = self.frames[fi].wms_base.clone() else {
            self.canvas.clear_wms(); // 未启用底图：清理残留纹理
            return;
        };
        let Some(conn) = self.wms_services.iter().find(|c| c.name == name).cloned() else {
            // 连接已删（删除时已清理引用，此处兜底）。
            self.frames[fi].wms_base = None;
            self.canvas.clear_wms();
            return;
        };
        let (Some(bbox), [w, h]) = (self.view_bbox, self.canvas.phys_px()) else {
            return;
        };
        if w == 0 || h == 0 {
            return;
        }
        let key = (bbox, w, h);
        if self.canvas.wms_key() == Some(key) {
            return; // 缓存已是最新
        }
        if self.wms_failed == Some(key) {
            return;
        }
        if self.wms_fetch.as_ref().is_some_and(|f| f.key == key) {
            return; // 在途
        }
        if let Some((t, last_bbox)) = &self.wms_last {
            if *last_bbox == bbox && t.elapsed() < Duration::from_secs(1) {
                return; // 同视口 1 秒去抖
            }
        }
        let url = crate::services::build_getmap_url(&conn.url, &conn.layer, bbox, w, h);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::services::fetch_wms_map(&url));
        });
        self.wms_fetch = Some(WmsFetch { key, rx });
        self.wms_last = Some((Instant::now(), bbox));
        self.wms_failed = None;
        ctx.request_repaint(); // 轮询持续推进
    }

    // ===== 帧处理辅助 =====

    fn error_modal(&mut self, ctx: &egui::Context) {
        if self.error_msg.is_none() {
            return;
        }
        let mut open = true;
        egui::Window::new(crate::ui_kit::text::heading("打开失败"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(self.error_msg.as_deref().unwrap_or_default());
                ui.add_space(8.0);
                if crate::ui_kit::button(ui, "确 定", crate::ui_kit::ButtonVariant::Primary, true)
                    .clicked()
                {
                    self.error_msg = None;
                }
            });
        if !open {
            self.error_msg = None;
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let paths: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for path in paths {
            self.open_file(&path);
        }
    }

    /// 窗口截图（验证模式退出 / 常规保存不退出共用回图通道）。
    fn handle_screenshots(&mut self, ctx: &egui::Context) {
        // 验证模式：保持帧流并按延时触发。
        if let Some(shot) = &mut self.screenshot {
            ctx.request_repaint();
            if !shot.requested && shot.start.elapsed() >= shot.delay {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                shot.requested = true;
            }
        }
        let mut image = None;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Screenshot { image: img, .. } = ev {
                    image = Some(img.clone());
                }
            }
        });
        if let Some(img) = image {
            // 验证模式优先（保存并关窗）。
            if let Some(shot) = self.screenshot.take() {
                match save_color_image_png(&img, &shot.out_path) {
                    Ok(()) => {
                        println!("截图已保存: {}", shot.out_path);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(e) => {
                        eprintln!("截图保存失败: {e}");
                        std::process::exit(1);
                    }
                }
            } else if let Some(path) = self.pending_window_shot.take() {
                match save_color_image_png(&img, &path) {
                    Ok(()) => self.console.info(format!("窗口截图已保存 → {path}")),
                    Err(e) => self
                        .console
                        .push(crate::console::LineKind::Err, format!("截图保存失败: {e}")),
                }
            }
        }
    }
}

impl eframe::App for KanyuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // 终端发起的主题切换在此统一应用。
        if self.theme_dirty {
            crate::theme::apply_theme(&ctx, self.theme);
            self.theme_dirty = false;
        }
        self.handle_dropped_files(&ctx);

        // 全局快捷键（Ctrl+Z/Y/Shift+Z 撤销重做、Ctrl+S 保存工程）；
        // 焦点在文本输入框时不拦截（text_edit_focused 守卫）。
        if !ctx.text_edit_focused() {
            let pressed = ctx.input(|i| {
                let m = i.modifiers;
                [egui::Key::Z, egui::Key::Y, egui::Key::S]
                    .into_iter()
                    .find(|k| i.key_pressed(*k))
                    .and_then(|k| crate::commands::match_shortcut(k, m.command, m.shift))
            });
            match pressed {
                Some(crate::commands::AppShortcut::Undo) => self.edit_undo(false),
                Some(crate::commands::AppShortcut::Redo) => self.edit_undo(true),
                Some(crate::commands::AppShortcut::SaveProject) => {
                    self.dispatch(RibbonAction::SaveProject, &ctx)
                }
                None => {}
            }
        }

        // Ribbon 功能区（顶部，86px）。命令可用条件按快照求值（无图层/无选中置灰）。
        let snap = crate::commands::AppSnapshot {
            layer_count: self.layers.len(),
            has_selection: self.selected.is_some(),
            editing: self.edit_session.is_some(),
            can_undo: self
                .edit_session
                .as_ref()
                .map(|s| s.history.can_undo())
                .unwrap_or(false),
            can_redo: self
                .edit_session
                .as_ref()
                .map(|s| s.history.can_redo())
                .unwrap_or(false),
        };
        egui::Panel::top("ribbon")
            .exact_size(sizes::RIBBON)
            .show(ui, |ui| {
                if let Some(action) = self.ribbon.ui(ui, &mut self.icon_cache, &snap) {
                    self.dispatch(action, &ctx);
                }
            });

        // StatusBar（最底；先注册的 bottom 面板居最下）。
        let span = self.view_bbox.map(|b| b[2] - b[0]);
        // 编辑会话指示（ArcGIS 状态栏语义）。
        let status = match &self.edit_session {
            Some(s) => {
                let tool = match &s.drawing {
                    Some(d) => format!("{}（绘制中 {} 点）", s.tool.label(), d.verts.len()),
                    None => s.tool.label().to_string(),
                };
                format!(
                    "编辑中: {}（{} 步可撤销，工具: {}，捕捉: {}） | {}",
                    s.target_name,
                    s.history.len(),
                    tool,
                    if s.snap { "开" } else { "关" },
                    self.status
                )
            }
            None => self.status.clone(),
        };
        let count = self.visible_feature_count();
        let mouse = self.mouse_data;
        panels::status_bar(
            ui,
            &status,
            mouse,
            count,
            span,
            self.map_theme_mode.label(),
            &self.project_crs,
        );

        // ===== 停靠系统（左/右/底停靠区 + 浮动窗，数据驱动，见 dock.rs）=====

        // 内容区 = Ribbon 与状态栏之间（投放判定/提示都以它为基准）。
        let screen = ctx.content_rect();
        let content_area = egui::Rect::from_min_max(
            egui::pos2(screen.min.x, screen.min.y + sizes::RIBBON),
            egui::pos2(screen.max.x, screen.max.y - sizes::STATUS_BAR),
        );

        // 1) 拖放结算（松开瞬间，布局前处理，本帧即按新停靠渲染）。
        if let Some(drag_id) = self.dock.dragging {
            if !ctx.input(|i| i.pointer.primary_down()) {
                self.dock.dragging = None;
                if let Some(pos) = ctx.pointer_latest_pos() {
                    // 源区矩形（"源区取消"规则：在源面板矩形内松开不触发浮动）。
                    let origin = match self.dock.zone_of(drag_id) {
                        crate::dock::DockZone::Floating => None,
                        zone => self.dock.zone_rects[zone.docked_index()].map(|r| (zone, r)),
                    };
                    match crate::dock::resolve_drop(pos, content_area, origin) {
                        Some(crate::dock::DockZone::Floating) => {
                            self.dock.float(drag_id);
                            self.console
                                .info(format!("面板「{}」已转为浮动窗口", drag_id.title()));
                            self.mark_state_dirty();
                        }
                        Some(zone) => {
                            self.dock.dock_to(drag_id, zone);
                            self.console
                                .info(format!("面板「{}」已停靠", drag_id.title()));
                            self.mark_state_dirty();
                        }
                        None => {}
                    }
                }
            }
        } else {
            // 拖拽也可能始于浮动窗标题条（页签内开始由页签条上报）：
            // 以「按下起点」命中标题条（当前指针早已移离标题条）。
            let (dragging_ptr, press_origin) =
                ctx.input(|i| (i.pointer.is_decidedly_dragging(), i.pointer.press_origin()));
            if dragging_ptr {
                if let Some(pos) = press_origin {
                    if let Some(id) = self.dock.hit_floating_title(pos) {
                        self.dock.dragging = Some(id);
                    }
                }
            }
        }

        // 2) 停靠区布局（底 → 右 → 左；egui 后注册的面板占剩余空间，与旧布局一致）。
        let views = self.layer_views();
        let selected_id = self
            .selected
            .and_then(|i| self.layers.get(i))
            .map(|e| e.layer.id().to_string());
        let mut dock_out = DockOutputs::default();
        for zone in [
            crate::dock::DockZone::Bottom,
            crate::dock::DockZone::Right,
            crate::dock::DockZone::Left,
        ] {
            self.dock.zone_rects[zone.docked_index()] = None;
            if !self.dock.zone_has_panels(zone) {
                continue; // 区内面板全关 → 整区隐藏
            }
            let content = |ui: &mut egui::Ui, app: &mut Self, out: &mut DockOutputs| {
                app.dock_zone_content(ui, zone, &views, selected_id.as_deref(), out);
            };
            let resp = match zone {
                crate::dock::DockZone::Bottom => egui::Panel::bottom("dock_bottom")
                    .default_size(200.0)
                    .size_range(120.0..=420.0)
                    .show(ui, |ui| content(ui, self, &mut dock_out)),
                crate::dock::DockZone::Right => egui::Panel::right("dock_right")
                    .default_size(280.0)
                    .size_range(200.0..=480.0)
                    .show(ui, |ui| content(ui, self, &mut dock_out)),
                crate::dock::DockZone::Left => egui::Panel::left("dock_left")
                    .default_size(280.0)
                    .size_range(200.0..=480.0)
                    .show(ui, |ui| content(ui, self, &mut dock_out)),
                crate::dock::DockZone::Floating => unreachable!("浮动区无停靠面板"),
            };
            self.dock.zone_rects[zone.docked_index()] = Some(resp.response.rect);
        }

        // 3) 浮动窗（egui::Window：可拖动/缩放/原生 × 关闭）。
        for id in crate::dock::PanelId::ALL {
            if !self.dock.is_floating(id) {
                continue;
            }
            let mut open = true;
            let resp = egui::Window::new(id.title())
                .default_size([380.0, 320.0])
                .open(&mut open)
                .show(&ctx, |ui| {
                    self.panel_content(ui, id, &views, selected_id.as_deref(), &mut dock_out);
                });
            if let Some(inner) = &resp {
                self.dock.float_rects[id.index()] = Some(inner.response.rect);
            }
            if !open {
                self.dock.close_panel(id);
            }
        }

        // 停靠编排产生的动作统一结算。
        for action in dock_out.catalog {
            match action {
                // .kyu 走工程恢复，其余走数据加载。
                crate::catalog::CatalogAction::LoadFile(path) => {
                    if path
                        .extension()
                        .map(|e| e.eq_ignore_ascii_case("kyu"))
                        .unwrap_or(false)
                    {
                        self.open_project(&path);
                    } else {
                        self.open_file(&path);
                    }
                }
                crate::catalog::CatalogAction::ActivateFrame(i) => {
                    self.activate_frame(i);
                }
                crate::catalog::CatalogAction::NewFrame2D => {
                    self.dispatch(RibbonAction::NewFrame2D, &ctx)
                }
                crate::catalog::CatalogAction::NewFrame3D => {
                    self.dispatch(RibbonAction::NewFrame3D, &ctx)
                }
                crate::catalog::CatalogAction::RenameFrame(i) => {
                    if i < self.frames.len() {
                        self.rename_frame_dlg = Some((i, self.frames[i].title.clone()));
                    }
                }
                crate::catalog::CatalogAction::DeleteFrame(i) => {
                    self.delete_frame(i);
                }
                crate::catalog::CatalogAction::ActivateLayout(i) => {
                    if i < self.layouts.len() {
                        self.layouts[i].open = true; // 关闭≠删除：重开页签
                        self.active_layout = Some(i);
                    }
                }
                crate::catalog::CatalogAction::DeleteLayout(i) => {
                    if i < self.layouts.len() {
                        let title = self.layouts[i].title.clone();
                        self.layouts.remove(i);
                        self.active_layout =
                            crate::mapview::adjust_active_after_remove(self.active_layout, i);
                        self.console.info(format!("已删除布局「{title}」"));
                        self.mark_state_dirty();
                    }
                }
                crate::catalog::CatalogAction::NewLayout => {
                    // 绑定地图框默认取当前激活框（可改「跟随当前地图框」）。
                    let map = self
                        .active_frame
                        .map(|i| self.frames[i].title.clone())
                        .unwrap_or_default();
                    self.layout_dlg = Some(crate::layoutview::LayoutDialogState {
                        title: format!("布局 {}", self.next_layout_id),
                        map,
                        ..Default::default()
                    });
                }
                crate::catalog::CatalogAction::NewService => {
                    self.service_dlg = Some(crate::services::ServiceDialogState {
                        name: format!("WFS 服务 {}", self.services.len() + 1),
                        ..Default::default()
                    });
                }
                crate::catalog::CatalogAction::DeleteService(i) => {
                    if i < self.services.len() {
                        let conn = self.services.remove(i);
                        self.console
                            .info(format!("已删除服务链接「{}」", conn.name));
                        self.mark_state_dirty();
                    }
                }
                crate::catalog::CatalogAction::ConnectService(i) => {
                    if let Some(conn) = self.services.get(i).cloned() {
                        self.start_service_fetch(conn);
                    }
                }
                crate::catalog::CatalogAction::EditService(i) => {
                    if let Some(conn) = self.services.get(i) {
                        let (base, layer) = crate::services::split_getfeature_url(&conn.url);
                        self.service_dlg = Some(crate::services::ServiceDialogState {
                            kind: crate::services::ServiceKind::Wfs,
                            name: conn.name.clone(),
                            url: base,
                            layer,
                            editing: Some(crate::services::ServiceEditTarget {
                                kind: crate::services::ServiceKind::Wfs,
                                index: i,
                            }),
                            ..Default::default()
                        });
                    }
                }
                crate::catalog::CatalogAction::EditWms(i) => {
                    if let Some(conn) = self.wms_services.get(i) {
                        self.service_dlg = Some(crate::services::ServiceDialogState {
                            kind: crate::services::ServiceKind::Wms,
                            name: conn.name.clone(),
                            url: conn.url.clone(),
                            layer: conn.layer.clone(),
                            editing: Some(crate::services::ServiceEditTarget {
                                kind: crate::services::ServiceKind::Wms,
                                index: i,
                            }),
                            ..Default::default()
                        });
                    }
                }
                crate::catalog::CatalogAction::DeleteWms(i) => {
                    if i < self.wms_services.len() {
                        let conn = self.wms_services.remove(i);
                        // 清理各框底图引用（激活框画布纹理同步清）。
                        for f in &mut self.frames {
                            if f.wms_base.as_deref() == Some(conn.name.as_str()) {
                                f.wms_base = None;
                            }
                        }
                        self.canvas.clear_wms();
                        self.console
                            .info(format!("已删除 WMS 连接「{}」", conn.name));
                        self.mark_state_dirty();
                    }
                }
                crate::catalog::CatalogAction::WmsBaseOn(i) => {
                    let Some(fi) = self.active_frame else {
                        self.toast_err("无打开的地图框——请先激活一个地图框");
                        continue;
                    };
                    if let Some(conn) = self.wms_services.get(i) {
                        self.frames[fi].wms_base = Some(conn.name.clone());
                        self.canvas.clear_wms(); // 强制按当前视口重新请求
                        self.wms_failed = None;
                        let msg =
                            format!("地图框「{}」底图 → {}", self.frames[fi].title, conn.name);
                        self.console.info(msg.clone());
                        self.toast_ok(msg);
                        self.mark_state_dirty();
                    }
                }
                crate::catalog::CatalogAction::WmsBaseOff => {
                    if let Some(fi) = self.active_frame {
                        if self.frames[fi].wms_base.take().is_some() {
                            self.canvas.clear_wms();
                            self.console
                                .info(format!("地图框「{}」已取消底图", self.frames[fi].title));
                            self.mark_state_dirty();
                        }
                    }
                }
            }
        }
        for action in dock_out.panel {
            self.dispatch_panel_action(action);
        }

        // 中央：视图页签条（文档页签范式）+ 当前页签内容。
        // 页签 = 打开且吸附的地图框 + 打开的布局；主框「地图」可关闭、不可弹出
        // （唯一特权：不可删除——见 delete_frame）；关闭 ≠ 删除（目录保留弱色行）。
        use crate::mapview::CentralTabKey;
        let mut docked_tabs: Vec<(CentralTabKey, String)> = self
            .frames
            .iter()
            .enumerate()
            .filter(|(_, f)| f.open && f.docked)
            .map(|(i, f)| (CentralTabKey::Map(i), f.title.clone()))
            .collect();
        docked_tabs.extend(
            self.layouts
                .iter()
                .enumerate()
                .filter(|(_, l)| l.open)
                .map(|(i, l)| (CentralTabKey::Layout(i), l.title.clone())),
        );
        let strip_active = if let Some(l) = self.active_layout {
            Some(CentralTabKey::Layout(l))
        } else {
            self.active_frame.map(CentralTabKey::Map)
        };
        let strip_out =
            ui.scope(|ui| crate::mapview::view_tab_strip(ui, &docked_tabs, strip_active));
        self.view_strip_rect = Some(strip_out.response.rect);
        let act = strip_out.inner;
        if let Some(sel) = act.activated {
            match sel {
                CentralTabKey::Map(i) => self.activate_frame(i),
                CentralTabKey::Layout(i) => {
                    self.active_layout = Some(i);
                }
            }
            self.mark_state_dirty();
        }
        if let Some(i) = act.floated {
            if i < self.frames.len() {
                self.frames[i].docked = false;
                if self.active_frame == Some(i) {
                    // 浮动即让出中央：休眠本框，回落主框（主框已关则中央空态）。
                    self.park_frame(i);
                    self.active_frame = None;
                    if self.frames[0].open {
                        self.activate_frame(0);
                    }
                }
                self.mark_state_dirty();
            }
        }
        if let Some(key) = act.closed {
            match key {
                CentralTabKey::Map(i) => self.close_frame(i),
                CentralTabKey::Layout(i) if i < self.layouts.len() => {
                    self.layouts[i].open = false;
                    if self.active_layout == Some(i) {
                        self.active_layout = None;
                    }
                    let title = self.layouts[i].title.clone();
                    self.console
                        .info(format!("已关闭布局「{title}」（目录中保留，双击行重开）"));
                    self.mark_state_dirty();
                }
                _ => {}
            }
        }
        if act.new_view_2d {
            self.dispatch(RibbonAction::NewFrame2D, &ctx);
        }
        if act.new_view_3d {
            self.dispatch(RibbonAction::NewFrame3D, &ctx);
        }

        // 当前页签内容：布局 > 激活地图框 > 空态引导。
        if let Some(li) = self.active_layout {
            if li < self.layouts.len() {
                self.layout_view_content(ui, li, &ctx);
            } else {
                self.active_layout = None;
            }
        } else if let Some(fi) = self.active_frame {
            // 激活地图框：二维/三维工具条 + 画布（主框与后续框同一渲染路径——
            // 功能性一致）；编辑会话仅作用激活框二维态。
            let edit_view = if self.frame_dim == crate::mapview::ViewDim::TwoD {
                self.edit_session.as_ref().map(|s| crate::canvas::EditView {
                    tool: s.tool,
                    target: s.target.as_str(),
                    selected: s.selected,
                    drawing: s.drawing.as_ref(),
                    snap: s.snap,
                })
            } else {
                None
            };
            let slices = build_layer_slices(&self.render_cache);
            // 空框给拖入引导（启动 onboarding），非空给通用提示。
            let empty_hint = if self.layers.is_empty() {
                "◇ 堪舆\n\n拖入数据文件，或经「主页 → 打开数据…」\n支持 shp / geojson / fgb / parquet / dxf / dwg / kml / kmz / csv / tsv / xlsx"
            } else {
                "（本地图框无可见图层）"
            };
            let vinput = crate::mapview::ViewInput {
                layers: &slices,
                theme: self.effective_map_theme(),
                data_extent: self.fit_extent.or(self.data_extent),
                edit: edit_view,
                empty_hint,
            };
            let out = crate::mapview::content_ui(
                ui,
                crate::mapview::FrameState {
                    dim: &mut self.frame_dim,
                    view_bbox: &mut self.view_bbox,
                    needs_fit: &mut self.needs_fit,
                    canvas: &mut self.canvas,
                    scene: &mut self.scene,
                    docked: &mut self.frames[fi].docked,
                },
                vinput,
                false,
            );
            self.mouse_data = out.mouse_data;
            if out.fit_consumed {
                self.fit_extent = None;
            }
            if let Some(e) = out.render_error {
                self.status = e;
            }
            if let Some(action) = out.edit_action {
                self.apply_edit_action(action);
            }
        } else {
            // 全部地图框已关闭：中央空态引导（铺底纯白与画布一致——无暗底透出）。
            let rect = ui.available_rect_before_wrap();
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::WHITE);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "◇ 堪舆\n\n无打开的地图框——目录「地图框」双击行重开，或 ＋ 新建",
                egui::FontId::proportional(16.0),
                crate::theme::palette(kanyu_render::Theme::Light).text_weak,
            );
        }

        // 浮动地图框窗口（open && !docked；× 关闭 = open=false，目录保留）。
        // 浮动框恒为休眠框：图层切片读自身 site（window_ui 内构建）。
        {
            let theme = self.effective_map_theme();
            let crs = self.project_crs.clone();
            let mut frames = std::mem::take(&mut self.frames);
            for f in &mut frames {
                if f.open && !f.docked {
                    crate::mapview::window_ui(&ctx, f, theme, &crs);
                }
            }
            self.frames = frames;
        }

        // 浮动视图拖到中央页签条 → 吸附（标题条拖拽判定同 dock 浮动窗）。
        if self.dragging_view.is_none() && ctx.input(|i| i.pointer.is_decidedly_dragging()) {
            if let Some(origin) = ctx.input(|i| i.pointer.press_origin()) {
                if let Some(idx) = self.frames.iter().position(|f| {
                    !f.docked
                        && f.open
                        && f.win_rect.is_some_and(|r| {
                            egui::Rect::from_min_max(r.min, egui::pos2(r.max.x, r.min.y + 28.0))
                                .contains(origin)
                        })
                }) {
                    self.dragging_view = Some(idx);
                }
            }
        }
        if let Some(idx) = self.dragging_view {
            if ctx.input(|i| i.pointer.primary_down()) {
                if let Some(strip) = self.view_strip_rect {
                    crate::mapview::paint_dock_hint(ui, strip);
                }
            } else {
                self.dragging_view = None;
                if let (Some(pos), Some(strip)) = (ctx.pointer_latest_pos(), self.view_strip_rect) {
                    if strip.contains(pos) && idx < self.frames.len() {
                        let title = self.frames[idx].title.clone();
                        self.activate_frame(idx); // 吸附即激活（含 docked/open 置位）
                        self.console.info(format!("地图框「{title}」已吸附到中央"));
                    }
                }
            }
        }

        // WMS 底图驱动（轮询 + 按需请求；须在中央内容渲染之后——视口已更新）。
        self.wms_drive(&ctx);

        // 4) 拖拽中的投放提示（Foreground 层：三边缘投放区 + 中央浮动提示）。
        if self.dock.dragging.is_some() {
            let pointer = ctx.pointer_latest_pos();
            let origin = self
                .dock
                .dragging
                .and_then(|id| match self.dock.zone_of(id) {
                    crate::dock::DockZone::Floating => None,
                    zone => self.dock.zone_rects[zone.docked_index()].map(|r| (zone, r)),
                });
            crate::dock::paint_drop_hints(
                &ctx,
                &crate::theme::palette(self.theme),
                content_area,
                pointer,
                origin,
            );
        }

        // 对话框与模态。
        let layer_ids = self.layer_ids();
        let skill_ids: Vec<String> = self.skill_metas.iter().map(|g| g.id.clone()).collect();
        if let Some(result) = self.dialogs.ui(&ctx, &layer_ids, &skill_ids) {
            self.dispatch_dialog_result(result);
        }
        // 重命名/新建组 模态（目录树右键「重命名…」「移至分组 ▸ 新建组…」）。
        if let Some(mut state) = self.rename.take() {
            let title = match &state.target {
                RenameTarget::Layer(_) => "重命名图层",
                RenameTarget::Group(_) => "重命名组",
                RenameTarget::NewGroupForLayer(_) => "新建组",
            };
            match crate::ui_kit::dialog_shell(&ctx, title, |ui| {
                crate::ui_kit::text_input(ui, "名称", &mut state.name, "输入名称", true);
            }) {
                crate::ui_kit::DialogAction::None => self.rename = Some(state),
                crate::ui_kit::DialogAction::Cancel => {}
                crate::ui_kit::DialogAction::Ok => self.apply_rename(state),
            }
        }
        // 设置对话框（坐标系 / 渲染）。
        if let Some(dlg) = self.settings.take() {
            let mut dlg = dlg;
            match dlg.ui(&ctx) {
                crate::settings::SettingsUi::Open => self.settings = Some(dlg),
                crate::settings::SettingsUi::Closed => {}
                crate::settings::SettingsUi::Applied(o) => {
                    self.project_crs = o.crs.clone();
                    self.map_export_size = o.export_size;
                    self.map_export_style = o.export_style;
                    self.map_theme_mode = o.map_theme;
                    // 界面缩放：egui 全局 zoom_factor（点单位等比放大）。
                    self.ui_zoom = o.ui_zoom;
                    ctx.set_zoom_factor(o.ui_zoom);
                    self.canvas.dirty = true;
                    self.mark_state_dirty();
                    let msg = format!(
                        "设置已应用（坐标系 {}，导出 {}×{}，{}）",
                        o.crs,
                        o.export_size.0,
                        o.export_size.1,
                        o.map_theme.label()
                    );
                    self.status = msg.clone();
                    self.console.info(msg);
                }
            }
        }
        // 工具箱参数对话框（ArcGIS Pro 式：参数帮助区 + 内联校验 + 运行置灰）。
        if let Some(mut st) = self.tool_run.take() {
            let layer_ids = self.layer_ids();
            match crate::toolbox::run_dialog(&ctx, &mut st, &layer_ids, &|id| {
                self.layers
                    .iter()
                    .find(|e| e.layer.id() == id)
                    .map(|e| e.summary.fields.clone())
                    .unwrap_or_default()
            }) {
                crate::toolbox::DialogOutcome::Open => self.tool_run = Some(st),
                crate::toolbox::DialogOutcome::Cancel => {}
                crate::toolbox::DialogOutcome::Run => {
                    // 后台线程执行（进度模态轮询，UI 不卡死）；图层数据先克隆入包。
                    let tool_id = st.tool.id;
                    let tool_name = st.tool.name.to_string();
                    let values = st.values.clone();
                    let mut data: std::collections::HashMap<String, FeatureCollection> =
                        std::collections::HashMap::new();
                    for (p, v) in st.tool.params.iter().zip(values.iter()) {
                        match p.kind {
                            kanyu_core::tooldef::ParamKind::Layer => {
                                if let Some(e) = self.layers.iter().find(|e| e.layer.id() == *v) {
                                    data.insert(v.clone(), e.layer.collection());
                                }
                            }
                            kanyu_core::tooldef::ParamKind::MultiLayers => {
                                for id in kanyu_core::toolrun::parse_multi_layers(v) {
                                    if let Some(e) = self.layers.iter().find(|e| e.layer.id() == id)
                                    {
                                        data.insert(id, e.layer.collection());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let r =
                            crate::toolbox::run_tool(tool_id, &values, |id| data.get(id).cloned());
                        let _ = tx.send(r);
                    });
                    self.tool_progress = Some(crate::toolbox::ToolProgress {
                        tool_id,
                        tool_name: tool_name.clone(),
                        rx,
                    });
                    // 日志（AddMessage 对应）：工具名 + 参数摘要。
                    self.console.info(format!(
                        "运行工具「{}」（{}）",
                        tool_name,
                        st.values.join("；")
                    ));
                }
            }
        }
        // 图层属性页（常规/源/字段/符号化）。
        self.layer_props_ui(&ctx);
        // 新建布局对话框。
        if let Some(mut dlg) = self.layout_dlg.take() {
            // 绑定地图框下拉选项（首项 = 跟随当前激活框）。
            let follow = "（跟随当前地图框）".to_string();
            let mut map_options = vec![follow.clone()];
            map_options.extend(self.frames.iter().map(|f| f.title.clone()));
            let action = crate::ui_kit::dialog_shell(&ctx, "新建布局框", |ui| {
                crate::ui_kit::text_input(ui, "标题", &mut dlg.title, "如 示范区总图", true);
                let mut ls = dlg.landscape;
                ui.horizontal(|ui| {
                    ui.label(crate::ui_kit::text::body("纸张:"));
                    if ui
                        .selectable_label(ls, crate::ui_kit::text::body("A4 横"))
                        .clicked()
                    {
                        ls = true;
                    }
                    if ui
                        .selectable_label(!ls, crate::ui_kit::text::body("A4 纵"))
                        .clicked()
                    {
                        ls = false;
                    }
                });
                dlg.landscape = ls;
                crate::ui_kit::checkbox(ui, &mut dlg.legend, "图例");
                crate::ui_kit::checkbox(ui, &mut dlg.scalebar, "比例尺");
                crate::ui_kit::checkbox(ui, &mut dlg.north, "指北针");
                // 地图框绑定（布局内容 = 该框图层集；不随激活框切换而变）。
                let mut choice = if dlg.map.is_empty() {
                    follow.clone()
                } else {
                    dlg.map.clone()
                };
                crate::ui_kit::combo(ui, "地图框", &mut choice, &map_options, true);
                dlg.map = if choice == follow {
                    String::new()
                } else {
                    choice
                };
            });
            match action {
                crate::ui_kit::DialogAction::Ok => match dlg.validate() {
                    Ok(()) => {
                        let id = self.next_layout_id;
                        self.next_layout_id += 1;
                        let title = dlg.title.trim().to_string();
                        let mut lv =
                            crate::layoutview::LayoutView::new(id, title.clone(), dlg.to_spec());
                        lv.map = dlg.map.clone();
                        self.layouts.push(lv);
                        self.active_layout = Some(self.layouts.len() - 1);
                        let bind = if dlg.map.is_empty() {
                            "跟随当前地图框".to_string()
                        } else {
                            format!("绑定「{}」", dlg.map)
                        };
                        self.console
                            .info(format!("已新建布局「{title}」（{bind}）"));
                        self.mark_state_dirty();
                    }
                    Err(e) => {
                        self.toast_err(&e);
                        self.console.push(crate::console::LineKind::Err, e);
                    }
                },
                crate::ui_kit::DialogAction::Cancel => {} // 取消：丢弃
                crate::ui_kit::DialogAction::None => self.layout_dlg = Some(dlg), // 继续显示
            }
        }
        // 重命名地图框对话框。
        if let Some((i, mut title)) = self.rename_frame_dlg.take() {
            let action = crate::ui_kit::dialog_shell(&ctx, "重命名地图框", |ui| {
                crate::ui_kit::text_input(ui, "标题", &mut title, "如 示范区三维", true);
            });
            match action {
                crate::ui_kit::DialogAction::Ok => {
                    let t = title.trim();
                    if t.is_empty() {
                        self.toast_err("地图框标题不能为空");
                        self.rename_frame_dlg = Some((i, title)); // 保留输入
                    } else if i < self.frames.len() {
                        let old = std::mem::replace(&mut self.frames[i].title, t.to_string());
                        self.console
                            .info(format!("地图框「{old}」已重命名为「{t}」"));
                        self.mark_state_dirty();
                    }
                }
                crate::ui_kit::DialogAction::Cancel => {} // 取消：丢弃
                crate::ui_kit::DialogAction::None => self.rename_frame_dlg = Some((i, title)),
            }
        }
        // 新建服务链接对话框（WFS：基址 + GetCapabilities 图层发现；WMS：基址 + 图层名）。
        if let Some(mut dlg) = self.service_dlg.take() {
            // 清单后台拉取轮询（对话框常显期间每帧检查）。
            let mut caps_done: Option<Result<Vec<crate::services::WfsLayerInfo>, String>> = None;
            if let Some(rx) = &dlg.caps_rx {
                match rx.try_recv() {
                    Ok(r) => caps_done = Some(r),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        dlg.caps_rx = None;
                        dlg.caps_note = Some("清单拉取中断".to_string());
                    }
                }
            }
            if let Some(r) = caps_done {
                dlg.caps_rx = None;
                match r {
                    Ok(list) => {
                        dlg.caps_note = Some(format!("已发现 {} 个图层", list.len()));
                        if let Some(first) = list.first() {
                            dlg.layer = first.name.clone();
                        }
                        dlg.caps = list;
                    }
                    Err(e) => {
                        dlg.caps_note = Some(e.clone());
                        self.toast_err(&e);
                    }
                }
            }
            let mut fetch_caps = false;
            let dlg_title = if dlg.editing.is_some() {
                "编辑服务链接"
            } else {
                "新建服务链接"
            };
            let action = crate::ui_kit::dialog_shell(&ctx, dlg_title, |ui| {
                // 类型切换（切型清空图层选择——WFS/WMS 图层语义不同）。
                let kind_label = dlg.kind.label().to_string();
                let mut choice_kind = kind_label.clone();
                crate::ui_kit::combo_static(
                    ui,
                    "类型",
                    &mut choice_kind,
                    &[
                        crate::services::ServiceKind::Wfs.label(),
                        crate::services::ServiceKind::Wms.label(),
                    ],
                    true,
                );
                if choice_kind != kind_label {
                    dlg.kind = if choice_kind == crate::services::ServiceKind::Wms.label() {
                        crate::services::ServiceKind::Wms
                    } else {
                        crate::services::ServiceKind::Wfs
                    };
                    dlg.layer.clear();
                    dlg.caps.clear();
                    dlg.caps_note = None;
                }
                crate::ui_kit::text_input(ui, "名称", &mut dlg.name, "如 全国区县 WFS", true);
                crate::ui_kit::text_input(
                    ui,
                    "基址",
                    &mut dlg.url,
                    "https://…/geoserver/wfs（不含查询串）",
                    true,
                );
                match dlg.kind {
                    crate::services::ServiceKind::Wfs => {
                        ui.horizontal(|ui| {
                            let busy = dlg.caps_rx.is_some();
                            if busy {
                                ui.add(egui::Spinner::new());
                            }
                            if crate::ui_kit::button(
                                ui,
                                "获取图层清单",
                                crate::ui_kit::ButtonVariant::Secondary,
                                !busy && !dlg.url.trim().is_empty(),
                            )
                            .clicked()
                            {
                                fetch_caps = true;
                            }
                        });
                        if let Some(note) = &dlg.caps_note {
                            crate::ui_kit::hint_caption(ui, note);
                        }
                        if !dlg.caps.is_empty() {
                            let options: Vec<String> = dlg
                                .caps
                                .iter()
                                .map(|c| match &c.title {
                                    Some(t) => format!("{t}（{}）", c.name),
                                    None => c.name.clone(),
                                })
                                .collect();
                            let idx = dlg
                                .caps
                                .iter()
                                .position(|c| c.name == dlg.layer)
                                .unwrap_or(0);
                            let mut choice = options[idx].clone();
                            crate::ui_kit::combo(ui, "图层", &mut choice, &options, true);
                            if let Some(i) = options.iter().position(|o| o == &choice) {
                                dlg.layer = dlg.caps[i].name.clone();
                            }
                        } else {
                            crate::ui_kit::text_input(
                                ui,
                                "图层",
                                &mut dlg.layer,
                                "typeNames，如 demo:blocks（可先获取清单选择）",
                                true,
                            );
                        }
                        crate::ui_kit::hint_caption(
                            ui,
                            "确定后按 GetFeature（GeoJSON 输出）构造完整请求地址；\
                             双击目录中的连接名即加载为图层。",
                        );
                    }
                    crate::services::ServiceKind::Wms => {
                        crate::ui_kit::text_input(
                            ui,
                            "图层",
                            &mut dlg.layer,
                            "layers，如 ne:countries",
                            true,
                        );
                        crate::ui_kit::hint_caption(
                            ui,
                            "WMS 影像按当前视口 GetMap 拉取（EPSG:4326 / PNG）；\
                             建好后在目录右键「设为当前框底图」。",
                        );
                    }
                }
            });
            if fetch_caps {
                let base = dlg.url.trim().to_string();
                dlg.caps_note = Some("正在获取图层清单…".to_string());
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::services::fetch_capabilities(&base));
                });
                dlg.caps_rx = Some(rx);
                self.service_dlg = Some(dlg);
            } else {
                match action {
                    crate::ui_kit::DialogAction::Ok => {
                        let result: Result<(), String> = match dlg.kind {
                            crate::services::ServiceKind::Wfs => {
                                if let Err(e) =
                                    crate::services::validate_connection(&dlg.name, &dlg.url)
                                {
                                    Err(e)
                                } else if dlg.layer.trim().is_empty() {
                                    Err("图层名不能为空（获取清单选择或手输 typeNames）"
                                        .to_string())
                                } else {
                                    Ok(())
                                }
                            }
                            crate::services::ServiceKind::Wms => {
                                crate::services::validate_wms(&dlg.name, &dlg.url, &dlg.layer)
                            }
                        };
                        match result {
                            Ok(()) => {
                                let name = dlg.name.trim().to_string();
                                match dlg.editing {
                                    // 编辑既有连接（更新入 ui-state 清单）。
                                    Some(t) => match t.kind {
                                        crate::services::ServiceKind::Wfs
                                            if t.index < self.services.len() =>
                                        {
                                            let url = crate::services::build_getfeature_url(
                                                &dlg.url, &dlg.layer,
                                            );
                                            let c = &mut self.services[t.index];
                                            c.name = name.clone();
                                            c.url = url;
                                            self.console.info(format!(
                                                "已更新服务链接「{name}」（WFS 图层 {}）",
                                                dlg.layer.trim()
                                            ));
                                        }
                                        crate::services::ServiceKind::Wms
                                            if t.index < self.wms_services.len() =>
                                        {
                                            let c = &mut self.wms_services[t.index];
                                            let old = std::mem::replace(&mut c.name, name.clone());
                                            c.url = dlg.url.trim().to_string();
                                            c.layer = dlg.layer.trim().to_string();
                                            // 改名同步各框底图引用。
                                            if old != name {
                                                for f in &mut self.frames {
                                                    if f.wms_base.as_deref() == Some(old.as_str()) {
                                                        f.wms_base = Some(name.clone());
                                                    }
                                                }
                                            }
                                            self.console.info(format!(
                                                "已更新 WMS 连接「{name}」（图层 {}）",
                                                dlg.layer.trim()
                                            ));
                                        }
                                        _ => {} // 下标漂移（防御）：丢弃
                                    },
                                    // 新建。
                                    None => match dlg.kind {
                                        crate::services::ServiceKind::Wfs => {
                                            let url = crate::services::build_getfeature_url(
                                                &dlg.url, &dlg.layer,
                                            );
                                            self.console.info(format!(
                                                "已新建服务链接「{name}」（WFS 图层 {}）",
                                                dlg.layer.trim()
                                            ));
                                            self.services
                                                .push(crate::services::WfsConnection { name, url });
                                        }
                                        crate::services::ServiceKind::Wms => {
                                            self.console.info(format!(
                                                "已新建 WMS 连接「{name}」（图层 {}）",
                                                dlg.layer.trim()
                                            ));
                                            self.wms_services.push(
                                                crate::services::WmsConnection {
                                                    name,
                                                    url: dlg.url.trim().to_string(),
                                                    layer: dlg.layer.trim().to_string(),
                                                },
                                            );
                                        }
                                    },
                                }
                                self.mark_state_dirty();
                            }
                            Err(e) => {
                                self.toast_err(&e);
                                self.console.push(crate::console::LineKind::Err, e);
                                self.service_dlg = Some(dlg); // 校验失败保留输入
                            }
                        }
                    }
                    crate::ui_kit::DialogAction::Cancel => {} // 取消：丢弃
                    crate::ui_kit::DialogAction::None => self.service_dlg = Some(dlg), // 继续显示
                }
            }
        }
        // 工具后台执行：进度模态 + 完成轮询 + 可终止。
        self.tool_progress_ui(&ctx);
        // WFS 后台拉取：进度模态 + 完成轮询 + 可终止。
        self.service_progress_ui(&ctx);
        self.error_modal(&ctx);
        self.handle_screenshots(&ctx);
        // 主题切换交叉淡化：旧主题底色遮罩 0.2s 渐隐（animate_value_with_time）。
        if let Some((start, old_bg)) = self.theme_fade {
            let t = start.elapsed().as_secs_f32();
            if t >= 0.25 {
                self.theme_fade = None;
            } else {
                let fade_id = egui::Id::new("theme_fade");
                let alpha = if t < 0.03 {
                    // 首帧置 1（旧底色全覆盖），随后渐隐。
                    ctx.animate_value_with_time(fade_id, 1.0_f32, 0.0)
                } else {
                    ctx.animate_value_with_time(fade_id, 0.0_f32, 0.2)
                };
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("theme_fade_layer"),
                ));
                painter.rect_filled(
                    ctx.content_rect(),
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(
                        old_bg.r(),
                        old_bg.g(),
                        old_bg.b(),
                        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
                    ),
                );
                ctx.request_repaint();
            }
        }
        // Toast 轻提示栈（最顶层，自动消退）。
        crate::ui_kit::toast_stack(&ctx, &mut self.toasts, &crate::theme::palette(self.theme));
        // UI 状态防抖写盘（变更后 1s 合并写一次）。
        self.flush_ui_state_if_due();
    }

    /// 退出时立即写盘（eframe 关闭钩子）。
    fn on_exit(&mut self) {
        self.collect_ui_state().save(&self.state_path);
    }
}

impl ConsoleHost for KanyuApp {
    fn host_load(&mut self, path: &str) -> Result<String, String> {
        self.open_file(Path::new(path));
        match self.error_msg.take() {
            Some(e) => {
                self.error_msg = None;
                Err(e)
            }
            None => Ok(self.status.clone()),
        }
    }

    fn host_layers(&self) -> Vec<(String, String, usize)> {
        self.layers
            .iter()
            .map(|e| {
                (
                    e.layer.id().to_string(),
                    e.summary.format.clone(),
                    e.summary.feature_count,
                )
            })
            .collect()
    }

    fn host_info(&self, id: &str) -> Result<String, String> {
        let idx = self.find_layer(id)?;
        let s = &self.layers[idx].summary;
        Ok(format!(
            "图层 {}: {} 要素 | 格式 {} | 几何 {} | 字段 [{}]",
            s.id,
            s.feature_count,
            s.format,
            s.geometry_types.join(", "),
            s.fields.join(", ")
        ))
    }

    fn host_query(&mut self, id: &str, expr: &str) -> Result<String, String> {
        self.op_query(id, expr)
    }

    fn host_buffer(&mut self, id: &str, distance: f64) -> Result<String, String> {
        self.op_buffer(id, distance)
    }

    fn host_measure(&self, id: &str, kind: &str) -> Result<String, String> {
        self.op_measure(id, kind)
    }

    fn host_topology(&self, id: &str) -> Result<String, String> {
        self.op_topology(id)
    }

    fn host_reproject(&mut self, id: &str, from: &str, to: &str) -> Result<String, String> {
        self.op_reproject(id, from, to)
    }

    fn host_export(&self, id: &str, out: &str, fmt: &str) -> Result<String, String> {
        self.op_export(id, out, fmt)
    }

    fn host_fit(&mut self) {
        self.needs_fit = true;
    }

    fn host_toggle_theme(&mut self) {
        self.theme_fade = Some((Instant::now(), crate::theme::palette(self.theme).bg_primary));
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        // 终端路径无 ctx：标记待应用，下一帧 ui() 开头统一 apply_theme。
        self.theme_dirty = true;
        self.canvas.dirty = true;
    }
}

/// base64 编码（布局 SVG 内嵌 PNG 用；RFC 4648 标准表，无依赖）。
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// ColorImage（直通 RGBA）→ PNG 落盘：转预乘后交给 tiny-skia 编码
///（与渲染内核同一 PNG 栈，不引入 image crate）。
fn save_color_image_png(image: &egui::ColorImage, out_path: &str) -> Result<(), String> {
    let [w, h] = image.size;
    let mut rgba = image.as_raw().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    let size = tiny_skia::IntSize::from_wh(w as u32, h as u32)
        .ok_or_else(|| format!("截图尺寸非法（{w}×{h}）"))?;
    let pixmap = tiny_skia::Pixmap::from_vec(rgba, size)
        .ok_or_else(|| "截图像素缓冲构造失败".to_string())?;
    let png = pixmap
        .encode_png()
        .map_err(|e| format!("PNG 编码失败: {e}"))?;
    std::fs::write(out_path, png).map_err(|e| format!("写入 {out_path} 失败: {e}"))
}
