//! 应用主体：TitleBar / 图层面板 / MapCanvas / StatusBar / 双主题 / 截图验证。
//!
//! 渲染链路：可见图层合并（缓存）→ `kanyu_render::render_png`（显式视口）→
//! tiny-skia 解码 → `egui::ColorImage` → TextureHandle。视图数学（缩放锚点 /
//! 平移 / 坐标逆变换）全部走 [`crate::view`] 纯函数，保证画布像素与数据坐标
//! 的线性映射不变式（见 view.rs 模块注释）。
//!
//! 注：eframe/egui 0.35 将 SidePanel/TopBottomPanel 统一为 [`egui::Panel`]，
//! 且 `App::update(ctx)` 改为 `App::ui(ui)`——面板在根 ui 内依次占位。

use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};
use geojson::FeatureCollection;
use kanyu_core::{Layer, LayerSummary};
use kanyu_render::{collection_extent, render_png, RenderOptions, Theme};

use crate::view::{self, BBox};
use crate::ShellArgs;

/// 打开数据对话框的格式过滤器（与内核原生读能力对齐）。
const OPEN_EXTENSIONS: &[&str] = &[
    "shp", "geojson", "fgb", "parquet", "dxf", "dwg", "kml", "kmz", "csv", "tsv", "xlsx",
];

/// 已加载图层（含 UI 态：可见性）。
struct LayerEntry {
    layer: Layer,
    summary: LayerSummary,
    visible: bool,
    file_name: String,
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
    theme: Theme,
    /// 当前视口（数据坐标 bbox；与画布同比例，view.rs 不变式）。
    view_bbox: Option<BBox>,
    /// 加载后待首帧等比嵌入画布。
    needs_fit: bool,
    /// 可见图层合并缓存（仅在加载/可见性变化时重建，平移缩放不重建）。
    merged: FeatureCollection,
    /// merged 对应的可见图层数据范围。
    data_extent: Option<BBox>,
    texture: Option<egui::TextureHandle>,
    /// 纹理对应的物理像素尺寸（用于高分屏重渲判定）。
    tex_px: [u32; 2],
    render_dirty: bool,
    error_msg: Option<String>,
    status: String,
    mouse_data: Option<(f64, f64)>,
    show_layers_panel: bool,
    screenshot: Option<ScreenshotState>,
}

