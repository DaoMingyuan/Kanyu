//! 参数组件库：每种 [`ParamKind`] 一个独立组件，统一契约——
//! `(&mut 值, 选项/提示, 校验错误 Option<&str>) -> Response`，
//! 供工具参数对话框（dialog.rs）或任何表单组合调用。
//! 全部经 ui_kit 基础控件组合（label + 控件 + error_caption）。

use eframe::egui;
use eframe::egui::{Response, Ui};

use super::{ParamKind, ToolDef, ToolParam};
use crate::ui_kit::{combo, combo_static, error_caption, layer_picker, text_input};

/// 图层参数（下拉选图层 id）。
pub fn layer(
    ui: &mut Ui,
    p: &ToolParam,
    value: &mut String,
    layers: &[String],
    err: Option<&str>,
) -> Response {
    let resp = layer_picker(ui, p.label, value, layers, true);
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 字段参数（下拉，选项 = 所属图层的字段；无字段时退化为文本输入）。
pub fn field(
    ui: &mut Ui,
    p: &ToolParam,
    value: &mut String,
    fields: &[String],
    err: Option<&str>,
) -> Response {
    let resp = if fields.is_empty() {
        text_input(ui, p.label, value, p.hint, true)
    } else {
        combo(ui, p.label, value, fields, true)
    };
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 数值参数。
pub fn number(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    let resp = text_input(ui, p.label, value, p.hint, true);
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 数值列表参数（逗号分隔）。
pub fn number_list(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    number(ui, p, value, err)
}

/// 自由文本参数。
pub fn text(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    number(ui, p, value, err)
}

/// 表达式参数。
pub fn expression(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    number(ui, p, value, err)
}

/// 枚举参数（中文标签下拉）。
pub fn choice(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    let ParamKind::Enum(options) = p.kind else {
        return text(ui, p, value, err);
    };
    let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
    let resp = combo_static(ui, p.label, value, &labels, true);
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 多值图层参数（multiValue；复选列表，内部以换行分隔 id 承载）。
pub fn multi_layers(
    ui: &mut Ui,
    p: &ToolParam,
    value: &mut String,
    layers: &[String],
    err: Option<&str>,
) -> Response {
    ui.label(crate::ui_kit::text::body(format!("{}:", p.label)));
    let resp = ui
        .vertical(|ui| {
            let mut selected: Vec<String> = kanyu_core::toolrun::parse_multi_layers(value);
            for id in layers {
                let mut on = selected.contains(id);
                if crate::ui_kit::checkbox(ui, &mut on, id).changed() {
                    if on {
                        selected.push(id.clone());
                    } else {
                        selected.retain(|s| s != id);
                    }
                    *value = selected.join("\n");
                }
            }
            ui.response()
        })
        .response;
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 线性单位参数（数值输入 + 单位下拉；承载 "数值|单位"）。
pub fn linear_unit(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    use kanyu_core::toolrun::LinearUnit;
    // 拆出数值与单位（空值给默认单位 米）。
    let (mut num, mut unit) = match value.split_once('|') {
        Some((n, u)) => (n.to_string(), u.to_string()),
        None => (value.clone(), "米".to_string()),
    };
    if unit.is_empty() {
        unit = "米".to_string();
    }
    let resp = ui
        .horizontal(|ui| {
            ui.label(crate::ui_kit::text::body(format!("{}:", p.label)));
            let r = ui.add(
                egui::TextEdit::singleline(&mut num)
                    .font(egui::FontId::proportional(13.0))
                    .desired_width(120.0)
                    .hint_text(p.hint),
            );
            let units: Vec<String> = LinearUnit::ALL
                .iter()
                .map(|u| u.label().to_string())
                .collect();
            crate::ui_kit::combo(ui, "单位", &mut unit, &units, true);
            r
        })
        .inner;
    *value = format!("{num}|{unit}");
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 布尔参数（复选框；承载 "true"/"false"）。
pub fn boolean(ui: &mut Ui, p: &ToolParam, value: &mut String, err: Option<&str>) -> Response {
    let mut on = value == "true";
    let resp = crate::ui_kit::checkbox(ui, &mut on, p.label);
    if resp.changed() {
        *value = on.to_string();
    }
    if let Some(e) = err {
        error_caption(ui, e);
    }
    resp
}

/// 按参数类型分派渲染（对话框用）。`fields_of` 注入字段清单查询
/// （Field 参数的锚点图层值在本函数内解析，避免调用方处理借用）。
pub fn render(
    ui: &mut Ui,
    def: &ToolDef,
    index: usize,
    values: &mut [String],
    layers: &[String],
    fields_of: &dyn Fn(&str) -> Vec<String>,
    err: Option<&str>,
) -> Response {
    let p = &def.params[index];
    match &p.kind {
        ParamKind::Layer => layer(ui, p, &mut values[index], layers, err),
        ParamKind::MultiLayers => multi_layers(ui, p, &mut values[index], layers, err),
        ParamKind::Field(anchor) => {
            let layer_id = values.get(*anchor).cloned().unwrap_or_default();
            let fields = fields_of(&layer_id);
            field(ui, p, &mut values[index], &fields, err)
        }
        ParamKind::Number | ParamKind::Long => number(ui, p, &mut values[index], err),
        ParamKind::NumberList => number_list(ui, p, &mut values[index], err),
        ParamKind::Text | ParamKind::Extent | ParamKind::Crs | ParamKind::OutFile => {
            text(ui, p, &mut values[index], err)
        }
        ParamKind::Expression => expression(ui, p, &mut values[index], err),
        ParamKind::Enum(_) => choice(ui, p, &mut values[index], err),
        ParamKind::LinearUnit => linear_unit(ui, p, &mut values[index], err),
        ParamKind::Boolean => boolean(ui, p, &mut values[index], err),
    }
}
