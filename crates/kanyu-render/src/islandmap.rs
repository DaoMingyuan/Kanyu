//! 用岛图件渲染器 —— GB/T 42547-2023《地籍调查规程》图 L.9 用岛范围图 /
//! 图 L.10 建筑物和设施布置图版式（A4 横向）。
//!
//! 页面（A4 横 297×210mm；几何/经纬网/表格与 [`crate::seamap`] 共享同一套排版助手）：
//! - 顶部居中图名（粗体，**无项目名前缀**）：L.9「用岛范围图」、
//!   L.10「建筑物和设施布置图」；左上「用岛代码：{用岛代码}（登记时填写或粘贴）」；
//! - **经纬网图廓**（同 seamap：自适应间隔 + DMS 注记顶/底横排、左/右竖排）；
//! - 地图区：用岛图斑填充 RGB(245,162,122)、界址线 0.5mm 红、界址点
//!   Ø2.0mm 黑圆圈白底（0.15mm）+ 圆心点（**无点号/边长注记**）；
//! - 右上**罗盘指北针**（十字罗盘：四向尖角星形，N 瓣最长，瓣体半黑半白 +
//!   N/E/W/S 四字母）；比例尺「1:N」地图框内**下中**（分母自动向上整百）；
//! - 左下**图例框**（题「图 例」+ 色块/线样 + 标注行，白底黑线 0.15mm）；
//! - 右下**网格签注表**（2 列 × 9 行：坐标系/比例尺=见下/投影方式=高斯-克吕格投影/
//!   中央经线=见坐标表/测绘单位/测量员/绘图员/审核人/绘制日期）；
//! - L.9：右侧**界址点编号及坐标表**（点号 | 纬度(北纬) | 经度(东经)，DMS 秒 3 位
//!   小数，末行重复起点闭合）+ 表下「用岛面积：{area}平方米」（2 位小数）；
//! - L.10：图斑上叠加**设施黄色图斑**（RGB(255,235,0) + 0.3mm 黑边）+ 设施编号
//!   （图斑内居中 2.4mm）；右侧**一览表**（编号 | 建筑物和设施名称 | 占地面积/㎡，
//!   末行「合计」跨前两栏 + 合计面积，SVG 加粗观感、PNG 忽略）。
//!
//! 比例尺缺省自动求解：用岛 bbox 适配地图框（扣除留白与右侧表带）后分母向上取整百。

use crate::layout::TextBackend;
use crate::seamap::{
    self, Anchor, GridTable, PageMap, Prim, Stroke, BLACK, COORD_TABLE_Y0, FIT_MIN_H, FIT_MIN_W,
    MAP_PAD, MAP_RECT, OUTER_RECT, PAGE_W, RED, SEA_CODE_FONT, SEA_CODE_Y, SEA_FILL,
    TABLE_ANCHOR_PAD, TABLE_FIT_GAP, TABLE_FONT, TABLE_ROW_H, TITLE_FONT, TITLE_Y, WHITE,
};
use crate::RenderError;
use kanyu_core::cartography::{self, ParcelBoundary};

/// 用岛图种（GB/T 42547-2023 附录 L）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IslandMapKind {
    /// 用岛范围图（图 L.9：界址点编号及坐标表 + 用岛面积行）。
    #[default]
    RangeMap,
    /// 建筑物和设施布置图（图 L.10：设施黄色图斑与编号 + 一览表）。
    FacilityMap,
}

impl IslandMapKind {
    /// 图名（无项目名前缀）。
    pub fn title(self) -> &'static str {
        match self {
            IslandMapKind::RangeMap => "用岛范围图",
            IslandMapKind::FacilityMap => "建筑物和设施布置图",
        }
    }
}

/// 建筑物和设施（L.10 图斑与一览表数据来源；L.9 忽略）。
#[derive(Debug, Clone)]
pub struct IslandFacility {
    /// 建筑物和设施名称（一览表名称栏）。
    pub name: String,
    /// 编号（图斑内居中注记与一览表首栏）。
    pub no: String,
    /// 占地面积（平方米；一览表面积栏与合计）。
    pub area_sqm: f64,
    /// 图斑多边形（地图坐标闭合环）。
    pub polygon: Vec<(f64, f64)>,
}

/// 用岛图件出图参数。
#[derive(Debug, Clone)]
pub struct IslandMapSpec {
    /// 图种（L.9 用岛范围图 / L.10 建筑物和设施布置图）。
    pub kind: IslandMapKind,
    /// 用岛代码（左上「登记时填写或粘贴」）。
    pub island_code: String,
    /// 源坐标系（EPSG:xxxx；L.9 界址点坐标表经此反算为 CGCS2000 经纬度度分秒；
    /// 空串回退 EPSG:4527）。
    pub source_epsg: String,
    /// 测绘单位（网格签注表）。
    pub survey_unit: String,
    /// 测量员。
    pub surveyor: String,
    /// 绘图员。
    pub drawer: String,
    /// 审核人。
    pub reviewer: String,
    /// 绘制日期。
    pub draw_date: String,
    /// 用岛面积（平方米；None 按几何现算（外环−内环）；L.9 面积行用）。
    pub area_sqm: Option<f64>,
    /// 建筑物和设施（L.10 用；L.9 忽略）。
    pub facilities: Vec<IslandFacility>,
    /// 比例尺分母（None 自动取整百）。
    pub scale: Option<u32>,
    /// 分辨率（默认 150，仅 PNG）。
    pub dpi: f64,
}

