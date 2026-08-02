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

/// ArcGIS Pro 式功能区大按钮：**图标（上，20px）、标题（下，11px）、
/// 简介（悬停浮现卡）三分离**——简介只经鼠标悬停浮现，不挤占按钮版面。
///
/// 版式（数值为规范常量）：宽 68 × 高 56；图标 20px 顶部内边距 7px；
/// 标题 11px 底部内边距 6px，图标与标题间自然呼吸间隙。
///
/// ```
/// if ribbon_button(ui, Icon::Buffer, "缓冲区", "按距离生成缓冲区", "结果存为新图层", true).clicked() { /* … */ }
/// ```
pub fn ribbon_button(
    ui: &mut egui::Ui,
    icon: super::icons::Icon,
    label: &str,
    desc_title: &str,
    desc_body: &str,
    enabled: bool,
) -> Response {
    const W: f32 = 68.0;
    const H: f32 = 56.0;
    const ICON: f32 = 20.0;
    const ICON_TOP: f32 = 7.0;
    const LABEL_BOTTOM: f32 = 6.0;

    let p = palette_of(ui);
    let btn = egui::Button::new("")
        .fill(egui::Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(radius::SM)
        .min_size(Vec2::new(W, H));
    let resp = ui.add_enabled(enabled, btn);
    let rect = resp.rect;
    let color = if enabled { p.text_primary } else { p.text_weak };

    // 图标（上部）。
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.min.y + ICON_TOP + ICON / 2.0),
        Vec2::splat(ICON),
    );
    super::icons::draw(ui.painter(), icon, icon_rect, color);
    // 标题（底部）。
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - LABEL_BOTTOM),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(11.0),
        color,
    );
    // 简介：鼠标悬停浮现卡（egui 原生悬停层，自动定位防遮挡）。
    if enabled {
        resp.clone().on_hover_ui(|ui| {
            ui.set_max_width(240.0);
            ui.label(text::body(desc_title).strong());
            ui.add_space(2.0);
            ui.label(text::caption(desc_body));
        });
    }
    resp
}

/// 页签条（终端 | AI 对话 等同级别切换）：文本页签 + 选中下划线。
///
/// ```
/// let mut active = 0;
/// tab_strip(ui, &["终端", "AI 对话"], &mut active);
/// ```
pub fn tab_strip(ui: &mut egui::Ui, tabs: &[&str], active: &mut usize) {
    let p = palette_of(ui);
    ui.horizontal(|ui| {
        for (i, tab) in tabs.iter().enumerate() {
            let selected = *active == i;
            let t = if selected {
                text::body(*tab).strong()
            } else {
                text::body(*tab).color(p.text_weak)
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
                    Stroke::new(2.0, p.accent),
                );
            }
            if resp.clicked() {
                *active = i;
            }
            ui.add_space(10.0);
        }
    });
}

/// 目录树行（ArcGIS Pro 骨架目录）：缩进参考线 + 展开箭头（可选）+
/// 图标（可选）+ 文本 + 行尾操作区。返回 (行响应, 展开箭头是否被点)。
///
/// ```
/// let (row, toggled) = tree_row(ui, 1, Some(Icon::Layers), "buildings.geojson", true, |ui| { /* 行尾按钮 */ });
/// ```
pub fn tree_row(
    ui: &mut egui::Ui,
    depth: usize,
    icon: Option<super::icons::Icon>,
    label: &str,
    expanded: Option<bool>,
    trailing: impl FnOnce(&mut egui::Ui),
) -> (Response, bool) {
    let p = palette_of(ui);
    let mut toggled = false;
    let row = ui.horizontal(|ui| {
        // 缩进参考线（每级 14px + 竖线）。
        for _ in 0..depth {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(14.0, 22.0), egui::Sense::hover());
            ui.painter().line_segment(
                [
                    egui::pos2(rect.center().x, rect.min.y),
                    egui::pos2(rect.center().x, rect.max.y),
                ],
                Stroke::new(1.0, p.border),
            );
        }
        // 展开箭头（expander；无子节点时占位）。
        match expanded {
            Some(is_open) => {
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(16.0, 22.0), egui::Sense::click());
                let cy = rect.center().y;
                let cx = rect.center().x;
                let pts = if is_open {
                    vec![
                        egui::pos2(cx - 4.0, cy - 2.0),
                        egui::pos2(cx + 4.0, cy - 2.0),
                        egui::pos2(cx, cy + 4.0),
                    ]
                } else {
                    vec![
                        egui::pos2(cx - 2.0, cy - 4.0),
                        egui::pos2(cx + 4.0, cy),
                        egui::pos2(cx - 2.0, cy + 4.0),
                    ]
                };
                ui.painter()
                    .add(egui::Shape::convex_polygon(pts, p.text_weak, Stroke::NONE));
                if resp.clicked() {
                    toggled = true;
                }
            }
            None => {
                ui.allocate_exact_size(Vec2::new(16.0, 22.0), egui::Sense::hover());
            }
        }
        // 图标。
        if let Some(ic) = icon {
            super::icons::icon_ui(ui, ic, 15.0, p.accent);
            ui.add_space(2.0);
        }
        // 文本。
        let resp = ui.selectable_label(false, text::body(label));
        // 行尾操作。
        trailing(ui);
        resp
    });
    (row.inner, toggled)
}

/// 密码输入（API Key 掩码）。
///
/// ```
/// password_input(ui, "API Key:", &mut key, true);
/// ```
pub fn password_input(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    enabled: bool,
) -> Response {
    ui.horizontal(|ui| {
        ui.label(text::body(format!("{label}:")));
        ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(value)
                .font(egui::FontId::proportional(13.0))
                .desired_width(super::tokens::sizes::INPUT_W)
                .password(true),
        )
    })
    .inner
}
