//! 属性表面板（ArcGIS Pro 表视图范式，简化版）：单表 + 图层切换。
//!
//! - 表格：列 = 字段（首列 # 行号冻结），行 = 要素；固定行高 + 手动窗口化
//!   虚拟滚动（只绘制可视区行，万级要素不卡）；列头点击排序（升/降/原序
//!   三态循环），右键列头菜单（重命名/删除）；顶部筛选框（任意字段子串，
//!   大小写不敏感）；单元格只读（编辑留后续）。
//! - 工具条：添加字段… / 删除字段 / 重命名字段… / 字段计算器…（含前 5 行预览）。
//! - 写回：操作产出 [`AttrAction`]，由 app 经 `kanyu_core::attrcalc` 应用并重建图层。

use eframe::egui;
use geojson::FeatureCollection;
use serde_json::{Map, Value as Json};

use crate::ui_kit::tokens::{sizes, spacing, text};
use crate::ui_kit::{
    button, combo_static, dialog_shell, error_caption, text_input, ButtonVariant, DialogAction,
};

/// 列宽（规范常量）。
const COL_W: f32 = 110.0;
/// 行号列宽。
const IDX_W: f32 = 44.0;

/// 属性表动作（app 结算）。
pub enum AttrAction {
    /// 对图层应用字段操作。
    Apply { layer: String, op: FieldOp },
    /// 单元格编辑提交（文本，类型由 app 按可解析性定）。
    EditCell {
        layer: String,
        feature: usize,
        field: String,
        text: String,
    },
}

/// 字段操作（与 attrcalc API 一一对应）。
pub enum FieldOp {
    /// 添加字段。
    Add { name: String, default: Option<Json> },
    /// 删除字段。
    Delete { name: String },
    /// 重命名字段。
    Rename { old: String, new: String },
    /// 字段计算（写入/新建目标字段）。
    Calc { target: String, expr: String },
}

// ===== 纯逻辑（可测）=====

/// 字段名清单（按首次出现序，去重）。
pub fn field_names(collection: &FeatureCollection) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in &collection.features {
        if let Some(p) = &f.properties {
            for k in p.keys() {
                if !out.contains(k) {
                    out.push(k.clone());
                }
            }
        }
    }
    out
}

