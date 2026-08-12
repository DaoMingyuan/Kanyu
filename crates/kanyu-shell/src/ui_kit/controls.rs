//! 基础控件：按钮（四变体）/ 图标按钮 / 文本输入 / 下拉框 / 复选框。
//!
//! 状态契约：全部控件接受 `enabled: bool`；禁用时 egui 自动降饱和。
//! 使用示例见各函数 rustdoc。

use eframe::egui;
use egui::{Response, RichText, Stroke, Vec2};

use super::tokens::{animation, radius, sizes, spacing, text};
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

/// 标准下拉框（动态字符串选项，如图层/技能 id 列表）。
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
    combo_width(ui, label, value, options, sizes::INPUT_W, enabled)
}

/// 指定宽度的下拉框（窄停靠区回流用，如属性表工具条）。
pub fn combo_width(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    options: &[String],
    width: f32,
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
                .width(width)
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

/// 图层/技能选择器（combo 的语义化别名，文档级组件）。
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
    cache: &mut super::icons::IconCache,
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

    // 悬停动画（egui 原生驱动，动画期间自动重绘）：
    // 背景淡入 hover 底色 + 图标 1.0→HOVER_SCALE 放大并微上移；按下缩至 PRESS_SCALE。
    let hover_t =
        ui.ctx()
            .animate_bool_with_time(resp.id, resp.hovered() && enabled, animation::HOVER_SECS);
    if hover_t > 0.0 {
        let bg = super::tokens::state::hover_bg(&p);
        let bg = egui::Color32::from_rgba_unmultiplied(
            bg.r(),
            bg.g(),
            bg.b(),
            (f32::from(bg.a()) * hover_t) as u8,
        );
        ui.painter().rect_filled(rect, radius::SM, bg);
    }
    let scale = animation::icon_scale(hover_t, resp.is_pointer_button_down_on());
    let lift = animation::icon_lift(hover_t);

    // 图标（上部；位图优先、手绘回退；矩形随动画缩放/上移）。
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.min.y + ICON_TOP + ICON / 2.0 - lift),
        Vec2::splat(ICON * scale),
    );
    super::icons::draw_or_image(ui, cache, icon, icon_rect, color);
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
/// let (row, toggled) = tree_row(ui, &mut cache, 1, Some(Icon::Layers), "buildings.geojson", true, |ui| { /* 行尾按钮 */ });
/// ```
pub fn tree_row(
    ui: &mut egui::Ui,
    cache: &mut super::icons::IconCache,
    depth: usize,
    icon: Option<super::icons::Icon>,
    label: &str,
    expanded: Option<bool>,
    trailing: impl FnOnce(&mut egui::Ui),
) -> (Response, bool) {
    tree_row_impl(ui, cache, depth, icon, label, expanded, false, trailing)
}

/// 目录树行弱色变体（已关闭/不可用条目：文本弱色，交互不变）。
pub fn tree_row_weak(
    ui: &mut egui::Ui,
    cache: &mut super::icons::IconCache,
    depth: usize,
    icon: Option<super::icons::Icon>,
    label: &str,
    expanded: Option<bool>,
    trailing: impl FnOnce(&mut egui::Ui),
) -> (Response, bool) {
    tree_row_impl(ui, cache, depth, icon, label, expanded, true, trailing)
}

#[allow(clippy::too_many_arguments)]
fn tree_row_impl(
    ui: &mut egui::Ui,
    cache: &mut super::icons::IconCache,
    depth: usize,
    icon: Option<super::icons::Icon>,
    label: &str,
    expanded: Option<bool>,
    weak: bool,
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
        // 图标（位图优先、手绘回退）。
        if let Some(ic) = icon {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(15.0), egui::Sense::hover());
            super::icons::draw_or_image(ui, cache, ic, rect, p.accent);
            ui.add_space(2.0);
        }
        // 文本（truncate：窄停靠区内超长截断为 …，不撑破布局；弱色行用于已关闭项）。
        let label = if weak {
            text::body(label).color(p.text_weak)
        } else {
            text::body(label)
        };
        let resp = ui.add(egui::Button::selectable(false, label).truncate());
        // 行尾操作。
        trailing(ui);
        resp
    });
    (row.inner, toggled)
}

