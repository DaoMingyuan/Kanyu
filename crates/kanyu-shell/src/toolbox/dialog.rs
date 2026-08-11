//! 工具参数对话框（ArcGIS Pro 地理处理窗格范式）：
//! 参数行（标签 + 参数组件 + 失焦内联红字）+ 底部参数帮助区
//! （显示当前焦点参数的说明）+ 「运行」（仅全表校验通过可用）/「取消」。

use eframe::egui;

use super::params;
use crate::ui_kit::tokens::{spacing, text};
use crate::ui_kit::{button, error_caption, hint_caption, ButtonVariant};
use kanyu_core::tooldef::ToolDef;
use kanyu_core::toolrun as run;

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
    egui::Window::new(text::heading(st.tool.name))
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            hint_caption(ui, st.tool.desc);
            ui.add_space(spacing::SM);

            // 参数行：组件 + 失焦内联校验红字；记录焦点参数（帮助区跟随）。
            let mut focused: Option<usize> = None;
            for i in 0..st.tool.params.len() {
                let err = run::validate_param(&st.tool.params[i], &st.values[i]);
                let err = if st.touched[i] { err } else { None };
                let resp = params::render(
                    ui,
                    st.tool,
                    i,
                    &mut st.values,
                    layer_ids,
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

            // 参数帮助区（ArcGIS Pro 式：底部固定区，随焦点参数切换）。
            ui.add_space(spacing::SM);
            ui.separator();
            let (title, body) = match focused {
                Some(i) => (st.tool.params[i].label, st.tool.params[i].help),
                None => ("说明", st.tool.desc),
            };
            ui.label(text::body(title).strong());
            ui.label(
                text::caption(body).color(
                    crate::theme::palette(if ui.visuals().dark_mode {
                        kanyu_render::Theme::Dark
                    } else {
                        kanyu_render::Theme::Light
                    })
                    .text_weak,
                ),
            );
            ui.add_space(spacing::MD);

            // 底部按钮：运行（Primary，全表校验通过才可用）/ 取消。
            let all_ok = run::validate(st.tool, &st.values).is_ok();
            ui.horizontal(|ui| {
                if button(ui, "运 行", ButtonVariant::Primary, all_ok).clicked() {
                    out = DialogOutcome::Run;
                }
                if button(ui, "取 消", ButtonVariant::Secondary, true).clicked() {
                    out = DialogOutcome::Cancel;
                }
                if let Some(e) = &st.err {
                    error_caption(ui, e);
                }
            });
        });
    if !open {
        return DialogOutcome::Cancel;
    }
    out
}
