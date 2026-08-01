//! 坐标参考系（CRS）工具：投影变换与测地线度量。
//!
//! 投影基于 proj4rs（纯 Rust PROJ 改写）+ crs-definitions EPSG 数据库；
//! 测地线度量基于 geo crate（Karney 2013 算法，单位为米/平方米）。
//! 与 [`crate::analysis`] 配套：EPSG:4326 数据先 `reproject` 到米制 CRS
//! 再做 buffer/overlay，或直接对经纬度数据做 `measure` 测地线度量。

use crate::error::{KanyuError, Result};

/// 解析 CRS 定义：`"EPSG:xxxx"`（内置 EPSG 数据库，crs-definitions crate）、
/// proj4 定义串（`+proj=…`）或 `"WGS84"` 快捷方式；无法解析报中文错误。
fn parse_crs(def: &str) -> Result<proj4rs::Proj> {
    proj4rs::Proj::from_user_string(def).map_err(|e| {
        KanyuError::Other(format!(
            "无法解析 CRS 定义 '{def}'：{e}；支持 \"EPSG:xxxx\"（内置 EPSG 数据库）、\
             proj4 定义串（+proj=…）与 \"WGS84\""
        ))
    })
}

/// 投影变换：逐坐标从 `from` 转换到 `to`（递归处理全部几何类型嵌套，z 不变）。
/// `from`/`to` 接受 `"EPSG:xxxx"` 或 proj4 定义串；`from == to`（大小写不敏感）
/// 时原样返回。转换失败（NaN/越界）报中文错误并指出要素序号。
///
/// 单位约定：地理 CRS（经纬度）按度↔弧度自动衔接（GeoJSON 为度，PROJ 内部
/// 为弧度）；投影 CRS 输出为其固有单位（米/英尺等）。
pub fn reproject(
    collection: &geojson::FeatureCollection,
    from: &str,
    to: &str,
) -> Result<geojson::FeatureCollection> {
    if from.trim().eq_ignore_ascii_case(to.trim()) {
        return Ok(collection.clone());
    }
    let src = parse_crs(from)?;
    let dst = parse_crs(to)?;
    let (src_deg, dst_deg) = (src.is_latlong(), dst.is_latlong());

    let mut out = collection.clone();
    for (idx, feature) in out.features.iter_mut().enumerate() {
        let Some(geom) = &mut feature.geometry else {
            continue;
        };
        reproject_value(&mut geom.value, &src, &dst, src_deg, dst_deg, idx)?;
    }
    Ok(out)
}

/// 递归转换 geojson 几何的全部坐标。
fn reproject_value(
    value: &mut geojson::Value,
    src: &proj4rs::Proj,
    dst: &proj4rs::Proj,
    src_deg: bool,
    dst_deg: bool,
    idx: usize,
) -> Result<()> {
    match value {
        geojson::Value::Point(pos) => reproject_position(pos, src, dst, src_deg, dst_deg, idx),
        geojson::Value::MultiPoint(pts) | geojson::Value::LineString(pts) => {
            for p in pts {
                reproject_position(p, src, dst, src_deg, dst_deg, idx)?;
            }
            Ok(())
        }
        geojson::Value::MultiLineString(ls) | geojson::Value::Polygon(ls) => {
            for l in ls {
                for p in l {
                    reproject_position(p, src, dst, src_deg, dst_deg, idx)?;
                }
            }
            Ok(())
        }
        geojson::Value::MultiPolygon(polys) => {
            for poly in polys {
                for ring in poly {
                    for p in ring {
                        reproject_position(p, src, dst, src_deg, dst_deg, idx)?;
                    }
                }
            }
            Ok(())
        }
        geojson::Value::GeometryCollection(geoms) => {
            for g in geoms {
                reproject_value(&mut g.value, src, dst, src_deg, dst_deg, idx)?;
            }
            Ok(())
        }
    }
}

/// 单坐标转换（z 不变；度↔弧度衔接；结果必须有限）。
fn reproject_position(
    pos: &mut [f64],
    src: &proj4rs::Proj,
    dst: &proj4rs::Proj,
    src_deg: bool,
    dst_deg: bool,
    idx: usize,
) -> Result<()> {
    if pos.len() < 2 {
        return Err(KanyuError::Other(format!(
            "要素 {} 存在非法坐标（维数 < 2）",
            idx + 1
        )));
    }
    let (mut x, mut y) = (pos[0], pos[1]);
    if src_deg {
        x = x.to_radians();
        y = y.to_radians();
    }
    let (x, y, _) = proj4rs::adaptors::transform_xyz(src, dst, x, y, 0.0).map_err(|e| {
        KanyuError::Other(format!(
            "坐标转换失败（要素 {}，坐标 ({x}, {y})）：{e}",
            idx + 1
        ))
    })?;
    if !x.is_finite() || !y.is_finite() {
        return Err(KanyuError::Other(format!(
            "坐标转换结果非法（要素 {}）：越界或 NaN",
            idx + 1
        )));
    }
    let (x, y) = if dst_deg {
        (x.to_degrees(), y.to_degrees())
    } else {
        (x, y)
    };
    pos[0] = x;
    pos[1] = y;
    Ok(())
}

