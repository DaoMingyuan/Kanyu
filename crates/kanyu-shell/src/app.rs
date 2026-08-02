//! 应用主体：Ribbon 功能区 + Contents/属性面板 + 独立终端 + MapCanvas +
//! 双主题 + 截图验证。全部界面由 [`crate::ui_kit`] 组件与各面板模块组合。
//!
//! 布局（ArcGIS Pro 式）：
//! ┌──────────────────────────────┐
//! │ Ribbon（页签 + 命令组，86px） │
//! ├─────────┬────────────┬───────┤
//! │ Contents│  MapCanvas │ 属性  │
//! │ 图层树  │            │ /基因 │
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
use kanyu_gene::{Gene, GeneHost};
use kanyu_render::{collection_extent, render_png, render_svg, RenderOptions, StyleRule, Theme};

use crate::canvas::{CanvasInput, MapCanvas};
use crate::console::{ConsoleHost, ConsolePanel, HELP_TEXT};
use crate::dialogs::{DialogResult, Dialogs};
use crate::panels::{self, GeneView, LayerView, PanelAction};
use crate::ribbon::{Ribbon, RibbonAction};
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

/// 堪舆桌面壳层应用。
pub struct KanyuApp {
    layers: Vec<LayerEntry>,
    /// 属性面板选中的图层索引。
    selected: Option<usize>,
    theme: Theme,
    ribbon: Ribbon,
    console: ConsolePanel,
    dialogs: Dialogs,
    canvas: MapCanvas,
    /// 基因宿主与注册表。
    gene_host: GeneHost,
    genes: HashMap<String, Gene>,
    gene_metas: Vec<GeneView>,
    /// 当前视口（数据坐标 bbox；与画布同比例，view.rs 不变式）。
    view_bbox: Option<BBox>,
    needs_fit: bool,
    /// 可见图层合并缓存（仅在加载/可见性/增删时重建，平移缩放不重建）。
    merged: FeatureCollection,
    data_extent: Option<BBox>,
    /// 地图导出设置（渲染设置对话框采集）。
    map_export_size: (u32, u32),
    map_export_style: Option<StyleRule>,
    error_msg: Option<String>,
    status: String,
    mouse_data: Option<(f64, f64)>,
    show_layers_panel: bool,
    show_console: bool,
    /// 底部停靠区当前页签（终端 | AI 对话）。
    dock_tab: panels::DockTab,
    /// AI 对话面板。
    ai_chat: crate::ai::AiChatPanel,
    /// 地图色彩模式（默认固定晨山）。
    map_theme_mode: MapThemeMode,
    show_props_panel: bool,
    /// 终端切主题后置位，下一帧开头统一 apply_theme。
    theme_dirty: bool,
    /// 「缩放到指定图层」的一次性适配范围（覆盖 data_extent，消费后清除）。
    fit_extent: Option<BBox>,
    /// 窗口截图（非退出）待保存路径。
    pending_window_shot: Option<String>,
    screenshot: Option<ScreenshotState>,
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
            selected: None,
            theme: args.theme,
            ribbon: Ribbon::default(),
            console: ConsolePanel::default(),
            dialogs: Dialogs::default(),
            canvas: MapCanvas::default(),
            gene_host: GeneHost::new().expect("wasmtime 引擎初始化失败（极少见）"),
            genes: HashMap::new(),
            gene_metas: Vec::new(),
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
            error_msg: None,
            status: "就绪".to_string(),
            mouse_data: None,
            show_layers_panel: true,
            show_console: true,
            dock_tab: panels::DockTab::Console,
            ai_chat: crate::ai::AiChatPanel::default(),
            map_theme_mode: MapThemeMode::FixedLight,
            show_props_panel: true,
            pending_window_shot: None,
            screenshot,
            theme_dirty: false,
            fit_extent: None,
        };
        if let Some(path) = &args.load {
            app.open_file(Path::new(path));
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
                self.layers.push(LayerEntry {
                    layer,
                    summary,
                    visible: true,
                    file_name,
                    expanded: false,
                    source_path: Some(path_str.to_string()),
                });
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

    /// 从内核结果集合登记新图层（分析/查询/基因产出）。
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
        self.selected = Some(self.layers.len() - 1);
        self.rebuild_merged();
        let msg = format!("{verb} → 新图层 {id}（{n} 要素）");
        self.status = msg.clone();
        msg
    }

    /// 重建可见图层合并缓存与数据范围。
    fn rebuild_merged(&mut self) {
        let mut features = Vec::new();
        let mut extents = Vec::new();
        for entry in &self.layers {
            if entry.visible {
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

    fn visible_feature_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|e| e.visible)
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
            .enumerate()
            .map(|(i, e)| LayerView {
                file_name: e.file_name.clone(),
                format: e.summary.format.clone(),
                feature_count: e.summary.feature_count,
                geometry_types: e.summary.geometry_types.clone(),
                fields: e.summary.fields.clone(),
                visible: e.visible,
                expanded: e.expanded,
                selected: self.selected == Some(i),
            })
            .collect()
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
            "EPSG:4326",
        );
        project.viewport = self.view_bbox;
        project.map_theme = self.map_theme_mode.as_str().to_string();
        let mut skipped = 0;
        for entry in &self.layers {
            match &entry.source_path {
                Some(src) => project.layers.push(kanyu_core::project::ProjectLayer {
                    id: entry.layer.id().to_string(),
                    source: src.clone(),
                    visible: entry.visible,
                    style: None,
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
        self.selected = None;
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
            } else {
                failed += 1;
                self.error_msg = None; // 单源失败不阻塞整工程
            }
        }
        self.map_theme_mode = MapThemeMode::parse(&project.map_theme);
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

    fn op_gene_run(&mut self, gene_id: &str, layer_id: &str) -> Result<String, String> {
        let idx = self.find_layer(layer_id)?;
        let gene = self
            .genes
            .get(gene_id)
            .ok_or_else(|| format!("基因未注册: {gene_id}（先「基因 → 热加载…」）"))?;
        let result = self
            .gene_host
            .run(gene, &self.layers[idx].layer.collection())
            .map_err(|e| e.to_string())?;
        Ok(self.add_result_layer(&format!("g_{layer_id}"), result, &format!("gene {gene_id}")))
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
                self.dialogs.reproject = Some(crate::dialogs::ReprojectState::default())
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
            RibbonAction::RenderSettingsDialog => {
                self.dialogs.render_settings = Some(crate::dialogs::RenderSettingsState::default())
            }
            RibbonAction::ExportMapDialog => {
                self.dialogs.export_map = Some(crate::dialogs::ExportMapState::default())
            }
            RibbonAction::ZoomToFit => self.needs_fit = true,
            RibbonAction::ResetView => {
                self.view_bbox = None;
                self.needs_fit = true;
                self.canvas.dirty = true;
            }
            RibbonAction::ToggleLayersPanel => self.show_layers_panel = !self.show_layers_panel,
            RibbonAction::ToggleConsole => self.show_console = !self.show_console,
            RibbonAction::TogglePropsPanel => self.show_props_panel = !self.show_props_panel,
            RibbonAction::CycleMapTheme => {
                self.map_theme_mode = self.map_theme_mode.next();
                self.console
                    .info(format!("地图色彩模式 → {}", self.map_theme_mode.label()));
                self.canvas.dirty = true;
            }
            RibbonAction::GeneHotload => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("WASM 基因", &["wasm"])
                    .pick_file()
                {
                    match self.gene_host.load(&path.to_string_lossy()) {
                        Ok(gene) => {
                            let meta = gene.meta().clone();
                            let replaced = self.genes.contains_key(&meta.name);
                            self.console.info(format!(
                                "基因已注册: {} v{}（能力: {}{}）",
                                meta.name,
                                meta.version,
                                meta.capabilities.join(", "),
                                if replaced { "，覆盖同名" } else { "" }
                            ));
                            self.gene_metas.push(GeneView {
                                id: meta.name.clone(),
                                version: meta.version.clone(),
                                capabilities: meta.capabilities.clone(),
                            });
                            self.genes.insert(meta.name.clone(), gene);
                        }
                        Err(e) => {
                            self.console
                                .push(crate::console::LineKind::Err, format!("基因校验失败: {e}"));
                        }
                    }
                }
            }
            RibbonAction::GeneList => {
                if self.gene_metas.is_empty() {
                    self.console.info("（无已注册基因）");
                } else {
                    for g in &self.gene_metas {
                        self.console.info(format!(
                            "{:<24} v{:<8} [{}]",
                            g.id,
                            g.version,
                            g.capabilities.join(", ")
                        ));
                    }
                }
            }
            RibbonAction::GeneRunDialog => {
                self.dialogs.gene_run = Some(crate::dialogs::GeneRunState::default())
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
            DialogResult::RenderSettings {
                width,
                height,
                style,
            } => {
                self.map_export_size = (width, height);
                self.map_export_style = if style.trim().is_empty() {
                    None
                } else {
                    match serde_json::from_str::<StyleRule>(&style) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            self.console.push(
                                crate::console::LineKind::Err,
                                format!("样式 JSON 解析失败: {e}"),
                            );
                            None
                        }
                    }
                };
                Ok(format!(
                    "渲染设置已更新（{width}×{height}{}）",
                    if self.map_export_style.is_some() {
                        "，含符号化"
                    } else {
                        ""
                    }
                ))
            }
            DialogResult::ExportMap { out } => self.op_export_map(&out),
            DialogResult::GeneRun { gene_id, layer } => self.op_gene_run(&gene_id, &layer),
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

    fn dispatch_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::ZoomToLayer(i) => {
                if let Some(entry) = self.layers.get(i) {
                    if let Ok(Some(ext)) = collection_extent(&entry.layer.collection()) {
                        self.selected = Some(i);
                        // 下一帧以该图层范围（而非可见并集）做一次性适配。
                        self.fit_extent = Some(ext);
                        self.needs_fit = true;
                    }
                }
            }
            PanelAction::RemoveLayer(i) => {
                if i < self.layers.len() {
                    let name = self.layers[i].file_name.clone();
                    self.layers.remove(i);
                    if self.selected == Some(i) {
                        self.selected = None;
                    } else if let Some(s) = self.selected {
                        if s > i {
                            self.selected = Some(s - 1);
                        }
                    }
                    self.console.info(format!("已移除图层 {name}"));
                    self.rebuild_merged();
                }
            }
            PanelAction::VisibilityChanged(i, _vis) => {
                if i < self.layers.len() {
                    self.rebuild_merged();
                }
            }
            PanelAction::SelectLayer(i) => {
                if i < self.layers.len() {
                    self.selected = Some(i);
                }
            }
            PanelAction::ToggleExpand(i) => {
                if let Some(entry) = self.layers.get_mut(i) {
                    entry.expanded = !entry.expanded;
                }
            }
            PanelAction::OpenGeneRun => {
                self.dialogs.gene_run = Some(crate::dialogs::GeneRunState::default())
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
                if let Some(action) = self.ribbon.ui(ui) {
                    self.dispatch(action, &ctx);
                }
            });

        // StatusBar（最底；先注册的 bottom 面板居最下）。
        let span = self.view_bbox.map(|b| b[2] - b[0]);
        let status = self.status.clone();
        let count = self.visible_feature_count();
        let mouse = self.mouse_data;
        panels::status_bar(ui, &status, mouse, count, span, self.map_theme_mode.label());

        // 底部双页签停靠区（终端 | AI 对话）。
        if self.show_console {
            let mut dock_tab = self.dock_tab;
            let mut console = std::mem::take(&mut self.console);
            let mut ai_chat = std::mem::take(&mut self.ai_chat);
            panels::bottom_dock(ui, &mut dock_tab, &mut console, &mut ai_chat, self);
            self.dock_tab = dock_tab;
            self.console = console;
            self.ai_chat = ai_chat;
        }

        // Contents 图层面板（左）。
        if self.show_layers_panel {
            let views = self.layer_views();
            for action in panels::contents_panel(ui, &views) {
                self.dispatch_panel_action(action);
            }
        }

        // 属性/基因面板（右）。
        if self.show_props_panel {
            let views = self.layer_views();
            let selected = self.selected.and_then(|i| views.get(i));
            let genes = self.gene_metas.clone();
            let actions = panels::props_panel(ui, selected, &genes);
            for action in actions {
                self.dispatch_panel_action(action);
            }
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

        // 对话框与模态。
        let layer_ids = self.layer_ids();
        let gene_ids: Vec<String> = self.gene_metas.iter().map(|g| g.id.clone()).collect();
        if let Some(result) = self.dialogs.ui(&ctx, &layer_ids, &gene_ids) {
            self.dispatch_dialog_result(result);
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
