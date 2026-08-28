//! 功能区（Ribbon）：ArcGIS Pro 分类设计 + "图标 + 文字 + 功能介绍"组合按钮
//! （总规 §1.4 线性图标 + 悬停介绍卡）。页签内命令组细分：组内聚、组间分隔。
//!
//! **DAML 范式**：本文件只声明「页签 → 组 → 命令 id」的布局结构
//! （[`layout_of`]），按钮文案/图标/简介/可用条件一律投影自
//! [`crate::commands::COMMANDS`] 注册表（单一事实来源）。

use eframe::egui;
use egui::{RichText, Vec2};

use crate::commands::{self, AppSnapshot};
use crate::ui_kit::icons::Icon;
use crate::ui_kit::{ribbon_button, text};

/// 功能区页签。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RibbonTab {
    /// 主页：工程文件、外观、关于。
    Home,
    /// 数据：图层、查询、交换、投影。
    Data,
    /// 编辑：编辑会话、编辑工具、撤销/重做。
    Edit,
    /// 分析：几何、叠加、关系统计。
    Analysis,
    /// 制图：渲染、输出。
    Cartography,
    /// 视图：相机、面板、地图色彩。
    View,
    /// 技能：WASM 插件。
    Skill,
    /// 帮助：文档、关于。
    Help,
}

impl RibbonTab {
    /// 全部页签（顺序即显示顺序）。
    pub const ALL: [RibbonTab; 8] = [
        RibbonTab::Home,
        RibbonTab::Data,
        RibbonTab::Edit,
        RibbonTab::Analysis,
        RibbonTab::Cartography,
        RibbonTab::View,
        RibbonTab::Skill,
        RibbonTab::Help,
    ];

    /// 页签名。
    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::Home => "主页",
            RibbonTab::Data => "数据",
            RibbonTab::Edit => "编辑",
            RibbonTab::Analysis => "分析",
            RibbonTab::Cartography => "制图",
            RibbonTab::View => "视图",
            RibbonTab::Skill => "技能",
            RibbonTab::Help => "帮助",
        }
    }
}

/// 功能区动作（按钮点击产生，由 app 统一分派执行）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RibbonAction {
    // 主页
    /// 打开数据文件对话框。
    OpenData,
    /// 加载内置示例。
    OpenExample,
    /// 打开工程（.kyu）。
    OpenProject,
    /// 保存工程（.kyu）。
    SaveProject,
    /// 保存当前窗口为 PNG。
    SaveScreenshot,
    /// 切换晨山/夜观星（仅界面，不影响地图色彩）。
    ToggleTheme,
    // 数据
    /// 图层概要（输出到终端）。
    LayerInfo,
    /// 属性查询对话框。
    QueryDialog,
    /// 图层导出对话框。
    ExportDialog,
    /// 投影变换对话框。
    ReprojectDialog,
    // 编辑
    /// 开始编辑（当前选中图层）。
    StartEdit,
    /// 保存编辑（结束会话）。
    SaveEdit,
    /// 放弃编辑（逐条逆回）。
    DiscardEdit,
    /// 切换编辑工具。
    SetEditTool(crate::edit::EditTool),
    /// 顶点捕捉开关（编辑会话内）。
    ToggleEditSnap,
    /// 拓扑编辑开关（编辑会话内；开时顶点拖拽联动共享顶点）。
    ToggleEditTopo,
    /// 撤销一步。
    Undo,
    /// 重做一步。
    Redo,
    // 分析
    /// 缓冲区对话框。
    BufferDialog,
    /// 叠加分析对话框。
    OverlayDialog,
    /// 拓扑检查（当前图层）。
    Topology,
    /// 空间连接对话框。
    SjoinDialog,
    /// 分区统计对话框。
    ZonalDialog,
    /// 测地线度量对话框。
    MeasureDialog,
    // 制图
    /// 地图导出对话框。
    ExportMapDialog,
    /// 不动产制图对话框。
    EstateMapDialog,
    // 视图
    /// 缩放到数据范围。
    ZoomToFit,
    /// 复位视图。
    ResetView,
    /// 新建二维地图框（绑定自有图层集）。
    NewFrame2D,
    /// 新建三维场景（二维/三维分开建立）。
    NewFrame3D,
    /// 面板开关（目录/图层/终端/AI 对话；关闭后可经此重开）。
    TogglePanel(crate::dock::PanelId),
    /// 打开设置对话框（坐标系 / 渲染）。
    SettingsDialog,
    /// 地图色彩模式循环（固定晨山 → 固定夜观星 → 跟随界面）。
    CycleMapTheme,
    // 技能
    /// 热加载 WASM 技能。
    SkillHotload,
    /// 技能清单（输出到终端）。
    SkillList,
    /// 运行技能对话框。
    SkillRunDialog,
    // 帮助
    /// 命令速查（输出到终端）。
    ShowHelp,
    /// 关于堪舆（模态）。
    About,
}

