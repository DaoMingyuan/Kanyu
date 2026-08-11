//! 打印布局排版器（ArcGIS Pro Layout 对应物，v1）：SVG 组合输出 + tiny-skia PNG。
//!
//! 页面排版：页边距 + 标题（顶部居中）+ 地图框（主体，嵌入渲染产物）+
//! 图例（右侧栏：色块+标注行）+ 比例尺（地图框下沿，1:xxxx 取整档标注 +
//! 分段条）+ 指北针（右上角 N 箭头）。
//!
//! - **SVG**：完整排版（文字齐全；地图经嵌套 `<svg>` 内嵌）；
//! - **PNG**：tiny-skia 光栅链复用；文字仅 ASCII 迷你点阵（比例尺数字与 N），
//!   标题/图例文字在 PNG 省略（完整文字请导出 SVG——内嵌字体栈属后续项）。
//!
//! 页面换算与比例尺取整为纯函数（[`LayoutFrame::compute`]/[`nice_scale`]），配单测。

use crate::RenderError;

/// 纸张（毫米）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    /// A4 横（297×210mm）。
    A4Landscape,
    /// A4 纵（210×297mm）。
    A4Portrait,
}

impl PageSize {
    /// 毫米尺寸（宽, 高）。
    pub fn mm(self) -> (f64, f64) {
        match self {
            PageSize::A4Landscape => (297.0, 210.0),
            PageSize::A4Portrait => (210.0, 297.0),
        }
    }
    /// 像素尺寸（dpi 换算：mm / 25.4 × dpi）。
    pub fn pixels(self, dpi: f64) -> (u32, u32) {
        let (w, h) = self.mm();
        (
            (w / 25.4 * dpi).round().clamp(1.0, 8192.0) as u32,
            (h / 25.4 * dpi).round().clamp(1.0, 8192.0) as u32,
        )
    }
}

/// 布局规格。
#[derive(Debug, Clone)]
pub struct LayoutSpec {
    /// 纸张。
    pub page: PageSize,
    /// 分辨率（默认 96）。
    pub dpi: f64,
    /// 标题（顶部居中；空串不绘）。
    pub title: String,
    /// 图例开关。
    pub show_legend: bool,
    /// 比例尺开关。
    pub show_scalebar: bool,
    /// 指北针开关。
    pub show_north: bool,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            page: PageSize::A4Landscape,
            dpi: 96.0,
            title: String::new(),
            show_legend: true,
            show_scalebar: true,
            show_north: true,
        }
    }
}

/// 排版帧几何（像素，纯计算）。
#[derive(Debug, Clone, Copy)]
pub struct LayoutFrame {
    /// 页宽/高。
    pub page_w: f64,
    /// 页高。
    pub page_h: f64,
    /// 页边距。
    pub margin: f64,
    /// 标题区高（无标题为 0）。
    pub title_h: f64,
    /// 图例栏宽（无图例为 0）。
    pub legend_w: f64,
    /// 地图框 `[x, y, w, h]`。
    pub map: [f64; 4],
}

impl LayoutFrame {
    /// 计算排版帧（标题 40px、图例栏 180px、边距 48px，均随 dpi 缩放）。
    pub fn compute(spec: &LayoutSpec) -> Self {
        let (pw, ph) = spec.page.pixels(spec.dpi);
        let k = spec.dpi / 96.0; // 96dpi 基准的缩放系数
        let (pw, ph) = (f64::from(pw), f64::from(ph));
        let margin = 48.0 * k;
        let title_h = if spec.title.is_empty() { 0.0 } else { 40.0 * k };
        let legend_w = if spec.show_legend { 180.0 * k } else { 0.0 };
        let map_x = margin;
        let map_y = margin + title_h;
        let map_w = (pw - margin * 2.0 - legend_w).max(10.0);
        let map_h = (ph - margin * 2.0 - title_h).max(10.0);
        Self {
            page_w: pw,
            page_h: ph,
            margin,
            title_h,
            legend_w,
            map: [map_x, map_y, map_w, map_h],
        }
    }
}

