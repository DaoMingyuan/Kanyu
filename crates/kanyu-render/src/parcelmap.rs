//! 宗地图渲染器 —— GB/T 42547-2023《地籍调查规程》图 L.3（土地使用权宗地图）版式。
//!
//! 页面（A4，竖/横自动）：
//! - 顶部居中标题「宗 地 图」+ 右上「单位：m㎡」；
//! - 头部信息框：宗地代码 / 所在图幅号 / 宗地面积 / 土地权利人；
//! - 地图框：界址线 0.3mm 红、界址点 Ø2.0mm 黑圆圈（圆心 0.2mm）、
//!   J 点号注记与边长注记（位置由 `kanyu_core::cartography` 勘测定界图注记契约计算）、
//!   宗地中央宗地号/地类编码分式（中等线 3.0mm）；
//! - 右侧界址点坐标表（点号 | X | Y | 边长；白底黑线 0.15mm；**X=纵坐标（北）、Y=横坐标（东）**
//!   测绘惯例；末行重复起点坐标闭合；边长挂接起点行；表尾宗地面积行）；
//! - 图框外下中比例尺 1:N（分母取整百）；右上指北针；
//! - 左下「XXXX年XX月解析法测绘界址点」+ 制图/审核日期；右下制图者/审核者；
//! - 左侧竖排单位名。
//!
//! 比例尺缺省自动求解：宗地 bbox 适配地图框后分母向上取整百。
//! SVG 全量（文字/旋转齐全）；PNG 经 `layout::TextBackend` 系统字体栈
//! （边长注记首版水平绘制于排版位置，不旋转字形——已知限制，SVG 为准）。

use crate::layout::{PageSize, TextBackend};
use crate::RenderError;
use kanyu_core::cartography::{
    self, BoundaryLineRecord, BoundaryPointRecord, ParcelBoundary, PlacementReport, RealestateRing,
};

/// 宗地图出图参数。
#[derive(Debug, Clone)]
pub struct ParcelMapSpec {
    /// 宗地代码（完整 19 位或任意长度；分式分子取末 7 位）。
    pub parcel_code: String,
    /// 土地权利人。
    pub owner: String,
    /// 所在图幅号。
    pub map_sheet: String,
    /// 宗地面积（㎡；None 按几何现算）。
    pub area_sqm: Option<f64>,
    /// 地类编码（分式分母）。
    pub land_use: String,
    /// 左侧竖排单位名（如 XXX自然资源局）。
    pub unit_name: String,
    /// 测绘说明（左下，如「2026年08月解析法测绘界址点」）。
    pub survey_note: String,
    /// 制图者 / 审核者。
    pub drawer: String,
    /// 审核者。
    pub reviewer: String,
    /// 制图日期 / 审核日期。
    pub draw_date: String,
    /// 审核日期。
    pub review_date: String,
    /// 比例尺分母（None 自动取整百）。
    pub scale: Option<u32>,
    /// 分辨率（默认 150）。
    pub dpi: f64,
    /// 界址点号前缀（默认 J）。
    pub point_prefix: String,
}

impl Default for ParcelMapSpec {
    fn default() -> Self {
        Self {
            parcel_code: String::new(),
            owner: String::new(),
            map_sheet: String::new(),
            area_sqm: None,
            land_use: String::new(),
            unit_name: String::new(),
            survey_note: String::new(),
            drawer: String::new(),
            reviewer: String::new(),
            draw_date: String::new(),
            review_date: String::new(),
            scale: None,
            dpi: 150.0,
            point_prefix: "J".to_string(),
        }
    }
}

/// 出图结果（含实际比例尺与排版诊断）。
#[derive(Debug, Clone)]
pub struct ParcelMapOutput {
    /// 实际采用的比例尺分母。
    pub scale: u32,
    /// 排版诊断行（每条注记一行：文本/途径/净空/残余压盖）。
    pub diagnostics: Vec<String>,
    /// SVG 或 PNG 产物。
    pub data: ParcelMapData,
}

/// 产物载体。
#[derive(Debug, Clone)]
pub enum ParcelMapData {
    /// SVG 文本。
    Svg(String),
    /// PNG 字节。
    Png(Vec<u8>),
}

