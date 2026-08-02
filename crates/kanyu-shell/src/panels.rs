//! 面板系统：左侧 Contents（ArcGIS Pro 骨架目录树）、右侧属性/基因面板、
//! 底部双页签停靠区（终端 | AI 对话）、状态栏。全部 ui_kit 组件组合。

use eframe::egui;

use crate::console::ConsolePanel;
use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{hint_caption, tab_strip, text, tree_row};

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
    /// 展开/折叠图层子节点。
    ToggleExpand(usize),
    /// 全部展开/折叠。
    SetAllExpanded(bool),
    /// 导出指定图层（对话框）。
    ExportLayer(usize),
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
    /// 展开（骨架目录子节点）。
    pub expanded: bool,
    /// 选中（属性面板联动）。
    pub selected: bool,
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

/// 几何类型 → ArcGIS 式图例色（RGB）。
fn geom_color(types: &[String]) -> egui::Color32 {
    let has = |k: &str| types.iter().any(|t| t.contains(k));
    if has("Polygon") {
        egui::Color32::from_rgb(0x2D, 0x6A, 0x5E) // 远黛青
    } else if has("LineString") {
        egui::Color32::from_rgb(0x4A, 0x7C, 0x9B) // 蓝灰
    } else {
        egui::Color32::from_rgb(0xD4, 0xA8, 0x43) // 琥珀
    }
}

// ===== 图层节点（骨架目录树的图层行与子节点）=====

/// 单个图层节点（含可折叠子节点）。
fn layer_node(ui: &mut egui::Ui, index: usize, lv: &LayerView, actions: &mut Vec<PanelAction>) {
    let idx = index;
    // 图层行：几何色块 + 名称（选中加粗）+ 行尾[可见性|缩放|移除]。
    let name = if lv.selected {
        text::body(&lv.file_name).strong()
    } else {
        text::body(&lv.file_name)
    };
    let (row, toggled) = tree_row(ui, 1, None, "", Some(lv.expanded), |ui| {
        // 行尾操作（图标按钮，hover 提示）。
        let eye = if lv.visible { Icon::Eye } else { Icon::EyeOff };
        if icon_btn(ui, eye, "可见性").clicked() {
            actions.push(PanelAction::VisibilityChanged(idx, !lv.visible));
        }
        if icon_btn(ui, Icon::ZoomFit, "缩放至图层").clicked() {
            actions.push(PanelAction::ZoomToLayer(idx));
        }
        if icon_btn(ui, Icon::Close, "移除图层").clicked() {
            actions.push(PanelAction::RemoveLayer(idx));
        }
    });
    if toggled {
        actions.push(PanelAction::ToggleExpand(idx));
    }
    // 色块 + 名称绘制在 tree_row 的图标/文本位（tree_row 文本位传空，此处自绘）。
    let rect = row.rect;
    let color = geom_color(&lv.geometry_types);
    ui.painter().rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 4.0, rect.center().y),
            egui::Vec2::splat(10.0),
        ),
        2.0,
        color,
    );
    let name_resp = ui.interact(
        egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 16.0, rect.min.y),
            egui::Vec2::new(140.0, rect.height()),
        ),
        ui.id().with(("layer_name", idx)),
        egui::Sense::click(),
    );
    ui.painter().text(
        egui::pos2(rect.min.x + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &lv.file_name,
        egui::FontId::proportional(13.0),
        if lv.visible {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        },
    );
    if name_resp.clicked() {
        actions.push(PanelAction::SelectLayer(idx));
    }
    // QGIS 式右键上下文菜单。
    name_resp.context_menu(|ui| {
        if ui.button("缩放至图层").clicked() {
            actions.push(PanelAction::ZoomToLayer(idx));
            ui.close();
        }
        if ui.button("导出图层…").clicked() {
            actions.push(PanelAction::ExportLayer(idx));
            ui.close();
        }
        ui.separator();
        if ui.button("移除图层").clicked() {
            actions.push(PanelAction::RemoveLayer(idx));
            ui.close();
        }
    });
    let _ = name;

    // 子节点（展开时）：几何 / 字段 / 格式。
    if lv.expanded {
        let (_r, _) = tree_row(
            ui,
            2,
            Some(Icon::Info),
            &format!("几何: {}", lv.geometry_types.join(", ")),
            None,
            |_ui| {},
        );
        let fields = if lv.fields.len() > 8 {
            format!("{} …(+{})", lv.fields[..8].join(" · "), lv.fields.len() - 8)
        } else {
            lv.fields.join(" · ")
        };
        let (_r, _) = tree_row(
            ui,
            2,
            Some(Icon::Field),
            &format!("字段: {fields}"),
            None,
            |_ui| {},
        );
        let (_r, _) = tree_row(
            ui,
            2,
            Some(Icon::List),
            &format!("格式: {} · {} 要素", lv.format, lv.feature_count),
            None,
            |_ui| {},
        );
    }
}

/// 行尾小图标按钮（14px，hover 提示）。
fn icon_btn(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::click());
    let color = if resp.hovered() {
        crate::ui_kit::icons_color(ui)
    } else {
        ui.visuals().weak_text_color()
    };
    icons::draw(ui.painter(), icon, rect.shrink(2.0), color);
    resp.on_hover_text(tip)
}

// ===== 左侧停靠区：目录 | 图层 双页签 =====

/// 左侧停靠区页签。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeftTab {
    /// 目录（Catalog 文件浏览）。
    Catalog = 0,
    /// 图层（Contents 骨架目录）。
    Layers = 1,
}

