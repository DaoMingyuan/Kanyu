//! 功能区（Ribbon）：ArcGIS Pro 分类设计 + "图标 + 文字 + 功能介绍"组合按钮
//! （总规 §1.4 线性图标 + 悬停介绍卡）。页签内命令组细分：组内聚、组间分隔。
//!
//! 页签与命令组的划分即总规 §2.2.1 全局菜单的现代化落地：
//! 主页 / 数据 / 分析 / 制图 / 视图 / 技能 / 帮助。

use eframe::egui;
use egui::{RichText, Vec2};

use crate::dock::PanelId;
use crate::ui_kit::icons::Icon;
use crate::ui_kit::{ribbon_button, text};

/// 功能区页签。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RibbonTab {
    /// 主页：工程文件、外观、关于。
    Home,
    /// 数据：图层、查询、交换、投影。
    Data,
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
    pub const ALL: [RibbonTab; 7] = [
        RibbonTab::Home,
        RibbonTab::Data,
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
    // 视图
    /// 缩放到数据范围。
    ZoomToFit,
    /// 复位视图。
    ResetView,
    /// 面板开关（目录/图层/终端/AI 对话；关闭后可经此重开）。
    TogglePanel(PanelId),
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

/// 一个按钮的定义：图标 + 标签 + 动作 + 介绍（标题/正文）。
#[derive(Clone, Copy)]
struct Btn {
    icon: Icon,
    label: &'static str,
    action: RibbonAction,
    desc_title: &'static str,
    desc_body: &'static str,
}

const fn btn(
    icon: Icon,
    label: &'static str,
    action: RibbonAction,
    desc_title: &'static str,
    desc_body: &'static str,
) -> Btn {
    Btn {
        icon,
        label,
        action,
        desc_title,
        desc_body,
    }
}

/// 一个命令组：组名 + 按钮列。
#[derive(Clone)]
struct Group {
    name: &'static str,
    buttons: Vec<Btn>,
}

/// 页签内容（声明式布局——Ribbon 的单一事实来源；按帧构造，量小无忧）。
fn groups_of(tab: RibbonTab) -> Vec<Group> {
    match tab {
        RibbonTab::Home => vec![
            Group {
                name: "工程",
                buttons: vec![
                    btn(Icon::Folder, "打开数据…", RibbonAction::OpenData, "打开地理数据文件", "支持 shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/xlsx/kdb；也可直接拖文件入窗"),
                    btn(Icon::Example, "打开示例", RibbonAction::OpenExample, "加载内置示例", "examples/buildings.geojson（3 建筑点 + 1 道路线）"),
                    btn(Icon::Export, "打开工程…", RibbonAction::OpenProject, "打开堪舆工程 (.kyu)", "恢复图层清单、可见性、视口与地图色彩设置"),
                    btn(Icon::Info, "保存工程", RibbonAction::SaveProject, "保存堪舆工程 (.kyu)", "把当前图层、可见性、视口、地图色彩保存为 .kyu 工程文件"),
                ],
            },
            Group {
                name: "窗口",
                buttons: vec![
                    btn(Icon::Camera, "保存截图", RibbonAction::SaveScreenshot, "窗口截图", "把当前整个窗口保存为 PNG（含 Ribbon 与面板）"),
                    btn(Icon::Sun, "切换主题", RibbonAction::ToggleTheme, "切换界面主题", "晨山 / 夜观星；只改变界面，不改变地图色彩（见「视图 → 地图色彩」）"),
                    btn(Icon::Settings, "设置…", RibbonAction::SettingsDialog, "设置", "工程坐标系选择与渲染设置（独立设置对话框，不占功能区）"),
                ],
            },
            Group {
                name: "关于",
                buttons: vec![btn(Icon::Info, "关于堪舆", RibbonAction::About, "关于堪舆", "版本、架构、许可证")],
            },
        ],
        RibbonTab::Data => vec![
            Group {
                name: "图层",
                buttons: vec![
                    btn(Icon::Info, "图层概要", RibbonAction::LayerInfo, "图层概要", "当前图层的要素数/几何类型/字段（输出到终端）"),
                    btn(Icon::Funnel, "属性查询…", RibbonAction::QueryDialog, "属性查询", "如 height > 50；结果存为新图层"),
                ],
            },
            Group {
                name: "交换",
                buttons: vec![
                    btn(Icon::Export, "导出图层…", RibbonAction::ExportDialog, "导出图层", "geojson/csv/fgb/parquet/dxf/kml/kmz/shp/kdb 全格式互转"),
                    btn(Icon::Compass, "投影变换…", RibbonAction::ReprojectDialog, "投影变换", "EPSG 全库（如 4326 → 3857），结果存为新图层"),
                ],
            },
        ],
        RibbonTab::Analysis => vec![
            Group {
                name: "几何分析",
                buttons: vec![
                    btn(Icon::Buffer, "缓冲区…", RibbonAction::BufferDialog, "缓冲区分析", "按距离生成缓冲区（结果存为新图层；米制请先投影）"),
                    btn(Icon::Overlay, "叠加分析…", RibbonAction::OverlayDialog, "叠加分析", "union/intersection/difference/xor（两个面图层）"),
                    btn(Icon::Topology, "拓扑检查", RibbonAction::Topology, "拓扑检查", "no_overlap 规则（结果输出到终端）"),
                ],
            },
            Group {
                name: "关系统计",
                buttons: vec![
                    btn(Icon::Link, "空间连接…", RibbonAction::SjoinDialog, "空间连接", "按空间谓词合并两图层属性（左连接 + explode）"),
                    btn(Icon::Grid, "分区统计…", RibbonAction::ZonalDialog, "分区统计", "面要素分区统计数值字段（count/sum/mean/min/max）"),
                    btn(Icon::Ruler, "测地度量…", RibbonAction::MeasureDialog, "测地线度量", "Karney 2013 测地线长度/面积（米/平方米）"),
                ],
            },
        ],
        RibbonTab::Cartography => vec![Group {
            name: "地图输出",
            buttons: vec![
                btn(Icon::Export, "导出地图…", RibbonAction::ExportMapDialog, "导出地图", "当前视图导出为 PNG / SVG（尺寸/样式在「设置 → 渲染」，色彩由「视图 → 地图色彩」决定）"),
            ],
        }],
        RibbonTab::View => vec![
            Group {
                name: "相机",
                buttons: vec![
                    btn(Icon::ZoomFit, "缩放到图层", RibbonAction::ZoomToFit, "缩放到图层", "全部可见图层的数据范围"),
                    btn(Icon::Reset, "复位视图", RibbonAction::ResetView, "复位视图", "恢复初始相机"),
                ],
            },
            Group {
                name: "面板",
                buttons: vec![
                    btn(Icon::Folder, "目录", RibbonAction::TogglePanel(PanelId::Catalog), "目录面板", "显示/关闭目录面板（页签可拖动改停靠、拖到画布变浮动窗）"),
                    btn(Icon::Layers, "图层", RibbonAction::TogglePanel(PanelId::Layers), "图层面板", "显示/关闭图层面板（Contents 目录树）"),
                    btn(Icon::Toolbox, "工具箱", RibbonAction::TogglePanel(PanelId::Toolbox), "工具箱面板", "显示/关闭工具箱面板（QGIS Processing 式算法清单）"),
                    btn(Icon::PanelBottom, "终端", RibbonAction::TogglePanel(PanelId::Console), "终端面板", "显示/关闭终端面板（命令直达内核）"),
                    btn(Icon::Chat, "AI 对话", RibbonAction::TogglePanel(PanelId::AiChat), "AI 对话面板", "显示/关闭 AI 对话面板（自然语言驱动分析）"),
                ],
            },
            Group {
                name: "地图色彩",
                buttons: vec![
                    btn(Icon::Sun, "地图色彩", RibbonAction::CycleMapTheme, "地图色彩模式", "固定晨山 → 固定夜观星 → 跟随界面（默认固定晨山，保证制图输出正确）"),
                ],
            },
        ],
        RibbonTab::Skill => vec![Group {
            name: "WASM 技能",
            buttons: vec![
                btn(Icon::Skill, "热加载…", RibbonAction::SkillHotload, "热加载技能", "加载并校验 .wasm 技能（wasmtime 沙箱 + fuel 配额）"),
                btn(Icon::List, "技能清单", RibbonAction::SkillList, "技能清单", "已注册技能（输出到终端）"),
                btn(Icon::Play, "运行技能…", RibbonAction::SkillRunDialog, "运行技能", "在选定图层上执行技能，结果存为新图层"),
            ],
        }],
        RibbonTab::Help => vec![Group {
            name: "文档",
            buttons: vec![
                btn(Icon::Help, "命令速查", RibbonAction::ShowHelp, "命令速查", "终端命令速查（输出到终端）"),
                btn(Icon::Info, "关于堪舆", RibbonAction::About, "关于堪舆", "版本、架构、许可证"),
            ],
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
    /// 返回点击产生的动作（每帧至多一个）。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut crate::ui_kit::icons::IconCache,
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
            // 高频动作（ArcGIS QAT：保存/撤销/重做）。
            if qat_button(ui, cache, Icon::Export, "保存工程 (.kyu)").clicked() {
                action = Some(RibbonAction::SaveProject);
            }
            ui.add_enabled_ui(false, |ui| {
                let _ = qat_button(ui, cache, Icon::Reset, "撤销（待编辑内核落地）");
                let _ = qat_button(ui, cache, Icon::Reset, "重做（待编辑内核落地）");
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
                if qat_button(ui, cache, Icon::Settings, "设置（坐标系 / 渲染）").clicked()
                {
                    action = Some(RibbonAction::SettingsDialog);
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
            for (gi, group) in groups_of(self.active).iter().enumerate() {
                if gi > 0 {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
                // 组宽 = n×68 + (n-1)×2（组名在此宽度内居中）。
                let n = group.buttons.len() as f32;
                let group_w = n * 68.0 + (n - 1.0).max(0.0) * 2.0;
                ui.allocate_ui_with_layout(
                    Vec2::new(group_w, 70.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            for (bi, b) in group.buttons.iter().enumerate() {
                                if bi > 0 {
                                    ui.add_space(2.0);
                                }
                                if ribbon_button(
                                    ui,
                                    cache,
                                    b.icon,
                                    b.label,
                                    b.desc_title,
                                    b.desc_body,
                                    true,
                                )
                                .clicked()
                                {
                                    action = Some(b.action);
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
        let bg = p.hover;
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
            let groups = groups_of(tab);
            assert!(!groups.is_empty(), "{tab:?} 无命令组");
            for g in &groups {
                assert!(!g.buttons.is_empty(), "{tab:?} 组无按钮");
            }
        }
    }

    #[test]
    fn every_action_reachable_from_some_tab() {
        let actions: Vec<RibbonAction> = RibbonTab::ALL
            .iter()
            .flat_map(|t| groups_of(*t))
            .flat_map(|g| g.buttons)
            .map(|b| b.action)
            .collect();
        for required in [
            RibbonAction::OpenData,
            RibbonAction::OpenProject,
            RibbonAction::SaveProject,
            RibbonAction::CycleMapTheme,
            RibbonAction::BufferDialog,
            RibbonAction::ExportMapDialog,
            RibbonAction::SkillHotload,
            RibbonAction::About,
        ] {
            assert!(actions.contains(&required), "动作不可达: {required:?}");
        }
    }
}
