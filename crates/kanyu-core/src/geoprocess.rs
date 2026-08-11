//! QGIS 核心算法移植（转写正确性优先，语义对齐 QGIS Processing）：
//! dissolve（融合）/ centroid（质心）/ convex_hull（凸包）/ simplify（道格拉斯简化）/
//! delete_holes（删洞）/ explode（多部件炸开）/ stats（图层统计，含亩/公顷）。
//! 第二批：boundary（边界）/ bounding_boxes（包络矩形）/ merge（合并矢量图层）/
//! extract_by_attribute（按属性提取）/ extract_by_location（按位置提取）/
//! count_points_in_polygon（面内点计数）/ field_stats（字段基本统计）/
//! mean_coordinates（平均坐标）。
//! 第三批：distance_matrix（距离矩阵）/ nearest_neighbor（最近邻分析）/
//! multi_ring_buffer（多环缓冲区）/ variable_buffer（按字段缓冲区）/
//! split_by_field（分割矢量图层）/ add_geometry_attributes（添加几何属性）/
//! create_grid（创建网格）/ points_along_lines（沿线等距点）/
//! concave_hull（凹包）/ minimum_rotated_rect（定向最小包络矩形）。
//!
//! 语义契约（与 QGIS 对照，rustdoc 即规范）：
//! - **dissolve**：按字段分组（None = 全组），组内面要素布尔并集；
//!   属性取组字段值 + 组内首要素其余属性（QGIS keep-first 语义）。
//! - **centroid**：几何质心（geo Centroid；可能落在凹面外，与 QGIS 默认一致）；
//!   空几何跳过并计数。
//! - **simplify**：Douglas-Peucker（geo Simplify），容差为 CRS 单位；
//!   简化后退化要素（面 <4 点）剔除并计数。
//! - **delete_holes**：`min_area=None` 删除全部洞；否则仅删面积 < min_area 的洞。
//! - **explode**：Multi* → 单部件逐要素（属性复制），GeometryCollection 展平。
//! - **stats**：测地线口径（Karney 2013）；亩 = 10000/15 ㎡，公顷 = 10000 ㎡。
//! - **boundary**：面 → 全部环转线（单环 LineString，多环/多面 MultiLineString）；
//!   开放线 → 首尾端点 MultiPoint；闭合线与点无边界，跳过。
//! - **extract_by_attribute**：表达式 "field op value"，op ∈ =/==/!=/>/>=/</<=/contains；
//!   数值比较优先、失败退字符串比较，语义与 `Layer::query` 对齐。
//! - **extract_by_location**：谓词 intersects/contains/within（DE-9IM，geo Relate）；
//!   与 mask 任一要素满足谓词即提取。
//! - **count_points_in_polygon**：追加 `NUMPOINTS` 整数属性；点在面上含边界
//!   （geo Covers），MultiPoint 按子点逐个计数。
//! - **field_stats**：stddev 为总体标准差；缺失/Null/非数值计入 null_count。

use geo::algorithm::line_measures::{Geodesic, Length as _};
use geo::{BooleanOps, Centroid, ConvexHull, Simplify};

use crate::error::{KanyuError, Result};

/// geo 几何 → geojson 几何 Feature（属性随行复制）。
fn with_geometry(feature: &geojson::Feature, value: geojson::Value) -> geojson::Feature {
    geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(value)),
        id: None,
        properties: feature.properties.clone(),
        foreign_members: None,
    }
}

/// geojson::Value → geo_types::Geometry（不可转换返回 None）。
fn to_geo(value: &geojson::Value) -> Option<geo_types::Geometry<f64>> {
    geo_types::Geometry::<f64>::try_from(value).ok()
}

fn empty_collection() -> geojson::FeatureCollection {
    geojson::FeatureCollection {
        bbox: None,
        features: Vec::new(),
        foreign_members: None,
    }
}

/// 融合（QGIS Dissolve）：按字段分组并集。
pub fn dissolve(
    collection: &geojson::FeatureCollection,
    field: Option<&str>,
) -> Result<geojson::FeatureCollection> {
    use std::collections::BTreeMap;
    // 分组键：字段值字符串化；缺字段统一空串（QGIS 语义）。
    let mut groups: BTreeMap<String, Vec<&geojson::Feature>> = BTreeMap::new();
    for feature in &collection.features {
        let key = match field {
            Some(f) => feature
                .properties
                .as_ref()
                .and_then(|p| p.get(f))
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default(),
            None => String::new(),
        };
        groups.entry(key).or_default().push(feature);
    }

    let mut out = empty_collection();
    for (key, members) in groups {
        // 面并集 / 线合并 MultiLineString / 点合并 MultiPoint（QGIS 分类型语义）。
        let mut polygons: Vec<geo_types::MultiPolygon<f64>> = Vec::new();
        let mut lines: Vec<Vec<Vec<f64>>> = Vec::new();
        let mut points: Vec<Vec<f64>> = Vec::new();
        for feature in &members {
            match feature.geometry.as_ref().map(|g| &g.value) {
                Some(geojson::Value::Polygon(_)) | Some(geojson::Value::MultiPolygon(_)) => {
                    if let Some(geo_types::Geometry::Polygon(p)) =
                        feature.geometry.as_ref().and_then(|g| to_geo(&g.value))
                    {
                        polygons.push(geo_types::MultiPolygon(vec![p]));
                    } else if let Some(geo_types::Geometry::MultiPolygon(mp)) =
                        feature.geometry.as_ref().and_then(|g| to_geo(&g.value))
                    {
                        polygons.push(mp);
                    }
                }
                Some(geojson::Value::LineString(ls)) => lines.push(ls.clone()),
                Some(geojson::Value::MultiLineString(mls)) => lines.extend(mls.clone()),
                Some(geojson::Value::Point(pt)) => points.push(pt.clone()),
                Some(geojson::Value::MultiPoint(mpt)) => points.extend(mpt.clone()),
                _ => {}
            }
        }
        let value = if !polygons.is_empty() {
            let mut acc = polygons[0].clone();
            for p in &polygons[1..] {
                acc = acc.union(p);
            }
            geojson::Value::from(&acc)
        } else if !lines.is_empty() {
            geojson::Value::MultiLineString(lines)
        } else if !points.is_empty() {
            geojson::Value::MultiPoint(points)
        } else {
            continue; // 空组（无可融合几何）跳过
        };
        // 属性：组字段值 + 首要素其余属性（QGIS keep-first）。
        let mut props = members[0].properties.clone().unwrap_or_default();
        if let Some(f) = field {
            props.insert(f.to_string(), serde_json::Value::String(key));
        }
        out.features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(value)),
            id: None,
            properties: Some(props),
            foreign_members: None,
        });
    }
    Ok(out)
}

/// 质心（QGIS Centroids）：逐要素质心点，属性随行；空几何跳过。
pub fn centroid(collection: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection> {
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        if let Some(c) = geom.centroid() {
            out.features.push(with_geometry(
                feature,
                geojson::Value::Point(vec![c.x(), c.y()]),
            ));
        }
    }
    Ok(out)
}

/// 凸包（QGIS Convex hull）：逐要素凸包面。
pub fn convex_hull(collection: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection> {
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let hull = geom.convex_hull();
        out.features
            .push(with_geometry(feature, geojson::Value::from(&hull)));
    }
    Ok(out)
}

/// 道格拉斯简化（QGIS Simplify）：tolerance 为 CRS 单位；退化要素剔除。
pub fn simplify(
    collection: &geojson::FeatureCollection,
    tolerance: f64,
) -> Result<geojson::FeatureCollection> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(KanyuError::Other(format!(
            "简化容差须为非负数值: {tolerance}"
        )));
    }
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(value) = feature.geometry.as_ref().map(|g| &g.value) else {
            continue;
        };
        let simplified: geojson::Value = match value {
            geojson::Value::Point(_) | geojson::Value::MultiPoint(_) => value.clone(),
            geojson::Value::GeometryCollection(_) => value.clone(), // 集合不简化（v1）
            _ => {
                let Some(geom) = to_geo(value) else {
                    continue;
                };
                match geom {
                    geo_types::Geometry::LineString(g) => {
                        geojson::Value::from(&g.simplify(tolerance))
                    }
                    geo_types::Geometry::MultiLineString(g) => {
                        geojson::Value::from(&g.simplify(tolerance))
                    }
                    geo_types::Geometry::Polygon(g) => {
                        let s = g.simplify(tolerance);
                        if s.exterior().0.len() < 4 {
                            continue; // 简化退化：剔除
                        }
                        geojson::Value::from(&s)
                    }
                    geo_types::Geometry::MultiPolygon(g) => {
                        geojson::Value::from(&g.simplify(tolerance))
                    }
                    _ => value.clone(),
                }
            }
        };
        out.features.push(with_geometry(feature, simplified));
    }
    Ok(out)
}

/// 删洞（QGIS Delete holes）：min_area=None 删全部洞；否则仅删 < min_area 的洞
/// （面积按 CRS 平面单位）。
pub fn delete_holes(
    collection: &geojson::FeatureCollection,
    min_area: Option<f64>,
) -> Result<geojson::FeatureCollection> {
    let keep = |ring: &[Vec<f64>]| -> bool {
        match min_area {
            None => false, // 全删
            Some(threshold) => {
                let mut total = 0.0;
                let n = ring.len();
                let m = if n > 1 && ring.first() == ring.last() {
                    n - 1
                } else {
                    n
                };
                for i in 0..m {
                    let (x1, y1) = (ring[i][0], ring[i][1]);
                    let (x2, y2) = (ring[(i + 1) % m][0], ring[(i + 1) % m][1]);
                    total += x1 * y2 - x2 * y1;
                }
                total.abs() / 2.0 >= threshold
            }
        }
    };
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(value) = feature.geometry.as_ref().map(|g| &g.value) else {
            continue;
        };
        let new_value = match value {
            geojson::Value::Polygon(rings) => {
                let mut kept: Vec<Vec<Vec<f64>>> = Vec::new();
                if let Some(exterior) = rings.first() {
                    kept.push(exterior.clone());
                }
                kept.extend(rings.iter().skip(1).filter(|r| keep(r)).cloned());
                geojson::Value::Polygon(kept)
            }
            geojson::Value::MultiPolygon(polys) => {
                let new_polys: Vec<Vec<Vec<Vec<f64>>>> = polys
                    .iter()
                    .map(|rings| {
                        let mut kept: Vec<Vec<Vec<f64>>> = Vec::new();
                        if let Some(exterior) = rings.first() {
                            kept.push(exterior.clone());
                        }
                        kept.extend(rings.iter().skip(1).filter(|r| keep(r)).cloned());
                        kept
                    })
                    .collect();
                geojson::Value::MultiPolygon(new_polys)
            }
            _ => value.clone(),
        };
        out.features.push(with_geometry(feature, new_value));
    }
    Ok(out)
}

