//! 宗海界址图渲染器 —— GB/T 42547-2023《地籍调查规程》图 L.7 版式（A4 横向）。
//!
//! 页面（A4 横 297×210mm）：
//! - 顶部居中标题「{项目名称}宗海界址图」（粗体）；
//!   左上「宗海代码：{宗海代码}（登记时填写或粘贴）」；
//! - **经纬网图廓**：地图框经纬网线（0.15mm 黑），度分秒注记顶/底横排、左/右竖排；
//!   网格间隔自适应（1″、2″、5″、10″、15″、30″、1′、2′、5′、10′、30′、1° 候选中
//!   取使每轴 3~8 条线的最大间隔），经纬度范围由图廓四角投影坐标反算
//!   （source_epsg → EPSG:4490）；地图与经纬网按投影坐标绘制（图廓注记才是经纬度）；
//! - 地图区：宗海图斑填充 RGB(245,162,122)、外部界址线 0.5mm 红、界址点
//!   Ø2.0mm 黑圆圈白底（圆心 0.2mm）、点号（1、2、3… **无 J 前缀**）与边长注记
//!   （2.4mm，位置由 `kanyu_core::cartography` 勘测定界图注记契约计算）；
//! - 右侧界址点编号及坐标表（题行「界址点编号及坐标（北纬 | 东经）」；
//!   点号 | 纬度(北纬) | 经度(东经)，度分秒秒 3 位小数，末行重复 1 号点坐标闭合；
//!   白底黑线 0.15mm）；
//! - 右下网格签注表（2 列 × 8 行：坐标系/高程基准/测绘单位/测量员/绘图员/
//!   绘制日期/检查人/审核人）；
//! - 左下比例尺 1:N（分母取整百）；右上 N 指北针。
//!
//! 比例尺缺省自动求解：宗海 bbox 适配地图框（扣除留白与右侧表带）后分母向上取整百。
//! SVG 全量（文字/旋转齐全）；PNG 经 `layout::TextBackend` 系统字体栈
//! （旋转注记走离屏 pixmap 旋转合成，与 SVG rotate() 同角，同 parcelmap 做法）。

use crate::layout::{PageSize, TextBackend};
use crate::RenderError;
use kanyu_core::cartography::{
    self, BoundaryPointRecord, ParcelBoundary, PlacementReport, RealestateRing,
};
use kanyu_core::crs;

/// 宗海图种（GB/T 42547-2023 附录 L）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeaMapKind {
    /// 宗海界址图（图 L.7：界址点编号及坐标表 + 点号/边长注记）。
    #[default]
    BoundaryMap,
    /// 宗海位置图（图 L.6：经纬网图廓 + 图斑，无坐标表/点号边长注记）。
    LocationMap,
    /// 宗海平面布置图（图 L.8：同 L.6 版式）。
    LayoutMap,
}

impl SeaMapKind {
    /// 标题后缀（「{项目名称}宗海界址图」等）。
    pub fn title_suffix(self) -> &'static str {
        match self {
            SeaMapKind::BoundaryMap => "宗海界址图",
            SeaMapKind::LocationMap => "宗海位置图",
            SeaMapKind::LayoutMap => "宗海平面布置图",
        }
    }
    /// 是否绘界址点编号及坐标表与点号/边长注记（仅 L.7）。
    pub fn has_coord_table(self) -> bool {
        matches!(self, SeaMapKind::BoundaryMap)
    }
}

/// 宗海界址图出图参数。
#[derive(Debug, Clone, Default)]
pub struct SeaBoundaryMapSpec {
    /// 图种（默认宗海界址图 L.7）。
    pub kind: SeaMapKind,
    /// 项目名称（标题「{project_name}宗海界址图」）。
    pub project_name: String,
    /// 宗海代码（左上「登记时填写或粘贴」）。
    pub sea_code: String,
    /// 源坐标系（EPSG:xxxx；界址点坐标表经此反算为 CGCS2000 经纬度度分秒）。
    pub source_epsg: String,
    /// 测绘单位 / 测量员 / 绘图员 / 绘制日期 / 检查人 / 审核人（网格签注表）。
    pub survey_unit: String,
    /// 测量员。
    pub surveyor: String,
    /// 绘图员。
    pub drawer: String,
    /// 绘制日期。
    pub draw_date: String,
    /// 检查人。
    pub inspector: String,
    /// 审核人。
    pub reviewer: String,
    /// 比例尺分母（None 自动取整百）。
    pub scale: Option<u32>,
    /// 分辨率（默认 150）。
    pub dpi: f64,
}

/// 出图结果（比例尺 + 诊断 + 产物）。
#[derive(Debug, Clone)]
pub struct SeaMapOutput {
    /// 实际比例尺分母。
    pub scale: u32,
    /// 排版诊断行。
    pub diagnostics: Vec<String>,
    /// SVG 或 PNG 产物。
    pub data: crate::parcelmap::ParcelMapData,
}

/// 渲染宗海界址图为 SVG（完整排版）。
pub fn render_sea_boundary_map_svg(
    boundary: &ParcelBoundary,
    spec: &SeaBoundaryMapSpec,
) -> Result<SeaMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    Ok(SeaMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: crate::parcelmap::ParcelMapData::Svg(scene_to_svg(&scene.prims)),
    })
}

/// 渲染宗海界址图为 PNG（tiny-skia 光栅链 + TextBackend 系统字体栈）。
pub fn render_sea_boundary_map_png(
    boundary: &ParcelBoundary,
    spec: &SeaBoundaryMapSpec,
) -> Result<SeaMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    let png = scene_to_png(&scene.prims, spec.dpi, &TextBackend::system())?;
    Ok(SeaMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: crate::parcelmap::ParcelMapData::Png(png),
    })
}

