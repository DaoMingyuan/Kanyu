//! 不动产制图引擎 —— GB/T 42547-2023《地籍调查规程》附录 L + 《不动产登记数据库标准》表 20。
//!
//! 移植自堪舆工具箱（KanyuTools QGIS 插件）`features/realestate_map/__init__.py` 与
//! `features/realestate_map/label_placement.py`（勘测定界图注记契约），行为与之一致：
//!
//! - **界址点号**：从宗地左上角（环 bbox 西北角最近顶点）起按 1、2、3… 沿环序编制，
//!   内环续编；注记在宗地外、近顶点、固定字号，角平分线朝外基准 1.2mm，
//!   压盖经径向/切向细步消解，外侧最小压盖候选优先，不回退宗地内。
//! - **边长注记**：锚点投影保持线段中点法线（零沿线平行偏移）、角度沿线
//!   （字头向北、允许向西）、固定字号、基准净空 1.0mm；压盖仅沿法线外移消解。
//! - **碰撞模型**：旋转矩形 SAT + 矩形-圆（界址点符号 Ø2.0mm 障碍）+ 矩形-环（压红线）。
//! - 毫米 ↔ 地图单位换算：`mu = mm × scale / 1000`。
//!
//! 引擎纯数据、不依赖渲染后端（SVG/PNG 渲染在 kanyu-render `parcelmap`）。

use crate::error::{KanyuError, Result};

/// 平面点（地图单位；x=东、y=北，与 GeoJSON 位置序一致）。
pub type Point2 = (f64, f64);

/// 环角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingRole {
    /// 外环。
    Exterior,
    /// 内环（洞）。
    Interior,
}

/// 闭合环（首尾坐标重复一次）。
#[derive(Debug, Clone)]
pub struct RealestateRing {
    /// 顶点（闭合）。
    pub points: Vec<Point2>,
    /// 外环/内环。
    pub role: RingRole,
}

impl RealestateRing {
    /// 构造（未闭合输入自动补闭合点）。
    pub fn new(points: Vec<Point2>, role: RingRole) -> Self {
        let mut points = points;
        if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            if first != last {
                points.push(first);
            }
        }
        Self { points, role }
    }
    /// 线段序列（相邻点对）。
    pub fn segments(&self) -> Vec<(Point2, Point2)> {
        self.points.windows(2).map(|w| (w[0], w[1])).collect()
    }
}

/// 单个宗地/宗海的权属边界（外环 + 内环）。
#[derive(Debug, Clone)]
pub struct ParcelBoundary {
    /// 外环。
    pub exterior: RealestateRing,
    /// 内环（洞）。
    pub interiors: Vec<RealestateRing>,
}

impl ParcelBoundary {
    /// 全部环（外环在前）。
    pub fn rings(&self) -> Vec<&RealestateRing> {
        std::iter::once(&self.exterior)
            .chain(self.interiors.iter())
            .collect()
    }
    /// 从 GeoJSON 几何（Polygon/MultiPolygon）提取权属边界；多部件取面积最大部件。
    pub fn from_geometry(value: &geojson::Value) -> Result<Self> {
        // GeoJSON Polygon 为环表（外环 + 内环）；MultiPolygon 为部件表
        let parts: Vec<&Vec<Vec<Vec<f64>>>> = match value {
            geojson::Value::Polygon(rings) => vec![rings],
            geojson::Value::MultiPolygon(polys) => polys.iter().collect(),
            _ => {
                return Err(KanyuError::Other(
                    "宗地几何不是面（仅支持 Polygon/MultiPolygon）".to_string(),
                ));
            }
        };
        let to_ring = |ring: &[Vec<f64>], role: RingRole| {
            RealestateRing::new(
                ring.iter()
                    .map(|p| {
                        (
                            p.first().copied().unwrap_or(0.0),
                            p.get(1).copied().unwrap_or(0.0),
                        )
                    })
                    .collect(),
                role,
            )
        };
        // 多部件宗地取面积最大部件为外环载体（并列时取前者，与 Python max 一致）
        let mut best: Option<(&Vec<Vec<Vec<f64>>>, f64)> = None;
        for part in parts.iter().copied().filter(|p| !p.is_empty()) {
            let area = ring_area(&to_ring(&part[0], RingRole::Exterior)).abs();
            let better = match best {
                None => true,
                Some((_, a)) => area > a,
            };
            if better {
                best = Some((part, area));
            }
        }
        let part = best
            .map(|(p, _)| p)
            .ok_or_else(|| KanyuError::Other("宗地几何为空".to_string()))?;
        let exterior = to_ring(&part[0], RingRole::Exterior);
        let interiors = part[1..]
            .iter()
            .map(|r| to_ring(r, RingRole::Interior))
            .collect();
        Ok(Self {
            exterior,
            interiors,
        })
    }
}

/// 鞋带公式面积（带符号；CCW 为正）。
pub fn ring_area(ring: &RealestateRing) -> f64 {
    let total: f64 = ring
        .points
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum();
    total / 2.0
}

/// 环中「左上角」顶点索引（不含重复闭合点）：环 bbox 西北角 (minX, maxY) 的最近顶点。
pub fn northwest_start_index(ring: &RealestateRing) -> usize {
    let pts: &[Point2] = if ring.points.len() > 1 {
        &ring.points[..ring.points.len() - 1]
    } else {
        &ring.points
    };
    if pts.is_empty() {
        return 0;
    }
    let min_x = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let mut best_index = 0;
    let mut best_dist = f64::INFINITY;
    for (i, &(x, y)) in pts.iter().enumerate() {
        let dist = (x - min_x).hypot(y - max_y);
        if dist < best_dist - 1e-9 {
            best_index = i;
            best_dist = dist;
        }
    }
    best_index
}

/// 界址点记录（对应 JZD 表）。
#[derive(Debug, Clone)]
pub struct BoundaryPointRecord {
    /// 界址点序号（左上角起 1、2、3…）。
    pub point_no: usize,
    /// 东坐标（地图单位）。
    pub x: f64,
    /// 北坐标（地图单位）。
    pub y: f64,
    /// 环索引（0=外环，1..=内环）。
    pub ring_index: usize,
    /// 点号前缀（宗地图为 J）。
    pub prefix: String,
}

impl BoundaryPointRecord {
    /// 显示点号（如 J1）。
    pub fn label(&self) -> String {
        format!("{}{}", self.prefix, self.point_no)
    }
}

