//! 基础控件：按钮（四变体）/ 图标按钮 / 文本输入 / 下拉框 / 复选框。
//!
//! 状态契约：全部控件接受 `enabled: bool`；禁用时 egui 自动降饱和。
//! 使用示例见各函数 rustdoc。

use eframe::egui;
use egui::{Response, RichText, Stroke, Vec2};

use super::tokens::{radius, sizes, text};
use crate::theme::{palette, Palette};

/// 按钮变体（对应总规语义色彩）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonVariant {
    /// 主按钮：强调色实心（每区块至多一个，用于主操作）。
    Primary,
    /// 次按钮：边框 + 文本（常规操作）。
    Secondary,
    /// 弱按钮：无框，悬停显底（低频/辅助操作）。
    Subtle,
    /// 危险按钮：朱砂/珊瑚边框文本（删除、覆盖等需二次确认的操作）。
    Danger,
}

/// 标准按钮。
///
/// ```
/// if button(ui, "确 定", ButtonVariant::Primary, true).clicked() { /* … */ }
/// ```
pub fn button(ui: &mut egui::Ui, label: &str, variant: ButtonVariant, enabled: bool) -> Response {
    let p = palette_of(ui);
    let (fill, stroke, text_color) = match variant {
        ButtonVariant::Primary => (p.accent, Stroke::new(1.0, p.accent), on_accent(ui)),
        ButtonVariant::Secondary => (
            egui::Color32::TRANSPARENT,
            Stroke::new(1.0, p.border),
            p.text_primary,
        ),
        ButtonVariant::Subtle => (egui::Color32::TRANSPARENT, Stroke::NONE, p.text_primary),
        ButtonVariant::Danger => (
            egui::Color32::TRANSPARENT,
            Stroke::new(1.0, p.accent_secondary),
            p.accent_secondary,
        ),
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(text::body(label).color(text_color))
            .fill(fill)
            .stroke(stroke)
            .corner_radius(radius::SM)
            .min_size(Vec2::new(0.0, sizes::CONTROL_MD)),
    )
}

/// 图标按钮（工具条/标题栏用）：glyph + 悬停提示。
///
/// ```
/// if icon_button(ui, "🌓", "切换主题", true).clicked() { /* … */ }
/// ```
pub fn icon_button(ui: &mut egui::Ui, glyph: &str, tip: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(glyph).size(14.0))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .corner_radius(radius::SM)
            .min_size(Vec2::new(sizes::CONTROL_MD, sizes::CONTROL_MD)),
    )
    .on_hover_text(tip)
}

/// 标准文本输入（标签 + 单行框 + 提示）。
///
/// ```
/// text_input(ui, "名称:", &mut name, "如 buildings", true);
/// ```
pub fn text_input(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    hint: &str,
    enabled: bool,
) -> Response {
    ui.horizontal(|ui| {
        ui.label(text::body(format!("{label}:")));
        ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(value)
                .font(egui::FontId::proportional(13.0))
                .desired_width(sizes::INPUT_W)
                .hint_text(hint),
        )
    })
    .inner
}

/// 多行文本输入（JSON 等）。
pub fn text_area(
    ui: &mut egui::Ui,
    value: &mut String,
    rows: usize,
    hint: &str,
    enabled: bool,
) -> Response {
    ui.add_enabled(
        enabled,
        egui::TextEdit::multiline(value)
            .desired_rows(rows)
            .font(egui::FontId::new(11.0, egui::FontFamily::Monospace))
            .hint_text(hint),
    )
}

/// 标准下拉框（动态字符串选项，如图层/基因 id 列表）。
///
/// ```
/// combo(ui, "图层", &mut layer, &layer_ids, true);
/// ```
pub fn combo(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[String],
    enabled: bool,
) -> Response {
    ui.horizontal(|ui| {
        ui.label(text::body(format!("{label}:")));
        if options.is_empty() {
            ui.label(text::caption("（无可用项）").color(palette_of(ui).text_weak));
            return ui.response();
        }
        if value.is_empty() || !options.contains(value) {
            *value = options[0].clone();
        }
        let mut resp = ui.response();
        ui.add_enabled_ui(enabled, |ui| {
            resp = egui::ComboBox::from_id_salt(label)
                .selected_text(text::body(value.as_str()))
                .show_ui(ui, |ui| {
                    for opt in options {
                        ui.selectable_value(value, opt.clone(), text::body(opt));
                    }
                })
                .response;
        });
        resp
    })
    .inner
}

/// 下拉框（静态选项，如操作/谓词枚举）。
pub fn combo_static(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[&str],
    enabled: bool,
) -> Response {
    let owned: Vec<String> = options.iter().map(|s| s.to_string()).collect();
    combo(ui, label, value, &owned, enabled)
}

/// 图层/基因选择器（combo 的语义化别名，文档级组件）。
pub fn layer_picker(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[String],
    enabled: bool,
) -> Response {
    combo(ui, label, value, options, enabled)
}

/// 复选框（标签 + 标准间距）。
pub fn checkbox(ui: &mut egui::Ui, checked: &mut bool, label: &str) -> Response {
    ui.checkbox(checked, text::body(label))
}

/// 验证错误红字（表单项下方，朱砂/珊瑚 caption）。
pub fn error_caption(ui: &mut egui::Ui, msg: &str) {
    ui.label(text::caption(msg).color(palette_of(ui).accent_secondary));
}

/// 弱文本注释（caption + 弱色）。
pub fn hint_caption(ui: &mut egui::Ui, msg: &str) {
    ui.label(text::caption(msg).color(palette_of(ui).text_weak));
}

/// 当前色板。
fn palette_of(ui: &egui::Ui) -> Palette {
    palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}

/// 强调色上的文字色（晨山用雾白，夜观星用墨夜——保证对比度）。
fn on_accent(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(0x0D, 0x0F, 0x12)
    } else {
        egui::Color32::from_rgb(0xF7, 0xF5, 0xF2)
    }
}