/// 渲染宗地图为 SVG（完整排版）。
pub fn render_parcel_map_svg(
    boundary: &ParcelBoundary,
    spec: &ParcelMapSpec,
) -> Result<ParcelMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    Ok(ParcelMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Svg(scene_to_svg(&scene.prims)),
    })
}

/// 渲染宗地图为 PNG（tiny-skia 光栅链 + TextBackend 系统字体栈）。
pub fn render_parcel_map_png(
    boundary: &ParcelBoundary,
    spec: &ParcelMapSpec,
) -> Result<ParcelMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    let png = scene_to_png(&scene.prims, spec.dpi, &TextBackend::system())?;
    Ok(ParcelMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Png(png),
    })
}

// ---------------------------------------------------------------------------
// 版式常量（毫米；A4 竖 210×297，图 L.3 首版固定竖向）
// ---------------------------------------------------------------------------

/// 页宽 / 页高。
const PAGE_W: f64 = 210.0;
const PAGE_H: f64 = 297.0;
/// 外图廓（0.5mm）/ 内图廓（0.2mm）。
const OUTER_RECT: [f64; 4] = [8.0, 8.0, 194.0, 281.0];
const INNER_RECT: [f64; 4] = [11.0, 11.0, 188.0, 275.0];
/// 标题基线 y 与字号（图名大号粗体）。
const TITLE_Y: f64 = 22.0;
const TITLE_FONT: f64 = 6.5;
/// 右上「单位：m㎡」。
const UNIT_NOTE_Y: f64 = 16.5;
const UNIT_NOTE_FONT: f64 = 2.6;
/// 头部信息框纵向区间与四栏分界（宗地代码 | 所在图幅号 | 宗地面积 | 土地权利人）。
/// 栏宽按内容估算分配：代码 53 / 图幅号 34 / 面积 36 / 权利人 65（毫米）。
const HEADER_Y0: f64 = 25.0;
const HEADER_Y1: f64 = 36.0;
const HEADER_COLS: [f64; 5] = [11.0, 64.0, 98.0, 134.0, 199.0];
/// 地图框（主体；即内图廓 y 36→272 段，底为签注带）。
const MAP_RECT: [f64; 4] = [11.0, 36.0, 188.0, 236.0];
/// 宗地适配留白（地图框内沿）。
const MAP_PAD: f64 = 10.0;
/// 底部签注带上沿。
const FOOT_Y0: f64 = 272.0;
/// 指北针（地图框内右上）中心 x / 三角顶 y。
const NORTH_X: f64 = 190.0;
const NORTH_TOP_Y: f64 = 42.0;
/// 坐标表字号 / 题行字号 / 行高 / 题行高 / 单元格横向留白 / 锚定内边距。
const TABLE_FONT: f64 = 2.2;
const TABLE_TITLE_FONT: f64 = 3.2;
const TABLE_ROW_H: f64 = 4.2;
const TABLE_TITLE_H: f64 = 5.0;
const TABLE_CELL_PAD: f64 = 1.5;
const TABLE_ANCHOR_PAD: f64 = 3.0;
/// 坐标表与宗地适配区间的竖向间隔。
const TABLE_FIT_GAP: f64 = 4.0;
/// 适配区最小高（点极多时兜底）。
const FIT_MIN_H: f64 = 40.0;
/// 颜色（黑 / 界址线红 / 白）。
const BLACK: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];

// ---------------------------------------------------------------------------
// 场景图元（毫米纸面坐标；SVG/PNG 双后端共用一份几何）
// ---------------------------------------------------------------------------

/// 描边（毫米线宽 + RGB）。
#[derive(Debug, Clone, Copy)]
struct Stroke {
    width: f64,
    color: [u8; 3],
}

/// 文本锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Start,
    Middle,
    End,
}

