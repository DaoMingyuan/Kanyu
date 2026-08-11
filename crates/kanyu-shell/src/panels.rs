//! 面板系统：图层 Contents 目录树（ArcGIS Pro 范式：复选框 + 分组 + 右键菜单）、
//! 状态栏。停靠编排（页签条/拖动/关闭）见 [`crate::dock`]；各面板内容在此渲染。

use std::collections::HashMap;

use eframe::egui;

use crate::toc::{self, MoveDir, TocNode};
use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{hint_caption, text, toc_row, tree_row};

/// 面板动作（app 分派；一律携带图层 id / 组路径——下标随增删漂移，id 才是稳定身份）。
#[derive(Debug, Clone)]
pub enum PanelAction {
    // —— 图层行 ——
    /// 选中图层（属性面板联动）。
    SelectLayer(String),
    /// 复选框置图层可见性（id + 新值）。
    SetLayerVisible(String, bool),
    /// 右键「显示或隐藏」（取反）。
    ToggleLayerVisible(String),
    /// 缩放到指定图层。
    ZoomToLayer(String),
    /// 显示概要（输出终端）。
    ShowSummary(String),
    /// 打开属性表（打开面板并选中该图层）。
    OpenAttrTable(String),
    /// 图层属性（只读概览对话框）。
    LayerProperties(String),
    /// 父列表内移动（上移/下移/顶层/底层）。
    MoveLayer(String, MoveDir),
    /// 重命名图层（打开模态对话框）。
    RenameLayer(String),
    /// 移至分组（None = 移出到根级）。
    MoveLayerToGroup(String, Option<String>),
    /// 「移至分组 ▸ 新建组…」：建新组并把图层移入（对话框采集组名）。
    NewGroupForLayer(String),
    /// 导出指定图层（对话框）。
    ExportLayer(String),
    /// 移除指定图层。
    RemoveLayer(String),
    /// 展开/折叠图层骨架子节点（几何/字段/格式）。
    ToggleExpand(String),
    // —— 组行 ——
    /// 选中组（行高亮）。
    SelectGroup(String),
    /// 展开/折叠组。
    ToggleGroupExpand(String),
    /// 组子树展开/折叠（组菜单「展开全部/折叠全部」）。
    SetGroupExpanded(String, bool),
    /// 组一键显隐（全显语义：on=true 时子孙图层全部置可见）。
    SetGroupVisible(String, bool),
    /// 缩放至组（成员图层范围并集）。
    ZoomToGroup(String),
    /// 重命名组（打开模态对话框）。
    RenameGroup(String),
    /// 新建图层组（None = 根级，Some = 父组路径，即「新建子组」）。
    NewGroup(Option<String>),
    /// 取消分组（子项上移一级，组壳删除）。
    Ungroup(String),
    /// 移除组及全部图层。
    RemoveGroup(String),
    // —— 全局 / 空白区 ——
    /// 全部展开/折叠（组 + 图层骨架子节点）。
    SetAllExpanded(bool),
    /// 全部显示/隐藏。
    SetAllVisible(bool),
}

/// 图层条目视图数据（app 提供，面板不触碰 Layer 本体）。
#[derive(Clone)]
pub struct LayerView {
    /// 图层 id（稳定身份，树节点与动作均以 id 定位）。
    pub id: String,
    /// 显示名（文件名，可重命名）。
    pub file_name: String,
    /// 格式。
    pub format: String,
    /// 要素数。
    pub feature_count: usize,
    /// 几何类型。
    pub geometry_types: Vec<String>,
    /// 字段。
    pub fields: Vec<String>,
    /// 自身可见性（有效可见性还受祖先组约束）。
    pub visible: bool,
    /// 展开（骨架目录子节点）。
    pub expanded: bool,
    /// 符号化分类行（色块 RGB + 标注；ArcGIS Pro 图例式展开）。
    pub sym_classes: Vec<([u8; 3], String)>,
}

/// 技能条目视图数据。
#[derive(Clone)]
pub struct SkillView {
    /// 技能 id。
    pub id: String,
    /// 版本。
    pub version: String,
    /// 能力。
    pub capabilities: Vec<String>,
}

/// 几何类型 → 图例色（语义出自 theme::palette：面=强调青、线=信息蓝灰、点=三强调琥珀）。
fn geom_color(p: &crate::theme::Palette, types: &[String]) -> egui::Color32 {
    let has = |k: &str| types.iter().any(|t| t.contains(k));
    if has("Polygon") {
        p.accent
    } else if has("LineString") {
        p.info
    } else {
        p.accent_tertiary
    }
}

