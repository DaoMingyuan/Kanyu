//! 拓扑共享顶点编辑（ArcGIS Pro Map Topology 语义 v1）。
//!
//! 问题：相邻面要素各自存顶点副本，逐要素编辑（[`crate::MoveVertex`]）移动
//! 共享顶点会产生裂缝。本模块按**坐标键**（f64 位模式精确相等，与
//! [`crate::dcel`] 键控合并同一约定）建立共享顶点索引，一次移动全部共享
//! 该坐标的顶点（含同一要素内环闭合首末点等多处出现），产出
//! [`DeltaSet`] 走 delta 通道——**撤销一次完成**。
//!
//! 范围约定（rustdoc 即契约）：
//! - [`move_shared_vertex`]：**当前图层**范围（单集合），壳层 v1 接线用此；
//! - [`move_shared_vertex_across`]：**跨图层**范围，逐图层各产一个
//!   DeltaSet（各图层历史各自入栈，撤销须逐层进行——跨图层单次撤销组
//!   为后续项）。

use std::collections::HashMap;

use geojson::{Feature, FeatureCollection, Value as GeoValue};
use kanyu_core::KanyuError;

use crate::delta::{DeltaSet, FeatureDelta};

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::Other(msg.into())
}

/// 坐标键（f64 位模式精确相等）。
fn key_of(coord: [f64; 2]) -> (u64, u64) {
    (coord[0].to_bits(), coord[1].to_bits())
}

/// 共享顶点索引：坐标键 → 共享该坐标的要素下标清单（去重升序）。
pub struct SharedVertexIndex {
    map: HashMap<(u64, u64), Vec<usize>>,
}