/// 命令组布局：组名 + 命令 id 列（投影 [`commands::COMMANDS`]）。
struct GroupLayout {
    name: &'static str,
    commands: &'static [&'static str],
}

/// 页签布局（声明式；命令文案/图标/简介见注册表）。
fn layout_of(tab: RibbonTab) -> &'static [GroupLayout] {
    match tab {
        RibbonTab::Home => &[
            GroupLayout {
                name: "工程",
                commands: &["open_data", "open_example", "open_project", "save_project"],
            },
            GroupLayout {
                name: "窗口",
                commands: &["save_screenshot", "toggle_theme", "settings"],
            },
            GroupLayout {
                name: "关于",
                commands: &["about"],
            },
        ],
        RibbonTab::Data => &[
            GroupLayout {
                name: "图层",
                commands: &["layer_info", "query"],
            },
            GroupLayout {
                name: "交换",
                commands: &["export_layer", "reproject"],
            },
        ],
        RibbonTab::Edit => &[
            GroupLayout {
                name: "会话",
                commands: &["start_edit", "save_edit", "discard_edit"],
            },
            GroupLayout {
                name: "工具",
                commands: &[
                    "edit_select",
                    "edit_vertex",
                    "edit_move",
                    "edit_add_point",
                    "edit_add_line",
                    "edit_add_polygon",
                    "edit_add_hole",
                    "edit_split",
                    "edit_delete",
                    "edit_snap",
                    "edit_topo",
                ],
            },
            GroupLayout {
                name: "撤销",
                commands: &["undo", "redo"],
            },
        ],
        RibbonTab::Analysis => &[
            GroupLayout {
                name: "几何分析",
                commands: &["buffer", "overlay", "topology"],
            },
            GroupLayout {
                name: "关系统计",
                commands: &["sjoin", "zonal", "measure"],
            },
        ],
        RibbonTab::Cartography => &[GroupLayout {
            name: "地图输出",
            commands: &["export_map", "estate_map"],
        }],
        RibbonTab::View => &[
            GroupLayout {
                name: "相机",
                commands: &["zoom_fit", "reset_view", "new_frame_2d", "new_frame_3d"],
            },
            GroupLayout {
                name: "面板",
                commands: &[
                    "panel_catalog",
                    "panel_layers",
                    "panel_toolbox",
                    "panel_attrtable",
                    "panel_console",
                    "panel_aichat",
                ],
            },
            GroupLayout {
                name: "地图色彩",
                commands: &["map_theme"],
            },
        ],
        RibbonTab::Skill => &[GroupLayout {
            name: "WASM 技能",
            commands: &["skill_hotload", "skill_list", "skill_run"],
        }],
        RibbonTab::Help => &[GroupLayout {
            name: "文档",
            commands: &["help", "about"],
        }],
    }
}

/// 功能区状态（当前页签）。
pub struct Ribbon {
    /// 当前激活页签。
    pub active: RibbonTab,
}

impl Default for Ribbon {
    fn default() -> Self {
        Self {
            active: RibbonTab::Home,
        }
    }
}