/// TOC 树行响应（Contents 窗格专用行组件的返回）。
pub struct TocRowResponse {
    /// 整行响应（单击选中 / 右键上下文菜单共用此 Response——
    /// 行内不再叠手写 interact 矩形，避免错位）。
    pub row: Response,
    /// 展开箭头被点击。
    pub toggled: bool,
    /// 复选框新状态（未变动为 None）。
    pub checked: Option<bool>,
}

/// Contents 目录树行（ArcGIS Pro 图层行范式）：
/// `[缩进参考线] [可见性复选框] [展开箭头/占位] [图标或几何色块] [文本]`。
///
/// - 整行一个 Response（先分配行矩形，子控件在其上排布：复选框/箭头
///   自占点击，其余区域归行响应）；选中底色 = palette.selection，
///   悬停 = palette.hover；不可见条目文本取弱色。
/// - `icon` 与 `swatch` 二选一（组行传图标，图层行传几何图例色块）。
/// - 样式数值为规范常量（与 tree_row 同一缩进/行高标尺）。
///
/// ```
/// let r = toc_row(ui, &mut cache, 0, true, Some(true), Some(Icon::Folder), None, "基底 (3 项)", false, false);
/// if r.row.clicked() { /* 选中 */ }
/// r.row.context_menu(|ui| { /* 右键菜单 */ });
/// ```
#[allow(clippy::too_many_arguments)]
pub fn toc_row(
    ui: &mut egui::Ui,
    cache: &mut super::icons::IconCache,
    depth: usize,
    checked: bool,
    expanded: Option<bool>,
    icon: Option<super::icons::Icon>,
    swatch: Option<egui::Color32>,
    label: &str,
    weak: bool,
    selected: bool,
) -> TocRowResponse {
    // 规范常量：与 tree_row 同标尺（每级缩进 14px、箭头槽 16px、图标槽 15px）。
    const INDENT: f32 = 14.0;
    const ARROW: f32 = 16.0;
    const SLOT: f32 = 15.0;

    let p = palette_of(ui);
    let row_h = sizes::CONTROL_SM;
    let width = ui.available_width();
    let (rect, row) = ui.allocate_exact_size(Vec2::new(width, row_h), egui::Sense::click());

    // 行底：选中（0.12s 淡入过渡，tokens::animation::HOVER_SECS）> 悬停。
    let sel_t =
        ui.ctx()
            .animate_bool_with_time(row.id.with("sel"), selected, animation::HOVER_SECS);
    if sel_t > 0.0 {
        let sel = super::tokens::state::selection_bg(&p);
        let bg = egui::Color32::from_rgba_unmultiplied(
            sel.r(),
            sel.g(),
            sel.b(),
            (f32::from(sel.a()) * sel_t) as u8,
        );
        ui.painter().rect_filled(rect, radius::SM, bg);
    } else if row.hovered() {
        ui.painter()
            .rect_filled(rect, radius::SM, super::tokens::state::hover_bg(&p));
    }

    let mut toggled = false;
    let mut checked_out = None;
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let ui = &mut child;
    ui.add_space(spacing::XS);

    // 缩进参考线（每级 14px + 竖线，与 tree_row 一致）。
    for _ in 0..depth {
        let (r, _) = ui.allocate_exact_size(Vec2::new(INDENT, row_h), egui::Sense::hover());
        ui.painter().line_segment(
            [
                egui::pos2(r.center().x, r.min.y),
                egui::pos2(r.center().x, r.max.y),
            ],
            Stroke::new(1.0, p.border),
        );
    }

    // 可见性复选框（egui 原生，主题色出自 apply_theme 的 visuals）。
    let mut c = checked;
    if ui.checkbox(&mut c, "").changed() {
        checked_out = Some(c);
    }

    // 展开箭头（无子节点时占位，保持对齐）。
    match expanded {
        Some(is_open) => {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ARROW, row_h), egui::Sense::click());
            let cy = r.center().y;
            let cx = r.center().x;
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
            ui.allocate_exact_size(Vec2::new(ARROW, row_h), egui::Sense::hover());
        }
    }

    // 图标（组）或几何图例色块（图层）。
    let (slot, _) = ui.allocate_exact_size(Vec2::new(SLOT, row_h), egui::Sense::hover());
    if let Some(ic) = icon {
        let icon_rect = egui::Rect::from_center_size(slot.center(), Vec2::splat(SLOT));
        super::icons::draw_or_image(ui, cache, ic, icon_rect, p.accent);
    } else if let Some(color) = swatch {
        let sw = egui::Rect::from_center_size(slot.center(), Vec2::splat(10.0));
        ui.painter().rect_filled(sw, 2.0, color);
        ui.painter().rect_stroke(
            sw,
            2.0,
            Stroke::new(0.5, p.border),
            egui::StrokeKind::Middle,
        );
    }
    ui.add_space(spacing::XS);

    // 文本（弱色 = 不可见；选中加粗）。Label 默认不吞点击，
    // 点击穿透到行响应（整行选中/右键统一由 row 承担）。
    let color = if weak { p.text_weak } else { p.text_primary };
    let mut t = text::body(label).color(color);
    if selected {
        t = t.strong();
    }
    ui.add(egui::Label::new(t).selectable(false));

    TocRowResponse {
        row,
        toggled,
        checked: checked_out,
    }
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