// ---------------------------------------------------------------------------
// 版式常量（毫米；A4 横 297×210，图 L.7 首版固定横向）
// ---------------------------------------------------------------------------

/// 页宽 / 页高。
pub(crate) const PAGE_W: f64 = 297.0;
const PAGE_H: f64 = 210.0;
/// 页面外框（0.3mm）。
pub(crate) const OUTER_RECT: [f64; 4] = [5.0, 5.0, 287.0, 200.0];
/// 标题基线 y 与字号（图名大号粗体）。
pub(crate) const TITLE_Y: f64 = 15.0;
pub(crate) const TITLE_FONT: f64 = 6.0;
/// 左上宗海代码行基线 y 与字号。
pub(crate) const SEA_CODE_Y: f64 = 20.5;
pub(crate) const SEA_CODE_FONT: f64 = 2.8;
/// 地图框（经纬网图廓；主体矩形，网线/注记挂接其四边）。
pub(crate) const MAP_RECT: [f64; 4] = [12.0, 26.0, 273.0, 156.0];
/// 宗海适配留白（地图框内沿）。
pub(crate) const MAP_PAD: f64 = 8.0;
/// 适配区最小宽/高（表带极宽时兜底）。
pub(crate) const FIT_MIN_W: f64 = 40.0;
pub(crate) const FIT_MIN_H: f64 = 40.0;
/// 经纬网注记字号 / 注记中心距图廓边距离。
const GRID_LABEL_FONT: f64 = 2.6;
const GRID_LABEL_OFF: f64 = 2.2;
/// 网格间隔候选（秒）：1″、2″、5″、10″、15″、30″、1′、2′、5′、10′、30′、1°。
const GRID_STEPS_SEC: [f64; 12] = [
    1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1800.0, 3600.0,
];
/// 坐标表字号 / 题行字号 / 行高 / 题行高 / 单元格横向留白 / 锚定内边距。
pub(crate) const TABLE_FONT: f64 = 2.2;
const TABLE_TITLE_FONT: f64 = 3.2;
pub(crate) const TABLE_ROW_H: f64 = 4.2;
const TABLE_TITLE_H: f64 = 5.0;
const TABLE_CELL_PAD: f64 = 1.5;
pub(crate) const TABLE_ANCHOR_PAD: f64 = 3.0;
/// 坐标表带上留白（题行上沿距地图框顶；为指北针留位）。
pub(crate) const COORD_TABLE_Y0: f64 = MAP_RECT[1] + 20.0;
/// 坐标表与宗海适配区间的横向间隔。
pub(crate) const TABLE_FIT_GAP: f64 = 4.0;
/// 指北针（地图框内右上）中心 x 偏移 / 顶 y 偏移。
const NORTH_DX: f64 = 8.0;
const NORTH_DY: f64 = 4.0;
/// 颜色（黑 / 界址线红 / 白 / 宗海图斑填充）。
pub(crate) const BLACK: [u8; 3] = [0, 0, 0];
pub(crate) const RED: [u8; 3] = [255, 0, 0];
pub(crate) const WHITE: [u8; 3] = [255, 255, 255];
pub(crate) const SEA_FILL: [u8; 3] = [245, 162, 122];

// ---------------------------------------------------------------------------
// 场景图元（毫米纸面坐标；SVG/PNG 双后端共用一份几何，与 parcelmap 同构）
// ---------------------------------------------------------------------------

/// 描边（毫米线宽 + RGB）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Stroke {
    pub(crate) width: f64,
    pub(crate) color: [u8; 3],
}

/// 文本锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Start,
    Middle,
}

/// 场景图元。
#[derive(Debug, Clone)]
pub(crate) enum Prim {
    /// 矩形（图廓 / 表格外框）。
    Rect {
        rect: [f64; 4],
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 折线 / 多边形（宗海图斑、界址线、表格线、经纬网线、指北针三角）。
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
// 度分秒与经纬网
// ---------------------------------------------------------------------------

/// 十进制度 → 度分秒串（`37°16′21.140″`；秒按 `sec_dp` 位小数，分/秒零补两位，
/// 输入取绝对值——经纬度输出恒正；秒舍入满 60 进位到分、分满 60 进位到度）。
pub(crate) fn format_dms(deg: f64, sec_dp: usize) -> String {
    let deg = deg.abs();
    let mut d = deg.floor() as u64;
    let rem_min = (deg - d as f64) * 60.0;
    let mut m = rem_min.floor() as u64;
    let factor = 10f64.powi(sec_dp as i32);
    let mut s = ((rem_min - m as f64) * 60.0 * factor).round() / factor;
    // 进位防御：59.9995+ 秒舍入满 60 进位到分；分满 60 进位到度
    if s >= 60.0 {
        s = 0.0;
        m += 1;
    }
    if m >= 60 {
        m = 0;
        d += 1;
    }
    let sec_str = if sec_dp == 0 {
        format!("{s:02.0}")
    } else {
        // 秒字段宽 = 整数 2 位 + 小数点 + 小数位（零补）
        format!("{s:0width$.sec_dp$}", width = sec_dp + 3)
    };
    format!("{d}°{m:02}′{sec_str}″")
}

/// 网格间隔选择：候选中取使线数 ∈ [3, 8] 的最大间隔（从大到小首个
/// `跨度/间隔 ≥ 3` 者；跨度小于 3″ 时兜底最小候选 1″）。
fn choose_grid_step(span_deg: f64) -> f64 {
    let span_sec = (span_deg * 3600.0).abs();
    for &step in GRID_STEPS_SEC.iter().rev() {
        if span_sec / step >= 3.0 {
            return step;
        }
    }
    GRID_STEPS_SEC[0]
}

/// 网格线值序列（度）：`step_sec` 的整倍数，覆盖 [min_deg, max_deg]。
fn grid_values(min_deg: f64, max_deg: f64, step_sec: f64) -> Vec<f64> {
    let min_s = min_deg * 3600.0;
    let max_s = max_deg * 3600.0;
    let first = (min_s / step_sec).ceil() as i64;
    let mut out = Vec::new();
    let mut k = first;
    while k as f64 * step_sec <= max_s + 1e-6 {
        out.push(k as f64 * step_sec / 3600.0);
        k += 1;
    }
    out
}

/// 经度半球字母（东经 E / 西经 W）。
fn hemi_lon(lon: f64) -> &'static str {
    if lon < 0.0 {
        "W"
    } else {
        "E"
    }
}

/// 纬度半球字母（北纬 N / 南纬 S）。
fn hemi_lat(lat: f64) -> &'static str {
    if lat < 0.0 {
        "S"
    } else {
        "N"
    }
}

