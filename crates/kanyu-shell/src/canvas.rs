//! 地图画布：交互（滚轮缩放 / 左键平移）→ 视图数学 → 显式视口重渲。
//! 渲染链路：可见图层合并缓存 → `kanyu_render::render_png` →
//! tiny-skia 解码 → `egui::ColorImage` → TextureHandle（物理像素，高分屏不糊）。
//!
//! 画布底色恒为纯白（用户指令：地图框背景纯白，与界面主题解耦；
//! 导出地图仍走主题色，见 app::op_export_map）。

/// 画布底色（纯白，render 的 background 覆盖参数与 egui 铺底共用）。
pub const CANVAS_BACKGROUND: &str = "#FFFFFF";

use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Vec2};
use geojson::FeatureCollection;
use kanyu_render::{render_png, RenderOptions, StyleRule, Theme};

use crate::view::{self, BBox};

/// 画布状态（纹理与视图）。多实例经 `tex_name` 区分（每视图一张纹理）。
pub struct MapCanvas {
    /// 纹理名前缀（egui 纹理 id 键；多视图必须唯一，层纹理按 `{名}-{层id}` 派生）。
    pub tex_name: String,
    /// 纹理对应的物理像素尺寸（高分屏重渲判定）。
    tex_px: [u32; 2],
    /// 逐图层纹理（按 toc 自下而上顺序叠图；层符号化独立）。
    textures: Vec<egui::TextureHandle>,
    /// 渲染脏标记。
    pub dirty: bool,
    /// 编辑按下捕获（编辑态手势暂存）。
    edit_press: Option<EditPress>,
    /// 演示用草图光标（数据坐标；截图验证橡皮筋预览，真实悬停优先）。
    pub demo_sketch_cursor: Option<(f64, f64)>,
    /// WMS 底图纹理（按地理范围映射；与矢量纹理独立——每帧绘制，不进 dirty 链）。
    wms: Option<WmsTex>,
}

/// WMS 底图纹理（缓存键 = 视口 bbox + 物理像素尺寸）。
struct WmsTex {
    /// 纹理。
    tex: egui::TextureHandle,
    /// 缓存键（请求时的视口 bbox 与尺寸）。
    key: ([f64; 4], u32, u32),
}

/// 编辑按下捕获（顶点/要素/起点）。
struct EditPress {
    feature: Option<usize>,
    path: Option<kanyu_edit::GeomPath>,
    old: Option<Vec<f64>>,
    start: (f64, f64),
}

impl EditPress {
    fn empty(start: (f64, f64)) -> Self {
        Self {
            feature: None,
            path: None,
            old: None,
            start,
        }
    }
}

impl Default for MapCanvas {
    fn default() -> Self {
        Self {
            tex_name: "map-canvas".to_string(),
            tex_px: [0, 0],
            textures: Vec::new(),
            dirty: true,
            edit_press: None,
            demo_sketch_cursor: None,
            wms: None,
        }
    }
}