impl Ribbon {
    /// 功能区 UI（ArcGIS Pro 三段式：QAT 快速访问栏 + 页签行 + 命令组行）。
    /// `snap` 为命令可用条件求值快照（无图层/无选中 → 相应命令置灰）。
    /// 返回点击产生的动作（每帧至多一个）。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut crate::ui_kit::icons::IconCache,
        snap: &AppSnapshot,
    ) -> Option<RibbonAction> {
        let mut action = None;

        // ── QAT 快速访问工具栏（26px：品牌标 + 高频小按钮 + 当前文件 + 主题）──
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(
                RichText::new("◇")
                    .size(14.0)
                    .color(crate::theme::palette(theme_of(ui)).accent),
            );
            ui.add_space(6.0);
            // 高频动作（注册表投影；ArcGIS QAT：保存/撤销/重做）。
            for id in commands::QAT_COMMANDS {
                let c = commands::find(id).expect("QAT 命令必在注册表");
                if qat_button(ui, cache, c.icon, c.desc_title).clicked() {
                    action = Some(c.action);
                }
            }
            // 撤销/重做（接编辑会话 History；无可撤销/重做置灰）。
            ui.add_enabled_ui(snap.can_undo, |ui| {
                if qat_button(ui, cache, Icon::Reset, "撤销（编辑会话）").clicked() {
                    action = Some(RibbonAction::Undo);
                }
            });
            ui.add_enabled_ui(snap.can_redo, |ui| {
                if qat_button(ui, cache, Icon::Play, "重做（编辑会话）").clicked() {
                    action = Some(RibbonAction::Redo);
                }
            });
            ui.separator();
            ui.label(
                text::caption("堪舆 Kanyu — AI 原生地理空间操作系统")
                    .color(ui.visuals().weak_text_color()),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(10.0);
                let theme_icon = if ui.visuals().dark_mode {
                    Icon::Moon
                } else {
                    Icon::Sun
                };
                if qat_button(
                    ui,
                    cache,
                    theme_icon,
                    "切换界面主题（晨山/夜观星，不影响地图色彩）",
                )
                .clicked()
                {
                    action = Some(RibbonAction::ToggleTheme);
                }
            });
        });
        ui.add_space(2.0);
        ui.separator();

        // ── 页签行（选中项以强调色下划线标识，ArcGIS Pro 风格）──
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            for tab in RibbonTab::ALL {
                let selected = self.active == tab;
                let t = if selected {
                    text::body(tab.label()).strong()
                } else {
                    text::body(tab.label())
                };
                let resp = ui.selectable_label(selected, t);
                // 选中下划线：淡入 + 宽度滑动（egui 原生动画驱动）。
                let line_t = ui.ctx().animate_bool_with_time(
                    resp.id.with("tab_underline"),
                    selected,
                    crate::ui_kit::tokens::animation::HOVER_SECS,
                );
                if line_t > 0.0 {
                    let rect = resp.rect;
                    let y = rect.max.y + 1.0;
                    let half = (rect.width() - 8.0) / 2.0 * line_t;
                    let accent = crate::theme::palette(theme_of(ui)).accent;
                    ui.painter().line_segment(
                        [
                            egui::pos2(rect.center().x - half, y),
                            egui::pos2(rect.center().x + half, y),
                        ],
                        egui::Stroke::new(2.0, accent.gamma_multiply(line_t.clamp(0.0, 1.0))),
                    );
                }
                if resp.clicked() {
                    self.active = tab;
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);

        // ── 命令组行：图标大按钮 + 组分隔 + 组名（在组宽内居中，ArcGIS 式）──
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            for (gi, group) in layout_of(self.active).iter().enumerate() {
                if gi > 0 {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
                // 组宽 = n×68 + (n-1)×2（组名在此宽度内居中）。
                let n = group.commands.len() as f32;
                let group_w = n * 68.0 + (n - 1.0).max(0.0) * 2.0;
                ui.allocate_ui_with_layout(
                    Vec2::new(group_w, 70.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            for (bi, id) in group.commands.iter().enumerate() {
                                if bi > 0 {
                                    ui.add_space(2.0);
                                }
                                let c = commands::find(id).expect("布局命令必在注册表");
                                if ribbon_button(
                                    ui,
                                    cache,
                                    c.icon,
                                    c.title,
                                    c.desc_title,
                                    c.desc_body,
                                    (c.enabled)(snap),
                                )
                                .clicked()
                                {
                                    action = Some(c.action);
                                }
                            }
                        });
                        ui.allocate_ui_with_layout(
                            Vec2::new(group_w, 12.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    text::caption(group.name).color(ui.visuals().weak_text_color()),
                                );
                            },
                        );
                    },
                );
            }
            ui.add_space(10.0);
        });
        action
    }
}

