//! 容器组件：卡片 / 区块标题 / 对话框壳 / 状态徽章。

use eframe::egui;
use egui::{Color32, Response, Stroke, Vec2};

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
        BadgeLevel::Stable => (on_badge_fg(ui), p.accent_strong),
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

// ===== Toast 轻提示（右上角浮现，自动消退）=====

/// Toast 类别（成功=青 / 错误=朱砂，色出自 palette）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    /// 成功（强调青）。
    Success,
    /// 失败（朱砂/珊瑚）。
    Error,
}

/// 一条轻提示。`added` 计时驱动淡入淡出与自动消退。
pub struct Toast {
    /// 唯一 id（动画状态键）。
    pub id: u64,
    /// 文本（中文）。
    pub text: String,
    /// 类别。
    pub kind: ToastKind,
    /// 入队时刻。
    pub added: std::time::Instant,
}

impl Toast {
    /// 是否已逾消退时长（tokens::animation::TOAST_SECS）。
    pub fn expired(&self) -> bool {
        self.added.elapsed().as_secs_f32() > crate::ui_kit::tokens::animation::TOAST_SECS
    }
}

/// Toast id 计数器（进程内单调）。
fn next_toast_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Toast 队列上限（超出丢最旧）。
pub const TOAST_MAX: usize = 5;

/// 入队一条 toast（纯队列逻辑，可测）。
pub fn push_toast(list: &mut Vec<Toast>, kind: ToastKind, text: impl Into<String>) {
    if list.len() >= TOAST_MAX {
        list.remove(0);
    }
    list.push(Toast {
        id: next_toast_id(),
        text: text.into(),
        kind,
        added: std::time::Instant::now(),
    });
}

/// 渲染 toast 栈（右上角，淡入淡出 + 到时自动消退）。app 每帧调用。
/// 色板由调用方给（Area 层无宿主 Ui，与 dock 投放提示同一取法）。
pub fn toast_stack(ctx: &egui::Context, toasts: &mut Vec<Toast>, p: &Palette) {
    toasts.retain(|t| !t.expired());
    if toasts.is_empty() {
        return;
    }
    // 动画/计时依赖帧推进：队列非空期间持续重绘。
    ctx.request_repaint();
    egui::Area::new(egui::Id::new("toast_stack"))
        .anchor(
            egui::Align2::RIGHT_TOP,
            Vec2::new(
                -spacing::MD,
                crate::ui_kit::tokens::sizes::RIBBON + spacing::SM,
            ),
        )
        .show(ctx, |ui| {
            for t in toasts.iter() {
                // 淡入 0.15s / 末段 0.3s 淡出（egui 原生动画驱动）。
                let fading = t.added.elapsed().as_secs_f32()
                    > crate::ui_kit::tokens::animation::TOAST_SECS - 0.3;
                let a =
                    ui.ctx()
                        .animate_bool_with_time(egui::Id::new(("toast", t.id)), !fading, 0.15);
                let accent = match t.kind {
                    ToastKind::Success => p.success,
                    ToastKind::Error => p.accent_secondary,
                };
                let fade = |c: Color32| {
                    Color32::from_rgba_unmultiplied(
                        c.r(),
                        c.g(),
                        c.b(),
                        (f32::from(c.a()) * a) as u8,
                    )
                };
                egui::Frame::new()
                    .fill(fade(p.bg_secondary))
                    .stroke(Stroke::new(1.0, fade(accent)))
                    .corner_radius(radius::SM)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(320.0);
                        ui.label(text::body(&t.text).color(fade(accent)));
                    });
                ui.add_space(spacing::SM);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_toast_caps_at_max() {
        let mut list = Vec::new();
        for i in 0..8 {
            push_toast(&mut list, ToastKind::Success, format!("t{i}"));
        }
        assert_eq!(list.len(), TOAST_MAX);
        assert_eq!(list[0].text, "t3"); // 最旧被丢
        assert_eq!(list[4].text, "t7");
    }

    #[test]
    fn toast_expiry() {
        let fresh = Toast {
            id: 1,
            text: "x".into(),
            kind: ToastKind::Success,
            added: std::time::Instant::now(),
        };
        assert!(!fresh.expired());
        let old = Toast {
            added: std::time::Instant::now()
                - std::time::Duration::from_secs_f32(
                    crate::ui_kit::tokens::animation::TOAST_SECS + 1.0,
                ),
            ..fresh
        };
        assert!(old.expired());
    }
}
