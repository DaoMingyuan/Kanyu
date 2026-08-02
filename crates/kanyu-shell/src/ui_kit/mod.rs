//! # UI 规范库（ui_kit）—— 堪舆桌面壳层设计系统
//!
//! ## 铁律（写进 AGENTS.md/AI_SYNC.md 的强制约定）
//!
//! 1. **先查后用**：新增任何界面元素前，先查本库是否已有对应组件，有则调用。
//! 2. **无则按类新建**：确无组件时，按本文件的分类标准新建**可复用**组件
//!    （命名/参数/状态契约与本库一致），禁止在业务代码里一次性手搓样式。
//! 3. **样式不出库**：颜色/字号/间距/圆角只允许来自 [`tokens`] 与
//!    [`crate::theme::palette`]，业务代码不得出现硬编码色值与魔法数字。
//!
//! ## 分类标准
//!
//! | 类别 | 模块 | 组件 |
//! |------|------|------|
//! | 设计令牌 | [`tokens`] | 间距/圆角/控件高度/文本分级 |
//! | 基础控件 | [`controls`] | KButton(四变体)、KIconButton、KTextInput、KCombo、KCheckbox |
//! | 容器组件 | [`containers`] | KCard、KSectionHeader、KDialogShell、KBadge |
//! | 业务组件 | 各面板模块 | 图层条目、终端行、ribbon 组（基于基础控件组合） |
//!
//! ## 组件契约
//!
//! - 每个控件返回 `egui::Response`（或其元组），禁用态经 `enabled: bool` 参数；
//! - 表单类组件只负责呈现与采集，**验证/执行归调用方**（错误文本经
//!   [`controls::error_caption`] 统一红字呈现）；
//! - 文本分级只允许用 [`tokens::text`] 的七级，不得自定义字号；
//! - 新增组件必须：中文 rustdoc + 归类的模块 + 状态示例（在文档注释中）。
//!
//! 注：作为组件库，部分组件为后续功能预留（dead_code 豁免是设计系统的常态，
//! 不代表冗余——组件一经注册即属规范资产）。

#![allow(dead_code)]
#![allow(unused_imports)] // 组件库保持完整公共面，未被当前业务消费的组件属规范资产。

pub mod containers;
pub mod controls;
pub mod icons;
pub mod tokens;

pub use containers::{badge, card, dialog_shell, section_header, DialogAction};
pub use controls::{
    button, checkbox, combo, combo_static, error_caption, hint_caption, icon_button, layer_picker,
    password_input, ribbon_button, tab_strip, text_area, text_input, tree_row, ButtonVariant,
};
pub use icons::{icon_ui, icons_color, Icon};
pub use tokens::{sizes, spacing, text};