impl Default for IslandMapSpec {
    fn default() -> Self {
        Self {
            kind: IslandMapKind::RangeMap,
            island_code: String::new(),
            source_epsg: "EPSG:4527".to_string(),
            survey_unit: String::new(),
            surveyor: String::new(),
            drawer: String::new(),
            reviewer: String::new(),
            draw_date: String::new(),
            area_sqm: None,
            facilities: Vec::new(),
            scale: None,
            dpi: 150.0,
        }
    }
}

/// 出图结果（比例尺 + 诊断 + 产物）。
#[derive(Debug, Clone)]
pub struct IslandMapOutput {
    /// 实际比例尺分母。
    pub scale: u32,
    /// 排版诊断行（用岛图件无点号/边长注记排版，通常为空）。
    pub diagnostics: Vec<String>,
    /// SVG 或 PNG 产物。
    pub data: crate::parcelmap::ParcelMapData,
}

/// 渲染用岛图件为 SVG（完整排版）。
pub fn render_island_map_svg(
    boundary: &ParcelBoundary,
    spec: &IslandMapSpec,
) -> Result<IslandMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    Ok(IslandMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: crate::parcelmap::ParcelMapData::Svg(seamap::scene_to_svg(&scene.prims)),
    })
}

/// 渲染用岛图件为 PNG（tiny-skia 光栅链 + TextBackend 系统字体栈）。
pub fn render_island_map_png(
    boundary: &ParcelBoundary,
    spec: &IslandMapSpec,
) -> Result<IslandMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    let png = seamap::scene_to_png(&scene.prims, spec.dpi, &TextBackend::system())?;
    Ok(IslandMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: crate::parcelmap::ParcelMapData::Png(png),
    })
}

// ---------------------------------------------------------------------------
// 版式常量（毫米；页面/图廓/表格常量共享 seamap，本节仅为用岛特有要素）
// ---------------------------------------------------------------------------

/// 设施图斑填充黄（RGB(255,235,0)）。
const FACILITY_FILL: [u8; 3] = [255, 235, 0];
/// 罗盘指北针（地图框内右上）中心距图廓右缘 / 顶缘。
const COMPASS_DX: f64 = 10.0;
const COMPASS_DY: f64 = 12.0;
/// 罗盘瓣长（N 瓣最长）与半宽。
const COMPASS_LEN_N: f64 = 6.5;
const COMPASS_LEN_EW: f64 = 4.2;
const COMPASS_LEN_S: f64 = 4.6;
const COMPASS_HALF_W: f64 = 1.0;
/// 罗盘字母字号与字母中心距瓣尖距离。
const COMPASS_FONT: f64 = 2.4;
const COMPASS_LABEL_OFF: f64 = 1.7;
/// 设施编号字号（图斑内居中黑字）。
const FACILITY_NO_FONT: f64 = 2.4;
/// 用岛面积行字号与坐标表下沿间距。
const AREA_FONT: f64 = 2.6;
const AREA_GAP: f64 = 4.0;
/// 图例框：内边距 / 色块宽×高 / 色块与标注间距 / 行高 / 题高 / 题字号 / 标注字号 /
/// 距地图框左缘与底缘距离。
const LEGEND_PAD: f64 = 2.5;
const LEGEND_SWATCH_W: f64 = 7.0;
const LEGEND_SWATCH_H: f64 = 3.5;
const LEGEND_SWATCH_GAP: f64 = 2.0;
const LEGEND_ROW_H: f64 = 5.5;
const LEGEND_TITLE_H: f64 = 4.5;
const LEGEND_TITLE_FONT: f64 = 3.0;
const LEGEND_FONT: f64 = 2.4;
const LEGEND_DX: f64 = 4.0;
const LEGEND_DY: f64 = 4.0;

// ---------------------------------------------------------------------------
// 表格（签注表复用 seamap::GridTable；一览表支持合计跨栏，本地实现）
// ---------------------------------------------------------------------------

/// 网格签注表（2 列 × 9 行，无题行/表头；较宗海签注表多比例尺/投影方式/
/// 中央经线三行，少高程基准/检查人）。
fn build_sign_table(spec: &IslandMapSpec) -> GridTable {
    let rows: Vec<Vec<String>> = vec![
        vec!["坐标系".to_string(), "2000国家大地坐标系".to_string()],
        vec!["比例尺".to_string(), "见下".to_string()],
        vec!["投影方式".to_string(), "高斯-克吕格投影".to_string()],
        vec!["中央经线".to_string(), "见坐标表".to_string()],
        vec!["测绘单位".to_string(), spec.survey_unit.clone()],
        vec!["测量员".to_string(), spec.surveyor.clone()],
        vec!["绘图员".to_string(), spec.drawer.clone()],
        vec!["审核人".to_string(), spec.reviewer.clone()],
        vec!["绘制日期".to_string(), spec.draw_date.clone()],
    ];
    let col_w = seamap::col_widths(&rows, &[12.0, 24.0]);
    GridTable {
        title: None,
        rows,
        col_w,
    }
}

