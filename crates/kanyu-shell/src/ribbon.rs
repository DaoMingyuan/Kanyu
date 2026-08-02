//! 功能区（Ribbon）：借鉴 ArcGIS Pro 的分类设计——页签（Tab）组织命令组，
//! 每组内聚相关操作，按钮即动作。比传统菜单发现性强，比工具栏组织性强。
//!
//! 页签与命令组的划分即总规 §2.2.1 全局菜单（文件/编辑/视图/图层/分析/AI/帮助）
//! 的现代化落地：主页 / 数据 / 分析 / 制图 / 视图 / 基因 / 帮助。

use eframe::egui;
use egui::{RichText, Vec2};

/// 功能区页签。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RibbonTab {
    /// 主页：文件、主题、关于。
    Home,
    /// 数据：加载、概要、查询、导出、投影。
    Data,
    /// 分析：缓冲、叠加、拓扑、连接、统计、度量。
    Analysis,
    /// 制图：渲染设置、地图导出。
    Cartography,
    /// 视图：缩放、面板显隐。
    View,
    /// 基因：WASM 插件热加载与运行。
    Gene,
    /// 帮助：命令速查、关于。
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
    /// 加载内置示例（examples/buildings.geojson）。
    OpenExample,
    /// 保存当前画布为 PNG。
    SaveScreenshot,
    /// 切换晨山/夜观星。
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
    /// 拓扑检查（当前图层，输出到终端）。
    Topology,
    /// 空间连接对话框。
    SjoinDialog,
    /// 分区统计对话框。
    ZonalDialog,
    /// 测地线度量对话框。
    MeasureDialog,
    // 制图
    /// 渲染设置对话框（尺寸/样式 JSON）。
    RenderSettingsDialog,
    /// 地图导出对话框（PNG/SVG）。
    ExportMapDialog,
    // 视图
    /// 缩放到数据范围。
    ZoomToFit,
    /// 复位视图（清空图层视口状态并重适配）。
    ResetView,
    /// 图层面板显隐。
    ToggleLayersPanel,
    /// 终端面板显隐。
    ToggleConsole,
    /// 属性/基因面板显隐。
    TogglePropsPanel,
    // 基因
    /// 热加载 WASM 基因（文件对话框）。
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

/// 一个按钮的定义：标签 + 动作 + 悬停提示。
#[derive(Clone)]
struct Btn {
    label: &'static str,
    action: RibbonAction,
    tip: &'static str,
}