/// 多部件炸开（QGIS Multipart to singleparts）：Multi* → 单部件逐要素，
/// GeometryCollection 展平；属性复制。
pub fn explode(collection: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection> {
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(value) = feature.geometry.as_ref().map(|g| &g.value) else {
            continue;
        };
        let parts: Vec<geojson::Value> = match value {
            geojson::Value::MultiPoint(pts) => pts
                .iter()
                .map(|p| geojson::Value::Point(p.clone()))
                .collect(),
            geojson::Value::MultiLineString(mls) => mls
                .iter()
                .map(|ls| geojson::Value::LineString(ls.clone()))
                .collect(),
            geojson::Value::MultiPolygon(polys) => polys
                .iter()
                .map(|p| geojson::Value::Polygon(p.clone()))
                .collect(),
            geojson::Value::GeometryCollection(geoms) => {
                geoms.iter().map(|g| g.value.clone()).collect()
            }
            single => vec![single.clone()],
        };
        for part in parts {
            out.features.push(with_geometry(feature, part));
        }
    }
    Ok(out)
}

/// 图层统计（选择集统计移植：测地线口径 + 亩/公顷单位）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayerStats {
    /// 要素总数。
    pub feature_count: usize,
    /// 点要素数。
    pub points: usize,
    /// 线要素数。
    pub lines: usize,
    /// 面要素数。
    pub polygons: usize,
    /// 其他几何数（GeometryCollection 等）。
    pub other: usize,
    /// 总长度（米）。
    pub total_length_m: f64,
    /// 总长度（千米）。
    pub total_length_km: f64,
    /// 总面积（平方米）。
    pub total_area_m2: f64,
    /// 总周长（米）。
    pub total_perimeter_m: f64,
    /// 总面积（公顷）。
    pub total_area_hectare: f64,
    /// 总面积（亩，1 亩 = 10000/15 ㎡）。
    pub total_area_mu: f64,
    /// 总面积（平方千米）。
    pub total_area_km2: f64,
}

/// 图层统计：逐要素测地线度量（Karney 2013；面 = 面积 + 周长，线 = 长度，
/// 点不计；绕向经 Orient 归一化）。
pub fn stats(collection: &geojson::FeatureCollection) -> Result<LayerStats> {
    let mut s = LayerStats {
        feature_count: collection.features.len(),
        points: 0,
        lines: 0,
        polygons: 0,
        other: 0,
        total_length_m: 0.0,
        total_length_km: 0.0,
        total_area_m2: 0.0,
        total_perimeter_m: 0.0,
        total_area_hectare: 0.0,
        total_area_mu: 0.0,
        total_area_km2: 0.0,
    };
    for feature in &collection.features {
        let Some(value) = feature.geometry.as_ref().map(|g| &g.value) else {
            continue;
        };
        match value {
            geojson::Value::Point(_) | geojson::Value::MultiPoint(_) => s.points += 1,
            geojson::Value::LineString(_) | geojson::Value::MultiLineString(_) => {
                s.lines += 1;
                match to_geo(value) {
                    Some(geo_types::Geometry::LineString(l)) => {
                        s.total_length_m += Geodesic.length(&l)
                    }
                    Some(geo_types::Geometry::MultiLineString(m)) => {
                        s.total_length_m += Geodesic.length(&m)
                    }
                    _ => {}
                }
            }
            geojson::Value::Polygon(_) | geojson::Value::MultiPolygon(_) => {
                s.polygons += 1;
                // 绕向归一化（防 ESRI 顺时针数据的补集面积，见 crs.rs）。
                use geo::algorithm::orient::{Direction, Orient};
                use geo::algorithm::GeodesicArea as _;
                match to_geo(value) {
                    Some(geo_types::Geometry::Polygon(p)) => {
                        let o = p.orient(Direction::Default);
                        s.total_area_m2 += o.geodesic_area_unsigned();
                        s.total_perimeter_m += Geodesic.length(o.exterior());
                    }
                    Some(geo_types::Geometry::MultiPolygon(mp)) => {
                        let o = mp.orient(Direction::Default);
                        s.total_area_m2 += o.geodesic_area_unsigned();
                        s.total_perimeter_m +=
                            o.iter().map(|p| Geodesic.length(p.exterior())).sum::<f64>();
                    }
                    _ => {}
                }
            }
            _ => s.other += 1,
        }
    }
    s.total_length_km = s.total_length_m / 1000.0;
    s.total_area_hectare = s.total_area_m2 / 10000.0;
    s.total_area_mu = s.total_area_m2 / (10000.0 / 15.0);
    s.total_area_km2 = s.total_area_m2 / 1_000_000.0;
    Ok(s)
}

/// 边界（QGIS Boundary）：
/// - 面 → 全部环转线（外环 + 内洞）：单环面输出 LineString，多环面/多面输出
///   MultiLineString；
/// - 开放线 → 首尾端点 MultiPoint；闭合线（首末点相同）无边界，跳过；
/// - 点 / 几何集合无边界，跳过；空几何跳过。属性随行复制。
pub fn boundary(collection: &geojson::FeatureCollection) -> Result<geojson::FeatureCollection> {
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(value) = feature.geometry.as_ref().map(|g| &g.value) else {
            continue;
        };
        let new_value = match value {
            geojson::Value::Polygon(rings) => match rings.len() {
                0 => continue,
                1 => geojson::Value::LineString(rings[0].clone()),
                _ => geojson::Value::MultiLineString(rings.clone()),
            },
            geojson::Value::MultiPolygon(polys) => {
                let lines: Vec<Vec<Vec<f64>>> = polys.iter().flatten().cloned().collect();
                if lines.is_empty() {
                    continue;
                }
                geojson::Value::MultiLineString(lines)
            }
            geojson::Value::LineString(ls) => {
                let Some((first, last)) = line_endpoints(ls) else {
                    continue;
                };
                geojson::Value::MultiPoint(vec![first, last])
            }
            geojson::Value::MultiLineString(mls) => {
                let mut pts: Vec<Vec<f64>> = Vec::new();
                for ls in mls {
                    if let Some((first, last)) = line_endpoints(ls) {
                        pts.push(first);
                        pts.push(last);
                    }
                }
                if pts.is_empty() {
                    continue;
                }
                geojson::Value::MultiPoint(pts)
            }
            _ => continue, // 点/几何集合：无边界
        };
        out.features.push(with_geometry(feature, new_value));
    }
    Ok(out)
}

/// 开放线的首尾端点；闭合线（首末点相同）或不足 2 点返回 None。
fn line_endpoints(ls: &[Vec<f64>]) -> Option<(Vec<f64>, Vec<f64>)> {
    if ls.len() < 2 {
        return None;
    }
    let first = ls.first()?.clone();
    let last = ls.last()?.clone();
    if first == last {
        None
    } else {
        Some((first, last))
    }
}

/// 包络矩形（QGIS Bounding boxes）：逐要素最小外接矩形面要素，属性随行；
/// 无几何或空坐标要素跳过。
pub fn bounding_boxes(
    collection: &geojson::FeatureCollection,
) -> Result<geojson::FeatureCollection> {
    use geo::BoundingRect;
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let Some(rect) = geom.bounding_rect() else {
            continue;
        };
        out.features.push(with_geometry(
            feature,
            geojson::Value::from(&rect.to_polygon()),
        ));
    }
    Ok(out)
}

/// 合并矢量图层（QGIS Merge vector layers）：多图层要素按输入顺序拼接为一层；
/// 不做几何类型/字段一致性校验，属性原样保留；空输入得空图层。
pub fn merge(collections: &[&geojson::FeatureCollection]) -> Result<geojson::FeatureCollection> {
    let mut out = empty_collection();
    for c in collections {
        out.features.extend(c.features.iter().cloned());
    }
    Ok(out)
}

/// 属性比较运算符（与 `Layer::query` 语义对齐，另加 contains）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
}

/// 编译后的属性谓词（"field op value"）。
struct AttrPredicate {
    field: String,
    op: AttrOp,
    value: serde_json::Value,
}

impl AttrPredicate {
    /// 解析 "field op value"：op ∈ =/==/!=/>/>=/</<=/contains；
    /// 右值数值优先，其次布尔，退化为字符串（同 `Layer::query`）。
    fn parse(expression: &str) -> Result<Self> {
        const OPS: [(&str, AttrOp); 7] = [
            (">=", AttrOp::Ge),
            ("<=", AttrOp::Le),
            ("!=", AttrOp::Ne),
            ("==", AttrOp::Eq),
            (">", AttrOp::Gt),
            ("<", AttrOp::Lt),
            ("=", AttrOp::Eq),
        ];
        for (token, op) in OPS {
            if let Some((lhs, rhs)) = expression.split_once(token) {
                let field = lhs.trim().to_string();
                let raw = rhs.trim().trim_matches('"').trim_matches('\'');
                if field.is_empty() || raw.is_empty() {
                    break;
                }
                let value = raw
                    .parse::<f64>()
                    .map(serde_json::Value::from)
                    .or_else(|_| raw.parse::<bool>().map(serde_json::Value::from))
                    .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
                return Ok(Self { field, op, value });
            }
        }
        // contains：词法运算符 "field contains value"（值不含空白）。
        let parts: Vec<&str> = expression.split_whitespace().collect();
        if let [field, "contains", raw] = parts.as_slice() {
            return Ok(Self {
                field: field.to_string(),
                op: AttrOp::Contains,
                value: serde_json::Value::String(raw.to_string()),
            });
        }
        Err(KanyuError::InvalidQuery(expression.to_string()))
    }

