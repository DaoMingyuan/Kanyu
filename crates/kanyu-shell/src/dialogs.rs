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
    /// 不动产制图。
    pub estate_map: Option<EstateMapState>,
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

/// 不动产制图九图种（下拉顺序；首项为默认图种）。
pub const ESTATE_MAP_KINDS: [&str; 9] = [
    "宗地图(使用权)",
    "所有权宗地图",
    "宗地草图",
    "房产图",
    "宗海界址图",
    "宗海位置图",
    "宗海平面布置图",
    "用岛范围图",
    "设施布置图",
];

/// 不动产图种（标签见 [`ESTATE_MAP_KINDS`]，单一事实来源；app 按此分派渲染器）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstateMapKind {
    /// 宗地图（土地使用权，图 L.3）。
    ParcelUseRight,
    /// 土地所有权宗地图（图 L.4）。
    ParcelOwnership,
    /// 宗地草图（表 B.4）。
    ParcelSketch,
    /// 房产图（图 L.5）。
    House,
    /// 宗海界址图（图 L.7）。
    SeaBoundary,
    /// 宗海位置图（图 L.6）。
    SeaLocation,
    /// 宗海平面布置图（图 L.8）。
    SeaLayout,
    /// 用岛范围图（图 L.9）。
    IslandRange,
    /// 建筑物和设施布置图（图 L.10）。
    IslandFacility,
}

impl EstateMapKind {
    /// 下拉标签 → 图种（未知标签 None）。
    pub fn from_label(label: &str) -> Option<Self> {
        ESTATE_MAP_KINDS
            .iter()
            .position(|&k| k == label)
            .map(|i| match i {
                0 => Self::ParcelUseRight,
                1 => Self::ParcelOwnership,
                2 => Self::ParcelSketch,
                3 => Self::House,
                4 => Self::SeaBoundary,
                5 => Self::SeaLocation,
                6 => Self::SeaLayout,
                7 => Self::IslandRange,
                _ => Self::IslandFacility,
            })
    }
}

/// 不动产制图表单（九图种出图；输出路径不进表单——确定后弹保存框采集）。
pub struct EstateMapState {
    /// 图种（ESTATE_MAP_KINDS 之一）。
    pub kind: String,
    /// 代码（宗地代码/宗海代码/用岛代码；可空，要素属性自动拾取）。
    pub code: String,
    /// 权利人或项目名（可空，要素属性自动拾取）。
    pub owner: String,
    /// 比例尺分母（空 = 自动整百）。
    pub scale: String,
    /// 分辨率 DPI（默认 150）。
    pub dpi: String,
    /// 丈量者（仅宗地草图显示）。
    pub measurer: String,
    /// 地籍区号（仅所有权宗地图显示）。
    pub cadastral_district: String,
}

impl Default for EstateMapState {
    fn default() -> Self {
        Self {
            kind: ESTATE_MAP_KINDS[0].to_string(),
            code: String::new(),
            owner: String::new(),
            scale: String::new(),
            dpi: "150".to_string(),
            measurer: String::new(),
            cadastral_district: String::new(),
        }
    }
}

impl EstateMapState {
    /// 当前图种是否宗地草图（显示丈量者输入）。
    pub fn is_sketch(&self) -> bool {
        self.kind == ESTATE_MAP_KINDS[2]
    }
    /// 当前图种是否所有权宗地图（显示地籍区号输入）。
    pub fn is_ownership(&self) -> bool {
        self.kind == ESTATE_MAP_KINDS[1]
    }
}

/// 比例尺分母解析：空串 → None（自动整百）；正整数 → Some；其余中文报错。
pub fn parse_scale(text: &str) -> Result<Option<u32>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    match t.parse::<u32>() {
        Ok(n) if n > 0 => Ok(Some(n)),
        _ => Err(format!("比例尺分母须为正整数（留空自动整百）: {t}")),
    }
}

