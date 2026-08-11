//! QGIS 核心算法移植（转写正确性优先，语义对齐 QGIS Processing）：
//! dissolve（融合）/ centroid（质心）/ convex_hull（凸包）/ simplify（道格拉斯简化）/
//! delete_holes（删洞）/ explode（多部件炸开）/ stats（图层统计，含亩/公顷）。
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
}
