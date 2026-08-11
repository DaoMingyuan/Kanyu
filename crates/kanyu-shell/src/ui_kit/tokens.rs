//! 设计令牌：间距 / 圆角 / 控件高度 / 文本分级（对应总规 §1.3 与 §2.1）。
//!
//! 所有 UI 代码只许引用本模块常量，禁止魔法数字。

/// 间距标尺（总规 §2.1：4px 基值）。
pub mod spacing {
    /// 4px —— 紧凑间距（图标与文字间）。
    pub const XS: f32 = 4.0;
    /// 8px —— 小间距（表单项间）。
    pub const SM: f32 = 8.0;
    /// 12px —— 标准间距（卡片内边距、区块内）。
    pub const MD: f32 = 12.0;
    /// 16px —— 大间距（区块间）。
    pub const LG: f32 = 16.0;
    /// 24px —— 面板级间距。
    pub const XL: f32 = 24.0;
    /// 32px —— 页面级间距。
    pub const XXL: f32 = 32.0;
}

/// 圆角标尺（Apple HIG 连续圆角语义：控件 6 / 卡片 10 / 大浮层 14）。
pub mod radius {
    use eframe::egui::CornerRadius;
    /// 6px —— 控件（按钮、输入框、徽章）。
    pub const SM: CornerRadius = CornerRadius::same(6);
    /// 10px —— 卡片、对话框。
    pub const MD: CornerRadius = CornerRadius::same(10);
    /// 14px —— 大型浮层。
    pub const LG: CornerRadius = CornerRadius::same(14);
}

/// 控件尺寸（高度，px）。
pub mod sizes {
    /// 小控件高（工具条按钮、徽章、树行）。
    /// WCAG 2.2 §2.5.8 指针目标 ≥24px——桌面端取 24px 达标档；
    /// 更小的纯图标按钮须保证 24px 间距圆不重叠（§2.5.8 Spacing 豁免）。
    pub const CONTROL_SM: f32 = 24.0;
    /// 标准控件高（按钮、输入框、下拉框）。
    pub const CONTROL_MD: f32 = 28.0;
    /// 大控件高（主操作按钮）。
    pub const CONTROL_LG: f32 = 36.0;
    /// 标题栏高。
    pub const TITLE_BAR: f32 = 40.0;
    /// 状态栏高。
    pub const STATUS_BAR: f32 = 28.0;
    /// 功能区（ribbon）总高（QAT 26 + 页签 24 + 分隔呼吸 6 + 按钮与组名 70）。
    pub const RIBBON: f32 = 126.0;
    /// 标准输入框宽。
    pub const INPUT_W: f32 = 240.0;
    /// 终端面板默认高。
    pub const CONSOLE_H: f32 = 180.0;
}

/// 动画参数（egui 原生 `animate_*` 驱动；图标/下划线过渡的唯一事实来源）。
pub mod animation {
    /// 悬停/选中过渡时长（秒）。
    pub const HOVER_SECS: f32 = 0.12;
    /// 悬停图标放大倍率。
    pub const HOVER_SCALE: f32 = 1.12;
    /// 悬停图标上移量（px，≤1px 微位移）。
    pub const HOVER_LIFT: f32 = 1.0;
    /// 按下图标缩小倍率。
    pub const PRESS_SCALE: f32 = 0.92;
    /// Toast 轻提示停留时长（秒，含淡出）。
    pub const TOAST_SECS: f32 = 3.0;

    /// 图标动画缩放（纯函数）：悬停插值放大（hover_t∈[0,1]，越界钳制），
    /// 按下优先缩至 [`PRESS_SCALE`]。
    pub fn icon_scale(hover_t: f32, pressed: bool) -> f32 {
        if pressed {
            return PRESS_SCALE;
        }
        1.0 + (HOVER_SCALE - 1.0) * hover_t.clamp(0.0, 1.0)
    }

    /// 图标动画上移量（px，随 hover_t 插值，钳制）。
    pub fn icon_lift(hover_t: f32) -> f32 {
        HOVER_LIFT * hover_t.clamp(0.0, 1.0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn constants_are_sane() {
            const {
                assert!(HOVER_SECS > 0.0 && HOVER_SECS <= 0.5);
                assert!(HOVER_SCALE > 1.0 && HOVER_SCALE <= 1.25);
                assert!(PRESS_SCALE < 1.0 && PRESS_SCALE >= 0.8);
                assert!(HOVER_LIFT <= 1.0 && HOVER_LIFT >= 0.0);
            }
        }

        #[test]
        fn icon_scale_clamps_and_press_wins() {
            assert_eq!(icon_scale(0.0, false), 1.0);
            assert_eq!(icon_scale(1.0, false), HOVER_SCALE);
            assert_eq!(icon_scale(2.5, false), HOVER_SCALE); // 越界钳制
            assert_eq!(icon_scale(-1.0, false), 1.0);
            assert_eq!(icon_scale(0.5, true), PRESS_SCALE); // 按下优先
        }

        #[test]
        fn icon_lift_clamps() {
            assert_eq!(icon_lift(0.0), 0.0);
            assert_eq!(icon_lift(1.0), HOVER_LIFT);
            assert_eq!(icon_lift(9.0), HOVER_LIFT);
        }
    }
}

/// 状态色派生（WCAG/设计规范固化取值规则；消费方一律用本模块函数，
/// 禁止业务代码临时取色）。
pub mod state {
    use eframe::egui::{Color32, Stroke};

    use crate::theme::Palette;