impl MapCanvas {
    /// 指定纹理名的实例（额外地图视图用）。
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            tex_name: name.into(),
            ..Default::default()
        }
    }

    /// 物理像素尺寸（WMS 请求 WIDTH/HEIGHT 用；未首渲为 [0,0]）。
    pub fn phys_px(&self) -> [u32; 2] {
        self.tex_px
    }

    /// WMS 底图当前缓存键（None = 无底图）。
    pub fn wms_key(&self) -> Option<([f64; 4], u32, u32)> {
        self.wms.as_ref().map(|w| w.key)
    }

    /// 设置 WMS 底图（PNG 字节 → 纹理；失败给中文错误，不影响矢量渲染）。
    pub fn set_wms(
        &mut self,
        key: ([f64; 4], u32, u32),
        png: &[u8],
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let pixmap =
            tiny_skia::Pixmap::decode_png(png).map_err(|e| format!("WMS 影像解码失败: {e}"))?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [pixmap.width() as usize, pixmap.height() as usize],
            pixmap.data(),
        );
        let tex = ctx.load_texture(
            format!("{}-wms", self.tex_name),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.wms = Some(WmsTex { tex, key });
        Ok(())
    }

    /// 清除 WMS 底图。
    pub fn clear_wms(&mut self) {
        self.wms = None;
    }

    /// 编辑手势（按下捕获 → 松开结算为 EditAction）。
    #[allow(clippy::too_many_arguments)]
    fn handle_edit_gestures(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        rect: Rect,
        bbox: BBox,
        w: f64,
        h: f64,
        layers: &[LayerSlice<'_>],
        ev: &EditView<'_>,
    ) -> Option<crate::edit::EditAction> {
        use crate::edit::{EditAction, EditTool};
        let slice = layers.iter().find(|l| l.id == ev.target)?;
        let to_data = |pos: Pos2| {
            view::screen_to_data(
                f64::from(pos.x - rect.min.x),
                f64::from(pos.y - rect.min.y),
                bbox,
                w,
                h,
            )
        };
        // 按下：按工具捕获。
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                let sp = (pos.x - rect.min.x, pos.y - rect.min.y);
                self.edit_press = Some(match ev.tool {
                    EditTool::Vertex => crate::edit::hit_vertex(
                        slice.collection,
                        bbox,
                        w,
                        h,
                        sp,
                        crate::edit::HIT_TOL_PX,
                    )
                    .map(|(fi, path, data)| EditPress {
                        feature: Some(fi),
                        path: Some(path),
                        old: Some(data),
                        start: to_data(pos),
                    })
                    .unwrap_or_else(|| EditPress::empty(to_data(pos))),
                    EditTool::Move => {
                        let hit = crate::edit::hit_feature(
                            slice.collection,
                            bbox,
                            w,
                            h,
                            sp,
                            crate::edit::HIT_TOL_PX,
                        );
                        EditPress {
                            feature: hit,
                            path: None,
                            old: None,
                            start: to_data(pos),
                        }
                    }
                    _ => EditPress::empty(to_data(pos)),
                });
            }
        }
        // 松开：结算。
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            let press = self.edit_press.take()?;
            let pos = response.interact_pointer_pos()?;
            let mut cur = to_data(pos);
            // 顶点捕捉：绘制/顶点编辑类工具，容差内吸附到既有顶点。
            if ev.snap
                && matches!(
                    ev.tool,
                    EditTool::Vertex
                        | EditTool::AddPoint
                        | EditTool::AddLine
                        | EditTool::AddPolygon
                        | EditTool::AddHole
                )
            {
                let cols: Vec<&geojson::FeatureCollection> =
                    layers.iter().map(|l| l.collection).collect();
                let sp = (pos.x - rect.min.x, pos.y - rect.min.y);
                if let Some((data, _)) =
                    crate::edit::snap_vertex(&cols, bbox, w, h, sp, crate::edit::SNAP_TOL_PX)
                {
                    cur = data;
                }
            }
            let moved_px = response.drag_delta().length();
            match ev.tool {
                EditTool::Vertex => {
                    if let (Some(fi), Some(path), Some(old)) =
                        (press.feature, press.path, press.old)
                    {
                        if moved_px > 2.0 {
                            return Some(EditAction::MoveVertex {
                                feature: fi,
                                path,
                                old,
                                new: vec![cur.0, cur.1],
                            });
                        }
                    }
                    None
                }
                EditTool::Move => {
                    if let Some(fi) = press.feature {
                        if moved_px > 2.0 {
                            return Some(EditAction::MoveFeature {
                                feature: fi,
                                dx: cur.0 - press.start.0,
                                dy: cur.1 - press.start.1,
                            });
                        }
                    }
                    None
                }
                EditTool::AddPoint => Some(EditAction::InsertPoint { pos: cur }),
                EditTool::AddLine | EditTool::AddPolygon | EditTool::AddHole => {
                    // 双击 = 加最后顶点并完成（第一击已加点，第二击仅结算）。
                    if response.double_clicked() {
                        return Some(EditAction::DrawFinish);
                    }
                    // 单击（位移 ≤2px）加顶点；拖动不加点。
                    if moved_px <= 2.0 {
                        Some(EditAction::DrawAddVertex { pos: cur })
                    } else {
                        None
                    }
                }
                EditTool::Select | EditTool::Delete => {
                    let sp = (pos.x - rect.min.x, pos.y - rect.min.y);
                    let hit = crate::edit::hit_feature(
                        slice.collection,
                        bbox,
                        w,
                        h,
                        sp,
                        crate::edit::HIT_TOL_PX,
                    );
                    Some(EditAction::Select(hit))
                }
            }
        } else {
            // Delete 键删选中（悬停画布时）。
            if response.hovered()
                && ev.selected.is_some()
                && ui.input(|i| i.key_pressed(egui::Key::Delete))
            {
                return Some(EditAction::DeleteSelected);
            }
            // 绘制中快捷键：Enter 完成 / Esc 放弃 / Backspace 撤点。
            if response.hovered() && ev.drawing.is_some() {
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    return Some(EditAction::DrawFinish);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    return Some(EditAction::DrawCancel);
                }
                if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
                    return Some(EditAction::DrawUndoVertex);
                }
            }
            None
        }
    }

    /// 编辑句柄绘制（顶点小方块；选中要素高亮）。
    #[allow(clippy::too_many_arguments)]
    fn draw_edit_handles(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        bbox: BBox,
        w: f64,
        h: f64,
        layers: &[LayerSlice<'_>],
        ev: &EditView<'_>,
    ) {
        let Some(slice) = layers.iter().find(|l| l.id == ev.target) else {
            return;
        };
        let p = crate::theme::palette(if ui.visuals().dark_mode {
            kanyu_render::Theme::Dark
        } else {
            kanyu_render::Theme::Light
        });
        let painter = ui.painter();
        for ((fi, _path), (sx, sy)) in crate::edit::vertex_positions(slice.collection, bbox, w, h) {
            let center = egui::pos2(rect.min.x + sx, rect.min.y + sy);
            let is_sel = ev.selected == Some(fi);
            let r =
                egui::Rect::from_center_size(center, Vec2::splat(if is_sel { 9.0 } else { 7.0 }));
            painter.rect_filled(r, 1.0, Color32::WHITE);
            painter.rect_stroke(
                r,
                1.0,
                egui::Stroke::new(if is_sel { 2.0 } else { 1.0 }, p.accent),
                egui::StrokeKind::Middle,
            );
        }
    }

    /// 线/面绘制草图预览：已定点串（实线 + 顶点小方块）+ 到光标的橡皮筋
    /// （accent 虚线）；面 ≥3 点时补首末虚线预示闭合。
    #[allow(clippy::too_many_arguments)]
    fn draw_sketch(
        &self,
        ui: &egui::Ui,
        rect: Rect,
        bbox: BBox,
        w: f64,
        h: f64,
        drawing: &crate::edit::DrawState,
        cursor: Option<Pos2>,
    ) {
        let p = crate::theme::palette(if ui.visuals().dark_mode {
            kanyu_render::Theme::Dark
        } else {
            kanyu_render::Theme::Light
        });
        let painter = ui.painter();
        let to_screen = |pt: &[f64; 2]| {
            let (sx, sy) = crate::scene3d::data_to_canvas(pt[0], pt[1], bbox, w, h);
            egui::pos2(rect.min.x + sx, rect.min.y + sy)
        };
        let pts: Vec<Pos2> = drawing.verts.iter().map(to_screen).collect();
        // 已定点串（实线）。
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(
                pts.clone(),
                egui::Stroke::new(1.5, p.accent),
            ));
        }
        // 面：首末虚线预示闭合（≥3 点）。
        if drawing.kind == crate::edit::DrawKind::Polygon && pts.len() >= 3 {
            dashed_segment(
                painter,
                *pts.last().expect("len≥3"),
                pts[0],
                egui::Stroke::new(1.0, p.accent),
            );
        }
        // 橡皮筋：末顶点 → 光标（虚线）。
        if let (Some(last), Some(cur)) = (pts.last(), cursor) {
            dashed_segment(painter, *last, cur, egui::Stroke::new(1.0, p.accent));
        }
        // 顶点小方块（与句柄同风格）。
        for pt in &pts {
            let r = egui::Rect::from_center_size(*pt, Vec2::splat(7.0));
            painter.rect_filled(r, 1.0, Color32::WHITE);
            painter.rect_stroke(
                r,
                1.0,
                egui::Stroke::new(1.0, p.accent),
                egui::StrokeKind::Middle,
            );
        }
    }
}