/// 当前色板（随界面主题）。
fn palette_of(ui: &egui::Ui) -> crate::theme::Palette {
    crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}

// ===== 图层树行（图层行 / 组行 / 上下文菜单）=====

/// 树渲染上下文（避免函数一长串参数）。
struct TocCtx<'a> {
    cache: &'a mut icons::IconCache,
    /// 目录树根（组状态查询用，见 toc 纯函数）。
    root: &'a [TocNode],
    /// id → 图层视图。
    by_id: HashMap<&'a str, &'a LayerView>,
    /// 全部组路径（「移至分组」子菜单）。
    groups: Vec<String>,
    selected_id: Option<&'a str>,
    selected_group: Option<&'a str>,
}

/// 图层自身可见性查询（供 toc 纯函数闭包注入）。
fn own_visible<'a>(ctx: &'a TocCtx) -> impl Fn(&str) -> bool + use<'a> {
    |id| ctx.by_id.get(id).map(|lv| lv.visible).unwrap_or(false)
}

/// 图层行：复选框 + 展开箭头 + 几何色块 + 名称；单击选中，右键菜单（全中文）。
fn layer_row(
    ui: &mut egui::Ui,
    ctx: &mut TocCtx,
    depth: usize,
    lv: &LayerView,
    ancestors_visible: bool,
    in_group: bool,
    actions: &mut Vec<PanelAction>,
) {
    let id = lv.id.clone();
    let p = palette_of(ui);
    let effective_visible = ancestors_visible && lv.visible;
    let r = toc_row(
        ui,
        ctx.cache,
        depth,
        lv.visible,
        Some(lv.expanded),
        None,
        Some(geom_color(&p, &lv.geometry_types)),
        &lv.file_name,
        !effective_visible,
        ctx.selected_id == Some(id.as_str()),
    );
    if let Some(v) = r.checked {
        actions.push(PanelAction::SetLayerVisible(id.clone(), v));
    }
    if r.toggled {
        actions.push(PanelAction::ToggleExpand(id.clone()));
    }
    if r.row.clicked() {
        actions.push(PanelAction::SelectLayer(id.clone()));
    }
    // 双击 = 打开图层属性（ArcGIS 习惯）。
    if r.row.double_clicked() {
        actions.push(PanelAction::LayerProperties(id.clone()));
    }
    r.row.context_menu(|ui| {
        if ui.button("缩放至图层").clicked() {
            actions.push(PanelAction::ZoomToLayer(id.clone()));
            ui.close();
        }
        if ui.button("显示概要").clicked() {
            actions.push(PanelAction::ShowSummary(id.clone()));
            ui.close();
        }
        if ui.button("打开属性表").clicked() {
            actions.push(PanelAction::OpenAttrTable(id.clone()));
            ui.close();
        }
        if ui.button("图层属性…").clicked() {
            actions.push(PanelAction::LayerProperties(id.clone()));
            ui.close();
        }
        if ui.button("显示或隐藏").clicked() {
            actions.push(PanelAction::ToggleLayerVisible(id.clone()));
            ui.close();
        }
        ui.separator();
        for (label, dir) in [
            ("上移", MoveDir::Up),
            ("下移", MoveDir::Down),
            ("移至顶层", MoveDir::Top),
            ("移至底层", MoveDir::Bottom),
        ] {
            if ui.button(label).clicked() {
                actions.push(PanelAction::MoveLayer(id.clone(), dir));
                ui.close();
            }
        }
        ui.separator();
        if ui.button("重命名…").clicked() {
            actions.push(PanelAction::RenameLayer(id.clone()));
            ui.close();
        }
        // 移至分组 ▸：现有组（全路径）+ 新建组…；组内图层另给「移出分组」。
        ui.menu_button("移至分组 ▸", |ui| {
            for g in &ctx.groups {
                if ui.button(g).clicked() {
                    actions.push(PanelAction::MoveLayerToGroup(id.clone(), Some(g.clone())));
                    ui.close();
                }
            }
            if !ctx.groups.is_empty() {
                ui.separator();
            }
            if ui.button("新建组…").clicked() {
                actions.push(PanelAction::NewGroupForLayer(id.clone()));
                ui.close();
            }
            if in_group {
                ui.separator();
                if ui.button("移出分组").clicked() {
                    actions.push(PanelAction::MoveLayerToGroup(id.clone(), None));
                    ui.close();
                }
            }
        });
        ui.separator();
        // 全局动作文案取自命令注册表（单一事实来源）。
        let export_title = crate::commands::find("export_layer")
            .map(|c| c.title)
            .unwrap_or("导出…");
        if ui.button(export_title).clicked() {
            actions.push(PanelAction::ExportLayer(id.clone()));
            ui.close();
        }
        if ui.button("移除图层").clicked() {
            actions.push(PanelAction::RemoveLayer(id.clone()));
            ui.close();
        }
    });

    // 子节点（展开时）：符号化分类行（ArcGIS Pro 图例式：色块 + 标注）垫底
    // 保留字段/格式信息行。
    if lv.expanded {
        for (color, label) in &lv.sym_classes {
            sym_class_row(ui, depth + 1, *color, label, !effective_visible);
        }
        let fields = if lv.fields.len() > 8 {
            format!("{} …(+{})", lv.fields[..8].join(" · "), lv.fields.len() - 8)
        } else {
            lv.fields.join(" · ")
        };
        let (_r, _) = tree_row(
            ui,
            ctx.cache,
            depth + 1,
            Some(Icon::Field),
            &format!("字段: {fields}"),
            None,
            |_ui| {},
        );
        let (_r, _) = tree_row(
            ui,
            ctx.cache,
            depth + 1,
            Some(Icon::List),
            &format!("格式: {} · {} 要素", lv.format, lv.feature_count),
            None,
            |_ui| {},
        );
    }
}

