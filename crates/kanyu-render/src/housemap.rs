//! 房产图渲染器 —— GB/T 42547-2023《地籍调查规程》图 L.5 版式。
//!
//! 页面（A4，**朝向自适应**——§5.4.5.5.4 按房屋朝向横放或竖放并加绘指北方向：
//! 房屋 bbox 宽 ≥ 高 → A4 横 297×210，否则 A4 竖 210×297）：
//! - 顶部居中标题「房 产 图」（6.5mm 粗体）+ 右上「单位：m·㎡」（2.6mm）；
//! - 头部四行表（横向行线 0.15mm 黑、跨内图廓全宽；图 L.5 金样无竖向栏线）：
//!   行1 宗地代码|值|结构|值|专有建筑面积|值；行2 幢号|值|总层数|值|分摊建筑面积|值；
//!   行3 户号|值|所在层次|值|建筑面积|值；行4 坐落|值（跨后五栏通栏居中）。
//!   标签小字 2.2mm、值 2.7mm；空值留空（面积值 `{:.2}`，None 留空）；
//! - 地图区：房屋轮廓**黑色线**（0.3mm，无填充，无界址点圆圈/点号）+
//!   逐边边长注记（2.4mm 黑，`kanyu_core::cartography` 勘测定界图注记契约排版——
//!   中点法线、角度沿线（字头向北允许向西），外法线按环走向）；
//! - 右上「北」指北针（细长三角，同 parcelmap 图 L.3 样式）；
//!   右下「绘制日期：{date}」（2.5mm）；内图廓外下中「1:N」（3.6mm，分母向上整百）；
//! - 左侧竖排单位名（可选，空串不绘；逐字纵向堆叠，同 parcelmap 做法）。
//!
//! 比例尺缺省自动求解：房屋 bbox 适配地图区（内沿留白）后分母向上取整百。
//! 场景图元与 parcelmap 共享（`pub(crate)` 助手）；SVG 全量（文字/旋转齐全），
//! PNG 经 `layout::TextBackend` 系统字体栈（旋转注记离屏 pixmap 旋转合成）。

use crate::layout::{PageSize, TextBackend};
use crate::parcelmap::{
    diagnostics_of, draw_rotated_text, esc, fill_attr, ring_bbox, round_up_hundred, stroke_attrs,
    Anchor, ParcelMapData, Prim, Stroke, BLACK,
};
use crate::RenderError;
use kanyu_core::cartography::{self, ParcelBoundary};

/// 房产图出图参数。
#[derive(Debug, Clone)]
pub struct HouseMapSpec {
    /// 宗地代码。
    pub parcel_code: String,
    /// 结构（如 B/钢/混）。
    pub structure: String,
    /// 专有建筑面积（㎡；None 留空）。
    pub exclusive_area: Option<f64>,
    /// 幢号。
    pub building_no: String,
    /// 总层数。
    pub total_floors: String,
    /// 分摊建筑面积（㎡；None 留空）。
    pub shared_area: Option<f64>,
    /// 户号。
    pub household_no: String,
    /// 所在层次。
    pub floor_no: String,
    /// 建筑面积（㎡；None 留空）。
    pub building_area: Option<f64>,
    /// 坐落。
    pub location: String,
    /// 绘制日期。
    pub draw_date: String,
    /// 左侧竖排单位名（可选，空串不绘）。
    pub unit_name: String,
    /// 比例尺分母（None 自动取整百）。
    pub scale: Option<u32>,
    /// 分辨率（默认 150）。
    pub dpi: f64,
}

impl Default for HouseMapSpec {
    fn default() -> Self {
        Self {
            parcel_code: String::new(),
            structure: String::new(),
            exclusive_area: None,
            building_no: String::new(),
            total_floors: String::new(),
            shared_area: None,
            household_no: String::new(),
            floor_no: String::new(),
            building_area: None,
            location: String::new(),
            draw_date: String::new(),
            unit_name: String::new(),
            scale: None,
            dpi: 150.0,
        }
    }
}