/// 建筑物和设施一览表模型（编号 | 建筑物和设施名称 | 占地面积/㎡；
/// 末行「合计」跨前两栏 + 合计面积，加粗观感）。
struct FacilityTable {
    /// 全部行（首行表头，其余数据行 [编号, 名称, 面积串]）。
    rows: Vec<Vec<String>>,
    /// 合计面积串（2 位小数）。
    total: String,
    /// 栏宽（毫米，含两侧留白）。
    col_w: Vec<f64>,
}

/// 组一览表：面积 2 位小数；空设施时仅表头 + 合计 0.00（诚实空态）。
fn build_facility_table(facilities: &[IslandFacility]) -> FacilityTable {
    let mut rows: Vec<Vec<String>> = vec![vec![
        "编号".to_string(),
        "建筑物和设施名称".to_string(),
        "占地面积/㎡".to_string(),
    ]];
    let mut total = 0.0_f64;
    for f in facilities {
        total += f.area_sqm;
        rows.push(vec![
            f.no.clone(),
            f.name.clone(),
            format!("{:.2}", f.area_sqm),
        ]);
    }
    let total_s = format!("{total:.2}");
    // 栏宽估算纳入合计行（「合计」跨前两栏按首栏内容计）
    let mut measure_rows = rows.clone();
    measure_rows.push(vec!["合计".to_string(), String::new(), total_s.clone()]);
    let col_w = seamap::col_widths(&measure_rows, &[8.0, 24.0, 12.0]);
    FacilityTable {
        rows,
        total: total_s,
        col_w,
    }
}

impl FacilityTable {
    /// 表总宽（毫米）。
    fn width(&self) -> f64 {
        self.col_w.iter().sum()
    }

    /// 表总高（毫米；数据行 + 合计行）。
    fn height(&self) -> f64 {
        TABLE_ROW_H * (self.rows.len() + 1) as f64
    }