/// 界址线记录（对应 JZX 表）。
#[derive(Debug, Clone)]
pub struct BoundaryLineRecord {
    /// 起点点号。
    pub start_no: usize,
    /// 终点点号。
    pub end_no: usize,
    /// 边长（米）。
    pub length_m: f64,
    /// 中点。
    pub midpoint: Point2,
    /// 数学角（度，逆时针为正，东为 0）。
    pub angle_deg: f64,
    /// 环索引。
    pub ring_index: usize,
}

/// 生成界址点记录：点号从外环左上角起按 1、2、3… 沿环序编制，内环续编。
pub fn generate_boundary_points(
    boundary: &ParcelBoundary,
    prefix: &str,
) -> Vec<BoundaryPointRecord> {
    let mut records = Vec::new();
    let mut number = 1;
    for (ring_index, ring) in boundary.rings().iter().enumerate() {
        let pts: &[Point2] = if ring.points.len() > 1 {
            &ring.points[..ring.points.len() - 1]
        } else {
            &ring.points
        };
        if pts.is_empty() {
            continue;
        }
        let start = northwest_start_index(ring);
        for &(x, y) in pts.iter().cycle().skip(start).take(pts.len()) {
            records.push(BoundaryPointRecord {
                point_no: number,
                x,
                y,
                ring_index,
                prefix: prefix.to_string(),
            });
            number += 1;
        }
    }
    records
}

/// 生成界址线记录：每条线段挂接相邻两点号，长度为两点间距离（米）。
pub fn generate_boundary_lines(
    boundary: &ParcelBoundary,
    points: &[BoundaryPointRecord],
) -> Vec<BoundaryLineRecord> {
    let mut lines = Vec::new();
    for (ring_index, _ring) in boundary.rings().iter().enumerate() {
        let ring_points: Vec<&BoundaryPointRecord> = points
            .iter()
            .filter(|p| p.ring_index == ring_index)
            .collect();
        let count = ring_points.len();
        if count == 0 {
            continue;
        }
        for (i, a) in ring_points.iter().enumerate() {
            let b = ring_points[(i + 1) % count];
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            lines.push(BoundaryLineRecord {
                start_no: a.point_no,
                end_no: b.point_no,
                length_m: dx.hypot(dy),
                midpoint: ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
                angle_deg: dy.atan2(dx).to_degrees(),
                ring_index,
            });
        }
    }
    lines
}

/// 边长注记格式：米，两位小数（与样图 10.98 / 135.00 一致）。
pub fn format_edge_length(length_m: f64) -> String {
    format!("{length_m:.2}")
}

// ---------------------------------------------------------------------------
// 宗地面要素选取与属性拾取（CLI/MCP 共用入口助手）
// ---------------------------------------------------------------------------

