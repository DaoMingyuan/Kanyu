//! 工具参数对话框（ArcGIS Pro 地理处理窗格范式，统一骨架）：
//! 说明区 → 输入参数组 → 输出参数组 → 校验消息区（错误红/警告琥珀/信息弱色，
//! updateMessages 语义）→ 焦点参数帮助区 → 「运行」（仅无错误可用）/「取消」。

use eframe::egui;

use super::params;
use crate::ui_kit::tokens::{spacing, text};
use crate::ui_kit::{button, error_caption, hint_caption, ButtonVariant};
use kanyu_core::tooldef::{Direction, ToolDef};
use kanyu_core::toolrun::{self as run, MsgLevel};

/// 对话框结果。
pub enum DialogOutcome {
    /// 继续显示。
    Open,
    /// 用户点「运行」（校验已通过；app 执行 run_tool）。
    Run,
    /// 取消/关闭。
    Cancel,
}

/// 参数对话框状态（工具 + 当前值 + 执行期错误 + 各参数"触碰"标记）。
pub struct ToolRunState {
    /// 工具定义。
    pub tool: &'static ToolDef,
    /// 当前参数值（与 tool.params 对齐）。
    pub values: Vec<String>,
    /// 执行期错误（run_tool 返回的错误，红字展示且不清空输入）。
    pub err: Option<String>,
    /// 各参数是否已触碰（失焦过才显示该校验红字，避免开框即满屏红）。
    touched: Vec<bool>,
}

impl ToolRunState {
    /// 以默认值初始化。
    pub fn new(tool: &'static ToolDef) -> Self {
        Self {
            tool,
            values: tool.params.iter().map(|p| p.default.to_string()).collect(),
            err: None,
            touched: vec![false; tool.params.len()],
        }
    }
}

/// 工具参数对话框 UI。`fields_of(layer_id)` 注入图层字段清单（Field 参数选项）。
pub fn run_dialog(
    ctx: &egui::Context,
    st: &mut ToolRunState,
    layer_ids: &[String],
    fields_of: &dyn Fn(&str) -> Vec<String>,
) -> DialogOutcome {
    let mut out = DialogOutcome::Open;
    let mut open = true;
    let mut closed = false;
    egui::Window::new(text::heading(st.tool.name))
        .collapsible(false)
        .resizable(false)
        .default_width(460.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            hint_caption(ui, st.tool.desc);
            ui.add_space(spacing::SM);

            // 参数行（输入组在前、输出组在后）；记录焦点参数（帮助区跟随）。
            let mut focused: Option<usize> = None;
            let layer_ids = layer_ids.to_vec();
            for group in [Direction::Input, Direction::Output] {
                if group == Direction::Output
                    && st
                        .tool
                        .params
                        .iter()
                        .any(|p| p.direction == Direction::Output)
                {
                    ui.add_space(spacing::XS);
                    crate::ui_kit::section_header(ui, "输出");
                }
                for i in 0..st.tool.params.len() {
                    if st.tool.params[i].direction != group {
                        continue;
                    }
                    // 行内红字仅错误级（警告/信息归消息区），失焦后才显示。
                    let err = run::validate_param(&st.tool.params[i], &st.values[i])
                        .into_iter()
                        .find(|m| m.level == MsgLevel::Error)
                        .map(|m| m.text);
                    let err = if st.touched[i] { err } else { None };
                    let resp = params::render(
                        ui,
                        st.tool,
                        i,
                        &mut st.values,
                        &layer_ids,
                        fields_of,
                        err.as_deref(),
                    );
                    if resp.lost_focus() {
                        st.touched[i] = true;
                    }
                    if resp.has_focus() || resp.hovered() {
                        focused = Some(i);
                    }
                }
            }

            // 校验消息区（错误红 / 警告琥珀 / 信息弱色）。
            let msgs = run::validate_msgs(st.tool, &st.values);
            if !msgs.is_empty() {
                ui.add_space(spacing::XS);
                ui.separator();
                let p = palette_of(ui);
                for m in &msgs {
                    let (icon, color) = match m.level {
                        MsgLevel::Error => ("✕", p.accent_secondary),
                        MsgLevel::Warning => ("⚠", p.warning),
                        MsgLevel::Info => ("ℹ", p.text_weak),
                    };
                    ui.label(text::caption(format!("{icon} {}", m.text)).color(color));
                }
            }

            // 参数帮助区（ArcGIS Pro 式：底部固定区，随焦点参数切换）。
            ui.add_space(spacing::SM);
            ui.separator();
            let p = palette_of(ui);
            let (title, body) = match focused {
                Some(i) => (st.tool.params[i].label, st.tool.params[i].help),
                None => ("说明", st.tool.desc),
            };
            ui.label(text::body(title).strong());
            ui.label(text::caption(body).color(p.text_weak));
            ui.add_space(spacing::MD);

            // 底部按钮：运行（Primary，无错误级消息才可用）/ 取消。
            let all_ok = run::validate(st.tool, &st.values).is_ok();
            ui.horizontal(|ui| {
                if button(ui, "运 行", ButtonVariant::Primary, all_ok).clicked() {
                    out = DialogOutcome::Run;
                }
                if button(ui, "取 消", ButtonVariant::Secondary, true).clicked() {
                    closed = true;
                }
                if let Some(e) = &st.err {
                    error_caption(ui, e);
                }
            });
        });
    if !open || closed {
        return DialogOutcome::Cancel;
    }
    out
}

/// 当前色板。
fn palette_of(ui: &egui::Ui) -> crate::theme::Palette {
    crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}

// ===== 运行进度（后台线程 + 进度模态 + 可终止）=====

/// 后台执行句柄（app 持有，每帧轮询）。
///
/// 协作式取消的简化语义（rustdoc 即契约）：内核算法为同步计算且不可中断，
/// 「取消」= 立即关闭模态并丢弃结果（通道接收端随本结构析构，后台线程
/// 自然结束后发送失败被忽略），不杀线程。
pub struct ToolProgress {
    /// 工具 id（完成后记「最近使用」）。
    pub tool_id: &'static str,
    /// 工具中文名。
    pub tool_name: String,
    /// 结果通道。
    pub rx: std::sync::mpsc::Receiver<Result<run::ToolOutcome, String>>,
}

/// 完成处置决策（纯函数，可测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionAction {
    /// 应用结果（新图层/报告）。
    Apply,
    /// 报错（终端 + toast）。
    ReportError,
    /// 丢弃（已取消）。
    Discard,
}

/// 完成处置决策：取消 → 丢弃；成功 → 应用；失败 → 报错。
pub fn completion_action(cancelled: bool, ok: bool) -> CompletionAction {
    if cancelled {
        CompletionAction::Discard
    } else if ok {
        CompletionAction::Apply
    } else {
        CompletionAction::ReportError
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_decision() {
        assert_eq!(completion_action(false, true), CompletionAction::Apply);
        assert_eq!(
            completion_action(false, false),
            CompletionAction::ReportError
        );
        assert_eq!(completion_action(true, true), CompletionAction::Discard);
        assert_eq!(completion_action(true, false), CompletionAction::Discard);
    }
}