    /// 出图元：白底黑线 0.15mm，右上锚定（x1 = 右缘，y0 = 上沿）；
    /// 合计行前两栏合并（栏界竖线在合计行断开），「合计」与合计面积加粗。
    fn emit(&self, prims: &mut Vec<Prim>, x1: f64, y0: f64) {
        let thin = Some(Stroke {
            width: 0.15,
            color: BLACK,
        });
        let (w, h) = (self.width(), self.height());
        let x0 = x1 - w;
        prims.push(Prim::Rect {
            rect: [x0, y0, w, h],
            fill: Some(WHITE),
            stroke: thin,
        });
        // 栏分界 x（左起累计）
        let mut xs = Vec::with_capacity(self.col_w.len() + 1);
        xs.push(x0);
        for cw in &self.col_w {
            xs.push(xs.last().unwrap() + cw);
        }
        let data_h = TABLE_ROW_H * self.rows.len() as f64; // 合计行上缘
                                                           // 横线：逐行分界（外框四边已由 Rect 承担）
        for r in 1..=self.rows.len() {
            let y = y0 + r as f64 * TABLE_ROW_H;
            prims.push(Prim::Path {
                pts: vec![(x0, y), (x1, y)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 竖线：首栏界止于合计行上缘（合计跨前两栏），其余贯通
        for (i, &x) in xs[1..xs.len() - 1].iter().enumerate() {
            let y_bot = if i == 0 { y0 + data_h } else { y0 + h };
            prims.push(Prim::Path {
                pts: vec![(x, y0), (x, y_bot)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 表头与数据行文本（通栏居中）
        for (r, row) in self.rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                prims.push(Prim::Text {
                    x: (xs[c] + xs[c + 1]) / 2.0,
                    y: y0 + r as f64 * TABLE_ROW_H + TABLE_ROW_H / 2.0,
                    font: TABLE_FONT,
                    text: text.clone(),
                    anchor: Anchor::Middle,
                    rotate_deg: 0.0,
                    vcenter: true,
                    bold: false,
                });
            }
        }
        // 合计行：「合计」跨前两栏居中 + 合计面积（加粗观感；PNG 后端忽略 bold）
        let total_y = y0 + data_h + TABLE_ROW_H / 2.0;
        prims.push(Prim::Text {
            x: (xs[0] + xs[2]) / 2.0,
            y: total_y,
            font: TABLE_FONT,
            text: "合计".to_string(),
            anchor: Anchor::Middle,
            rotate_deg: 0.0,
            vcenter: true,
            bold: true,
        });
        prims.push(Prim::Text {
            x: (xs[2] + xs[3]) / 2.0,
            y: total_y,
            font: TABLE_FONT,
            text: self.total.clone(),
            anchor: Anchor::Middle,
            rotate_deg: 0.0,
            vcenter: true,
            bold: true,
        });
    }
}

// ---------------------------------------------------------------------------
// 罗盘指北针 / 图例
// ---------------------------------------------------------------------------

/// 罗盘指北针（地图框内右上）：四向尖角星形（上下左右四瓣，N 瓣最长；
/// 每瓣沿中线半黑半白，白瓣 0.1mm 黑描边）+ 中心白点 + N/E/W/S 四字母。
fn emit_compass(prims: &mut Vec<Prim>) {
    let cx = MAP_RECT[0] + MAP_RECT[2] - COMPASS_DX;
    let cy = MAP_RECT[1] + COMPASS_DY;
    // 四瓣：（瓣尖， 底左， 底右）——底左/底右为瓣根中线两侧半宽点
    let petals = [
        (
            (cx, cy - COMPASS_LEN_N),
            (cx - COMPASS_HALF_W, cy),
            (cx + COMPASS_HALF_W, cy),
        ),
        (
            (cx + COMPASS_LEN_EW, cy),
            (cx, cy - COMPASS_HALF_W),
            (cx, cy + COMPASS_HALF_W),
        ),
        (
            (cx, cy + COMPASS_LEN_S),
            (cx - COMPASS_HALF_W, cy),
            (cx + COMPASS_HALF_W, cy),
        ),
        (
            (cx - COMPASS_LEN_EW, cy),
            (cx, cy - COMPASS_HALF_W),
            (cx, cy + COMPASS_HALF_W),
        ),
    ];
    let white_edge = Some(Stroke {
        width: 0.1,
        color: BLACK,
    });
    for (tip, bl, br) in petals {
        // 左半瓣黑填充；右半瓣白填充黑描边（经典罗盘玫瑰观感）
        prims.push(Prim::Path {
            pts: vec![tip, bl, (cx, cy)],
            close: true,
            fill: Some(BLACK),
            stroke: None,
        });
        prims.push(Prim::Path {
            pts: vec![tip, (cx, cy), br],
            close: true,
            fill: Some(WHITE),
            stroke: white_edge,
        });
    }
    // 中心白点（盖瓣根缝合）
    prims.push(Prim::Circle {
        cx,
        cy,
        r: 0.5,
        fill: Some(WHITE),
        stroke: Some(Stroke {
            width: 0.15,
            color: BLACK,
        }),
    });
    // N/E/W/S 四字母（瓣尖外侧）
    for (x, y, ch) in [
        (cx, cy - COMPASS_LEN_N - COMPASS_LABEL_OFF, "N"),
        (cx + COMPASS_LEN_EW + COMPASS_LABEL_OFF, cy, "E"),
        (cx, cy + COMPASS_LEN_S + COMPASS_LABEL_OFF, "S"),
        (cx - COMPASS_LEN_EW - COMPASS_LABEL_OFF, cy, "W"),
    ] {
        prims.push(Prim::Text {
            x,
            y,
            font: COMPASS_FONT,
            text: ch.to_string(),
            anchor: Anchor::Middle,
            rotate_deg: 0.0,
            vcenter: true,
            bold: false,
        });
    }
}

/// 图例行（色块填充/边色/边宽 + 标注文本）。
struct LegendRow {
    fill: Option<[u8; 3]>,
    stroke: [u8; 3],
    stroke_w: f64,
    label: String,
}

/// 图例行按图种：L.9 = 用岛范围（图斑色块）/ 海岛岸线/界址线（红线样）；
/// L.10 = 界址线（红线样）/ 建筑物和设施（黄色块）。
fn legend_rows(kind: IslandMapKind) -> Vec<LegendRow> {
    match kind {
        IslandMapKind::RangeMap => vec![
            LegendRow {
                fill: Some(SEA_FILL),
                stroke: RED,
                stroke_w: 0.3,
                label: "用岛范围".to_string(),
            },
            LegendRow {
                fill: None,
                stroke: RED,
                stroke_w: 0.5,
                label: "海岛岸线/界址线".to_string(),
            },
        ],
        IslandMapKind::FacilityMap => vec![
            LegendRow {
                fill: None,
                stroke: RED,
                stroke_w: 0.5,
                label: "界址线".to_string(),
            },
            LegendRow {
                fill: Some(FACILITY_FILL),
                stroke: BLACK,
                stroke_w: 0.3,
                label: "建筑物和设施".to_string(),
            },
        ],
    }
}

/// 图例框（地图框内左下）：题「图 例」居中 + 色块/线样 + 标注行，白底黑线 0.15mm。
fn emit_legend(prims: &mut Vec<Prim>, kind: IslandMapKind) {
    let rows = legend_rows(kind);
    let title = "图 例";
    // 框宽：题宽与最长行（色块 + 间距 + 标注）取大者 + 两侧内边距
    let title_w = cartography::text_extent_mm(title, LEGEND_TITLE_FONT).0;
    let label_max = rows
        .iter()
        .map(|r| cartography::text_extent_mm(&r.label, LEGEND_FONT).0)
        .fold(0.0_f64, f64::max);
    let content_w = (LEGEND_SWATCH_W + LEGEND_SWATCH_GAP + label_max).max(title_w);
    let w = content_w + LEGEND_PAD * 2.0;
    let h = LEGEND_PAD * 2.0 + LEGEND_TITLE_H + LEGEND_ROW_H * rows.len() as f64;
    let x0 = MAP_RECT[0] + LEGEND_DX;
    let y0 = MAP_RECT[1] + MAP_RECT[3] - LEGEND_DY - h;
    prims.push(Prim::Rect {
        rect: [x0, y0, w, h],
        fill: Some(WHITE),
        stroke: Some(Stroke {
            width: 0.15,
            color: BLACK,
        }),
    });
    // 题「图 例」（通栏居中）
    prims.push(Prim::Text {
        x: x0 + w / 2.0,
        y: y0 + LEGEND_PAD + LEGEND_TITLE_H / 2.0,
        font: LEGEND_TITLE_FONT,
        text: title.to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    // 逐行：色块 + 标注
    for (i, row) in rows.iter().enumerate() {
        let cy = y0 + LEGEND_PAD + LEGEND_TITLE_H + i as f64 * LEGEND_ROW_H + LEGEND_ROW_H / 2.0;
        prims.push(Prim::Rect {
            rect: [
                x0 + LEGEND_PAD,
                cy - LEGEND_SWATCH_H / 2.0,
                LEGEND_SWATCH_W,
                LEGEND_SWATCH_H,
            ],
            fill: row.fill,
            stroke: Some(Stroke {
                width: row.stroke_w,
                color: row.stroke,
            }),
        });
        prims.push(Prim::Text {
            x: x0 + LEGEND_PAD + LEGEND_SWATCH_W + LEGEND_SWATCH_GAP,
            y: cy,
            font: LEGEND_FONT,
            text: row.label.clone(),
            anchor: Anchor::Start,
            rotate_deg: 0.0,
            vcenter: true,
            bold: false,
        });
    }
}

// ---------------------------------------------------------------------------
// 场景构建
// ---------------------------------------------------------------------------

/// 排版场景（图元 + 实际比例尺 + 诊断）。
struct Scene {
    prims: Vec<Prim>,
    scale: u32,
    diagnostics: Vec<String>,
}

/// 用岛几何面积（平方米；外环 − 内环，鞋带公式绝对值）。
fn boundary_area(boundary: &ParcelBoundary) -> f64 {
    let ext = cartography::ring_area(&boundary.exterior).abs();
    let holes: f64 = boundary
        .interiors
        .iter()
        .map(|r| cartography::ring_area(r).abs())
        .sum();
    (ext - holes).max(0.0)
}

/// 组场景：版面几何 + 经纬网 + 设施图斑 + 罗盘/图例/表格（SVG/PNG 共用）。
fn build_scene(boundary: &ParcelBoundary, spec: &IslandMapSpec) -> Result<Scene, RenderError> {
    // 界址点（符号与 L.9 坐标表用；用岛图件无点号/边长注记）
    let points = cartography::generate_boundary_points(boundary, "");
    if points.is_empty() {
        return Err(RenderError::InvalidStyle("用岛几何无界址点".to_string()));
    }
    // 源坐标系缺省 EPSG:4527（CGCS2000 3° 带 39 带；签注表坐标系恒为 CGCS2000）
    let source_epsg = if spec.source_epsg.trim().is_empty() {
        "EPSG:4527"
    } else {
        spec.source_epsg.trim()
    };
    // 界址点经纬度（L.9 坐标表 DMS 来源；L.10 跳过反算）
    let map_pts: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
    let lonlats = if spec.kind == IslandMapKind::RangeMap {
        seamap::to_lonlat(&map_pts, source_epsg)?
    } else {
        Vec::new()
    };
    // 用岛面积（L.9：给定值优先，缺省几何现算 外环−内环）
    let area = spec.area_sqm.unwrap_or_else(|| boundary_area(boundary));
    // 表格（先组表：表宽参与适配区与自动比例尺求解）
    let coord_table = seamap::build_coord_table(&points, &lonlats);
    let sign_table = build_sign_table(spec);
    let facility_table = build_facility_table(&spec.facilities);
    // 用岛适配区：地图框扣除留白与右侧表带（L.9 坐标表/L.10 一览表 与签注表较宽者）
    let band_w = match spec.kind {
        IslandMapKind::RangeMap => coord_table.width().max(sign_table.width()),
        IslandMapKind::FacilityMap => facility_table.width().max(sign_table.width()),
    };
    let fit_x0 = MAP_RECT[0] + MAP_PAD;
    let fit_y0 = MAP_RECT[1] + MAP_PAD;
    let fit_w = (MAP_RECT[2] - 2.0 * MAP_PAD - band_w - TABLE_FIT_GAP).max(FIT_MIN_W);
    let fit_h = (MAP_RECT[3] - 2.0 * MAP_PAD).max(FIT_MIN_H);
    let (min_x, min_y, max_x, max_y) = seamap::ring_bbox(&boundary.exterior);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    // 比例尺：spec 给定直接用；缺省取宽/高方向较大 raw 分母向上整百
    let scale = match spec.scale {
        Some(s) => s,
        None => seamap::round_up_hundred((span_x * 1000.0 / fit_w).max(span_y * 1000.0 / fit_h)),
    };
    let pm = PageMap {
        mu: 1000.0 / f64::from(scale),
        fit_c: (fit_x0 + fit_w / 2.0, fit_y0 + fit_h / 2.0),
        bbox_c: ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0),
    };

    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    let mut prims: Vec<Prim> = Vec::with_capacity(256);
    // —— 页面外框 + 图名（无项目名前缀）+ 用岛代码行 ——
    prims.push(Prim::Rect {
        rect: OUTER_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.3,
            color: BLACK,
        }),
    });
    prims.push(Prim::Text {
        x: PAGE_W / 2.0,
        y: TITLE_Y,
        font: TITLE_FONT,
        text: spec.kind.title().to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: true,
    });
    prims.push(Prim::Text {
        x: OUTER_RECT[0] + 3.0,
        y: SEA_CODE_Y,
        font: SEA_CODE_FONT,
        text: format!("用岛代码：{}（登记时填写或粘贴）", spec.island_code),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 地图框（经纬网图廓）——
    prims.push(Prim::Rect {
        rect: MAP_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.3,
            color: BLACK,
        }),
    });
    // —— 用岛图斑填充（RGB(245,162,122)，纯色）——
    prims.push(Prim::Path {
        pts: boundary
            .exterior
            .points
            .iter()
            .map(|&p| pm.to_page(p))
            .collect(),
        close: true,
        fill: Some(SEA_FILL),
        stroke: None,
    });
    // —— 经纬网线 + 四边 DMS 注记（绘于图斑之上、设施与界址线之下）——
    seamap::emit_graticule(&mut prims, &pm, source_epsg)?;
    // —— 设施黄色图斑（L.10：RGB(255,235,0) 填充 + 0.3mm 黑边；L.9 忽略）——
    if spec.kind == IslandMapKind::FacilityMap {
        for f in &spec.facilities {
            prims.push(Prim::Path {
                pts: f.polygon.iter().map(|&p| pm.to_page(p)).collect(),
                close: true,
                fill: Some(FACILITY_FILL),
                stroke: Some(Stroke {
                    width: 0.3,
                    color: BLACK,
                }),
            });
        }
    }
    // —— 界址线（0.5mm 红；外环 + 内环）——
    for ring in boundary.rings() {
        prims.push(Prim::Path {
            pts: ring.points.iter().map(|&p| pm.to_page(p)).collect(),
            close: true,
            fill: None,
            stroke: Some(Stroke {
                width: 0.5,
                color: RED,
            }),
        });
    }
    // —— 界址点（Ø2.0mm 黑圆圈白底 + Ø0.2mm 圆心点，线粗 0.15mm；无点号/边长注记）——
    for p in &points {
        let (x, y) = pm.to_page((p.x, p.y));
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
    // —— 设施编号（L.10：图斑 bbox 中心 2.4mm 黑字，最上层）——
    if spec.kind == IslandMapKind::FacilityMap {
        for f in &spec.facilities {
            let (fx0, fy0, fx1, fy1) = f.polygon.iter().fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)),
            );
            let (x, y) = pm.to_page(((fx0 + fx1) / 2.0, (fy0 + fy1) / 2.0));
            prims.push(Prim::Text {
                x,
                y,
                font: FACILITY_NO_FONT,
                text: f.no.clone(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    // —— 罗盘指北针（地图框内右上，四向尖角星形 + N/E/W/S）——
    emit_compass(&mut prims);
    // —— 比例尺（地图框内下中，分母取整百）——
    prims.push(Prim::Text {
        x: MAP_RECT[0] + MAP_RECT[2] / 2.0,
        y: MAP_RECT[1] + MAP_RECT[3] - 2.5,
        font: 3.2,
        text: format!("1:{scale}"),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 图例框（地图框内左下）——
    emit_legend(&mut prims, spec.kind);
    // —— 右侧表带（右上锚定，指北针下方）——
    let table_x1 = MAP_RECT[0] + MAP_RECT[2] - TABLE_ANCHOR_PAD;
    match spec.kind {
        IslandMapKind::RangeMap => {
            // 界址点编号及坐标表 + 表下用岛面积行
            coord_table.emit(&mut prims, table_x1, COORD_TABLE_Y0);
            prims.push(Prim::Text {
                x: table_x1 - coord_table.width() / 2.0,
                y: COORD_TABLE_Y0 + coord_table.height() + AREA_GAP,
                font: AREA_FONT,
                text: format!("用岛面积：{area:.2}平方米"),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
        IslandMapKind::FacilityMap => {
            // 建筑物和设施一览表（末行合计跨前两栏）
            facility_table.emit(&mut prims, table_x1, COORD_TABLE_Y0);
        }
    }
    // —— 网格签注表（右下锚定）——
    let sign_y0 = MAP_RECT[1] + MAP_RECT[3] - TABLE_ANCHOR_PAD - sign_table.height();
    sign_table.emit(&mut prims, table_x1, sign_y0);
    Ok(Scene {
        prims,
        scale,
        diagnostics: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kanyu_core::cartography::{RealestateRing, RingRole};

    /// 合成用岛：EPSG:4527 投影坐标矩形 40m × 100m（东 39595460 起、北 4127200 起；
    /// 近金样区位，东经约 118°、北纬约 37°）。
    fn test_boundary() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595460.0, 4127200.0),
                    (39595500.0, 4127200.0),
                    (39595500.0, 4127300.0),
                    (39595460.0, 4127300.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 完整 spec（L.9；比例尺缺省自动求解、源坐标系 EPSG:4527）。
    fn full_spec() -> IslandMapSpec {
        IslandMapSpec {
            kind: IslandMapKind::RangeMap,
            island_code: "371602113005JB00088".to_string(),
            source_epsg: "EPSG:4527".to_string(),
            survey_unit: "XXX测绘单位".to_string(),
            surveyor: "张三".to_string(),
            drawer: "李四".to_string(),
            reviewer: "赵六".to_string(),
            draw_date: "2026年08月25日".to_string(),
            area_sqm: None,
            facilities: Vec::new(),
            scale: None,
            dpi: 150.0,
        }
    }

    /// 合成设施（L.10）：用岛矩形内 3 个小矩形（10×20 / 10×30 / 5×10 m）。
    fn test_facilities() -> Vec<IslandFacility> {
        let rect = |x: f64, y: f64, w: f64, h: f64| {
            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
        };
        vec![
            IslandFacility {
                name: "设施1".to_string(),
                no: "1".to_string(),
                area_sqm: 200.0,
                polygon: rect(39595465.0, 4127210.0, 10.0, 20.0),
            },
            IslandFacility {
                name: "设施2".to_string(),
                no: "2".to_string(),
                area_sqm: 300.0,
                polygon: rect(39595480.0, 4127210.0, 10.0, 30.0),
            },
            IslandFacility {
                name: "设施3".to_string(),
                no: "3".to_string(),
                area_sqm: 86.86,
                polygon: rect(39595465.0, 4127260.0, 5.0, 10.0),
            },
        ]
    }

    /// 从产物取 SVG 文本（断言辅助）。
    fn svg_of(out: &IslandMapOutput) -> &str {
        match &out.data {
            crate::parcelmap::ParcelMapData::Svg(s) => s,
            crate::parcelmap::ParcelMapData::Png(_) => panic!("应为 SVG 产物"),
        }
    }

    #[test]
    fn range_map_svg_full_layout() {
        let out = render_island_map_svg(&test_boundary(), &full_spec()).unwrap();
        // 自动比例尺手算验证：坐标表宽约 55.9mm（签注表较窄）→ 适配区 ≈197.1×140mm；
        // raw = max(40×1000/197.1, 100×1000/140) = max(202.9, 714.3) = 714.3 → 整百 800
        assert_eq!(out.scale, 800);
        assert_eq!(out.scale % 100, 0, "比例尺分母应向上整百");
        let svg = svg_of(&out);
        // 版式要素：图名（无项目名前缀）/ 用岛代码 / 坐标表题行与表头 / 面积行 /
        // 签注表（高斯-克吕格投影等新增行）/ 图例 / 比例尺
        for frag in [
            "用岛范围图",
            "用岛代码：371602113005JB00088（登记时填写或粘贴）",
            "界址点编号及坐标（北纬 | 东经）",
            "纬度(北纬)",
            "经度(东经)",
            "用岛面积：4000.00平方米",
            "高斯-克吕格投影",
            "见坐标表",
            "2000国家大地坐标系",
            "海岛岸线/界址线",
            "用岛范围",
            "1:800",
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 罗盘指北针：N/E/W/S 四字母文本
        for frag in [">N</text>", ">E</text>", ">W</text>", ">S</text>"] {
            assert!(svg.contains(frag), "罗盘缺少字母: {frag}");
        }
        // DMS 串存在（°′″ 三符号齐全），且 1 号点（西北角 (39595460, 4127300)）
        // 独立反算值与表内串一致、末行闭合重复出现
        for sym in ["°", "′", "″"] {
            assert!(svg.contains(sym), "SVG 缺少 DMS 符号: {sym}");
        }
        let back = seamap::to_lonlat(&[(39595460.0, 4127300.0)], "EPSG:4527").unwrap();
        let (lon, lat) = back[0];
        assert!((117.9..118.2).contains(&lon), "东经应约 118°: {lon}");
        assert!((37.1..37.4).contains(&lat), "北纬应约 37°: {lat}");
        let (lat_s, lon_s) = (seamap::format_dms(lat, 3), seamap::format_dms(lon, 3));
        assert!(svg.contains(&lat_s), "坐标表应含 1 号点纬度 {lat_s}");
        assert!(svg.contains(&lon_s), "坐标表应含 1 号点经度 {lon_s}");
        assert!(
            svg.matches(&lat_s).count() >= 2,
            "1 号点纬度应重复出现闭合: {lat_s}"
        );
        // 无点号/边长注记：合成矩形已知边长 40m / 100m 的边长串不得出现
        assert!(!svg.contains("40.00"), "不应含边长注记 40.00");
        assert!(!svg.contains("100.00"), "不应含边长注记 100.00");
        // 宗海式图斑填充色沿用（RGB(245,162,122)）
        assert!(svg.contains("#F5A27A"), "用岛图斑填充 RGB(245,162,122)");
    }

    #[test]
    fn facility_map_svg_full_layout() {
        let spec = IslandMapSpec {
            kind: IslandMapKind::FacilityMap,
            facilities: test_facilities(),
            ..full_spec()
        };
        let out = render_island_map_svg(&test_boundary(), &spec).unwrap();
        // L.10 表带为一览表（约 49.5mm 宽）：raw = max(40000/203.5, 100000/140) → 800
        assert_eq!(out.scale, 800);
        let svg = svg_of(&out);
        for frag in [
            "建筑物和设施布置图",
            "用岛代码：371602113005JB00088（登记时填写或粘贴）",
            "建筑物和设施名称",
            "占地面积/㎡",
            "合计",
            "586.86", // 200.00 + 300.00 + 86.86
            "设施1",
            "建筑物和设施", // 图例行
            "界址线",       // 图例行
            "1:800",
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 设施编号文本（图斑内居中注记）
        for frag in [">1</text>", ">2</text>", ">3</text>"] {
            assert!(svg.contains(frag), "缺少设施编号: {frag}");
        }
        // 设施黄色填充 RGB(255,235,0)
        assert!(svg.contains("#FFEB00"), "设施图斑填充 RGB(255,235,0)");
        // 罗盘四字母仍在
        for frag in [">N</text>", ">E</text>", ">W</text>", ">S</text>"] {
            assert!(svg.contains(frag), "罗盘缺少字母: {frag}");
        }
        // L.10 无坐标表（反算跳过）
        assert!(!svg.contains("界址点编号及坐标"), "L.10 无界址点坐标表");
    }

    #[test]
    fn facility_map_empty_facilities_honest_zero() {
        // 空设施：图斑区无设施，一览表仅表头 + 合计 0.00（诚实空态）
        let spec = IslandMapSpec {
            kind: IslandMapKind::FacilityMap,
            facilities: Vec::new(),
            ..full_spec()
        };
        let out = render_island_map_svg(&test_boundary(), &spec).unwrap();
        let svg = svg_of(&out);
        assert!(svg.contains("建筑物和设施名称"), "空态仍有表头");
        assert!(svg.contains("合计"), "空态仍有合计行");
        assert!(svg.contains("0.00"), "空态合计 0.00");
        // 空态图斑区无设施：#FFEB00 仅图例色块 1 处（有设施时另有图斑 polygon）
        assert_eq!(svg.matches("#FFEB00").count(), 1, "空态黄色仅图例色块一处");
    }

    #[test]
    fn area_default_computed_from_geometry() {
        // area_sqm=None：面积行 = 几何鞋带面积（外环 − 内环，2 位小数）
        let mut boundary = test_boundary();
        boundary.interiors.push(RealestateRing::new(
            vec![
                (39595470.0, 4127210.0),
                (39595480.0, 4127210.0),
                (39595480.0, 4127220.0),
                (39595470.0, 4127220.0),
            ],
            RingRole::Interior,
        ));
        let out = render_island_map_svg(&boundary, &full_spec()).unwrap();
        // 40×100 − 10×10 = 3900.00
        assert!(svg_of(&out).contains("用岛面积：3900.00平方米"));
        // 给定值直通（不现算）
        let spec = IslandMapSpec {
            area_sqm: Some(3483.61),
            ..full_spec()
        };
        let out2 = render_island_map_svg(&test_boundary(), &spec).unwrap();
        assert!(svg_of(&out2).contains("用岛面积：3483.61平方米"));
    }

    #[test]
    fn given_scale_used_directly() {
        let spec = IslandMapSpec {
            scale: Some(1200),
            ..full_spec()
        };
        let out = render_island_map_svg(&test_boundary(), &spec).unwrap();
        assert_eq!(out.scale, 1200);
        assert!(svg_of(&out).contains("1:1200"));
    }

    #[test]
    fn png_encodes_and_dump() {
        // L.9 PNG：编码 + 版式像素断言 + 落盘目检
        let out = render_island_map_png(&test_boundary(), &full_spec()).unwrap();
        assert_eq!(out.scale, 800);
        let png = match &out.data {
            crate::parcelmap::ParcelMapData::Png(b) => b,
            crate::parcelmap::ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG 魔数");
        let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
        assert!(pixmap.width() > pixmap.height(), "A4 应横向");
        // 红界址线 + 图斑填充色存在（抗锯齿容差 ±8）
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 80 && p.blue() < 80));
        assert!(pixmap.pixels().iter().any(|p| {
            (p.red() as i16 - 245).abs() <= 8
                && (p.green() as i16 - 162).abs() <= 8
                && (p.blue() as i16 - 122).abs() <= 8
        }));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/island_map_test.png");
        std::fs::write(&path, png).unwrap();

        // L.10 PNG：设施黄色图斑像素 + 落盘目检
        let spec = IslandMapSpec {
            kind: IslandMapKind::FacilityMap,
            facilities: test_facilities(),
            ..full_spec()
        };
        let out10 = render_island_map_png(&test_boundary(), &spec).unwrap();
        let png10 = match &out10.data {
            crate::parcelmap::ParcelMapData::Png(b) => b,
            crate::parcelmap::ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        let pixmap10 = tiny_skia::Pixmap::decode_png(png10).unwrap();
        assert!(
            pixmap10.pixels().iter().any(|p| {
                (p.red() as i16 - 255).abs() <= 8
                    && (p.green() as i16 - 235).abs() <= 8
                    && p.blue() <= 8
            }),
            "设施黄色图斑 RGB(255,235,0)"
        );
        let path10 = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/island_facility_map_test.png");
        std::fs::write(&path10, png10).unwrap();
    }
}