const fn btn(label: &'static str, action: RibbonAction, tip: &'static str) -> Btn {
    Btn { label, action, tip }
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
                name: "文件",
                buttons: vec![
                    btn(
                        "打开数据…",
                        RibbonAction::OpenData,
                        "打开地理数据文件（支持 shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/xlsx）",
                    ),
                    btn(
                        "打开示例",
                        RibbonAction::OpenExample,
                        "加载内置示例 buildings.geojson",
                    ),
                    btn(
                        "保存截图",
                        RibbonAction::SaveScreenshot,
                        "把当前窗口保存为 PNG 截图",
                    ),
                ],
            },
            Group {
                name: "外观",
                buttons: vec![btn("切换主题", RibbonAction::ToggleTheme, "晨山 / 夜观星")],
            },
            Group {
                name: "关于",
                buttons: vec![btn("关于堪舆", RibbonAction::About, "版本、架构、许可证")],
            },
        ],
        RibbonTab::Data => vec![
            Group {
                name: "图层",
                buttons: vec![
                    btn(
                        "图层概要",
                        RibbonAction::LayerInfo,
                        "当前图层的要素数/几何类型/字段（输出到终端）",
                    ),
                    btn(
                        "属性查询…",
                        RibbonAction::QueryDialog,
                        "如 height > 50；结果存为新图层",
                    ),
                ],
            },
            Group {
                name: "交换",
                buttons: vec![
                    btn(
                        "导出图层…",
                        RibbonAction::ExportDialog,
                        "导出为 geojson/csv/fgb/parquet/dxf/kml/kmz/shp",
                    ),
                    btn(
                        "投影变换…",
                        RibbonAction::ReprojectDialog,
                        "EPSG 全库（如 4326 → 3857），结果存为新图层",
                    ),
                ],
            },
        ],
        RibbonTab::Analysis => vec![
            Group {
                name: "几何分析",
                buttons: vec![
                    btn(
                        "缓冲区…",
                        RibbonAction::BufferDialog,
                        "按距离生成缓冲区（结果存为新图层）",
                    ),
                    btn(
                        "叠加分析…",
                        RibbonAction::OverlayDialog,
                        "union/intersection/difference/xor（两图层）",
                    ),
                    btn(
                        "拓扑检查",
                        RibbonAction::Topology,
                        "no_overlap 规则（结果输出到终端）",
                    ),
                ],
            },
            Group {
                name: "关系统计",
                buttons: vec![
                    btn(
                        "空间连接…",
                        RibbonAction::SjoinDialog,
                        "按空间谓词合并两图层属性",
                    ),
                    btn(
                        "分区统计…",
                        RibbonAction::ZonalDialog,
                        "面要素分区统计数值字段",
                    ),
                    btn(
                        "测地度量…",
                        RibbonAction::MeasureDialog,
                        "测地线长度/面积（米/平方米）",
                    ),
                ],
            },
        ],
        RibbonTab::Cartography => vec![Group {
            name: "地图输出",
            buttons: vec![
                btn(
                    "渲染设置…",
                    RibbonAction::RenderSettingsDialog,
                    "输出尺寸与符号化样式（graduated/categorical JSON）",
                ),
                btn(
                    "导出地图…",
                    RibbonAction::ExportMapDialog,
                    "当前视图导出为 PNG / SVG",
                ),
            ],
        }],
        RibbonTab::View => vec![
            Group {
                name: "相机",
                buttons: vec![
                    btn(
                        "缩放到图层",
                        RibbonAction::ZoomToFit,
                        "全部可见图层的数据范围",
                    ),
                    btn("复位视图", RibbonAction::ResetView, "恢复初始相机"),
                ],
            },
            Group {
                name: "面板",
                buttons: vec![
                    btn(
                        "图层面板",
                        RibbonAction::ToggleLayersPanel,
                        "显示/隐藏左侧图层面板",
                    ),
                    btn(
                        "终端面板",
                        RibbonAction::ToggleConsole,
                        "显示/隐藏底部独立终端",
                    ),
                    btn(
                        "属性面板",
                        RibbonAction::TogglePropsPanel,
                        "显示/隐藏右侧属性/基因面板",
                    ),
                ],
            },
        ],
        RibbonTab::Gene => vec![Group {
            name: "WASM 基因",
            buttons: vec![
                btn(
                    "热加载…",
                    RibbonAction::GeneHotload,
                    "加载并校验 .wasm 基因（wasmtime 沙箱）",
                ),
                btn(
                    "基因清单",
                    RibbonAction::GeneList,
                    "已注册基因（输出到终端）",
                ),
                btn(
                    "运行基因…",
                    RibbonAction::GeneRunDialog,
                    "在选定图层上执行基因，结果存为新图层",
                ),
            ],
        }],
        RibbonTab::Help => vec![Group {
            name: "文档",
            buttons: vec![
                btn(
                    "命令速查",
                    RibbonAction::ShowHelp,
                    "终端命令速查（输出到终端）",
                ),
                btn("关于堪舆", RibbonAction::About, "版本、架构、许可证"),
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
                let text = if selected {
                    RichText::new(tab.label()).strong()
                } else {
                    RichText::new(tab.label())
                };
                let resp = ui.selectable_label(selected, text);
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
            }
        });
        ui.separator();
        // 命令组行：按钮 + 组名（按钮在上、组名在下，ArcGIS  ribbon 的组结构）。
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let groups = groups_of(self.active);
            for (gi, group) in groups.iter().enumerate() {
                if gi > 0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        for b in &group.buttons {
                            let resp = ui
                                .add(egui::Button::new(b.label).min_size(Vec2::new(0.0, 26.0)))
                                .on_hover_text(b.tip);
                            if resp.clicked() {
                                action = Some(b.action);
                            }
                        }
                    });
                    ui.add_space(1.0);
                    ui.label(
                        RichText::new(group.name)
                            .size(10.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            }
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
            for g in groups {
                assert!(!g.buttons.is_empty(), "{tab:?}/{:?} 组无按钮", g.name);
            }
        }
    }

    #[test]
    fn every_action_reachable_from_some_tab() {
        // 关键动作必须可从功能区触发（防声明遗漏）。
        let actions: Vec<RibbonAction> = RibbonTab::ALL
            .iter()
            .flat_map(|t| groups_of(*t))
            .flat_map(|g| g.buttons)
            .map(|b| b.action)
            .collect();
        for required in [
            RibbonAction::OpenData,
            RibbonAction::BufferDialog,
            RibbonAction::OverlayDialog,
            RibbonAction::ExportMapDialog,
            RibbonAction::GeneHotload,
            RibbonAction::ToggleConsole,
            RibbonAction::About,
        ] {
            assert!(actions.contains(&required), "动作不可达: {required:?}");
        }
    }
}
