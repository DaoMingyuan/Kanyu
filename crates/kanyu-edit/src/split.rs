//! 分割要素（编辑内核）：面要素切割线分割 + 线要素按点打断。
//!
//! ## 语义（rustdoc 即契约）
//!
//! - [`split_polygon_by_line`]：务实实现——切割线微量缓冲（ε = 要素与切割线
//!   合并 bbox 对角线 × 1e-7；geo Buffer 内核下限实测 1e-8 相对量级）后与面
//!   做 `difference`，结果 MultiPolygon 炸开，
//!   **丢弃面积 < 2ε×对角线 的碎条**（碎屑阈值即缓冲带宽量级的细长条；
//!   小于它的碎片视为数值残渣而非真实切片）。产出 ≥2 片才成立：原要素
//!   就地改为首片（modify），其余片按序插入其后（insert），属性随行复制。
//!   线不切面/退化结果报中文错误。
//! - [`split_line_at_point`]：仅 LineString（MultiLineString 请先「炸开
//!   多部件」）；点投影到最近线段（t 截断 [0,1]，1e-9 内吸附既有顶点），
//!   截为两段——首段就地改（modify），次段插入其后（insert），属性复制。
//!   投影落于线端点（段长 <2 点）报中文错误。
//!
//! 两者均产 [`DeltaSet`]（modify + insert 组合），入 `History::push_delta`
//! 一次撤销。

use geojson::{Feature, FeatureCollection, Value as GeoValue};
use kanyu_core::KanyuError;

use crate::delta::{DeltaSet, FeatureDelta};

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::Other(msg.into())
}

/// 面要素被折线切割为多片（语义见模块头）。返回 DeltaSet：
/// `分割为 N 片`（modify 原要素为首片 + insert 其余片）。
pub fn split_polygon_by_line(
    collection: &FeatureCollection,
    feature_index: usize,
    cut_line: &[Vec<f64>],
) -> Result<DeltaSet, KanyuError> {
    use geo::{Area, BooleanOps, BoundingRect, Buffer};
    let feature = collection
        .features
        .get(feature_index)
        .ok_or_else(|| err(format!("要素下标越界: {feature_index}")))?;
    let geom = feature
        .geometry
        .as_ref()
        .ok_or_else(|| err(format!("要素 #{feature_index} 无几何")))?;
    let mp = match geo::Geometry::<f64>::try_from(&geom.value) {
        Ok(geo::Geometry::Polygon(p)) => geo::MultiPolygon(vec![p]),
        Ok(geo::Geometry::MultiPolygon(mp)) => mp,
        _ => return Err(err("分割要素仅支持 Polygon/MultiPolygon 面要素")),
    };
    if cut_line.len() < 2 {
        return Err(err(format!(
            "切割线至少需要 2 个顶点（当前 {}）",
            cut_line.len()
        )));
    }
    // ε 自适应：要素与切割线合并 bbox 对角线 × 1e-7。
    // （实测下限：geo Buffer 在相对量级 ≤1e-9 时返回空带、1e-8 起正常——
    // 取 1e-7 留一个数量级裕量；面积损失 ≈ 2ε×切割长，相对量级 1e-7。）
    let line = geo::LineString::from(cut_line.iter().map(|p| (p[0], p[1])).collect::<Vec<_>>());
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let mut grow = |r: geo::Rect<f64>| {
        min[0] = min[0].min(r.min().x);
        min[1] = min[1].min(r.min().y);
        max[0] = max[0].max(r.max().x);
        max[1] = max[1].max(r.max().y);
    };
    if let Some(r) = mp.bounding_rect() {
        grow(r);
    }
    if let Some(r) = line.bounding_rect() {
        grow(r);
    }
    let diag = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2)).sqrt();
    let eps = diag * 1e-7;
    // 切割线微量缓冲 → 差集 → 炸开 → 碎条剔除（阈值 2ε×对角线）。
    let band = line.buffer(eps);
    let diff = mp.difference(&band);
    let sliver = 2.0 * eps * diag;
    let pieces: Vec<geo::Polygon<f64>> = diff
        .0
        .into_iter()
        .filter(|p| p.unsigned_area() >= sliver)
        .collect();
    if pieces.len() < 2 {
        return Err(err(
            "切割线未将面分为多片（线不切面或切于边缘/顶点）".to_string()
        ));
    }
    // modify 原要素为首片 + insert 其余片（属性随行复制）。
    let mut ds = DeltaSet::new(format!("分割为 {} 片", pieces.len()));
    let mut iter = pieces.into_iter();
    let first = iter.next().expect("pieces 非空");
    ds.push(FeatureDelta::modify(
        feature_index,
        feature.clone(),
        with_value(feature, GeoValue::from(&first)),
    ));
    for (k, piece) in iter.enumerate() {
        ds.push(FeatureDelta::insert(
            feature_index + 1 + k,
            with_value(feature, GeoValue::from(&piece)),
        ));
    }
    Ok(ds)
}

