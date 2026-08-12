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
//! ## 设计审查规范（design-review 技能沉淀，App UI 规则）
//!
//! 每次 UI 改动对照以下清单（源自 gstack design-review 技能，适配 egui 桌面端）：
//!
//! - **信息层级**：每个区域一个主角；信息密度"紧凑但可读"；装饰让位于内容
//!   （卡片只在"卡片即交互"时使用，如对话框/消息气泡/设置卡）。
//! - **间距节奏**：只许 tokens::spacing 标尺（4/8/12/16/24/32）；相关项更近、
//!   分节更远；边距即呼吸（面板/组边缘必须留白）。
//! - **文本分级**：只许 tokens::text 七级；正文 ≥13px、注释 ≥11px、
//!   数据/坐标用等宽（tabular 语义）；弱文本只用于注释。
//! - **色彩语义**：强调色唯一（远黛青/青玉）；语义色固定（成功=青、警告=琥珀、
//!   错误=朱砂、信息=蓝灰）；禁纯紫渐变等 AI slop 配色；暗色=层级（elevation）
//!   非简单反色。
//! - **交互状态**：可点必有悬停反馈；禁用必降饱和；危险操作必二次确认或
//!   Danger 变体；触控目标 ≥ 22px（桌面）/ 44px（触屏，总规 §2.1）。
//! - **对比度（WCAG 2.2 §1.4.3，强制）**：正文文本对背景 ≥ 4.5:1，弱文本
//!   ≥ 3:1；由 `theme.rs` 的 `contrast_ratio` 单元测试强制守护——调色板
//!   改动必须跑测试，不得引入不达标色对。
//! - **目标尺寸（WCAG 2.2 §2.5.8，强制）**：可点目标 ≥ 24px
//!   （`tokens::sizes::CONTROL_SM` 即 24 达标档；更小的纯图标按钮必须满足
//!   §2.5.8 Spacing 豁免——24px 间距圆互不重叠）。
//! - **缩放等比例（强制）**：字号/图标/间距一律出自 tokens（点单位），
//!   界面缩放（`egui zoom_factor` 档位）后所有元素等比缩放；禁止绕过
//!   tokens 的硬编码像素（自绘元素同）。
//! - **状态色（强制）**：hover/pressed/selection/focus/disabled 只许出自
//!   palette 语义位与 `tokens::state` 派生函数（取值规则固化在该模块注释），
//!   禁止业务代码临时取色。
//! - **三分离原则**（功能区）：图标归图标（视觉）、标题归标题（按钮内）、
//!   简介归简介（**悬停浮现**，永不挤占版面）。
//! - **AI slop 黑名单**（适配桌面端）：图标彩圈装饰、万物居中、统一大圆角、
//!   装饰性色块/波浪线、emoji 当设计元素、彩色左边条卡片、样板化三列特性栅格。
//!
//! ## 分类标准
//!
//! | 类别 | 模块 | 组件 |
//! |------|------|------|
//! | 设计令牌 | [`tokens`] | 间距/圆角/控件高度/文本分级 |
//! | 基础控件 | [`controls`] | KButton(四变体)、KIconButton、KTextInput、KCombo、KCheckbox、ribbon_button、tab_strip、tree_row、toc_row、menu_button、spinner、password_input |
//! | 容器组件 | [`containers`] | KCard、KSectionHeader、KDialogShell、KBadge、toast 轻提示栈 |
//! | 图标系统 | [`icons`] | 33 枚线性图标（stroke 1.5px、几何极简，总规 §1.4） |
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

pub use containers::{
    badge, card, dialog_shell, push_toast, section_header, toast_stack, BadgeLevel, DialogAction,
    Toast, ToastKind,
};
pub use controls::{
    button, checkbox, clamp_step, combo, combo_static, combo_width, error_caption, hint_caption,
    icon_button, layer_picker, menu_button, password_input, ribbon_button, spinner, tab_strip,
    text_area, text_input, toc_row, tree_row, tree_row_weak, ButtonVariant, MenuButtonResponse,
    TocRowResponse,
};
pub use icons::{icon_ui, icons_color, Icon};
pub use tokens::{sizes, spacing, state, text};