/// 单元格显示文本。
pub fn cell_text(v: Option<&Json>) -> String {
    match v {
        None | Some(Json::Null) => String::new(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n.to_string(),
        Some(Json::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// 排序比较：数值按数值、其余按字符串；Null 恒排最后。
pub fn compare_cells(a: Option<&Json>, b: Option<&Json>) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (None, None) => return Ordering::Equal,
        (None, Some(Json::Null)) => return Ordering::Equal,
        (Some(Json::Null), None) => return Ordering::Equal,
        (None, _) | (Some(Json::Null), _) => return Ordering::Greater,
        (_, None) | (_, Some(Json::Null)) => return Ordering::Less,
        _ => {}
    }
    let (a, b) = (a.unwrap(), b.unwrap());
    match (a, b) {
        (Json::Number(x), Json::Number(y)) => x
            .as_f64()
            .partial_cmp(&y.as_f64())
            .unwrap_or(Ordering::Equal),
        _ => cell_text(Some(a)).cmp(&cell_text(Some(b))),
    }
}

/// 排序后的行下标（asc=true 升序）。
pub fn sorted_indices(collection: &FeatureCollection, field: &str, asc: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..collection.features.len()).collect();
    idx.sort_by(|&i, &j| {
        let pi = collection.features[i]
            .properties
            .as_ref()
            .and_then(|p| p.get(field));
        let pj = collection.features[j]
            .properties
            .as_ref()
            .and_then(|p| p.get(field));
        let o = compare_cells(pi, pj);
        if asc {
            o
        } else {
            o.reverse()
        }
    });
    idx
}

/// 行筛选：任意字段值包含子串（大小写不敏感；空串恒真）。
pub fn row_matches(props: Option<&Map<String, Json>>, filter_lc: &str) -> bool {
    if filter_lc.is_empty() {
        return true;
    }
    let Some(p) = props else {
        return false;
    };
    p.values()
        .any(|v| cell_text(Some(v)).to_lowercase().contains(filter_lc))
}

/// 字段类型推断（图层属性对话框；扫前 200 要素，首个非空值定类型）。
pub fn infer_field_types(collection: &FeatureCollection) -> Vec<(String, String)> {
    let names = field_names(collection);
    let mut out = Vec::new();
    for name in names {
        let mut kind = "未知";
        for f in collection.features.iter().take(200) {
            if let Some(v) = f.properties.as_ref().and_then(|p| p.get(&name)) {
                match v {
                    Json::Null => continue,
                    Json::Number(_) => {
                        kind = "数值";
                        break;
                    }
                    Json::String(_) => {
                        kind = "文本";
                        break;
                    }
                    Json::Bool(_) => {
                        kind = "布尔";
                        break;
                    }
                    _ => {
                        kind = "其他";
                        break;
                    }
                }
            }
        }
        out.push((name, kind.to_string()));
    }
    out
}

// ===== 面板 =====

/// 添加字段对话框状态。
#[derive(Default)]
struct AddFieldState {
    name: String,
    kind: String,
    default: String,
    /// 校验错误（保留在状态里跨帧展示）。
    err: Option<String>,
}

/// 单元格编辑态（双击进入；Enter 提交 / Esc 取消）。
struct CellEditState {
    /// 要素原下标。
    feature: usize,
    /// 字段名。
    field: String,
    /// 编辑中文本。
    text: String,
    /// 首帧聚焦标记。
    focused: bool,
}

/// 字段计算器对话框状态。
#[derive(Default)]
struct CalcState {
    target: String,
    expr: String,
    /// 预览结果（前 5 行求值；Err 为中文错误）。
    preview: Option<Result<Vec<String>, String>>,
}

/// 属性表面板状态。
#[derive(Default)]
pub struct AttrTablePanel {
    /// 当前服务图层 id。
    layer: Option<String>,
    /// 筛选框。
    filter: String,
    /// 排序（字段, 升序）；None = 原序。
    sort: Option<(String, bool)>,
    /// 选中列（删除/重命名目标）。
    sel_col: Option<String>,
    add_dlg: Option<AddFieldState>,
    rename_dlg: Option<(String, String)>,
    calc_dlg: Option<CalcState>,
    /// 单元格编辑态。
    cell_edit: Option<CellEditState>,
}

impl AttrTablePanel {
    /// 打开并选中图层（右键「打开属性表」）。
    pub fn set_layer(&mut self, id: String) {
        if self.layer.as_deref() != Some(id.as_str()) {
            self.layer = Some(id);
            self.sort = None;
            self.sel_col = None;
        }
    }

    /// 当前图层 id。
    pub fn layer(&self) -> Option<&str> {
        self.layer.as_deref()
    }

    /// 演示/验证（--calc-demo）：打开字段计算器（预填示例，含预览结果）。
    pub fn demo_open_calc(&mut self, preview: Result<Vec<String>, String>) {
        self.calc_dlg = Some(CalcState {
            target: "楼高米".to_string(),
            expr: "height * 3.5".to_string(),
            preview: Some(preview),
        });
    }

    /// 面板 UI。`layers` = (id, 显示名)；`collection` = 当前图层要素集。
    /// 返回待应用动作。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        layers: &[(String, String)],
        collection: Option<&FeatureCollection>,
    ) -> Vec<AttrAction> {
        let mut actions = Vec::new();
        let has_data = collection.is_some();

        // 工具条：图层切换 | 添加/删除/重命名/字段计算器 | 筛选。
        // horizontal_wrapped：窄停靠区（200px 下限）内按钮换行而不溢出。
        ui.horizontal_wrapped(|ui| {
            let mut cur = self.layer.clone().unwrap_or_default();
            let ids: Vec<String> = layers.iter().map(|(id, _)| id.clone()).collect();
            crate::ui_kit::combo_width(ui, "图层", &mut cur, &ids, 120.0, !ids.is_empty());
            if !cur.is_empty() && self.layer.as_deref() != Some(cur.as_str()) {
                self.set_layer(cur);
            }
            ui.separator();
            if crate::ui_kit::icon_button(ui, "＋", "添加字段…", has_data).clicked() {
                self.add_dlg = Some(AddFieldState::default());
            }
            if crate::ui_kit::icon_button(
                ui,
                "－",
                "删除字段（先点列头选列）",
                has_data && self.sel_col.is_some(),
            )
            .clicked()
            {
                if let Some(c) = self.sel_col.clone() {
                    if let Some(layer) = self.layer.clone() {
                        actions.push(AttrAction::Apply {
                            layer,
                            op: FieldOp::Delete { name: c },
                        });
                    }
                }
            }
            if crate::ui_kit::icon_button(
                ui,
                "✎",
                "重命名字段…（先点列头选列）",
                has_data && self.sel_col.is_some(),
            )
            .clicked()
            {
                if let Some(c) = self.sel_col.clone() {
                    self.rename_dlg = Some((c, String::new()));
                }
            }
            if crate::ui_kit::icon_button(ui, "ƒx", "字段计算器…", has_data).clicked() {
                self.calc_dlg = Some(CalcState::default());
            }
            ui.separator();
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(f32::INFINITY)
                    .hint_text("筛选…"),
            );
        });
        ui.separator();

        // 表格。
        if let Some(c) = collection {
            self.table(ui, c, &mut actions);
        } else {
            crate::ui_kit::hint_caption(ui, "未选择图层：经图层右键「打开属性表」或上方下拉选择");
        }

        // 对话框。
        self.dialogs(ui.ctx(), collection, &mut actions);
        actions
    }

    /// 表格（手动窗口化虚拟滚动）。
    fn table(&mut self, ui: &mut egui::Ui, c: &FeatureCollection, actions: &mut Vec<AttrAction>) {
        let fields = field_names(c);
        let filter_lc = self.filter.trim().to_lowercase();
        // 行序：筛选 → 排序。
        let mut rows: Vec<usize> = (0..c.features.len())
            .filter(|&i| row_matches(c.features[i].properties.as_ref(), &filter_lc))
            .collect();
        if let Some((f, asc)) = &self.sort {
            let order = sorted_indices(c, f, *asc);
            rows = order.into_iter().filter(|i| rows.contains(i)).collect();
        }

        let p = crate::theme::palette(if ui.visuals().dark_mode {
            kanyu_render::Theme::Dark
        } else {
            kanyu_render::Theme::Light
        });
        let row_h = sizes::CONTROL_SM;
        let total = egui::Vec2::new(
            IDX_W + fields.len() as f32 * COL_W,
            row_h * (rows.len() as f32 + 1.0), // + 表头
        );
        // 双向滚动 + 不收缩（表格始终填满停靠区，少行时不塌陷）。
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (content, _) = ui.allocate_exact_size(total, egui::Sense::hover());
                let clip = ui.clip_rect().intersect(content);
                let painter = ui.painter().clone();
                // 表头背景。
                painter.rect_filled(
                    egui::Rect::from_min_size(content.min, egui::Vec2::new(total.x, row_h)),
                    0.0,
                    p.bg_tertiary,
                );
                // 行号列表头。
                painter.text(
                    egui::pos2(content.min.x + IDX_W / 2.0, content.min.y + row_h / 2.0),
                    egui::Align2::CENTER_CENTER,
                    "#",
                    egui::FontId::proportional(text::SIZE_CAPTION),
                    p.text_weak,
                );
                // 列头（点击排序三态；右键菜单 重命名/删除）。
                for (ci, name) in fields.iter().enumerate() {
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(content.min.x + IDX_W + ci as f32 * COL_W, content.min.y),
                        egui::Vec2::new(COL_W, row_h),
                    );
                    let resp =
                        ui.interact(rect, ui.id().with(("attr_col", ci)), egui::Sense::click());
                    let sorted = matches!(&self.sort, Some((f, _)) if f == name);
                    let arrow = match &self.sort {
                        Some((f, asc)) if f == name => {
                            if *asc {
                                " ▲"
                            } else {
                                " ▼"
                            }
                        }
                        _ => "",
                    };
                    let selected = self.sel_col.as_deref() == Some(name.as_str());
                    if selected {
                        painter.rect_filled(rect, 0.0, p.selection);
                    } else if resp.hovered() {
                        painter.rect_filled(rect, 0.0, p.hover);
                    }
                    painter.with_clip_rect(rect).text(
                        egui::pos2(rect.min.x + spacing::XS, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("{name}{arrow}"),
                        egui::FontId::proportional(text::SIZE_BODY),
                        if sorted || selected {
                            p.accent
                        } else {
                            p.text_primary
                        },
                    );
                    if resp.clicked() {
                        // 三态循环：原序 → 升 → 降 → 原序；同时选中该列。
                        self.sel_col = Some(name.clone());
                        self.sort = match &self.sort {
                            Some((f, true)) if f == name => Some((name.clone(), false)),
                            Some((f, false)) if f == name => None,
                            _ => Some((name.clone(), true)),
                        };
                    }
                    resp.context_menu(|ui| {
                        if ui.button("重命名字段…").clicked() {
                            self.rename_dlg = Some((name.clone(), String::new()));
                            ui.close();
                        }
                        if ui.button("删除字段").clicked() {
                            if let Some(layer) = self.layer.clone() {
                                actions.push(AttrAction::Apply {
                                    layer,
                                    op: FieldOp::Delete { name: name.clone() },
                                });
                            }
                            ui.close();
                        }
                    });
                }
                // 可视行范围（窗口化）。
                let first = (((clip.min.y - content.min.y) / row_h).floor().max(0.0) as usize)
                    .saturating_sub(1);
                let last =
                    (((clip.max.y - content.min.y) / row_h).ceil() as usize + 1).min(rows.len());
                for (vi, &fi) in rows.iter().enumerate().take(last).skip(first) {
                    let y = content.min.y + row_h * (vi as f32 + 1.0);
                    let row_rect = egui::Rect::from_min_size(
                        egui::pos2(content.min.x, y),
                        egui::Vec2::new(total.x, row_h),
                    );
                    if vi % 2 == 1 {
                        painter.rect_filled(row_rect, 0.0, p.bg_secondary);
                    }
                    // 行号（要素原序号）。
                    painter.text(
                        egui::pos2(row_rect.min.x + IDX_W / 2.0, row_rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        fi.to_string(),
                        egui::FontId::proportional(text::SIZE_CAPTION),
                        p.text_weak,
                    );
                    let props = c.features[fi].properties.as_ref();
                    for (ci, name) in fields.iter().enumerate() {
                        let cell = egui::Rect::from_min_size(
                            egui::pos2(row_rect.min.x + IDX_W + ci as f32 * COL_W, y),
                            egui::Vec2::new(COL_W, row_h),
                        );
                        let editing_this = self
                            .cell_edit
                            .as_ref()
                            .is_some_and(|s| s.feature == fi && s.field == *name);
                        if editing_this {
                            // 编辑态：内嵌文本框（Enter 提交 / Esc 取消）。
                            let Some(st) = &mut self.cell_edit else {
                                continue;
                            };
                            let resp = ui.put(
                                cell,
                                egui::TextEdit::singleline(&mut st.text)
                                    .font(egui::FontId::proportional(text::SIZE_BODY)),
                            );
                            if !st.focused {
                                resp.request_focus();
                                st.focused = true;
                            }
                            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                            if enter {
                                if let Some(layer) = self.layer.clone() {
                                    actions.push(AttrAction::EditCell {
                                        layer,
                                        feature: st.feature,
                                        field: st.field.clone(),
                                        text: st.text.clone(),
                                    });
                                }
                                self.cell_edit = None;
                            } else if esc {
                                self.cell_edit = None;
                            }
                            continue;
                        }
                        let v = props.and_then(|pr| pr.get(name));
                        painter.with_clip_rect(cell).text(
                            egui::pos2(cell.min.x + spacing::XS, cell.center().y),
                            egui::Align2::LEFT_CENTER,
                            cell_text(v),
                            egui::FontId::proportional(text::SIZE_BODY),
                            p.text_primary,
                        );
                        // 双击进入编辑（文本框预填当前值）。
                        let resp = ui.interact(
                            cell,
                            ui.id().with(("attr_cell", fi, ci)),
                            egui::Sense::click(),
                        );
                        if resp.double_clicked() {
                            self.cell_edit = Some(CellEditState {
                                feature: fi,
                                field: name.clone(),
                                text: cell_text(v),
                                focused: false,
                            });
                        }
                    }
                }
            });
    }

    /// 三个字段对话框（添加/重命名/计算器）。
    fn dialogs(
        &mut self,
        ctx: &egui::Context,
        collection: Option<&FeatureCollection>,
        actions: &mut Vec<AttrAction>,
    ) {
        let Some(layer) = self.layer.clone() else {
            return;
        };
        // 添加字段。
        if let Some(mut st) = self.add_dlg.take() {
            let action = dialog_shell(ctx, "添加字段", |ui| {
                text_input(ui, "字段名", &mut st.name, "如 容积率", true);
                combo_static(ui, "类型", &mut st.kind, &["文本", "数值", "布尔"], true);
                text_input(ui, "默认值", &mut st.default, "可空 = null", true);
                if let Some(e) = &st.err {
                    error_caption(ui, e);
                }
            });
            match action {
                DialogAction::Ok => {
                    st.err = None;
                    let name = st.name.trim().to_string();
                    if name.is_empty() {
                        st.err = Some("字段名不能为空".to_string());
                    }
                    let default = if st.default.trim().is_empty() {
                        None
                    } else {
                        match st.kind.as_str() {
                            "数值" => match st.default.trim().parse::<f64>() {
                                Ok(v) => serde_json::Number::from_f64(v).map(Json::from),
                                Err(_) => {
                                    st.err = Some(format!("默认值须为数值: {}", st.default));
                                    None
                                }
                            },
                            "布尔" => match st.default.trim() {
                                "true" | "是" => Some(Json::from(true)),
                                "false" | "否" => Some(Json::from(false)),
                                other => {
                                    st.err = Some(format!("默认值须为 是/否: {other}"));
                                    None
                                }
                            },
                            _ => Some(Json::from(st.default.clone())),
                        }
                    };
                    if st.err.is_some() {
                        self.add_dlg = Some(st); // 校验失败：留框红字，不吞输入
                    } else {
                        actions.push(AttrAction::Apply {
                            layer: layer.clone(),
                            op: FieldOp::Add { name, default },
                        });
                    }
                }
                DialogAction::Cancel => {}
                DialogAction::None => self.add_dlg = Some(st),
            }
        }
        // 重命名字段。
        if let Some((old, new_name)) = &mut self.rename_dlg {
            let action = dialog_shell(ctx, "重命名字段", |ui| {
                text_input(ui, "新名称", new_name, &format!("原字段: {old}"), true);
            });
            match action {
                DialogAction::Ok => {
                    let new = new_name.trim().to_string();
                    if !new.is_empty() {
                        actions.push(AttrAction::Apply {
                            layer: layer.clone(),
                            op: FieldOp::Rename {
                                old: old.clone(),
                                new,
                            },
                        });
                    }
                    self.rename_dlg = None;
                }
                DialogAction::Cancel => self.rename_dlg = None,
                DialogAction::None => {}
            }
        }
        // 字段计算器（含前 5 行预览）。
        if let Some(mut st) = self.calc_dlg.take() {
            let mut do_preview = false;
            let mut do_apply = false;
            let mut closed = false;
            egui::Window::new(text::heading("字段计算器"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    text_input(ui, "目标字段", &mut st.target, "不存在则新建，存在则覆盖", true);
                    text_input(ui, "表达式", &mut st.expr, "如 [建筑 高度] * 2 或 $area / 10000", true);
                    crate::ui_kit::hint_caption(
                        ui,
                        "支持 + - * / %、比较、and/or/not、函数（round/upper/concat/coalesce…）与 $area/$length/$x/$y",
                    );
                    // 预览结果。
                    if let Some(preview) = &st.preview {
                        ui.separator();
                        match preview {
                            Ok(vals) => {
                                crate::ui_kit::hint_caption(ui, "预览（前 5 行）：");
                                for v in vals {
                                    ui.label(text::data(v));
                                }
                            }
                            Err(e) => error_caption(ui, e),
                        }
                    }
                    ui.add_space(spacing::SM);
                    ui.horizontal(|ui| {
                        if button(ui, "预 览", ButtonVariant::Secondary, true).clicked() {
                            do_preview = true;
                        }
                        if button(ui, "应 用", ButtonVariant::Primary, true).clicked() {
                            do_apply = true;
                        }
                        if button(ui, "取 消", ButtonVariant::Secondary, true).clicked() {
                            closed = true;
                        }
                    });
                });
            if do_preview {
                st.preview = Some(preview_calc(collection, &st.target, &st.expr, 5));
            }
            if do_apply {
                let target = st.target.trim().to_string();
                if !target.is_empty() && !st.expr.trim().is_empty() {
                    actions.push(AttrAction::Apply {
                        layer: layer.clone(),
                        op: FieldOp::Calc {
                            target,
                            expr: st.expr.clone(),
                        },
                    });
                    closed = true;
                } else {
                    st.preview = Some(Err("目标字段与表达式均不能为空".to_string()));
                }
            }
            if !closed {
                self.calc_dlg = Some(st);
            }
        }
    }
}

