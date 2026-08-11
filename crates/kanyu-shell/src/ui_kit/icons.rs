//! 图标系统：总规 §1.4 落地——线性图标（stroke 1.5px、圆角端点、几何极简）。
//!
//! 所有图标用 egui painter 即时绘制（无字体/图片依赖），颜色由调用方给
//! （随主题/状态变化）。图标语义遵循总规：山形=地形、水波纹=水系、
//! 罗盘=坐标/方向、简化太极=AI 智能模式。

use eframe::egui;
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// 图标枚举（Ribbon/面板/树行/按钮共用）。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    /// 打开数据（文件夹）。
    Folder,
    /// 打开示例（星）。
    Example,
    /// 保存截图（相机）。
    Camera,
    /// 晨山（太阳）。
    Sun,
    /// 夜观星（月亮）。
    Moon,
    /// 图层概要（i）。
    Info,
    /// 图层（叠层）。
    Layers,
    /// 属性查询（漏斗）。
    Funnel,
    /// 导出（出箭头）。
    Export,
    /// 投影变换（罗盘）。
    Compass,
    /// 缓冲区（同心圆）。
    Buffer,
    /// 叠加分析（两方交叠）。
    Overlay,
    /// 拓扑检查（节点网）。
    Topology,
    /// 空间连接（链环）。
    Link,
    /// 分区统计（网格）。
    Grid,
    /// 测地度量（尺）。
    Ruler,
    /// 渲染/地图输出（图片框）。
    Image,
    /// 缩放到图层（四角外扩）。
    ZoomFit,
    /// 复位视图（回环箭头）。
    Reset,
    /// 左面板。
    PanelLeft,
    /// 底部面板。
    PanelBottom,
    /// 右面板。
    PanelRight,
    /// WASM 技能（六边形）。
    Skill,
    /// 清单（列表）。
    List,
    /// 运行（三角）。
    Play,
    /// 帮助（?）。
    Help,
    /// AI 对话（简化太极/云）。
    Chat,
    /// 发送（纸飞机）。
    Send,
    /// 设置（齿轮简化）。
    Settings,
    /// 关闭/移除（×）。
    Close,
    /// 可见（眼）。
    Eye,
    /// 隐藏（闭眼）。
    EyeOff,
    /// 字段（小表格行）。
    Field,
}