/// 线在指定点打断为两条（语义见模块头；仅 LineString）。
pub fn split_line_at_point(
    collection: &FeatureCollection,
    feature_index: usize,
    point: [f64; 2],
) -> Result<DeltaSet, KanyuError> {
    let feature = collection
        .features
        .get(feature_index)
        .ok_or_else(|| err(format!("要素下标越界: {feature_index}")))?;
    let geom = feature
        .geometry
        .as_ref()
        .ok_or_else(|| err(format!("要素 #{feature_index} 无几何")))?;
    let coords: &[Vec<f64>] = match &geom.value {
        GeoValue::LineString(ls) => ls,
        GeoValue::MultiLineString(_) => {
            return Err(err("MultiLineString 请先「炸开多部件」再打断"))
        }
        _ => return Err(err("打断仅支持 LineString 线要素")),
    };
    if coords.len() < 2 {
        return Err(err("线要素不足 2 个顶点，无法打断"));
    }
    // 最近线段投影（t 截断 [0,1]）。
    let mut best: Option<(f64, usize, f64, [f64; 2])> = None; // (距离², 段号, t, 投影点)
    for (i, seg) in coords.windows(2).enumerate() {
        let (ax, ay) = (seg[0][0], seg[0][1]);
        let (bx, by) = (seg[1][0], seg[1][1]);
        let (dx, dy) = (bx - ax, by - ay);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-24 {
            continue; // 退化段跳过
        }
        let t = (((point[0] - ax) * dx + (point[1] - ay) * dy) / len2).clamp(0.0, 1.0);
        let proj = [ax + dx * t, ay + dy * t];
        let d2 = (point[0] - proj[0]).powi(2) + (point[1] - proj[1]).powi(2);
        if best.as_ref().is_none_or(|(bd, _, _, _)| d2 < *bd) {
            best = Some((d2, i, t, proj));
        }
    }
    let (_, i, t, proj) = best.ok_or_else(|| err("线要素无有效线段"))?;
    // 1e-9 内吸附既有顶点（切段落在顶点上）。
    let snap = |a: &[f64], t: f64| {
        t < 1e-9 || (proj[0] - a[0]).abs() < 1e-9 && (proj[1] - a[1]).abs() < 1e-9
    };
    let (first, second): (Vec<Vec<f64>>, Vec<Vec<f64>>) = if snap(&coords[i], t) {
        (coords[..=i].to_vec(), coords[i..].to_vec())
    } else if t > 1.0 - 1e-9 {
        (coords[..=i + 1].to_vec(), coords[i + 1..].to_vec())
    } else {
        let mut f: Vec<Vec<f64>> = coords[..=i].to_vec();
        f.push(proj.to_vec());
        let mut s: Vec<Vec<f64>> = vec![proj.to_vec()];
        s.extend_from_slice(&coords[i + 1..]);
        (f, s)
    };
    if first.len() < 2 || second.len() < 2 {
        return Err(err("打断点位于线端点（段长不足 2 点）"));
    }
    let mut ds = DeltaSet::new("线打断为两段");
    ds.push(FeatureDelta::modify(
        feature_index,
        feature.clone(),
        with_value(feature, GeoValue::LineString(first)),
    ));
    ds.push(FeatureDelta::insert(
        feature_index + 1,
        with_value(feature, GeoValue::LineString(second)),
    ));
    Ok(ds)
}