/// 虚线段（dash 4px / gap 3px 手绘分段，避免依赖 egui 虚线 API 形状差异）。
fn dashed_segment(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: egui::Stroke) {
    const DASH: f32 = 4.0;
    const GAP: f32 = 3.0;
    let v = b - a;
    let len = v.length();
    if len < 1e-3 {
        return;
    }
    let dir = v / len;
    let mut d = 0.0;
    while d < len {
        let e = (d + DASH).min(len);
        painter.line_segment([a + dir * d, a + dir * e], stroke);
        d += DASH + GAP;
    }
}

/// 单层渲染切片（app 按目录树自下而上顺序构造）。
pub struct LayerSlice<'a> {
    /// 图层 id。
    pub id: &'a str,
    /// 要素集合。
    pub collection: &'a FeatureCollection,
    /// 符号化样式（None = 主题默认）。
    pub style: Option<StyleRule>,
    /// 符号化主色（3D 场景棱柱取色）。
    pub color: Color32,
}

/// 画布帧输入。
pub struct CanvasInput<'a> {
    /// 有效可见图层切片（自下而上绘制）。
    pub layers: &'a [LayerSlice<'a>],
    /// 主题。
    pub theme: Theme,
    /// 当前视口（数据坐标 bbox）。
    pub view_bbox: Option<BBox>,
    /// 加载后待首帧等比嵌入。
    pub needs_fit: bool,
    /// 数据范围（可见图层 bbox 并集）。
    pub data_extent: Option<BBox>,
    /// 空状态提示（无图层时）。
    pub empty_hint: &'a str,
    /// 编辑态（Some = 编辑会话中：手势走编辑工具而非平移）。
    pub edit: Option<EditView<'a>>,
}