/// 出图结果（含实际比例尺、幅面朝向与排版诊断）。
#[derive(Debug, Clone)]
pub struct HouseMapOutput {
    /// 实际采用的比例尺分母。
    pub scale: u32,
    /// 幅面朝向（true = A4 横；false = A4 竖）。
    pub landscape: bool,
    /// 边长注记排版诊断行（每条注记一行：文本/途径/净空/残余压盖）。
    pub diagnostics: Vec<String>,
    /// SVG 或 PNG 产物。
    pub data: ParcelMapData,
}

/// 渲染房产图为 SVG（完整排版）。
pub fn render_house_map_svg(
    boundary: &ParcelBoundary,
    spec: &HouseMapSpec,
) -> Result<HouseMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    Ok(HouseMapOutput {
        scale: scene.scale,
        landscape: scene.landscape,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Svg(scene_to_svg(&scene.prims, scene.page)),
    })
}

/// 渲染房产图为 PNG（tiny-skia 光栅链 + TextBackend 系统字体栈）。
pub fn render_house_map_png(
    boundary: &ParcelBoundary,
    spec: &HouseMapSpec,
) -> Result<HouseMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    let png = scene_to_png(&scene.prims, spec.dpi, &TextBackend::system(), scene.page)?;
    Ok(HouseMapOutput {
        scale: scene.scale,
        landscape: scene.landscape,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Png(png),
    })
}

// ---------------------------------------------------------------------------
// 版式常量（毫米；A4 横/竖自适应，几何全部由页面尺寸推导）
// ---------------------------------------------------------------------------

/// 外图廓距页边（0.5mm 线）/ 内图廓左右距页边（0.2mm 线）。
const OUTER_MARGIN: f64 = 8.0;
const INNER_MARGIN_X: f64 = 11.0;
/// 标题基线 y（图名大号粗体）/ 右上单位注记基线 y。
const TITLE_Y: f64 = 14.0;
const TITLE_FONT: f64 = 6.5;
const UNIT_NOTE_Y: f64 = 12.5;
const UNIT_NOTE_FONT: f64 = 2.6;
/// 内图廓上沿（= 头部表上沿；标题下方）/ 头部表行高（×4 行）。
const INNER_TOP: f64 = 16.5;
const HEADER_ROW_H: f64 = 8.4;
/// 头部表标签 / 值字号。
const HEADER_LABEL_FONT: f64 = 2.2;
const HEADER_VALUE_FONT: f64 = 2.7;
/// 内图廓下沿距页底距离（外图廓 8 + 底部比例尺带 20）。
const INNER_BOTTOM_OFF: f64 = 28.0;
/// 房屋适配留白（地图区内沿）/ 适配区最小高（兜底）。
const MAP_PAD: f64 = 10.0;
const FIT_MIN_H: f64 = 40.0;
/// 指北针（地图区内右上）右边距 / 顶边距。
const NORTH_DX: f64 = 9.0;
const NORTH_DY: f64 = 5.0;

/// 版面几何（A4 横/竖自适应；横竖同边距规则推导）。
struct Layout {
    /// 纸张（横/竖）。
    page: PageSize,
    /// 页宽 / 页高。
    page_w: f64,
    page_h: f64,
    /// 外图廓（0.5mm）/ 内图廓（0.2mm，含头部表 + 地图区）。
    outer: [f64; 4],
    inner: [f64; 4],
    /// 地图区（内图廓扣除头部表带）。
    map_rect: [f64; 4],
}

impl Layout {
    /// 按朝向推导版面（landscape=true → A4 横 297×210，否则 A4 竖 210×297）。
    fn new(landscape: bool) -> Self {
        let page = if landscape {
            PageSize::A4Landscape
        } else {
            PageSize::A4Portrait
        };
        let (page_w, page_h) = page.mm();
        let outer = [
            OUTER_MARGIN,
            OUTER_MARGIN,
            page_w - 2.0 * OUTER_MARGIN,
            page_h - 2.0 * OUTER_MARGIN,
        ];
        let inner = [
            INNER_MARGIN_X,
            INNER_TOP,
            page_w - 2.0 * INNER_MARGIN_X,
            page_h - INNER_BOTTOM_OFF - INNER_TOP,
        ];
        let header_y1 = INNER_TOP + HEADER_ROW_H * 4.0;
        let map_rect = [
            inner[0],
            header_y1,
            inner[2],
            inner[1] + inner[3] - header_y1,
        ];
        Self {
            page,
            page_w,
            page_h,
            outer,
            inner,
            map_rect,
        }
    }
}