/// 分辨率解析：空串 → 默认 150；正数 → 值；其余中文报错。
pub fn parse_dpi(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(150.0);
    }
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(format!("DPI 须为正数: {t}")),
    }
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
    /// 不动产制图出图（输出路径由保存框采集，不在表单内）。
    EstateMap {
        kind: String,
        code: String,
        owner: String,
        scale: Option<u32>,
        dpi: f64,
        measurer: String,
        cadastral_district: String,
    },
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

        if let Some(st) = &mut self.estate_map {
            let mut err: Option<String> = None;
            match dialog_shell(ctx, "不动产制图出图", |ui| {
                combo_static(ui, "图种", &mut st.kind, &ESTATE_MAP_KINDS, true);
                text_input(
                    ui,
                    "代码",
                    &mut st.code,
                    "宗地代码/宗海代码/用岛代码（可空）",
                    true,
                );
                text_input(ui, "权利人/项目名", &mut st.owner, "可空", true);
                text_input(ui, "比例尺分母", &mut st.scale, "空 = 自动整百", true);
                text_input(ui, "DPI", &mut st.dpi, "默认 150", true);
                if st.is_sketch() {
                    text_input(ui, "丈量者", &mut st.measurer, "宗地草图签注栏", true);
                }
                if st.is_ownership() {
                    text_input(ui, "地籍区号", &mut st.cadastral_district, "DJQDM", true);
                }
                if let Some(e) = &err {
                    error_caption(ui, e);
                } else {
                    hint_caption(
                        ui,
                        "留空字段从要素属性自动拾取（ZDDM/QLRMC/DJQDM/XMMC/ZHDM…）；确定后选择保存路径",
                    );
                }
            }) {
                DialogAction::Ok => match (parse_scale(&st.scale), parse_dpi(&st.dpi)) {
                    (Ok(scale), Ok(dpi)) => {
                        result = Some(DialogResult::EstateMap {
                            kind: st.kind.clone(),
                            code: st.code.trim().to_string(),
                            owner: st.owner.trim().to_string(),
                            scale,
                            dpi,
                            measurer: st.measurer.trim().to_string(),
                            cadastral_district: st.cadastral_district.trim().to_string(),
                        });
                        self.estate_map = None;
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        err = Some(e);
                        result = Some(DialogResult::Invalid {
                            reason: err.clone().unwrap(),
                        });
                    }
                },
                DialogAction::Cancel => self.estate_map = None,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estate_map_parse_scale() {
        // 空串（含纯空白）→ None（自动整百）。
        assert_eq!(parse_scale("").unwrap(), None);
        assert_eq!(parse_scale("   ").unwrap(), None);
        // 正整数（容忍首尾空白）。
        assert_eq!(parse_scale("500").unwrap(), Some(500));
        assert_eq!(parse_scale(" 1000 ").unwrap(), Some(1000));
        // 非数 / 非正 / 小数 → 中文报错。
        assert!(parse_scale("abc").is_err());
        assert!(parse_scale("0").is_err());
        assert!(parse_scale("-100").is_err());
        assert!(parse_scale("12.5").is_err());
    }

    #[test]
    fn estate_map_parse_dpi() {
        assert_eq!(parse_dpi("").unwrap(), 150.0);
        assert_eq!(parse_dpi("  ").unwrap(), 150.0);
        assert_eq!(parse_dpi("300").unwrap(), 300.0);
        assert!(parse_dpi("abc").is_err());
        assert!(parse_dpi("0").is_err());
        assert!(parse_dpi("-1").is_err());
    }

    #[test]
    fn estate_map_state_defaults() {
        let st = EstateMapState::default();
        assert_eq!(st.kind, ESTATE_MAP_KINDS[0]);
        assert_eq!(st.dpi, "150");
        assert!(!st.is_sketch());
        assert!(!st.is_ownership());
        let sketch = EstateMapState {
            kind: ESTATE_MAP_KINDS[2].to_string(),
            ..Default::default()
        };
        assert!(sketch.is_sketch());
        let ownership = EstateMapState {
            kind: ESTATE_MAP_KINDS[1].to_string(),
            ..Default::default()
        };
        assert!(ownership.is_ownership());
    }

    #[test]
    fn estate_map_kinds_nine() {
        assert_eq!(ESTATE_MAP_KINDS.len(), 9);
        assert!(ESTATE_MAP_KINDS.iter().all(|k| !k.is_empty()));
        // 标签 ↔ 图种枚举全量往返（改名不失联）。
        for label in ESTATE_MAP_KINDS {
            assert!(
                EstateMapKind::from_label(label).is_some(),
                "未映射: {label}"
            );
        }
        assert!(EstateMapKind::from_label("不存在的图种").is_none());
        assert_eq!(
            EstateMapKind::from_label(ESTATE_MAP_KINDS[0]),
            Some(EstateMapKind::ParcelUseRight)
        );
        assert_eq!(
            EstateMapKind::from_label(ESTATE_MAP_KINDS[8]),
            Some(EstateMapKind::IslandFacility)
        );
    }
}
