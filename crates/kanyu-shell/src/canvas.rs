//! 地图画布：交互（滚轮缩放 / 左键平移）→ 视图数学 → 显式视口重渲。
//! 渲染链路：可见图层合并缓存 → `kanyu_render::render_png` →
//! tiny-skia 解码 → `egui::ColorImage` → TextureHandle（物理像素，高分屏不糊）。

use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Vec2};
use geojson::FeatureCollection;
use kanyu_render::{render_png, RenderOptions, StyleRule, Theme};

use crate::view::{self, BBox};

/// 画布状态（纹理与视图）。多实例经 `tex_name` 区分（每视图一张纹理）。
pub struct MapCanvas {
    /// 纹理名（egui 纹理 id 键；多视图必须唯一）。
    pub tex_name: String,
    /// 纹理对应的物理像素尺寸（高分屏重渲判定）。
    tex_px: [u32; 2],
    texture: Option<egui::TextureHandle>,
    /// 渲染脏标记。
    pub dirty: bool,
}

impl Default for MapCanvas {
    fn default() -> Self {
        Self {
            tex_name: "map-canvas".to_string(),
            tex_px: [0, 0],
            texture: None,
            dirty: true,
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
}

/// 画布帧输入。
pub struct CanvasInput<'a> {
    /// 可见图层合并数据。
    pub merged: &'a FeatureCollection,
    /// 主题。
    pub theme: Theme,
    /// 当前视口（数据坐标 bbox）。
    pub view_bbox: Option<BBox>,
    /// 加载后待首帧等比嵌入。
    pub needs_fit: bool,
    /// 数据范围（可见图层 bbox 并集）。
    pub data_extent: Option<BBox>,
    /// 符号化样式（地图导出用，画布默认 None）。
    pub style: Option<&'a StyleRule>,
    /// 空状态提示（无图层时）。
    pub empty_hint: &'a str,
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
}

impl MapCanvas {
    /// 画布帧（在根 ui 的剩余区域内自绘）。
    pub fn ui(&mut self, ui: &mut egui::Ui, input: CanvasInput<'_>) -> CanvasOutput {
        let mut output = CanvasOutput {
            view_bbox: input.view_bbox,
            mouse_data: None,
            fit_consumed: false,
            render_error: None,
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

            // 左键拖拽平移（内容跟随鼠标）。
            if response.dragged_by(egui::PointerButton::Primary) {
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

        // 状态变化重渲：物理像素渲染（乘 pixels_per_point）。
        let size_changed = self.tex_px != [px_w, px_h];
        if self.dirty || (size_changed && output.view_bbox.is_some()) {
            let opts = RenderOptions {
                width: px_w,
                height: px_h,
                padding: 0.0,
                theme: input.theme,
                viewport: output.view_bbox,
                style: input.style.cloned(),
                ..Default::default()
            };
            match render_png(input.merged, &opts)
                .map_err(|e| e.to_string())
                .and_then(|png| {
                    tiny_skia::Pixmap::decode_png(&png).map_err(|e| format!("PNG 解码失败: {e}"))
                }) {
                Ok(pixmap) => {
                    // 渲染内核以不透明画布色铺底，预乘与直通 RGBA 等价。
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [pixmap.width() as usize, pixmap.height() as usize],
                        pixmap.data(),
                    );
                    self.texture = Some(ui.ctx().load_texture(
                        self.tex_name.clone(),
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                    self.tex_px = [px_w, px_h];
                    self.dirty = false;
                }
                Err(e) => {
                    output.render_error = Some(format!("渲染失败: {e}"));
                    self.dirty = false;
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
        if input.merged.features.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                input.empty_hint,
                egui::FontId::proportional(16.0),
                ui.visuals().weak_text_color(),
            );
        }
        output
    }
}