    /// 对单个属性 JSON 值求值（字段缺失/空值由调用方短路）。
    /// 双数值按 f64 数值比较，其余按 to_string 字典序（Eq/Ne 用 JSON 值相等），
    /// 与 `Layer::query` 一致；contains 为子串包含（字符串取其本体）。
    fn matches_value(&self, actual: &serde_json::Value) -> bool {
        if self.op == AttrOp::Contains {
            let hay = match actual {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let needle = match &self.value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            return hay.contains(&needle);
        }
        match (actual, &self.value) {
            (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                let (a, b) = (
                    a.as_f64().unwrap_or(f64::NAN),
                    b.as_f64().unwrap_or(f64::NAN),
                );
                match self.op {
                    AttrOp::Eq => a == b,
                    AttrOp::Ne => a != b,
                    AttrOp::Gt => a > b,
                    AttrOp::Ge => a >= b,
                    AttrOp::Lt => a < b,
                    AttrOp::Le => a <= b,
                    AttrOp::Contains => unreachable!(),
                }
            }
            _ => {
                let ord = actual.to_string().cmp(&self.value.to_string());
                match self.op {
                    AttrOp::Eq => actual == &self.value,
                    AttrOp::Ne => actual != &self.value,
                    AttrOp::Gt => ord.is_gt(),
                    AttrOp::Ge => ord.is_gt() || ord.is_eq(),
                    AttrOp::Lt => ord.is_lt(),
                    AttrOp::Le => ord.is_lt() || ord.is_eq(),
                    AttrOp::Contains => unreachable!(),
                }
            }
        }
    }
}

/// 按属性提取（QGIS Extract by attribute）：表达式 "field op value"
/// （op ∈ =/==/!=/>/>=/</<=/contains；数值比较优先、失败退字符串比较，
/// 语义与 `Layer::query` 对齐）。字段缺失/Null 的要素不匹配，属性随行；
/// 表达式非法返回 `KanyuError::InvalidQuery`。
pub fn extract_by_attribute(
    collection: &geojson::FeatureCollection,
    expression: &str,
) -> Result<geojson::FeatureCollection> {
    let predicate = AttrPredicate::parse(expression)?;
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(actual) = feature
            .properties
            .as_ref()
            .and_then(|p| p.get(&predicate.field))
        else {
            continue;
        };
        if actual.is_null() {
            continue;
        }
        if predicate.matches_value(actual) {
            out.features.push(feature.clone());
        }
    }
    Ok(out)
}

/// 空间谓词（QGIS Extract by location 谓词子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpatialPredicate {
    Intersects,
    Contains,
    Within,
}

impl SpatialPredicate {
    fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "intersects" => Ok(Self::Intersects),
            "contains" => Ok(Self::Contains),
            "within" => Ok(Self::Within),
            _ => Err(KanyuError::InvalidQuery(format!(
                "未知空间谓词: '{s}'（支持 intersects/contains/within）"
            ))),
        }
    }
}

/// 按位置提取（QGIS Extract by location）：凡与 mask 任一要素满足谓词的要素
/// 被提取，属性随行；无几何要素跳过。拓扑判定走 DE-9IM（geo Relate）；
/// contains/within 为严格内含（不含边界，如边界命中请用 intersects）；
/// 非法谓词返回 `KanyuError::InvalidQuery`。
pub fn extract_by_location(
    collection: &geojson::FeatureCollection,
    mask: &geojson::FeatureCollection,
    predicate: &str,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::relate::Relate;
    let pred = SpatialPredicate::parse(predicate)?;
    let mask_geoms: Vec<geo_types::Geometry<f64>> = mask
        .features
        .iter()
        .filter_map(|f| f.geometry.as_ref().and_then(|g| to_geo(&g.value)))
        .collect();
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let hit = mask_geoms.iter().any(|m| {
            let im = geom.relate(m);
            match pred {
                SpatialPredicate::Intersects => im.is_intersects(),
                SpatialPredicate::Contains => im.is_contains(),
                SpatialPredicate::Within => im.is_within(),
            }
        });
        if hit {
            out.features.push(feature.clone());
        }
    }
    Ok(out)
}

/// 面内点计数（QGIS Count points in polygon）：输出面图层要素追加 `NUMPOINTS`
/// 整数属性，统计落入面内的点数（点在面上含边界，geo Covers 语义；
/// MultiPoint 按子点逐个计数）。非面/空几何要素跳过。
pub fn count_points_in_polygon(
    polygons: &geojson::FeatureCollection,
    points: &geojson::FeatureCollection,
) -> Result<geojson::FeatureCollection> {
    use geo::Covers;
    let point_geoms: Vec<geo_types::Geometry<f64>> = points
        .features
        .iter()
        .filter_map(|f| f.geometry.as_ref().and_then(|g| to_geo(&g.value)))
        .collect();
    let mut out = empty_collection();
    for feature in &polygons.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        if !matches!(
            geom,
            geo_types::Geometry::Polygon(_) | geo_types::Geometry::MultiPolygon(_)
        ) {
            continue;
        }
        let mut count: u64 = 0;
        for pg in &point_geoms {
            match pg {
                geo_types::Geometry::Point(_) => {
                    if geom.covers(pg) {
                        count += 1;
                    }
                }
                geo_types::Geometry::MultiPoint(mp) => {
                    count += mp.iter().filter(|p| geom.covers(*p)).count() as u64;
                }
                _ => {}
            }
        }
        let mut f = feature.clone();
        f.properties
            .get_or_insert_with(Default::default)
            .insert("NUMPOINTS".to_string(), serde_json::Value::from(count));
        out.features.push(f);
    }
    Ok(out)
}

/// 字段基本统计结果（QGIS Basic statistics for fields 数值字段子集）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldStats {
    /// 有效数值个数。
    pub count: usize,
    /// 空值个数（字段缺失/Null/非数值）。
    pub null_count: usize,
    /// 最小值。
    pub min: f64,
    /// 最大值。
    pub max: f64,
    /// 总和。
    pub sum: f64,
    /// 算术平均。
    pub mean: f64,
    /// 极差（max - min）。
    pub range: f64,
    /// 总体标准差（除以 n 而非 n-1）。
    pub stddev: f64,
}

/// 字段基本统计（QGIS Basic statistics for fields）：数值字段描述统计，
/// stddev 为总体标准差。字段缺失/Null/非数值的要素计入 null_count；
/// 字段不存在或全空（无有效数值）返回 `KanyuError::InvalidQuery`。
pub fn field_stats(collection: &geojson::FeatureCollection, field: &str) -> Result<FieldStats> {
    let mut values: Vec<f64> = Vec::new();
    let mut null_count = 0usize;
    for feature in &collection.features {
        match feature.properties.as_ref().and_then(|p| p.get(field)) {
            Some(serde_json::Value::Number(n)) => match n.as_f64() {
                Some(v) => values.push(v),
                None => null_count += 1,
            },
            _ => null_count += 1,
        }
    }
    if values.is_empty() {
        return Err(KanyuError::InvalidQuery(format!(
            "字段 '{field}' 不存在或无有效数值"
        )));
    }
    let count = values.len();
    let sum: f64 = values.iter().sum();
    let mean = sum / count as f64;
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // 总体标准差：sqrt(Σ(x - mean)² / n)。
    let stddev = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64).sqrt();
    Ok(FieldStats {
        count,
        null_count,
        min,
        max,
        sum,
        mean,
        range: max - min,
        stddev,
    })
}

/// 平均坐标（QGIS Mean coordinate(s)）：全部要素几何质心的（可加权）平均，
/// 输出单点要素，属性含 MEAN_X/MEAN_Y。weight_field 给定但缺失/非数值的要素
/// 权重按 0 计；空几何跳过。无有效要素或权重总和为 0 返回
/// `KanyuError::Other`。
pub fn mean_coordinates(
    collection: &geojson::FeatureCollection,
    weight_field: Option<&str>,
) -> Result<geojson::FeatureCollection> {
    let (mut sx, mut sy, mut sw) = (0.0, 0.0, 0.0);
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let Some(c) = geom.centroid() else {
            continue;
        };
        let w = match weight_field {
            Some(field) => feature
                .properties
                .as_ref()
                .and_then(|p| p.get(field))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
            None => 1.0,
        };
        sx += c.x() * w;
        sy += c.y() * w;
        sw += w;
    }
    if sw == 0.0 {
        return Err(KanyuError::Other(
            "mean_coordinates: 无有效要素或权重总和为 0".to_string(),
        ));
    }
    let mut props = serde_json::Map::new();
    props.insert("MEAN_X".to_string(), serde_json::Value::from(sx / sw));
    props.insert("MEAN_Y".to_string(), serde_json::Value::from(sy / sw));
    let mut out = empty_collection();
    out.features.push(geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![
            sx / sw,
            sy / sw,
        ]))),
        id: None,
        properties: Some(props),
        foreign_members: None,
    });
    Ok(out)
}

/// 要素代表点：点取坐标，其余几何取质心（QGIS 距离矩阵/最近邻分析口径）。
fn representative_point(feature: &geojson::Feature) -> Option<geo_types::Point<f64>> {
    let geom = feature.geometry.as_ref().and_then(|g| to_geo(&g.value))?;
    match geom {
        geo_types::Geometry::Point(p) => Some(p),
        other => other.centroid(),
    }
}

/// 要素标签：要素 id 字符串化，缺 id 取要素序号（QGIS 距离矩阵行/列名口径）。
fn feature_label(feature: &geojson::Feature, index: usize) -> String {
    match &feature.id {
        Some(geojson::feature::Id::String(s)) => s.clone(),
        Some(geojson::feature::Id::Number(n)) => n.to_string(),
        None => index.to_string(),
    }
}

/// 距离矩阵结果（QGIS Distance matrix）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DistanceMatrix {
    /// 行标签（a 图层要素 id 或序号）。
    pub rows: Vec<String>,
    /// 列标签（b 图层要素 id 或序号）。
    pub cols: Vec<String>,
    /// 逐对测地线距离（米），rows.len() × cols.len()。
    pub matrix: Vec<Vec<f64>>,
    /// 最小距离（米）。
    pub min: f64,
    /// 最大距离（米）。
    pub max: f64,
    /// 平均距离（米，全部 n×m 对）。
    pub mean: f64,
}