/// 在 rect 内绘制图标（stroke 1.5px、圆角端点）。
pub fn draw(painter: &Painter, icon: Icon, rect: Rect, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let r = rect.shrink2(Vec2::splat(rect.width().min(rect.height()) * 0.12));
    let c = r.center();
    let (x0, y0, x1, y1) = (r.min.x, r.min.y, r.max.x, r.max.y);
    let (w, h) = (r.width(), r.height());
    let p = |x: f32, y: f32| Pos2::new(x, y);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let circle = |center: Pos2, radius: f32| painter.circle_stroke(center, radius, stroke);

    match icon {
        Icon::Folder => {
            // 文件夹轮廓
            let tab_w = w * 0.45;
            let path = vec![
                p(x0, y1),
                p(x0, y0 + h * 0.2),
                p(x0 + tab_w * 0.6, y0 + h * 0.2),
                p(x0 + tab_w, y0),
                p(x1 - w * 0.15, y0),
            ];
            painter.line(path, stroke);
            line(p(x1 - w * 0.15, y0), p(x1 - w * 0.15, y0 + h * 0.25));
            line(p(x1 - w * 0.15, y0 + h * 0.25), p(x1, y0 + h * 0.25));
            line(p(x1, y0 + h * 0.25), p(x1, y1));
            line(p(x1, y1), p(x0, y1));
        }
        Icon::Example => {
            // 四角星
            let (cx, cy) = (c.x, c.y);
            line(p(cx, y0), p(cx, y1));
            line(p(x0, cy), p(x1, cy));
            line(p(cx - w * 0.2, cy - h * 0.2), p(cx + w * 0.2, cy + h * 0.2));
            line(p(cx + w * 0.2, cy - h * 0.2), p(cx - w * 0.2, cy + h * 0.2));
        }
        Icon::Camera => {
            painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Middle);
            circle(c, w * 0.2);
            line(p(x1 - w * 0.3, y0), p(x1 - w * 0.3, y0 - h * 0.12));
            line(
                p(x1 - w * 0.3, y0 - h * 0.12),
                p(x1 - w * 0.05, y0 - h * 0.12),
            );
            line(p(x1 - w * 0.05, y0 - h * 0.12), p(x1 - w * 0.05, y0));
        }
        Icon::Sun => {
            circle(c, w * 0.22);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (dx, dy) = (a.cos(), a.sin());
                line(
                    p(c.x + dx * w * 0.33, c.y + dy * w * 0.33),
                    p(c.x + dx * w * 0.46, c.y + dy * w * 0.46),
                );
            }
        }
        Icon::Moon => {
            painter.circle_stroke(c, w * 0.3, stroke);
            // 月牙遮罩线
            painter.line(
                (0..=8)
                    .map(|i| {
                        let t = i as f32 / 8.0 * std::f32::consts::PI;
                        p(
                            c.x + (w * 0.3) * t.cos() * 0.4 + w * 0.12,
                            c.y - (w * 0.3) * t.sin(),
                        )
                    })
                    .collect(),
                stroke,
            );
        }
        Icon::Info => {
            circle(c, w * 0.42);
            line(p(c.x, c.y - h * 0.05), p(c.x, c.y + h * 0.28));
            painter.circle_filled(p(c.x, c.y - h * 0.22), 1.6, color);
        }
        Icon::Layers => {
            for i in 0..3 {
                let yy = y0 + h * (0.18 + 0.28 * i as f32);
                painter.line(
                    vec![
                        p(c.x, yy - h * 0.14),
                        p(x1 - w * 0.06, yy),
                        p(c.x, yy + h * 0.14),
                        p(x0 + w * 0.06, yy),
                        p(c.x, yy - h * 0.14),
                    ],
                    stroke,
                );
            }
        }
        Icon::Funnel => {
            painter.line(
                vec![
                    p(x0, y0),
                    p(x1, y0),
                    p(c.x + w * 0.08, c.y),
                    p(c.x + w * 0.08, y1),
                    p(c.x - w * 0.08, y1 - h * 0.12),
                    p(c.x - w * 0.08, c.y),
                    p(x0, y0),
                ],
                stroke,
            );
        }
        Icon::Export => {
            line(
                p(c.x - w * 0.1, y0 + h * 0.1),
                p(c.x - w * 0.1, y1 - h * 0.1),
            );
            painter.line(
                vec![
                    p(c.x - w * 0.3, c.y),
                    p(c.x - w * 0.1, c.y - h * 0.2),
                    p(c.x + w * 0.1, c.y),
                ],
                stroke,
            );
            line(
                p(x0 + w * 0.1, y1 - h * 0.15),
                p(x1 - w * 0.1, y1 - h * 0.15),
            );
        }
        Icon::Compass => {
            circle(c, w * 0.42);
            painter.line(
                vec![
                    p(c.x + w * 0.15, c.y - h * 0.15),
                    p(c.x - w * 0.05, c.y + h * 0.05),
                    p(c.x - w * 0.15, c.y + h * 0.15),
                    p(c.x + w * 0.05, c.y - h * 0.05),
                    p(c.x + w * 0.15, c.y - h * 0.15),
                ],
                stroke,
            );
        }
        Icon::Buffer => {
            circle(c, w * 0.16);
            circle(c, w * 0.34);
            circle(c, w * 0.46);
        }
        Icon::Overlay => {
            let s = w * 0.55;
            painter.rect_stroke(
                Rect::from_min_size(p(x0, y0), Vec2::splat(s)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                Rect::from_min_size(p(x1 - s, y1 - s), Vec2::splat(s)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Icon::Topology => {
            let n1 = p(x0 + w * 0.2, y0 + h * 0.25);
            let n2 = p(x1 - w * 0.2, y0 + h * 0.3);
            let n3 = p(c.x, y1 - h * 0.15);
            for (a, b) in [(n1, n2), (n2, n3), (n3, n1)] {
                line(a, b);
            }
            for n in [n1, n2, n3] {
                painter.circle_filled(n, 2.2, color);
                circle(n, 4.0);
            }
        }
        Icon::Link => {
            painter.circle_stroke(p(c.x - w * 0.18, c.y), w * 0.2, stroke);
            painter.circle_stroke(p(c.x + w * 0.18, c.y), w * 0.2, stroke);
            line(p(c.x - w * 0.02, c.y), p(c.x + w * 0.02, c.y));
        }
        Icon::Grid => {
            painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Middle);
            line(p(c.x, y0), p(c.x, y1));
            line(p(x0, c.y), p(x1, c.y));
        }
        Icon::Ruler => {
            let r2 = Rect::from_min_size(p(x0, y0 + h * 0.35), Vec2::new(w, h * 0.4));
            painter.rect_stroke(r2, 1.5, stroke, egui::StrokeKind::Middle);
            for i in 1..4 {
                let xx = x0 + w * i as f32 / 4.0;
                line(p(xx, y0 + h * 0.35), p(xx, y0 + h * 0.5));
            }
        }
        Icon::Image => {
            painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Middle);
            circle(p(x0 + w * 0.28, y0 + h * 0.3), w * 0.08);
            painter.line(
                vec![
                    p(x0, y1 - h * 0.15),
                    p(x0 + w * 0.35, c.y + h * 0.1),
                    p(c.x, y1 - h * 0.12),
                    p(x0 + w * 0.7, c.y + h * 0.18),
                    p(x1, y1 - h * 0.05),
                ],
                stroke,
            );
        }
        Icon::ZoomFit => {
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                let oy = sy * h * 0.3;
                painter.line(
                    vec![
                        p(c.x + sx * w * 0.1, c.y + oy),
                        p(c.x + sx * w * 0.42, c.y + oy),
                        p(c.x + sx * w * 0.42, c.y + sy * h * 0.1),
                    ],
                    stroke,
                );
            }
        }
        Icon::Reset => {
            // 270° 圆弧（折线近似）+ 箭头。
            let radius = w * 0.34;
            let start = std::f32::consts::FRAC_PI_4;
            let sweep = std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            let pts: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = start + sweep * i as f32 / 12.0;
                    p(c.x + a.cos() * radius, c.y + a.sin() * radius)
                })
                .collect();
            painter.line(pts.clone(), stroke);
            let tip = *pts.last().unwrap();
            painter.line(
                vec![
                    p(tip.x - w * 0.12, tip.y - h * 0.06),
                    tip,
                    p(tip.x + w * 0.02, tip.y + h * 0.14),
                ],
                stroke,
            );
        }
        Icon::PanelLeft | Icon::PanelBottom | Icon::PanelRight => {
            painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Middle);
            match icon {
                Icon::PanelLeft => {
                    painter.rect_filled(
                        Rect::from_min_size(p(x0, y0), Vec2::new(w * 0.35, h)),
                        1.5,
                        color,
                    );
                }
                Icon::PanelBottom => {
                    painter.rect_filled(
                        Rect::from_min_size(p(x0, y1 - h * 0.35), Vec2::new(w, h * 0.35)),
                        1.5,
                        color,
                    );
                }
                _ => {
                    painter.rect_filled(
                        Rect::from_min_size(p(x1 - w * 0.35, y0), Vec2::new(w * 0.35, h)),
                        1.5,
                        color,
                    );
                }
            }
        }
        Icon::Skill => {
            // 六边形 + 内核点
            let pts: Vec<Pos2> = (0..6)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 6.0 - std::f32::consts::FRAC_PI_2;
                    p(c.x + a.cos() * w * 0.42, c.y + a.sin() * w * 0.42)
                })
                .collect();
            let mut ring = pts.clone();
            ring.push(pts[0]);
            painter.line(ring, stroke);
            circle(c, w * 0.12);
        }
        Icon::List => {
            for i in 0..3 {
                let yy = y0 + h * (0.2 + 0.3 * i as f32);
                painter.circle_filled(p(x0 + w * 0.1, yy), 1.6, color);
                line(p(x0 + w * 0.28, yy), p(x1, yy));
            }
        }
        Icon::Play => {
            painter.line(
                vec![
                    p(x0 + w * 0.15, y0 + h * 0.1),
                    p(x1 - w * 0.1, c.y),
                    p(x0 + w * 0.15, y1 - h * 0.1),
                    p(x0 + w * 0.15, y0 + h * 0.1),
                ],
                stroke,
            );
        }
        Icon::Help => {
            circle(c, w * 0.42);
            // 问号上弧（折线近似，从左上到右侧约 300°）。
            let ac = p(c.x, c.y - h * 0.08);
            let ar = w * 0.16;
            let pts: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let a = std::f32::consts::PI + (std::f32::consts::TAU - 1.2) * i as f32 / 10.0;
                    p(ac.x + a.cos() * ar, ac.y + a.sin() * ar)
                })
                .collect();
            painter.line(pts.clone(), stroke);
            let end = *pts.last().unwrap();
            line(end, p(c.x, c.y + h * 0.12));
            painter.circle_filled(p(c.x, c.y + h * 0.28), 1.6, color);
        }
        Icon::Chat => {
            // 对话气泡
            let r2 =
                Rect::from_min_size(p(x0 + w * 0.05, y0 + h * 0.1), Vec2::new(w * 0.9, h * 0.62));
            painter.rect_stroke(r2, 3.0, stroke, egui::StrokeKind::Middle);
            painter.line(
                vec![
                    p(x0 + w * 0.3, y0 + h * 0.72),
                    p(x0 + w * 0.3, y1 - h * 0.02),
                    p(x0 + w * 0.55, y0 + h * 0.72),
                ],
                stroke,
            );
            for i in 0..3 {
                painter.circle_filled(
                    p(c.x + (i as f32 - 1.0) * w * 0.18, c.y - h * 0.09),
                    1.6,
                    color,
                );
            }
        }
        Icon::Send => {
            painter.line(
                vec![
                    p(x0 + w * 0.08, c.y),
                    p(x1 - w * 0.06, y0 + h * 0.08),
                    p(c.x + w * 0.1, y1 - h * 0.08),
                    p(c.x - w * 0.02, c.y + h * 0.1),
                    p(x0 + w * 0.08, c.y),
                ],
                stroke,
            );
            line(
                p(c.x - w * 0.02, c.y + h * 0.1),
                p(c.x + w * 0.06, c.y - h * 0.02),
            );
        }
        Icon::Settings => {
            circle(c, w * 0.16);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                let (dx, dy) = (a.cos(), a.sin());
                line(
                    p(c.x + dx * w * 0.26, c.y + dy * w * 0.26),
                    p(c.x + dx * w * 0.42, c.y + dy * w * 0.42),
                );
            }
        }
        Icon::Close => {
            line(p(x0, y0), p(x1, y1));
            line(p(x1, y0), p(x0, y1));
        }
        Icon::Eye => {
            painter.line(
                vec![
                    p(x0, c.y),
                    p(c.x - w * 0.15, y0 + h * 0.15),
                    p(c.x + w * 0.15, y0 + h * 0.15),
                    p(x1, c.y),
                    p(c.x + w * 0.15, y1 - h * 0.15),
                    p(c.x - w * 0.15, y1 - h * 0.15),
                    p(x0, c.y),
                ],
                stroke,
            );
            circle(c, w * 0.1);
        }
        Icon::EyeOff => {
            draw(painter, Icon::Eye, rect, color);
            line(p(x0, y1), p(x1, y0));
        }
        Icon::Field => {
            for i in 0..3 {
                let yy = y0 + h * (0.2 + 0.3 * i as f32);
                line(p(x0, yy), p(x1 * 0.75 + x0 * 0.25, yy));
            }
        }
    }
}

