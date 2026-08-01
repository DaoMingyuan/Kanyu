//! 空间分析内核：buffer / overlay / topology（geo crate，纯 Rust、零 C 依赖）。
//!
//! 对应总规 §4.2.2 的 MCP 分析工具组（裁决 #16：分析内核优先于 UI 壳层）。
//! 三个函数均以 GeoJSON FeatureCollection 为边界格式，与 [`crate::layer`]
//! 的导出管线同构——分析结果可直接走任意 `Layer::to_*` 序列化器。
//!
//! **坐标单位警示**：所有距离/面积均以数据 CRS 单位计。EPSG:4326 下
//! distance 是"度"而非米；米制缓冲/面积请先投影（proj4rs 投影工具为后续迭代）。

use crate::error::{KanyuError, Result};

/// geojson 几何 → geo 几何（z 丢弃；不支持的类型返回 None）。
fn to_geo(value: &geojson::Value) -> Option<geo_types::Geometry<f64>> {
    geo_types::Geometry::<f64>::try_from(value).ok()
}

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

/// 缓冲区分析：逐要素 geojson→geo 转换并缓冲，结果为 Polygon/MultiPolygon，
/// **属性随行**（原 properties 复制到缓冲结果要素）。
///
/// - `distance`：缓冲距离（数据 CRS 单位；EPSG:4326 下是度）。
/// - `segments`：圆弧拟合的每象限分段数（≥1；越大越圆滑，对应 geo 圆角
///   连接角 `π/2 / segments`）。
/// - 几何缺失或类型不支持转换的要素跳过，跳过数计入返回集合的
///   `foreign_members.skipped`（无跳过时该键缺省）。
pub fn buffer(
    collection: &geojson::FeatureCollection,
    distance: f64,
    segments: usize,
) -> Result<geojson::FeatureCollection> {
    use geo::algorithm::buffer::{Buffer, BufferStyle, LineCap, LineJoin};

    if segments == 0 {
        return Err(KanyuError::Other("segments 必须 >= 1".to_string()));
    }
    if !distance.is_finite() {
        return Err(KanyuError::Other(format!(
            "缓冲距离必须为有限数值，实际: {distance}"
        )));
    }
    let angle = std::f64::consts::FRAC_PI_2 / segments as f64;
    let style = BufferStyle::new(distance)
        .line_join(LineJoin::Round(angle))
        .line_cap(LineCap::Round(angle));

    let mut features = Vec::new();
    let mut skipped = 0usize;
    for feature in &collection.features {
        let buffered = feature
            .geometry
            .as_ref()
            .and_then(|g| to_geo(&g.value))
            .map(|g| g.buffer_with_style(style.clone()));
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

/// 叠加操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayOp {
    /// 并集。
    Union,
    /// 交集。
    Intersection,
    /// 差集（target − overlay）。
    Difference,
    /// 对称差。
    Xor,
}

impl std::str::FromStr for OverlayOp {
    type Err = KanyuError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "union" => Ok(Self::Union),
            "intersection" => Ok(Self::Intersection),
            "difference" => Ok(Self::Difference),
            "xor" => Ok(Self::Xor),
            other => Err(KanyuError::Other(format!(
                "未知叠加操作 '{other}'（支持 union/intersection/difference/xor）"
            ))),
        }
    }
}