/// 距离矩阵（QGIS Distance matrix）：两图层要素代表点（点取坐标、非点取质心）
/// 逐对测地线距离（Karney 2013，米）。行/列标签取要素 id（字符串化），缺 id
/// 取要素序号。无几何/空质心要素不计入；任一图层无有效要素返回
/// `KanyuError::Other`。
pub fn distance_matrix(
    a: &geojson::FeatureCollection,
    b: &geojson::FeatureCollection,
) -> Result<DistanceMatrix> {
    use geo::algorithm::line_measures::Distance as _;
    let points_of = |c: &geojson::FeatureCollection| -> Vec<(String, geo_types::Point<f64>)> {
        c.features
            .iter()
            .enumerate()
            .filter_map(|(i, f)| representative_point(f).map(|p| (feature_label(f, i), p)))
            .collect()
    };
    let rows = points_of(a);
    let cols = points_of(b);
    if rows.is_empty() || cols.is_empty() {
        return Err(KanyuError::Other(
            "distance_matrix: 输入图层无有效要素（点取坐标、非点取质心）".to_string(),
        ));
    }
    let (mut min, mut max, mut sum) = (f64::INFINITY, f64::NEG_INFINITY, 0.0);
    let mut matrix = Vec::with_capacity(rows.len());
    for (_, pa) in &rows {
        let mut row = Vec::with_capacity(cols.len());
        for (_, pb) in &cols {
            let d = Geodesic.distance(*pa, *pb);
            min = min.min(d);
            max = max.max(d);
            sum += d;
            row.push(d);
        }
        matrix.push(row);
    }
    let count = (rows.len() * cols.len()) as f64;
    Ok(DistanceMatrix {
        rows: rows.into_iter().map(|(label, _)| label).collect(),
        cols: cols.into_iter().map(|(label, _)| label).collect(),
        matrix,
        min,
        max,
        mean: sum / count,
    })
}

/// 最近邻分析结果（QGIS Nearest neighbour analysis）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct NearestNeighborReport {
    /// 参与统计的点数（点取坐标、非点取质心）。
    pub count: usize,
    /// 观测平均最近邻距离（米）。
    pub observed_mean_distance: f64,
    /// 随机分布期望平均距离（米，0.5/sqrt(n/A)，A 为点集外包矩形测地面积 ㎡）。
    pub expected_mean_distance: f64,
    /// 最近邻指数（observed/expected；<1 聚集、≈1 随机、>1 离散）。
    pub nearest_neighbor_index: f64,
    /// 最小最近邻距离（米）。
    pub min: f64,
    /// 最大最近邻距离（米）。
    pub max: f64,
}

/// 最近邻分析（QGIS Nearest neighbour analysis）：点图层（非点取质心）的
/// 最近邻统计。测地线口径（Karney 2013，米）；A 取点集外包矩形的测地面积。
/// 有效点数 < 2 或外包矩形面积为零返回 `KanyuError::Other`。
pub fn nearest_neighbor(collection: &geojson::FeatureCollection) -> Result<NearestNeighborReport> {
    use geo::algorithm::line_measures::Distance as _;
    let pts: Vec<geo_types::Point<f64>> = collection
        .features
        .iter()
        .filter_map(representative_point)
        .collect();
    let n = pts.len();
    if n < 2 {
        return Err(KanyuError::Other(format!(
            "nearest_neighbor: 有效点数须 >= 2，实际 {n}"
        )));
    }
    let mut nearest = Vec::with_capacity(n);
    for (i, p) in pts.iter().enumerate() {
        let mut dmin = f64::INFINITY;
        for (j, q) in pts.iter().enumerate() {
            if i != j {
                dmin = dmin.min(Geodesic.distance(*p, *q));
            }
        }
        nearest.push(dmin);
    }
    use geo::algorithm::orient::{Direction, Orient};
    use geo::algorithm::GeodesicArea as _;
    use geo::BoundingRect;
    let rect = geo_types::MultiPoint(pts).bounding_rect();
    let Some(rect) = rect else {
        return Err(KanyuError::Other(
            "nearest_neighbor: 点集无有效外包矩形".to_string(),
        ));
    };
    let area = rect
        .to_polygon()
        .orient(Direction::Default)
        .geodesic_area_unsigned();
    if area <= 0.0 || area.is_nan() {
        return Err(KanyuError::Other(
            "nearest_neighbor: 点集外包矩形面积为零，无法计算随机期望".to_string(),
        ));
    }
    let observed = nearest.iter().sum::<f64>() / n as f64;
    let expected = 0.5 / (n as f64 / area).sqrt();
    Ok(NearestNeighborReport {
        count: n,
        observed_mean_distance: observed,
        expected_mean_distance: expected,
        nearest_neighbor_index: observed / expected,
        min: nearest.iter().cloned().fold(f64::INFINITY, f64::min),
        max: nearest.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    })
}

/// 多环缓冲区（QGIS Multi-ring buffer）：每个距离一档，输出该距离缓冲区与
/// 上一档缓冲区的差集环（第一档为完整缓冲区）；属性随行复制并追加
/// `RING`（环序号，1 起）与 `DISTANCE`（本档外缘距离）。distances 须非空、
/// 严格递增且非负有限，否则返回 `KanyuError::Other`；距离为 CRS 单位
/// （geo Buffer 平面口径，同 [`crate::analysis::buffer`]`）；差集为空的环跳过。
pub fn multi_ring_buffer(
    collection: &geojson::FeatureCollection,
    distances: &[f64],
) -> Result<geojson::FeatureCollection> {
    use geo::Buffer as _;
    if distances.is_empty() {
        return Err(KanyuError::Other(
            "multi_ring_buffer: distances 不能为空".to_string(),
        ));
    }
    for (i, &d) in distances.iter().enumerate() {
        if !d.is_finite() || d < 0.0 {
            return Err(KanyuError::Other(format!(
                "缓冲距离须为非负有限数值: distances[{i}] = {d}"
            )));
        }
        if i > 0 && d <= distances[i - 1] {
            return Err(KanyuError::Other(format!(
                "缓冲距离须严格递增: distances[{}] = {} 不大于 distances[{}] = {}",
                i,
                d,
                i - 1,
                distances[i - 1]
            )));
        }
    }
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let mut prev: Option<geo_types::MultiPolygon<f64>> = None;
        for (i, &d) in distances.iter().enumerate() {
            let buf = geom.buffer(d);
            let ring = match &prev {
                None => buf.clone(),
                Some(p) => buf.difference(p),
            };
            prev = Some(buf);
            if ring.0.is_empty() {
                continue; // 差集为空（如 0 距离于点）：跳过该环
            }
            let mut f = with_geometry(feature, geojson::Value::from(&ring));
            let props = f.properties.get_or_insert_with(Default::default);
            props.insert("RING".to_string(), serde_json::Value::from(i + 1));
            props.insert("DISTANCE".to_string(), serde_json::Value::from(d));
            out.features.push(f);
        }
    }
    Ok(out)
}

/// 按字段缓冲区（QGIS Variable distance buffer / Buffer by field）：缓冲距离
/// 取自数值字段；字段缺失/Null/非数值/非有限/负值的要素跳过并计数（计入
/// 返回集合 `foreign_members.skipped`，无跳过时缺省，同 [`crate::analysis::buffer`]`）。
/// `segments` 为圆弧拟合每象限分段数（>= 1）；距离为 CRS 单位（平面口径）。
pub fn variable_buffer(
    collection: &geojson::FeatureCollection,
    field: &str,
    segments: u32,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::buffer::{Buffer, BufferStyle, LineCap, LineJoin};
    if segments == 0 {
        return Err(KanyuError::Other("segments 必须 >= 1".to_string()));
    }
    let angle = std::f64::consts::FRAC_PI_2 / segments as f64;
    let mut features = Vec::new();
    let mut skipped = 0usize;
    for feature in &collection.features {
        let distance = feature
            .properties
            .as_ref()
            .and_then(|p| p.get(field))
            .and_then(|v| v.as_f64())
            .filter(|d| d.is_finite() && *d >= 0.0);
        let buffered = match distance {
            Some(d) => {
                let style = BufferStyle::new(d)
                    .line_join(LineJoin::Round(angle))
                    .line_cap(LineCap::Round(angle));
                feature
                    .geometry
                    .as_ref()
                    .and_then(|g| to_geo(&g.value))
                    .map(|g| g.buffer_with_style(style))
            }
            None => None,
        };
        match buffered {
            Some(mp) => features.push(with_geometry(feature, geojson::Value::from(&mp))),
            None => skipped += 1,
        }
    }
    let foreign_members = (skipped > 0).then(|| {
        let mut m = serde_json::Map::new();
        m.insert("skipped".to_string(), serde_json::Value::from(skipped));
        m
    });
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members,
    })
}

/// 分割矢量图层（QGIS Split vector layer）：按字段值分组输出，值字符串化
/// （字符串取本体，其余 JSON 值 to_string），字段缺失归空串组；
/// 分组按 BTreeMap 字典序，组内保持要素原顺序，要素原样克隆。
pub fn split_by_field(
    collection: &geojson::FeatureCollection,
    field: &str,
) -> Result<Vec<(String, geojson::FeatureCollection)>> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<geojson::Feature>> = BTreeMap::new();
    for feature in &collection.features {
        let key = feature
            .properties
            .as_ref()
            .and_then(|p| p.get(field))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        groups.entry(key).or_default().push(feature.clone());
    }
    Ok(groups
        .into_iter()
        .map(|(key, features)| {
            (
                key,
                geojson::FeatureCollection {
                    bbox: None,
                    features,
                    foreign_members: None,
                },
            )
        })
        .collect())
}