/// 排版场景（图元 + 纸张 + 实际比例尺 + 朝向 + 诊断）。
struct Scene {
    prims: Vec<Prim>,
    page: PageSize,
    scale: u32,
    landscape: bool,
    diagnostics: Vec<String>,
}

// ---------------------------------------------------------------------------
// 场景构建
// ---------------------------------------------------------------------------

/// 头部四行表文本（标签 2.2mm 左对位、值 2.7mm；行4 坐落值跨后五栏通栏居中；
/// 面积值 `{:.2}`、None 留空）。栏位为内图廓宽比例锚点（图 L.5 金样无竖向栏线，
/// 横竖幅面均跨页宽铺排）。
fn emit_header(prims: &mut Vec<Prim>, layout: &Layout, spec: &HouseMapSpec) {
    let il = layout.inner[0];
    let ir = il + layout.inner[2];
    let iw = layout.inner[2];
    // 六栏锚点（标签/值相间；末值右对位贴内图廓右缘）
    let x_label1 = il + iw * 0.035;
    let x_value1 = il + iw * 0.22;
    let x_label2 = il + iw * 0.53;
    let x_value2 = il + iw * 0.68;
    let x_label3 = il + iw * 0.76;
    let x_value3 = ir - 3.6;
    let area = |v: Option<f64>| v.map(|a| format!("{a:.2}")).unwrap_or_default();
    let rows: [[String; 6]; 3] = [
        [
            "宗地代码".to_string(),
            spec.parcel_code.clone(),
            "结构".to_string(),
            spec.structure.clone(),
            "专有建筑面积".to_string(),
            area(spec.exclusive_area),
        ],
        [
            "幢号".to_string(),
            spec.building_no.clone(),
            "总层数".to_string(),
            spec.total_floors.clone(),
            "分摊建筑面积".to_string(),
            area(spec.shared_area),
        ],
        [
            "户号".to_string(),
            spec.household_no.clone(),
            "所在层次".to_string(),
            spec.floor_no.clone(),
            "建筑面积".to_string(),
            area(spec.building_area),
        ],
    ];
    let cell = |prims: &mut Vec<Prim>, x: f64, y: f64, font: f64, anchor: Anchor, text: &str| {
        prims.push(Prim::Text {
            x,
            y,
            font,
            text: text.to_string(),
            anchor,
            rotate_deg: 0.0,
            vcenter: true,
            bold: false,
        });
    };
    for (r, row) in rows.iter().enumerate() {
        let y = INNER_TOP + (r as f64 + 0.5) * HEADER_ROW_H;
        cell(
            prims,
            x_label1,
            y,
            HEADER_LABEL_FONT,
            Anchor::Start,
            &row[0],
        );
        cell(
            prims,
            x_value1,
            y,
            HEADER_VALUE_FONT,
            Anchor::Start,
            &row[1],
        );
        cell(
            prims,
            x_label2,
            y,
            HEADER_LABEL_FONT,
            Anchor::Start,
            &row[2],
        );
        cell(
            prims,
            x_value2,
            y,
            HEADER_VALUE_FONT,
            Anchor::Start,
            &row[3],
        );
        cell(
            prims,
            x_label3,
            y,
            HEADER_LABEL_FONT,
            Anchor::Start,
            &row[4],
        );
        cell(prims, x_value3, y, HEADER_VALUE_FONT, Anchor::End, &row[5]);
    }
    // 行4：坐落 | 值（跨后五栏通栏居中）
    let y4 = INNER_TOP + 3.5 * HEADER_ROW_H;
    cell(
        prims,
        x_label1,
        y4,
        HEADER_LABEL_FONT,
        Anchor::Start,
        "坐落",
    );
    cell(
        prims,
        (x_value1 + x_value3) / 2.0,
        y4,
        HEADER_VALUE_FONT,
        Anchor::Middle,
        &spec.location,
    );
}