/// 符号化分类行（色块 + 标注；色取自符号化实际值）。
fn sym_class_row(ui: &mut egui::Ui, depth: usize, color: [u8; 3], label: &str, weak: bool) {
    let p = palette_of(ui);
    let row_h = crate::ui_kit::sizes::CONTROL_SM;
    let (rect, _) = ui.allocate_exact_size(
        egui::Vec2::new(ui.available_width(), row_h),
        egui::Sense::hover(),
    );
    // 缩进参考线（与 toc_row 同标尺）。
    for d in 0..depth {
        let x = rect.min.x + crate::ui_kit::spacing::XS + d as f32 * 14.0 + 7.0;
        ui.painter().line_segment(
            [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
            egui::Stroke::new(1.0, p.border),
        );
    }
    // 色块（10px 圆角）。
    let sw = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + crate::ui_kit::spacing::XS + depth as f32 * 14.0 + 16.0,
            rect.center().y,
        ),
        egui::Vec2::splat(10.0),
    );
    ui.painter().rect_filled(
        sw,
        2.0,
        egui::Color32::from_rgb(color[0], color[1], color[2]),
    );
    ui.painter().rect_stroke(
        sw,
        2.0,
        egui::Stroke::new(0.5, p.border),
        egui::StrokeKind::Middle,
    );
    ui.painter().text(
        egui::pos2(sw.max.x + crate::ui_kit::spacing::SM, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(crate::ui_kit::text::SIZE_BODY),
        if weak { p.text_weak } else { p.text_primary },
    );
}

/// 组行：复选框（一键全显/全隐）+ 展开箭头 + 组图标 + 「组名 (N 项)」。
fn group_row(
    ui: &mut egui::Ui,
    ctx: &mut TocCtx,
    depth: usize,
    g: &toc::GroupNode,
    path: &str,
    ancestors_visible: bool,
    actions: &mut Vec<PanelAction>,
) {
    let all_on = toc::group_all_on(ctx.root, path, own_visible(ctx));
    let label = format!("{} ({} 项)", g.name, toc::layer_count(&g.children));
    let r = toc_row(
        ui,
        ctx.cache,
        depth,
        all_on,
        Some(g.expanded),
        Some(Icon::Folder),
        None,
        &label,
        !(ancestors_visible && g.visible),
        ctx.selected_group == Some(path),
    );
    if let Some(v) = r.checked {
        actions.push(PanelAction::SetGroupVisible(path.to_string(), v));
    }
    if r.toggled {
        actions.push(PanelAction::ToggleGroupExpand(path.to_string()));
    }
    if r.row.clicked() {
        actions.push(PanelAction::SelectGroup(path.to_string()));
    }
    r.row.context_menu(|ui| {
        if ui.button("缩放至组").clicked() {
            actions.push(PanelAction::ZoomToGroup(path.to_string()));
            ui.close();
        }
        if ui.button("重命名组…").clicked() {
            actions.push(PanelAction::RenameGroup(path.to_string()));
            ui.close();
        }
        if ui.button("新建子组").clicked() {
            actions.push(PanelAction::NewGroup(Some(path.to_string())));
            ui.close();
        }
        ui.separator();
        if ui.button("全部显示").clicked() {
            actions.push(PanelAction::SetGroupVisible(path.to_string(), true));
            ui.close();
        }
        if ui.button("全部隐藏").clicked() {
            actions.push(PanelAction::SetGroupVisible(path.to_string(), false));
            ui.close();
        }
        ui.separator();
        if ui.button("展开全部").clicked() {
            actions.push(PanelAction::SetGroupExpanded(path.to_string(), true));
            ui.close();
        }
        if ui.button("折叠全部").clicked() {
            actions.push(PanelAction::SetGroupExpanded(path.to_string(), false));
            ui.close();
        }
        ui.separator();
        if ui.button("取消分组").clicked() {
            actions.push(PanelAction::Ungroup(path.to_string()));
            ui.close();
        }
        if ui.button("移除组及全部图层").clicked() {
            actions.push(PanelAction::RemoveGroup(path.to_string()));
            ui.close();
        }
    });
}