/// 度量类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureKind {
    /// 测地线长度（米）。
    Length,
    /// 测地线面积（平方米）。
    Area,
}

impl std::str::FromStr for MeasureKind {
    type Err = KanyuError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "length" => Ok(Self::Length),
            "area" => Ok(Self::Area),
            other => Err(KanyuError::Other(format!(
                "未知度量类型 '{other}'（支持 length/area）"
            ))),
        }
    }
}

/// 测地线度量（Karney 2013，WGS84 椭球；输入应为经纬度数据如 EPSG:4326，
/// 投影数据请先 `reproject` 回地理 CRS——rustdoc 即契约）。
///
/// 输出 JSON：
/// `{"kind": "length"|"area", "unit": "m"|"m²", "total": f64,
///   "per_feature": [{"index": i, "value": v}, …]}`
///
/// 口径：Length 取线长度与面外环周长，Area 取面面积（含洞扣除）；
/// Point/MultiPoint 与无几何要素度量值为 0（不产生错误）。
pub fn measure(
    collection: &geojson::FeatureCollection,
    kind: MeasureKind,
) -> Result<serde_json::Value> {
    let mut per_feature = Vec::new();
    let mut total = 0.0f64;
    for (idx, feature) in collection.features.iter().enumerate() {
        let value = match &feature.geometry {
            Some(g) => match kind {
                MeasureKind::Length => geodesic_length(&g.value),
                MeasureKind::Area => geodesic_area(&g.value),
            },
            None => 0.0,
        };
        total += value;
        per_feature.push(serde_json::json!({ "index": idx, "value": value }));
    }
    let (kind_name, unit) = match kind {
        MeasureKind::Length => ("length", "m"),
        MeasureKind::Area => ("area", "m²"),
    };
    Ok(serde_json::json!({
        "kind": kind_name,
        "unit": unit,
        "total": total,
        "per_feature": per_feature,
    }))
}

/// 测地线长度（米）：线取长度，面取外环周长，其余为 0。
fn geodesic_length(value: &geojson::Value) -> f64 {
    // geo 0.33 的 `GeodesicLength` trait 已弃用，走 `Geodesic` 度量空间 API。
    use geo::algorithm::line_measures::{Geodesic, Length};
    match geo_types::Geometry::<f64>::try_from(value) {
        Ok(geo_types::Geometry::LineString(l)) => Geodesic.length(&l),
        Ok(geo_types::Geometry::MultiLineString(m)) => Geodesic.length(&m),
        Ok(geo_types::Geometry::Polygon(p)) => Geodesic.length(p.exterior()),
        Ok(geo_types::Geometry::MultiPolygon(m)) => {
            m.iter().map(|p| Geodesic.length(p.exterior())).sum()
        }
        _ => 0.0,
    }
}

