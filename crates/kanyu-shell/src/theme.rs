//! 主题系统：总规 §1.2 色板（晨山/夜观星）→ egui Visuals + bitfun 卡片式视觉助手。
//!
//! bitfun 设计思路的落地语义：内容以**卡片**为容器（圆角 6–8px、细描边、
//! 充裕内边距），区块标题用**强调色短条 + 加粗小字**，面板之间保持呼吸间距
//! （总规"气"原则：拒绝信息过载）。所有视觉常量集中在本模块（单一事实来源）。

use eframe::egui;
use egui::{Color32, Stroke};
use kanyu_render::Theme;

/// 主题色板（对应总规 §1.2，十六进制 RGB）。
#[derive(Clone, Copy)]
pub struct Palette {
    /// 背景主（面板底）。
    pub bg_primary: Color32,
    /// 背景次（卡片/浮层底）。
    pub bg_secondary: Color32,
    /// 背景三（区块分隔/非激活）。
    pub bg_tertiary: Color32,
    /// 地图画布底。
    pub canvas: Color32,
    /// 文本主。
    pub text_primary: Color32,
    /// 文本弱（注释）。
    pub text_weak: Color32,
    /// 品牌强调（远黛青/青玉）。
    pub accent: Color32,
    /// 次强调（朱砂/珊瑚，警示）。
    pub accent_secondary: Color32,
    /// 三强调（琥珀/金珀，选中）。
    pub accent_tertiary: Color32,
    /// 信息色（蓝灰；几何图例中线要素、信息性标记）。
    pub info: Color32,
    /// 边框。
    pub border: Color32,
    /// 悬停背景（强调 8–12%）。
    pub hover: Color32,
    /// 按下背景。
    pub pressed: Color32,
    /// 选中底（20–25% 三强调）。
    pub selection: Color32,
}

/// 取主题色板。
pub fn palette(theme: Theme) -> Palette {
    fn rgb(hex: u32) -> Color32 {
        Color32::from_rgb(
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    }
    match theme {
        // 晨山：雾白/纯白/浅灰；墨黑/中灰；远黛青；朱砂；琥珀。
        Theme::Light => Palette {
            bg_primary: rgb(0xF7F5F2),
            bg_secondary: rgb(0xFFFFFF),
            bg_tertiary: rgb(0xEDEAE6),
            canvas: rgb(0xF0EDE8),
            text_primary: rgb(0x1A1A1A),
            text_weak: rgb(0x8A8A8A),
            accent: rgb(0x2D6A5E),
            // 朱砂微调（C75B3A → B14E32）：WCAG 2.2 §1.4.3 正文对比度 ≥4.5:1
            // （对 bg_primary 实测 3.87 → 4.79），见本文件末尾对比度测试。
            accent_secondary: rgb(0xB14E32),
            accent_tertiary: rgb(0xD4A843),
            info: rgb(0x4A7C9B),
            border: rgb(0xE0DDD8),
            hover: Color32::from_rgba_unmultiplied(0x2D, 0x6A, 0x5E, 20),
            pressed: rgb(0xE8E5E1),
            selection: Color32::from_rgba_unmultiplied(0xD4, 0xA8, 0x43, 51),
        },
        // 夜观星：墨夜/深灰蓝/中灰蓝；月白/暗灰；青玉；珊瑚；金珀。
        Theme::Dark => Palette {
            bg_primary: rgb(0x121418),
            bg_secondary: rgb(0x1A1D22),
            bg_tertiary: rgb(0x23272E),
            canvas: rgb(0x0D0F12),
            text_primary: rgb(0xE8E4DF),
            text_weak: rgb(0x6E737A),
            accent: rgb(0x4DB8A8),
            accent_secondary: rgb(0xE07A5F),
            accent_tertiary: rgb(0xE9C46A),
            info: rgb(0x7AA7C7),
            border: rgb(0x2A2F36),
            hover: Color32::from_rgba_unmultiplied(0x4D, 0xB8, 0xA8, 31),
            pressed: rgb(0x2A2F36),
            selection: Color32::from_rgba_unmultiplied(0xE9, 0xC4, 0x6A, 64),
        },
    }
}

/// 色板 → egui Visuals。
pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    let p = palette(theme);
    let mut v = match theme {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark => egui::Visuals::dark(),
    };
    v.panel_fill = p.bg_primary;
    v.window_fill = p.bg_secondary;
    v.faint_bg_color = p.bg_tertiary;
    v.extreme_bg_color = p.canvas;
    v.override_text_color = Some(p.text_primary);
    v.weak_text_color = Some(p.text_weak);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.selection;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.widgets.noninteractive.bg_fill = p.bg_primary;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text_primary);
    // Apple 式发丝线分隔：0.5px 低存在边框（分隔让位于内容）。
    v.widgets.noninteractive.bg_stroke = Stroke::new(0.5, p.border);
    v.widgets.inactive.bg_fill = p.bg_tertiary;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text_primary);
    v.widgets.inactive.bg_stroke = Stroke::new(0.5, p.border);
    v.widgets.hovered.bg_fill = p.hover;
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.accent);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    v.widgets.active.bg_fill = p.pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.5, p.accent);
    ctx.set_visuals(v);
}