/// 场景图元。
#[derive(Debug, Clone)]
enum Prim {
    /// 矩形（图廓 / 表格外框）。
    Rect {
        rect: [f64; 4],
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 折线 / 多边形（界址线、表格线、分数线、指北针三角）。
    Path {
        pts: Vec<(f64, f64)>,
        close: bool,
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 圆（界址点符号 / 圆心点）。
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 文本（vcenter 时 y 为竖向中心，否则 y 为基线）。
    Text {
        x: f64,
        y: f64,
        font: f64,
        text: String,
        anchor: Anchor,
        rotate_deg: f64,
        vcenter: bool,
        bold: bool,
    },
}

/// 排版场景（图元 + 实际比例尺 + 诊断）。
struct Scene {
    prims: Vec<Prim>,
    scale: u32,
    diagnostics: Vec<String>,
}

// ---------------------------------------------------------------------------
// 界址点坐标表（开发计划 §6：点号 | X | Y | 边长；X=纵坐标（北）、Y=横坐标（东））
// ---------------------------------------------------------------------------

/// 坐标表模型。
struct CoordTable {
    /// 表头。
    headers: [String; 4],
    /// 数据行（末行为重复 1 号点坐标的闭合行，无边长）。
    rows: Vec<[String; 4]>,
    /// 表尾面积行标签（跨前三栏）。
    area_label: String,
    /// 面积值（2 位小数）。
    area_value: String,
    /// 栏宽（毫米，含两侧留白）。
    col_w: [f64; 4],
}

impl CoordTable {
    /// 组表：按点号升序逐行；边长挂接起点行（start_no == point_no）；末行重复 1 号点闭合。
    fn build(points: &[BoundaryPointRecord], lines: &[BoundaryLineRecord], area: f64) -> Self {
        let mut rows: Vec<[String; 4]> = points
            .iter()
            .map(|p| {
                let edge = lines
                    .iter()
                    .find(|l| l.start_no == p.point_no)
                    .map(|l| cartography::format_edge_length(l.length_m))
                    .unwrap_or_default();
                [
                    p.label(),
                    format!("{:.3}", p.y), // X = 纵坐标（北）
                    format!("{:.3}", p.x), // Y = 横坐标（东）
                    edge,
                ]
            })
            .collect();
        // 闭合行：重复 1 号点坐标（点号仍写 1 号点号），无边长
        if let Some(first) = points.first() {
            rows.push([
                first.label(),
                format!("{:.3}", first.y),
                format!("{:.3}", first.x),
                String::new(),
            ]);
        }
        let headers = [
            "点号".to_string(),
            "X坐标(m)".to_string(),
            "Y坐标(m)".to_string(),
            "边长(m)".to_string(),
        ];
        let area_label = "宗地面积(平方米)".to_string();
        let area_value = format!("{area:.2}");
        // 栏宽：内容估算宽 + 两侧留白（点号栏保底）
        let mut col_w = [0.0_f64; 4];
        for (c, w) in col_w.iter_mut().enumerate() {
            let mut width = cartography::text_extent_mm(&headers[c], TABLE_FONT).0;
            for row in &rows {
                width = width.max(cartography::text_extent_mm(&row[c], TABLE_FONT).0);
            }
            *w = width + TABLE_CELL_PAD * 2.0;
        }
        col_w[0] = col_w[0].max(9.0);
        col_w[3] = col_w[3]
            .max(cartography::text_extent_mm(&area_value, TABLE_FONT).0 + TABLE_CELL_PAD * 2.0);
        // 面积行标签跨前三栏：不足时补到第 3 栏
        let label_w = cartography::text_extent_mm(&area_label, TABLE_FONT).0 + TABLE_CELL_PAD * 2.0;
        let first3: f64 = col_w[..3].iter().sum();
        if first3 < label_w {
            col_w[2] += label_w - first3;
        }
        Self {
            headers,
            rows,
            area_label,
            area_value,
            col_w,
        }
    }

    /// 表总宽（毫米）。
    fn width(&self) -> f64 {
        self.col_w.iter().sum()
    }

    /// 表总行数（题行 + 表头 + 数据/闭合行 + 面积行）。
    fn row_count(&self) -> usize {
        self.rows.len() + 3
    }

    /// 表总高（毫米）。
    fn height(&self) -> f64 {
        TABLE_TITLE_H + TABLE_ROW_H * (self.row_count() - 1) as f64
    }

    /// 出图元：白底黑线 0.15mm，锚定地图框右下角。
    fn emit(&self, prims: &mut Vec<Prim>) {
        let x1 = MAP_RECT[0] + MAP_RECT[2] - TABLE_ANCHOR_PAD;
        let y1 = MAP_RECT[1] + MAP_RECT[3] - TABLE_ANCHOR_PAD;
        let x0 = x1 - self.width();
        let y0 = y1 - self.height();
        let thin = Some(Stroke {
            width: 0.15,
            color: BLACK,
        });
        prims.push(Prim::Rect {
            rect: [x0, y0, self.width(), self.height()],
            fill: Some(WHITE),
            stroke: thin,
        });
        // 栏分界 x（左起累计）
        let mut col_x = [0.0_f64; 5];
        col_x[0] = x0;
        for c in 0..4 {
            col_x[c + 1] = col_x[c] + self.col_w[c];
        }
        let n_rows = self.row_count();
        // 行界 y（第 r 行上沿）：r=0 → y0；r≥1 → y0 + 题行高 + (r-1)×行高
        let row_y = |r: usize| {
            if r == 0 {
                y0
            } else {
                y0 + TABLE_TITLE_H + (r - 1) as f64 * TABLE_ROW_H
            }
        };
        // 横线（逐行分界）
        for r in 1..n_rows {
            let y = row_y(r);
            prims.push(Prim::Path {
                pts: vec![(x0, y), (x1, y)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 竖线：表头行起；面积行仅值栏分界（前三栏通栏）
        let head_y = row_y(1);
        let area_y = row_y(n_rows - 1);
        for (i, &x) in col_x[1..4].iter().enumerate() {
            let yb = if i == 2 { y1 } else { area_y };
            prims.push(Prim::Path {
                pts: vec![(x, head_y), (x, yb)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 单元格文本（通栏居中）
        let cell = |prims: &mut Vec<Prim>, text: &str, xa: f64, xb: f64, r: usize, font: f64| {
            prims.push(Prim::Text {
                x: (xa + xb) / 2.0,
                y: (row_y(r) + row_y(r + 1)) / 2.0,
                font,
                text: text.to_string(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        };
        cell(prims, "界址点坐标表", x0, x1, 0, TABLE_TITLE_FONT);
        for c in 0..4 {
            cell(
                prims,
                &self.headers[c],
                col_x[c],
                col_x[c + 1],
                1,
                TABLE_FONT,
            );
        }
        for (r, row) in self.rows.iter().enumerate() {
            for c in 0..4 {
                cell(prims, &row[c], col_x[c], col_x[c + 1], r + 2, TABLE_FONT);
            }
        }
        let last = n_rows - 1;
        cell(prims, &self.area_label, x0, col_x[3], last, TABLE_FONT);
        cell(prims, &self.area_value, col_x[3], x1, last, TABLE_FONT);
    }
}

// ---------------------------------------------------------------------------
// 场景构建
// ---------------------------------------------------------------------------

/// 比例尺分母向上取整百（786→800；下限 100）。
fn round_up_hundred(raw: f64) -> u32 {
    if !raw.is_finite() || raw <= 100.0 {
        return 100;
    }
    (raw / 100.0).ceil() as u32 * 100
}

/// 环 bbox（min_x, min_y, max_x, max_y）。
fn ring_bbox(ring: &RealestateRing) -> (f64, f64, f64, f64) {
    ring.points.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)),
    )
}

/// 外环质心（面积加权；退化时回退 bbox 中心）。
fn ring_centroid(ring: &RealestateRing) -> (f64, f64) {
    let (mut cross_sum, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for w in ring.points.windows(2) {
        let cross = w[0].0 * w[1].1 - w[1].0 * w[0].1;
        cross_sum += cross;
        cx += (w[0].0 + w[1].0) * cross;
        cy += (w[0].1 + w[1].1) * cross;
    }
    if cross_sum.abs() < 1e-12 {
        let (min_x, min_y, max_x, max_y) = ring_bbox(ring);
        return ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    }
    (cx / (3.0 * cross_sum), cy / (3.0 * cross_sum))
}

/// 排版诊断汇总（每条注记一行：text reason=… clearance=…mm overlap=…）。
fn diagnostics_of(reports: &[&PlacementReport]) -> Vec<String> {
    let mut out = Vec::new();
    for report in reports {
        for l in &report.labels {
            out.push(format!(
                "{} reason={} clearance={:.2}mm overlap={}",
                l.text, l.reason, l.clearance_mm, l.overlap
            ));
        }
    }
    out
}

/// 组场景：版面几何 + 注记排版 + 诊断（SVG/PNG 共用）。
fn build_scene(boundary: &ParcelBoundary, spec: &ParcelMapSpec) -> Result<Scene, RenderError> {
    let points = cartography::generate_boundary_points(boundary, &spec.point_prefix);
    if points.is_empty() {
        return Err(RenderError::InvalidStyle("宗地几何无界址点".to_string()));
    }
    let lines = cartography::generate_boundary_lines(boundary, &points);
    // 宗地面积：缺省按鞋带面积现算（外环 − 内环）
    let area = spec.area_sqm.unwrap_or_else(|| {
        let ext = cartography::ring_area(&boundary.exterior).abs();
        let holes: f64 = boundary
            .interiors
            .iter()
            .map(|r| cartography::ring_area(r).abs())
            .sum();
        (ext - holes).max(0.0)
    });
    // 坐标表（先组表：表高参与适配区与自动比例尺求解）
    let table = CoordTable::build(&points, &lines, area);
    // 宗地适配区：地图框扣除留白与底部坐标表带
    let fit_w = MAP_RECT[2] - 2.0 * MAP_PAD;
    let fit_h = (MAP_RECT[3] - 2.0 * MAP_PAD - table.height() - TABLE_FIT_GAP).max(FIT_MIN_H);
    let (min_x, min_y, max_x, max_y) = ring_bbox(&boundary.exterior);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    // 比例尺：spec 给定直接用；缺省取宽/高方向较大 raw 分母向上整百
    let scale = match spec.scale {
        Some(s) => s,
        None => round_up_hundred((span_x * 1000.0 / fit_w).max(span_y * 1000.0 / fit_h)),
    };
    // 地图单位（米）→ 纸面毫米：mu = 1000 / scale；bbox 中心对适配区中心，北朝上
    let mu = 1000.0 / f64::from(scale);
    let fit_cx = MAP_RECT[0] + MAP_PAD + fit_w / 2.0;
    let fit_cy = MAP_RECT[1] + MAP_PAD + fit_h / 2.0;
    let bbox_c = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let to_page = |(x, y): (f64, f64)| (fit_cx + (x - bbox_c.0) * mu, fit_cy - (y - bbox_c.1) * mu);

    // 注记排版（点号先排，其矩形作为边长注记的附加障碍）
    let point_report = cartography::place_point_labels(
        boundary,
        &points,
        &cartography::PointLabelOptions {
            scale,
            ..Default::default()
        },
        &[],
    );
    let point_rects: Vec<_> = point_report.labels.iter().map(|l| l.rect).collect();
    let edge_report = cartography::place_edge_labels(
        boundary,
        &lines,
        &points,
        &cartography::EdgeLabelOptions {
            scale,
            ..Default::default()
        },
        &point_rects,
    );
    let diagnostics = diagnostics_of(&[&point_report, &edge_report]);

    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    let mut prims: Vec<Prim> = Vec::with_capacity(128);
    // —— 图廓（外粗内细）+ 头部/签注带分隔线 ——
    prims.push(Prim::Rect {
        rect: OUTER_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.5,
            color: BLACK,
        }),
    });
    prims.push(Prim::Rect {
        rect: INNER_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.2,
            color: BLACK,
        }),
    });
    for y in [HEADER_Y1, FOOT_Y0] {
        prims.push(Prim::Path {
            pts: vec![(INNER_RECT[0], y), (INNER_RECT[0] + INNER_RECT[2], y)],
            close: false,
            fill: None,
            stroke: thin,
        });
    }
    // —— 标题（顶部居中）+ 右上「单位：m㎡」——
    prims.push(Prim::Text {
        x: PAGE_W / 2.0,
        y: TITLE_Y,
        font: TITLE_FONT,
        text: "宗 地 图".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: true,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 2.0,
        y: UNIT_NOTE_Y,
        font: UNIT_NOTE_FONT,
        text: "单位：m㎡".to_string(),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 头部信息框（面积一栏按金样加大字号）——
    let header_texts = [
        format!("宗地代码：{}", spec.parcel_code),
        format!("所在图幅号：{}", spec.map_sheet),
        format!("宗地面积：{area:.2}"),
        format!("土地权利人：{}", spec.owner),
    ];
    for (c, text) in header_texts.iter().enumerate() {
        prims.push(Prim::Text {
            x: (HEADER_COLS[c] + HEADER_COLS[c + 1]) / 2.0,
            y: (HEADER_Y0 + HEADER_Y1) / 2.0,
            font: if c == 2 { 3.4 } else { 2.7 },
            text: text.clone(),
            anchor: Anchor::Middle,
            rotate_deg: 0.0,
            vcenter: true,
            bold: false,
        });
    }
    // —— 指北针（地图框内右上：「北」字 + 细长三角，对齐 GB/T 42547 图 L.3 样图）——
    prims.push(Prim::Text {
        x: NORTH_X,
        y: NORTH_TOP_Y + 1.6,
        font: 3.4,
        text: "北".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Path {
        pts: vec![
            (NORTH_X, NORTH_TOP_Y + 4.0),
            (NORTH_X - 1.4, NORTH_TOP_Y + 14.0),
            (NORTH_X, NORTH_TOP_Y + 12.0),
            (NORTH_X + 1.4, NORTH_TOP_Y + 14.0),
        ],
        close: true,
        fill: Some(BLACK),
        stroke: None,
    });
    // —— 界址线（0.3mm 红）——
    for ring in boundary.rings() {
        prims.push(Prim::Path {
            pts: ring.points.iter().map(|&p| to_page(p)).collect(),
            close: true,
            fill: None,
            stroke: Some(Stroke {
                width: 0.3,
                color: RED,
            }),
        });
    }
    // —— 界址点（Ø2.0mm 黑圆圈 + Ø0.2mm 圆心点，线粗 0.15mm）——
    for p in &points {
        let (x, y) = to_page((p.x, p.y));
        prims.push(Prim::Circle {
            cx: x,
            cy: y,
            r: 1.0,
            fill: Some(WHITE),
            stroke: thin,
        });
        prims.push(Prim::Circle {
            cx: x,
            cy: y,
            r: 0.1,
            fill: Some(BLACK),
            stroke: None,
        });
    }
    // —— J 点号注记 / 边长注记（2.4mm，位置与旋转按 cartography 排版结果）——
    for l in point_report.labels.iter().chain(edge_report.labels.iter()) {
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
    // —— 宗地中央分式（分子=宗地代码末 7 位 / 分母=地类编码，3.0mm 中等线，分数线横贯）——
    let code_tail: String = {
        let chars: Vec<char> = spec.parcel_code.chars().collect();
        chars.iter().skip(chars.len().saturating_sub(7)).collect()
    };
    let (gcx, gcy) = to_page(ring_centroid(&boundary.exterior));
    let half = (cartography::text_extent_mm(&code_tail, 3.0)
        .0
        .max(cartography::text_extent_mm(&spec.land_use, 3.0).0))
        / 2.0
        + 1.0;
    prims.push(Prim::Text {
        x: gcx,
        y: gcy - 2.3,
        font: 3.0,
        text: code_tail,
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Path {
        pts: vec![(gcx - half, gcy), (gcx + half, gcy)],
        close: false,
        fill: None,
        stroke: thin,
    });
    prims.push(Prim::Text {
        x: gcx,
        y: gcy + 2.3,
        font: 3.0,
        text: spec.land_use.clone(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    // —— 界址点坐标表（地图框右下锚定）——
    table.emit(&mut prims);
    // —— 底部签注带 ——
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 277.2,
        font: 2.5,
        text: spec.survey_note.clone(),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 281.2,
        font: 2.5,
        text: format!("制图日期：{}", spec.draw_date),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 285.0,
        font: 2.5,
        text: format!("审核日期：{}", spec.review_date),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: PAGE_W / 2.0,
        y: (FOOT_Y0 + INNER_RECT[1] + INNER_RECT[3]) / 2.0,
        font: 3.6,
        text: format!("1:{scale}"),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 3.0,
        y: 278.5,
        font: 2.6,
        text: format!("制图：{}", spec.drawer),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 3.0,
        y: 283.5,
        font: 2.6,
        text: format!("审核：{}", spec.reviewer),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 左侧竖排单位名（地图框内左缘，逐字纵向堆叠）——
    let unit_chars: Vec<char> = spec.unit_name.chars().collect();
    if !unit_chars.is_empty() {
        let step = 3.4;
        let mid = MAP_RECT[1] + MAP_RECT[3] / 2.0;
        let y_start = mid - (unit_chars.len() as f64 - 1.0) * step / 2.0;
        for (i, ch) in unit_chars.iter().enumerate() {
            prims.push(Prim::Text {
                x: MAP_RECT[0] + 3.0,
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
        scale,
        diagnostics,
    })
}

// ---------------------------------------------------------------------------
// SVG 后端
// ---------------------------------------------------------------------------

/// 文本 XML 转义。
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// RGB → #RRGGBB。
fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// fill 属性。
fn fill_attr(fill: Option<[u8; 3]>) -> String {
    match fill {
        Some(c) => format!("fill=\"{}\"", hex(c)),
        None => "fill=\"none\"".to_string(),
    }
}

/// stroke 属性串。
fn stroke_attrs(stroke: Option<Stroke>) -> String {
    match stroke {
        Some(s) => format!(
            " stroke=\"{}\" stroke-width=\"{:.2}\"",
            hex(s.color),
            s.width
        ),
        None => String::new(),
    }
}

/// 场景 → SVG 文档（viewBox 即毫米坐标系）。
fn scene_to_svg(prims: &[Prim]) -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {PAGE_W:.0} {PAGE_H:.0}\" width=\"{PAGE_W:.0}mm\" height=\"{PAGE_H:.0}mm\">\n"
    ));
    out.push_str("<!-- kanyu-render parcelmap · GB/T 42547-2023 图 L.3 -->\n");
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

/// 场景 → PNG 字节（A4 竖；旋转注记按已知限制水平绘制于排版中心）。
fn scene_to_png(prims: &[Prim], dpi: f64, tb: &TextBackend) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{
        Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Shader, Stroke as SkStroke, Transform,
    };
    let (pw, ph) = PageSize::A4Portrait.pixels(dpi);
    let k = dpi / 25.4; // 毫米 → 像素
    let mut page = Pixmap::new(pw, ph).ok_or(RenderError::InvalidSize(pw, ph))?;
    page.fill(Color::from_rgba8(255, 255, 255, 255));
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
                    fill_stroke(&mut page, &path, fill, stroke);
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
                    fill_stroke(&mut page, &path, fill, stroke);
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
                    fill_stroke(&mut page, &path, fill, stroke);
                }
            }
            Prim::Text {
                x,
                y,
                font,
                text,
                anchor,
                vcenter,
                ..
            } => {
                let px = (font * k) as f32;
                let w = tb.measure(text, px);
                let x_px = (*x * k) as f32;
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
                    &mut page,
                    text,
                    sx,
                    baseline,
                    px,
                    Color::from_rgba8(0, 0, 0, 255),
                );
            }
        }
    }
    page.encode_png()
        .map_err(|e| RenderError::InvalidStyle(format!("宗地图 PNG 编码失败: {e}")))
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kanyu_core::cartography::RingRole;

    /// 测试宗地：40m×30m 矩形（投影坐标，东 39595000 起、北 4127000 起，CCW 存储）。
    fn test_boundary() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595000.0, 4127000.0),
                    (39595040.0, 4127000.0),
                    (39595040.0, 4127030.0),
                    (39595000.0, 4127030.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 完整 spec（比例尺缺省自动求解、面积缺省现算）。
    fn full_spec() -> ParcelMapSpec {
        ParcelMapSpec {
            parcel_code: "371602113005GB00032".to_string(),
            owner: "中国联合网络通信有限公司滨州市分公司".to_string(),
            map_sheet: "27.00-95.25".to_string(),
            area_sqm: None,
            land_use: "0801".to_string(),
            unit_name: "某某市自然资源局".to_string(),
            survey_note: "2026年08月解析法测绘界址点".to_string(),
            drawer: "XXX".to_string(),
            reviewer: "XXX".to_string(),
            draw_date: "2026年08月25日".to_string(),
            review_date: "2026年08月25日".to_string(),
            scale: None,
            dpi: 150.0,
            point_prefix: "J".to_string(),
        }
    }

    /// 从产物取 SVG 文本（断言辅助）。
    fn svg_of(out: &ParcelMapOutput) -> &str {
        match &out.data {
            ParcelMapData::Svg(s) => s,
            ParcelMapData::Png(_) => panic!("应为 SVG 产物"),
        }
    }

    #[test]
    fn svg_rect_parcel_full_spec() {
        let out = render_parcel_map_svg(&test_boundary(), &full_spec()).unwrap();
        // 自动比例尺手算验证：坐标表高 5.0+4.2×7=34.4mm → 适配区 168×177.6mm；
        // raw = max(40×1000/168, 30×1000/177.6) = max(238.1, 168.9) = 238.1 → 整百 300
        assert_eq!(out.scale, 300);
        let svg = svg_of(&out);
        // 版式要素：标题 / 单位注记 / 头部信息框 / 点号 / 比例尺 / 分式 / 坐标表
        for frag in [
            "宗 地 图",
            "单位：m㎡",
            "371602113005GB00032",
            "J1",
            "J4",
            "1:300",
            "GB00032", // 分式分子 = 宗地代码末 7 位
            "0801",    // 分式分母 = 地类编码
            "界址点坐标表",
            "宗地面积",
            "1200.00", // 面积缺省现算：40×30=1200.00
            "土地权利人：中国联合网络通信有限公司滨州市分公司",
            "2026年08月解析法测绘界址点",
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 坐标表 X 列 = 北坐标（测绘惯例）：J1 为西北角（东 39595000、北 4127030）
        assert!(svg.contains("4127030.000"), "X 列应为北坐标");
        assert!(svg.contains("39595000.000"), "Y 列应为东坐标");
        // 排版诊断非空（点号 + 边长逐条摘要）
        assert!(!out.diagnostics.is_empty());
        assert!(out.diagnostics.iter().any(|d| d.starts_with("J1 reason=")
            && d.contains("clearance=")
            && d.contains("overlap=")));
    }

    #[test]
    fn coord_table_rows_rules() {
        let boundary = test_boundary();
        let points = cartography::generate_boundary_points(&boundary, "J");
        let lines = cartography::generate_boundary_lines(&boundary, &points);
        let table = CoordTable::build(&points, &lines, 1200.0);
        // 行数 = 4 点 + 1 闭合行
        assert_eq!(table.rows.len(), 5);
        // 首行 J1（西北角起编）：X=北 4127030.000、Y=东 39595000.000；边长挂接 J1→J2 = 30.00
        assert_eq!(
            table.rows[0],
            ["J1", "4127030.000", "39595000.000", "30.00"]
        );
        // J2→J3 边长 40.00 挂在 J2 行
        assert_eq!(table.rows[1][0], "J2");
        assert_eq!(table.rows[1][3], "40.00");
        // 末行重复 1 号点坐标闭合（点号仍写 1 号点号），无边长
        assert_eq!(table.rows[4], ["J1", "4127030.000", "39595000.000", ""]);
        // 表尾面积行（2 位小数）
        assert_eq!(table.area_value, "1200.00");
    }

    #[test]
    fn given_scale_used_directly() {
        let spec = ParcelMapSpec {
            scale: Some(500),
            ..full_spec()
        };
        let out = render_parcel_map_svg(&test_boundary(), &spec).unwrap();
        assert_eq!(out.scale, 500);
        assert!(svg_of(&out).contains("1:500"));
    }

    #[test]
    fn png_encodes_and_dump() {
        let out = render_parcel_map_png(&test_boundary(), &full_spec()).unwrap();
        assert_eq!(out.scale, 300);
        let png = match &out.data {
            ParcelMapData::Png(b) => b,
            ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        // PNG 魔数
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
        // 红色界址线存在
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 80 && p.blue() < 80));
        // 落盘目检（仿 layout_cjk_test）
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/parcel_map_test.png");
        std::fs::write(&path, png).unwrap();
    }
}