/// 测地线面积（平方米）：面取 unsigned 面积，其余为 0。
/// geo 的 geodesic 面积假设外环逆时针（SF 规范）；ESRI 系数据常为顺时针，
/// 先用 `Orient` 归一化绕向，避免算成"全球减负数"的补集面积。
fn geodesic_area(value: &geojson::Value) -> f64 {
    use geo::algorithm::orient::{Direction, Orient};
    use geo::GeodesicArea;
    match geo_types::Geometry::<f64>::try_from(value) {
        Ok(geo_types::Geometry::Polygon(p)) => {
            p.orient(Direction::Default).geodesic_area_unsigned()
        }
        Ok(geo_types::Geometry::MultiPolygon(m)) => {
            m.orient(Direction::Default).geodesic_area_unsigned()
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection_from_str(text: &str) -> geojson::FeatureCollection {
        let gj: geojson::GeoJson = text.parse().unwrap();
        geojson::FeatureCollection::try_from(gj).unwrap()
    }

    fn beijing_point() -> geojson::FeatureCollection {
        collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.3914,39.9072]},
                 "properties":{"name":"北京"}}
            ]}"#,
        )
    }

    #[test]
    fn reproject_4326_to_3857_matches_spherical_mercator() {
        let out = reproject(&beijing_point(), "EPSG:4326", "EPSG:3857").unwrap();
        let geojson::Value::Point(pos) = &out.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point 几何");
        };
        // 期望值：球面 Web 墨卡托公式（EPSG:3857 定义，R=6378137）：
        // x = R·λ，y = R·ln(tan(π/4 + φ/2))。
        let lambda = 116.3914f64.to_radians();
        let phi = 39.9072f64.to_radians();
        let expected_x = 6_378_137.0 * lambda;
        let expected_y = 6_378_137.0 * (std::f64::consts::FRAC_PI_4 + phi / 2.0).tan().ln();
        assert!(
            (pos[0] - expected_x).abs() < 1.0,
            "x {} 应接近期望 {}（±1m）",
            pos[0],
            expected_x
        );
        assert!(
            (pos[1] - expected_y).abs() < 1.0,
            "y {} 应接近期望 {}（±1m）",
            pos[1],
            expected_y
        );
        // 属性不受影响。
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["name"],
            serde_json::Value::String("北京".to_string())
        );
    }

    #[test]
    fn reproject_roundtrip_error_below_1e6() {
        let there = reproject(&beijing_point(), "EPSG:4326", "EPSG:3857").unwrap();
        let back = reproject(&there, "EPSG:3857", "EPSG:4326").unwrap();
        let geojson::Value::Point(pos) = &back.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point 几何");
        };
        assert!((pos[0] - 116.3914).abs() < 1e-6, "往返 x 误差: {}", pos[0]);
        assert!((pos[1] - 39.9072).abs() < 1e-6, "往返 y 误差: {}", pos[1]);
        // from == to 原样返回。
        let same = reproject(&beijing_point(), "epsg:4326", "EPSG:4326").unwrap();
        assert_eq!(same, beijing_point());
    }

    #[test]
    fn reproject_rejects_bad_crs_with_chinese_error() {
        let err = reproject(&beijing_point(), "EPSG:4326", "EPSG:foo").unwrap_err();
        assert!(
            err.to_string().contains("无法解析 CRS 定义"),
            "错误应指出 CRS 解析问题: {err}"
        );
    }

    #[test]
    fn reproject_polygon_roundtrip_preserves_ring_coordinates() {
        // 多边形（含内环）往返：顶点数不变、坐标误差 < 1e-6。
        let poly = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                    [[116.0,39.0],[116.0,40.0],[117.0,40.0],[117.0,39.0],[116.0,39.0]],
                    [[116.2,39.2],[116.2,39.4],[116.4,39.4],[116.4,39.2],[116.2,39.2]]
                ]},"properties":{}}
            ]}"#,
        );
        let there = reproject(&poly, "EPSG:4326", "EPSG:3857").unwrap();
        let back = reproject(&there, "EPSG:3857", "EPSG:4326").unwrap();
        let geojson::Value::Polygon(rings) = &back.features[0].geometry.as_ref().unwrap().value
        else {
            panic!("应为 Polygon 几何");
        };
        assert_eq!(rings.len(), 2, "内环不得丢失");
        assert_eq!(rings[0].len(), 5, "外环顶点数不得变化: {:?}", rings[0]);
        assert_eq!(rings[1].len(), 5, "内环顶点数不得变化: {:?}", rings[1]);
        assert!((rings[0][0][0] - 116.0).abs() < 1e-6);
        assert!((rings[0][0][1] - 39.0).abs() < 1e-6);
    }

    #[test]
    fn measure_geodesic_area_of_unit_square_at_equator() {
        // 赤道 1°×1° 方格：geodesic 面积 ≈ 111.2km × 111.2km（±2%）。
        let square = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                    [[0,0],[0,1],[1,1],[1,0],[0,0]]]},"properties":{}}
            ]}"#,
        );
        let report = measure(&square, MeasureKind::Area).unwrap();
        assert_eq!(report["unit"], "m²");
        let expected = 111_195.0f64 * 111_195.0;
        let total = report["total"].as_f64().unwrap();
        assert!(
            (total - expected).abs() / expected < 0.02,
            "赤道 1°×1° 面积 {total} 应接近 {expected}（±2%）"
        );
        assert_eq!(report["per_feature"][0]["index"], 0);
        assert!((report["per_feature"][0]["value"].as_f64().unwrap() - total).abs() < 1e-6);
    }

    #[test]
    fn measure_geodesic_length_of_equator_segment() {
        // 赤道 1° 线段：geodesic 长度 ≈ 111.2km（±1%）；Point 为 0。
        let line = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,0]]},
                 "properties":{}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},
                 "properties":{}}
            ]}"#,
        );
        let report = measure(&line, MeasureKind::Length).unwrap();
        assert_eq!(report["unit"], "m");
        assert_eq!(report["kind"], "length");
        let v0 = report["per_feature"][0]["value"].as_f64().unwrap();
        assert!(
            (v0 - 111_195.0).abs() / 111_195.0 < 0.01,
            "赤道 1° 长度 {v0} 应接近 111195m（±1%）"
        );
        assert_eq!(report["per_feature"][1]["value"], 0.0);
    }

    #[test]
    fn measure_kind_parse_chinese_error() {
        assert!("length".parse::<MeasureKind>().is_ok());
        assert!("AREA".parse::<MeasureKind>().is_ok());
        assert!("bogus"
            .parse::<MeasureKind>()
            .unwrap_err()
            .to_string()
            .contains("度量类型"));
    }
}