/// 便捷函数：给 ui 分配一个图标位并绘制，返回响应矩形。
pub fn icon_ui(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    draw(ui.painter(), icon, rect, color);
    rect
}

/// 图标默认色（主题强调色）：面板标题/树行图标的统一取色口。
pub fn icons_color(ui: &egui::Ui) -> Color32 {
    crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
    .accent
}

// ===== 位图图标（ArcGIS Pro 本机资源，可选增强）=====
//
// 许可说明：PNG 图标来自用户本机已授权的 ArcGIS Pro 安装
// （ArcGIS.Desktop.Resources.dll，images/ 与 darkimages/ 双主题变体），
// 提取到用户目录 %LOCALAPPDATA%\Programs\kanyu\icons\ 使用，**不进入开源仓库**。
// 缺图时回退到上方的手绘线性图标（draw）——双轨保证任意环境可用。

/// 语义图标 → ArcGIS Pro 资源名（不含扩展名；32px 档）。
/// 新增功能扩展图标时在此登记映射（单一事实来源）。
pub fn arcgis_resource_name(icon: Icon) -> &'static str {
    match icon {
        Icon::Folder => "folderconnectionadd32",
        Icon::Example => "genericnewsparklelarge32",
        Icon::Camera => "itemthumbnailcreate32",
        Icon::Sun => "sun32",
        Icon::Moon => "colorramp32",
        Icon::Info => "featureclassproperties32",
        Icon::Layers => "layergroup32",
        Icon::Funnel => "selectionselectbyattributes32",
        Icon::Export => "exportfeatures32",
        Icon::Compass => "projectcoordinatesystem32",
        Icon::Buffer => "editingbuffer32",
        Icon::Overlay => "overlaymain32",
        Icon::Topology => "highlightgapsandoverlaps32",
        Icon::Link => "tablespatialjoin32",
        Icon::Grid => "tablesummarystatistics32",
        Icon::Ruler => "measuredistance32",
        Icon::Image => "picture32",
        Icon::ZoomFit => "zoomfullextent32",
        Icon::Reset => "genericreset32",
        Icon::PanelLeft => "contentswindowshow32",
        Icon::PanelBottom => "geoprocessingpythonwindowshow32",
        Icon::PanelRight => "showdetailspanel32",
        Icon::Skill => "geoprocessingtoolboxpythonnew32",
        Icon::List => "viewgallery32",
        Icon::Play => "genericrun32",
        Icon::Help => "arcgisprohelp32",
        Icon::Chat => "geoai32",
        Icon::Send => "play16",
        Icon::Settings => "cogwheel32",
        Icon::Close => "close32",
        Icon::Eye => "showdetailspanel32",
        Icon::EyeOff => "hidedetailspanel32",
        Icon::Field => "tablefieldsview32",
    }
}