// ---------------------------------------------------------------------------
// 投影反算（界址点坐标表 / 图廓四角经纬度共用）
// ---------------------------------------------------------------------------

/// 平面点列 → 单要素 MultiPoint FeatureCollection（投影反算载体）。
fn point_fc(pts: &[(f64, f64)]) -> geojson::FeatureCollection {
    geojson::FeatureCollection {
        features: vec![geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::MultiPoint(
                pts.iter().map(|&(x, y)| vec![x, y]).collect(),
            ))),
            id: None,
            properties: None,
            foreign_members: None,
        }],
        bbox: None,
        foreign_members: None,
    }
}

/// 反算结果取回（MultiPoint 坐标序 [(lon, lat)]，度）。
fn fc_points(fc: &geojson::FeatureCollection) -> Result<Vec<(f64, f64)>, RenderError> {
    match fc
        .features
        .first()
        .and_then(|f| f.geometry.as_ref())
        .map(|g| &g.value)
    {
        Some(geojson::Value::MultiPoint(pts)) => Ok(pts.iter().map(|p| (p[0], p[1])).collect()),
        _ => Err(RenderError::InvalidExtent(
            "投影反算结果几何缺失（应为 MultiPoint）".to_string(),
        )),
    }
}

/// 平面点列（源坐标系）→ 经纬度（CGCS2000，度）：
/// MultiPoint FC → `crs::reproject(source_epsg → EPSG:4490)` → [lon, lat]。
pub(crate) fn to_lonlat(
    pts: &[(f64, f64)],
    source_epsg: &str,
) -> Result<Vec<(f64, f64)>, RenderError> {
    let out = crs::reproject(&point_fc(pts), source_epsg, "EPSG:4490").map_err(|e| {
        RenderError::InvalidExtent(format!("投影反算失败（{source_epsg} → EPSG:4490）: {e}"))
    })?;
    fc_points(&out)
}

// ---------------------------------------------------------------------------
// 网格表（坐标表 / 签注表共用：白底黑线 0.15mm）
// ---------------------------------------------------------------------------

/// 通用网格表模型（题行可缺省；首行通常为表头）。
pub(crate) struct GridTable {
    /// 题行（跨全宽；None 无题行）。
    pub(crate) title: Option<String>,
    /// 全部行（含表头行）。
    pub(crate) rows: Vec<Vec<String>>,
    /// 栏宽（毫米，含两侧留白）。
    pub(crate) col_w: Vec<f64>,
}

impl GridTable {
    /// 表总宽（毫米）。
    pub(crate) fn width(&self) -> f64 {
        self.col_w.iter().sum()
    }

    /// 表总高（毫米）。
    pub(crate) fn height(&self) -> f64 {
        let title_h = if self.title.is_some() {
            TABLE_TITLE_H
        } else {
            0.0
        };
        title_h + TABLE_ROW_H * self.rows.len() as f64
    }