    /// 悬停底（强调色 8–12% 透明度；palette.hover 同语义，此为规范入口）。
    pub fn hover_bg(p: &Palette) -> Color32 {
        p.hover
    }
    /// 按下底（强调色 16–20% 透明度派生）。
    pub fn pressed_bg(p: &Palette) -> Color32 {
        with_alpha(p.accent, 0.18)
    }
    /// 选中底（三强调 20–25% 透明度；palette.selection 同语义）。
    pub fn selection_bg(p: &Palette) -> Color32 {
        p.selection
    }
    /// 焦点描边（强调色 1.5px）。
    pub fn focus_stroke(p: &Palette) -> Stroke {
        Stroke::new(1.5, p.accent)
    }
    /// 禁用文本（text_primary 45% 透明度派生）。
    pub fn disabled_text(p: &Palette) -> Color32 {
        p.text_disabled
    }
    /// 禁用底（bg_tertiary 60% 透明度派生）。
    pub fn disabled_bg(p: &Palette) -> Color32 {
        p.bg_disabled
    }

    /// 透明度换算（0.0–1.0 → 预乘 alpha 通道；派生统一入口）。
    fn with_alpha(c: Color32, alpha: f32) -> Color32 {
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha * 255.0) as u8)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::theme::palette;

        #[test]
        fn state_derivations_match_rules() {
            for t in [kanyu_render::Theme::Light, kanyu_render::Theme::Dark] {
                let p = palette(t);
                // 悬停 = 强调 8–12% alpha。
                let h = hover_bg(&p);
                assert!(h.a() > 15 && h.a() < 40, "hover alpha {} 越界", h.a());
                // 按下 = 强调 16–20% alpha（Color32 内部预乘存储，只验 alpha）。
                let pr = pressed_bg(&p);
                assert!((pr.a() as f32 / 255.0 - 0.18).abs() < 0.01);
                // 选中 = 三强调 20–25% alpha。
                let s = selection_bg(&p);
                assert!(s.a() >= 50 && s.a() <= 64, "selection alpha {} 越界", s.a());
                // 焦点描边 = 强调 1.5px。
                assert_eq!(focus_stroke(&p).width, 1.5);
            }
        }
    }
}

/// 文本分级（Apple HIG 字号层级适配桌面端：Large Title 28 / Title2 22 /
/// Headline 17sb / Subhead 15 / Footnote 13 / Caption2 11 / 数据等宽 12；
/// 行内引用 HIG Type Scale，比例关系与 iOS 一致，绝对值按桌面密度下调）。
/// 最小有效字号：caption 11px 为 100% 缩放档下限（桌面可读性下限约 11px；
/// 低于此值的信息须改分级或放大——缩放档经 egui zoom_factor 等比放大，不破坏下限）。
pub mod text {
    use eframe::egui::{self, RichText};

    /// display-xl 字号（28px，HIG Large Title 桌面适配：启动页/品牌大标题）。
    pub const SIZE_DISPLAY_XL: f32 = 28.0;
    /// display-lg 字号（22px，HIG Title 2 桌面适配：面板大标题）。
    pub const SIZE_DISPLAY_LG: f32 = 22.0;
    /// heading 字号（17px 半粗，HIG Headline：区块标题）。
    pub const SIZE_HEADING: f32 = 17.0;
    /// body-lg 字号（15px，HIG Subheadline：正文/属性值）。
    pub const SIZE_BODY_LG: f32 = 15.0;
    /// body 字号（13px，HIG Footnote：标签/按钮）。
    pub const SIZE_BODY: f32 = 13.0;
    /// caption 字号（11px，HIG Caption 2：注释/状态栏）。
    pub const SIZE_CAPTION: f32 = 11.0;
    /// data 字号（12px 等宽，坐标/ID/终端）。
    pub const SIZE_DATA: f32 = 12.0;

    /// display-xl 28px —— 启动页/品牌大标题。
    pub fn display_xl(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_DISPLAY_XL).strong()
    }
    /// display-lg 22px —— 面板大标题。
    pub fn display_lg(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_DISPLAY_LG).strong()
    }
    /// heading 17px 半粗 —— 区块标题。
    pub fn heading(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_HEADING).strong()
    }
    /// body-lg 15px —— 正文、属性值。
    pub fn body_lg(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_BODY_LG)
    }
    /// body 13px —— 标签、按钮。
    pub fn body(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_BODY)
    }
    /// caption 11px —— 注释、状态栏。
    pub fn caption(t: impl Into<String>) -> RichText {
        RichText::new(t.into()).size(SIZE_CAPTION)
    }
    /// data 12px 等宽 —— 坐标、ID、命令、终端。
    pub fn data(t: impl Into<String>) -> RichText {
        RichText::new(t.into())
            .size(SIZE_DATA)
            .family(egui::FontFamily::Monospace)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn levels_are_monotonic() {
            let sizes = [
                SIZE_DISPLAY_XL,
                SIZE_DISPLAY_LG,
                SIZE_HEADING,
                SIZE_BODY_LG,
                SIZE_BODY,
                SIZE_DATA,
                SIZE_CAPTION,
            ];
            for w in sizes.windows(2) {
                assert!(w[0] >= w[1], "字号层级必须单调不增: {sizes:?}");
            }
        }

        #[test]
        fn hig_scale_values() {
            // HIG 适配值的锚点断言（防手滑改动基准）。
            assert_eq!(SIZE_HEADING, 17.0);
            assert_eq!(SIZE_BODY, 13.0);
            assert_eq!(SIZE_CAPTION, 11.0);
            const {
                assert!(SIZE_DISPLAY_XL > SIZE_DISPLAY_LG && SIZE_DISPLAY_LG > SIZE_HEADING);
            }
        }
    }
}