/// 组场景：版面几何 + 边长注记排版 + 诊断（SVG/PNG 共用）。
fn build_scene(boundary: &ParcelBoundary, spec: &HouseMapSpec) -> Result<Scene, RenderError> {
    // 轮廓点记录仅供边长排版的端点符号障碍查找（房产图不绘界址点圆圈/点号）
    let points = cartography::generate_boundary_points(boundary, "");
    if points.is_empty() {
        return Err(RenderError::InvalidStyle("房屋几何无轮廓点".to_string()));
    }
    let lines = cartography::generate_boundary_lines(boundary, &points);
    let (min_x, min_y, max_x, max_y) = ring_bbox(&boundary.exterior);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    // 朝向自适应（§5.4.5.5.4：按房屋朝向横放或竖放并加绘指北方向）
    let landscape = span_x >= span_y;
    let layout = Layout::new(landscape);
    let fit_w = (layout.map_rect[2] - 2.0 * MAP_PAD).max(1.0);
    let fit_h = (layout.map_rect[3] - 2.0 * MAP_PAD).max(FIT_MIN_H);
    // 比例尺：spec 给定直接用；缺省取宽/高方向较大 raw 分母向上整百
    let scale = match spec.scale {
        Some(s) => s,
        None => round_up_hundred((span_x * 1000.0 / fit_w).max(span_y * 1000.0 / fit_h)),
    };
    // 地图单位（米）→ 纸面毫米：mu = 1000 / scale；bbox 中心对适配区中心，北朝上
    let mu = 1000.0 / f64::from(scale);
    let fit_cx = layout.map_rect[0] + layout.map_rect[2] / 2.0;
    let fit_cy = layout.map_rect[1] + layout.map_rect[3] / 2.0;
    let bbox_c = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let to_page = |(x, y): (f64, f64)| (fit_cx + (x - bbox_c.0) * mu, fit_cy - (y - bbox_c.1) * mu);

    // 边长注记排版（2.4mm；中点法线、角度沿线字头向北允许向西，外法线按环走向）
    let edge_report = cartography::place_edge_labels(
        boundary,
        &lines,
        &points,
        &cartography::EdgeLabelOptions {
            scale,
            ..Default::default()
        },
        &[],
    );
    let diagnostics = diagnostics_of(&[&edge_report]);

    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    let mut prims: Vec<Prim> = Vec::with_capacity(96);
    // —— 图廓（外粗内细）——
    prims.push(Prim::Rect {
        rect: layout.outer,
        fill: None,
        stroke: Some(Stroke {
            width: 0.5,
            color: BLACK,
        }),
    });
    prims.push(Prim::Rect {
        rect: layout.inner,
        fill: None,
        stroke: Some(Stroke {
            width: 0.2,
            color: BLACK,
        }),
    });
    // —— 头部表横向行线（0.15mm 黑，跨内图廓全宽；图 L.5 金样无竖向栏线）——
    let inner_x1 = layout.inner[0] + layout.inner[2];
    for r in 1..=4usize {
        let y = INNER_TOP + r as f64 * HEADER_ROW_H;
        prims.push(Prim::Path {
            pts: vec![(layout.inner[0], y), (inner_x1, y)],
            close: false,
            fill: None,
            stroke: thin,
        });
    }
    // —— 标题（顶部居中，粗体）+ 右上「单位：m·㎡」——
    prims.push(Prim::Text {
        x: layout.page_w / 2.0,
        y: TITLE_Y,
        font: TITLE_FONT,
        text: "房 产 图".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: true,
    });
    prims.push(Prim::Text {
        x: inner_x1 - 2.0,
        y: UNIT_NOTE_Y,
        font: UNIT_NOTE_FONT,
        text: "单位：m·㎡".to_string(),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 头部四行表 ——
    emit_header(&mut prims, &layout, spec);
    // —— 指北针（地图区内右上：「北」字 + 细长三角，同 parcelmap 图 L.3 样式）——
    let north_x = layout.map_rect[0] + layout.map_rect[2] - NORTH_DX;
    let north_top = layout.map_rect[1] + NORTH_DY;
    prims.push(Prim::Text {
        x: north_x,
        y: north_top + 1.6,
        font: 3.4,
        text: "北".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Path {
        pts: vec![
            (north_x, north_top + 4.0),
            (north_x - 1.4, north_top + 14.0),
            (north_x, north_top + 12.0),
            (north_x + 1.4, north_top + 14.0),
        ],
        close: true,
        fill: Some(BLACK),
        stroke: None,
    });
    // —— 房屋轮廓（0.3mm 黑线，无填充；非红、无界址点符号）——
    for ring in boundary.rings() {
        prims.push(Prim::Path {
            pts: ring.points.iter().map(|&p| to_page(p)).collect(),
            close: true,
            fill: None,
            stroke: Some(Stroke {
                width: 0.3,
                color: BLACK,
            }),
        });
    }
    // —— 逐边边长注记（2.4mm 黑，位置与旋转按 cartography 排版结果）——
    for l in &edge_report.labels {
        let (x, y) = to_page((l.rect.cx, l.rect.cy));
        prims.push(Prim::Text {
            x,
            y,
            font: 2.4,
            text: l.text.clone(),
            anchor: Anchor::Middle,
            rotate_deg: l.rotation_deg,
            vcenter: true,
            bold: false,
        });
    }
    // —— 右下「绘制日期：{date}」（地图区内右下）——
    prims.push(Prim::Text {
        x: layout.map_rect[0] + layout.map_rect[2] - 4.0,
        y: layout.map_rect[1] + layout.map_rect[3] - 5.0,
        font: 2.5,
        text: format!("绘制日期：{}", spec.draw_date),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 比例尺 1:N（内图廓外下中，3.6mm）——
    let inner_bottom = layout.inner[1] + layout.inner[3];
    prims.push(Prim::Text {
        x: layout.page_w / 2.0,
        y: (inner_bottom + layout.page_h - OUTER_MARGIN) / 2.0,
        font: 3.6,
        text: format!("1:{scale}"),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    // —— 左侧竖排单位名（地图区内左缘，逐字纵向堆叠；空串不绘）——
    let unit_chars: Vec<char> = spec.unit_name.chars().collect();
    if !unit_chars.is_empty() {
        let step = 3.4;
        let mid = layout.map_rect[1] + layout.map_rect[3] / 2.0;
        let y_start = mid - (unit_chars.len() as f64 - 1.0) * step / 2.0;
        for (i, ch) in unit_chars.iter().enumerate() {
            prims.push(Prim::Text {
                x: layout.map_rect[0] + 3.0,
                y: y_start + i as f64 * step,
                font: 2.8,
                text: ch.to_string(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    Ok(Scene {
        prims,
        page: layout.page,
        scale,
        landscape,
        diagnostics,
    })
}

// ---------------------------------------------------------------------------
// SVG 后端（与 parcelmap 同构；纸张按朝向自适应）
// ---------------------------------------------------------------------------

/// 场景 → SVG 文档（viewBox 即毫米坐标系，A4 横/竖按纸张）。
fn scene_to_svg(prims: &[Prim], page: PageSize) -> String {
    let (page_w, page_h) = page.mm();
    let mut out = String::with_capacity(8192);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {page_w:.0} {page_h:.0}\" width=\"{page_w:.0}mm\" height=\"{page_h:.0}mm\">\n"
    ));
    out.push_str("<!-- kanyu-render housemap · GB/T 42547-2023 图 L.5 -->\n");
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#FFFFFF\"/>\n");
    for p in prims {
        match p {
            Prim::Rect { rect, fill, stroke } => {
                out.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {}{}/>\n",
                    rect[0],
                    rect[1],
                    rect[2],
                    rect[3],
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Path {
                pts,
                close,
                fill,
                stroke,
            } => {
                let tag = if *close { "polygon" } else { "polyline" };
                let points: Vec<String> =
                    pts.iter().map(|(x, y)| format!("{x:.2},{y:.2}")).collect();
                out.push_str(&format!(
                    "<{tag} points=\"{}\" {}{}/>\n",
                    points.join(" "),
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Circle {
                cx,
                cy,
                r,
                fill,
                stroke,
            } => {
                out.push_str(&format!(
                    "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r:.2}\" {}{}/>\n",
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Text {
                x,
                y,
                font,
                text,
                anchor,
                rotate_deg,
                vcenter,
                bold,
            } => {
                let anchor_s = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let mut attrs = String::new();
                if *vcenter {
                    attrs.push_str(" dominant-baseline=\"central\"");
                }
                if *bold {
                    attrs.push_str(" font-weight=\"bold\"");
                }
                if rotate_deg.abs() > 1e-9 {
                    attrs.push_str(&format!(
                        " transform=\"rotate({rotate_deg:.2} {x:.2} {y:.2})\""
                    ));
                }
                out.push_str(&format!(
                    "<text x=\"{x:.2}\" y=\"{y:.2}\" font-size=\"{font:.2}\" font-family=\"sans-serif\" text-anchor=\"{anchor_s}\" fill=\"#000000\"{attrs}>{}</text>\n",
                    esc(text)
                ));
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

// ---------------------------------------------------------------------------
// PNG 后端（tiny-skia + TextBackend 系统字体栈；毫米 → 像素 k = dpi/25.4）
// ---------------------------------------------------------------------------

/// 场景 → PNG 字节（A4 横/竖按纸张；旋转注记离屏 pixmap 旋转合成，与 SVG rotate() 同角）。
fn scene_to_png(
    prims: &[Prim],
    dpi: f64,
    tb: &TextBackend,
    page: PageSize,
) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{
        Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Shader, Stroke as SkStroke, Transform,
    };
    let (pw, ph) = page.pixels(dpi);
    let k = dpi / 25.4; // 毫米 → 像素
    let mut page_pix = Pixmap::new(pw, ph).ok_or(RenderError::InvalidSize(pw, ph))?;
    page_pix.fill(Color::from_rgba8(255, 255, 255, 255));
    let paint = |c: [u8; 3]| Paint {
        shader: Shader::SolidColor(Color::from_rgba8(c[0], c[1], c[2], 255)),
        anti_alias: true,
        ..Default::default()
    };
    // 路径填充 + 描边（图元公共尾段）
    let fill_stroke = |page: &mut Pixmap,
                       path: &tiny_skia::Path,
                       fill: &Option<[u8; 3]>,
                       stroke: &Option<Stroke>| {
        if let Some(c) = fill {
            page.fill_path(
                path,
                &paint(*c),
                FillRule::Winding,
                Transform::default(),
                None,
            );
        }
        if let Some(s) = stroke {
            page.stroke_path(
                path,
                &paint(s.color),
                &SkStroke {
                    width: (s.width * k) as f32,
                    ..Default::default()
                },
                Transform::default(),
                None,
            );
        }
    };
    for p in prims {
        match p {
            Prim::Rect { rect, fill, stroke } => {
                let Some(r) = Rect::from_xywh(
                    (rect[0] * k) as f32,
                    (rect[1] * k) as f32,
                    (rect[2] * k) as f32,
                    (rect[3] * k) as f32,
                ) else {
                    continue;
                };
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page_pix, &path, fill, stroke);
                }
            }
            Prim::Path {
                pts,
                close,
                fill,
                stroke,
            } => {
                if pts.len() < 2 {
                    continue;
                }
                let mut pb = PathBuilder::new();
                pb.move_to((pts[0].0 * k) as f32, (pts[0].1 * k) as f32);
                for &(x, y) in &pts[1..] {
                    pb.line_to((x * k) as f32, (y * k) as f32);
                }
                if *close {
                    pb.close();
                }
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page_pix, &path, fill, stroke);
                }
            }
            Prim::Circle {
                cx,
                cy,
                r,
                fill,
                stroke,
            } => {
                let mut pb = PathBuilder::new();
                pb.push_circle((cx * k) as f32, (cy * k) as f32, (r * k) as f32);
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page_pix, &path, fill, stroke);
                }
            }
            Prim::Text {
                x,
                y,
                font,
                text,
                anchor,
                vcenter,
                rotate_deg,
                ..
            } => {
                let px = (font * k) as f32;
                let w = tb.measure(text, px);
                let x_px = (*x * k) as f32;
                // 旋转注记（边长沿线）：离屏小 pixmap 写字后旋转合成，
                // 与 SVG rotate() 同角（顺时针为正）
                if rotate_deg.abs() > 1e-6 {
                    draw_rotated_text(
                        &mut page_pix,
                        tb,
                        text,
                        x_px,
                        (*y * k) as f32,
                        px,
                        *rotate_deg,
                    );
                    continue;
                }
                let sx = match anchor {
                    Anchor::Start => x_px,
                    Anchor::Middle => x_px - w / 2.0,
                    Anchor::End => x_px - w,
                };
                // vcenter：基线 ≈ 中心 + 0.35em（cap 高近似）
                let baseline = if *vcenter {
                    ((*y + font * 0.35) * k) as f32
                } else {
                    (*y * k) as f32
                };
                tb.draw(
                    &mut page_pix,
                    text,
                    sx,
                    baseline,
                    px,
                    Color::from_rgba8(0, 0, 0, 255),
                );
            }
        }
    }
    page_pix
        .encode_png()
        .map_err(|e| RenderError::InvalidStyle(format!("房产图 PNG 编码失败: {e}")))
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kanyu_core::cartography::{RealestateRing, RingRole};

    /// L 形合成房屋（外沿 17m 宽 × 24m 高，投影坐标 东 39595000/北 4127000 附近，CCW）。
    fn l_shape_house() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595000.0, 4127000.0),
                    (39595017.0, 4127000.0),
                    (39595017.0, 4127014.0),
                    (39595008.0, 4127014.0),
                    (39595008.0, 4127024.0),
                    (39595000.0, 4127024.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 横形合成房屋（40m 宽 × 10m 高矩形）。
    fn wide_house() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595000.0, 4127000.0),
                    (39595040.0, 4127000.0),
                    (39595040.0, 4127010.0),
                    (39595000.0, 4127010.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 全要素 spec（比例尺缺省自动求解）。
    fn full_spec() -> HouseMapSpec {
        HouseMapSpec {
            parcel_code: "371602111019GB00005".to_string(),
            structure: "B".to_string(),
            exclusive_area: Some(876.01),
            building_no: "371602111019GB00005F0022".to_string(),
            total_floors: "3".to_string(),
            shared_area: Some(12.50),
            household_no: "101".to_string(),
            floor_no: "1".to_string(),
            building_area: Some(888.51),
            location: "朝阳现代城".to_string(),
            draw_date: "2026年08月25日".to_string(),
            unit_name: "某某市不动产登记中心".to_string(),
            scale: None,
            dpi: 150.0,
        }
    }

    /// 从产物取 SVG 文本（断言辅助）。
    fn svg_of(out: &HouseMapOutput) -> &str {
        match &out.data {
            ParcelMapData::Svg(s) => s,
            ParcelMapData::Png(_) => panic!("应为 SVG 产物"),
        }
    }

    #[test]
    fn svg_l_shape_portrait_full_elements() {
        let out = render_house_map_svg(&l_shape_house(), &full_spec()).unwrap();
        // 朝向自适应：17 宽 × 24 高 → 竖放（A4 竖）
        assert!(!out.landscape, "竖形房屋应竖放");
        // 自动比例尺手算验证：适配区 168×198.9mm；
        // raw = max(17×1000/168, 24×1000/198.9) = max(101.2, 120.7) = 120.7 → 整百 200
        assert_eq!(out.scale, 200);
        let svg = svg_of(&out);
        assert!(svg.contains("viewBox=\"0 0 210 297\""), "A4 竖幅面: {svg}");
        // 版式要素：标题 / 单位注记 / 指北针 / 绘制日期 / 比例尺
        for frag in [
            "房 产 图",
            "单位：m·㎡",
            "北",
            "绘制日期：2026年08月25日",
            "1:200",
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 头部四行表标签（「建筑面积」须为独立单元格文本，区别于专有/分摊建筑面积）
        for frag in [
            "宗地代码",
            "结构",
            "专有建筑面积",
            "幢号",
            "总层数",
            "分摊建筑面积",
            "户号",
            "所在层次",
            ">建筑面积</text>",
            "坐落",
        ] {
            assert!(svg.contains(frag), "SVG 缺少表签: {frag}");
        }
        // 头部表值
        for frag in [
            "371602111019GB00005",
            ">B</text>",
            "876.01",
            "F0022",
            ">3</text>",
            "12.50",
            ">101</text>",
            ">1</text>",
            "888.51",
            "朝阳现代城",
        ] {
            assert!(svg.contains(frag), "SVG 缺少表值: {frag}");
        }
        // 边长注记（L 形外沿 24.00 / 17.00）
        assert!(svg.contains("24.00"), "边长注记 24.00: {svg}");
        assert!(svg.contains("17.00"), "边长注记 17.00: {svg}");
    }

    #[test]
    fn wide_house_landscape() {
        let out = render_house_map_svg(&wide_house(), &full_spec()).unwrap();
        // 朝向自适应：40 宽 × 10 高 → 横放（A4 横）
        assert!(out.landscape, "横形房屋应横放");
        let svg = svg_of(&out);
        assert!(svg.contains("viewBox=\"0 0 297 210\""), "A4 横幅面: {svg}");
        assert!(
            svg.contains("40.00") && svg.contains("10.00"),
            "边长注记: {svg}"
        );
        // 竖形 L 房回归：竖放
        assert!(
            !render_house_map_svg(&l_shape_house(), &full_spec())
                .unwrap()
                .landscape
        );
    }

    #[test]
    fn empty_area_leaves_blank_cell() {
        // 面积空值：对应格留空（不出现「None」字面量），标签仍在
        let spec = HouseMapSpec {
            exclusive_area: None,
            shared_area: None,
            building_area: None,
            ..full_spec()
        };
        let out = render_house_map_svg(&l_shape_house(), &spec).unwrap();
        let svg = svg_of(&out);
        assert!(svg.contains("专有建筑面积") && svg.contains("分摊建筑面积"));
        assert!(svg.contains(">建筑面积</text>"));
        assert!(!svg.contains("None"), "空值应留空串而非 None: {svg}");
        assert!(!svg.contains("876.01") && !svg.contains("888.51"));
    }

    #[test]
    fn edge_label_diagnostics_clean() {
        let out = render_house_map_svg(&l_shape_house(), &full_spec()).unwrap();
        // L 形 6 边 → 6 条边长注记诊断（逐条摘要：text reason=… clearance=…mm overlap=…）
        assert_eq!(out.diagnostics.len(), 6, "{:?}", out.diagnostics);
        assert!(out
            .diagnostics
            .iter()
            .any(|d| d.starts_with("24.00 reason=")
                && d.contains("clearance=")
                && d.contains("overlap=")));
        // 方正 L 形排版压力小：残余压盖应全零
        assert!(
            out.diagnostics.iter().all(|d| d.contains("overlap=false")),
            "残余压盖应全零: {:?}",
            out.diagnostics
        );
    }

    #[test]
    fn png_encodes_and_dump() {
        let out = render_house_map_png(&l_shape_house(), &full_spec()).unwrap();
        assert_eq!(out.scale, 200);
        assert!(!out.landscape);
        let png = match &out.data {
            ParcelMapData::Png(b) => b,
            ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        // PNG 魔数
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
        // A4 竖幅面（150dpi：1240×1754）
        assert!(pixmap.width() < pixmap.height(), "竖形房屋应竖放");
        // 黑色房屋轮廓/图廓墨迹存在
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() < 80 && p.green() < 80 && p.blue() < 80));
        // 落盘目检（仿 parcel_map_test）
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/house_map_test.png");
        std::fs::write(&path, png).unwrap();
    }
}
