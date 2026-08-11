//! 参数组件库：每种 [`ParamKind`] 一个独立组件，统一契约——
//! `(&mut 值, 选项/提示, 校验错误 Option<&str>) -> Response`，
//! 供工具参数对话框（dialog.rs）或任何表单组合调用。
//! 全部经 ui_kit 基础控件组合（label + 控件 + error_caption）。

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
        ParamKind::Field(anchor) => {
            let layer_id = values.get(*anchor).cloned().unwrap_or_default();
            let fields = fields_of(&layer_id);
            field(ui, p, &mut values[index], &fields, err)
        }
        ParamKind::Number => number(ui, p, &mut values[index], err),
        ParamKind::NumberList => number_list(ui, p, &mut values[index], err),
        ParamKind::Text => text(ui, p, &mut values[index], err),
        ParamKind::Expression => expression(ui, p, &mut values[index], err),
        ParamKind::Enum(_) => choice(ui, p, &mut values[index], err),
    }
}
