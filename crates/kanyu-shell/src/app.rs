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

/// 已加载图层（含 UI 态：可见性、目录展开、来源路径）。
struct LayerEntry {
    layer: Layer,
    summary: LayerSummary,
    visible: bool,
    file_name: String,
    /// 骨架目录子节点展开。
    expanded: bool,
    /// 数据源路径（内存图层为 None，不入 .kyu 工程）。
    source_path: Option<String>,
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
    data_extent: Option<BBox>,
    /// 地图导出设置（渲染设置对话框采集 → 现由「设置 → 渲染」编辑）。
    map_export_size: (u32, u32),
    map_export_style: Option<StyleRule>,
    /// 工程坐标系（保存进 .kyu；投影变换默认目标；状态栏显示）。
    project_crs: String,
    /// 工具箱面板状态。
    toolbox: crate::toolbox::ToolboxPanel,
    /// 工具箱参数对话框状态。
    tool_run: Option<crate::toolbox::ToolRunState>,
    /// 设置对话框状态（坐标系/渲染）。
    settings: Option<crate::settings::SettingsDialog>,
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
    /// 「缩放到指定图层」的一次性适配范围（覆盖 data_extent，消费后清除）。
    fit_extent: Option<BBox>,
    /// 窗口截图（非退出）待保存路径。
    pending_window_shot: Option<String>,
    screenshot: Option<ScreenshotState>,
    /// 位图图标缓存（ArcGIS Pro 本机资源；缺图自动回退手绘线性图标）。
    icon_cache: crate::ui_kit::icons::IconCache,
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
            data_extent: None,
            map_export_size: (1200, 800),
            map_export_style: None,
            project_crs: "EPSG:4326".to_string(),
            toolbox: crate::toolbox::ToolboxPanel::default(),
            tool_run: None,
            settings: None,
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
        }
        // --open-settings / --tool-demo（隐藏验证参数）：预设对话框打开态。
        if args.open_settings {
            app.open_settings();
        }
        if args.tool_demo {
            if let Some(def) = crate::toolbox::find("buffer") {
                app.tool_run = Some(crate::toolbox::ToolRunState::new(def));
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
                self.console.info(msg);
                let id = layer.id().to_string();
                self.layers.push(LayerEntry {
                    layer,
                    summary,
                    visible: true,
                    file_name,
                    expanded: false,
                    source_path: Some(path_str.to_string()),
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
        self.layers.push(LayerEntry {
            layer,
            summary,
            visible: true,
            file_name: id.clone(),
            expanded: false,
            source_path: None,
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
        for id in &order {
            if let Some(entry) = self.layers.iter().find(|e| e.layer.id() == id) {
                let collection = entry.layer.collection();
                if let Ok(Some(ext)) = collection_extent(&collection) {
                    extents.push(ext);
                }
                features.extend(collection.features);
            }
        }
        self.merged = FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        };
        self.data_extent = view::union(extents);
        self.canvas.dirty = true;
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
            .map(|e| LayerView {
                id: e.layer.id().to_string(),
                file_name: e.file_name.clone(),
                format: e.summary.format.clone(),
                feature_count: e.summary.feature_count,
                geometry_types: e.summary.geometry_types.clone(),
                fields: e.summary.fields.clone(),
                visible: e.visible,
                expanded: e.expanded,
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
                // 分组路径：根级图层写 None（不输出 group 键，保持文件干净）。
                Some(src) => project.layers.push(kanyu_core::project::ProjectLayer {
                    id: entry.layer.id().to_string(),
                    source: src.clone(),
                    visible: entry.visible,
                    style: None,
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
                self.console.info(msg);
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

    /// 打开设置对话框（以工程当前值快照）。
    fn open_settings(&mut self) {
        self.settings = Some(crate::settings::SettingsDialog::open_with(
            &self.project_crs,
            self.map_export_size,
            self.map_theme_mode,
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
                self.console.info(msg);
            }
            Err(e) => {
                self.status = format!("失败: {e}");
                self.console.push(crate::console::LineKind::Err, e);
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
                let mut catalog = std::mem::take(&mut self.catalog);
                out.catalog.extend(catalog.ui(ui, &mut self.icon_cache));
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
                        self.tool_run = Some(st);
                    }
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

        // Ribbon 功能区（顶部，86px）。
        egui::Panel::top("ribbon")
            .exact_size(sizes::RIBBON)
            .show(ui, |ui| {
                if let Some(action) = self.ribbon.ui(ui, &mut self.icon_cache) {
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
                crate::catalog::CatalogAction::LoadFile(path) => self.open_file(&path),
            }
        }
        for action in dock_out.panel {
            self.dispatch_panel_action(action);
        }

        // 中央地图画布（地图色彩由 map_theme_mode 决定，与界面主题解耦）。
        let out = self.canvas.ui(
            ui,
            CanvasInput {
                merged: &self.merged,
                theme: self.effective_map_theme(),
                view_bbox: self.view_bbox,
                needs_fit: self.needs_fit,
                data_extent: self.fit_extent.or(self.data_extent),
                style: None,
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
        // 工具箱参数对话框（通用表单由注册表驱动）。
        if let Some(mut st) = self.tool_run.take() {
            let layer_ids = self.layer_ids();
            let title = format!("{}（工具箱）", st.tool.name);
            match crate::ui_kit::dialog_shell(&ctx, &title, |ui| {
                crate::toolbox::run_form(ui, &mut st, &layer_ids, &|id| {
                    self.layers
                        .iter()
                        .find(|e| e.layer.id() == id)
                        .map(|e| e.summary.fields.clone())
                        .unwrap_or_default()
                });
            }) {
                crate::ui_kit::DialogAction::None => self.tool_run = Some(st),
                crate::ui_kit::DialogAction::Cancel => {}
                crate::ui_kit::DialogAction::Ok => {
                    let outcome = crate::toolbox::run_tool(st.tool.id, &st.values, |id| {
                        self.layers
                            .iter()
                            .find(|e| e.layer.id() == id)
                            .map(|e| e.layer.collection())
                    });
                    match outcome {
                        Ok(crate::toolbox::ToolOutcome::NewLayer {
                            collection,
                            base,
                            verb,
                        }) => {
                            let msg = self.add_result_layer(&base, collection, &verb);
                            self.status = msg.clone();
                            self.console.info(msg);
                        }
                        Ok(crate::toolbox::ToolOutcome::Report(text)) => {
                            self.console.info(text);
                        }
                        Ok(crate::toolbox::ToolOutcome::Export { layer, out }) => {
                            let fmt = out.rsplit('.').next().unwrap_or("").to_string();
                            match self.op_export(&layer, &out, &fmt) {
                                Ok(m) => {
                                    self.status = m.clone();
                                    self.console.info(m);
                                }
                                Err(e) => {
                                    self.console.push(crate::console::LineKind::Err, &e);
                                    self.error_msg = Some(e);
                                }
                            }
                        }
                        // 校验/执行失败：留在对话框内红字（不吞输入）。
                        Err(e) => {
                            st.err = Some(e);
                            self.tool_run = Some(st);
                        }
                    }
                }
            }
        }
        self.error_modal(&ctx);
        self.handle_screenshots(&ctx);
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
