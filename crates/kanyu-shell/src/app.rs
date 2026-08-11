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
use kanyu_core::{analysis, crs, Layer, LayerSummary};
use kanyu_render::{collection_extent, render_png, render_svg, RenderOptions, StyleRule, Theme};
use kanyu_skill::{Skill, SkillHost};

use crate::canvas::{CanvasInput, MapCanvas};
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
struct LayerEntry {
    layer: Layer,
    summary: LayerSummary,
    visible: bool,
    file_name: String,
    /// 骨架目录子节点展开。
    expanded: bool,
    /// 数据源路径（内存图层为 None，不入 .kyu 工程）。
    source_path: Option<String>,
    /// 符号化（默认单色按几何类型；属性页可改，.kyu 持久化）。
    symbology: crate::symbology::LayerSymbology,
}

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
    /// 额外地图视图（主画布 = 默认视图「地图」恒吸附；窗口化/吸附见 mapview.rs）。
    map_views: Vec<crate::mapview::MapView>,
    /// 视图序号计数（标题「地图 N」递增）。
    next_view_id: usize,
    /// 中央当前页签（None = 主视图「地图」）。
    active_view: Option<usize>,
    /// 中央页签条矩形（浮动视图拖入吸附的投放区）。
    view_strip_rect: Option<egui::Rect>,
    /// 正在拖拽的浮动视图（map_views 下标）。
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

