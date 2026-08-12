//! 布局视图（ArcGIS Pro Layout 对应物的壳层承载）：中央页签内容 =
//! 白底纸张 + 标题 + 地图框 + 图例 + 比例尺 + 指北针（egui painter 直绘，
//! 全中文文本完整）；导出走 `kanyu_render::layout`（SVG 全排版 / PNG）。

use eframe::egui;
use kanyu_render::layout::{LayoutFrame, LayoutSpec, LegendRow};

use crate::ui_kit::tokens::text;

/// 布局视图状态。
pub struct LayoutView {
    /// 序号（「布局 N」）。
    pub id: usize,
    /// 标题（页签与页面标题共用）。
    pub title: String,
    /// 排版规格。
    pub spec: LayoutSpec,
    /// 打开状态（关闭 ≠ 删除：目录清单保留，双击行重开）。
    pub open: bool,
    /// 地图 PNG 缓存（内容纪元变化重合成）。
    pub map_png: Option<Vec<u8>>,
    /// 已合成内容纪元。
    pub epoch: u64,
    /// 地图纹理缓存（按内容尺寸重建）。
    tex: Option<egui::TextureHandle>,
    /// 缓存纹理的地图框尺寸（变化重建）。
    tex_size: (u32, u32),
}

impl LayoutView {
    /// 新建。
    pub fn new(id: usize, title: String, spec: LayoutSpec) -> Self {
        Self {
            id,
            title,
            spec,
            open: true,
            map_png: None,
            epoch: 0,
            tex: None,
            tex_size: (0, 0),
        }
    }
}

/// 布局视图动作（app 结算）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutAction {
    /// 导出 PNG…。
    ExportPng,
    /// 导出 SVG…。
    ExportSvg,
}