/// 递归渲染目录树节点。
fn toc_nodes_ui(
    ui: &mut egui::Ui,
    ctx: &mut TocCtx,
    nodes: &[TocNode],
    prefix: &str,
    depth: usize,
    ancestors_visible: bool,
    actions: &mut Vec<PanelAction>,
) {
    for node in nodes {
        match node {
            TocNode::Layer(id) => {
                if let Some(lv) = ctx.by_id.get(id.as_str()) {
                    layer_row(
                        ui,
                        ctx,
                        depth,
                        lv,
                        ancestors_visible,
                        !prefix.is_empty(),
                        actions,
                    );
                }
            }
            TocNode::Group(g) => {
                let path = if prefix.is_empty() {
                    g.name.clone()
                } else {
                    format!("{prefix}/{}", g.name)
                };
                group_row(ui, ctx, depth, g, &path, ancestors_visible, actions);
                if g.expanded {
                    toc_nodes_ui(
                        ui,
                        ctx,
                        &g.children,
                        &path,
                        depth + 1,
                        ancestors_visible && g.visible,
                        actions,
                    );
                }
            }
        }
    }
}

/// 行尾小图标按钮（24px = WCAG 2.2 §2.5.8 指针目标达标档，hover 提示）。
fn icon_btn(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        egui::Vec2::splat(crate::ui_kit::sizes::CONTROL_SM),
        egui::Sense::click(),
    );
    let color = if resp.hovered() {
        crate::ui_kit::icons_color(ui)
    } else {
        ui.visuals().weak_text_color()
    };
    icons::draw(ui.painter(), icon, rect.shrink(3.0), color);
    resp.on_hover_text(tip)
}

// ===== 图层树（layers_tree）=====
//
// 停靠编排（页签条 / 拖动 / 关闭 / 浮动窗）已迁至 [`crate::dock`]；
// 此处只保留面板内容渲染。