impl KanyuApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: ShellArgs) -> Self {
        load_cjk_font(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, args.theme);
        let screenshot = args.screenshot.map(|out_path| ScreenshotState {
            out_path,
            start: Instant::now(),
            delay: Duration::from_secs_f64(args.delay_secs.max(0.0)),
            requested: false,
        });
        let mut app = Self {
            layers: Vec::new(),
            theme: args.theme,
            view_bbox: None,
            needs_fit: false,
            merged: FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
            data_extent: None,
            texture: None,
            tex_px: [0, 0],
            render_dirty: true,
            error_msg: None,
            status: "就绪".to_string(),
            mouse_data: None,
            show_layers_panel: true,
            screenshot,
        };
        if let Some(path) = &args.load {
            app.open_file(Path::new(path));
        }
        app
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
        match Layer::load(id, &path_str) {
            Ok(layer) => {
                let summary = layer.summary();
                self.status = format!(
                    "已加载 {file_name}（{} 要素，{}）",
                    summary.feature_count, summary.format
                );
                self.layers.push(LayerEntry {
                    layer,
                    summary,
                    visible: true,
                    file_name,
                });
                self.rebuild_merged();
                self.needs_fit = true;
            }
            Err(e) => {
                self.error_msg = Some(format!("无法打开 {file_name}\n\n{e}"));
            }
        }
    }

    /// 重建可见图层合并缓存与数据范围（加载/可见性切换时调用）。
    /// 数据范围为各可见图层 bbox 的并集（view::union）。
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
        self.render_dirty = true;
    }

    /// 可见要素总数（状态栏）。
    fn visible_feature_count(&self) -> usize {
        self.layers
            .iter()
            .filter(|e| e.visible)
            .map(|e| e.summary.feature_count)
            .sum()
    }

    /// 当前文件名（TitleBar 中部）：最近加载的文件。
    fn current_file_name(&self) -> Option<&str> {
        self.layers.last().map(|e| e.file_name.as_str())
    }

    /// 切换主题：egui Visuals 与渲染主题联动。
    fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.theme = match self.theme {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        apply_theme(ctx, self.theme);
        self.render_dirty = true;
    }

    // ===== 各区域绘制 =====

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("title_bar")
            .exact_size(40.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new("◇ 堪舆").strong().size(17.0));
                    ui.add_space(8.0);
                    let file = self.current_file_name().unwrap_or("未打开数据");
                    ui.label(
                        egui::RichText::new(file)
                            .size(12.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    let ctx = ui.ctx().clone();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(12.0);
                        let theme_label = match self.theme {
                            Theme::Light => "切换到夜观星",
                            Theme::Dark => "切换到晨山",
                        };
                        if ui.button(theme_label).clicked() {
                            self.toggle_theme(&ctx);
                        }
                        if ui.button("打开数据…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("地理数据", OPEN_EXTENSIONS)
                                .pick_file()
                            {
                                self.open_file(&path);
                            }
                        }
                        let panel_label = if self.show_layers_panel {
                            "收起图层面板"
                        } else {
                            "展开图层面板"
                        };
                        if ui.button(panel_label).clicked() {
                            self.show_layers_panel = !self.show_layers_panel;
                        }
                    });
                });
            });
    }

    fn layers_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("layers_panel")
            .default_size(260.0)
            .size_range(180.0..=480.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("图层");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut toggled = false;
                    for entry in &mut self.layers {
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut entry.visible, "").changed() {
                                toggled = true;
                            }
                            ui.label(egui::RichText::new(&entry.file_name).strong());
                        });
                        ui.indent(entry.file_name.clone(), |ui| {
                            ui.label(format!(
                                "{} · {} 要素",
                                entry.summary.format, entry.summary.feature_count
                            ));
                            if !entry.summary.geometry_types.is_empty() {
                                ui.label(format!(
                                    "几何: {}",
                                    entry.summary.geometry_types.join(", ")
                                ));
                            }
                            if !entry.summary.fields.is_empty() {
                                ui.label(format!("字段: {}", entry.summary.fields.join(", ")));
                            }
                        });
                        ui.add_space(6.0);
                    }
                    if self.layers.is_empty() {
                        ui.label("尚未加载图层");
                    }
                    if toggled {
                        self.rebuild_merged();
                    }
                });
            });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status_bar")
            .exact_size(28.0)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(&self.status).size(11.5));
                    ui.separator();
                    let coord = match self.mouse_data {
                        Some((x, y)) => format!("{x:.5}°E, {y:.5}°N"),
                        None => "—".to_string(),
                    };
                    ui.label(
                        egui::RichText::new(format!("坐标: {coord}"))
                            .size(11.5)
                            .family(egui::FontFamily::Monospace),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .size(11.5),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("要素: {}", self.visible_feature_count()))
                                .size(11.5),
                        );
                    });
                });
            });
    }

    /// 中央画布：交互（滚轮缩放 / 左键平移）→ 视图数学 → 视口重渲。
    /// 根 ui 在面板占位后剩余的区域即画布（不套 CentralPanel，画布自绘背景）。
    fn map_canvas(&mut self, ui: &mut egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let (w, h) = (f64::from(rect.width()), f64::from(rect.height()));
        if w < 1.0 || h < 1.0 {
            return;
        }
        let ppp = f64::from(ui.ctx().pixels_per_point());
        let px_w = ((rect.width() as f64 * ppp).round() as u32).clamp(1, 8192);
        let px_h = ((rect.height() as f64 * ppp).round() as u32).clamp(1, 8192);

        // 首帧 / 新加载：把数据范围等比嵌入画布。
        if self.needs_fit {
            self.view_bbox = self.data_extent.map(|ext| view::fit_view(ext, w, h));
            self.needs_fit = false;
            self.render_dirty = true;
        }

        if let Some(bbox) = self.view_bbox {
            // 画布尺寸变化：重扩边维持"视口与画布同比例"不变式。
            if self.tex_px != [0, 0] {
                let logical_w = f64::from(self.tex_px[0]) / ppp;
                let logical_h = f64::from(self.tex_px[1]) / ppp;
                if (logical_w - w).abs() > 1.0 || (logical_h - h).abs() > 1.0 {
                    self.view_bbox = Some(view::fit_view(bbox, w, h));
                    self.render_dirty = true;
                }
            }

            // 滚轮缩放（光标锚点为不动点）。
            let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
            if response.hovered() && scroll != 0.0 {
                if let Some(pos) = response.hover_pos() {
                    let anchor = view::screen_to_data(
                        f64::from(pos.x - rect.min.x),
                        f64::from(pos.y - rect.min.y),
                        bbox,
                        w,
                        h,
                    );
                    let factor = (f64::from(scroll) * 0.002).exp();
                    self.view_bbox = Some(view::zoom_at(bbox, anchor, factor));
                    self.render_dirty = true;
                }
            }

            // 左键拖拽平移（内容跟随鼠标）。
            if response.dragged_by(egui::PointerButton::Primary) {
                let d = response.drag_delta();
                if d != Vec2::ZERO {
                    self.view_bbox = Some(view::pan(bbox, f64::from(d.x), f64::from(d.y), w, h));
                    self.render_dirty = true;
                }
            }
        }

        // 鼠标数据坐标（状态栏）。
        self.mouse_data = match (response.hover_pos(), self.view_bbox) {
            (Some(pos), Some(bbox)) => Some(view::screen_to_data(
                f64::from(pos.x - rect.min.x),
                f64::from(pos.y - rect.min.y),
                bbox,
                w,
                h,
            )),
            _ => None,
        };

        // 状态变化重渲：render_png → tiny-skia 解码 → Texture。
        // 物理像素渲染（乘 pixels_per_point），高分屏不糊。
        let size_changed = self.tex_px != [px_w, px_h];
        if self.render_dirty || (size_changed && self.view_bbox.is_some()) {
            let opts = RenderOptions {
                width: px_w,
                height: px_h,
                padding: 0.0,
                theme: self.theme,
                viewport: self.view_bbox,
                ..Default::default()
            };
            match render_png(&self.merged, &opts)
                .map_err(|e| e.to_string())
                .and_then(|png| {
                    tiny_skia::Pixmap::decode_png(&png).map_err(|e| format!("PNG 解码失败: {e}"))
                }) {
                Ok(pixmap) => {
                    // 渲染内核以不透明画布色铺底，像素全不透明（a=255），
                    // 预乘与直通 RGBA 等价。
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [pixmap.width() as usize, pixmap.height() as usize],
                        pixmap.data(),
                    );
                    self.texture = Some(ui.ctx().load_texture(
                        "map-canvas",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.tex_px = [px_w, px_h];
                    self.render_dirty = false;
                }
                Err(e) => {
                    self.status = format!("渲染失败: {e}");
                    self.render_dirty = false;
                }
            }
        }

        // 绘制：纹理铺满画布；空状态给中文引导。
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::ZERO, ui.visuals().extreme_bg_color);
        if let Some(tex) = &self.texture {
            painter.image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        if self.layers.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "◇ 堪舆\n\n拖入数据文件，或点击右上角「打开数据…」\n支持 shp / geojson / fgb / parquet / dxf / dwg / kml / kmz / csv / tsv / xlsx",
                egui::FontId::proportional(16.0),
                ui.visuals().weak_text_color(),
            );
        }
    }

    /// 打开失败：中文错误模态框。
    fn error_modal(&mut self, ctx: &egui::Context) {
        if self.error_msg.is_none() {
            return;
        }
        let mut open = true;
        egui::Window::new("打开失败")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(self.error_msg.as_deref().unwrap_or_default());
                ui.add_space(8.0);
                if ui.button("确 定").clicked() {
                    self.error_msg = None;
                }
            });
        if !open {
            self.error_msg = None;
        }
    }

    /// 拖文件入窗即打开。
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

    /// 截图验证模式：延时到点后请求窗口截图，收到回图保存 PNG 并关窗。
    /// 管线为 egui 原生 `ViewportCommand::Screenshot` → `Event::Screenshot`
    ///（eframe wgpu 集成在渲染后读回交换链），截取真实窗口全部内容。
    /// 截图模式下每帧 `request_repaint()` 保持帧流（交互模式不调用本函数）。
    fn handle_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.screenshot else {
            return;
        };
        ctx.request_repaint();
        if !shot.requested && shot.start.elapsed() >= shot.delay {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            shot.requested = true;
        }

        let mut image = None;
        ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Screenshot { image: img, .. } = ev {
                    image = Some(img.clone());
                }
            }
        });
        // 注意：`self.screenshot.take()` 只在收到回图后执行
        //（放进 if-let 元组会每帧无条件求值，提前吞掉状态）。
        if let Some(img) = image {
            let shot = self.screenshot.take().unwrap_or_else(|| unreachable!());
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
        }
    }
}

