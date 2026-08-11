//! 模态对话框：参数化操作的表单（ArcGIS Pro 地理处理窗格的
//! "参数 → 验证 → 执行"模式）。**全部由 ui_kit 组件组合而成**——
//! 新对话框必须复用 kit 组件，缺失控件先入 kit 再使用。

use eframe::egui;

use crate::ui_kit::{
    combo_static, dialog_shell, error_caption, hint_caption, layer_picker, text_input, DialogAction,
};

/// 对话框集合（任一时刻至多一个活动）。
#[derive(Default)]
pub struct Dialogs {
    /// 属性查询。
    pub query: Option<QueryState>,
    /// 图层导出。
    pub export: Option<ExportState>,
    /// 投影变换。
    pub reproject: Option<ReprojectState>,
    /// 缓冲区。
    pub buffer: Option<BufferState>,
    /// 叠加分析。
    pub overlay: Option<OverlayState>,
    /// 空间连接。
    pub sjoin: Option<SjoinState>,
    /// 分区统计。
    pub zonal: Option<ZonalState>,
    /// 测地度量。
    pub measure: Option<MeasureState>,
    /// 地图导出。
    pub export_map: Option<ExportMapState>,
    /// 运行技能。
    pub skill_run: Option<SkillRunState>,
    /// 关于堪舆。
    pub about: bool,
}

/// 属性查询表单。
#[derive(Default)]
pub struct QueryState {
    /// 图层 id。
    pub layer: String,
    /// 过滤表达式。
    pub expr: String,
}

/// 图层导出表单。
#[derive(Default)]
pub struct ExportState {
    /// 图层 id。
    pub layer: String,
    /// 输出路径。
    pub out: String,
}

/// 投影变换表单。
#[derive(Default)]
pub struct ReprojectState {
    /// 图层 id。
    pub layer: String,
    /// 源 CRS。
    pub from: String,
    /// 目标 CRS。
    pub to: String,
}

/// 缓冲区表单。
#[derive(Default)]
pub struct BufferState {
    /// 图层 id。
    pub layer: String,
    /// 距离（CRS 单位）。
    pub distance: String,
}

/// 叠加分析表单。
#[derive(Default)]
pub struct OverlayState {
    /// 目标图层。
    pub target: String,
    /// 叠加图层。
    pub overlay: String,
    /// 操作。
    pub op: String,
}

/// 空间连接表单。
#[derive(Default)]
pub struct SjoinState {
    /// 目标图层。
    pub target: String,
    /// 连接图层。
    pub join: String,
    /// 谓词。
    pub predicate: String,
}

/// 分区统计表单。
#[derive(Default)]
pub struct ZonalState {
    /// 分区（面）图层。
    pub zones: String,
    /// 值图层。
    pub values: String,
    /// 数值字段。
    pub field: String,
    /// 统计项（逗号分隔）。
    pub stats: String,
}

/// 测地度量表单。
#[derive(Default)]
pub struct MeasureState {
    /// 图层 id。
    pub layer: String,
    /// 类别（length|area）。
    pub kind: String,
}

/// 地图导出表单。
#[derive(Default)]
pub struct ExportMapState {
    /// 输出路径（.png/.svg）。
    pub out: String,
}

/// 运行技能表单。
#[derive(Default)]
pub struct SkillRunState {
    /// 技能 id。
    pub skill_id: String,
    /// 图层 id。
    pub layer: String,
}

/// 对话框执行结果（app 分派执行）。
#[derive(Debug, Clone)]
pub enum DialogResult {
    /// 属性查询。
    Query { layer: String, expr: String },
    /// 图层导出。
    Export {
        layer: String,
        out: String,
        fmt: String,
    },
    /// 投影变换。
    Reproject {
        layer: String,
        from: String,
        to: String,
    },
    /// 缓冲区。
    Buffer { layer: String, distance: f64 },
    /// 叠加分析。
    Overlay {
        target: String,
        overlay: String,
        op: String,
    },
    /// 空间连接。
    Sjoin {
        target: String,
        join: String,
        predicate: String,
    },
    /// 分区统计。
    Zonal {
        zones: String,
        values: String,
        field: String,
        stats: String,
    },
    /// 测地度量。
    Measure { layer: String, kind: String },
    /// 地图导出。
    ExportMap { out: String },
    /// 运行技能。
    SkillRun { skill_id: String, layer: String },
    /// 表单验证失败（红字提示，由 app 反馈到终端）。
    Invalid { reason: String },
}

