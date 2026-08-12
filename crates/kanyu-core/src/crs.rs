//! 坐标参考系（CRS）工具：投影变换与测地线度量。
//!
//! 投影基于 proj4rs（纯 Rust PROJ 改写）+ crs-definitions EPSG 数据库；
//! 测地线度量基于 geo crate（Karney 2013 算法，单位为米/平方米）。
//! 与 [`crate::analysis`] 配套：EPSG:4326 数据先 `reproject` 到米制 CRS
//! 再做 buffer/overlay，或直接对经纬度数据做 `measure` 测地线度量。
//!
//! **轴序约定**：proj4rs 是 PROJ4 风格改写，EPSG 定义经 proj4 串解析、
//! 不携带官方轴序——即使官方轴序为纬度在前的 CRS（如 EPSG:4490），
//! 本模块输入/输出也一律为 GIS 序（经度 lon 在前、纬度 lat 在后），
//! 与 GeoJSON（RFC 7946）一致。实测 `EPSG:4490` 与 `EPSG:4326` 对同一
//! (lon, lat) 点转 3857 结果一致（差异 < 1mm，仅椭球参数 GRS80/WGS84
//! 末位差别），见 `tests::reproject_4490_matches_4326_to_3857`。
//!
//! **EPSG 覆盖**：crs-definitions 0.4.0 内置 7507 条定义（代码域
//! 2000..=32766），经 [`search_crs`]/[`crs_info`] 全库检索；
//! 无独立名称字段，名称取自 WKT 首段引号串。

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

/// 校验 CRS 定义可解析（设置对话框手动输入的预检入口；与 [`reproject`] 同一解析器）。
pub fn validate_crs(def: &str) -> Result<()> {
    parse_crs(def).map(|_| ())
}

/// CRS 类型（EPSG 条目分类，按 proj4 串 `+proj=` 判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CrsKind {
    /// 地理坐标系（经纬度，如 EPSG:4326/4490）。
    Geographic,
    /// 投影坐标系（如 EPSG:3857/4527）。
    Projected,
    /// 其他（地心等非常规投影条目）。
    Other,
}

/// EPSG 条目信息（[`search_crs`] / [`crs_info`] 返回）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrsInfo {
    /// EPSG 代码。
    pub code: u32,
    /// 官方名称（取自 WKT 首段引号串，如 "China Geodetic Coordinate System 2000"）。
    pub name: String,
    /// 类型（地理/投影/其他）。
    pub kind: CrsKind,
    /// 单位（中文友好：地理 CRS 为 "度"；投影 CRS 解析 proj4 串
    /// `+units=`/`+to_meter=`，常见值映射 "米"/"千米"/"英尺" 等）。
    pub unit: String,
}

/// 常用精选代码（search_crs 空查询返回，库中缺失的自动跳过）。
const COMMON_CRS: &[u16] = &[4326, 3857, 4490, 4526, 4527, 4610, 4214, 32650, 4547];

/// 按代码查询 EPSG 条目；库中不存在返回 None。
pub fn crs_info(code: u32) -> Option<CrsInfo> {
    let def = crs_definitions::from_code(u16::try_from(code).ok()?)?;
    Some(info_from_def(&def))
}

/// 按代码取 EPSG 条目的 proj4 定义串（MCP `kanyu://crs/{code}` 资源等需要
/// 原始定义的消费方）；库中不存在返回 None。
pub fn crs_proj4_def(code: u32) -> Option<&'static str> {
    let def = crs_definitions::from_code(u16::try_from(code).ok()?)?;
    Some(def.proj4)
}

/// EPSG 全库检索（crs-definitions 0.4.0，7507 条，代码域 2000..=32766）：
/// 按代码子串或名称（大小写不敏感）匹配，按代码升序返回，至多 `limit` 条；
/// 空查询返回常用精选（见 [`COMMON_CRS`]）。
pub fn search_crs(query: &str, limit: usize) -> Vec<CrsInfo> {
    let q = query.trim();
    if q.is_empty() {
        return COMMON_CRS
            .iter()
            .filter_map(|&c| crs_info(c as u32))
            .take(limit)
            .collect();
    }
    let q = q.to_lowercase();
    let mut out = Vec::new();
    for code in 2000u16..=32766 {
        if out.len() >= limit {
            break;
        }
        let Some(def) = crs_definitions::from_code(code) else {
            continue;
        };
        let info = info_from_def(&def);
        if code.to_string().contains(&q) || info.name.to_lowercase().contains(&q) {
            out.push(info);
        }
    }
    out
}