/// 字段计算预览（前 limit 行求值；纯逻辑）。
pub fn preview_calc(
    collection: Option<&FeatureCollection>,
    target: &str,
    expr: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let c = collection.ok_or("未选择图层")?;
    if target.trim().is_empty() {
        return Err("目标字段不能为空".to_string());
    }
    let sub = FeatureCollection {
        bbox: None,
        features: c.features.iter().take(limit).cloned().collect(),
        foreign_members: None,
    };
    let out = kanyu_core::attrcalc::calc_field(&sub, "__预览", expr).map_err(|e| e.to_string())?;
    Ok(out
        .features
        .iter()
        .map(|f| {
            f.properties
                .as_ref()
                .and_then(|p| p.get("__预览"))
                .map(|v| cell_text(Some(v)))
                .unwrap_or_default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::Feature;

    fn feat(props: &[(&str, Json)]) -> Feature {
        Feature {
            bbox: None,
            geometry: None,
            id: None,
            properties: Some(
                props
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            ),
            foreign_members: None,
        }
    }

    fn coll() -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: vec![
                feat(&[("name", Json::from("甲")), ("h", Json::from(30.0))]),
                feat(&[("name", Json::from("乙")), ("h", Json::from(10.0))]),
                feat(&[("name", Json::from("丙"))]), // h 缺失
            ],
            foreign_members: None,
        }
    }

    #[test]
    fn field_names_ordered_dedup() {
        // serde_json Map 默认 BTreeMap（键有序）——字段名按字典序。
        assert_eq!(field_names(&coll()), vec!["h", "name"]);
    }

    #[test]
    fn compare_and_sort() {
        let c = coll();
        let asc = sorted_indices(&c, "h", true);
        assert_eq!(asc, vec![1, 0, 2]); // 10 < 30，Null 恒排最后
        let desc = sorted_indices(&c, "h", false);
        assert_eq!(desc, vec![2, 0, 1]); // 降序 Null 在前
        let by_name = sorted_indices(&c, "name", true);
        assert_eq!(by_name, vec![2, 1, 0]); // 丙(U+4E19) < 乙(U+4E59) < 甲(U+7532)
    }

    #[test]
    fn row_filter_substring() {
        let c = coll();
        assert!(row_matches(c.features[0].properties.as_ref(), "甲"));
        assert!(row_matches(c.features[0].properties.as_ref(), "30"));
        assert!(!row_matches(c.features[2].properties.as_ref(), "30"));
        assert!(row_matches(c.features[2].properties.as_ref(), "")); // 空恒真
    }

    #[test]
    fn infer_types() {
        let kinds = infer_field_types(&coll());
        assert_eq!(kinds[0], ("h".to_string(), "数值".to_string()));
        assert_eq!(kinds[1], ("name".to_string(), "文本".to_string()));
    }

    #[test]
    fn preview_calc_rows() {
        let c = coll();
        let vals = preview_calc(Some(&c), "t", "h * 2", 5).unwrap();
        assert_eq!(vals, vec!["60.0", "20.0", ""]);
        assert!(preview_calc(Some(&c), "", "h", 5).is_err());
        assert!(preview_calc(Some(&c), "t", "h + 'x'", 5).is_err());
        assert!(preview_calc(None, "t", "1", 5).is_err());
    }
}