/// 比例尺取整档（1/2/5 × 10^n；返回 ≤ raw 的最大整档）。
pub fn nice_scale_step(raw: f64) -> f64 {
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    let mantissa = raw / base;
    let m = if mantissa >= 5.0 {
        5.0
    } else if mantissa >= 2.0 {
        2.0
    } else {
        1.0
    };
    m * base
}

/// 比例尺标注（纯函数）。
///
/// `span_m` = 视口东西跨度（米）；`map_w_px` = 地图框像素宽；`dpi` 分辨率。
/// 返回（标注文本 "1:N"，比例条像素长，条代表米数）：条长取整档（≤ 地图框宽 25%）。
pub fn nice_scale(span_m: f64, map_w_px: f64, dpi: f64) -> (String, f64, f64) {
    let map_w_m = map_w_px / dpi * 0.0254; // 地图框物理宽（米）
    let denominator = if map_w_m > 0.0 { span_m / map_w_m } else { 1.0 };
    // 条长候选：跨度 25% 内的最大整档。
    let bar_m = nice_scale_step(span_m * 0.25);
    let bar_px = if span_m > 0.0 {
        bar_m / span_m * map_w_px
    } else {
        0.0
    };
    // 分母取 2 位有效数字。
    let exp = denominator.max(1.0).log10().floor();
    let rounded = (denominator / 10f64.powf(exp - 1.0)).round() * 10f64.powf(exp - 1.0);
    (format!("1:{}", rounded as u64), bar_px, bar_m)
}

/// 图例行（色块 + 标注）。
pub struct LegendRow {
    /// 色块 RGB。
    pub color: [u8; 3],
    /// 标注。
    pub label: String,
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// 文本 XML 转义。
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 渲染布局为 SVG（完整排版；`map_svg` 为 render_svg 产物，经嵌套 `<svg>` 内嵌）。
pub fn render_layout_svg(
    spec: &LayoutSpec,
    map_svg: &str,
    legend: &[LegendRow],
    scale: Option<(&str, f64)>, // (标注, 条长 px)
) -> String {
    let f = LayoutFrame::compute(spec);
    let (w, h) = (f.page_w, f.page_h);
    let mut out = String::with_capacity(4096 + map_svg.len());
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" width=\"{w:.0}\" height=\"{h:.0}\">\n"
    ));
    out.push_str("<!-- kanyu-render layout -->\n");
    // 页面白底。
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#FFFFFF\"/>\n");
    let (mx, my, mw, mh) = (f.map[0], f.map[1], f.map[2], f.map[3]);
    // 标题。
    if f.title_h > 0.0 {
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" text-anchor=\"middle\" fill=\"#1A1A1A\">{}</text>\n",
            w / 2.0,
            f.margin + f.title_h * 0.62,
            f.title_h * 0.5,
            esc(&spec.title)
        ));
    }
    // 地图框（嵌套 svg 内嵌地图）。
    out.push_str(&format!(
        "<rect x=\"{mx:.1}\" y=\"{my:.1}\" width=\"{mw:.1}\" height=\"{mh:.1}\" fill=\"#FFFFFF\" stroke=\"#E0DDD8\" stroke-width=\"1\"/>\n"
    ));
    out.push_str(&format!(
        "<svg x=\"{mx:.1}\" y=\"{my:.1}\" width=\"{mw:.1}\" height=\"{mh:.1}\" viewBox=\"0 0 {mw:.0} {mh:.0}\">\n"
    ));
    // 内嵌地图内容（调用方给完整 <svg> 文档或 <image> 元素，均直接内嵌；
    // 嵌套 <svg> 为合法 SVG 结构）。
    out.push_str(map_svg);
    out.push('\n');
    out.push_str("</svg>\n");
    // 图例栏。
    if spec.show_legend && !legend.is_empty() {
        let lx = mx + mw + 12.0;
        let mut ly = my + 8.0;
        for row in legend {
            out.push_str(&format!(
                "<rect x=\"{lx:.1}\" y=\"{ly:.1}\" width=\"12\" height=\"12\" rx=\"2\" fill=\"{}\"/>\n",
                hex(row.color)
            ));
            out.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" fill=\"#1A1A1A\">{}</text>\n",
                lx + 18.0,
                ly + 10.0,
                esc(&row.label)
            ));
            ly += 20.0;
        }
    }
    // 比例尺（地图框下沿）：分段条 + 标注。
    if let (Some((label, bar_px)), true) = (scale, spec.show_scalebar) {
        let sx = mx;
        let sy = my + mh + 18.0;
        let half = bar_px / 2.0;
        out.push_str(&format!(
            "<rect x=\"{sx:.1}\" y=\"{sy:.1}\" width=\"{half:.1}\" height=\"5\" fill=\"#1A1A1A\"/>\n"
        ));
        out.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{sy:.1}\" width=\"{half:.1}\" height=\"5\" fill=\"none\" stroke=\"#1A1A1A\" stroke-width=\"1\"/>\n",
            sx + half
        ));
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"12\" fill=\"#1A1A1A\">{}</text>\n",
            sx + bar_px + 8.0,
            sy + 5.0,
            esc(label)
        ));
    }
    // 指北针（右上角 N 箭头）。
    if spec.show_north {
        let nx = w - f.margin / 2.0;
        let ny = f.margin / 2.0;
        let (top_y, left_x, right_x, bot_y) = (ny - 12.0, nx - 6.0, nx + 6.0, ny + 6.0);
        out.push_str(&format!(
            "<polygon points=\"{nx:.1},{top_y:.1} {left_x:.1},{bot_y:.1} {right_x:.1},{bot_y:.1}\" fill=\"#1A1A1A\"/>\n"
        ));
        out.push_str(&format!(
            "<text x=\"{nx:.1}\" y=\"{:.1}\" font-size=\"11\" text-anchor=\"middle\" fill=\"#1A1A1A\">N</text>\n",
            ny + 18.0
        ));
    }
    out.push_str("</svg>\n");
    out
}