/// 渲染缓存 → 画布切片（目录树自下而上序；与 rebuild_merged 同一顺序约定）。
/// 自由函数以便与 &mut canvas 共存（只借用 render_cache 字段）。
fn build_layer_slices(
    cache: &[(String, FeatureCollection, crate::symbology::LayerSymbology)],
) -> Vec<crate::canvas::LayerSlice<'_>> {
    cache
        .iter()
        .map(|rc| crate::canvas::LayerSlice {
            id: &rc.0,
            collection: &rc.1,
            style: Some(crate::symbology::to_style_rule(&rc.2)),
            color: {
                let c = crate::symbology::primary_color(&rc.2);
                egui::Color32::from_rgb(c[0], c[1], c[2])
            },
        })
        .collect()
}

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
            map_views: Vec::new(),
            next_view_id: 2,
            active_view: None,
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
        for path in &args.load {
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
        // view-demo：吸附「地图 2」（二维）+ 浮动「地图 3」（三维，预置方位角）。
        if args.view_demo {
            let v2 = crate::mapview::MapView::new(app.next_view_id);
            app.next_view_id += 1;
            app.map_views.push(v2);
            let mut v3 = crate::mapview::MapView::new(app.next_view_id);
            app.next_view_id += 1;
            v3.dim = crate::mapview::ViewDim::ThreeD;
            v3.docked = false;
            app.map_views.push(v3);
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
        app
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
        // 全部额外视图同脏（可见性/增删变化 → 各视图重渲）。
        for v in &mut self.map_views {
            v.canvas.dirty = true;
        }
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

    /// 保存堪舆工程（.kyu）：图层引用 + 可见性 + 视口 + 地图色彩。
    /// 无来源的内存图层（分析产出）不入工程并在终端明示。
    fn save_project(&mut self, path: &Path) {
        let mut project = kanyu_core::project::KanyuProject::new(
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "untitled".to_string()),
            &self.project_crs,
        );
        project.viewport = self.view_bbox;
        project.map_theme = self.map_theme_mode.as_str().to_string();
        let mut skipped = 0;
        for entry in &self.layers {
            match &entry.source_path {
                // 分组路径：根级图层写 None（不输出 group 键，保持文件干净）；
                // 符号化 JSON 写入 style（shell 侧模型，core 原样透传）。
                Some(src) => project.layers.push(kanyu_core::project::ProjectLayer {
                    id: entry.layer.id().to_string(),
                    source: src.clone(),
                    visible: entry.visible,
                    style: serde_json::to_value(&entry.symbology).ok(),
                    group: toc::group_path_of(&self.toc, entry.layer.id())
                        .filter(|p| !p.is_empty()),
                }),
                None => skipped += 1,
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

    /// 打开堪舆工程（.kyu）：恢复图层（按引用加载）、可见性、视口、地图色彩。
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
        // 清空当前现场再恢复（与"打开工程"语义一致）。
        self.layers.clear();
        self.toc.clear();
        self.selected = None;
        self.selected_group = None;
        self.console.info(format!(
            "打开工程 {}（{} 个图层引用）",
            project.name,
            project.layers.len()
        ));
        let mut failed = 0;
        for pl in &project.layers {
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
        self.map_theme_mode = MapThemeMode::parse(&project.map_theme);
        self.project_crs = project.crs.clone();
        self.view_bbox = project.viewport;
        if project.viewport.is_some() {
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
            RibbonAction::NewMapView => {
                let id = self.next_view_id;
                self.next_view_id += 1;
                // 新建视图默认吸附中央并置为当前页签。
                self.map_views.push(crate::mapview::MapView::new(id));
                self.active_view = Some(self.map_views.len() - 1);
                self.console.info(format!("已新建地图视图「地图 {id}」"));
            }
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
        let crate::attrtable::AttrAction::Apply { layer, op } = action;
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
        }
        if let Some(id) = strip.closed {
            self.dock.close_panel(id);
            self.console.info(format!("面板「{}」已关闭", id.title()));
        }
        if let Some(id) = strip.drag_started {
            self.dock.dragging = Some(id);
        }
        if let Some(open) = strip.set_all_open {
            self.dock.set_all_open(open);
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
                // 地图框分类数据：主视图 + 全部视图（含浮动）。
                let mut view_rows = vec![crate::catalog::ViewRow {
                    index: None,
                    title: "地图".to_string(),
                    dim_label: crate::mapview::ViewDim::TwoD.label(),
                }];
                for (i, v) in self.map_views.iter().enumerate() {
                    view_rows.push(crate::catalog::ViewRow {
                        index: Some(i),
                        title: v.title.clone(),
                        dim_label: v.dim.label(),
                    });
                }
                let mut catalog = std::mem::take(&mut self.catalog);
                out.catalog
                    .extend(catalog.ui(ui, &mut self.icon_cache, &view_rows));
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

        // Ribbon 功能区（顶部，86px）。命令可用条件按快照求值（无图层/无选中置灰）。
        let snap = crate::commands::AppSnapshot {
            layer_count: self.layers.len(),
            has_selection: self.selected.is_some(),
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
        let status = self.status.clone();
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
                        }
                        Some(zone) => {
                            self.dock.dock_to(drag_id, zone);
                            self.console
                                .info(format!("面板「{}」已停靠", drag_id.title()));
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
                crate::catalog::CatalogAction::ActivateView(idx) => match idx {
                    None => self.active_view = None,
                    Some(i) if i < self.map_views.len() => {
                        if self.map_views[i].docked {
                            self.active_view = Some(i);
                        } else {
                            // 浮动窗：提到最前聚焦。
                            ctx.move_to_top(egui::LayerId::new(
                                egui::Order::Middle,
                                egui::Id::new(("map_view", self.map_views[i].id)),
                            ));
                        }
                    }
                    _ => {}
                },
                crate::catalog::CatalogAction::NewMapView => {
                    self.dispatch(RibbonAction::NewMapView, &ctx)
                }
            }
        }
        for action in dock_out.panel {
            self.dispatch_panel_action(action);
        }

        // 中央：视图页签条（文档页签范式）+ 当前视图内容。
        // 主视图「地图」恒在首位（不可关闭/弹出，启动即有地图框）。
        let docked_tabs: Vec<(usize, String)> = self
            .map_views
            .iter()
            .enumerate()
            .filter(|(_, v)| v.docked)
            .map(|(i, v)| (i, v.title.clone()))
            .collect();
        let strip_out =
            ui.scope(|ui| crate::mapview::view_tab_strip(ui, &docked_tabs, self.active_view));
        self.view_strip_rect = Some(strip_out.response.rect);
        let act = strip_out.inner;
        if let Some(sel) = act.activated {
            self.active_view = sel;
        }
        if let Some(i) = act.floated {
            if let Some(v) = self.map_views.get_mut(i) {
                v.docked = false;
                if self.active_view == Some(i) {
                    self.active_view = None;
                }
            }
        }
        if let Some(i) = act.closed {
            if i < self.map_views.len() {
                let title = self.map_views[i].title.clone();
                self.map_views.remove(i);
                match self.active_view {
                    Some(a) if a == i => self.active_view = None,
                    Some(a) if a > i => self.active_view = Some(a - 1),
                    _ => {}
                }
                self.console.info(format!("视图「{title}」已关闭"));
            }
        }
        if act.new_view {
            self.dispatch(RibbonAction::NewMapView, &ctx);
        }

        // 当前视图内容：None = 主视图；Some(i) = 吸附的额外视图。
        match self.active_view {
            Some(i) if i < self.map_views.len() && self.map_views[i].docked => {
                let mut v = self.map_views.remove(i);
                let vinput = crate::mapview::ViewInput {
                    layers: &build_layer_slices(&self.render_cache),
                    theme: self.effective_map_theme(),
                    data_extent: self.data_extent,
                    crs: &self.project_crs,
                };
                crate::mapview::content_ui(ui, &mut v, &vinput, false);
                self.map_views.insert(i, v);
            }
            Some(_) => self.active_view = None, // 目标已浮动/关闭 → 回落主视图
            None => {
                // 主视图「地图」（地图色彩由 map_theme_mode 决定，与界面主题解耦）。
                let out = self.canvas.ui(
                    ui,
                    CanvasInput {
                        layers: &build_layer_slices(&self.render_cache),
                        theme: self.effective_map_theme(),
                        view_bbox: self.view_bbox,
                        needs_fit: self.needs_fit,
                        data_extent: self.fit_extent.or(self.data_extent),
                        empty_hint: "◇ 堪舆\n\n拖入数据文件，或经「主页 → 打开数据…」\n支持 shp / geojson / fgb / parquet / dxf / dwg / kml / kmz / csv / tsv / xlsx",
                    },
                );
                self.view_bbox = out.view_bbox;
                self.mouse_data = out.mouse_data;
                if out.fit_consumed {
                    self.needs_fit = false;
                    self.fit_extent = None;
                }
                if let Some(e) = out.render_error {
                    self.status = e;
                }
            }
        }

        // 浮动视图窗口（docked=false 的视图；关闭即从清单移除）。
        {
            let mut views = std::mem::take(&mut self.map_views);
            let vinput = crate::mapview::ViewInput {
                layers: &build_layer_slices(&self.render_cache),
                theme: self.effective_map_theme(),
                data_extent: self.data_extent,
                crs: &self.project_crs,
            };
            for v in &mut views {
                if !v.docked {
                    crate::mapview::window_ui(&ctx, v, &vinput);
                }
            }
            views.retain(|v| v.docked || v.open);
            self.map_views = views;
        }

        // 浮动视图拖到中央页签条 → 吸附（标题条拖拽判定同 dock 浮动窗）。
        if self.dragging_view.is_none() && ctx.input(|i| i.pointer.is_decidedly_dragging()) {
            if let Some(origin) = ctx.input(|i| i.pointer.press_origin()) {
                if let Some(idx) = self.map_views.iter().position(|v| {
                    !v.docked
                        && v.win_rect.is_some_and(|r| {
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
                    if strip.contains(pos) && idx < self.map_views.len() {
                        self.map_views[idx].docked = true;
                        self.active_view = Some(idx);
                        self.console
                            .info(format!("视图「{}」已吸附到中央", self.map_views[idx].title));
                    }
                }
            }
        }

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
        // 工具后台执行：进度模态 + 完成轮询 + 可终止。
        self.tool_progress_ui(&ctx);
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