/// 左侧停靠区：页签条 + 内容区（目录与图层各自保活）。
/// 返回（目录动作，图层面板动作）。
pub fn left_dock(
    ui: &mut egui::Ui,
    active: &mut LeftTab,
    catalog: &mut crate::catalog::CatalogPanel,
    layers: &[LayerView],
    layer_filter: &mut String,
) -> (Vec<crate::catalog::CatalogAction>, Vec<PanelAction>) {
    let mut catalog_actions = Vec::new();
    let mut layer_actions = Vec::new();
    egui::Panel::left("left_dock")
        .default_size(280.0)
        .size_range(200.0..=480.0)
        .show(ui, |ui| {
            ui.add_space(6.0);
            let mut idx = *active as usize;
            tab_strip(ui, &["目录", "图层"], &mut idx);
            *active = if idx == 0 {
                LeftTab::Catalog
            } else {
                LeftTab::Layers
            };
            ui.separator();
            match active {
                LeftTab::Catalog => {
                    catalog_actions = catalog.ui(ui);
                }
                LeftTab::Layers => {
                    layer_actions = layers_tree(ui, layers, layer_filter);
                }
            }
        });
    (catalog_actions, layer_actions)
}

/// Contents 骨架目录（QGIS 图层面板式）：顶部工具栏（缩放/移除/展开/折叠/
/// 筛选）+ 树行（可见性眼 + 几何图例色块 + 名称 + 右键上下文菜单）。
pub fn layers_tree(
    ui: &mut egui::Ui,
    layers: &[LayerView],
    filter: &mut String,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();

    // QGIS 式工具栏：选中图层操作 | 展开/折叠 | 筛选框。
    ui.horizontal(|ui| {
        let selected_idx = layers.iter().position(|lv| lv.selected);
        if icon_btn(ui, Icon::ZoomFit, "缩放至选中图层").clicked() {
            if let Some(i) = selected_idx {
                actions.push(PanelAction::ZoomToLayer(i));
            }
        }
        if icon_btn(ui, Icon::Close, "移除选中图层").clicked() {
            if let Some(i) = selected_idx {
                actions.push(PanelAction::RemoveLayer(i));
            }
        }
        ui.separator();
        if icon_btn(ui, Icon::ZoomFit, "展开全部").clicked() {
            actions.push(PanelAction::SetAllExpanded(true));
        }
        if icon_btn(ui, Icon::Reset, "折叠全部").clicked() {
            actions.push(PanelAction::SetAllExpanded(false));
        }
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(filter)
                .desired_width(f32::INFINITY)
                .hint_text("筛选图层…"),
        );
    });
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        let filter_lc = filter.trim().to_lowercase();
        let filtered: Vec<(usize, &LayerView)> = layers
            .iter()
            .enumerate()
            .filter(|(_, lv)| {
                filter_lc.is_empty() || lv.file_name.to_lowercase().contains(&filter_lc)
            })
            .collect();
        if layers.is_empty() {
            ui.add_space(4.0);
            hint_caption(
                ui,
                "尚未加载图层：从「目录」双击数据文件，或「主页 → 打开数据…」",
            );
        } else if filtered.is_empty() {
            hint_caption(ui, &format!("无匹配「{filter}」的图层"));
        }
        for (i, lv) in filtered {
            layer_node(ui, i, lv, &mut actions);
        }
    });
    actions
}

// ===== 底部双页签停靠区（终端 | AI 对话）=====

/// 底部停靠区页签。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockTab {
    /// 独立终端。
    Console = 0,
    /// AI 对话。
    AiChat = 1,
}

/// 底部双页签停靠区：页签条 + 内容区（两页签状态各自保活）。
pub fn bottom_dock(
    ui: &mut egui::Ui,
    active: &mut DockTab,
    console: &mut ConsolePanel,
    ai: &mut crate::ai::AiChatPanel,
    host: &mut dyn crate::console::ConsoleHost,
) {
    egui::Panel::bottom("bottom_dock")
        .default_size(200.0)
        .size_range(120.0..=420.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            let mut idx = *active as usize;
            tab_strip(ui, &["终端", "AI 对话"], &mut idx);
            *active = if idx == 0 {
                DockTab::Console
            } else {
                DockTab::AiChat
            };
            ui.separator();
            match active {
                DockTab::Console => {
                    console.ui(ui, host);
                }
                DockTab::AiChat => {
                    ai.ui(ui, host);
                }
            }
        });
}

// ===== 状态栏 =====

/// 底部状态栏（ArcGIS Pro 状态栏：坐标/视口宽/地图色彩模式/要素数/版本）。
pub fn status_bar(
    ui: &mut egui::Ui,
    status: &str,
    mouse_data: Option<(f64, f64)>,
    feature_count: usize,
    viewport_span: Option<f64>,
    map_theme_label: &str,
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
                    ui.separator();
                    ui.label(text::caption(map_theme_label));
                });
            });
        });
}

/// 视口宽度的友好显示（度分秒）。
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

    #[test]
    fn geom_color_by_dominant_type() {
        let poly = geom_color(&["Polygon".to_string()]);
        assert_eq!(poly, egui::Color32::from_rgb(0x2D, 0x6A, 0x5E));
        let pt = geom_color(&["Point".to_string()]);
        assert_eq!(pt, egui::Color32::from_rgb(0xD4, 0xA8, 0x43));
    }
}