/// 宗地面要素选取与权属边界提取：多面要素缺省取面积最大者（外环鞋带面积）；
/// `index` 指定后按文档序第 N 个（0 起，越界中文报错）。
/// 返回（权属边界, 要素属性表）。集合中无面要素时报中文错误。
pub fn boundary_from_collection(
    collection: &geojson::FeatureCollection,
    index: Option<usize>,
) -> Result<(ParcelBoundary, serde_json::Map<String, serde_json::Value>)> {
    let mut polygons: Vec<&geojson::Feature> = collection
        .features
        .iter()
        .filter(|f| {
            matches!(
                f.geometry.as_ref().map(|g| &g.value),
                Some(geojson::Value::Polygon(_)) | Some(geojson::Value::MultiPolygon(_))
            )
        })
        .collect();
    if polygons.is_empty() {
        return Err(crate::error::KanyuError::Other(
            "集合中无面要素（宗地制图需要 Polygon/MultiPolygon 宗地边界）".to_string(),
        ));
    }
    let feature = match index {
        Some(i) => polygons.get(i).copied().ok_or_else(|| {
            crate::error::KanyuError::Other(format!(
                "面要素序号越界：index {i}，实际仅 {} 个面要素",
                polygons.len()
            ))
        })?,
        None => {
            polygons.sort_by(|a, b| {
                polygon_area(b)
                    .partial_cmp(&polygon_area(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            polygons[0]
        }
    };
    let boundary = ParcelBoundary::from_geometry(&feature.geometry.as_ref().unwrap().value)?;
    let props = feature.properties.clone().unwrap_or_default();
    Ok((boundary, props))
}

/// 面要素外环鞋带面积（多面取最大部件；排序用）。
fn polygon_area(feature: &geojson::Feature) -> f64 {
    match &feature.geometry.as_ref().unwrap().value {
        geojson::Value::Polygon(rings) => rings.first().map(|r| shoelace(r)).unwrap_or(0.0),
        geojson::Value::MultiPolygon(polys) => polys
            .iter()
            .filter_map(|r| r.first())
            .map(|r| shoelace(r))
            .fold(0.0_f64, f64::max),
        _ => 0.0,
    }
}

/// 环鞋带面积（绝对值）。
fn shoelace(ring: &[Vec<f64>]) -> f64 {
    let mut total = 0.0;
    for i in 0..ring.len().saturating_sub(1) {
        total += ring[i][0] * ring[i + 1][1] - ring[i + 1][0] * ring[i][1];
    }
    total.abs() / 2.0
}

/// 要素属性字符串拾取（多候选键按序，空串/Null 跳过；非字符串取 JSON 文本）。
pub fn feature_prop_str(
    props: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    for key in keys {
        if let Some(v) = props.get(*key) {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => continue,
                other => other.to_string(),
            };
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// 要素属性数值拾取（多候选键按序；字符串可解析为数值亦收）。
pub fn feature_prop_f64(
    props: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<f64> {
    for key in keys {
        match props.get(*key) {
            Some(serde_json::Value::Number(n)) => return n.as_f64(),
            Some(serde_json::Value::String(s)) => {
                if let Ok(v) = s.trim().parse::<f64>() {
                    return Some(v);
                }
            }
            _ => {}
        }
    }
    None
}

/// 排版旋转归一化：数学角 → (-90°, 90°] 内**顺时针为正**的度数
/// （字头向北、允许向西，附录 L.1.2；与 SVG rotate()/QGIS 存储同号，渲染直接可用）。
pub fn upright_rotation(angle_deg: f64) -> f64 {
    let mut normalized = (angle_deg + 90.0).rem_euclid(180.0) - 90.0;
    if normalized == -90.0 {
        normalized = 90.0;
    }
    -normalized
}

/// 估算单行文本纸面宽/高（毫米）：CJK 1.0em、ASCII 0.6em、行高 1.0 倍。
pub fn text_extent_mm(text: &str, font_mm: f64) -> (f64, f64) {
    let width: f64 = text
        .chars()
        .map(|ch| {
            if ch as u32 > 0x7F {
                font_mm
            } else {
                font_mm * 0.6
            }
        })
        .sum();
    (width, font_mm)
}

/// 注记印刷矩形（地图单位；rot_rad 为数学角，逆时针为正）。
#[derive(Debug, Clone, Copy)]
pub struct LabelRect {
    /// 中心 x。
    pub cx: f64,
    /// 中心 y。
    pub cy: f64,
    /// 宽。
    pub w: f64,
    /// 高。
    pub h: f64,
    /// 旋转（弧度，数学角）。
    pub rot_rad: f64,
}

impl LabelRect {
    /// 四角坐标（从左上起逆时针）。
    pub fn corners(&self) -> [Point2; 4] {
        let (c, s) = (self.rot_rad.cos(), self.rot_rad.sin());
        let (hw, hh) = (self.w / 2.0, self.h / 2.0);
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
            .map(|(dx, dy)| (self.cx + dx * c - dy * s, self.cy + dx * s + dy * c))
    }
}

/// SAT 旋转矩形相交检测。
pub fn rects_overlap(a: &LabelRect, b: &LabelRect) -> bool {
    let ca = a.corners();
    let cb = b.corners();
    for pts in [&ca, &cb] {
        for (p1, p2) in rect_edges(pts) {
            let (ex, ey) = (p2.0 - p1.0, p2.1 - p1.1);
            let norm = ex.hypot(ey);
            let norm = if norm == 0.0 { 1.0 } else { norm };
            let (nx, ny) = (-ey / norm, ex / norm);
            let (amin, amax) = project_pts(&ca, nx, ny);
            let (bmin, bmax) = project_pts(&cb, nx, ny);
            if amax < bmin || bmax < amin {
                return false;
            }
        }
    }
    true
}

/// 矩形与圆相交（端点界址点符号 Ø2.0mm 障碍）。
pub fn rect_circle_overlap(rect: &LabelRect, center: Point2, radius: f64) -> bool {
    let (c, s) = ((-rect.rot_rad).cos(), (-rect.rot_rad).sin());
    let (dx, dy) = (center.0 - rect.cx, center.1 - rect.cy);
    let (lx, ly) = (dx * c - dy * s, dx * s + dy * c);
    let qx = (lx.abs() - rect.w / 2.0).max(0.0);
    let qy = (ly.abs() - rect.h / 2.0).max(0.0);
    qx * qx + qy * qy <= radius * radius
}

/// 点在环内（射线法；环为闭合点列）。
pub fn point_in_ring(pt: Point2, ring_pts: &[Point2]) -> bool {
    let (x, y) = pt;
    let mut inside = false;
    for w in ring_pts.windows(2) {
        let ((x1, y1), (x2, y2)) = (w[0], w[1]);
        if (y1 > y) != (y2 > y) {
            let xin = (x2 - x1) * (y - y1) / (y2 - y1) + x1;
            if x < xin {
                inside = !inside;
            }
        }
    }
    inside
}

/// 矩形与权属环相交（注记压红线检测）。
pub fn rect_ring_overlap(rect: &LabelRect, ring_pts: &[Point2]) -> bool {
    let corners = rect.corners();
    // 角点落入环内
    if corners.iter().any(|&pt| point_in_ring(pt, ring_pts)) {
        return true;
    }
    // 矩形边与环边相交
    let edge_hit = rect_edges(&corners).any(|(a1, a2)| {
        ring_pts
            .windows(2)
            .any(|w| segments_intersect(a1, a2, w[0], w[1]))
    });
    edge_hit
}

/// 单个注记放置结果（含诊断）。
#[derive(Debug, Clone)]
pub struct PlacedLabel {
    /// 注记文本。
    pub text: String,
    /// 最终印刷矩形（地图单位）。
    pub rect: LabelRect,
    /// 排版旋转（度，顺时针为正；见 [`upright_rotation`]）。
    pub rotation_deg: f64,
    /// 放置途径：base | escape+k | tangent±k | least_bad | inside_fallback。
    pub reason: String,
    /// 中心到权属边界最近距离（纸面毫米）。
    pub clearance_mm: f64,
    /// 最终是否仍有残余压盖（最小压盖兜底时 true）。
    pub overlap: bool,
}

/// 排版诊断报告（纸面毫米，供回归与调参）。
#[derive(Debug, Clone, Default)]
pub struct PlacementReport {
    /// 放置结果。
    pub labels: Vec<PlacedLabel>,
}

impl PlacementReport {
    /// 残余压盖注记数。
    pub fn overlap_count(&self) -> usize {
        self.labels.iter().filter(|lb| lb.overlap).count()
    }
    /// 注记两两相交对数。
    pub fn pair_overlaps(&self) -> usize {
        let mut count = 0;
        for (i, a) in self.labels.iter().enumerate() {
            for b in &self.labels[i + 1..] {
                if rects_overlap(&a.rect, &b.rect) {
                    count += 1;
                }
            }
        }
        count
    }
}

/// 边长注记排版参数。
#[derive(Debug, Clone, Copy)]
pub struct EdgeLabelOptions {
    /// 字号（毫米，默认 2.4）。
    pub font_mm: f64,
    /// 基准净空（毫米，默认 1.0）。
    pub base_clear_mm: f64,
    /// 出图比例尺分母（默认 1000）。
    pub scale: u32,
    /// 法线逃逸步数（默认 6）。
    pub escape_steps: usize,
    /// 逃逸步长（毫米，默认 0.5）。
    pub escape_step_mm: f64,
}

impl Default for EdgeLabelOptions {
    fn default() -> Self {
        Self {
            font_mm: 2.4,
            base_clear_mm: 1.0,
            scale: 1000,
            escape_steps: 6,
            escape_step_mm: 0.5,
        }
    }
}

/// 界址点号注记排版参数。
#[derive(Debug, Clone, Copy)]
pub struct PointLabelOptions {
    /// 字号（毫米，默认 2.4）。
    pub font_mm: f64,
    /// 基准偏移（毫米，默认 1.2）。
    pub base_offset_mm: f64,
    /// 出图比例尺分母（默认 1000）。
    pub scale: u32,
    /// 径向逃逸步数（默认 5）。
    pub escape_steps: usize,
    /// 逃逸步长（毫米，默认 0.5）。
    pub escape_step_mm: f64,
    /// 切向微步数（默认 2）。
    pub tangent_steps: usize,
}

impl Default for PointLabelOptions {
    fn default() -> Self {
        Self {
            font_mm: 2.4,
            base_offset_mm: 1.2,
            scale: 1000,
            escape_steps: 5,
            escape_step_mm: 0.5,
            tangent_steps: 2,
        }
    }
}

/// 边长注记排版：基准中点法线 1.0mm；压盖沿法线细步外移；清洁偏远候选回拉。
///
/// 障碍：权属环（压红线）、端点界址点符号（Ø2.0mm 圆）、已放置边长注记、
/// 外部附加障碍 `extra_obstacle_rects`（如先放置的点号注记矩形）。
/// 外法线按环走向（鞋带面积判 CCW）取，不依赖质心，凹角/锯齿宗地同样正确。
pub fn place_edge_labels(
    boundary: &ParcelBoundary,
    lines: &[BoundaryLineRecord],
    points: &[BoundaryPointRecord],
    opts: &EdgeLabelOptions,
    extra_obstacle_rects: &[LabelRect],
) -> PlacementReport {
    let mut report = PlacementReport::default();
    let mut placed_rects: Vec<LabelRect> = extra_obstacle_rects.to_vec();
    let rings = boundary.rings();

    for rec in lines {
        let Some(ring) = rings.get(rec.ring_index) else {
            continue;
        };
        let ring_pts = &ring.points;
        let text = format_edge_length(rec.length_m);
        let (w_mm, h_mm) = text_extent_mm(&text, opts.font_mm);
        let (w_map, h_map) = (mm_to_map(w_mm, opts.scale), mm_to_map(h_mm, opts.scale));
        // 外法线按环走向取（边方向单位向量即 (cos θ, sin θ)）
        let ccw = ring_ccw(ring_pts);
        let (nx, ny) = edge_outward_normal(
            rec.angle_deg.to_radians().cos(),
            rec.angle_deg.to_radians().sin(),
            ccw,
        );
        let rot_math = rec.angle_deg.to_radians();
        let qgis_rot = upright_rotation(rec.angle_deg);
        // 边两端界址点坐标（端点符号障碍）：按起点/终点点号直接查找
        let endpoints: Vec<Point2> = [rec.start_no, rec.end_no]
            .iter()
            .filter_map(|no| {
                points
                    .iter()
                    .find(|p| p.point_no == *no)
                    .map(|p| (p.x, p.y))
            })
            .collect();

        let mut best: Option<(f64, LabelRect)> = None; // (overlap_score, rect)
        let mut first_clean: Option<(usize, LabelRect)> = None;
        for k in 0..=opts.escape_steps {
            let off_mm = opts.base_clear_mm + k as f64 * opts.escape_step_mm;
            let off = mm_to_map(off_mm + h_mm / 2.0, opts.scale);
            let rect = LabelRect {
                cx: rec.midpoint.0 + nx * off,
                cy: rec.midpoint.1 + ny * off,
                w: w_map,
                h: h_map,
                rot_rad: rot_math,
            };
            let ring_hit = rect_ring_overlap(&rect, ring_pts);
            let circle_hit = endpoints
                .iter()
                .any(|&ep| rect_circle_overlap(&rect, ep, mm_to_map(1.0, opts.scale)));
            let label_hits: Vec<&LabelRect> = placed_rects
                .iter()
                .filter(|r| rects_overlap(&rect, r))
                .collect();
            if !ring_hit && !circle_hit && label_hits.is_empty() {
                first_clean = Some((k, rect));
                break;
            }
            let score = if ring_hit { 100.0 } else { 0.0 }
                + if circle_hit { 10.0 } else { 0.0 }
                + label_hits
                    .iter()
                    .map(|r| overlap_area_estimate(&rect, r))
                    .sum::<f64>();
            let better = match &best {
                None => true,
                Some((s, _)) => score < *s,
            };
            if better {
                best = Some((score, rect));
            }
        }

        let (reason, rect, overlap) = match first_clean {
            Some((k, rect)) => (
                if k == 0 {
                    "base".to_string()
                } else {
                    format!("escape+{k}")
                },
                rect,
                false,
            ),
            // 候选循环至少评估一次（k=0），best 必有值
            None => (
                "least_bad".to_string(),
                best.expect("候选循环至少评估一次").1,
                true,
            ),
        };
        let clearance_mm =
            distance_point_to_ring((rect.cx, rect.cy), ring_pts) / mm_to_map(1.0, opts.scale);
        report.labels.push(PlacedLabel {
            text,
            rect,
            rotation_deg: qgis_rot,
            reason,
            clearance_mm,
            overlap,
        });
        placed_rects.push(rect);
    }
    report
}

/// 界址点号注记排版：角平分线朝外 1.2mm 基准；压盖经径向+切向细步消解。
///
/// 硬约束：注记中心在宗地外（不回退宗地内）；障碍：权属环、邻顶点符号、
/// 已放置点号注记、外部障碍 `obstacle_rects`（如先放置的边长注记矩形）。
/// 全部候选中心均落宗地内（尖锐凹角）时内侧兜底并标记 `inside_fallback`。
pub fn place_point_labels(
    boundary: &ParcelBoundary,
    points: &[BoundaryPointRecord],
    opts: &PointLabelOptions,
    obstacle_rects: &[LabelRect],
) -> PlacementReport {
    let mut report = PlacementReport::default();
    let mut placed_rects: Vec<LabelRect> = obstacle_rects.to_vec();

    for (ring_index, ring) in boundary.rings().iter().enumerate() {
        let ring_points: Vec<&BoundaryPointRecord> = points
            .iter()
            .filter(|p| p.ring_index == ring_index)
            .collect();
        let count = ring_points.len();
        if count == 0 {
            continue;
        }
        let ring_pts = &ring.points;
        let ccw = ring_ccw(ring_pts);
        for (i, rec) in ring_points.iter().enumerate() {
            let prev = ring_points[(i + count - 1) % count];
            let next = ring_points[(i + 1) % count];
            let (bx, by) =
                bisector_outward(ccw, (rec.x, rec.y), (prev.x, prev.y), (next.x, next.y));
            let text = rec.label();
            let (w_mm, h_mm) = text_extent_mm(&text, opts.font_mm);
            let (w_map, h_map) = (mm_to_map(w_mm, opts.scale), mm_to_map(h_mm, opts.scale));

            // 候选序列：先径向 k=0..=escape_steps（off=base+k×step），
            // 再切向 ±t（t=1..=tangent_steps，沿 ⊥角平分线方向 ±t×step，径向距离取 base）
            // 每项为 (径向偏移 mm, 切向偏移 mm, 途径标签)
            let mut candidates: Vec<(f64, f64, String)> = Vec::new();
            for k in 0..=opts.escape_steps {
                candidates.push((
                    opts.base_offset_mm + k as f64 * opts.escape_step_mm,
                    0.0,
                    if k == 0 {
                        "base".to_string()
                    } else {
                        format!("escape+{k}")
                    },
                ));
            }
            for t in 1..=opts.tangent_steps {
                for sign in [1.0_f64, -1.0] {
                    candidates.push((
                        opts.base_offset_mm,
                        sign * t as f64 * opts.escape_step_mm,
                        format!("tangent{}{t}", if sign > 0.0 { "+" } else { "-" }),
                    ));
                }
            }

            let (tx, ty) = (-by, bx); // 切线 ⊥ 角平分线
            let mut best: Option<(f64, f64, LabelRect)> = None; // (score, 距顶点距离, rect)
            let mut first_clean: Option<(String, LabelRect)> = None;
            for (off_mm, lat_mm, tag) in &candidates {
                let off = mm_to_map(off_mm + h_mm / 2.0, opts.scale);
                let lat = mm_to_map(*lat_mm, opts.scale);
                let cx = rec.x + bx * off + tx * lat;
                let cy = rec.y + by * off + ty * lat;
                // 硬约束：中心必须在宗地外
                if point_in_ring((cx, cy), ring_pts) {
                    continue;
                }
                let rect = LabelRect {
                    cx,
                    cy,
                    w: w_map,
                    h: h_map,
                    rot_rad: 0.0,
                };
                let ring_hit = rect_ring_overlap(&rect, ring_pts);
                let circle_hit =
                    ring_points
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .any(|(_, v)| {
                            rect_circle_overlap(&rect, (v.x, v.y), mm_to_map(1.0, opts.scale))
                        });
                let label_hits: Vec<&LabelRect> = placed_rects
                    .iter()
                    .filter(|r| rects_overlap(&rect, r))
                    .collect();
                if !ring_hit && !circle_hit && label_hits.is_empty() {
                    first_clean = Some((tag.clone(), rect));
                    break;
                }
                let score = if ring_hit { 100.0 } else { 0.0 }
                    + if circle_hit { 10.0 } else { 0.0 }
                    + label_hits
                        .iter()
                        .map(|r| overlap_area_estimate(&rect, r))
                        .sum::<f64>();
                let dist = (cx - rec.x).hypot(cy - rec.y);
                // 同分取距顶点近者（严格小于，先候选赢同分同距）
                let better = match &best {
                    None => true,
                    Some((s, d, _)) => (score, dist) < (*s, *d),
                };
                if better {
                    best = Some((score, dist, rect));
                }
            }

            let (reason, rect, overlap) = if let Some((tag, rect)) = first_clean {
                (tag, rect, false)
            } else if let Some((_, _, rect)) = best {
                ("least_bad".to_string(), rect, true)
            } else {
                // 全部候选中心均落宗地内（尖锐凹角）：内侧兜底（最小偏移），
                // 仅作最后手段并显式标记，不回退为常态
                let off = mm_to_map(opts.base_offset_mm + h_mm / 2.0, opts.scale);
                let rect = LabelRect {
                    cx: rec.x + bx * off,
                    cy: rec.y + by * off,
                    w: w_map,
                    h: h_map,
                    rot_rad: 0.0,
                };
                ("inside_fallback".to_string(), rect, true)
            };
            let clearance_mm =
                distance_point_to_ring((rect.cx, rect.cy), ring_pts) / mm_to_map(1.0, opts.scale);
            report.labels.push(PlacedLabel {
                text,
                rect,
                rotation_deg: 0.0,
                reason,
                clearance_mm,
                overlap,
            });
            placed_rects.push(rect);
        }
    }
    report
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 毫米 → 地图单位：`mu = mm × scale / 1000`。
fn mm_to_map(mm: f64, scale: u32) -> f64 {
    mm * scale as f64 / 1000.0
}

/// 旋转矩形四条边（首尾相接）。
fn rect_edges(pts: &[Point2; 4]) -> impl Iterator<Item = (Point2, Point2)> + '_ {
    pts.iter()
        .zip(pts.iter().cycle().skip(1))
        .map(|(&a, &b)| (a, b))
}

/// 点集在轴 (ax, ay) 上的投影区间。
fn project_pts(pts: &[Point2; 4], ax: f64, ay: f64) -> (f64, f64) {
    pts.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            let v = p.0 * ax + p.1 * ay;
            (lo.min(v), hi.max(v))
        })
}

/// 线段相交判定（跨立实验，退化共线不判交，与 Python 蓝本一致）。
fn segments_intersect(a1: Point2, a2: Point2, b1: Point2, b2: Point2) -> bool {
    let orient =
        |p: Point2, q: Point2, r: Point2| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let d1 = orient(b1, b2, a1);
    let d2 = orient(b1, b2, a2);
    let d3 = orient(a1, a2, b1);
    let d4 = orient(a1, a2, b2);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// 压盖严重度估算（非精确面积：中心距反比，仅供最小压盖候选排序）。
fn overlap_area_estimate(a: &LabelRect, b: &LabelRect) -> f64 {
    if !rects_overlap(a, b) {
        return 0.0;
    }
    let d = (a.cx - b.cx).hypot(a.cy - b.cy);
    1.0 / d.max(1e-6)
}

/// 点到环（闭合点列）的最近距离（地图单位）。
fn distance_point_to_ring(pt: Point2, ring_pts: &[Point2]) -> f64 {
    let mut best = f64::INFINITY;
    for w in ring_pts.windows(2) {
        let ((x1, y1), (x2, y2)) = (w[0], w[1]);
        let (dx, dy) = (x2 - x1, y2 - y1);
        let length2 = dx * dx + dy * dy;
        if length2 <= 0.0 {
            continue;
        }
        let t = (((pt.0 - x1) * dx + (pt.1 - y1) * dy) / length2).clamp(0.0, 1.0);
        let (px, py) = (x1 + dx * t, y1 + dy * t);
        best = best.min((pt.0 - px).hypot(pt.1 - py));
    }
    best
}

/// 环走向：鞋带面积 > 0 为逆时针（内部在行进方向左侧）。
fn ring_ccw(ring_pts: &[Point2]) -> bool {
    let total: f64 = ring_pts
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum();
    total > 0.0
}

/// 按环走向求边的外法线（不依赖质心，锯齿/凹角宗地同样正确）。
fn edge_outward_normal(dx: f64, dy: f64, ccw: bool) -> Point2 {
    let norm = dx.hypot(dy);
    let norm = if norm == 0.0 { 1.0 } else { norm };
    if ccw {
        // 内部在左 → 外部在右（行进方向顺时针旋转 90°）
        (dy / norm, -dx / norm)
    } else {
        (-dy / norm, dx / norm)
    }
}

/// 环某边的外法线（公共入口；按环走向判定外侧，锯齿/凹角宗地同样正确）。
/// 供四至/邻宗地注记等版面要素取边外指方向。
pub fn ring_edge_outward_normal(ring: &RealestateRing, start: Point2, end: Point2) -> Point2 {
    edge_outward_normal(end.0 - start.0, end.1 - start.1, ring_ccw(&ring.points))
}

/// 顶点角平分线朝外单位向量（按环走向取两邻边外法线合成；
/// 尖点退化时用 prev→next 边外法线，不依赖质心——锯齿/凹角宗地同正确）。
fn bisector_outward(ccw: bool, rec: Point2, prev: Point2, next: Point2) -> Point2 {
    let n1 = edge_outward_normal(rec.0 - prev.0, rec.1 - prev.1, ccw);
    let n2 = edge_outward_normal(next.0 - rec.0, next.1 - rec.1, ccw);
    let (bx, by) = (n1.0 + n2.0, n1.1 + n2.1);
    let bn = bx.hypot(by);
    if bn < 1e-9 {
        edge_outward_normal(next.0 - prev.0, next.1 - prev.1, ccw)
    } else {
        (bx / bn, by / bn)
    }
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造 CCW 存储的 100m×50m 矩形宗地（投影坐标，scale=1000 时 1mm=1m）。
    fn rect_boundary_ccw() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 同一矩形的 CW 存储形态。
    fn rect_boundary_cw() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![(0.0, 0.0), (0.0, 50.0), (100.0, 50.0), (100.0, 0.0)],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    #[test]
    fn ring_close_and_northwest_start() {
        // 未闭合输入自动补闭合点
        let ring =
            RealestateRing::new(vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)], RingRole::Exterior);
        assert_eq!(ring.points.first(), ring.points.last());
        assert_eq!(ring.segments().len(), 3);

        // 左上角顶点 (0,50) 为环中索引 3（不含重复闭合点）
        let boundary = rect_boundary_ccw();
        assert_eq!(northwest_start_index(&boundary.exterior), 3);
        assert!((ring_area(&boundary.exterior) - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn boundary_points_and_lines_records() {
        let boundary = rect_boundary_ccw();
        let points = generate_boundary_points(&boundary, "J");
        assert_eq!(points.len(), 4);
        // 1 号点为左上角，沿环序（CCW）续编
        assert_eq!(
            (points[0].point_no, points[0].x, points[0].y),
            (1, 0.0, 50.0)
        );
        assert_eq!((points[1].x, points[1].y), (0.0, 0.0));
        assert_eq!((points[2].x, points[2].y), (100.0, 0.0));
        assert_eq!((points[3].x, points[3].y), (100.0, 50.0));
        assert_eq!(points[0].label(), "J1");

        let lines = generate_boundary_lines(&boundary, &points);
        assert_eq!(lines.len(), 4);
        // 首尾挂接：1→2、2→3、3→4、4→1
        let hooks: Vec<(usize, usize)> = lines.iter().map(|l| (l.start_no, l.end_no)).collect();
        assert_eq!(hooks, vec![(1, 2), (2, 3), (3, 4), (4, 1)]);
        // 边长：50 / 100 / 50 / 100 米
        assert!((lines[0].length_m - 50.0).abs() < 1e-9);
        assert!((lines[1].length_m - 100.0).abs() < 1e-9);
        // 中点与角度（数学角：东 0°，逆时针为正）
        assert_eq!(lines[1].midpoint, (50.0, 0.0));
        assert!((lines[1].angle_deg - 0.0).abs() < 1e-9);
        assert!((lines[0].angle_deg + 90.0).abs() < 1e-9);
    }

    #[test]
    fn edge_length_format_and_upright_rotation() {
        // 两位小数（与样图 10.98 / 135.00 一致）
        assert_eq!(format_edge_length(10.98), "10.98");
        assert_eq!(format_edge_length(135.0), "135.00");
        assert_eq!(format_edge_length(0.5), "0.50");
        // 归一化到 (-90, 90] 后取负（顺时针为正，字头向北/允许向西）：
        // 0°→0、90°→-90、135°→45、-135°→-45（按 Python 语义）
        assert_eq!(upright_rotation(0.0), 0.0);
        assert_eq!(upright_rotation(90.0), -90.0);
        assert_eq!(upright_rotation(-90.0), -90.0);
        assert_eq!(upright_rotation(135.0), 45.0);
        assert_eq!(upright_rotation(-135.0), -45.0);
        assert_eq!(upright_rotation(45.0), -45.0);
        assert_eq!(upright_rotation(180.0), 0.0);
    }

    #[test]
    fn text_extent_ascii_cjk() {
        // 纯 ASCII：每字符 0.6em，行高 1.0 倍
        let (w, h) = text_extent_mm("135.00", 2.4);
        assert!((w - 6.0 * 0.6 * 2.4).abs() < 1e-9);
        assert!((h - 2.4).abs() < 1e-9);
        // CJK 与 ASCII 混合：CJK 每字符 1.0em
        let (w, _) = text_extent_mm("宗地J1", 2.4);
        assert!((w - (2.0 * 2.4 + 2.0 * 0.6 * 2.4)).abs() < 1e-9);
    }

    #[test]
    fn outward_normal_rect_both_windings() {
        // CCW 与 CW 存储的同一矩形：各边外法线都朝外（环走向法）
        for boundary in [rect_boundary_ccw(), rect_boundary_cw()] {
            let pts = &boundary.exterior.points;
            let ccw = ring_ccw(pts);
            for (a, b) in boundary.exterior.segments() {
                let n = edge_outward_normal(b.0 - a.0, b.1 - a.1, ccw);
                let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                // 中点 + 法线×ε 必须在环外
                let probe = (mid.0 + n.0 * 0.5, mid.1 + n.1 * 0.5);
                assert!(!point_in_ring(probe, pts), "外法线应朝外: {a:?}->{b:?}");
            }
        }
    }

    #[test]
    fn outward_normal_l_shape_concave() {
        // L 形凹角宗地（CCW）；(30,30) 为凹点，凹点两侧边外法线也必须朝外
        let ring = RealestateRing::new(
            vec![
                (0.0, 0.0),
                (60.0, 0.0),
                (60.0, 30.0),
                (30.0, 30.0),
                (30.0, 60.0),
                (0.0, 60.0),
            ],
            RingRole::Exterior,
        );
        assert!(ring_ccw(&ring.points));
        let ccw = ring_ccw(&ring.points);
        for (a, b) in ring.segments() {
            let n = edge_outward_normal(b.0 - a.0, b.1 - a.1, ccw);
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let probe = (mid.0 + n.0 * 0.5, mid.1 + n.1 * 0.5);
            assert!(
                !point_in_ring(probe, &ring.points),
                "凹角边外法线应朝外: {a:?}->{b:?}"
            );
        }
    }

    #[test]
    fn sat_rect_overlap_cases() {
        let axis_a = LabelRect {
            cx: 0.0,
            cy: 0.0,
            w: 4.0,
            h: 4.0,
            rot_rad: 0.0,
        };
        // 轴对齐：相交 / 相离
        let b_hit = LabelRect {
            cx: 3.9,
            cy: 0.0,
            w: 4.0,
            h: 4.0,
            rot_rad: 0.0,
        };
        let b_miss = LabelRect {
            cx: 4.1,
            cy: 0.0,
            w: 4.0,
            h: 4.0,
            rot_rad: 0.0,
        };
        assert!(rects_overlap(&axis_a, &b_hit));
        assert!(!rects_overlap(&axis_a, &b_miss));
        // 旋转 45°：角点刺入 → 相交
        let rot_hit = LabelRect {
            cx: 4.5,
            cy: 0.0,
            w: 4.0,
            h: 4.0,
            rot_rad: std::f64::consts::FRAC_PI_4,
        };
        assert!(rects_overlap(&axis_a, &rot_hit));
        // 旋转 45°：轴对齐包围盒重叠但 SAT 沿对角轴判离
        let rot_miss = LabelRect {
            cx: 3.4,
            cy: 3.4,
            w: 2.0,
            h: 2.0,
            rot_rad: std::f64::consts::FRAC_PI_4,
        };
        assert!(!rects_overlap(&axis_a, &rot_miss));
    }

    #[test]
    fn circle_and_ring_overlap_cases() {
        let rect = LabelRect {
            cx: 0.0,
            cy: 0.0,
            w: 2.0,
            h: 2.0,
            rot_rad: 0.0,
        };
        // 圆心在内 → 相交
        assert!(rect_circle_overlap(&rect, (0.0, 0.0), 0.1));
        // 圆与矩形边相切（中心距边恰为半径）→ 边界判交
        assert!(rect_circle_overlap(&rect, (1.5, 0.0), 0.5));
        // 差 0.01 相离
        assert!(!rect_circle_overlap(&rect, (1.5, 0.0), 0.49));

        let boundary = rect_boundary_ccw();
        let pts = &boundary.exterior.points;
        // 点环包含：内外点
        assert!(point_in_ring((50.0, 25.0), pts));
        assert!(!point_in_ring((150.0, 25.0), pts));
        assert!(!point_in_ring((-1.0, -1.0), pts));
        // 矩形压红线：跨界 → true；整矩形在外 → false
        let cross = LabelRect {
            cx: 0.0,
            cy: 0.0,
            w: 4.0,
            h: 4.0,
            rot_rad: 0.0,
        };
        assert!(rect_ring_overlap(&cross, pts));
        let outside = LabelRect {
            cx: -10.0,
            cy: -10.0,
            w: 2.0,
            h: 2.0,
            rot_rad: 0.0,
        };
        assert!(!rect_ring_overlap(&outside, pts));
    }

    #[test]
    fn edge_labels_rect_all_base() {
        let boundary = rect_boundary_ccw();
        let points = generate_boundary_points(&boundary, "J");
        let lines = generate_boundary_lines(&boundary, &points);
        let report = place_edge_labels(
            &boundary,
            &lines,
            &points,
            &EdgeLabelOptions::default(),
            &[],
        );
        assert_eq!(report.labels.len(), 4);
        assert_eq!(report.overlap_count(), 0);
        for (label, line) in report.labels.iter().zip(&lines) {
            // 全部走基准位（中点法线 1.0mm 净空）
            assert_eq!(label.reason, "base");
            assert_eq!(label.text, format_edge_length(line.length_m));
            // 注记中心在环外
            assert!(!point_in_ring(
                (label.rect.cx, label.rect.cy),
                &boundary.exterior.points
            ));
            // 净空 ≈ 基准 1.0mm + 字高一半 1.2mm
            assert!(
                (label.clearance_mm - 2.2).abs() < 0.01,
                "clearance={}",
                label.clearance_mm
            );
            // 旋转沿线且字头向北/西（(-90, 90] 内顺时针为正）
            assert_eq!(label.rotation_deg, upright_rotation(line.angle_deg));
            assert!(label.rotation_deg.abs() <= 90.0);
        }
    }

    #[test]
    fn point_labels_rect_all_base_outside() {
        let boundary = rect_boundary_ccw();
        let points = generate_boundary_points(&boundary, "J");
        let report = place_point_labels(&boundary, &points, &PointLabelOptions::default(), &[]);
        assert_eq!(report.labels.len(), 4);
        assert_eq!(report.overlap_count(), 0);
        assert_eq!(report.pair_overlaps(), 0);
        for label in &report.labels {
            assert_eq!(label.reason, "base");
            // 硬约束：注记中心在宗地外
            assert!(!point_in_ring(
                (label.rect.cx, label.rect.cy),
                &boundary.exterior.points
            ));
            // 净空 ≈ 基准 1.2mm + 字高一半 1.2mm
            assert!(
                (label.clearance_mm - 2.4).abs() < 0.01,
                "clearance={}",
                label.clearance_mm
            );
        }
    }

    #[test]
    fn point_labels_zigzag_parcel_no_overlap() {
        // 锯齿宗地：底边多顶点近似共线（±0.2m 抖动）
        let boundary = ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (0.0, 0.0),
                    (10.0, 0.2),
                    (20.0, -0.2),
                    (30.0, 0.2),
                    (40.0, -0.2),
                    (50.0, 0.0),
                    (50.0, 20.0),
                    (0.0, 20.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        };
        let points = generate_boundary_points(&boundary, "J");
        assert_eq!(points.len(), 8);
        let report = place_point_labels(&boundary, &points, &PointLabelOptions::default(), &[]);
        assert_eq!(report.labels.len(), 8);
        // 锯齿下角平分线（环走向法）仍朝外：中心全部在环外、两两无压盖
        assert_eq!(report.pair_overlaps(), 0);
        assert_eq!(report.overlap_count(), 0);
        for label in &report.labels {
            assert_eq!(label.reason, "base");
            assert!(!point_in_ring(
                (label.rect.cx, label.rect.cy),
                &boundary.exterior.points
            ));
        }
    }

    #[test]
    fn point_labels_sharp_crack_inside_fallback() {
        // 尖锐裂缝宗地：裂缝尖端 (25,5) 为极尖锐凹角，
        // 外法线合成的角平分线指向宗地内部，全部候选中心落环内 → 内侧兜底
        let boundary = ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (0.0, 0.0),
                    (50.0, 0.0),
                    (50.0, 50.0),
                    (24.99, 50.0),
                    (25.0, 5.0),
                    (25.01, 50.0),
                    (0.0, 50.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        };
        let points = generate_boundary_points(&boundary, "J");
        let report = place_point_labels(&boundary, &points, &PointLabelOptions::default(), &[]);
        assert_eq!(report.labels.len(), 7);
        let tip = report
            .labels
            .iter()
            .find(|l| l.text == "J6")
            .expect("应有 J6 注记");
        assert_eq!(tip.reason, "inside_fallback");
        assert!(tip.overlap);
        // 兜底注记中心落在宗地内（仅作最后手段并显式标记）
        assert!(point_in_ring(
            (tip.rect.cx, tip.rect.cy),
            &boundary.exterior.points
        ));
        assert_eq!(report.overlap_count(), 1);
    }

    #[test]
    fn from_geometry_polygon_multipolygon() {
        // Polygon（带洞）：内环续编
        let poly = geojson::Value::Polygon(vec![
            vec![
                vec![0.0, 0.0],
                vec![10.0, 0.0],
                vec![10.0, 10.0],
                vec![0.0, 10.0],
                vec![0.0, 0.0],
            ],
            vec![
                vec![2.0, 2.0],
                vec![4.0, 2.0],
                vec![4.0, 4.0],
                vec![2.0, 4.0],
                vec![2.0, 2.0],
            ],
        ]);
        let boundary = ParcelBoundary::from_geometry(&poly).expect("Polygon 应解析成功");
        assert_eq!(boundary.exterior.role, RingRole::Exterior);
        assert_eq!(boundary.interiors.len(), 1);
        assert_eq!(boundary.interiors[0].role, RingRole::Interior);
        let points = generate_boundary_points(&boundary, "J");
        assert_eq!(points.len(), 8);
        assert_eq!(points[4].ring_index, 1);
        assert_eq!(points[4].label(), "J5");

        // MultiPolygon 取面积最大部件
        let multi = geojson::Value::MultiPolygon(vec![
            vec![vec![
                vec![0.0, 0.0],
                vec![1.0, 0.0],
                vec![1.0, 1.0],
                vec![0.0, 1.0],
                vec![0.0, 0.0],
            ]],
            vec![vec![
                vec![0.0, 0.0],
                vec![10.0, 0.0],
                vec![10.0, 10.0],
                vec![0.0, 10.0],
                vec![0.0, 0.0],
            ]],
        ]);
        let boundary = ParcelBoundary::from_geometry(&multi).expect("MultiPolygon 应解析成功");
        assert!((ring_area(&boundary.exterior).abs() - 100.0).abs() < 1e-9);

        // 非面几何 → 中文错误
        let point = geojson::Value::Point(vec![0.0, 0.0]);
        assert!(ParcelBoundary::from_geometry(&point).is_err());
    }

    #[test]
    fn report_pair_overlaps() {
        let mut report = PlacementReport::default();
        let a = LabelRect {
            cx: 0.0,
            cy: 0.0,
            w: 2.0,
            h: 2.0,
            rot_rad: 0.0,
        };
        let b = LabelRect {
            cx: 1.0,
            cy: 0.0,
            w: 2.0,
            h: 2.0,
            rot_rad: 0.0,
        };
        let c = LabelRect {
            cx: 10.0,
            cy: 0.0,
            w: 2.0,
            h: 2.0,
            rot_rad: 0.0,
        };
        for (text, rect, overlap) in [("a", a, false), ("b", b, true), ("c", c, false)] {
            report.labels.push(PlacedLabel {
                text: text.to_string(),
                rect,
                rotation_deg: 0.0,
                reason: "base".to_string(),
                clearance_mm: 0.0,
                overlap,
            });
        }
        assert_eq!(report.pair_overlaps(), 1);
        assert_eq!(report.overlap_count(), 1);
    }
    /// 两面要素集合（小矩形 10×10 + 大矩形 40×30，均带 parcel_* 属性）。
    fn two_parcels_collection() -> geojson::FeatureCollection {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,10],[0,0]]]},
                 "properties":{"parcel_id":"small","parcel_use":"0701","area":"100.0"}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[100,100],[140,100],[140,130],[100,130],[100,100]]]},
                 "properties":{"parcel_id":"big","parcel_use":"0801","area":1200.0,"owner":"张三"}}
            ]
        }"#
        .parse()
        .unwrap();
        geojson::FeatureCollection::try_from(gj).unwrap()
    }

    #[test]
    fn boundary_from_collection_picks_largest_or_indexed() {
        let collection = two_parcels_collection();
        // 缺省取面积最大者（big，40×30）。
        let (boundary, props) = boundary_from_collection(&collection, None).unwrap();
        assert_eq!(props["parcel_id"].as_str().unwrap(), "big");
        assert_eq!(boundary.exterior.points.len(), 5);
        // 显式序号取文档序第 0 个（small）。
        let (_b0, props0) = boundary_from_collection(&collection, Some(0)).unwrap();
        assert_eq!(props0["parcel_id"].as_str().unwrap(), "small");
        // 越界中文报错。
        let err = boundary_from_collection(&collection, Some(5)).unwrap_err();
        assert!(err.to_string().contains("越界"), "{err}");
        // 无面要素报错。
        let empty: geojson::GeoJson = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{}}]}"#
            .parse()
            .unwrap();
        let points_only = geojson::FeatureCollection::try_from(empty).unwrap();
        assert!(boundary_from_collection(&points_only, None).is_err());
    }

    #[test]
    fn feature_prop_helpers_pick_by_keys() {
        let (_b, props) = boundary_from_collection(&two_parcels_collection(), None).unwrap();
        // 字符串拾取：首命中键优先、缺键跳过。
        assert_eq!(
            feature_prop_str(&props, &["missing", "parcel_id"]).as_deref(),
            Some("big")
        );
        assert_eq!(
            feature_prop_str(&props, &["owner"]).as_deref(),
            Some("张三")
        );
        assert_eq!(feature_prop_str(&props, &["missing"]), None);
        // 数值拾取：Number 直取、String 解析。
        assert_eq!(feature_prop_f64(&props, &["area"]), Some(1200.0));
        let (_b0, props0) = boundary_from_collection(&two_parcels_collection(), Some(0)).unwrap();
        assert_eq!(feature_prop_f64(&props0, &["area"]), Some(100.0));
    }
}