/// 要素换几何（属性/其余字段随行复制）。
fn with_value(feature: &Feature, value: GeoValue) -> Feature {
    Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(value)),
        id: feature.id.clone(),
        properties: feature.properties.clone(),
        foreign_members: feature.foreign_members.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::History;
    use geo::Area;
    use serde_json::{Map, Value as Json};

    fn poly_feature(ring: &[[f64; 2]], name: &str) -> Feature {
        let mut props = Map::new();
        props.insert("name".to_string(), Json::String(name.to_string()));
        let mut r: Vec<Vec<f64>> = ring.iter().map(|p| p.to_vec()).collect();
        if r.first() != r.last() {
            r.push(ring[0].to_vec());
        }
        Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(GeoValue::Polygon(vec![r]))),
            id: None,
            properties: Some(props),
            foreign_members: None,
        }
    }

    fn coll_of(features: Vec<Feature>) -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }
    }

    fn poly_area(f: &Feature) -> f64 {
        match geo::Geometry::<f64>::try_from(&f.geometry.as_ref().unwrap().value).unwrap() {
            geo::Geometry::Polygon(p) => p.unsigned_area(),
            geo::Geometry::MultiPolygon(mp) => mp.unsigned_area(),
            _ => 0.0,
        }
    }

    const SQ: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    #[test]
    fn split_polygon_by_line_conserves_area() {
        let c = coll_of(vec![poly_feature(&SQ, "甲")]);
        let cut = vec![vec![0.5, -0.5], vec![0.5, 1.5]];
        let ds = split_polygon_by_line(&c, 0, &cut).unwrap();
        assert_eq!(ds.len(), 2, "两片 = 1 modify + 1 insert");
        let mut out = c.clone();
        ds.apply(&mut out).unwrap();
        assert_eq!(out.features.len(), 2);
        // 面积守恒（扣除 2ε 宽碎条，容差 1e-6）。
        let total = poly_area(&out.features[0]) + poly_area(&out.features[1]);
        assert!((total - 1.0).abs() < 1e-6, "面积守恒: {total}");
        for f in &out.features {
            let a = poly_area(f);
            assert!((a - 0.5).abs() < 1e-3, "竖线中切各约一半: {a}");
            assert_eq!(
                f.properties.as_ref().unwrap()["name"],
                Json::String("甲".to_string()),
                "属性复制"
            );
        }
        // 一次撤销还原。
        ds.revert(&mut out).unwrap();
        assert_eq!(out.features.len(), 1);
        assert!((poly_area(&out.features[0]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn split_polygon_by_line_miss_is_chinese_error() {
        let c = coll_of(vec![poly_feature(&SQ, "甲")]);
        // 不切割（线远在面外）。
        let miss = vec![vec![5.0, 5.0], vec![6.0, 6.0]];
        let e = split_polygon_by_line(&c, 0, &miss).unwrap_err();
        assert!(e.to_string().contains("未将面分为多片"), "{e}");
        // 线顶点不足。
        assert!(split_polygon_by_line(&c, 0, &[vec![0.5, 0.5]]).is_err());
        // 非面要素。
        let pts = coll_of(vec![Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(GeoValue::Point(vec![0.0, 0.0]))),
            id: None,
            properties: None,
            foreign_members: None,
        }]);
        let e = split_polygon_by_line(&pts, 0, &[vec![0.0, 0.0], vec![1.0, 1.0]]).unwrap_err();
        assert!(e.to_string().contains("面要素"), "{e}");
    }

    #[test]
    fn split_line_at_point_conserves_length() {
        let line = Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(GeoValue::LineString(vec![
                vec![0.0, 0.0],
                vec![2.0, 0.0],
                vec![2.0, 2.0],
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        };
        let c = coll_of(vec![line]);
        // 中点 (1,0) 打断：两段 1.0 与 2.0（投影恰在首段中点）。
        let ds = split_line_at_point(&c, 0, [1.0, 0.1]).unwrap();
        let mut out = c.clone();
        ds.apply(&mut out).unwrap();
        assert_eq!(out.features.len(), 2);
        let len = |f: &Feature| match &f.geometry.as_ref().unwrap().value {
            GeoValue::LineString(ls) => ls
                .windows(2)
                .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
                .sum::<f64>(),
            _ => 0.0,
        };
        let (l1, l2) = (len(&out.features[0]), len(&out.features[1]));
        assert!((l1 - 1.0).abs() < 1e-9, "首段 1.0: {l1}");
        assert!((l2 - 3.0).abs() < 1e-9, "次段 3.0: {l2}");
        assert!((l1 + l2 - 4.0).abs() < 1e-9, "总长守恒");
        // 端点打断报错。
        assert!(split_line_at_point(&c, 0, [0.0, 0.0]).is_err());
        // MultiLineString 请先炸开。
        let ml = coll_of(vec![Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(GeoValue::MultiLineString(vec![
                vec![vec![0.0, 0.0], vec![1.0, 1.0]],
            ]))),
            id: None,
            properties: None,
            foreign_members: None,
        }]);
        let e = split_line_at_point(&ml, 0, [0.5, 0.5]).unwrap_err();
        assert!(e.to_string().contains("炸开"), "{e}");
    }

    #[test]
    fn split_via_history_single_undo() {
        let c = coll_of(vec![poly_feature(&SQ, "甲")]);
        let mut out = c.clone();
        let mut h = History::default();
        let ds = split_polygon_by_line(&c, 0, &[vec![0.5, -0.5], vec![0.5, 1.5]]).unwrap();
        h.push_delta(ds, &mut out).unwrap();
        assert_eq!(out.features.len(), 2);
        h.undo(&mut out).unwrap();
        assert_eq!(out.features.len(), 1, "一次撤销还原单片");
        assert_eq!(out, c, "撤销后与原集合一致");
    }
}