impl eframe::App for KanyuApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);
        self.title_bar(ui);
        if self.show_layers_panel {
            self.layers_panel(ui);
        }
        self.status_bar(ui);
        self.map_canvas(ui);
        self.error_modal(&ctx);
        self.handle_screenshot(&ctx);
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

/// egui 默认字体不含 CJK：从系统字体目录注入中文字体作为回退族
///（Windows 微软雅黑/黑体/宋体、macOS 苹方、Linux Noto Sans CJK）。
fn load_cjk_font(ctx: &egui::Context) {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\Noto Sans SC (TrueType).otf",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ]
    } else {
        &[
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ]
    };
    let Some((path, bytes)) = candidates
        .iter()
        .find_map(|p| std::fs::read(p).ok().map(|b| (*p, b)))
    else {
        eprintln!("警告：未找到系统中文字体，中文可能无法显示");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk".to_string(), egui::FontData::from_owned(bytes).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk".to_string());
    }
    ctx.set_fonts(fonts);
    eprintln!("已注入中文字体: {path}");
}

/// 总规 §1.2 色板 → egui Visuals（晨山 / 夜观星）。
fn apply_theme(ctx: &egui::Context, theme: Theme) {
    fn rgb(hex: u32) -> Color32 {
        Color32::from_rgb(
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    }
    let mut v = match theme {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark => egui::Visuals::dark(),
    };
    // (背景主/背景次/背景三/画布/文本主/文本弱/强调/边框/悬停/按下/选中)
    let (bg1, bg2, bg3, canvas, text, text_weak, accent, border, hover, pressed, selection) =
        match theme {
            // 晨山：雾白 / 纯白 / 浅灰；墨黑 / 中灰；远黛青；琥珀选中 20%。
            Theme::Light => (
                rgb(0xF7F5F2),
                rgb(0xFFFFFF),
                rgb(0xEDEAE6),
                rgb(0xF0EDE8),
                rgb(0x1A1A1A),
                rgb(0x8A8A8A),
                rgb(0x2D6A5E),
                rgb(0xE0DDD8),
                Color32::from_rgba_unmultiplied(0x2D, 0x6A, 0x5E, 20),
                rgb(0xE8E5E1),
                Color32::from_rgba_unmultiplied(0xD4, 0xA8, 0x43, 51),
            ),
            // 夜观星：墨夜 / 深灰蓝 / 中灰蓝；月白 / 暗灰；青玉；金珀选中 25%。
            Theme::Dark => (
                rgb(0x121418),
                rgb(0x1A1D22),
                rgb(0x23272E),
                rgb(0x0D0F12),
                rgb(0xE8E4DF),
                rgb(0x6E737A),
                rgb(0x4DB8A8),
                rgb(0x2A2F36),
                Color32::from_rgba_unmultiplied(0x4D, 0xB8, 0xA8, 31),
                rgb(0x2A2F36),
                Color32::from_rgba_unmultiplied(0xE9, 0xC4, 0x6A, 64),
            ),
        };
    v.panel_fill = bg1;
    v.window_fill = bg2;
    v.faint_bg_color = bg3;
    v.extreme_bg_color = canvas;
    v.override_text_color = Some(text);
    v.weak_text_color = Some(text_weak);
    v.hyperlink_color = accent;
    v.selection.bg_fill = selection;
    v.selection.stroke = Stroke::new(1.0, accent);
    v.widgets.noninteractive.bg_fill = bg1;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
    v.widgets.inactive.bg_fill = bg3;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, border);
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, accent);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.bg_fill = pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.5, accent);
    ctx.set_visuals(v);
}