/// 叠加分析：仅接受 Polygon/MultiPolygon 要素（其余类型遇到即报错并指出
/// 要素序号——叠加分析的面约束）。
///
/// 语义（v0.4 诚实简化，rustdoc 即契约）：
/// - Union/Intersection/Xor：target × overlay 逐要素对（笛卡尔积）布尔，
///   每个非空结果为一个要素；**未做跨对融合（dissolve）**，与 QGIS 的
///   "union 后自动融合"语义有差异。
/// - Difference：每个 target 要素依次减去全部 overlay 要素（连续差），
///   每 target 至多一个结果要素。
/// - 属性：target 要素属性 + overlay 要素属性（键冲突时 overlay 侧键加
///   `overlay_` 前缀）；Difference 结果仅带 target 属性（overlay 部分已被减去）。
pub fn overlay(
    target: &geojson::FeatureCollection,
    overlay: &geojson::FeatureCollection,
    op: OverlayOp,
) -> Result<geojson::FeatureCollection> {
    use geo::BooleanOps;

    let targets = collect_polygons(target, "target")?;
    let overlays = collect_polygons(overlay, "overlay")?;

    let mut features = Vec::new();
    match op {
        OverlayOp::Difference => {
            for t in &targets {
                let mut acc = t.geom.clone();
                for o in &overlays {
                    acc = acc.difference(&o.geom);
                }
                if !acc.0.is_empty() {
                    features.push(with_geometry(t.feature, geojson::Value::from(&acc)));
                }
            }
        }
        _ => {
            for t in &targets {
                for o in &overlays {
                    let result = match op {
                        OverlayOp::Union => t.geom.union(&o.geom),
                        OverlayOp::Intersection => t.geom.intersection(&o.geom),
                        OverlayOp::Xor => t.geom.xor(&o.geom),
                        OverlayOp::Difference => unreachable!("已在上分支处理"),
                    };
                    if result.0.is_empty() {
                        continue;
                    }
                    let mut feature = with_geometry(t.feature, geojson::Value::from(&result));
                    merge_overlay_props(&mut feature, o.feature);
                    features.push(feature);
                }
            }
        }
    }

    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// 面要素引用（要素 + geo MultiPolygon）。
struct PolygonFeature<'a> {
    feature: &'a geojson::Feature,
    geom: geo_types::MultiPolygon<f64>,
}

/// 收集 Polygon/MultiPolygon 要素；其余类型报中文错误并指出要素序号。
fn collect_polygons<'a>(
    collection: &'a geojson::FeatureCollection,
    side: &str,
) -> Result<Vec<PolygonFeature<'a>>> {
    let mut out = Vec::new();
    for (idx, feature) in collection.features.iter().enumerate() {
        let Some(geom) = &feature.geometry else {
            return Err(KanyuError::Other(format!(
                "叠加分析仅接受面要素：{side} 第 {} 个要素无几何",
                idx + 1
            )));
        };
        let mp = match geo_types::Geometry::<f64>::try_from(&geom.value) {
            Ok(geo_types::Geometry::Polygon(p)) => geo_types::MultiPolygon(vec![p]),
            Ok(geo_types::Geometry::MultiPolygon(mp)) => mp,
            _ => {
                return Err(KanyuError::Other(format!(
                    "叠加分析仅接受 Polygon/MultiPolygon：{side} 第 {} 个要素为 {}",
                    idx + 1,
                    geom.value.type_name()
                )))
            }
        };
        out.push(PolygonFeature { feature, geom: mp });
    }
    Ok(out)
}

/// 把 overlay 要素属性并入结果：键冲突时 overlay 侧键加 `overlay_` 前缀。
fn merge_overlay_props(result: &mut geojson::Feature, overlay: &geojson::Feature) {
    let Some(overlay_props) = &overlay.properties else {
        return;
    };
    let props = result.properties.get_or_insert_with(serde_json::Map::new);
    for (key, value) in overlay_props {
        let key = if props.contains_key(key) {
            format!("overlay_{key}")
        } else {
            key.clone()
        };
        props.insert(key, value.clone());
    }
}

/// 拓扑检查规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyRule {
    /// 面要素不得重叠（交集面积 > 1e-10 判违规）。
    NoOverlap,
}

impl std::str::FromStr for TopologyRule {
    type Err = KanyuError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "no_overlap" => Ok(Self::NoOverlap),
            other => Err(KanyuError::Other(format!(
                "未知拓扑规则 '{other}'（支持 no_overlap）"
            ))),
        }
    }
}

