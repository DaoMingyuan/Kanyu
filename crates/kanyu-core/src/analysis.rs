//! 空间分析内核：buffer / overlay / topology（geo crate，纯 Rust、零 C 依赖）。
//!
//! 对应总规 §4.2.2 的 MCP 分析工具组（裁决 #16：分析内核优先于 UI 壳层）。
//! 三个函数均以 GeoJSON FeatureCollection 为边界格式，与 [`crate::layer`]
//! 的导出管线同构——分析结果可直接走任意 `Layer::to_*` 序列化器。
//!
//! **坐标单位警示**：所有距离/面积均以数据 CRS 单位计。EPSG:4326 下
//! distance 是"度"而非米；米制缓冲/面积请先用 [`crate::crs::reproject`]
//! 投影到米制 CRS，或对经纬度数据用 [`crate::crs::measure`] 做测地线度量。

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
/// - `distance`：缓冲距离（数据 CRS 单位；EPSG:4326 下是度，
///   米制缓冲请先用 [`crate::crs::reproject`] 投影到米制 CRS）。
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
///
/// **性能（rstar 裁剪，§8.1 复测路线项）**：要素对数 ≥ `SPATIAL_INDEX_MIN_PAIRS`
/// 时启用空间索引/包矩形裁剪，**语义与朴素版完全一致**（裁剪而非近似，
/// 对拍测试见 `tests::overlay_indexed_matches_naive`）：
/// - Intersection/Difference：包矩形相交是产生影响（非空交集/差集被改变）的
///   必要条件——overlay 侧建 R 树，只对候选对进精确布尔；
/// - Union/Xor：不相交要素对同样产出（两者简单拼合），索引无法减少产出数，
///   逐对包矩形判定：不相交走拼合直通（跳过布尔管线），相交才进精确布尔。
///
/// 低于阈值走朴素路径（建树开销不抵）。
pub fn overlay(
    target: &geojson::FeatureCollection,
    overlay: &geojson::FeatureCollection,
    op: OverlayOp,
) -> Result<geojson::FeatureCollection> {
    let targets = collect_polygons(target, "target")?;
    let overlays = collect_polygons(overlay, "overlay")?;
    let features = if targets.len() * overlays.len() < SPATIAL_INDEX_MIN_PAIRS {
        overlay_naive(&targets, &overlays, op)
    } else {
        overlay_indexed(&targets, &overlays, op)
    };
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// 朴素笛卡尔积实现（小数据集路径 + 对拍基准；v0.4 原始语义）。
fn overlay_naive(
    targets: &[PolygonFeature],
    overlays: &[PolygonFeature],
    op: OverlayOp,
) -> Vec<geojson::Feature> {
    use geo::BooleanOps;
    let mut features = Vec::new();
    match op {
        OverlayOp::Difference => {
            for t in targets {
                let mut acc = t.geom.clone();
                for o in overlays {
                    acc = acc.difference(&o.geom);
                }
                if !acc.0.is_empty() {
                    features.push(with_geometry(t.feature, geojson::Value::from(&acc)));
                }
            }
        }
        _ => {
            for t in targets {
                for o in overlays {
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
    features
}

/// 索引裁剪实现（语义与 [`overlay_naive`] 完全一致，见 overlay rustdoc）。
fn overlay_indexed(
    targets: &[PolygonFeature],
    overlays: &[PolygonFeature],
    op: OverlayOp,
) -> Vec<geojson::Feature> {
    use geo::{BooleanOps, BoundingRect};
    let mut features = Vec::new();
    match op {
        OverlayOp::Difference => {
            let tree = BboxTree::build(overlays.iter().map(|o| o.geom.bounding_rect()));
            let mut candidates = Vec::new();
            for t in targets {
                let mut acc = t.geom.clone();
                // 仅包矩形相交的 overlay 可能改变差集（其余差运算为恒等）。
                tree.candidates_into(t.geom.bounding_rect(), &mut candidates);
                for &o_idx in &candidates {
                    acc = acc.difference(&overlays[o_idx].geom);
                }
                if !acc.0.is_empty() {
                    features.push(with_geometry(t.feature, geojson::Value::from(&acc)));
                }
            }
        }
        OverlayOp::Intersection => {
            let tree = BboxTree::build(overlays.iter().map(|o| o.geom.bounding_rect()));
            let mut candidates = Vec::new();
            for t in targets {
                // 包矩形不相交 ⇒ 交集必空（朴素版同样跳过），只算候选对。
                tree.candidates_into(t.geom.bounding_rect(), &mut candidates);
                for &o_idx in &candidates {
                    let o = &overlays[o_idx];
                    let result = t.geom.intersection(&o.geom);
                    if result.0.is_empty() {
                        continue;
                    }
                    let mut feature = with_geometry(t.feature, geojson::Value::from(&result));
                    merge_overlay_props(&mut feature, o.feature);
                    features.push(feature);
                }
            }
        }
        OverlayOp::Union | OverlayOp::Xor => {
            // 不相交对的并/对称差 = 两者简单拼合（集合恒等）：包矩形逐对判定，
            // 不相交直通拼合，相交才进布尔管线。
            for t in targets {
                let tb = t.geom.bounding_rect();
                for o in overlays {
                    let ob = o.geom.bounding_rect();
                    let result = match (tb, ob) {
                        (Some(tb), Some(ob)) if !rects_intersect(&tb, &ob) => {
                            disjoint_concat(&t.geom, &o.geom)
                        }
                        (Some(_), Some(_)) => match op {
                            OverlayOp::Union => t.geom.union(&o.geom),
                            OverlayOp::Xor => t.geom.xor(&o.geom),
                            _ => unreachable!("仅 Union/Xor 进入本分支"),
                        },
                        // 一侧为空 MultiPolygon：并/对称差 = 另一侧原样。
                        (Some(_), None) => t.geom.clone(),
                        (None, _) => o.geom.clone(),
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
    features
}

/// 包矩形相交（含边界接触；与 rstar 信封判定同口径）。
fn rects_intersect(a: &geo_types::Rect<f64>, b: &geo_types::Rect<f64>) -> bool {
    a.min().x <= b.max().x
        && a.max().x >= b.min().x
        && a.min().y <= b.max().y
        && a.max().y >= b.min().y
}

/// 不相交 MultiPolygon 的简单拼合（Union/Xor 在包矩形不相交时的集合恒等式）。
fn disjoint_concat(
    a: &geo_types::MultiPolygon<f64>,
    b: &geo_types::MultiPolygon<f64>,
) -> geo_types::MultiPolygon<f64> {
    let mut polys = Vec::with_capacity(a.0.len() + b.0.len());
    polys.extend(a.0.iter().cloned());
    polys.extend(b.0.iter().cloned());
    geo_types::MultiPolygon(polys)
}

/// 空间索引阈值：要素对数低于此走朴素路径（建树开销不抵；语义两侧一致）。
const SPATIAL_INDEX_MIN_PAIRS: usize = 1000;

/// sjoin 索引路径的 join 侧最小规模：join 侧过小时谓词全扫比建树查询更便宜
/// （实测依据见 sjoin rustdoc）。
const SJOIN_INDEX_MIN_JOIN: usize = 64;

/// rstar 索引项（要素序号 + 外包矩形信封）。
struct BboxItem {
    index: usize,
    bbox: rstar::AABB<[f64; 2]>,
}

impl rstar::RTreeObject for BboxItem {
    type Envelope = rstar::AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.bbox
    }
}

/// 单侧图层的包矩形 R 树（候选筛选：返回包矩形相交的要素序号，升序）。
struct BboxTree(rstar::RTree<BboxItem>);

impl BboxTree {
    /// 以各要素外包矩形建树（无外包矩形的空几何不入门——其候选恒空，
    /// 与朴素路径的谓词全不命中一致）。
    fn build(rects: impl Iterator<Item = Option<geo_types::Rect<f64>>>) -> Self {
        let items = rects
            .enumerate()
            .filter_map(|(index, r)| {
                r.map(|r| BboxItem {
                    index,
                    bbox: rstar::AABB::from_corners([r.min().x, r.min().y], [r.max().x, r.max().y]),
                })
            })
            .collect();
        Self(rstar::RTree::bulk_load(items))
    }

    /// 查询候选（包矩形相交的要素序号，升序——保持朴素路径的输出序）。
    /// 缓冲由调用方复用（避免逐 target 分配）。
    fn candidates_into(&self, rect: Option<geo_types::Rect<f64>>, buf: &mut Vec<usize>) {
        buf.clear();
        let Some(r) = rect else {
            return;
        };
        buf.extend(
            self.0
                .locate_in_envelope_intersecting(&rstar::AABB::from_corners(
                    [r.min().x, r.min().y],
                    [r.max().x, r.max().y],
                ))
                .map(|item| item.index),
        );
        buf.sort_unstable();
    }
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

/// 空间谓词（sjoin 用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialPredicate {
    /// 相交（任一公共点）。
    Intersects,
    /// target 包含 join。
    Contains,
    /// target 位于 join 内（join 包含 target）。
    Within,
}

impl std::str::FromStr for SpatialPredicate {
    type Err = KanyuError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "intersects" => Ok(Self::Intersects),
            "contains" => Ok(Self::Contains),
            "within" => Ok(Self::Within),
            other => Err(KanyuError::Other(format!(
                "未知空间谓词 '{other}'（支持 intersects/contains/within）"
            ))),
        }
    }
}

/// 空间连接：**左连接 + 匹配展开**语义（与 GeoPandas 默认 inner 不同——
/// 本实现保留全部 target 要素：无匹配时输出一条、join 侧属性缺省；
/// 一对多匹配时每个匹配各输出一条，rustdoc 即契约）。
///
/// 属性合并：target 属性 + join 属性（键冲突加 `join_` 前缀），另加
/// `join_index`（join 要素序号，便于溯源；无匹配时缺省）。
/// 无几何或类型不可转换的 target 要素按无匹配处理；join 侧同类要素对跳过
/// （不产生脏数据）。
///
/// **性能（rstar 裁剪，§8.1 复测路线项）**：要素对数 ≥ `SPATIAL_INDEX_MIN_PAIRS`
/// **且 join 侧 ≥ `SJOIN_INDEX_MIN_JOIN`** 时 join 侧建包矩形 R 树，target
/// 逐要素查询候选——包矩形相交是 intersects/contains/within 的必要条件，
/// 裁剪不改变结果集与输出序（候选按 join 序号升序，与朴素路径一致；
/// 对拍测试见 `tests::sjoin_indexed_matches_naive`）。低于阈值或 join 侧
/// 过小走朴素路径（实测 100 万 × 16 格场景索引反而慢约 10%：谓词本身
/// 极廉价，建树与逐点查询开销不抵——索引收益取决于裁剪率）。
pub fn sjoin(
    target: &geojson::FeatureCollection,
    join: &geojson::FeatureCollection,
    predicate: SpatialPredicate,
) -> Result<geojson::FeatureCollection> {
    let join_side: Vec<Option<geo_types::Geometry<f64>>> = join
        .features
        .iter()
        .map(|f| f.geometry.as_ref().and_then(|g| to_geo(&g.value)))
        .collect();
    let indexed = target.features.len() * join.features.len() >= SPATIAL_INDEX_MIN_PAIRS
        && join.features.len() >= SJOIN_INDEX_MIN_JOIN;
    let features = if indexed {
        sjoin_indexed(target, join, &join_side, predicate)
    } else {
        sjoin_naive(target, join, &join_side, predicate)
    };
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// 朴素笛卡尔积实现（小数据集路径 + 对拍基准；v0.4 原始语义）。
fn sjoin_naive(
    target: &geojson::FeatureCollection,
    join: &geojson::FeatureCollection,
    join_side: &[Option<geo_types::Geometry<f64>>],
    predicate: SpatialPredicate,
) -> Vec<geojson::Feature> {
    use geo::{Contains, Intersects};
    let mut features = Vec::new();
    for t in &target.features {
        let t_geom = t.geometry.as_ref().and_then(|g| to_geo(&g.value));
        let mut matched = false;
        if let Some(tg) = &t_geom {
            for (j_idx, jg) in join_side.iter().enumerate() {
                let Some(jg) = jg else { continue };
                let hit = match predicate {
                    SpatialPredicate::Intersects => tg.intersects(jg),
                    SpatialPredicate::Contains => tg.contains(jg),
                    SpatialPredicate::Within => jg.contains(tg),
                };
                if hit {
                    matched = true;
                    features.push(sjoined_feature(t, Some((j_idx, &join.features[j_idx]))));
                }
            }
        }
        if !matched {
            features.push(sjoined_feature(t, None));
        }
    }
    features
}

/// 索引裁剪实现（语义与 [`sjoin_naive`] 完全一致，见 sjoin rustdoc）。
fn sjoin_indexed(
    target: &geojson::FeatureCollection,
    join: &geojson::FeatureCollection,
    join_side: &[Option<geo_types::Geometry<f64>>],
    predicate: SpatialPredicate,
) -> Vec<geojson::Feature> {
    use geo::{BoundingRect, Contains, Intersects};
    let tree = BboxTree::build(
        join_side
            .iter()
            .map(|g| g.as_ref().and_then(|g| g.bounding_rect())),
    );
    let mut candidates = Vec::new();
    let mut features = Vec::new();
    for t in &target.features {
        let t_geom = t.geometry.as_ref().and_then(|g| to_geo(&g.value));
        let mut matched = false;
        if let Some(tg) = &t_geom {
            tree.candidates_into(tg.bounding_rect(), &mut candidates);
            for &j_idx in &candidates {
                let jg = join_side[j_idx].as_ref().expect("索引项必有geometry");
                let hit = match predicate {
                    SpatialPredicate::Intersects => tg.intersects(jg),
                    SpatialPredicate::Contains => tg.contains(jg),
                    SpatialPredicate::Within => jg.contains(tg),
                };
                if hit {
                    matched = true;
                    features.push(sjoined_feature(t, Some((j_idx, &join.features[j_idx]))));
                }
            }
        }
        if !matched {
            features.push(sjoined_feature(t, None));
        }
    }
    features
}

/// 组装 sjoin 输出要素：target 几何与属性 + join 属性（冲突加 `join_` 前缀）
/// + `join_index`。
fn sjoined_feature(
    target: &geojson::Feature,
    join: Option<(usize, &geojson::Feature)>,
) -> geojson::Feature {
    let mut properties = target.properties.clone().unwrap_or_default();
    if let Some((j_idx, j_feature)) = join {
        if let Some(j_props) = &j_feature.properties {
            for (key, value) in j_props {
                let key = if properties.contains_key(key) {
                    format!("join_{key}")
                } else {
                    key.clone()
                };
                properties.insert(key, value.clone());
            }
        }
        properties.insert("join_index".to_string(), serde_json::Value::from(j_idx));
    }
    geojson::Feature {
        bbox: None,
        geometry: target.geometry.clone(),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    }
}

/// 分区统计项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZonalStat {
    /// 落入要素数（要求 field 存在且为数值，见 [`zonal_stats`]）。
    Count,
    /// 数值和。
    Sum,
    /// 平均值。
    Mean,
    /// 最小值。
    Min,
    /// 最大值。
    Max,
}

impl ZonalStat {
    /// 输出列名后缀（小写）。
    fn suffix(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Mean => "mean",
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

impl std::str::FromStr for ZonalStat {
    type Err = KanyuError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "count" => Ok(Self::Count),
            "sum" => Ok(Self::Sum),
            "mean" => Ok(Self::Mean),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            other => Err(KanyuError::Other(format!(
                "未知统计项 '{other}'（支持 count/sum/mean/min/max）"
            ))),
        }
    }
}

/// 分区统计：values 要素按代表点归属 zones 面要素，对数值字段 `field`
/// 计算统计列并追加到 zones 要素属性（命名 `{field}_{stat}` 小写，
/// 如 `height_mean`；`stats` 去重保序）。
///
/// 归属语义（rustdoc 即契约）：
/// - 代表点：Point 直接用坐标；LineString/Polygon 等用 `geo::Centroid` 质心；
///   无几何或质心不可得的值跳过（计入 `unzoned_count`）。
/// - 一值落入多个区时计入**首个**匹配区；不属任何区计入 `unzoned_count`，
///   写入返回集合的 `foreign_members.unzoned_count`（为 0 时该键缺省）。
/// - zones 仅接受 Polygon/MultiPolygon（其余类型报中文错误并指出序号）。
/// - `field` 必须在 values 中至少一个要素上存在且可转 f64，否则中文报错；
///   **count 同样只统计 field 存在且为数值的要素**（直白发语义，注释即契约）。
/// - 某区无有效值时 `{field}_count` 写 0，其余统计列缺省（不产生脏数据）。
pub fn zonal_stats(
    zones: &geojson::FeatureCollection,
    values: &geojson::FeatureCollection,
    field: &str,
    stats: &[ZonalStat],
) -> Result<geojson::FeatureCollection> {
    use geo::Contains;

    if stats.is_empty() {
        return Err(KanyuError::Other(
            "分区统计至少需指定一项统计（支持 count/sum/mean/min/max）".to_string(),
        ));
    }
    let zone_polys = collect_polygons(zones, "zones")?;

    // field 校验：至少一个 values 要素存在且为数值。
    let field_ok = values.features.iter().any(|f| {
        f.properties
            .as_ref()
            .and_then(|p| p.get(field))
            .and_then(|v| v.as_f64())
            .is_some()
    });
    if !field_ok {
        return Err(KanyuError::Other(format!(
            "字段 '{field}' 在 values 中不存在或非数值（分区统计要求数值字段）"
        )));
    }

    // 逐值归属：每个 zone 收集落入的数值。
    let mut buckets: Vec<Vec<f64>> = vec![Vec::new(); zone_polys.len()];
    let mut unzoned_count = 0usize;
    for feature in &values.features {
        let Some(v) = feature
            .properties
            .as_ref()
            .and_then(|p| p.get(field))
            .and_then(|v| v.as_f64())
        else {
            continue; // field 缺失/非数值：不参与任何统计（含 count）。
        };
        let Some(repr) = representative_point(feature) else {
            unzoned_count += 1;
            continue;
        };
        let mut assigned = false;
        for (z_idx, zone) in zone_polys.iter().enumerate() {
            if zone.geom.contains(&repr) {
                buckets[z_idx].push(v);
                assigned = true;
                break; // 首个匹配区。
            }
        }
        if !assigned {
            unzoned_count += 1;
        }
    }

    // stats 去重保序。
    let mut ordered: Vec<ZonalStat> = Vec::new();
    for s in stats {
        if !ordered.contains(s) {
            ordered.push(*s);
        }
    }

    let mut features = Vec::with_capacity(zone_polys.len());
    for (z_idx, zone) in zone_polys.iter().enumerate() {
        let bucket = &buckets[z_idx];
        let mut properties = zone.feature.properties.clone().unwrap_or_default();
        for stat in &ordered {
            let column = format!("{field}_{}", stat.suffix());
            let value = match stat {
                ZonalStat::Count => Some(serde_json::Value::from(bucket.len())),
                ZonalStat::Sum if !bucket.is_empty() => {
                    Some(serde_json::Value::from(bucket.iter().sum::<f64>()))
                }
                ZonalStat::Mean if !bucket.is_empty() => Some(serde_json::Value::from(
                    bucket.iter().sum::<f64>() / bucket.len() as f64,
                )),
                ZonalStat::Min if !bucket.is_empty() => bucket
                    .iter()
                    .copied()
                    .reduce(f64::min)
                    .map(serde_json::Value::from),
                ZonalStat::Max if !bucket.is_empty() => bucket
                    .iter()
                    .copied()
                    .reduce(f64::max)
                    .map(serde_json::Value::from),
                _ => None,
            };
            if let Some(value) = value {
                properties.insert(column, value);
            }
        }
        features.push(geojson::Feature {
            bbox: None,
            geometry: zone.feature.geometry.clone(),
            id: None,
            properties: Some(properties),
            foreign_members: None,
        });
    }

    let foreign_members = (unzoned_count > 0).then(|| {
        let mut m = serde_json::Map::new();
        m.insert(
            "unzoned_count".to_string(),
            serde_json::Value::from(unzoned_count),
        );
        m
    });
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members,
    })
}

/// 要素代表点：Point 直接用坐标；其余类型用 geo::Centroid 质心；
/// 无几何或质心不可得返回 None。
fn representative_point(feature: &geojson::Feature) -> Option<geo_types::Point<f64>> {
    use geo::Centroid;
    let geom = feature.geometry.as_ref()?;
    match &geom.value {
        geojson::Value::Point(pos) => Some(geo_types::Point::new(
            pos.first().copied().unwrap_or(0.0),
            pos.get(1).copied().unwrap_or(0.0),
        )),
        other => to_geo(other).and_then(|g| g.centroid()),
    }
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

    /// sjoin 测试集：两个重叠面（name 键与点目标冲突，用于验证 join_ 前缀）。
    fn sjoin_zones() -> geojson::FeatureCollection {
        collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{},{}]}}"#,
            square(0.0, 0.0, 4.0, 4.0, "z1"),
            square(2.0, 2.0, 6.0, 6.0, "z2")
        ))
    }

    #[test]
    fn sjoin_left_explode_with_join_prefix_and_index() {
        let target = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[3,3]},
                 "properties":{"name":"p_in_both"}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
                 "properties":{"name":"p_in_z1"}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[100,100]},
                 "properties":{"name":"p_outside"}}
            ]}"#,
        );
        let out = sjoin(&target, &sjoin_zones(), SpatialPredicate::Within).unwrap();
        // 左连接 + explode：3 个 target → 2(双区命中) + 1 + 1(无匹配) = 4 条。
        assert_eq!(out.features.len(), 4);
        // 双区命中点展开为两条，join_index 分别为 0/1。
        let both: Vec<_> = out
            .features
            .iter()
            .filter(|f| {
                f.properties
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    == Some("p_in_both")
            })
            .collect();
        assert_eq!(both.len(), 2);
        let mut indices: Vec<i64> = both
            .iter()
            .map(|f| {
                f.properties
                    .as_ref()
                    .and_then(|p| p.get("join_index"))
                    .and_then(|v| v.as_i64())
                    .unwrap()
            })
            .collect();
        indices.sort_unstable();
        assert_eq!(indices, vec![0, 1]);
        // 键冲突：target 的 name 保留，join 的 name 进 join_name。
        let props = both[0].properties.as_ref().unwrap();
        assert_eq!(
            props["name"],
            serde_json::Value::String("p_in_both".to_string())
        );
        assert!(props.get("join_name").is_some());
        // 无匹配：join 侧属性与 join_index 缺省。
        let outside = out
            .features
            .iter()
            .find(|f| {
                f.properties
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    == Some("p_outside")
            })
            .unwrap();
        let props = outside.properties.as_ref().unwrap();
        assert!(props.get("join_index").is_none());
        assert!(props.get("join_name").is_none());
    }

    /// zonal 测试集：两区四点（一值跨区取首匹配、一值在区外）。
    fn zonal_fixtures() -> (geojson::FeatureCollection, geojson::FeatureCollection) {
        let zones = collection_from_str(&format!(
            r#"{{"type":"FeatureCollection","features":[{},{}]}}"#,
            square(0.0, 0.0, 4.0, 4.0, "z1"),
            square(2.0, 2.0, 6.0, 6.0, "z2")
        ));
        let values = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
                 "properties":{"height":10}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[3,3]},
                 "properties":{"height":20}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[5,5]},
                 "properties":{"height":40}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[100,100]},
                 "properties":{"height":99}}
            ]}"#,
        );
        (zones, values)
    }

    #[test]
    fn zonal_stats_first_match_and_unzoned_count() {
        let (zones, values) = zonal_fixtures();
        let out = zonal_stats(
            &zones,
            &values,
            "height",
            &[
                ZonalStat::Count,
                ZonalStat::Sum,
                ZonalStat::Mean,
                ZonalStat::Min,
                ZonalStat::Max,
            ],
        )
        .unwrap();
        assert_eq!(out.features.len(), 2);
        // (3,3) 同时落入两区 → 计入首个匹配区 z1。
        // z1: {10, 20}；z2: {40}；区外 99 → unzoned_count=1。
        let z1 = out.features[0].properties.as_ref().unwrap();
        assert_eq!(z1["height_count"], serde_json::Value::from(2));
        assert_eq!(z1["height_sum"].as_f64(), Some(30.0));
        assert_eq!(z1["height_mean"].as_f64(), Some(15.0));
        assert_eq!(z1["height_min"].as_f64(), Some(10.0));
        assert_eq!(z1["height_max"].as_f64(), Some(20.0));
        let z2 = out.features[1].properties.as_ref().unwrap();
        assert_eq!(z2["height_count"], serde_json::Value::from(1));
        assert_eq!(z2["height_sum"].as_f64(), Some(40.0));
        // unzoned_count 写入 foreign_members。
        assert_eq!(
            out.foreign_members.as_ref().unwrap()["unzoned_count"],
            serde_json::Value::from(1)
        );
        // zone 原有属性保留。
        assert_eq!(z1["name"], serde_json::Value::String("z1".to_string()));
    }

    #[test]
    fn zonal_stats_errors_on_non_polygon_zone_and_missing_field() {
        let (_, values) = zonal_fixtures();
        let err = zonal_stats(&values, &values, "height", &[ZonalStat::Count]).unwrap_err();
        assert!(
            err.to_string().contains("第 1 个要素"),
            "非面 zone 应报错: {err}"
        );
        let (zones, _) = zonal_fixtures();
        let err = zonal_stats(&zones, &values, "nonexist", &[ZonalStat::Count]).unwrap_err();
        assert!(
            err.to_string().contains("nonexist"),
            "缺失字段应点名: {err}"
        );
    }

    #[test]
    fn spatial_predicate_and_zonal_stat_parse_chinese_errors() {
        assert!("within".parse::<SpatialPredicate>().is_ok());
        assert!("bogus"
            .parse::<SpatialPredicate>()
            .unwrap_err()
            .to_string()
            .contains("空间谓词"));
        assert!("mean".parse::<ZonalStat>().is_ok());
        assert!("bogus"
            .parse::<ZonalStat>()
            .unwrap_err()
            .to_string()
            .contains("统计项"));
    }

    // ===== rstar 索引裁剪对拍（索引版 vs 朴素版，语义完全一致）=====

    /// 一排互不相交的方格图层（边长 0.5，起点 (x0, y)，步长 1.0）。
    fn squares_fc(n: usize, x0: f64, y: f64) -> geojson::FeatureCollection {
        let mut fc = geojson::FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        for i in 0..n {
            let x = x0 + i as f64;
            fc.features.push(geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(vec![vec![
                    vec![x, y],
                    vec![x + 0.5, y],
                    vec![x + 0.5, y + 0.5],
                    vec![x, y + 0.5],
                    vec![x, y],
                ]]))),
                id: None,
                properties: None,
                foreign_members: None,
            });
        }
        fc
    }

    /// 一排互不相交的点图层。
    fn points_fc(n: usize, x0: f64, y: f64) -> geojson::FeatureCollection {
        let mut fc = geojson::FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        for i in 0..n {
            fc.features.push(geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![
                    x0 + i as f64,
                    y,
                ]))),
                id: None,
                properties: None,
                foreign_members: None,
            });
        }
        fc
    }

    /// 逐要素比对：属性逐字节一致 + 几何集合相等（对称差面积 < 1e-4；
    /// 布尔核对不相交对亦有 ~1e-9 坐标扰动，故不做逐坐标比对——
    /// 对称差面积是集合相等的度量，容差远高于核扰动、远低于场景尺度）。
    fn assert_same_features(fast: &[geojson::Feature], slow: &[geojson::Feature]) {
        use geo::{Area, BooleanOps};
        assert_eq!(fast.len(), slow.len(), "要素数不一致");
        for (i, (f, s)) in fast.iter().zip(slow).enumerate() {
            assert_eq!(f.properties, s.properties, "要素 {i} 属性不一致");
            let fg = f.geometry.as_ref().and_then(|g| to_geo(&g.value));
            let sg = s.geometry.as_ref().and_then(|g| to_geo(&g.value));
            match (&fg, &sg) {
                (
                    Some(geo_types::Geometry::MultiPolygon(fmp)),
                    Some(geo_types::Geometry::MultiPolygon(smp)),
                ) => {
                    let sym = fmp.xor(smp).unsigned_area();
                    assert!(sym < 1e-4, "要素 {i} 对称差面积 {sym} 超容差");
                }
                (Some(_), Some(_)) => panic!("要素 {i} 应为 MultiPolygon"),
                _ => panic!("要素 {i} 几何缺失"),
            }
        }
    }

    #[test]
    fn overlay_indexed_matches_naive() {
        let ops = [
            OverlayOp::Union,
            OverlayOp::Intersection,
            OverlayOp::Xor,
            OverlayOp::Difference,
        ];
        // 场景一：错半格重叠方格对（40×30=1200 对 > 阈值 → 索引路径）。
        let (a, _) = crate::bench::overlay_pair(40, 1);
        let (_, b) = crate::bench::overlay_pair(30, 2);
        // 场景二：全不相交（Union/Xor 拼合直通路径；40×30=1200 对）。
        let far_a = squares_fc(40, 0.0, 0.0);
        let far_b = squares_fc(30, 1000.0, 1000.0);
        // 场景三：空图层边界（0 对 → 朴素路径亦应一致）。
        let (empty, _) = crate::bench::overlay_pair(0, 1);
        for (ta, tb) in [(&a, &b), (&far_a, &far_b), (&empty, &b)] {
            let t = collect_polygons(ta, "t").unwrap();
            let o = collect_polygons(tb, "o").unwrap();
            for op in ops {
                let fast = overlay(ta, tb, op).unwrap();
                let slow = overlay_naive(&t, &o, op);
                assert_same_features(&fast.features, &slow);
            }
        }
    }

    #[test]
    fn sjoin_indexed_matches_naive() {
        let preds = [
            SpatialPredicate::Intersects,
            SpatialPredicate::Contains,
            SpatialPredicate::Within,
        ];
        // 重叠场景：600 混合要素 × 100 格（60000 对 > 阈值、join 侧 100 ≥ 64
        // → 索引路径）；几何原样随行，索引版与朴素版应逐字节一致。
        let target = crate::bench::mixed(600, 9);
        let (join, _) = crate::bench::overlay_pair(100, 3);
        // 无相交边界：600 远点 × 100 格（全部无匹配，走左连接缺省行）。
        let far = points_fc(600, 5000.0, 5000.0);
        for (t, j) in [(&target, &join), (&far, &join)] {
            let side: Vec<_> = j
                .features
                .iter()
                .map(|f| f.geometry.as_ref().and_then(|g| to_geo(&g.value)))
                .collect();
            for pred in preds {
                let fast = sjoin(t, j, pred).unwrap();
                let slow = sjoin_naive(t, j, &side, pred);
                assert_eq!(fast.features, slow, "{pred:?} 索引版与朴素版应一致");
            }
        }
    }
}