impl Dialogs {
    /// 对话框 UI。`layers` 为可选图层 id 列表，`skills` 为已注册技能 id 列表。
    /// 返回待执行结果（每帧至多一个）。
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        layers: &[String],
        skills: &[String],
    ) -> Option<DialogResult> {
        let mut result = None;

        if let Some(st) = &mut self.query {
            match dialog_shell(ctx, "属性查询（结果存为新图层）", |ui| {
                layer_picker(ui, "图层", &mut st.layer, layers, true);
                text_input(
                    ui,
                    "表达式",
                    &mut st.expr,
                    "如 height > 50 或 usage == residential",
                    true,
                );
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Query {
                        layer: st.layer.clone(),
                        expr: st.expr.clone(),
                    });
                    self.query = None;
                }
                DialogAction::Cancel => self.query = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.export {
            match dialog_shell(ctx, "导出图层", |ui| {
                layer_picker(ui, "图层", &mut st.layer, layers, true);
                text_input(
                    ui,
                    "输出路径",
                    &mut st.out,
                    "如 out.fgb（格式按扩展名）",
                    true,
                );
            }) {
                DialogAction::Ok => {
                    let fmt = st.out.rsplit('.').next().unwrap_or("").to_string();
                    result = Some(DialogResult::Export {
                        layer: st.layer.clone(),
                        out: st.out.clone(),
                        fmt,
                    });
                    self.export = None;
                }
                DialogAction::Cancel => self.export = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.reproject {
            match dialog_shell(ctx, "投影变换（结果存为新图层）", |ui| {
                layer_picker(ui, "图层", &mut st.layer, layers, true);
                text_input(ui, "源 CRS", &mut st.from, "如 EPSG:4326", true);
                text_input(ui, "目标 CRS", &mut st.to, "如 EPSG:3857", true);
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Reproject {
                        layer: st.layer.clone(),
                        from: st.from.clone(),
                        to: st.to.clone(),
                    });
                    self.reproject = None;
                }
                DialogAction::Cancel => self.reproject = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.buffer {
            let mut err: Option<String> = None;
            match dialog_shell(ctx, "缓冲区分析（结果存为新图层）", |ui| {
                layer_picker(ui, "图层", &mut st.layer, layers, true);
                text_input(ui, "距离", &mut st.distance, "CRS 单位（投影后为米）", true);
                if let Some(e) = &err {
                    error_caption(ui, e);
                } else {
                    hint_caption(ui, "EPSG:4326 下单位为度；米制缓冲请先投影变换");
                }
            }) {
                DialogAction::Ok => match st.distance.trim().parse::<f64>() {
                    Ok(d) if d.is_finite() && d != 0.0 => {
                        result = Some(DialogResult::Buffer {
                            layer: st.layer.clone(),
                            distance: d,
                        });
                        self.buffer = None;
                    }
                    _ => {
                        err = Some(format!("距离须为非零数值: {}", st.distance));
                        result = Some(DialogResult::Invalid {
                            reason: err.clone().unwrap(),
                        });
                    }
                },
                DialogAction::Cancel => self.buffer = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.overlay {
            match dialog_shell(ctx, "叠加分析（结果存为新图层）", |ui| {
                layer_picker(ui, "目标图层", &mut st.target, layers, true);
                layer_picker(ui, "叠加图层", &mut st.overlay, layers, true);
                combo_static(
                    ui,
                    "操作",
                    &mut st.op,
                    &["intersection", "union", "difference", "xor"],
                    true,
                );
                hint_caption(ui, "仅面要素参与；属性取要素对合并");
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Overlay {
                        target: st.target.clone(),
                        overlay: st.overlay.clone(),
                        op: st.op.clone(),
                    });
                    self.overlay = None;
                }
                DialogAction::Cancel => self.overlay = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.sjoin {
            match dialog_shell(ctx, "空间连接（结果存为新图层）", |ui| {
                layer_picker(ui, "目标图层", &mut st.target, layers, true);
                layer_picker(ui, "连接图层", &mut st.join, layers, true);
                combo_static(
                    ui,
                    "谓词",
                    &mut st.predicate,
                    &["intersects", "contains", "within"],
                    true,
                );
                hint_caption(ui, "左连接 + explode；键冲突加 join_ 前缀");
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Sjoin {
                        target: st.target.clone(),
                        join: st.join.clone(),
                        predicate: st.predicate.clone(),
                    });
                    self.sjoin = None;
                }
                DialogAction::Cancel => self.sjoin = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.zonal {
            match dialog_shell(ctx, "分区统计（结果写回分区图层）", |ui| {
                layer_picker(ui, "分区（面）图层", &mut st.zones, layers, true);
                layer_picker(ui, "值图层", &mut st.values, layers, true);
                text_input(ui, "数值字段", &mut st.field, "如 height", true);
                text_input(ui, "统计项", &mut st.stats, "count,sum,mean,min,max", true);
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Zonal {
                        zones: st.zones.clone(),
                        values: st.values.clone(),
                        field: st.field.clone(),
                        stats: st.stats.clone(),
                    });
                    self.zonal = None;
                }
                DialogAction::Cancel => self.zonal = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.measure {
            match dialog_shell(ctx, "测地线度量（结果输出到终端）", |ui| {
                layer_picker(ui, "图层", &mut st.layer, layers, true);
                combo_static(ui, "类别", &mut st.kind, &["length", "area"], true);
                hint_caption(ui, "Karney 2013 测地线算法，米/平方米");
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::Measure {
                        layer: st.layer.clone(),
                        kind: st.kind.clone(),
                    });
                    self.measure = None;
                }
                DialogAction::Cancel => self.measure = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.export_map {
            match dialog_shell(ctx, "导出地图", |ui| {
                text_input(ui, "输出路径", &mut st.out, "如 map.png 或 map.svg", true);
                hint_caption(ui, "范围 = 当前视图；尺寸/样式取「设置 → 渲染」");
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::ExportMap {
                        out: st.out.clone(),
                    });
                    self.export_map = None;
                }
                DialogAction::Cancel => self.export_map = None,
                DialogAction::None => {}
            }
        }

        if let Some(st) = &mut self.skill_run {
            match dialog_shell(ctx, "运行技能（结果存为新图层）", |ui| {
                layer_picker(ui, "技能", &mut st.skill_id, skills, true);
                layer_picker(ui, "图层", &mut st.layer, layers, true);
            }) {
                DialogAction::Ok => {
                    result = Some(DialogResult::SkillRun {
                        skill_id: st.skill_id.clone(),
                        layer: st.layer.clone(),
                    });
                    self.skill_run = None;
                }
                DialogAction::Cancel => self.skill_run = None,
                DialogAction::None => {}
            }
        }

        if self.about {
            let mut open = true;
            egui::Window::new(crate::ui_kit::text::heading("关于堪舆"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(crate::ui_kit::text::display_lg("◇ 堪舆 Kanyu"));
                    ui.label(crate::ui_kit::text::body(format!(
                        "桌面壳层 kanyu-shell v{}",
                        env!("CARGO_PKG_VERSION")
                    )));
                    ui.add_space(4.0);
                    ui.label(crate::ui_kit::text::body(
                        "AI 原生地理空间操作系统：GeoArrow 内核 · MCP 神经接口 · WASM 技能。",
                    ));
                    ui.label(crate::ui_kit::text::body(
                        "以天地为盘，以数据为爻，以 AI 为神。",
                    ));
                    ui.add_space(4.0);
                    ui.hyperlink_to(
                        crate::ui_kit::text::body("github.com/DaoMingyuan/Kanyu"),
                        "https://github.com/DaoMingyuan/Kanyu",
                    );
                    ui.label(crate::ui_kit::text::caption("双许可 MIT OR Apache-2.0"));
                    ui.add_space(6.0);
                    if crate::ui_kit::button(
                        ui,
                        "确 定",
                        crate::ui_kit::ButtonVariant::Primary,
                        true,
                    )
                    .clicked()
                    {
                        self.about = false;
                    }
                });
            if !open {
                self.about = false;
            }
        }

        result
    }
}
