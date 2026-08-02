//! 容器组件：卡片 / 区块标题 / 对话框壳 / 状态徽章。

use eframe::egui;
use egui::{Response, Stroke, Vec2};

use super::controls::{button, ButtonVariant};
use super::tokens::{radius, spacing, text};
use crate::theme::{palette, Palette};

/// 卡片容器（bitfun 视觉核心）：圆角 7px、细描边、内边距 12px、卡片底色。
///
/// ```
/// card(ui, |ui| { ui.label("卡片内容"); });
/// ```
pub fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let p = palette_of(ui);
    egui::Frame::new()
        .fill(p.bg_secondary)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(radius::MD)
        .inner_margin(egui::Margin::same(spacing::MD as i8))
        .show(ui, |ui| add(ui));
}

/// 区块标题：强调色短条 + 加粗小字（bitfun 式分节）。
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    let p = palette_of(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(3.0, 13.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, radius::SM, p.accent);
        ui.label(text::body(title).strong());
    });
    ui.add_space(6.0);
}

/// 对话框动作。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogAction {
    /// 无操作（继续显示）。
    None,
    /// 用户确认。
    Ok,
    /// 用户取消。
    Cancel,
}

/// 标准对话框壳：标题 + 表单区 + 确定（主按钮）/ 取消（次按钮）。
/// 返回用户动作；Ok/Cancel 后调用方负责清空对话框状态。
///
/// ```
/// if dialog_shell(ctx, "缓冲区分析", |ui| { /* 表单项 */ }) == DialogAction::Ok {
///     // 采集参数并执行
/// }
/// ```
pub fn dialog_shell(
    ctx: &egui::Context,
    title: &str,
    body: impl FnOnce(&mut egui::Ui),
) -> DialogAction {
    let mut action = DialogAction::None;
    egui::Window::new(text::heading(title))
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            body(ui);
            ui.add_space(spacing::MD);
            ui.horizontal(|ui| {
                if button(ui, "确 定", ButtonVariant::Primary, true).clicked() {
                    action = DialogAction::Ok;
                }
                if button(ui, "取 消", ButtonVariant::Secondary, true).clicked() {
                    action = DialogAction::Cancel;
                }
            });
        });
    action
}

/// 状态徽章：小药丸（stable=强调 / incubating=三强调 / planned=弱色）。
pub fn badge(ui: &mut egui::Ui, label: &str, level: BadgeLevel) -> Response {
    let p = palette_of(ui);
    let (fg, bg) = match level {
        BadgeLevel::Stable => (on_badge_fg(ui), p.accent),
        BadgeLevel::Incubating => (egui::Color32::from_rgb(0x1A, 0x1A, 0x1A), p.accent_tertiary),
        BadgeLevel::Planned => (p.text_weak, p.bg_tertiary),
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(radius::SM)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| ui.label(text::caption(label).color(fg)))
        .response
}

/// 徽章级别。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BadgeLevel {
    /// 稳定（强调色底）。
    Stable,
    /// 孵化中（三强调色底）。
    Incubating,
    /// 规划中（灰底弱字）。
    Planned,
}

/// 当前色板。
fn palette_of(ui: &egui::Ui) -> Palette {
    palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}

/// 徽章前景色（深底浅字/浅底深字随主题）。
fn on_badge_fg(ui: &egui::Ui) -> egui::Color32 {
    if ui.visuals().dark_mode {
        egui::Color32::from_rgb(0x0D, 0x0F, 0x12)
    } else {
        egui::Color32::from_rgb(0xF7, 0xF5, 0xF2)
    }
}