/// 渲染布局为 PNG（tiny-skia 复用链；`map_png` 为 render_png 产物）。
/// 文字仅 ASCII 迷你点阵（比例尺数字与 N；标题/图例文字 PNG 省略，见模块头）。
pub fn render_layout_png(
    spec: &LayoutSpec,
    map_png: &[u8],
    legend: &[LegendRow],
    scale: Option<(&str, f64)>,
) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{Color, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};

    let f = LayoutFrame::compute(spec);
    let (pw, ph) = spec.page.pixels(spec.dpi);
    let mut page = Pixmap::new(pw, ph).ok_or(RenderError::InvalidSize(pw, ph))?;
    page.fill(Color::from_rgba8(255, 255, 255, 255));

    // 地图：解码后缩放贴入地图框。
    let map_pix = tiny_skia::Pixmap::decode_png(map_png)
        .map_err(|e| RenderError::InvalidStyle(format!("地图 PNG 解码失败: {e}")))?;
    let (mx, my, mw, mh) = (f.map[0], f.map[1], f.map[2], f.map[3]);
    let sx = mw / f64::from(map_pix.width());
    let sy = mh / f64::from(map_pix.height());
    page.draw_pixmap(
        mx as i32,
        my as i32,
        map_pix.as_ref(),
        &PixmapPaint::default(),
        Transform::from_scale(sx as f32, sy as f32),
        None,
    );
    // 地图框描边。
    let stroke_paint = Paint {
        shader: tiny_skia::Shader::SolidColor(Color::from_rgba8(0xE0, 0xDD, 0xD8, 255)),
        anti_alias: true,
        ..Default::default()
    };
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(mx as f32, my as f32, mw as f32, mh as f32).unwrap());
    let path = pb.finish().ok_or(RenderError::InvalidSize(pw, ph))?;
    let stroke = tiny_skia::Stroke {
        width: 1.0,
        ..Default::default()
    };
    page.stroke_path(&path, &stroke_paint, &stroke, Transform::default(), None);

    // 图例色块（文字 PNG 省略，见模块头）。
    if spec.show_legend {
        let lx = (mx + mw + 12.0) as f32;
        let mut ly = (my + 8.0) as f32;
        for row in legend {
            let mut pb = PathBuilder::new();
            pb.push_rect(Rect::from_xywh(lx, ly, 12.0, 12.0).unwrap());
            if let Some(path) = pb.finish() {
                page.fill_path(
                    &path,
                    &Paint {
                        shader: tiny_skia::Shader::SolidColor(Color::from_rgba8(
                            row.color[0],
                            row.color[1],
                            row.color[2],
                            255,
                        )),
                        anti_alias: true,
                        ..Default::default()
                    },
                    tiny_skia::FillRule::Winding,
                    Transform::default(),
                    None,
                );
            }
            ly += 20.0;
        }
    }
    // 比例尺分段条 + ASCII 数字。
    if let (Some((label, bar_px)), true) = (scale, spec.show_scalebar) {
        let sy = (my + mh + 18.0) as f32;
        let dark = Paint {
            shader: tiny_skia::Shader::SolidColor(Color::from_rgba8(0x1A, 0x1A, 0x1A, 255)),
            anti_alias: true,
            ..Default::default()
        };
        let half = (bar_px / 2.0) as f32;
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(mx as f32, sy, half, 5.0).unwrap());
        if let Some(path) = pb.finish() {
            page.fill_path(
                &path,
                &dark,
                tiny_skia::FillRule::Winding,
                Transform::default(),
                None,
            );
        }
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(mx as f32 + half, sy, half, 5.0).unwrap());
        if let Some(path) = pb.finish() {
            page.stroke_path(&path, &dark, &stroke, Transform::default(), None);
        }
        draw_ascii(
            &mut page,
            label,
            mx as f32 + bar_px as f32 + 8.0,
            sy + 5.0,
            2.0,
            Color::from_rgba8(0x1A, 0x1A, 0x1A, 255),
        );
    }
    // 指北针（N 箭头 + N）。
    if spec.show_north {
        let nx = f.page_w - f.margin / 2.0;
        let ny = f.margin / 2.0;
        let mut pb = PathBuilder::new();
        pb.move_to(nx as f32, (ny - 12.0) as f32);
        pb.line_to((nx - 6.0) as f32, (ny + 6.0) as f32);
        pb.line_to((nx + 6.0) as f32, (ny + 6.0) as f32);
        pb.close();
        if let Some(path) = pb.finish() {
            page.fill_path(
                &path,
                &Paint {
                    shader: tiny_skia::Shader::SolidColor(Color::from_rgba8(0x1A, 0x1A, 0x1A, 255)),
                    anti_alias: true,
                    ..Default::default()
                },
                tiny_skia::FillRule::Winding,
                Transform::default(),
                None,
            );
        }
        draw_ascii(
            &mut page,
            "N",
            (nx - 3.5) as f32,
            (ny + 16.0) as f32,
            2.0,
            Color::from_rgba8(0x1A, 0x1A, 0x1A, 255),
        );
    }
    page.encode_png()
        .map_err(|e| RenderError::InvalidStyle(format!("布局 PNG 编码失败: {e}")))
}