/// crs-definitions 条目 → CrsInfo。
fn info_from_def(def: &crs_definitions::Def) -> CrsInfo {
    CrsInfo {
        code: def.code as u32,
        name: wkt_name(def.wkt, def.code),
        kind: kind_of(def.proj4),
        unit: unit_of(def.proj4),
    }
}

/// WKT 首段引号串为 CRS 名称（WKT1 GEOGCS/PROJCS 与 WKT2 GEOGCRS/PROJCRS
/// 同为该结构）；解析失败回退 "EPSG:{code}"。
fn wkt_name(wkt: &str, code: u16) -> String {
    match wkt.split('"').nth(1) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => format!("EPSG:{code}"),
    }
}

/// 按 proj4 串 `+proj=` 判定类型：longlat → 地理；geocent → 其他；其余 → 投影。
fn kind_of(proj4: &str) -> CrsKind {
    if proj4.split_whitespace().any(|t| t == "+proj=longlat") {
        CrsKind::Geographic
    } else if proj4.split_whitespace().any(|t| t == "+proj=geocent") {
        CrsKind::Other
    } else {
        CrsKind::Projected
    }
}

/// 单位中文友好映射：地理 CRS "度"；投影 CRS 解析 `+units=`（缺省为米，
/// PROJ4 默认），无 `+units=` 但有 `+to_meter=` 报换算系数。
fn unit_of(proj4: &str) -> String {
    if kind_of(proj4) == CrsKind::Geographic {
        return "度".to_string();
    }
    for token in proj4.split_whitespace() {
        if let Some(units) = token.strip_prefix("+units=") {
            return match units {
                "m" => "米".to_string(),
                "km" => "千米".to_string(),
                "cm" => "厘米".to_string(),
                "mm" => "毫米".to_string(),
                "ft" => "英尺".to_string(),
                "us-ft" => "美制英尺".to_string(),
                "mi" => "英里".to_string(),
                "yd" => "码".to_string(),
                "in" => "英寸".to_string(),
                "nmi" | "kmi" => "海里".to_string(),
                other => other.to_string(),
            };
        }
    }
    for token in proj4.split_whitespace() {
        if let Some(to_meter) = token.strip_prefix("+to_meter=") {
            return format!("自定义单位（1 单位 = {to_meter} 米）");
        }
    }
    "米".to_string()
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
    fn validate_crs_entry() {
        assert!(validate_crs("EPSG:4326").is_ok());
        assert!(validate_crs("EPSG:4490").is_ok());
        assert!(validate_crs("WGS84").is_ok());
        let err = validate_crs("EPSG:foo").unwrap_err();
        assert!(err.to_string().contains("无法解析 CRS 定义"));
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

    #[test]
    fn crs_info_known_entries() {
        let wgs84 = crs_info(4326).unwrap();
        assert_eq!(wgs84.name, "WGS 84");
        assert_eq!(wgs84.kind, CrsKind::Geographic);
        assert_eq!(wgs84.unit, "度");
        let cgcs2000 = crs_info(4490).unwrap();
        assert_eq!(cgcs2000.name, "China Geodetic Coordinate System 2000");
        assert_eq!(cgcs2000.kind, CrsKind::Geographic);
        let zone39 = crs_info(4527).unwrap();
        assert_eq!(zone39.name, "CGCS2000 / 3-degree Gauss-Kruger zone 39");
        assert_eq!(zone39.kind, CrsKind::Projected);
        assert_eq!(zone39.unit, "米");
        // 美制英尺投影（NAD83 / Texas South Central）。
        let texas = crs_info(2277).unwrap();
        assert_eq!(texas.unit, "美制英尺");
        // 越界代码：u16 域外或库中不存在。
        assert!(crs_info(65536).is_none());
        assert!(crs_info(9999).is_none());
    }

    #[test]
    fn search_crs_by_code_substring_and_name() {
        // 代码子串。
        let by_code = search_crs("4527", 10);
        assert!(by_code.iter().any(|c| c.code == 4527));
        assert!(by_code.len() <= 10);
        // 名称大小写不敏感。
        let by_name = search_crs("cgcs2000", 5);
        assert_eq!(by_name.len(), 5);
        assert!(by_name
            .iter()
            .all(|c| c.name.to_lowercase().contains("cgcs2000")));
        // 空查询返回常用精选，代码与清单一致（库中缺失自动跳过）。
        let common = search_crs("", 20);
        assert!(common.len() >= 7);
        assert_eq!(common[0].code, 4326);
        assert!(common.iter().any(|c| c.code == 3857));
        assert!(common.iter().any(|c| c.code == 4490));
        // limit = 0 得空。
        assert!(search_crs("4326", 0).is_empty());
    }

    #[test]
    fn reproject_4490_matches_4326_to_3857() {
        // 轴序约定实测：proj4rs 按 GIS 序（lon, lat）解释 EPSG:4490 输入，
        // 与 EPSG:4326 同点转换结果一致（< 1mm；CGCS2000/GRS80 与 WGS84
        // 椭球差异在该精度内）。
        let a = reproject(&beijing_point(), "EPSG:4490", "EPSG:3857").unwrap();
        let b = reproject(&beijing_point(), "EPSG:4326", "EPSG:3857").unwrap();
        let geojson::Value::Point(pa) = &a.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point")
        };
        let geojson::Value::Point(pb) = &b.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point")
        };
        assert!((pa[0] - pb[0]).abs() < 0.001, "x 差 {}", pa[0] - pb[0]);
        assert!((pa[1] - pb[1]).abs() < 0.001, "y 差 {}", pa[1] - pb[1]);
    }

    #[test]
    fn reproject_4490_to_4527_gauss_kruger_zone39() {
        // 北京 (116.3914, 39.9072) → CGCS2000 3°带 39 带（中央经线 117°E，
        // 假东 39500000）：实测 x = 39447958.84（偏西 0.6086° ≈ −52.0km），
        // y = 4419402.42（±1m 断言，proj4rs tmerc 实测值）。
        let out = reproject(&beijing_point(), "EPSG:4490", "EPSG:4527").unwrap();
        let geojson::Value::Point(pos) = &out.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point")
        };
        assert!(
            (pos[0] - 39_447_958.84).abs() < 1.0,
            "x {} 应为 39447958.84（±1m）",
            pos[0]
        );
        assert!(
            (pos[1] - 4_419_402.42).abs() < 1.0,
            "y {} 应为 4419402.42（±1m）",
            pos[1]
        );
        // 往返误差 < 1e-6°。
        let back = reproject(&out, "EPSG:4527", "EPSG:4490").unwrap();
        let geojson::Value::Point(p) = &back.features[0].geometry.as_ref().unwrap().value else {
            panic!("应为 Point")
        };
        assert!((p[0] - 116.3914).abs() < 1e-6);
        assert!((p[1] - 39.9072).abs() < 1e-6);
    }

    #[test]
    fn epsg_library_coverage_spot_check() {
        // CGCS2000：4490–4513（地理 + 高斯系列）、4526–4533（3°带 38–45 带）。
        for code in 4490..=4513 {
            assert!(crs_info(code).is_some(), "EPSG:{code} 缺失");
        }
        for code in 4526..=4533 {
            assert!(crs_info(code).is_some(), "EPSG:{code} 缺失");
        }
        // 北京54：4214（Beijing 1954 地理，krass 椭球）在库；
        // 注意 4547 实为 CGCS2000 / 3°带 CM 114E（非北京54），名称断言之。
        assert_eq!(crs_info(4214).unwrap().name, "Beijing 1954");
        assert!(crs_info(4547).unwrap().name.contains("CGCS2000"));
        // 西安80：4610（Xian 1980 地理，IAU76 椭球）可解析可转换。
        assert_eq!(crs_info(4610).unwrap().name, "Xian 1980");
        assert!(validate_crs("EPSG:4610").is_ok());
        // Web 墨卡托：3857 在库；900913 超出 u16 代码域（2000..=32766），库无此别名。
        assert_eq!(crs_info(3857).unwrap().name, "WGS 84 / Pseudo-Mercator");
        assert!(crs_info(900913).is_none());
        // 全库条目总数 = 7507（crs-definitions 0.4.0，代码域 2000..=32766）。
        let total = (2000u16..=32766)
            .filter(|&c| crs_definitions::from_code(c).is_some())
            .count();
        assert_eq!(total, 7507, "EPSG 库条目总数变动");
    }
}