/// 位图图标缓存：按（图标, 主题）缓存纹理；目录不存在则全程回退手绘。
pub struct IconCache {
    /// 图标根目录（含 light/ dark/ 子目录）。
    base: Option<std::path::PathBuf>,
    textures: std::collections::HashMap<(Icon, bool), egui::TextureHandle>,
}

impl Default for IconCache {
    fn default() -> Self {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .map(|p| p.join("Programs").join("kanyu").join("icons"))
            .filter(|p| p.join("light").is_dir());
        Self {
            base,
            textures: std::collections::HashMap::new(),
        }
    }
}

impl IconCache {
    /// 取图标纹理（dark=true 用 darkimages 变体——主题风格优化）；
    /// 缺图返回 None（调用方回退手绘 draw）。
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        icon: Icon,
        dark: bool,
    ) -> Option<&egui::TextureHandle> {
        let base = self.base.clone()?;
        let key = (icon, dark);
        let entry = match self.textures.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => return Some(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(e) => e,
        };
        let theme_dir = if dark { "dark" } else { "light" };
        let path = base
            .join(theme_dir)
            .join(format!("{}.png", arcgis_resource_name(icon)));
        let bytes = std::fs::read(&path).ok()?;
        let pixmap = tiny_skia::Pixmap::decode_png(&bytes).ok()?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [pixmap.width() as usize, pixmap.height() as usize],
            pixmap.data(),
        );
        let tex = ctx.load_texture(
            format!("icon-{icon:?}-{dark}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        Some(entry.insert(tex))
    }
}