/// 添加几何属性（QGIS Add geometry attributes / Export geometry columns）：
/// 面要素追加 `AREA_M2`（测地面积，Karney 2013，绕向经 Orient 归一化）与
/// `PERIMETER_M`（测地周长，含内洞环）；线要素追加 `LENGTH_M`（测地长度）；
/// 点及其他几何不追加。属性随行，无几何要素跳过。
pub fn add_geometry_attributes(
    collection: &geojson::FeatureCollection,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::orient::{Direction, Orient};
    use geo::algorithm::GeodesicArea as _;
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let mut f = feature.clone();
        match geom {
            geo_types::Geometry::Polygon(p) => {
                let o = p.orient(Direction::Default);
                let area = o.geodesic_area_unsigned();
                let perimeter = Geodesic.length(o.exterior())
                    + o.interiors()
                        .iter()
                        .map(|r| Geodesic.length(r))
                        .sum::<f64>();
                let props = f.properties.get_or_insert_with(Default::default);
                props.insert("AREA_M2".to_string(), serde_json::Value::from(area));
                props.insert(
                    "PERIMETER_M".to_string(),
                    serde_json::Value::from(perimeter),
                );
            }
            geo_types::Geometry::MultiPolygon(mp) => {
                let o = mp.orient(Direction::Default);
                let area = o.geodesic_area_unsigned();
                let perimeter = o
                    .iter()
                    .map(|p| {
                        Geodesic.length(p.exterior())
                            + p.interiors()
                                .iter()
                                .map(|r| Geodesic.length(r))
                                .sum::<f64>()
                    })
                    .sum::<f64>();
                let props = f.properties.get_or_insert_with(Default::default);
                props.insert("AREA_M2".to_string(), serde_json::Value::from(area));
                props.insert(
                    "PERIMETER_M".to_string(),
                    serde_json::Value::from(perimeter),
                );
            }
            geo_types::Geometry::LineString(l) => {
                f.properties
                    .get_or_insert_with(Default::default)
                    .insert("LENGTH_M".to_string(), Geodesic.length(&l).into());
            }
            geo_types::Geometry::MultiLineString(m) => {
                f.properties
                    .get_or_insert_with(Default::default)
                    .insert("LENGTH_M".to_string(), Geodesic.length(&m).into());
            }
            _ => {} // 点/几何集合：不追加
        }
        out.features.push(f);
    }
    Ok(out)
}

/// 创建网格（QGIS Create grid，矩形格网）：生成覆盖 extent
/// `[minx, miny, maxx, maxy]` 的方格面要素，末行/末列裁剪至 extent；
/// 属性含 `ROW`/`COL`（行/列号，0 起，行自南向北、列自西向东）。
/// cell_size 须为正有限值且 maxx > minx、maxy > miny，否则返回
/// `KanyuError::Other`。
pub fn create_grid(extent: [f64; 4], cell_size: f64) -> Result<geojson::FeatureCollection> {
    let [minx, miny, maxx, maxy] = extent;
    if !cell_size.is_finite() || cell_size <= 0.0 {
        return Err(KanyuError::Other(format!(
            "格网单元尺寸须为正有限数值: {cell_size}"
        )));
    }
    if !(minx.is_finite() && miny.is_finite() && maxx.is_finite() && maxy.is_finite())
        || maxx <= minx
        || maxy <= miny
    {
        return Err(KanyuError::Other(format!(
            "范围须为有限值且 maxx > minx、maxy > miny: {extent:?}"
        )));
    }
    let cols = ((maxx - minx) / cell_size).ceil() as usize;
    let rows = ((maxy - miny) / cell_size).ceil() as usize;
    let mut out = empty_collection();
    for row in 0..rows {
        for col in 0..cols {
            let x0 = minx + col as f64 * cell_size;
            let y0 = miny + row as f64 * cell_size;
            let x1 = (x0 + cell_size).min(maxx);
            let y1 = (y0 + cell_size).min(maxy);
            let mut props = serde_json::Map::new();
            props.insert("ROW".to_string(), serde_json::Value::from(row));
            props.insert("COL".to_string(), serde_json::Value::from(col));
            out.features.push(geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![vec![
                    vec![x0, y0],
                    vec![x1, y0],
                    vec![x1, y1],
                    vec![x0, y1],
                    vec![x0, y0],
                ]]))),
                id: None,
                properties: Some(props),
                foreign_members: None,
            });
        }
    }
    Ok(out)
}

/// 沿线等距点（QGIS Points along geometry）：沿线每 `distance` 米（测地线
/// 口径，Karney 2013；geo `InterpolateLine::point_at_distance_from_start`）
/// 输出一个点。含起点；终点仅当线长恰为间距整数倍时包含（不额外补终点，
/// QGIS 语义）。属性随行复制并追加 `DISTANCE`（距线起点的里程，米）。
/// distance 须为正有限值；非线/无几何要素跳过。
pub fn points_along_lines(
    collection: &geojson::FeatureCollection,
    distance: f64,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::line_measures::InterpolateLine as _;
    if !distance.is_finite() || distance <= 0.0 {
        return Err(KanyuError::Other(format!(
            "点间距须为正有限数值: {distance}"
        )));
    }
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let lines: Vec<geo_types::LineString<f64>> = match geom {
            geo_types::Geometry::LineString(l) => vec![l],
            geo_types::Geometry::MultiLineString(m) => m.0,
            _ => continue, // 非线要素跳过
        };
        for line in lines {
            let total = Geodesic.length(&line);
            let mut d = 0.0;
            while d <= total {
                if let Some(p) = Geodesic.point_at_distance_from_start(&line, d) {
                    let mut f = with_geometry(feature, geojson::Value::Point(vec![p.x(), p.y()]));
                    f.properties
                        .get_or_insert_with(Default::default)
                        .insert("DISTANCE".to_string(), serde_json::Value::from(d));
                    out.features.push(f);
                }
                d += distance;
            }
        }
    }
    Ok(out)
}

/// 凹包（QGIS Concave hull）：整层点集凹包（QGIS 默认口径：图层全部几何
/// 顶点参与），输出单个面要素。geo `ConcaveHull`（concaveman 移植版）：
/// `concavity` 为相对凹度，值越小越凹、越大越接近凸包（∞ 等价凸包），
/// length_threshold 取 0（不限制最小边长）；平面口径（CRS 单位，同 QGIS
/// 平面凹包算法）。concavity 须为正有限值；有效顶点 < 3 返回
/// `KanyuError::Other`。
pub fn concave_hull(
    collection: &geojson::FeatureCollection,
    concavity: f64,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::concave_hull::ConcaveHullOptions;
    use geo::{ConcaveHull as _, CoordsIter as _};
    if !concavity.is_finite() || concavity <= 0.0 {
        return Err(KanyuError::Other(format!(
            "凹度须为正有限数值: {concavity}"
        )));
    }
    let mut points: Vec<geo_types::Point<f64>> = Vec::new();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        points.extend(geom.coords_iter().map(geo_types::Point::from));
    }
    if points.len() < 3 {
        return Err(KanyuError::Other(format!(
            "concave_hull: 有效顶点须 >= 3，实际 {}",
            points.len()
        )));
    }
    let hull = geo_types::MultiPoint(points).concave_hull_with_options(ConcaveHullOptions {
        concavity,
        length_threshold: 0.0,
    });
    let mut out = empty_collection();
    out.features.push(geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(geojson::Value::from(&hull))),
        id: None,
        properties: None,
        foreign_members: None,
    });
    Ok(out)
}