/// 单条拓扑违规。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TopologyViolation {
    /// 违规要素 A 在输入集合中的序号（0 起）。
    pub feature_a: usize,
    /// 违规要素 B 在输入集合中的序号（0 起）。
    pub feature_b: usize,
    /// 违规说明（如重叠面积）。
    pub note: String,
}

/// 拓扑检查报告（MCP `kanyu_analysis_topology` 与 CLI `--json` 的返回形状）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TopologyReport {
    /// 执行的规则（逗号分隔）。
    pub rule: String,
    /// 输入要素总数。
    pub feature_count: usize,
    /// 违规条数。
    pub violation_count: usize,
    /// 违规明细。
    pub violations: Vec<TopologyViolation>,
}

/// 拓扑检查：逐规则执行。NoOverlap 对面要素两两判定（intersects 粗筛 +
/// 交集面积 > 1e-10 确认）；非面要素在该规则下跳过。
///
/// O(n²) 朴素实现——rstar 空间索引加速为后续优化（万级要素以下无感）。
pub fn topology_check(
    collection: &geojson::FeatureCollection,
    rules: &[TopologyRule],
) -> Result<TopologyReport> {
    use geo::{Area, BooleanOps, Intersects};

    if rules.is_empty() {
        return Err(KanyuError::Other(
            "拓扑检查至少需指定一条规则（支持 no_overlap）".to_string(),
        ));
    }
    let mut violations = Vec::new();
    for rule in rules {
        match rule {
            TopologyRule::NoOverlap => {
                let polys: Vec<(usize, geo_types::MultiPolygon<f64>)> = collection
                    .features
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, f)| {
                        let geom = f.geometry.as_ref()?;
                        match geo_types::Geometry::<f64>::try_from(&geom.value).ok()? {
                            geo_types::Geometry::Polygon(p) => {
                                Some((idx, geo_types::MultiPolygon(vec![p])))
                            }
                            geo_types::Geometry::MultiPolygon(mp) => Some((idx, mp)),
                            _ => None,
                        }
                    })
                    .collect();
                for i in 0..polys.len() {
                    for j in (i + 1)..polys.len() {
                        let (ia, ga) = &polys[i];
                        let (ib, gb) = &polys[j];
                        if !ga.intersects(gb) {
                            continue;
                        }
                        let area = ga.intersection(gb).unsigned_area();
                        if area > 1e-10 {
                            violations.push(TopologyViolation {
                                feature_a: *ia,
                                feature_b: *ib,
                                note: format!("面要素重叠，交集面积 {area:.6}"),
                            });
                        }
                    }
                }
            }
        }
    }

    let rule_names: Vec<&'static str> = rules
        .iter()
        .map(|r| match r {
            TopologyRule::NoOverlap => "no_overlap",
        })
        .collect();
    Ok(TopologyReport {
        rule: rule_names.join(","),
        feature_count: collection.features.len(),
        violation_count: violations.len(),
        violations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection_from_str(text: &str) -> geojson::FeatureCollection {
        let gj: geojson::GeoJson = text.parse().unwrap();
        geojson::FeatureCollection::try_from(gj).unwrap()
    }

    fn square(x0: f64, y0: f64, x1: f64, y1: f64, name: &str) -> String {
        format!(
            r#"{{"type":"Feature","geometry":{{"type":"Polygon","coordinates":[
                [[{x0},{y0}],[{x0},{y1}],[{x1},{y1}],[{x1},{y0}],[{x0},{y0}]]]}},
             "properties":{{"name":"{name}"}}}}"#
        )
    }

    fn total_area(collection: &geojson::FeatureCollection) -> f64 {
        use geo::Area;
        collection
            .features
            .iter()
            .filter_map(|f| f.geometry.as_ref())
            .filter_map(|g| geo_types::Geometry::<f64>::try_from(&g.value).ok())
            .map(|g| g.unsigned_area())
            .sum()
    }

    #[test]
    fn buffer_point_yields_polygon_with_expected_area() {
        let collection = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[5,5]},
                 "properties":{"name":"甲","height":80}}
            ]}"#,
        );
        let out = buffer(&collection, 3.0, 16).unwrap();
        assert_eq!(out.features.len(), 1);
        let result = &out.features[0];
        assert!(matches!(
            result.geometry.as_ref().unwrap().value,
            geojson::Value::Polygon(_) | geojson::Value::MultiPolygon(_)
        ));
        // 面积 ≈ πr²（±5%）。
        let area = total_area(&out);
        let expected = std::f64::consts::PI * 9.0;
        assert!(
            (area - expected).abs() / expected < 0.05,
            "缓冲面积 {area} 应接近 πr²={expected}"
        );
        // 属性随行。
        let props = result.properties.as_ref().unwrap();
        assert_eq!(props["name"], serde_json::Value::String("甲".to_string()));
        assert_eq!(props["height"].as_f64(), Some(80.0));
    }

    #[test]
    fn overlay_intersection_and_union_areas() {
        let target = collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            square(0.0, 0.0, 4.0, 4.0, "a")
        ));
        let over = collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            square(2.0, 2.0, 6.0, 6.0, "b")
        ));
        // 交集 = 2×2 = 4。
        let inter = overlay(&target, &over, OverlayOp::Intersection).unwrap();
        assert_eq!(inter.features.len(), 1);
        assert!((total_area(&inter) - 4.0).abs() < 1e-9);
        // 并集 = 16 + 16 − 4 = 28。
        let union = overlay(&target, &over, OverlayOp::Union).unwrap();
        assert!((total_area(&union) - 28.0).abs() < 1e-9);
        // 差集 = 16 − 4 = 12，属性仅 target 侧。
        let diff = overlay(&target, &over, OverlayOp::Difference).unwrap();
        assert_eq!(diff.features.len(), 1);
        assert!((total_area(&diff) - 12.0).abs() < 1e-9);
        // 交集属性：target 属性 + overlay 属性合并。
        let props = inter.features[0].properties.as_ref().unwrap();
        assert_eq!(props["name"], serde_json::Value::String("a".to_string()));
        assert_eq!(
            props["overlay_name"],
            serde_json::Value::String("b".to_string())
        );
    }

    #[test]
    fn overlay_rejects_non_polygon_features() {
        let points = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},
                 "properties":{}}
            ]}"#,
        );
        let err = overlay(&points, &points, OverlayOp::Union).unwrap_err();
        assert!(
            err.to_string().contains("第 1 个要素"),
            "错误应指出序号: {err}"
        );
    }

    #[test]
    fn topology_no_overlap_reports_violating_pair() {
        let collection = collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{},{},{}]}}"#,
            square(0.0, 0.0, 4.0, 4.0, "a"),
            square(2.0, 2.0, 6.0, 6.0, "b"),     // 与 a 重叠
            square(10.0, 10.0, 12.0, 12.0, "c")  // 独立
        ));
        let report = topology_check(&collection, &[TopologyRule::NoOverlap]).unwrap();
        assert_eq!(report.feature_count, 3);
        assert_eq!(report.violation_count, 1);
        assert_eq!(report.violations[0].feature_a, 0);
        assert_eq!(report.violations[0].feature_b, 1);

        let disjoint = collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{},{}]}}"#,
            square(0.0, 0.0, 2.0, 2.0, "a"),
            square(10.0, 10.0, 12.0, 12.0, "b")
        ));
        let clean = topology_check(&disjoint, &[TopologyRule::NoOverlap]).unwrap();
        assert_eq!(clean.violation_count, 0);
    }

    #[test]
    fn overlay_op_and_rule_parse_chinese_errors() {
        assert!("union".parse::<OverlayOp>().is_ok());
        assert!("bogus"
            .parse::<OverlayOp>()
            .unwrap_err()
            .to_string()
            .contains("叠加操作"));
        assert!("bogus"
            .parse::<TopologyRule>()
            .unwrap_err()
            .to_string()
            .contains("拓扑规则"));
    }
}