// ===== 复合控件（菜单按钮 / 数值步进）=====

/// 菜单按钮响应（split button 语义）。
pub struct MenuButtonResponse {
    /// 主体被点（执行默认动作）。
    pub clicked: bool,
    /// 下拉菜单选中项下标。
    pub selected: Option<usize>,
}

/// 菜单按钮（split button）：`[主体 ▾]`——点主体执行默认动作，点箭头开下拉菜单。
/// 样式出自 tokens/palette（Subtle 按钮语言）。
///
/// ```
/// let r = menu_button(ui, "选项", &["全部展开", "全部折叠"]);
/// if r.clicked { /* 默认动作 */ }
/// if let Some(0) = r.selected { /* 全部展开 */ }
/// ```
pub fn menu_button(ui: &mut egui::Ui, label: &str, items: &[&str]) -> MenuButtonResponse {
    let mut out = MenuButtonResponse {
        clicked: false,
        selected: None,
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        out.clicked = button(ui, label, ButtonVariant::Subtle, true).clicked();
        ui.menu_button(text::body("▾"), |ui| {
            for (i, item) in items.iter().enumerate() {
                if ui.button(text::body(*item)).clicked() {
                    out.selected = Some(i);
                    ui.close();
                }
            }
        });
    });
    out
}

/// 数值步进钳制（纯函数）：value + delta 后钳入 [min, max]。
pub fn clamp_step(value: f64, delta: f64, min: f64, max: f64) -> f64 {
    (value + delta).clamp(min, max)
}

/// 数值步进输入框（spinner）：标签 + `[−] [可输入框] [＋]`，min/max 钳制。
/// 直接输入失焦后同样钳制；步进按 step。
///
/// ```
/// let mut v = 1200.0;
/// spinner(ui, "宽度 px", &mut v, 64.0..=8192.0, 50.0);
/// ```
pub fn spinner(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
) -> Response {
    let (min, max) = (*range.start(), *range.end());
    ui.horizontal(|ui| {
        ui.label(text::body(format!("{label}:")));
        if ui
            .add(egui::Button::new(text::body("−")).min_size(Vec2::splat(sizes::CONTROL_SM)))
            .clicked()
        {
            *value = clamp_step(*value, -step, min, max);
        }
        let resp = ui.add(
            egui::DragValue::new(value)
                .speed(step)
                .range(min..=max)
                .max_decimals(2),
        );
        if ui
            .add(egui::Button::new(text::body("＋")).min_size(Vec2::splat(sizes::CONTROL_SM)))
            .clicked()
        {
            *value = clamp_step(*value, step, min, max);
        }
        resp
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_step_bounds() {
        assert_eq!(clamp_step(1200.0, 50.0, 64.0, 8192.0), 1250.0);
        assert_eq!(clamp_step(64.0, -50.0, 64.0, 8192.0), 64.0); // 下钳制
        assert_eq!(clamp_step(8192.0, 50.0, 64.0, 8192.0), 8192.0); // 上钳制
        assert_eq!(clamp_step(0.5, -1.0, 0.0, 10.0), 0.0);
    }
}