/// QAT 小图标按钮（20px，悬停提示；悬停背景淡入 + 图标动画，与 ribbon_button 同一参数）。
fn qat_button(
    ui: &mut egui::Ui,
    cache: &mut crate::ui_kit::icons::IconCache,
    icon: Icon,
    tip: &str,
) -> egui::Response {
    let p = crate::theme::palette(theme_of(ui));
    let resp = ui.add(
        egui::Button::new("")
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE)
            .corner_radius(crate::ui_kit::tokens::radius::SM)
            .min_size(Vec2::new(22.0, 20.0)),
    );
    let hover_t = ui.ctx().animate_bool_with_time(
        resp.id,
        resp.hovered(),
        crate::ui_kit::tokens::animation::HOVER_SECS,
    );
    if hover_t > 0.0 {
        let bg = crate::ui_kit::tokens::state::hover_bg(&p);
        let bg = egui::Color32::from_rgba_unmultiplied(
            bg.r(),
            bg.g(),
            bg.b(),
            (f32::from(bg.a()) * hover_t) as u8,
        );
        ui.painter()
            .rect_filled(resp.rect, crate::ui_kit::tokens::radius::SM, bg);
    }
    let scale =
        crate::ui_kit::tokens::animation::icon_scale(hover_t, resp.is_pointer_button_down_on());
    let lift = crate::ui_kit::tokens::animation::icon_lift(hover_t);
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(resp.rect.center().x, resp.rect.center().y - lift),
        Vec2::splat(16.0 * scale),
    );
    icons_draw(ui, cache, icon, icon_rect);
    resp.on_hover_text(tip)
}

/// 以文本色绘制图标（QAT 用）。
fn icons_draw(
    ui: &mut egui::Ui,
    cache: &mut crate::ui_kit::icons::IconCache,
    icon: Icon,
    rect: egui::Rect,
) {
    let color = ui.visuals().text_color();
    crate::ui_kit::icons::draw_or_image(ui, cache, icon, rect, color);
}

/// 当前主题（按 visuals 反推）。
fn theme_of(ui: &egui::Ui) -> kanyu_render::Theme {
    if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tab_has_groups_and_buttons() {
        for tab in RibbonTab::ALL {
            let groups = layout_of(tab);
            assert!(!groups.is_empty(), "{tab:?} 无命令组");
            for g in groups {
                assert!(!g.commands.is_empty(), "{tab:?} 组无按钮");
            }
        }
    }

    /// 布局投影完整性：全部 id 在注册表可解析（DAML 投影不脱靶）。
    #[test]
    fn layout_ids_resolve_in_registry() {
        for tab in RibbonTab::ALL {
            for g in layout_of(tab) {
                for id in g.commands {
                    assert!(commands::find(id).is_some(), "未登记的命令 id: {id}");
                }
            }
        }
    }

    #[test]
    fn every_action_reachable_from_some_tab() {
        let actions: Vec<RibbonAction> = RibbonTab::ALL
            .iter()
            .flat_map(|t| layout_of(*t))
            .flat_map(|g| g.commands)
            .map(|id| commands::find(id).unwrap().action)
            .collect();
        for required in [
            RibbonAction::OpenData,
            RibbonAction::OpenProject,
            RibbonAction::SaveProject,
            RibbonAction::CycleMapTheme,
            RibbonAction::BufferDialog,
            RibbonAction::ExportMapDialog,
            RibbonAction::EstateMapDialog,
            RibbonAction::SkillHotload,
            RibbonAction::About,
        ] {
            assert!(actions.contains(&required), "动作不可达: {required:?}");
        }
    }
}
