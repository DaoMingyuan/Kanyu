//! 功能区（Ribbon）：ArcGIS Pro 分类设计 + "图标 + 文字 + 功能介绍"组合按钮
//! （总规 §1.4 线性图标 + 悬停介绍卡）。页签内命令组细分：组内聚、组间分隔。
//!
//! 页签与命令组的划分即总规 §2.2.1 全局菜单的现代化落地：
//! 主页 / 数据 / 分析 / 制图 / 视图 / 基因 / 帮助。

use eframe::egui;
use egui::RichText;

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
    /// 基因：WASM 插件。
    Gene,
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
        RibbonTab::Gene,
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
            RibbonTab::Gene => "基因",
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
    /// 渲染设置对话框。
    RenderSettingsDialog,
    /// 地图导出对话框。
    ExportMapDialog,
    // 视图
    /// 缩放到数据范围。
    ZoomToFit,
    /// 复位视图。
    ResetView,
    /// 图层面板显隐。
    ToggleLayersPanel,
    /// 底部停靠区显隐。
    ToggleConsole,
    /// 地图色彩模式循环（固定晨山 → 固定夜观星 → 跟随界面）。
    CycleMapTheme,
    // 基因
    /// 热加载 WASM 基因。
    GeneHotload,
    /// 基因清单（输出到终端）。
    GeneList,
    /// 运行基因对话框。
    GeneRunDialog,
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
                btn(Icon::Image, "渲染设置…", RibbonAction::RenderSettingsDialog, "渲染设置", "输出尺寸与符号化样式（graduated/categorical JSON）"),
                btn(Icon::Export, "导出地图…", RibbonAction::ExportMapDialog, "导出地图", "当前视图导出为 PNG / SVG（色彩由「视图 → 地图色彩」决定，与界面主题无关）"),
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
                    btn(Icon::PanelLeft, "图层面板", RibbonAction::ToggleLayersPanel, "左侧停靠区显隐", "目录 / 图层 双页签"),
                    btn(Icon::PanelBottom, "终端面板", RibbonAction::ToggleConsole, "底部停靠区显隐", "终端 / AI 对话 双页签"),
                ],
            },
            Group {
                name: "地图色彩",
                buttons: vec![
                    btn(Icon::Sun, "地图色彩", RibbonAction::CycleMapTheme, "地图色彩模式", "固定晨山 → 固定夜观星 → 跟随界面（默认固定晨山，保证制图输出正确）"),
                ],
            },
        ],
        RibbonTab::Gene => vec![Group {
            name: "WASM 基因",
            buttons: vec![
                btn(Icon::Gene, "热加载…", RibbonAction::GeneHotload, "热加载基因", "加载并校验 .wasm 基因（wasmtime 沙箱 + fuel 配额）"),
                btn(Icon::List, "基因清单", RibbonAction::GeneList, "基因清单", "已注册基因（输出到终端）"),
                btn(Icon::Play, "运行基因…", RibbonAction::GeneRunDialog, "运行基因", "在选定图层上执行基因，结果存为新图层"),
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
    /// 功能区 UI（页签行 + 命令组行）。返回点击产生的动作（每帧至多一个）。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<RibbonAction> {
        let mut action = None;
        // 页签行：品牌标识 + 页签（选中项以强调色下划线标识，ArcGIS Pro 风格）。
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(RichText::new("◇ 堪舆").strong().size(16.0));
            ui.add_space(14.0);
            for tab in RibbonTab::ALL {
                let selected = self.active == tab;
                let t = if selected {
                    text::body(tab.label()).strong()
                } else {
                    text::body(tab.label())
                };
                let resp = ui.selectable_label(selected, t);
                if selected {
                    let rect = resp.rect;
                    let y = rect.max.y + 1.0;
                    ui.painter().line_segment(
                        [
                            egui::pos2(rect.min.x + 4.0, y),
                            egui::pos2(rect.max.x - 4.0, y),
                        ],
                        egui::Stroke::new(2.0, crate::theme::palette(theme_of(ui)).accent),
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
        // 命令组行：图标大按钮（按钮间距 2px）+ 组分隔（8px + 分隔线 + 8px）+
        // 组名居中于组下方（10px 弱色）；右端留白 10px（边距即呼吸）。
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            for (gi, group) in groups_of(self.active).iter().enumerate() {
                if gi > 0 {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for (bi, b) in group.buttons.iter().enumerate() {
                            if bi > 0 {
                                ui.add_space(2.0);
                            }
                            if ribbon_button(ui, b.icon, b.label, b.desc_title, b.desc_body, true)
                                .clicked()
                            {
                                action = Some(b.action);
                            }
                        }
                    });
                    // 组名：贴组左对齐（不可用居中布局——会横跨整个窗口）。
                    ui.label(text::caption(group.name).color(ui.visuals().weak_text_color()));
                });
            }
            ui.add_space(10.0);
        });
        action
    }
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
            RibbonAction::GeneHotload,
            RibbonAction::About,
        ] {
            assert!(actions.contains(&required), "动作不可达: {required:?}");
        }
    }
}