    /// 出图元：白底黑线 0.15mm，右上锚定（x1 = 右缘，y0 = 上沿）；
    /// 题行通栏居中，栏分界竖线自题行下缘起。
    pub(crate) fn emit(&self, prims: &mut Vec<Prim>, x1: f64, y0: f64) {
        let thin = Some(Stroke {
            width: 0.15,
            color: BLACK,
        });
        let (w, h) = (self.width(), self.height());
        let x0 = x1 - w;
        let title_h = if self.title.is_some() {
            TABLE_TITLE_H
        } else {
            0.0
        };
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
        // 横线：题行下缘 + 逐行分界（外框四边已由 Rect 承担）
        if self.title.is_some() {
            prims.push(Prim::Path {
                pts: vec![(x0, y0 + title_h), (x1, y0 + title_h)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        for r in 1..self.rows.len() {
            let y = y0 + title_h + r as f64 * TABLE_ROW_H;
            prims.push(Prim::Path {
                pts: vec![(x0, y), (x1, y)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 竖线（题行下缘起）
        for &x in &xs[1..xs.len() - 1] {
            prims.push(Prim::Path {
                pts: vec![(x, y0 + title_h), (x, y0 + h)],
                close: false,
                fill: None,
                stroke: thin,
            });
        }
        // 题行与单元格文本（通栏居中）
        if let Some(title) = &self.title {
            prims.push(Prim::Text {
                x: (x0 + x1) / 2.0,
                y: y0 + TABLE_TITLE_H / 2.0,
                font: TABLE_TITLE_FONT,
                text: title.clone(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
        for (r, row) in self.rows.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                prims.push(Prim::Text {
                    x: (xs[c] + xs[c + 1]) / 2.0,
                    y: y0 + title_h + r as f64 * TABLE_ROW_H + TABLE_ROW_H / 2.0,
                    font: TABLE_FONT,
                    text: text.clone(),
                    anchor: Anchor::Middle,
                    rotate_deg: 0.0,
                    vcenter: true,
                    bold: false,
                });
            }
        }
    }
}

/// 栏宽：全部行内容估算宽最大值 + 两侧留白，并应用最小栏宽。
pub(crate) fn col_widths(rows: &[Vec<String>], mins: &[f64]) -> Vec<f64> {
    let n = rows.first().map(|r| r.len()).unwrap_or(0);
    (0..n)
        .map(|c| {
            let w = rows
                .iter()
                .map(|r| {
                    cartography::text_extent_mm(
                        r.get(c).map(String::as_str).unwrap_or(""),
                        TABLE_FONT,
                    )
                    .0
                })
                .fold(0.0_f64, f64::max);
            (w + TABLE_CELL_PAD * 2.0).max(mins.get(c).copied().unwrap_or(0.0))
        })
        .collect()
}

/// 界址点编号及坐标表：点号 | 纬度(北纬) | 经度(东经)；按点号升序，
/// 每行 DMS（秒 3 位小数），末行重复 1 号点坐标闭合。
pub(crate) fn build_coord_table(
    points: &[BoundaryPointRecord],
    lonlats: &[(f64, f64)],
) -> GridTable {
    let mut rows: Vec<Vec<String>> = vec![vec![
        "点号".to_string(),
        "纬度(北纬)".to_string(),
        "经度(东经)".to_string(),
    ]];
    for (p, &(lon, lat)) in points.iter().zip(lonlats.iter()) {
        rows.push(vec![p.label(), format_dms(lat, 3), format_dms(lon, 3)]);
    }
    // 闭合行：重复 1 号点坐标
    if let (Some(p), Some(&(lon, lat))) = (points.first(), lonlats.first()) {
        rows.push(vec![p.label(), format_dms(lat, 3), format_dms(lon, 3)]);
    }
    let col_w = col_widths(&rows, &[9.0, 0.0, 0.0]);
    GridTable {
        title: Some("界址点编号及坐标（北纬 | 东经）".to_string()),
        rows,
        col_w,
    }
}

/// 网格签注表（2 列 × 8 行，无题行/表头）。
fn build_sign_table(spec: &SeaBoundaryMapSpec) -> GridTable {
    let rows: Vec<Vec<String>> = vec![
        vec!["坐标系".to_string(), "2000国家大地坐标系".to_string()],
        vec!["高程基准".to_string(), "1985国家高程基准".to_string()],
        vec!["测绘单位".to_string(), spec.survey_unit.clone()],
        vec!["测量员".to_string(), spec.surveyor.clone()],
        vec!["绘图员".to_string(), spec.drawer.clone()],
        vec!["绘制日期".to_string(), spec.draw_date.clone()],
        vec!["检查人".to_string(), spec.inspector.clone()],
        vec!["审核人".to_string(), spec.reviewer.clone()],
    ];
    let col_w = col_widths(&rows, &[12.0, 24.0]);
    GridTable {
        title: None,
        rows,
        col_w,
    }
}

// ---------------------------------------------------------------------------
// 场景构建
// ---------------------------------------------------------------------------

/// 比例尺分母向上取整百（786→800；下限 100）。
pub(crate) fn round_up_hundred(raw: f64) -> u32 {
    if !raw.is_finite() || raw <= 100.0 {
        return 100;
    }
    (raw / 100.0).ceil() as u32 * 100
}

/// 环 bbox（min_x, min_y, max_x, max_y）。
pub(crate) fn ring_bbox(ring: &RealestateRing) -> (f64, f64, f64, f64) {
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

/// 页面 ↔ 投影坐标映射（mu = 1000/scale；宗海 bbox 中心对适配区中心，北朝上）。
pub(crate) struct PageMap {
    pub(crate) mu: f64,
    pub(crate) fit_c: (f64, f64),
    pub(crate) bbox_c: (f64, f64),
}

impl PageMap {
    /// 投影坐标 → 纸面毫米。
    pub(crate) fn to_page(&self, p: (f64, f64)) -> (f64, f64) {
        (
            self.fit_c.0 + (p.0 - self.bbox_c.0) * self.mu,
            self.fit_c.1 - (p.1 - self.bbox_c.1) * self.mu,
        )
    }
    /// 纸面毫米 → 投影坐标（图廓四角反算用）。
    pub(crate) fn to_map(&self, p: (f64, f64)) -> (f64, f64) {
        (
            self.bbox_c.0 + (p.0 - self.fit_c.0) / self.mu,
            self.bbox_c.1 - (p.1 - self.fit_c.1) / self.mu,
        )
    }
}

/// 组场景：版面几何 + 经纬网 + 注记排版 + 诊断（SVG/PNG 共用）。
fn build_scene(boundary: &ParcelBoundary, spec: &SeaBoundaryMapSpec) -> Result<Scene, RenderError> {
    // 宗海界址图点号不带 J 前缀
    let points = cartography::generate_boundary_points(boundary, "");
    if points.is_empty() {
        return Err(RenderError::InvalidStyle("宗海几何无界址点".to_string()));
    }
    let lines = cartography::generate_boundary_lines(boundary, &points);
    // 源坐标系缺省 EPSG:4527（CGCS2000 3° 带 39 带；签注表坐标系恒为 CGCS2000）
    let source_epsg = if spec.source_epsg.trim().is_empty() {
        "EPSG:4527"
    } else {
        spec.source_epsg.trim()
    };
    // 界址点经纬度（坐标表 DMS 来源；仅 L.7 需要，L.6/L.8 跳过反算）
    let map_pts: Vec<(f64, f64)> = points.iter().map(|p| (p.x, p.y)).collect();
    let lonlats = if spec.kind.has_coord_table() {
        to_lonlat(&map_pts, source_epsg)?
    } else {
        Vec::new()
    };
    // 表格（先组表：表宽参与适配区与自动比例尺求解）
    let coord_table = build_coord_table(&points, &lonlats);
    let sign_table = build_sign_table(spec);
    // 宗海适配区：地图框扣除留白与右侧表带（坐标表/签注表较宽者；L.6/L.8 仅签注表）
    let band_w = if spec.kind.has_coord_table() {
        coord_table.width().max(sign_table.width())
    } else {
        sign_table.width()
    };
    let fit_x0 = MAP_RECT[0] + MAP_PAD;
    let fit_y0 = MAP_RECT[1] + MAP_PAD;
    let fit_w = (MAP_RECT[2] - 2.0 * MAP_PAD - band_w - TABLE_FIT_GAP).max(FIT_MIN_W);
    let fit_h = (MAP_RECT[3] - 2.0 * MAP_PAD).max(FIT_MIN_H);
    let (min_x, min_y, max_x, max_y) = ring_bbox(&boundary.exterior);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    // 比例尺：spec 给定直接用；缺省取宽/高方向较大 raw 分母向上整百
    let scale = match spec.scale {
        Some(s) => s,
        None => round_up_hundred((span_x * 1000.0 / fit_w).max(span_y * 1000.0 / fit_h)),
    };
    let pm = PageMap {
        mu: 1000.0 / f64::from(scale),
        fit_c: (fit_x0 + fit_w / 2.0, fit_y0 + fit_h / 2.0),
        bbox_c: ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0),
    };

    // 注记排版（点号先排，其矩形作为边长注记的附加障碍；仅 L.7 绘注记）
    let (point_report, edge_report) = if spec.kind.has_coord_table() {
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
        (point_report, edge_report)
    } else {
        (
            cartography::PlacementReport::default(),
            cartography::PlacementReport::default(),
        )
    };
    let diagnostics = diagnostics_of(&[&point_report, &edge_report]);

    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    let mut prims: Vec<Prim> = Vec::with_capacity(256);
    // —— 页面外框 + 标题 + 宗海代码行 ——
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
        text: format!("{}{}", spec.project_name, spec.kind.title_suffix()),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: true,
    });
    prims.push(Prim::Text {
        x: OUTER_RECT[0] + 3.0,
        y: SEA_CODE_Y,
        font: SEA_CODE_FONT,
        text: format!("宗海代码：{}（登记时填写或粘贴）", spec.sea_code),
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
    // —— 宗海图斑填充（RGB(245,162,122)，纯色）——
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
    // —— 经纬网线 + 四边 DMS 注记（绘于图斑之上、界址线之下）——
    emit_graticule(&mut prims, &pm, source_epsg)?;
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
    // —— 界址点（Ø2.0mm 黑圆圈白底 + Ø0.2mm 圆心点，线粗 0.15mm）——
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
    // —— 点号（无 J 前缀）/ 边长注记（2.4mm，位置与旋转按 cartography 排版结果；
    // 仅 L.7 宗海界址图绘注记，L.6/L.8 仅绘界址点符号）——
    for l in point_report.labels.iter().chain(edge_report.labels.iter()) {
        let (x, y) = pm.to_page((l.rect.cx, l.rect.cy));
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
    // —— 指北针（地图框内右上：N + 细长三角，同 parcelmap 样式）——
    let north_x = MAP_RECT[0] + MAP_RECT[2] - NORTH_DX;
    let north_top = MAP_RECT[1] + NORTH_DY;
    prims.push(Prim::Text {
        x: north_x,
        y: north_top + 1.6,
        font: 3.4,
        text: "N".to_string(),
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
    // —— 比例尺（分母取整百；L.7 地图框内左下，L.6/L.8 地图框内下中，对齐金样）——
    let (scale_x, scale_anchor) = if spec.kind.has_coord_table() {
        (MAP_RECT[0] + 4.0, Anchor::Start)
    } else {
        (MAP_RECT[0] + MAP_RECT[2] / 2.0, Anchor::Middle)
    };
    prims.push(Prim::Text {
        x: scale_x,
        y: MAP_RECT[1] + MAP_RECT[3] - 2.5,
        font: 3.2,
        text: format!("1:{scale}"),
        anchor: scale_anchor,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 界址点编号及坐标表（右侧，指北针下方；仅 L.7）——
    let table_x1 = MAP_RECT[0] + MAP_RECT[2] - TABLE_ANCHOR_PAD;
    if spec.kind.has_coord_table() {
        coord_table.emit(&mut prims, table_x1, COORD_TABLE_Y0);
    }
    // —— 网格签注表（右下锚定）——
    let sign_y0 = MAP_RECT[1] + MAP_RECT[3] - TABLE_ANCHOR_PAD - sign_table.height();
    sign_table.emit(&mut prims, table_x1, sign_y0);
    Ok(Scene {
        prims,
        scale,
        diagnostics,
    })
}

/// 经纬网发射：图廓四角投影坐标反算为经纬度（source_epsg → EPSG:4490），
/// 经线顶/底边插值、纬线左/右边插值绘网线（0.15mm 黑）；
/// 注记对齐网线：顶/底横排（如 118°04′30″E）、左/右竖排（如 37°16′19″N，rotate -90）。
pub(crate) fn emit_graticule(
    prims: &mut Vec<Prim>,
    pm: &PageMap,
    source_epsg: &str,
) -> Result<(), RenderError> {
    let (x0, y0, w, h) = (MAP_RECT[0], MAP_RECT[1], MAP_RECT[2], MAP_RECT[3]);
    // 图廓四角（页面序：左上/右上/左下/右下）→ 投影坐标 → 经纬度
    let corners_page = [(x0, y0), (x0 + w, y0), (x0, y0 + h), (x0 + w, y0 + h)];
    let corners_map: Vec<(f64, f64)> = corners_page.iter().map(|&p| pm.to_map(p)).collect();
    let corners = to_lonlat(&corners_map, source_epsg)?;
    let [tl, tr, bl, br] = [corners[0], corners[1], corners[2], corners[3]];
    let lon_min = tl.0.min(tr.0).min(bl.0).min(br.0);
    let lon_max = tl.0.max(tr.0).max(bl.0).max(br.0);
    let lat_min = tl.1.min(tr.1).min(bl.1).min(br.1);
    let lat_max = tl.1.max(tr.1).max(bl.1).max(br.1);
    let lon_step = choose_grid_step(lon_max - lon_min);
    let lat_step = choose_grid_step(lat_max - lat_min);
    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    // 经线（纵线）：顶/底边各按经度线性插值，连线成网；注记顶/底横排
    for v in grid_values(lon_min, lon_max, lon_step) {
        let d_top = tr.0 - tl.0;
        let d_bot = br.0 - bl.0;
        if d_top.abs() < 1e-12 || d_bot.abs() < 1e-12 {
            continue;
        }
        let t_top = (v - tl.0) / d_top;
        let t_bot = (v - bl.0) / d_bot;
        if !(0.0..=1.0).contains(&t_top) || !(0.0..=1.0).contains(&t_bot) {
            continue;
        }
        let px_top = x0 + t_top * w;
        let px_bot = x0 + t_bot * w;
        prims.push(Prim::Path {
            pts: vec![(px_top, y0), (px_bot, y0 + h)],
            close: false,
            fill: None,
            stroke: thin,
        });
        let label = format!("{}{}", format_dms(v, 0), hemi_lon(v));
        for (px, yc) in [
            (px_top, y0 - GRID_LABEL_OFF),
            (px_bot, y0 + h + GRID_LABEL_OFF),
        ] {
            prims.push(Prim::Text {
                x: px,
                y: yc,
                font: GRID_LABEL_FONT,
                text: label.clone(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    // 纬线（横线）：左/右边各按纬度线性插值，连线成网；注记左/右竖排（rotate -90）
    for v in grid_values(lat_min, lat_max, lat_step) {
        let d_left = bl.1 - tl.1;
        let d_right = br.1 - tr.1;
        if d_left.abs() < 1e-12 || d_right.abs() < 1e-12 {
            continue;
        }
        let t_left = (v - tl.1) / d_left;
        let t_right = (v - tr.1) / d_right;
        if !(0.0..=1.0).contains(&t_left) || !(0.0..=1.0).contains(&t_right) {
            continue;
        }
        let py_left = y0 + t_left * h;
        let py_right = y0 + t_right * h;
        prims.push(Prim::Path {
            pts: vec![(x0, py_left), (x0 + w, py_right)],
            close: false,
            fill: None,
            stroke: thin,
        });
        let label = format!("{}{}", format_dms(v, 0), hemi_lat(v));
        for (xc, py) in [
            (x0 - GRID_LABEL_OFF, py_left),
            (x0 + w + GRID_LABEL_OFF, py_right),
        ] {
            prims.push(Prim::Text {
                x: xc,
                y: py,
                font: GRID_LABEL_FONT,
                text: label.clone(),
                anchor: Anchor::Middle,
                rotate_deg: -90.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    Ok(())
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

/// 场景 → SVG 文档（viewBox 即毫米坐标系，A4 横）。
pub(crate) fn scene_to_svg(prims: &[Prim]) -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {PAGE_W:.0} {PAGE_H:.0}\" width=\"{PAGE_W:.0}mm\" height=\"{PAGE_H:.0}mm\">\n"
    ));
    out.push_str("<!-- kanyu-render seamap · GB/T 42547-2023 图 L.7 -->\n");
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

/// 场景 → PNG 字节（A4 横；旋转注记离屏旋转合成，与 SVG rotate() 同角）。
pub(crate) fn scene_to_png(
    prims: &[Prim],
    dpi: f64,
    tb: &TextBackend,
) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{
        Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Shader, Stroke as SkStroke, Transform,
    };
    let (pw, ph) = PageSize::A4Landscape.pixels(dpi);
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
                rotate_deg,
                ..
            } => {
                let px = (font * k) as f32;
                let w = tb.measure(text, px);
                let x_px = (*x * k) as f32;
                // 旋转注记（边长沿线 / 图廓左右竖排）：离屏小 pixmap 写字后旋转合成，
                // 与 SVG rotate() 同角（顺时针为正）
                if rotate_deg.abs() > 1e-6 {
                    draw_rotated_text(&mut page, tb, text, x_px, (*y * k) as f32, px, *rotate_deg);
                    continue;
                }
                let sx = match anchor {
                    Anchor::Start => x_px,
                    Anchor::Middle => x_px - w / 2.0,
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
        .map_err(|e| RenderError::InvalidStyle(format!("宗海界址图 PNG 编码失败: {e}")))
}

/// 旋转文本绘制（PNG）：文本先绘入透明离屏 pixmap，再绕中心旋转移植。
/// 中心语义与排版引擎输出一致（anchor=Middle + vcenter）；`deg` 顺时针为正
/// （y 向下屏幕系，与 SVG rotate() 同号）。
fn draw_rotated_text(
    page: &mut tiny_skia::Pixmap,
    tb: &TextBackend,
    text: &str,
    cx: f32,
    cy: f32,
    px: f32,
    deg: f64,
) {
    use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
    let pad = 2_u32;
    let ow = (tb.measure(text, px).ceil() as u32 + pad * 2).max(4);
    let oh = ((px * 1.4).ceil() as u32 + pad * 2).max(4);
    let Some(mut off) = Pixmap::new(ow, oh) else {
        return;
    };
    // 基线：cap 中心对 pixmap 中心（cap 高 ≈0.7em，中心 ≈ 基线 − 0.35em；
    // pixmap 中心 = pad + 0.7em → 基线 = pad + 1.05em）
    let baseline = pad as f32 + px * 1.05;
    tb.draw(
        &mut off,
        text,
        pad as f32,
        baseline,
        px,
        Color::from_rgba8(0, 0, 0, 255),
    );
    let (owf, ohf) = (ow as f32, oh as f32);
    let rad = (deg as f32).to_radians();
    let (sn, cs) = rad.sin_cos();
    // off 中心 → (cx, cy)：t = c − R(θ)·(ow/2, oh/2)
    let tx = cx - (owf / 2.0) * cs + (ohf / 2.0) * sn;
    let ty = cy - (owf / 2.0) * sn - (ohf / 2.0) * cs;
    let ts = Transform::from_row(cs, sn, -sn, cs, tx, ty);
    page.draw_pixmap(0, 0, off.as_ref(), &PixmapPaint::default(), ts, None);
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kanyu_core::cartography::RingRole;

    /// 测试宗海：EPSG:4527 投影坐标 9 点多边形（东 39595460 起、北 4127200 起，
    /// 约 40m×100m 形状，CCW 存储；近金样淄博近海区位，东经约 118°、北纬约 37°）。
    fn test_boundary() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595460.0, 4127200.0),
                    (39595480.0, 4127200.0),
                    (39595500.0, 4127200.0),
                    (39595500.0, 4127250.0),
                    (39595500.0, 4127300.0),
                    (39595480.0, 4127300.0),
                    (39595460.0, 4127300.0),
                    (39595460.0, 4127250.0),
                    (39595462.0, 4127210.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 完整 spec（比例尺缺省自动求解、源坐标系 EPSG:4527）。
    fn full_spec() -> SeaBoundaryMapSpec {
        SeaBoundaryMapSpec {
            kind: SeaMapKind::BoundaryMap,
            project_name: "代理围填海项目".to_string(),
            sea_code: "371602113005JB00088".to_string(),
            source_epsg: "EPSG:4527".to_string(),
            survey_unit: "XXX测绘单位".to_string(),
            surveyor: "张三".to_string(),
            drawer: "李四".to_string(),
            draw_date: "2026年08月25日".to_string(),
            inspector: "王五".to_string(),
            reviewer: "赵六".to_string(),
            scale: None,
            dpi: 150.0,
        }
    }

    /// 从产物取 SVG 文本（断言辅助）。
    fn svg_of(out: &SeaMapOutput) -> &str {
        match &out.data {
            crate::parcelmap::ParcelMapData::Svg(s) => s,
            crate::parcelmap::ParcelMapData::Png(_) => panic!("应为 SVG 产物"),
        }
    }

    #[test]
    fn dms_format_known_values() {
        // 已知十进制度 ↔ 度分秒串（秒 3 位小数；分/秒零补两位）
        assert_eq!(
            format_dms(37.0 + 16.0 / 60.0 + 21.140 / 3600.0, 3),
            "37°16′21.140″"
        );
        assert_eq!(
            format_dms(118.0 + 4.0 / 60.0 + 34.712 / 3600.0, 3),
            "118°04′34.712″"
        );
        // 图廓注记（整数秒）
        assert_eq!(
            format_dms(118.0 + 4.0 / 60.0 + 30.0 / 3600.0, 0),
            "118°04′30″"
        );
        assert_eq!(
            format_dms(37.0 + 16.0 / 60.0 + 19.0 / 3600.0, 0),
            "37°16′19″"
        );
    }

    #[test]
    fn dms_carry_and_defense() {
        // 59.9996 秒舍入满 60 进位到分
        assert_eq!(
            format_dms(37.0 + 16.0 / 60.0 + 59.9996 / 3600.0, 3),
            "37°17′00.000″"
        );
        // 分满 60 进位到度（59 分 59.9999 秒）
        assert_eq!(
            format_dms(37.0 + 59.0 / 60.0 + 59.9999 / 3600.0, 3),
            "38°00′00.000″"
        );
        // 0° 防御
        assert_eq!(format_dms(0.0, 3), "0°00′00.000″");
        // 负值防御：经纬度输出恒正
        assert_eq!(
            format_dms(-(118.0 + 4.0 / 60.0 + 34.712 / 3600.0), 3),
            "118°04′34.712″"
        );
    }

    #[test]
    fn grid_step_selection() {
        // 典型跨度 → 期望间隔（秒）；线数须落在 3~8
        let cases = [
            (40.0, 10.0),
            (100.0, 30.0),
            (300.0, 60.0),
            (1000.0, 300.0),
            (7200.0, 1800.0),
        ];
        for (span_sec, want_step) in cases {
            let step = choose_grid_step(span_sec / 3600.0);
            assert_eq!(step, want_step, "跨度 {span_sec}″ 的间隔");
            // 线数范围验证：以 0 为起点的跨度上网线数 ∈ [3, 8]
            let n = grid_values(0.0, span_sec / 3600.0, step).len();
            assert!(
                (3..=8).contains(&n),
                "跨度 {span_sec}″ 间隔 {step}″ 线数 {n} 应在 3~8"
            );
        }
        // 极小跨度兜底 1″
        assert_eq!(choose_grid_step(2.0 / 3600.0), 1.0);
    }

    #[test]
    fn svg_full_layout_synthetic_sea() {
        let out = render_sea_boundary_map_svg(&test_boundary(), &full_spec()).unwrap();
        // 自动比例尺手算验证：坐标表宽约 55.9mm（签注表较窄）→ 适配区 ≈197.1×140mm；
        // raw = max(40×1000/197.1, 100×1000/140) = max(202.9, 714.3) = 714.3 → 整百 800
        assert_eq!(out.scale, 800);
        let svg = svg_of(&out);
        // 版式要素：标题 / 宗海代码 / 坐标表题行与表头 / 签注表 / 比例尺 / 指北针 / 点号
        for frag in [
            "代理围填海项目宗海界址图",
            "宗海代码：371602113005JB00088（登记时填写或粘贴）",
            "界址点编号及坐标（北纬 | 东经）",
            "纬度(北纬)",
            "经度(东经)",
            "2000国家大地坐标系",
            "1985国家高程基准",
            "测绘单位",
            "审核人",
            "1:800",
            ">N</text>",
            ">1</text>", // 点号 1（无 J 前缀）
            ">9</text>", // 点号 9
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 点号无 J 前缀（J 仅允许出现在宗海代码 JB00088 中）
        assert!(!svg.contains(">J1</text>"), "点号不应带 J 前缀");
        // DMS 串存在（°′″ 三符号齐全）
        for sym in ["°", "′", "″"] {
            assert!(svg.contains(sym), "SVG 缺少 DMS 符号: {sym}");
        }
        // 经纬网竖排注记（rotate -90）与横排注记（含半球字母）
        assert!(svg.contains("rotate(-90.00"), "左右图廓注记应竖排");
        assert!(
            svg.contains("\"E</text>") || svg.contains("E</text>"),
            "经度注记应含 E"
        );
        assert!(svg.contains("N</text>"), "纬度注记应含 N");
        // 宗海图斑填充色
        assert!(svg.contains("#F5A27A"), "宗海图斑填充 RGB(245,162,122)");
        // 排版诊断非空（点号 + 边长逐条摘要）
        assert!(!out.diagnostics.is_empty());
        assert!(out
            .diagnostics
            .iter()
            .any(|d| d.contains("reason=") && d.contains("clearance=")));
        // DMS 值合理性与一致性：1 号界址点（环西北角 (39595460, 4127300)）
        // 独立经 crs::reproject 反算 —— 东经约 118°、北纬约 37°，且表内串一致
        let fc = point_fc(&[(39595460.0, 4127300.0)]);
        let back = crs::reproject(&fc, "EPSG:4527", "EPSG:4490").unwrap();
        let (lon, lat) = fc_points(&back).unwrap()[0];
        assert!((117.9..118.2).contains(&lon), "东经应约 118°: {lon}");
        assert!((37.1..37.4).contains(&lat), "北纬应约 37°: {lat}");
        let (lat_s, lon_s) = (format_dms(lat, 3), format_dms(lon, 3));
        assert!(svg.contains(&lat_s), "坐标表应含 1 号点纬度 {lat_s}");
        assert!(svg.contains(&lon_s), "坐标表应含 1 号点经度 {lon_s}");
        // 末行闭合：1 号点 DMS 串在坐标表中出现两次（首行 + 闭合行）
        assert!(
            svg.matches(&lat_s).count() >= 2,
            "1 号点纬度应重复出现闭合: {lat_s}"
        );
    }

    #[test]
    fn given_scale_used_directly() {
        let spec = SeaBoundaryMapSpec {
            scale: Some(1200),
            ..full_spec()
        };
        let out = render_sea_boundary_map_svg(&test_boundary(), &spec).unwrap();
        assert_eq!(out.scale, 1200);
        assert!(svg_of(&out).contains("1:1200"));
    }

    #[test]
    fn png_encodes_and_dump() {
        let out = render_sea_boundary_map_png(&test_boundary(), &full_spec()).unwrap();
        assert_eq!(out.scale, 800);
        let png = match &out.data {
            crate::parcelmap::ParcelMapData::Png(b) => b,
            crate::parcelmap::ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        // PNG 魔数
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
        // A4 横向（宽 > 高）
        assert!(pixmap.width() > pixmap.height(), "A4 应横向");
        // 红色界址线存在
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 80 && p.blue() < 80));
        // 宗海图斑填充色存在（RGB(245,162,122)，抗锯齿容差 ±8）
        assert!(pixmap.pixels().iter().any(|p| {
            (p.red() as i16 - 245).abs() <= 8
                && (p.green() as i16 - 162).abs() <= 8
                && (p.blue() as i16 - 122).abs() <= 8
        }));
        // 落盘目检（仿 parcel_map_test）
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sea_map_test.png");
        std::fs::write(&path, png).unwrap();
    }

    #[test]
    fn location_and_layout_kinds_skip_table_and_labels() {
        // L.6 宗海位置图：无坐标表/点号边长注记，比例尺下中
        let spec = SeaBoundaryMapSpec {
            kind: SeaMapKind::LocationMap,
            ..full_spec()
        };
        let out = render_sea_boundary_map_svg(&test_boundary(), &spec).unwrap();
        let svg = svg_of(&out);
        assert!(
            svg.contains("代理围填海项目宗海位置图"),
            "标题后缀应为位置图"
        );
        assert!(!svg.contains("界址点编号及坐标"), "L.6 无坐标表");
        assert!(out.diagnostics.is_empty(), "L.6 无注记排版诊断");
        // 比例尺下中（地图框水平中心 x≈148.5 附近；L.7 为左下 x≈16）
        assert!(svg.contains("1:"), "比例尺存在");
        // L.8 宗海平面布置图：标题后缀
        let spec8 = SeaBoundaryMapSpec {
            kind: SeaMapKind::LayoutMap,
            ..full_spec()
        };
        let out8 = render_sea_boundary_map_svg(&test_boundary(), &spec8).unwrap();
        let svg8 = svg_of(&out8);
        assert!(
            svg8.contains("代理围填海项目宗海平面布置图"),
            "标题后缀应为平面布置图"
        );
        assert!(!svg8.contains("界址点编号及坐标"), "L.8 无坐标表");
        // L.7 回归：标题/坐标表/点号注记仍在
        let out7 = render_sea_boundary_map_svg(&test_boundary(), &full_spec()).unwrap();
        let svg7 = svg_of(&out7);
        assert!(svg7.contains("宗海界址图"));
        assert!(svg7.contains("界址点编号及坐标（北纬 | 东经）"));
        assert!(!out7.diagnostics.is_empty(), "L.7 有注记排版诊断");
    }
}