/// 定向最小包络矩形（QGIS Oriented minimum bounding box）：逐要素最小旋转
/// 矩形面（geo `MinimumRotatedRect`，旋转卡壳法，平面 CRS 单位口径），
/// 属性随行复制；无几何或退化（无法构成矩形）要素跳过。
pub fn minimum_rotated_rect(
    collection: &geojson::FeatureCollection,
) -> Result<geojson::FeatureCollection> {
    use geo::MinimumRotatedRect as _;
    let mut out = empty_collection();
    for feature in &collection.features {
        let Some(geom) = feature.geometry.as_ref().and_then(|g| to_geo(&g.value)) else {
            continue;
        };
        let Some(rect) = geom.minimum_rotated_rect() else {
            continue;
        };
        out.features
            .push(with_geometry(feature, geojson::Value::from(&rect)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Area;

    fn square(x0: f64, y0: f64, size: f64) -> geojson::Feature {
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![vec![
                vec![x0, y0],
                vec![x0 + size, y0],
                vec![x0 + size, y0 + size],
                vec![x0, y0 + size],
                vec![x0, y0],
            ]]))),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn with_props(mut f: geojson::Feature, zone: &str, extra: f64) -> geojson::Feature {
        let mut props = serde_json::Map::new();
        props.insert("zone".to_string(), zone.into());
        props.insert("v".to_string(), extra.into());
        f.properties = Some(props);
        f
    }

    fn collection_of(features: Vec<geojson::Feature>) -> geojson::FeatureCollection {
        geojson::FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }
    }

    #[test]
    fn dissolve_unions_overlapping_by_field() {
        // 两个重叠方格（同 zone）+ 一个独立方格（异 zone）。
        let a = with_props(square(0.0, 0.0, 2.0), "x", 1.0);
        let b = with_props(square(1.0, 0.0, 2.0), "x", 2.0);
        let c = with_props(square(10.0, 10.0, 1.0), "y", 3.0);
        let out = dissolve(&collection_of(vec![a, b, c]), Some("zone")).unwrap();
        assert_eq!(out.features.len(), 2, "zone x 融合为 1、y 独立");
        let zoned_x = out
            .features
            .iter()
            .find(|f| f.properties.as_ref().unwrap()["zone"] == "x")
            .unwrap();
        // 并集面积 = 4+4-2（重叠 2×1）= 6（CRS 平面单位）。
        let geom = to_geo(&zoned_x.geometry.as_ref().unwrap().value).unwrap();
        let area = match geom {
            geo_types::Geometry::Polygon(p) => p.unsigned_area(),
            geo_types::Geometry::MultiPolygon(mp) => mp.unsigned_area(),
            _ => 0.0,
        };
        assert!((area - 6.0).abs() < 1e-9, "并集面积应为 6，实测 {area}");
        // keep-first 语义：v 取首要素 1.0。
        assert_eq!(
            zoned_x.properties.as_ref().unwrap()["v"].as_f64().unwrap(),
            1.0
        );
    }

    #[test]
    fn centroid_of_square_is_center() {
        let out = centroid(&collection_of(vec![square(0.0, 0.0, 2.0)])).unwrap();
        match &out.features[0].geometry.as_ref().unwrap().value {
            geojson::Value::Point(p) => {
                assert!((p[0] - 1.0).abs() < 1e-9 && (p[1] - 1.0).abs() < 1e-9);
            }
            other => panic!("应为 Point: {other:?}"),
        }
    }

    #[test]
    fn convex_hull_wraps_line() {
        let line = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(vec![
                vec![0.0, 0.0],
                vec![1.0, 0.5],
                vec![2.0, 0.0],
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let out = convex_hull(&collection_of(vec![line])).unwrap();
        match &out.features[0].geometry.as_ref().unwrap().value {
            geojson::Value::Polygon(_) => {}
            other => panic!("凸包应为 Polygon: {other:?}"),
        }
    }

    #[test]
    fn simplify_reduces_vertices_and_drops_degenerate() {
        // 近直线上的密集点应被抽稀。
        let mut line: Vec<Vec<f64>> = Vec::new();
        for i in 0..=20 {
            line.push(vec![i as f64, (i % 2) as f64 * 0.0001]);
        }
        let f = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(line))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let out = simplify(&collection_of(vec![f]), 0.01).unwrap();
        if let geojson::Value::LineString(ls) = &out.features[0].geometry.as_ref().unwrap().value {
            assert!(ls.len() < 10, "抽稀后顶点应显著减少，实测 {}", ls.len());
        } else {
            panic!("应为 LineString");
        }
        // 负容差报错。
        assert!(simplify(&collection_of(vec![]), -1.0).is_err());
    }

    #[test]
    fn delete_holes_removes_or_keeps_by_area() {
        let exterior = vec![
            vec![0.0, 0.0],
            vec![10.0, 0.0],
            vec![10.0, 10.0],
            vec![0.0, 10.0],
            vec![0.0, 0.0],
        ];
        let hole_small = vec![
            vec![1.0, 1.0],
            vec![2.0, 1.0],
            vec![2.0, 2.0],
            vec![1.0, 2.0],
            vec![1.0, 1.0],
        ];
        let hole_big = vec![
            vec![3.0, 3.0],
            vec![9.0, 3.0],
            vec![9.0, 9.0],
            vec![3.0, 9.0],
            vec![3.0, 3.0],
        ];
        let f = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![
                exterior, hole_small, hole_big,
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        // 全删。
        let out = delete_holes(&collection_of(vec![f.clone()]), None).unwrap();
        if let geojson::Value::Polygon(rings) = &out.features[0].geometry.as_ref().unwrap().value {
            assert_eq!(rings.len(), 1);
        } else {
            panic!("应为 Polygon");
        }
        // 阈值 10：小洞删（1.0）大洞留（36.0）。
        let out = delete_holes(&collection_of(vec![f]), Some(10.0)).unwrap();
        if let geojson::Value::Polygon(rings) = &out.features[0].geometry.as_ref().unwrap().value {
            assert_eq!(rings.len(), 2, "应保留外环 + 大洞");
        } else {
            panic!("应为 Polygon");
        }
    }

    #[test]
    fn explode_splits_multiparts() {
        let f = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::MultiPoint(vec![
                vec![0.0, 0.0],
                vec![1.0, 1.0],
                vec![2.0, 2.0],
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let out = explode(&collection_of(vec![f])).unwrap();
        assert_eq!(out.features.len(), 3);
        for feat in &out.features {
            assert!(matches!(
                feat.geometry.as_ref().unwrap().value,
                geojson::Value::Point(_)
            ));
        }
    }

    #[test]
    fn stats_counts_and_units() {
        // 赤道附近 1°×1° 方格 + 一条线 + 一个点。
        let poly = square(0.0, 0.0, 1.0);
        let line = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(vec![
                vec![0.0, 0.0],
                vec![1.0, 0.0],
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let point = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![
                0.0, 0.0,
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let s = stats(&collection_of(vec![poly, line, point])).unwrap();
        assert_eq!(s.feature_count, 3);
        assert_eq!(s.polygons, 1);
        assert_eq!(s.lines, 1);
        assert_eq!(s.points, 1);
        // 赤道 1°×1° ≈ 12308 km²（±2%）。
        let expected_km2 = 12308.0;
        let actual_km2 = s.total_area_km2;
        assert!(
            (actual_km2 - expected_km2).abs() / expected_km2 < 0.02,
            "面积 {actual_km2} km² 与预期 {expected_km2} 偏差过大"
        );
        // 亩换算：1 亩 = 10000/15 ㎡。
        assert!((s.total_area_mu - s.total_area_m2 / (10000.0 / 15.0)).abs() < 1e-6);
        // 1° 赤道长度 ≈ 111.19 km（±1%）。
        assert!((s.total_length_km - 111.19).abs() / 111.19 < 0.01 + 0.005);
    }

    fn point_feature(x: f64, y: f64) -> geojson::Feature {
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![x, y]))),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn line_feature(coords: Vec<Vec<f64>>) -> geojson::Feature {
        geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::LineString(coords))),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn props_of(
        pairs: &[(&str, serde_json::Value)],
    ) -> Option<serde_json::Map<String, serde_json::Value>> {
        let mut m = serde_json::Map::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        Some(m)
    }

    #[test]
    fn boundary_emits_rings_endpoints_and_skips_boundless() {
        // 带洞面 → MultiLineString（外环 + 洞）。
        let holed = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![
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
            ]))),
            id: None,
            properties: props_of(&[("name", "holed".into())]),
            foreign_members: None,
        };
        // 单环面 → LineString；开放线 → 两端点 MultiPoint；闭合线与点 → 跳过。
        let simple = with_props(square(0.0, 0.0, 2.0), "s", 1.0);
        let open = line_feature(vec![vec![0.0, 0.0], vec![1.0, 1.0], vec![2.0, 0.0]]);
        let closed = line_feature(vec![
            vec![0.0, 0.0],
            vec![1.0, 0.0],
            vec![1.0, 1.0],
            vec![0.0, 0.0],
        ]);
        let pt = point_feature(5.0, 5.0);
        let out = boundary(&collection_of(vec![holed, simple, open, closed, pt])).unwrap();
        assert_eq!(out.features.len(), 3, "闭合线与点无边界应跳过");
        match &out.features[0].geometry.as_ref().unwrap().value {
            geojson::Value::MultiLineString(mls) => assert_eq!(mls.len(), 2, "外环 + 洞"),
            other => panic!("带洞面边界应为 MultiLineString: {other:?}"),
        }
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["name"],
            "holed",
            "属性随行"
        );
        assert!(matches!(
            out.features[1].geometry.as_ref().unwrap().value,
            geojson::Value::LineString(_)
        ));
        match &out.features[2].geometry.as_ref().unwrap().value {
            geojson::Value::MultiPoint(pts) => {
                assert_eq!(pts, &vec![vec![0.0, 0.0], vec![2.0, 0.0]], "开放线端点");
            }
            other => panic!("开放线边界应为 MultiPoint: {other:?}"),
        }
    }

    #[test]
    fn bounding_boxes_wraps_each_feature() {
        let f = with_props(square(1.0, 2.0, 3.0), "z", 7.0);
        let out = bounding_boxes(&collection_of(vec![f, point_feature(9.0, 9.0)])).unwrap();
        assert_eq!(out.features.len(), 2, "点也有退化包络矩形");
        let geom = to_geo(&out.features[0].geometry.as_ref().unwrap().value).unwrap();
        let geo_types::Geometry::Polygon(p) = geom else {
            panic!("包络应为 Polygon")
        };
        use geo::BoundingRect;
        let r = p.bounding_rect().unwrap();
        assert_eq!(
            (r.min().x, r.min().y, r.max().x, r.max().y),
            (1.0, 2.0, 4.0, 5.0)
        );
        assert_eq!(out.features[0].properties.as_ref().unwrap()["zone"], "z");
    }

    #[test]
    fn merge_concatenates_in_input_order() {
        let a1 = with_props(square(0.0, 0.0, 1.0), "a1", 1.0);
        let a2 = with_props(square(2.0, 0.0, 1.0), "a2", 2.0);
        let b = with_props(point_feature(9.0, 9.0), "b", 3.0);
        let ca = collection_of(vec![a1, a2]);
        let cb = collection_of(vec![b]);
        let out = merge(&[&ca, &cb]).unwrap();
        assert_eq!(out.features.len(), 3);
        let zones: Vec<&str> = out
            .features
            .iter()
            .map(|f| f.properties.as_ref().unwrap()["zone"].as_str().unwrap())
            .collect();
        assert_eq!(zones, vec!["a1", "a2", "b"], "按输入顺序拼接且属性原样");
        // 空输入得空图层。
        assert_eq!(merge(&[]).unwrap().features.len(), 0);
    }

    #[test]
    fn extract_by_attribute_matches_query_semantics() {
        let mk = |h: serde_json::Value, u: serde_json::Value| {
            let mut f = point_feature(0.0, 0.0);
            f.properties = props_of(&[("height", h), ("usage", u)]);
            f
        };
        let coll = collection_of(vec![
            mk(80.0.into(), "office".into()),
            mk(30.0.into(), "residential".into()),
            mk(55.5.into(), "office".into()),
            mk(serde_json::Value::Null, serde_json::Value::Null),
        ]);
        // 数值比较。
        assert_eq!(
            extract_by_attribute(&coll, "height > 50")
                .unwrap()
                .features
                .len(),
            2
        );
        assert_eq!(
            extract_by_attribute(&coll, "height >= 55.5")
                .unwrap()
                .features
                .len(),
            2
        );
        // 字符串相等（= 与 == 等价），Null 要素不匹配。
        assert_eq!(
            extract_by_attribute(&coll, "usage = residential")
                .unwrap()
                .features
                .len(),
            1
        );
        assert_eq!(
            extract_by_attribute(&coll, "usage == office")
                .unwrap()
                .features
                .len(),
            2
        );
        assert_eq!(
            extract_by_attribute(&coll, "usage != office")
                .unwrap()
                .features
                .len(),
            1
        );
        // 子串包含。
        assert_eq!(
            extract_by_attribute(&coll, "usage contains siden")
                .unwrap()
                .features
                .len(),
            1
        );
        // 不存在字段：全不匹配；非法表达式：报错。
        assert_eq!(
            extract_by_attribute(&coll, "nonexist = 1")
                .unwrap()
                .features
                .len(),
            0
        );
        assert!(extract_by_attribute(&coll, "garbage expr").is_err());
        // 属性随行：几何与属性原样保留。
        let out = extract_by_attribute(&coll, "height > 50").unwrap();
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["height"]
                .as_f64()
                .unwrap(),
            80.0
        );
    }

    #[test]
    fn extract_by_location_predicates() {
        let mask = collection_of(vec![square(0.0, 0.0, 10.0)]);
        let coll = collection_of(vec![
            point_feature(5.0, 5.0),   // 内
            point_feature(0.0, 5.0),   // 边界
            point_feature(20.0, 20.0), // 外
        ]);
        // intersects：含边界 → 2。
        assert_eq!(
            extract_by_location(&coll, &mask, "intersects")
                .unwrap()
                .features
                .len(),
            2
        );
        // within：严格内含（不含边界）→ 1。
        assert_eq!(
            extract_by_location(&coll, &mask, "within")
                .unwrap()
                .features
                .len(),
            1
        );
        // contains：面含内点 → 1；边界点不算 contained。
        let polys = collection_of(vec![square(0.0, 0.0, 10.0)]);
        let inner_pt = collection_of(vec![point_feature(5.0, 5.0)]);
        let edge_pt = collection_of(vec![point_feature(0.0, 5.0)]);
        assert_eq!(
            extract_by_location(&polys, &inner_pt, "contains")
                .unwrap()
                .features
                .len(),
            1
        );
        assert_eq!(
            extract_by_location(&polys, &edge_pt, "contains")
                .unwrap()
                .features
                .len(),
            0
        );
        // 非法谓词报结构化错误。
        let err = extract_by_location(&coll, &mask, "touches").unwrap_err();
        assert!(
            matches!(err, KanyuError::InvalidQuery(_)),
            "应为 InvalidQuery: {err}"
        );
    }

    #[test]
    fn count_points_in_polygon_counts_boundary_and_subpoints() {
        let p1 = with_props(square(0.0, 0.0, 10.0), "a", 1.0);
        let p2 = with_props(square(20.0, 20.0, 5.0), "b", 2.0);
        let not_poly = point_feature(1.0, 1.0); // 非面要素跳过
        let polys = collection_of(vec![p1, p2, not_poly]);
        let multi = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::MultiPoint(vec![
                vec![22.0, 22.0], // 落入 p2
                vec![30.0, 30.0], // 面外
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let pts = collection_of(vec![
            point_feature(5.0, 5.0),   // p1 内
            point_feature(0.0, 5.0),   // p1 边界（计入）
            point_feature(15.0, 15.0), // 两面之外
            multi,
        ]);
        let out = count_points_in_polygon(&polys, &pts).unwrap();
        assert_eq!(out.features.len(), 2, "非面要素应跳过");
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["NUMPOINTS"]
                .as_u64()
                .unwrap(),
            2,
            "内部 1 + 边界 1"
        );
        assert_eq!(
            out.features[1].properties.as_ref().unwrap()["NUMPOINTS"]
                .as_u64()
                .unwrap(),
            1,
            "MultiPoint 按子点计 1"
        );
        // 原属性保留。
        assert_eq!(out.features[0].properties.as_ref().unwrap()["zone"], "a");
    }

    #[test]
    fn field_stats_population_stddev_and_errors() {
        let mk = |v: serde_json::Value| {
            let mut f = point_feature(0.0, 0.0);
            f.properties = props_of(&[("v", v)]);
            f
        };
        let missing = point_feature(0.0, 0.0); // 无 v 字段
        let coll = collection_of(vec![
            mk(10.0.into()),
            mk(20.0.into()),
            mk(30.0.into()),
            mk("str".into()), // 非数值计入 null_count
            missing,
        ]);
        let s = field_stats(&coll, "v").unwrap();
        assert_eq!(s.count, 3);
        assert_eq!(s.null_count, 2);
        assert_eq!(s.min, 10.0);
        assert_eq!(s.max, 30.0);
        assert_eq!(s.sum, 60.0);
        assert_eq!(s.mean, 20.0);
        assert_eq!(s.range, 20.0);
        // 总体标准差 sqrt(((−10)²+0²+10²)/3) = sqrt(200/3)。
        assert!((s.stddev - (200.0_f64 / 3.0).sqrt()).abs() < 1e-12);
        // 字段不存在/全空：结构化错误。
        assert!(matches!(
            field_stats(&coll, "nonexist"),
            Err(KanyuError::InvalidQuery(_))
        ));
        assert!(field_stats(&collection_of(vec![]), "v").is_err());
    }

    #[test]
    fn mean_coordinates_plain_and_weighted() {
        // 不加权：方格质心 (1,1) 与点 (3,3) → (2,2)。
        let coll = collection_of(vec![square(0.0, 0.0, 2.0), point_feature(3.0, 3.0)]);
        let out = mean_coordinates(&coll, None).unwrap();
        assert_eq!(out.features.len(), 1);
        let f = &out.features[0];
        match &f.geometry.as_ref().unwrap().value {
            geojson::Value::Point(p) => {
                assert!((p[0] - 2.0).abs() < 1e-9 && (p[1] - 2.0).abs() < 1e-9);
            }
            other => panic!("应为 Point: {other:?}"),
        }
        let props = f.properties.as_ref().unwrap();
        assert_eq!(props["MEAN_X"].as_f64().unwrap(), 2.0);
        assert_eq!(props["MEAN_Y"].as_f64().unwrap(), 2.0);
        // 加权：(0,0)×1 与 (2,2)×3 → (1.5,1.5)。
        let w = |x: f64, weight: f64| {
            let mut f = point_feature(x, x);
            f.properties = props_of(&[("w", weight.into())]);
            f
        };
        let wcoll = collection_of(vec![w(0.0, 1.0), w(2.0, 3.0)]);
        let out = mean_coordinates(&wcoll, Some("w")).unwrap();
        let props = out.features[0].properties.as_ref().unwrap();
        assert!((props["MEAN_X"].as_f64().unwrap() - 1.5).abs() < 1e-9);
        assert!((props["MEAN_Y"].as_f64().unwrap() - 1.5).abs() < 1e-9);
        // 权重缺失按 0 计 → 权重总和为 0 报错。
        assert!(
            mean_coordinates(&collection_of(vec![point_feature(1.0, 1.0)]), Some("w")).is_err()
        );
        // 空输入报错。
        assert!(mean_coordinates(&collection_of(vec![]), None).is_err());
    }

    #[test]
    fn distance_matrix_pairwise_geodesic() {
        // 赤道三点：a = (0,0),(2,0)，b = (1,0)。
        let mut a0 = point_feature(0.0, 0.0);
        a0.id = Some(geojson::feature::Id::String("a0".to_string()));
        let a1 = point_feature(2.0, 0.0); // 无 id → 序号 "1"
        let b0 = point_feature(1.0, 0.0);
        let m = distance_matrix(&collection_of(vec![a0, a1]), &collection_of(vec![b0])).unwrap();
        assert_eq!(
            m.rows,
            vec!["a0".to_string(), "1".to_string()],
            "id 优先、缺 id 取序号"
        );
        assert_eq!(m.cols, vec!["0".to_string()]);
        // 赤道 1° ≈ 111319.49 m（±0.5%）。
        for row in &m.matrix {
            assert!(
                (row[0] - 111319.49).abs() / 111319.49 < 0.005,
                "实测 {}",
                row[0]
            );
        }
        assert_eq!(m.matrix.len(), 2);
        assert!((m.min - m.max).abs() < 1e-9, "对称布点 min == max");
        assert!((m.mean - m.matrix[0][0]).abs() < 1e-9);
        // 面要素取质心：2° 方格质心 (1,1) 对 (1,0) 为子午线 1° ≈ 110574.39 m。
        let poly = collection_of(vec![square(0.0, 0.0, 2.0)]);
        let pt = collection_of(vec![point_feature(1.0, 0.0)]);
        let m = distance_matrix(&poly, &pt).unwrap();
        assert!((m.matrix[0][0] - 110574.39).abs() / 110574.39 < 0.005);
        // 空图层报错。
        assert!(distance_matrix(&collection_of(vec![]), &pt).is_err());
        assert!(distance_matrix(&pt, &collection_of(vec![])).is_err());
    }

    #[test]
    fn nearest_neighbor_stats_and_errors() {
        // 赤道 1°×1° 四点：最近邻均为子午线边 ≈ 110574.39 m。
        let coll = collection_of(vec![
            point_feature(0.0, 0.0),
            point_feature(1.0, 0.0),
            point_feature(1.0, 1.0),
            point_feature(0.0, 1.0),
        ]);
        let r = nearest_neighbor(&coll).unwrap();
        assert_eq!(r.count, 4);
        assert!((r.observed_mean_distance - 110574.39).abs() / 110574.39 < 0.01);
        assert!((r.min - r.max).abs() < 1e-6, "对称布点 min == max");
        // 期望 = 0.5/sqrt(n/A)，NNI = 观测/期望；四点散布 NNI > 1（离散）。
        assert!(r.expected_mean_distance > 0.0);
        assert!(
            (r.nearest_neighbor_index - r.observed_mean_distance / r.expected_mean_distance).abs()
                < 1e-12
        );
        assert!(r.nearest_neighbor_index > 1.0);
        // n < 2 结构化错误。
        let err = nearest_neighbor(&collection_of(vec![point_feature(0.0, 0.0)])).unwrap_err();
        assert!(matches!(err, KanyuError::Other(_)), "应为 Other: {err}");
        // 共线点集外包矩形面积为零 → 结构化错误。
        let collinear = collection_of(vec![
            point_feature(0.0, 0.0),
            point_feature(1.0, 0.0),
            point_feature(2.0, 0.0),
        ]);
        assert!(nearest_neighbor(&collinear).is_err());
    }

    #[test]
    fn multi_ring_buffer_rings_are_differences() {
        use geo::Area;
        let pt = collection_of(vec![point_feature(0.0, 0.0)]);
        let out = multi_ring_buffer(&pt, &[0.1, 0.2]).unwrap();
        assert_eq!(out.features.len(), 2, "每档一环");
        let props0 = out.features[0].properties.as_ref().unwrap();
        assert_eq!(props0["RING"].as_u64().unwrap(), 1);
        assert_eq!(props0["DISTANCE"].as_f64().unwrap(), 0.1);
        let area_of = |f: &geojson::Feature| -> f64 {
            match to_geo(&f.geometry.as_ref().unwrap().value).unwrap() {
                geo_types::Geometry::Polygon(p) => p.unsigned_area(),
                geo_types::Geometry::MultiPolygon(mp) => mp.unsigned_area(),
                _ => 0.0,
            }
        };
        // 第一环 ≈ π·0.1²，第二环 ≈ π·(0.2²−0.1²)（圆角拟合，容差 10%）。
        let a1 = area_of(&out.features[0]);
        let a2 = area_of(&out.features[1]);
        assert!(
            (a1 - std::f64::consts::PI * 0.01).abs() / (std::f64::consts::PI * 0.01) < 0.1,
            "环1面积 {a1}"
        );
        assert!(
            (a2 - std::f64::consts::PI * 0.03).abs() / (std::f64::consts::PI * 0.03) < 0.1,
            "环2面积 {a2}"
        );
        // 非法距离序列：空 / 非递增 / 负值。
        assert!(multi_ring_buffer(&pt, &[]).is_err());
        assert!(multi_ring_buffer(&pt, &[0.2, 0.1]).is_err());
        assert!(multi_ring_buffer(&pt, &[-1.0]).is_err());
    }

    #[test]
    fn variable_buffer_uses_field_and_counts_skipped() {
        let mk = |v: serde_json::Value| {
            let mut f = point_feature(0.0, 0.0);
            f.properties = props_of(&[("r", v)]);
            f
        };
        let missing = point_feature(0.0, 0.0); // 无 r 字段
        let coll = collection_of(vec![mk(0.1.into()), mk("bad".into()), missing]);
        let out = variable_buffer(&coll, "r", 8).unwrap();
        assert_eq!(out.features.len(), 1, "仅数值字段要素缓冲");
        let skipped = out.foreign_members.as_ref().unwrap()["skipped"]
            .as_u64()
            .unwrap();
        assert_eq!(skipped, 2, "非数值/缺失计入 skipped");
        assert!(matches!(
            out.features[0].geometry.as_ref().unwrap().value,
            geojson::Value::MultiPolygon(_)
        ));
        // segments = 0 报错。
        assert!(variable_buffer(&coll, "r", 0).is_err());
    }

    #[test]
    fn split_by_field_groups_in_btree_order() {
        let mk = |v: Option<serde_json::Value>| {
            let mut f = point_feature(0.0, 0.0);
            if let Some(v) = v {
                f.properties = props_of(&[("zone", v)]);
            }
            f
        };
        let coll = collection_of(vec![
            mk(Some("b".into())),
            mk(Some("a".into())),
            mk(Some(5.0.into())), // 数值字符串化
            mk(None),             // 缺字段归空串组
            mk(Some("a".into())),
        ]);
        let groups = split_by_field(&coll, "zone").unwrap();
        let keys: Vec<&str> = groups.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["", "5.0", "a", "b"], "BTreeMap 字典序");
        assert_eq!(groups[2].1.features.len(), 2, "a 组两个要素");
        assert_eq!(groups[0].1.features.len(), 1, "空串组一个要素");
    }

    #[test]
    fn add_geometry_attributes_by_type() {
        let poly = square(0.0, 0.0, 1.0); // 赤道 1°×1°
        let line = line_feature(vec![vec![0.0, 0.0], vec![1.0, 0.0]]);
        let pt = point_feature(5.0, 5.0);
        let out = add_geometry_attributes(&collection_of(vec![poly, line, pt])).unwrap();
        assert_eq!(out.features.len(), 3);
        let p0 = out.features[0].properties.as_ref().unwrap();
        let area = p0["AREA_M2"].as_f64().unwrap();
        assert!((area - 1.2308e10).abs() / 1.2308e10 < 0.02, "面积 {area}");
        let perimeter = p0["PERIMETER_M"].as_f64().unwrap();
        // 周长 = 2×(111319.49 + 110574.39) ≈ 443788 m（±1%）。
        assert!(
            (perimeter - 443788.0).abs() / 443788.0 < 0.01,
            "周长 {perimeter}"
        );
        let p1 = out.features[1].properties.as_ref().unwrap();
        let length = p1["LENGTH_M"].as_f64().unwrap();
        assert!(
            (length - 111319.49).abs() / 111319.49 < 0.01,
            "长度 {length}"
        );
        assert!(p1.get("AREA_M2").is_none(), "线要素不追加面积");
        // 点要素不追加任何几何属性。
        let p2 = out.features[2].properties.as_ref();
        assert!(p2.is_none() || p2.unwrap().get("LENGTH_M").is_none());
    }

    #[test]
    fn create_grid_covers_extent_with_clipped_edge() {
        let out = create_grid([0.0, 0.0, 2.5, 1.0], 1.0).unwrap();
        assert_eq!(out.features.len(), 3, "3 列 × 1 行（末列裁剪）");
        let props = out.features[2].properties.as_ref().unwrap();
        assert_eq!(props["ROW"].as_u64().unwrap(), 0);
        assert_eq!(props["COL"].as_u64().unwrap(), 2);
        if let geojson::Value::Polygon(rings) = &out.features[2].geometry.as_ref().unwrap().value {
            assert_eq!(rings[0][1], vec![2.5, 0.0], "末列右缘裁剪至 maxx");
        } else {
            panic!("格网单元应为 Polygon");
        }
        // 非法参数：cell_size <= 0 / 范围倒置。
        assert!(create_grid([0.0, 0.0, 1.0, 1.0], 0.0).is_err());
        assert!(create_grid([1.0, 0.0, 0.0, 1.0], 0.5).is_err());
    }

    #[test]
    fn points_along_lines_spacing_and_endpoint() {
        // 赤道线 (0,0)-(1,0)，间距取半长 → 起点 + 中点 + 终点（整除含终点）。
        let line = line_feature(vec![vec![0.0, 0.0], vec![1.0, 0.0]]);
        let total = Geodesic.length(&geo_types::LineString::from(vec![(0.0, 0.0), (1.0, 0.0)]));
        let out = points_along_lines(&collection_of(vec![line.clone()]), total / 2.0).unwrap();
        assert_eq!(out.features.len(), 3, "0、D/2、D 三点");
        match &out.features[1].geometry.as_ref().unwrap().value {
            geojson::Value::Point(p) => {
                assert!((p[0] - 0.5).abs() < 1e-6 && p[1].abs() < 1e-9, "中点 {p:?}");
            }
            other => panic!("应为 Point: {other:?}"),
        }
        let d1 = out.features[1].properties.as_ref().unwrap()["DISTANCE"]
            .as_f64()
            .unwrap();
        assert!((d1 - total / 2.0).abs() < 1e-9, "DISTANCE 里程");
        // 间距超过线长 → 仅起点。
        let out = points_along_lines(&collection_of(vec![line]), total * 2.0).unwrap();
        assert_eq!(out.features.len(), 1);
        // 非线要素跳过；非法间距报错。
        let out = points_along_lines(&collection_of(vec![point_feature(0.0, 0.0)]), 100.0).unwrap();
        assert_eq!(out.features.len(), 0);
        assert!(points_along_lines(&collection_of(vec![]), 0.0).is_err());
    }

    #[test]
    fn concave_hull_drills_into_interior() {
        use geo::Area;
        // (1,2) 为三角形 (0,0),(3,0),(0,4) 的内点。
        let coll = collection_of(vec![
            point_feature(0.0, 0.0),
            point_feature(3.0, 0.0),
            point_feature(1.0, 2.0),
            point_feature(0.0, 4.0),
        ]);
        let area_of = |c: &geojson::FeatureCollection| -> f64 {
            match to_geo(&c.features[0].geometry.as_ref().unwrap().value).unwrap() {
                geo_types::Geometry::Polygon(p) => p.unsigned_area(),
                _ => 0.0,
            }
        };
        // concavity = 2.0：钻入内点，面积 5 < 凸包 6。
        let concave = concave_hull(&coll, 2.0).unwrap();
        assert_eq!(concave.features.len(), 1, "整层单个凹包面");
        assert!((area_of(&concave) - 5.0).abs() < 1e-9, "凹包面积应为 5");
        // 超大 concavity：等价凸包，面积 6。
        let convex_like = concave_hull(&coll, 1e9).unwrap();
        assert!((area_of(&convex_like) - 6.0).abs() < 1e-9, "应退化为凸包");
        // 非法凹度 / 顶点不足。
        assert!(concave_hull(&coll, 0.0).is_err());
        assert!(concave_hull(&collection_of(vec![point_feature(0.0, 0.0)]), 2.0).is_err());
    }

    #[test]
    fn minimum_rotated_rect_recovers_rotation() {
        use geo::Area;
        // 旋转 45° 的矩形：边长 2√2 × √2，面积 4（轴对齐包络面积为 9）。
        let rotated = geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![vec![
                vec![0.0, 0.0],
                vec![2.0, 2.0],
                vec![3.0, 1.0],
                vec![1.0, -1.0],
                vec![0.0, 0.0],
            ]]))),
            id: None,
            properties: props_of(&[("name", "rot45".into())]),
            foreign_members: None,
        };
        let out = minimum_rotated_rect(&collection_of(vec![rotated])).unwrap();
        assert_eq!(out.features.len(), 1);
        let geom = to_geo(&out.features[0].geometry.as_ref().unwrap().value).unwrap();
        let geo_types::Geometry::Polygon(p) = geom else {
            panic!("应为 Polygon")
        };
        assert!(
            (p.unsigned_area() - 4.0).abs() < 0.01,
            "最小旋转矩形面积应为 4，实测 {}",
            p.unsigned_area()
        );
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["name"],
            "rot45",
            "属性随行"
        );
        // 无几何要素跳过。
        let empty = geojson::Feature {
            bbox: None,
            geometry: None,
            id: None,
            properties: None,
            foreign_members: None,
        };
        assert_eq!(
            minimum_rotated_rect(&collection_of(vec![empty]))
                .unwrap()
                .features
                .len(),
            0
        );
    }
}