/// 统一图标绘制入口：**有位图用位图（按主题取 light/dark 变体），
/// 否则回退手绘线性图标**。所有调用方（ribbon/树行/QAT/终端）走此口。
pub fn draw_or_image(
    ui: &mut egui::Ui,
    cache: &mut IconCache,
    icon: Icon,
    rect: Rect,
    color: Color32,
) {
    let dark = ui.visuals().dark_mode;
    if let Some(tex) = cache.texture(ui.ctx(), icon, dark) {
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        draw(ui.painter(), icon, rect, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_icons_have_distinct_discriminants() {
        // 防重复粘贴：枚举变体数量即语义数量（编译期保证，此测试防手滑改重名）。
        let icons = [
            Icon::Folder,
            Icon::Example,
            Icon::Camera,
            Icon::Sun,
            Icon::Moon,
            Icon::Info,
            Icon::Layers,
            Icon::Funnel,
            Icon::Export,
            Icon::Compass,
            Icon::Buffer,
            Icon::Overlay,
            Icon::Topology,
            Icon::Link,
            Icon::Grid,
            Icon::Ruler,
            Icon::Image,
            Icon::ZoomFit,
            Icon::Reset,
            Icon::PanelLeft,
            Icon::PanelBottom,
            Icon::PanelRight,
            Icon::Skill,
            Icon::List,
            Icon::Play,
            Icon::Help,
            Icon::Chat,
            Icon::Send,
            Icon::Settings,
            Icon::Close,
            Icon::Eye,
            Icon::EyeOff,
            Icon::Field,
        ];
        for (i, a) in icons.iter().enumerate() {
            for (j, b) in icons.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "图标重复: {a:?} vs {b:?}");
                }
            }
        }
        assert_eq!(icons.len(), 33);
    }
}
