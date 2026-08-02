//! 面板系统：左侧 Contents（图层树）、右侧属性/基因面板、底部状态栏。
//! 布局借鉴 ArcGIS Pro 的可停靠面板理念，视觉为 bitfun 卡片（ui_kit）。

use eframe::egui;

use crate::ui_kit::containers::BadgeLevel;
use crate::ui_kit::{
    badge, button, card, checkbox, hint_caption, section_header, text, ButtonVariant,
};

/// 面板动作（app 分派）。
#[derive(Debug, Clone)]
pub enum PanelAction {
    /// 缩放到指定图层。
    ZoomToLayer(usize),
    /// 移除指定图层。
    RemoveLayer(usize),
    /// 可见性变化（索引 + 新值）。
    VisibilityChanged(usize, bool),
    /// 选中图层（属性面板联动）。
    SelectLayer(usize),
    /// 打开运行基因对话框。
    OpenGeneRun,
}

/// 图层条目视图数据（app 提供，面板不触碰 Layer 本体）。
#[derive(Clone)]
pub struct LayerView {
    /// 显示名（文件名）。
    pub file_name: String,
    /// 格式。
    pub format: String,
    /// 要素数。
    pub feature_count: usize,
    /// 几何类型。
    pub geometry_types: Vec<String>,
    /// 字段。
    pub fields: Vec<String>,
    /// 可见性。
    pub visible: bool,
}

/// 基因条目视图数据。
#[derive(Clone)]
pub struct GeneView {
    /// 基因 id。
    pub id: String,
    /// 版本。
    pub version: String,
    /// 能力。
    pub capabilities: Vec<String>,
}

/// 左侧 Contents 面板（ArcGIS Pro 内容窗格：图层树）。
pub fn contents_panel(ui: &mut egui::Ui, layers: &[LayerView]) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    egui::Panel::left("contents_panel")
        .default_size(280.0)
        .size_range(200.0..=480.0)
        .show(ui, |ui| {
            ui.add_space(10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                card(ui, |ui| {
                    section_header(ui, &format!("图层（{}）", layers.len()));
                    if layers.is_empty() {
                        hint_caption(
                            ui,
                            "尚未加载图层：拖入数据文件，或经「主页 → 打开数据…」加载",
                        );
                    }
                    for (i, lv) in layers.iter().enumerate() {
                        layer_card(ui, i, lv, &mut actions);
                    }
                });
            });
        });
    actions
}

/// 单个图层卡片（可见性 + 概要 + 操作）。
fn layer_card(ui: &mut egui::Ui, index: usize, lv: &LayerView, actions: &mut Vec<PanelAction>) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(egui::CornerRadius::same(5))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let mut vis = lv.visible;
                if checkbox(ui, &mut vis, "").changed() {
                    actions.push(PanelAction::VisibilityChanged(index, vis));
                }
                if ui
                    .selectable_label(false, text::body_lg(&lv.file_name).strong())
                    .clicked()
                {
                    actions.push(PanelAction::SelectLayer(index));
                }
            });
            ui.horizontal(|ui| {
                badge(ui, &lv.format, BadgeLevel::Stable);
                ui.label(text::caption(format!("{} 要素", lv.feature_count)));
            });
            if !lv.geometry_types.is_empty() {
                ui.label(
                    text::caption(format!("几何: {}", lv.geometry_types.join(", ")))
                        .color(ui.visuals().weak_text_color()),
                );
            }
            if !lv.fields.is_empty() {
                let fields = if lv.fields.len() > 6 {
                    format!("{} …(+{})", lv.fields[..6].join(", "), lv.fields.len() - 6)
                } else {
                    lv.fields.join(", ")
                };
                ui.label(
                    text::caption(format!("字段: {fields}")).color(ui.visuals().weak_text_color()),
                );
            }
            ui.horizontal(|ui| {
                if button(ui, "缩放至图层", ButtonVariant::Subtle, true).clicked() {
                    actions.push(PanelAction::ZoomToLayer(index));
                }
                if button(ui, "移除", ButtonVariant::Danger, true).clicked() {
                    actions.push(PanelAction::RemoveLayer(index));
                }
            });
        });
    ui.add_space(6.0);
}

/// 右侧属性/基因面板。
pub fn props_panel(
    ui: &mut egui::Ui,
    selected: Option<&LayerView>,
    genes: &[GeneView],
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    egui::Panel::right("props_panel")
        .default_size(260.0)
        .size_range(200.0..=420.0)
        .show(ui, |ui| {
            ui.add_space(10.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                card(ui, |ui| {
                    section_header(ui, "属性");
                    match selected {
                        Some(lv) => {
                            ui.label(text::body_lg(&lv.file_name).strong());
                            ui.add_space(4.0);
                            ui.label(text::caption(format!("格式: {}", lv.format)));
                            ui.label(text::caption(format!("要素: {}", lv.feature_count)));
                            ui.label(text::caption(format!(
                                "几何: {}",
                                lv.geometry_types.join(", ")
                            )));
                            ui.add_space(4.0);
                            ui.label(text::caption("字段清单:"));
                            for f in &lv.fields {
                                ui.label(text::data(format!("  {f}")));
                            }
                        }
                        None => {
                            hint_caption(ui, "未选中图层（点击图层卡片的图层名查看）");
                        }
                    }
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_header(ui, &format!("基因（{}）", genes.len()));
                    if genes.is_empty() {
                        hint_caption(ui, "未加载基因：「基因 → 热加载…」选择 .wasm 文件");
                    }
                    for g in genes {
                        ui.horizontal(|ui| {
                            ui.label(text::body(&g.id).strong());
                            badge(ui, &g.version, BadgeLevel::Incubating);
                        });
                        ui.label(
                            text::caption(format!("能力: {}", g.capabilities.join(", ")))
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add_space(4.0);
                    }
                    if !genes.is_empty()
                        && button(ui, "运行基因…", ButtonVariant::Secondary, true).clicked()
                    {
                        actions.push(PanelAction::OpenGeneRun);
                    }
                });
            });
        });
    actions
}

/// 底部状态栏（ArcGIS Pro 状态栏：坐标/比例提示/要素数/版本）。
pub fn status_bar(
    ui: &mut egui::Ui,
    status: &str,
    mouse_data: Option<(f64, f64)>,
    feature_count: usize,
    viewport_span: Option<f64>,
) {
    egui::Panel::bottom("status_bar")
        .exact_size(crate::ui_kit::sizes::STATUS_BAR)
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(12.0);
                ui.label(text::caption(status));
                ui.separator();
                let coord = match mouse_data {
                    Some((x, y)) => format!("{x:.5}°E, {y:.5}°N"),
                    None => "—".to_string(),
                };
                ui.label(text::data(format!("坐标: {coord}")));
                if let Some(span) = viewport_span {
                    ui.separator();
                    ui.label(text::caption(format!("视口宽: {}", format_span(span))));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    ui.label(text::caption(format!("v{}", env!("CARGO_PKG_VERSION"))));
                    ui.separator();
                    ui.label(text::caption(format!("要素: {feature_count}")));
                });
            });
        });
}

/// 视口宽度的友好显示（度或投影单位）。
fn format_span(span: f64) -> String {
    if span >= 1.0 {
        format!("{span:.3}°")
    } else if span >= 0.001 {
        format!("{:.2}′", span * 60.0)
    } else {
        format!("{:.1}″", span * 3600.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_formatting_scales() {
        assert_eq!(format_span(2.5), "2.500°");
        assert_eq!(format_span(0.01), "0.60′");
        assert_eq!(format_span(0.0005), "1.8″");
    }
}