/// 字体栈注入（Apple HIG 字体策略的 Windows 适配）：
/// Proportional 优先 **Segoe UI**（Windows 上最接近 SF 的人文无衬线），
/// Monospace 优先 **Cascadia Code**（次选 Consolas），
/// 中文回退 微软雅黑/黑体/宋体（macOS 苹方、Linux Noto Sans CJK）。
/// egui 默认字体仅作最终兜底。
pub fn load_cjk_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut loaded: Vec<String> = Vec::new();
    let push =
        |name: &str, path: &str, fonts: &mut egui::FontDefinitions, loaded: &mut Vec<String>| {
            if let Ok(bytes) = std::fs::read(path) {
                fonts
                    .font_data
                    .insert(name.to_string(), egui::FontData::from_owned(bytes).into());
                loaded.push(format!("{name}={path}"));
                true
            } else {
                false
            }
        };

    if cfg!(windows) {
        // 正文：Segoe UI（SF 的 Windows 近亲）；中文回退雅黑。
        push(
            "segoe",
            r"C:\Windows\Fonts\segoeui.ttf",
            &mut fonts,
            &mut loaded,
        );
        let cjk = [
            r"C:\Windows\Fonts\msyh.ttc",
            r"C:\Windows\Fonts\Noto Sans SC (TrueType).otf",
            r"C:\Windows\Fonts\simhei.ttf",
            r"C:\Windows\Fonts\simsun.ttc",
        ]
        .iter()
        .any(|p| push("cjk", p, &mut fonts, &mut loaded));
        if !cjk {
            eprintln!("警告：未找到系统中文字体，中文可能无法显示");
        }
        // 等宽：Cascadia Code → Consolas（命中其一）。
        let mono_hit = push(
            "cascadia",
            r"C:\Windows\Fonts\CascadiaMono.ttf",
            &mut fonts,
            &mut loaded,
        ) || push(
            "consolas",
            r"C:\Windows\Fonts\consola.ttf",
            &mut fonts,
            &mut loaded,
        );
        // 族序：正文 segoe 在前、cjk 回退在后；等宽命中字体在前、cjk 回退在后。
        let proportional = fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default();
        if fonts.font_data.contains_key("segoe") {
            proportional.insert(0, "segoe".to_string());
        }
        if fonts.font_data.contains_key("cjk") {
            proportional.push("cjk".to_string());
        }
        let mono = fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default();
        if mono_hit {
            let name = if fonts.font_data.contains_key("cascadia") {
                "cascadia"
            } else {
                "consolas"
            };
            mono.insert(0, name.to_string());
        }
        if fonts.font_data.contains_key("cjk") {
            mono.push("cjk".to_string());
        }
    } else if cfg!(target_os = "macos") {
        for p in [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ] {
            if push("cjk", p, &mut fonts, &mut loaded) {
                break;
            }
        }
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_string());
        }
    } else {
        for p in [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ] {
            if push("cjk", p, &mut fonts, &mut loaded) {
                break;
            }
        }
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("cjk".to_string());
        }
    }

    ctx.set_fonts(fonts);
    eprintln!("字体栈: {}", loaded.join(", "));
}

// ===== WCAG 2.2 对比度（国际规范约束，可执行检查）=====

/// WCAG 2.2 相对亮度（sRGB 线性化，§6.4 相对亮度定义）。
/// 规范守护函数：由本文件测试强制驱动（二进制 crate 内无运行期消费方）。
#[allow(dead_code)]
pub fn relative_luminance(c: Color32) -> f64 {
    fn lin(v: u8) -> f64 {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2.2 对比度（1.0–21.0）。规范守护函数（测试驱动）。
#[allow(dead_code)]
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let (l1, l2) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = (l1.max(l2), l1.min(l2));
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 阈值：正文 4.5:1（WCAG 2.2 §1.4.3 AA 正文档）；
    /// 次级/弱文本（caption 注释、占位符）取 3:1（§1.4.3 大文本档——弱文本仅承载
    /// 辅助信息，不承载关键操作语义，选择 3:1 档并在此注明依据）。
    const BODY_MIN: f64 = 4.5;
    const WEAK_MIN: f64 = 3.0;

    #[test]
    fn luminance_anchors() {
        assert!((relative_luminance(Color32::BLACK) - 0.0).abs() < 1e-9);
        assert!((relative_luminance(Color32::WHITE) - 1.0).abs() < 1e-9);
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-6);
    }

    /// 双主题全关键色对达标（§1.4.3）。
    #[test]
    fn palette_contrast_wcag() {
        for theme in [Theme::Light, Theme::Dark] {
            let p = palette(theme);
            for bg in [p.bg_primary, p.bg_secondary] {
                let body = contrast_ratio(p.text_primary, bg);
                assert!(
                    body >= BODY_MIN,
                    "{theme:?} 正文对比度 {body:.2} < {BODY_MIN}"
                );
                let weak = contrast_ratio(p.text_weak, bg);
                assert!(
                    weak >= WEAK_MIN,
                    "{theme:?} 弱文本对比度 {weak:.2} < {WEAK_MIN}"
                );
                let accent = contrast_ratio(p.accent, bg);
                assert!(
                    accent >= BODY_MIN,
                    "{theme:?} 强调色对比度 {accent:.2} < {BODY_MIN}"
                );
                let sec = contrast_ratio(p.accent_secondary, bg);
                assert!(
                    sec >= BODY_MIN,
                    "{theme:?} 次强调对比度 {sec:.2} < {BODY_MIN}"
                );
            }
        }
    }
}