/// ASCII 迷你点阵（5×7 字体子集：数字/冒号/点/N；比例尺与指北针用）。
fn draw_ascii(
    page: &mut tiny_skia::Pixmap,
    text: &str,
    x: f32,
    y: f32,
    scale: f32,
    color: tiny_skia::Color,
) {
    let mut cx = x;
    for ch in text.chars() {
        if let Some(glyph) = glyph5x7(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        let mut pb = tiny_skia::PathBuilder::new();
                        pb.push_rect(
                            tiny_skia::Rect::from_xywh(
                                cx + col as f32 * scale,
                                y + row as f32 * scale,
                                scale,
                                scale,
                            )
                            .unwrap(),
                        );
                        if let Some(path) = pb.finish() {
                            page.fill_path(
                                &path,
                                &tiny_skia::Paint {
                                    shader: tiny_skia::Shader::SolidColor(color),
                                    anti_alias: false,
                                    ..Default::default()
                                },
                                tiny_skia::FillRule::Winding,
                                tiny_skia::Transform::default(),
                                None,
                            );
                        }
                    }
                }
            }
        }
        cx += 6.0 * scale;
    }
}

/// 5×7 字形（行位图，MSB 左）。
fn glyph5x7(c: char) -> Option<[u8; 7]> {
    Some(match c {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        ':' => [0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        '-' => [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_pixels_96dpi() {
        assert_eq!(PageSize::A4Landscape.pixels(96.0), (1123, 794));
        assert_eq!(PageSize::A4Portrait.pixels(96.0), (794, 1123));
        // 144dpi 1.5 倍。
        assert_eq!(PageSize::A4Landscape.pixels(144.0), (1684, 1191));
    }

    #[test]
    fn frame_layout_reserves_zones() {
        let spec = LayoutSpec {
            title: "测试".into(),
            ..Default::default()
        };
        let f = LayoutFrame::compute(&spec);
        assert!(f.title_h > 0.0 && f.legend_w > 0.0);
        assert!(f.map[2] > 0.0 && f.map[3] > 0.0);
        // 地图框不超页宽。
        assert!(f.map[0] + f.map[2] + f.legend_w <= f.page_w);
        // 无标题无图例：地图框变大。
        let f2 = LayoutFrame::compute(&LayoutSpec {
            title: String::new(),
            show_legend: false,
            ..Default::default()
        });
        assert_eq!(f2.title_h, 0.0);
        assert!(f2.map[2] > f.map[2]);
    }

    #[test]
    fn nice_scale_steps_and_label() {
        assert_eq!(nice_scale_step(900.0), 500.0);
        assert_eq!(nice_scale_step(1200.0), 1000.0);
        assert_eq!(nice_scale_step(3700.0), 2000.0);
        assert_eq!(nice_scale_step(0.0), 1.0);
        // 0.02° 跨度（约 2226 米）于 800px@96dpi：分母约 1:10500 档。
        let (label, bar_px, bar_m) = nice_scale(0.02 * 111320.0, 800.0, 96.0);
        assert!(label.starts_with("1:"));
        assert!(bar_px > 0.0 && bar_px <= 800.0 * 0.3);
        assert_eq!(bar_m, nice_scale_step(0.02 * 111320.0 * 0.25));
    }

    #[test]
    fn svg_contains_all_parts() {
        let spec = LayoutSpec {
            title: "示范布局".into(),
            ..Default::default()
        };
        let legend = vec![
            LegendRow {
                color: [0x2D, 0x6A, 0x5E],
                label: "≤ 50".into(),
            },
            LegendRow {
                color: [0xD4, 0xA8, 0x43],
                label: "> 50".into(),
            },
        ];
        let svg = render_layout_svg(
            &spec,
            "<svg viewBox=\"0 0 10 10\">\n<rect width=\"10\" height=\"10\"/>\n</svg>\n",
            &legend,
            Some(("1:10000", 120.0)),
        );
        assert!(svg.contains("示范布局"), "标题");
        assert!(svg.contains("#2D6A5E"), "图例色块");
        assert!(svg.contains("≤ 50"), "图例标注");
        assert!(svg.contains("1:10000"), "比例尺标注");
        assert!(svg.contains(">N<"), "指北针");
        assert!(svg.ends_with("</svg>\n"));
        // 转义。
        let svg2 = render_layout_svg(&spec, "", &[], None);
        let _ = svg2;
        let spec3 = LayoutSpec {
            title: "a<b>".into(),
            ..Default::default()
        };
        assert!(render_layout_svg(&spec3, "", &[], None).contains("a&lt;b&gt;"));
    }

    #[test]
    fn png_layout_encodes() {
        // 最小地图 PNG（render_png 空集合）。
        let map = crate::render_png(
            &geojson::FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
            &crate::RenderOptions::default(),
        )
        .unwrap();
        let spec = LayoutSpec::default();
        let png = render_layout_png(
            &spec,
            &map,
            &[LegendRow {
                color: [255, 0, 0],
                label: "A".into(),
            }],
            Some(("1:5000", 100.0)),
        )
        .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        // 白底 + 图例红色块存在。
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 60 && p.blue() < 60));
    }
}