/// 编辑态画布输入。
pub struct EditView<'a> {
    /// 当前工具。
    pub tool: crate::edit::EditTool,
    /// 目标图层 id。
    pub target: &'a str,
    /// 选中要素（句柄高亮）。
    pub selected: Option<usize>,
    /// 线/面绘制中状态（草图预览）。
    pub drawing: Option<&'a crate::edit::DrawState>,
    /// 顶点捕捉开关（指示圆环 + 手势吸附）。
    pub snap: bool,
}

/// 逐图层渲染并合成单张 PNG（白底键控叠图；布局地图框/导出共用）。
pub fn composite_layers_png(
    layers: &[LayerSlice],
    width: u32,
    height: u32,
    viewport: Option<BBox>,
    theme: Theme,
) -> Result<Vec<u8>, String> {
    use tiny_skia::{Pixmap, PixmapPaint, Transform};
    let mut base = Pixmap::new(width.max(1), height.max(1))
        .ok_or_else(|| format!("合成尺寸非法（{width}×{height}）"))?;
    base.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
    for (i, slice) in layers.iter().enumerate() {
        let opts = RenderOptions {
            width,
            height,
            padding: 0.0,
            theme,
            background: Some(CANVAS_BACKGROUND.to_string()),
            viewport,
            style: slice.style.clone(),
        };
        let png = render_png(slice.collection, &opts).map_err(|e| e.to_string())?;
        let pixmap =
            tiny_skia::Pixmap::decode_png(&png).map_err(|e| format!("PNG 解码失败: {e}"))?;
        if i == 0 {
            base.draw_pixmap(
                0,
                0,
                pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::default(),
                None,
            );
        } else {
            // 上层：纯白底抠为透明后叠图（画布恒纯白，键控边缘无感知）。
            let mut data = pixmap.data().to_vec();
            for px in data.chunks_exact_mut(4) {
                if px[0] > 250 && px[1] > 250 && px[2] > 250 {
                    px[3] = 0;
                }
            }
            let keyed = tiny_skia::Pixmap::from_vec(
                data,
                tiny_skia::IntSize::from_wh(pixmap.width(), pixmap.height())
                    .ok_or("合成像素缓冲构造失败")?,
            )
            .ok_or("合成像素缓冲构造失败")?;
            base.draw_pixmap(
                0,
                0,
                keyed.as_ref(),
                &PixmapPaint::default(),
                Transform::default(),
                None,
            );
        }
    }
    base.encode_png().map_err(|e| format!("PNG 编码失败: {e}"))
}

/// 画布帧输出。
pub struct CanvasOutput {
    /// 交互后的视口。
    pub view_bbox: Option<BBox>,
    /// 鼠标数据坐标。
    pub mouse_data: Option<(f64, f64)>,
    /// needs_fit 已消费。
    pub fit_consumed: bool,
    /// 渲染错误（状态栏反馈）。
    pub render_error: Option<String>,
    /// 编辑手势产出（编辑态下）。
    pub edit_action: Option<crate::edit::EditAction>,
}