/// Contents 目录树（ArcGIS Pro 窗格范式）：顶部工具栏（新建组/缩放/移除/
/// 展开/折叠/筛选）+ 树行（可见性复选框 + 展开箭头 + 图标/几何色块 + 名称
/// + 右键上下文菜单）+ 空白区菜单。
///
/// 约定：树顶 = 最上层；有效可见性 = 自身可见且祖先组皆可见（弱色显示）。
#[allow(clippy::too_many_arguments)]
pub fn layers_tree(
    ui: &mut egui::Ui,
    toc: &[TocNode],
    layers: &[LayerView],
    selected_id: Option<&str>,
    selected_group: Option<&str>,
    filter: &mut String,
    cache: &mut crate::ui_kit::icons::IconCache,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();

    // 工具栏：新建图层组 | 缩放至选中 | 移除选中 | 展开/折叠全部 | 筛选框。
    ui.horizontal(|ui| {
        if icon_btn(ui, Icon::Folder, "新建图层组").clicked() {
            actions.push(PanelAction::NewGroup(None));
        }
        ui.separator();
        if icon_btn(ui, Icon::ZoomFit, "缩放至选中").clicked() {
            if let Some(id) = selected_id {
                actions.push(PanelAction::ZoomToLayer(id.to_string()));
            } else if let Some(g) = selected_group {
                actions.push(PanelAction::ZoomToGroup(g.to_string()));
            }
        }
        if icon_btn(ui, Icon::Close, "移除选中").clicked() {
            if let Some(id) = selected_id {
                actions.push(PanelAction::RemoveLayer(id.to_string()));
            } else if let Some(g) = selected_group {
                actions.push(PanelAction::RemoveGroup(g.to_string()));
            }
        }
        ui.separator();
        if icon_btn(ui, Icon::List, "展开全部").clicked() {
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

    // auto_shrink([false, true])：滚动条出现/消失不引发布局宽度跳动。
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let filter_lc = filter.trim().to_lowercase();
            if layers.is_empty() {
                ui.add_space(4.0);
                hint_caption(
                    ui,
                    "尚未加载图层：从「目录」双击数据文件，或「主页 → 打开数据…」",
                );
            }
            let mut ctx = TocCtx {
                cache,
                root: toc,
                by_id: layers.iter().map(|lv| (lv.id.as_str(), lv)).collect(),
                groups: toc::group_paths(toc),
                selected_id,
                selected_group,
            };
            if filter_lc.is_empty() {
                // 树模式：按目录树渲染（组嵌套）。
                toc_nodes_ui(ui, &mut ctx, toc, "", 0, true, &mut actions);
            } else {
                // 筛选模式：平铺匹配图层（ArcGIS 筛选语义），行交互与树模式一致；
                // 有效可见性仍按目录树计算（祖先组不可见 → 弱色）。
                let visible_set: std::collections::HashSet<String> =
                    toc::visible_draw_order(toc, own_visible(&ctx))
                        .into_iter()
                        .collect();
                let mut any = false;
                for lv in layers {
                    if lv.file_name.to_lowercase().contains(&filter_lc) {
                        any = true;
                        // 自身可见时，有效不可见 ⇔ 祖先组不可见。
                        let ancestors_visible = !lv.visible || visible_set.contains(&lv.id);
                        let in_group = toc::group_path_of(toc, &lv.id)
                            .map(|p| !p.is_empty())
                            .unwrap_or(false);
                        layer_row(
                            ui,
                            &mut ctx,
                            0,
                            lv,
                            ancestors_visible,
                            in_group,
                            &mut actions,
                        );
                    }
                }
                if !any {
                    hint_caption(ui, &format!("无匹配「{filter}」的图层"));
                }
            }
            // 空白区：剩余空间一个 Response 承载右键菜单（整树操作）。
            // 注意：滚动区内 available_size 高度为无限——按视口余量计算，
            // 否则内容高度被撑到无穷、滚动范围失真（长图层列表必现）。
            let blank_h = (ui.clip_rect().max.y - ui.cursor().min.y).max(24.0);
            let (_rect, blank) = ui.allocate_exact_size(
                egui::Vec2::new(ui.available_width(), blank_h),
                egui::Sense::click(),
            );
            blank.context_menu(|ui| {
                if ui.button("新建图层组").clicked() {
                    actions.push(PanelAction::NewGroup(None));
                    ui.close();
                }
                ui.separator();
                if ui.button("全部展开").clicked() {
                    actions.push(PanelAction::SetAllExpanded(true));
                    ui.close();
                }
                if ui.button("全部折叠").clicked() {
                    actions.push(PanelAction::SetAllExpanded(false));
                    ui.close();
                }
                ui.separator();
                if ui.button("全部显示").clicked() {
                    actions.push(PanelAction::SetAllVisible(true));
                    ui.close();
                }
                if ui.button("全部隐藏").clicked() {
                    actions.push(PanelAction::SetAllVisible(false));
                    ui.close();
                }
            });
        });
    actions
}

// ===== 状态栏 =====

/// 底部状态栏（ArcGIS Pro 状态栏：坐标/视口宽/坐标系/地图色彩模式/要素数/版本）。
pub fn status_bar(
    ui: &mut egui::Ui,
    status: &str,
    mouse_data: Option<(f64, f64)>,
    feature_count: usize,
    viewport_span: Option<f64>,
    map_theme_label: &str,
    crs: &str,
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
                    ui.separator();
                    ui.label(text::caption(format!("坐标系: {crs}")));
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
        let p = crate::theme::palette(kanyu_render::Theme::Light);
        assert_eq!(geom_color(&p, &["Polygon".to_string()]), p.accent);
        assert_eq!(geom_color(&p, &["LineString".to_string()]), p.info);
        assert_eq!(geom_color(&p, &["Point".to_string()]), p.accent_tertiary);
    }
}