impl SharedVertexIndex {
    /// 共享该坐标的要素下标（无共享为空片）。
    pub fn features_at(&self, coord: [f64; 2]) -> &[usize] {
        self.map
            .get(&key_of(coord))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// 是否跨要素共享顶点（出现于 ≥2 个要素）。
    pub fn is_shared(&self, coord: [f64; 2]) -> bool {
        self.features_at(coord).len() >= 2
    }
}

/// 从要素集合构建共享顶点索引（GeometryCollection 递归展开）。
pub fn shared_vertex_index(collection: &FeatureCollection) -> SharedVertexIndex {
    let mut map: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
    for (fi, feature) in collection.features.iter().enumerate() {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        walk_coords(&geom.value, &mut |c| {
            let entry = map.entry(key_of(c)).or_default();
            if entry.last() != Some(&fi) {
                entry.push(fi);
            }
        });
    }
    SharedVertexIndex { map }
}

/// 逐顶点坐标遍历（全部几何类型，含 GeometryCollection 递归）。
fn walk_coords(v: &GeoValue, f: &mut impl FnMut([f64; 2])) {
    let pt = |p: &[f64]| [p[0], p[1]];
    match v {
        GeoValue::Point(p) => f(pt(p)),
        GeoValue::MultiPoint(ps) | GeoValue::LineString(ps) => {
            for p in ps {
                f(pt(p));
            }
        }
        GeoValue::MultiLineString(ls) | GeoValue::Polygon(ls) => {
            for l in ls {
                for p in l {
                    f(pt(p));
                }
            }
        }
        GeoValue::MultiPolygon(ps) => {
            for poly in ps {
                for ring in poly {
                    for p in ring {
                        f(pt(p));
                    }
                }
            }
        }
        GeoValue::GeometryCollection(gs) => {
            for g in gs {
                walk_coords(&g.value, f);
            }
        }
    }
}

/// 要素几何内全部等于 coord 的顶点替换为 new_coord（含环闭合首末点等多处）。
fn replace_coord(feature: &mut Feature, coord: [f64; 2], new_coord: [f64; 2]) {
    fn walk(v: &mut GeoValue, coord: [f64; 2], new_coord: [f64; 2]) {
        fn replace_in(pts: &mut [Vec<f64>], coord: [f64; 2], new_coord: [f64; 2]) {
            for p in pts {
                if p[0] == coord[0] && p[1] == coord[1] {
                    p[0] = new_coord[0];
                    p[1] = new_coord[1];
                }
            }
        }
        match v {
            GeoValue::Point(p) => {
                if p[0] == coord[0] && p[1] == coord[1] {
                    p[0] = new_coord[0];
                    p[1] = new_coord[1];
                }
            }
            GeoValue::MultiPoint(ps) | GeoValue::LineString(ps) => replace_in(ps, coord, new_coord),
            GeoValue::MultiLineString(ls) | GeoValue::Polygon(ls) => {
                for l in ls {
                    replace_in(l, coord, new_coord);
                }
            }
            GeoValue::MultiPolygon(ps) => {
                for poly in ps {
                    for ring in poly {
                        replace_in(ring, coord, new_coord);
                    }
                }
            }
            GeoValue::GeometryCollection(gs) => {
                for g in gs {
                    walk(&mut g.value, coord, new_coord);
                }
            }
        }
    }
    if let Some(geom) = &mut feature.geometry {
        walk(&mut geom.value, coord, new_coord);
    }
}

/// 移动全部共享该坐标的顶点（**当前图层**范围），返回 DeltaSet——调用方经
/// `History::push_delta` 应用，一次撤销。坐标无共享要素报中文错误。
pub fn move_shared_vertex(
    collection: &FeatureCollection,
    coord: [f64; 2],
    new_coord: [f64; 2],
) -> Result<DeltaSet, KanyuError> {
    let index = shared_vertex_index(collection);
    let features = index.features_at(coord);
    if features.is_empty() {
        return Err(err(format!(
            "坐标 ({}, {}) 无顶点（拓扑移动未命中）",
            coord[0], coord[1]
        )));
    }
    let mut ds = DeltaSet::new("拓扑移动共享顶点");
    for &fi in features {
        let before = collection.features[fi].clone();
        let mut after = before.clone();
        replace_coord(&mut after, coord, new_coord);
        ds.push(FeatureDelta::modify(fi, before, after));
    }
    Ok(ds)
}

/// 跨图层范围变体：逐图层各产一个 DeltaSet（不含该坐标的图层跳过；
/// 全部未命中报中文错误）。撤销须逐层进行（见模块头范围约定）。
pub fn move_shared_vertex_across(
    layers: &[&FeatureCollection],
    coord: [f64; 2],
    new_coord: [f64; 2],
) -> Result<Vec<DeltaSet>, KanyuError> {
    let mut out = Vec::new();
    for layer in layers {
        let index = shared_vertex_index(layer);
        if index.features_at(coord).is_empty() {
            continue;
        }
        out.push(move_shared_vertex(layer, coord, new_coord)?);
    }
    if out.is_empty() {
        return Err(err(format!(
            "坐标 ({}, {}) 在全部图层均无顶点",
            coord[0], coord[1]
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::History;

    fn feat(v: GeoValue) -> Feature {
        Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(v)),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn ring(pts: &[[f64; 2]]) -> Vec<Vec<f64>> {
        let mut v: Vec<Vec<f64>> = pts.iter().map(|p| p.to_vec()).collect();
        v.push(pts[0].to_vec());
        v
    }

    /// 两相邻方格（共享 x=1 边）要素集合。
    fn adjacent_squares() -> FeatureCollection {
        let sq1 = ring(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let sq2 = ring(&[[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]);
        FeatureCollection {
            bbox: None,
            features: vec![
                feat(GeoValue::Polygon(vec![sq1])),
                feat(GeoValue::Polygon(vec![sq2])),
            ],
            foreign_members: None,
        }
    }

    #[test]
    fn shared_index_identifies_shared_and_unique() {
        let c = adjacent_squares();
        let index = shared_vertex_index(&c);
        // 共享边两端点出现于两个要素。
        assert!(index.is_shared([1.0, 0.0]));
        assert!(index.is_shared([1.0, 1.0]));
        assert_eq!(index.features_at([1.0, 0.0]), &[0, 1]);
        // 独有顶点仅一个要素。
        assert!(!index.is_shared([0.0, 0.0]));
        assert_eq!(index.features_at([2.0, 2.0]), &[] as &[usize]);
    }

    #[test]
    fn move_shared_vertex_cascades_without_crack() {
        let mut c = adjacent_squares();
        let ds = move_shared_vertex(&c, [1.0, 0.0], [1.3, 0.2]).unwrap();
        assert_eq!(ds.len(), 2, "两要素联动");
        ds.apply(&mut c).unwrap();
        // 无裂缝断言：两面的共享顶点同为新坐标（坐标级）。
        for (fi, expect_vi) in [(0, 1), (1, 0)] {
            let GeoValue::Polygon(rings) = &c.features[fi].geometry.as_ref().unwrap().value else {
                panic!("应为 Polygon")
            };
            assert_eq!(rings[0][expect_vi], vec![1.3, 0.2], "要素 {fi} 顶点应联动");
            assert_eq!(rings[0].first(), rings[0].last(), "要素 {fi} 环仍闭合");
        }
        // 旧坐标在两要素中均不残留。
        for f in &c.features {
            let GeoValue::Polygon(rings) = &f.geometry.as_ref().unwrap().value else {
                panic!()
            };
            assert!(!rings[0].iter().any(|p| p[0] == 1.0 && p[1] == 0.0));
        }
        // 一次撤销全还原。
        ds.revert(&mut c).unwrap();
        let GeoValue::Polygon(rings) = &c.features[0].geometry.as_ref().unwrap().value else {
            panic!()
        };
        assert_eq!(rings[0][1], vec![1.0, 0.0]);
    }

    #[test]
    fn move_shared_vertex_closing_point_and_degenerate() {
        // 环闭合首末点（同要素多处出现）：移动顶点 0 坐标，首末同步。
        let mut c = FeatureCollection {
            bbox: None,
            features: vec![feat(GeoValue::Polygon(vec![ring(&[
                [0.0, 0.0],
                [1.0, 0.0],
                [1.0, 1.0],
            ])]))],
            foreign_members: None,
        };
        let ds = move_shared_vertex(&c, [0.0, 0.0], [9.0, 9.0]).unwrap();
        assert_eq!(ds.len(), 1, "无共享单点退化为单要素 Delta");
        ds.apply(&mut c).unwrap();
        let GeoValue::Polygon(rings) = &c.features[0].geometry.as_ref().unwrap().value else {
            panic!()
        };
        assert_eq!(rings[0][0], vec![9.0, 9.0]);
        assert_eq!(rings[0][3], vec![9.0, 9.0], "闭合末点同步");
        // 未命中报错。
        assert!(move_shared_vertex(&c, [5.0, 5.0], [0.0, 0.0]).is_err());
    }

    #[test]
    fn move_shared_vertex_across_layers() {
        let a = adjacent_squares();
        let b = FeatureCollection {
            bbox: None,
            features: vec![feat(GeoValue::Point(vec![1.0, 0.0]))],
            foreign_members: None,
        };
        let empty = FeatureCollection {
            bbox: None,
            features: vec![],
            foreign_members: None,
        };
        let sets = move_shared_vertex_across(&[&a, &b, &empty], [1.0, 0.0], [5.0, 5.0]).unwrap();
        assert_eq!(sets.len(), 2, "空图层跳过，两命中图层各一 DeltaSet");
        // 全部未命中报错。
        assert!(move_shared_vertex_across(&[&empty], [1.0, 0.0], [0.0, 0.0]).is_err());
    }

    #[test]
    fn delta_channel_undo_restores_both_features() {
        let mut c = adjacent_squares();
        let mut h = History::default();
        let ds = move_shared_vertex(&c, [1.0, 0.0], [1.3, 0.2]).unwrap();
        h.push_delta(ds, &mut c).unwrap();
        assert_eq!(h.len(), 1);
        h.undo(&mut c).unwrap();
        let GeoValue::Polygon(rings) = &c.features[1].geometry.as_ref().unwrap().value else {
            panic!()
        };
        assert_eq!(rings[0][0], vec![1.0, 0.0], "一次撤销两面同还原");
    }
}
