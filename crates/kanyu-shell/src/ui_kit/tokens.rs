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
    /// 小控件高（工具条按钮、徽章）。
    pub const CONTROL_SM: f32 = 22.0;
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

/// 文本分级（Apple HIG 字号层级适配桌面端：Large Title 28 / Title2 22 /
/// Headline 17sb / Subhead 15 / Footnote 13 / Caption2 11 / 数据等宽 12；
/// 行内引用 HIG Type Scale，比例关系与 iOS 一致，绝对值按桌面密度下调）。
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