impl MapCanvas {
    /// 画布帧（在根 ui 的剩余区域内自绘）。
    pub fn ui(&mut self, ui: &mut egui::Ui, input: CanvasInput<'_>) -> CanvasOutput {
        let mut output = CanvasOutput {
            view_bbox: input.view_bbox,
            mouse_data: None,
            fit_consumed: false,
            render_error: None,
            edit_action: None,
        };
        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        let (w, h) = (f64::from(rect.width()), f64::from(rect.height()));
        if w < 1.0 || h < 1.0 {
            return output;
        }
        let ppp = f64::from(ui.ctx().pixels_per_point());
        let px_w = ((rect.width() as f64 * ppp).round() as u32).clamp(1, 8192);
        let px_h = ((rect.height() as f64 * ppp).round() as u32).clamp(1, 8192);

        // 首帧 / 新加载：数据范围等比嵌入画布。
        if input.needs_fit {
            output.view_bbox = input.data_extent.map(|ext| view::fit_view(ext, w, h));
            output.fit_consumed = true;
            self.dirty = true;
        }

        if let Some(bbox) = output.view_bbox {
            // 画布尺寸变化：重扩边维持"视口与画布同比例"不变式。
            if self.tex_px != [0, 0] {
                let logical_w = f64::from(self.tex_px[0]) / ppp;
                let logical_h = f64::from(self.tex_px[1]) / ppp;
                if (logical_w - w).abs() > 1.0 || (logical_h - h).abs() > 1.0 {
                    output.view_bbox = Some(view::fit_view(bbox, w, h));
                    self.dirty = true;
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
                    output.view_bbox = Some(view::zoom_at(bbox, anchor, factor));
                    self.dirty = true;
                }
            }

            // 编辑态：手势走编辑工具（平移让位；滚轮缩放保留）。
            if let Some(ev) = &input.edit {
                output.edit_action =
                    self.handle_edit_gestures(ui, &response, rect, bbox, w, h, input.layers, ev);
            } else if response.dragged_by(egui::PointerButton::Primary) {
                // 左键拖拽平移（内容跟随鼠标）。
                let d = response.drag_delta();
                if d != Vec2::ZERO {
                    output.view_bbox = Some(view::pan(bbox, f64::from(d.x), f64::from(d.y), w, h));
                    self.dirty = true;
                }
            }
        }

        // 鼠标数据坐标（状态栏）。
        output.mouse_data = match (response.hover_pos(), output.view_bbox) {
            (Some(pos), Some(bbox)) => Some(view::screen_to_data(
                f64::from(pos.x - rect.min.x),
                f64::from(pos.y - rect.min.y),
                bbox,
                w,
                h,
            )),
            _ => None,
        };

        // 状态变化重渲：逐图层独立 render_png（带该层符号化），
        // 同尺寸同视口按序叠图。性能注：渲染成本 ≈ 图层数 × 单层成本
        // （每帧仅在 dirty/尺寸变化时重渲，平移缩放走纹理复用）。
        let size_changed = self.tex_px != [px_w, px_h];
        if self.dirty || (size_changed && output.view_bbox.is_some()) {
            self.textures.clear();
            let mut first_err = None;
            for (i, slice) in input.layers.iter().enumerate() {
                let opts = RenderOptions {
                    width: px_w,
                    height: px_h,
                    padding: 0.0,
                    theme: input.theme,
                    // 画布底色恒纯白（见模块头注释）；上层叠图时白底抠透明。
                    background: Some(CANVAS_BACKGROUND.to_string()),
                    viewport: output.view_bbox,
                    style: slice.style.clone(),
                };
                match render_png(slice.collection, &opts)
                    .map_err(|e| e.to_string())
                    .and_then(|png| {
                        tiny_skia::Pixmap::decode_png(&png)
                            .map_err(|e| format!("PNG 解码失败: {e}"))
                    }) {
                    Ok(pixmap) => {
                        // 渲染内核以不透明画布色铺底，预乘与直通 RGBA 等价。
                        let mut data = pixmap.data().to_vec();
                        if i > 0 {
                            // 上层：纯白底抠为透明（画布恒纯白，键控边缘无感知）。
                            for px in data.chunks_exact_mut(4) {
                                if px[0] > 250 && px[1] > 250 && px[2] > 250 {
                                    px[3] = 0;
                                }
                            }
                        }
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [pixmap.width() as usize, pixmap.height() as usize],
                            &data,
                        );
                        self.textures.push(ui.ctx().load_texture(
                            format!("{}-{}", self.tex_name, slice.id),
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(format!("渲染失败: {e}"));
                        }
                    }
                }
            }
            self.tex_px = [px_w, px_h];
            self.dirty = false;
            output.render_error = first_err;
        }

        // 绘制：层纹理按序铺满画布（自下而上）；空状态给中文引导。
        // 铺底恒纯白（与 render background 一致）；提示文字用晨山弱色
        // （夜观星的弱色在白底上对比不足）。
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::ZERO, Color32::WHITE);
        // WMS 底图（白底之上、矢量层之下）：按缓存图的地理范围映射到当前视口——
        // 视口微动时旧图随范围平移缩放（不产生错位），新图到达后无缝替换。
        if let (Some(wms), Some(bbox)) = (&self.wms, output.view_bbox) {
            let tl = crate::scene3d::data_to_canvas(wms.key.0[0], wms.key.0[3], bbox, w, h);
            let br = crate::scene3d::data_to_canvas(wms.key.0[2], wms.key.0[1], bbox, w, h);
            let dest = Rect::from_min_max(
                Pos2::new(rect.min.x + tl.0, rect.min.y + tl.1),
                Pos2::new(rect.min.x + br.0, rect.min.y + br.1),
            );
            painter.with_clip_rect(rect).image(
                wms.tex.id(),
                dest,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        for tex in &self.textures {
            painter.image(
                tex.id(),
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // 编辑态：顶点句柄叠加（纹理之上）。
        if let (Some(ev), Some(bbox)) = (&input.edit, output.view_bbox) {
            self.draw_edit_handles(ui, rect, bbox, w, h, input.layers, ev);
            // 顶点捕捉：指示圆环（容差内最近既有顶点）+ 草图橡皮筋端点吸附。
            let snap_on = ev.snap
                && matches!(
                    ev.tool,
                    crate::edit::EditTool::Vertex
                        | crate::edit::EditTool::AddPoint
                        | crate::edit::EditTool::AddLine
                        | crate::edit::EditTool::AddPolygon
                        | crate::edit::EditTool::AddHole
                );
            let cursor_raw = response.hover_pos().or_else(|| {
                self.demo_sketch_cursor.map(|d| {
                    let (sx, sy) = crate::scene3d::data_to_canvas(d.0, d.1, bbox, w, h);
                    egui::pos2(rect.min.x + sx, rect.min.y + sy)
                })
            });
            let snap_screen = if snap_on {
                cursor_raw.and_then(|pos| {
                    let cols: Vec<&FeatureCollection> =
                        input.layers.iter().map(|l| l.collection).collect();
                    crate::edit::snap_vertex(
                        &cols,
                        bbox,
                        w,
                        h,
                        (pos.x - rect.min.x, pos.y - rect.min.y),
                        crate::edit::SNAP_TOL_PX,
                    )
                    .map(|(_d, sp)| egui::pos2(rect.min.x + sp.0, rect.min.y + sp.1))
                })
            } else {
                None
            };
            // 线/面绘制草图预览（吸附点 > 真实悬停 > 演示光标）。
            if let Some(drawing) = ev.drawing {
                self.draw_sketch(ui, rect, bbox, w, h, drawing, snap_screen.or(cursor_raw));
            }
            if let Some(c) = snap_screen {
                let p = crate::theme::palette(if ui.visuals().dark_mode {
                    kanyu_render::Theme::Dark
                } else {
                    kanyu_render::Theme::Light
                });
                ui.painter()
                    .circle_stroke(c, 6.0, egui::Stroke::new(1.5, p.accent));
            }
        }
        if input
            .layers
            .iter()
            .all(|l| l.collection.features.is_empty())
        {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                input.empty_hint,
                egui::FontId::proportional(16.0),
                crate::theme::palette(kanyu_render::Theme::Light).text_weak,
            );
        }
        output
    }
}