/// 布局视图帧。`map_png` = 当前可见图层按地图框尺寸合成的 PNG
/// （app 经 canvas::composite_layers_png 供给；变化时调用方重算）。
/// 返回导出请求。
pub fn layout_ui(
    ui: &mut egui::Ui,
    view: &mut LayoutView,
    legend: &[LegendRow],
    scale_span_m: Option<f64>,
    map_png: Option<&[u8]>,
) -> Option<LayoutAction> {
    let mut action = None;
    // 中央区无默认填充：内容全区先铺灰底（纸张白底+阴影在其上）。
    let backdrop = crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
    .bg_tertiary;
    let full_rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(full_rect, 0.0, backdrop);
    // 工具条。
    ui.horizontal(|ui| {
        if ui
            .button(text::body("导出 PNG…"))
            .on_hover_text("按纸张规格导出排版 PNG")
            .clicked()
        {
            action = Some(LayoutAction::ExportPng);
        }
        if ui
            .button(text::body("导出 SVG…"))
            .on_hover_text("按纸张规格导出排版 SVG（文字完整）")
            .clicked()
        {
            action = Some(LayoutAction::ExportSvg);
        }
        ui.separator();
        ui.label(
            text::caption(format!(
                "{} · {}dpi · 标题「{}」",
                match view.spec.page {
                    kanyu_render::layout::PageSize::A4Landscape => "A4 横",
                    kanyu_render::layout::PageSize::A4Portrait => "A4 纵",
                },
                view.spec.dpi as u32,
                view.title
            ))
            .color(ui.visuals().weak_text_color()),
        );
    });
    ui.separator();

    // 纸张适配（保持宽高比 letterbox）。
    let avail = ui.available_rect_before_wrap();
    let (pw, ph) = view.spec.page.pixels(view.spec.dpi);
    let (pw, ph) = (pw as f32, ph as f32);
    let k = (avail.width() / pw).min(avail.height() / ph);
    let page = egui::Rect::from_center_size(avail.center(), egui::Vec2::new(pw * k, ph * k));
    let painter = ui.painter().clone();
    // 纸张阴影 + 白底。
    painter.rect_filled(
        page.translate(egui::Vec2::new(4.0, 4.0)),
        4.0,
        egui::Color32::from_black_alpha(30),
    );
    painter.rect_filled(page, 2.0, egui::Color32::WHITE);
    painter.rect_stroke(
        page,
        2.0,
        egui::Stroke::new(0.5, egui::Color32::from_gray(0xC0)),
        egui::StrokeKind::Middle,
    );

    // 帧几何按显示比例换算。
    let f = LayoutFrame::compute(&view.spec);
    let tx = |x: f64| page.min.x + (x as f32) * k;
    let ty = |y: f64| page.min.y + (y as f32) * k;
    let dark = egui::Color32::from_gray(0x1A);
    let border = egui::Color32::from_gray(0xE0 - 0x1D);

    // 标题（顶部居中）。
    if f.title_h > 0.0 {
        painter.text(
            egui::pos2(page.center().x, ty(f.margin + f.title_h * 0.5)),
            egui::Align2::CENTER_CENTER,
            &view.title,
            egui::FontId::proportional(17.0 * k.max(0.5)),
            dark,
        );
    }
    // 地图框。
    let map_rect = egui::Rect::from_min_size(
        egui::pos2(tx(f.map[0]), ty(f.map[1])),
        egui::Vec2::new((f.map[2] as f32) * k, (f.map[3] as f32) * k),
    );
    painter.rect_filled(map_rect, 0.0, egui::Color32::WHITE);
    if let Some(png) = map_png {
        // 纹理缓存（尺寸变化重建）。
        let map_px = (f.map[2].round() as u32, f.map[3].round() as u32);
        if view.tex.is_none() || view.tex_size != map_px {
            if let Ok(pixmap) = tiny_skia::Pixmap::decode_png(png) {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [pixmap.width() as usize, pixmap.height() as usize],
                    pixmap.data(),
                );
                view.tex = Some(ui.ctx().load_texture(
                    format!("layout-map-{}", view.id),
                    image,
                    egui::TextureOptions::LINEAR,
                ));
                view.tex_size = map_px;
            }
        }
        if let Some(tex) = &view.tex {
            painter.image(
                tex.id(),
                map_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
    painter.rect_stroke(
        map_rect,
        0.0,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Middle,
    );

    // 图例栏（色块 + 标注）。
    if view.spec.show_legend {
        let mut ly = map_rect.min.y + 8.0 * k;
        let lx = map_rect.max.x + 12.0 * k;
        for row in legend {
            let sw =
                egui::Rect::from_min_size(egui::pos2(lx, ly), egui::Vec2::splat(10.0 * k.max(0.6)));
            painter.rect_filled(
                sw,
                2.0,
                egui::Color32::from_rgb(row.color[0], row.color[1], row.color[2]),
            );
            painter.text(
                egui::pos2(sw.max.x + 6.0 * k, sw.center().y),
                egui::Align2::LEFT_CENTER,
                &row.label,
                egui::FontId::proportional(12.0 * k.max(0.6)),
                dark,
            );
            ly += 20.0 * k;
        }
    }
    // 比例尺（地图框下沿，分段条 + 标注）。
    if view.spec.show_scalebar {
        if let Some(span_m) = scale_span_m {
            let (label, bar_px, _bar_m) =
                kanyu_render::layout::nice_scale(span_m, f64::from(map_rect.width()), 96.0);
            let sy = map_rect.max.y + 14.0 * k;
            let half = (bar_px as f32 / 2.0) * k;
            let bx = map_rect.min.x;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(bx, sy), egui::Vec2::new(half, 4.0 * k)),
                0.0,
                dark,
            );
            painter.rect_stroke(
                egui::Rect::from_min_size(
                    egui::pos2(bx + half, sy),
                    egui::Vec2::new(half, 4.0 * k),
                ),
                0.0,
                egui::Stroke::new(1.0, dark),
                egui::StrokeKind::Middle,
            );
            painter.text(
                egui::pos2(bx + half * 2.0 + 6.0 * k, sy + 2.0 * k),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0 * k.max(0.6)),
                dark,
            );
        }
    }
    // 指北针（右上角 N 箭头）。
    if view.spec.show_north {
        let nx = page.max.x - f.margin as f32 * k * 0.5;
        let ny = page.min.y + f.margin as f32 * k * 0.5;
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(nx, ny - 10.0 * k),
                egui::pos2(nx - 5.0 * k, ny + 5.0 * k),
                egui::pos2(nx + 5.0 * k, ny + 5.0 * k),
            ],
            dark,
            egui::Stroke::NONE,
        ));
        painter.text(
            egui::pos2(nx, ny + 8.0 * k),
            egui::Align2::CENTER_TOP,
            "N",
            egui::FontId::proportional(11.0 * k.max(0.6)),
            dark,
        );
    }
    action
}

/// 规格对话框状态（「＋ 新建布局框」）。
pub struct LayoutDialogState {
    /// 标题。
    pub title: String,
    /// 纸张横向。
    pub landscape: bool,
    /// 图例。
    pub legend: bool,
    /// 比例尺。
    pub scalebar: bool,
    /// 指北针。
    pub north: bool,
}

impl Default for LayoutDialogState {
    fn default() -> Self {
        Self {
            title: String::new(),
            landscape: true,
            legend: true,
            scalebar: true,
            north: true,
        }
    }
}

impl LayoutDialogState {
    /// 校验（纯函数；标题空 → 错误）。
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("布局标题不能为空".to_string());
        }
        Ok(())
    }

    /// 生成规格。
    pub fn to_spec(&self) -> LayoutSpec {
        LayoutSpec {
            page: if self.landscape {
                kanyu_render::layout::PageSize::A4Landscape
            } else {
                kanyu_render::layout::PageSize::A4Portrait
            },
            title: self.title.trim().to_string(),
            show_legend: self.legend,
            show_scalebar: self.scalebar,
            show_north: self.north,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_validate_and_spec() {
        let mut d = LayoutDialogState::default();
        assert!(d.validate().is_err()); // 空标题
        d.title = "  示范布局  ".into();
        assert!(d.validate().is_ok());
        let spec = d.to_spec();
        assert_eq!(spec.title, "示范布局");
        assert_eq!(spec.page, kanyu_render::layout::PageSize::A4Landscape);
        d.landscape = false;
        assert_eq!(d.to_spec().page, kanyu_render::layout::PageSize::A4Portrait);
    }
}
