//! 基础编辑命令（全部为 `EditCommand` 纯函数实现，中文结构化错误）。
//!
//! ## 几何路径定位约定（`GeomPath`）
//!
//! 三级下标 `part / ring / vertex`（要素本身由命令的 `index` 字段定位）：
//!
//! | 几何类型 | part | ring | vertex |
//! |---|---|---|---|
//! | Point | 恒 0 | 恒 0 | 恒 0 |
//! | MultiPoint | 点下标 | 恒 0 | 恒 0 |
//! | LineString | 恒 0 | 恒 0 | 顶点下标 |
//! | MultiLineString | 线下标 | 恒 0 | 顶点下标 |
//! | Polygon | 恒 0 | 环下标（0=外环） | 顶点下标 |
//! | MultiPolygon | 面下标 | 环下标（0=外环） | 顶点下标 |
//! | GeometryCollection | 不支持（编辑前须炸开，见 geoprocess::explode） | | |

use geojson::{Feature, FeatureCollection, Value as GeoValue};
use kanyu_core::KanyuError;
use serde_json::{Map, Value as Json};

use crate::history::EditCommand;

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::Other(msg.into())
}

/// 几何路径（三级下标，见模块头约定表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeomPath {
    /// 部件下标（Multi* 系列）。
    pub part: usize,
    /// 环下标（面；0=外环）。
    pub ring: usize,
    /// 顶点下标。
    pub vertex: usize,
}

/// 取要素几何（无几何 → 中文错误）。
fn geometry_mut(feature: &mut Feature, index: usize) -> Result<&mut GeoValue, KanyuError> {
    feature
        .geometry
        .as_mut()
        .map(|g| &mut g.value)
        .ok_or_else(|| err(format!("要素 #{index} 无几何")))
}

/// 按路径取顶点（可变借用）。
fn vertex_mut<'a>(
    value: &'a mut GeoValue,
    path: &GeomPath,
) -> Result<&'a mut Vec<f64>, KanyuError> {
    let bad = || err(format!("几何路径越界: {path:?}"));
    match value {
        GeoValue::Point(pos) => {
            if path.part == 0 && path.ring == 0 && path.vertex == 0 {
                Ok(pos)
            } else {
                Err(bad())
            }
        }
        GeoValue::MultiPoint(pts) => pts.get_mut(path.part).ok_or_else(bad),
        GeoValue::LineString(line) => {
            if path.part == 0 {
                line.get_mut(path.vertex).ok_or_else(bad)
            } else {
                Err(bad())
            }
        }
        GeoValue::MultiLineString(lines) => lines
            .get_mut(path.part)
            .and_then(|l| l.get_mut(path.vertex))
            .ok_or_else(bad),
        GeoValue::Polygon(rings) => rings
            .get_mut(path.ring)
            .and_then(|r| r.get_mut(path.vertex))
            .ok_or_else(bad),
        GeoValue::MultiPolygon(polys) => polys
            .get_mut(path.part)
            .and_then(|p| p.get_mut(path.ring))
            .and_then(|r| r.get_mut(path.vertex))
            .ok_or_else(bad),
        GeoValue::GeometryCollection(_) => Err(err(
            "GeometryCollection 不支持路径编辑（请先「炸开多部件」）",
        )),
    }
}

/// 递归平移几何全部坐标。
fn translate(value: &mut GeoValue, dx: f64, dy: f64) {
    match value {
        GeoValue::Point(p) => {
            p[0] += dx;
            p[1] += dy;
        }
        GeoValue::MultiPoint(ps) | GeoValue::LineString(ps) => {
            for p in ps {
                p[0] += dx;
                p[1] += dy;
            }
        }
        GeoValue::MultiLineString(ls) | GeoValue::Polygon(ls) => {
            for l in ls {
                for p in l {
                    p[0] += dx;
                    p[1] += dy;
                }
            }
        }
        GeoValue::MultiPolygon(ps) => {
            for p in ps {
                for r in p {
                    for pt in r {
                        pt[0] += dx;
                        pt[1] += dy;
                    }
                }
            }
        }
        GeoValue::GeometryCollection(gs) => {
            for g in gs {
                translate(&mut g.value, dx, dy);
            }
        }
    }
}

// ===== 命令 =====

/// 顶点移动（顶点拖拽的内核语义：定位要素 → 几何路径 → 顶点）。
#[derive(Debug, Clone)]
pub struct MoveVertex {
    /// 要素下标。
    pub index: usize,
    /// 几何路径。
    pub path: GeomPath,
    /// 旧坐标（revert 用；调用方在拖拽开始时捕获）。
    pub old_pos: Vec<f64>,
    /// 新坐标。
    pub new_pos: Vec<f64>,
}

impl EditCommand for MoveVertex {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        *vertex_mut(geometry_mut(f, self.index)?, &self.path)? = self.new_pos.clone();
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        *vertex_mut(geometry_mut(f, self.index)?, &self.path)? = self.old_pos.clone();
        Ok(())
    }
    fn describe(&self) -> String {
        format!("移动顶点（要素 #{}）", self.index)
    }
}

/// 整要素平移。
#[derive(Debug, Clone)]
pub struct MoveFeature {
    /// 要素下标。
    pub index: usize,
    /// 位移（数据坐标单位）。
    pub dx: f64,
    /// 位移（数据坐标单位）。
    pub dy: f64,
}

impl EditCommand for MoveFeature {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        translate(geometry_mut(f, self.index)?, self.dx, self.dy);
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        translate(geometry_mut(f, self.index)?, -self.dx, -self.dy);
        Ok(())
    }
    fn describe(&self) -> String {
        format!("平移要素 #{}", self.index)
    }
}

/// 删除要素（被删要素留存 revert 用；构造时用 [`DeleteFeatures::new`] 捕获）。
#[derive(Debug, Clone)]
pub struct DeleteFeatures {
    /// 删除下标（升序去重）。
    pub indices: Vec<usize>,
    /// 被删要素及其原下标（升序留存）。
    pub deleted: Vec<(usize, Feature)>,
}

impl DeleteFeatures {
    /// 构造并捕获被删要素（下标越界报错，不产生半态）。
    pub fn new(collection: &FeatureCollection, indices: &[usize]) -> Result<Self, KanyuError> {
        let mut idx = indices.to_vec();
        idx.sort_unstable();
        idx.dedup();
        let mut deleted = Vec::with_capacity(idx.len());
        for &i in &idx {
            let f = collection
                .features
                .get(i)
                .ok_or_else(|| err(format!("要素下标越界: {i}")))?;
            deleted.push((i, f.clone()));
        }
        Ok(Self {
            indices: idx,
            deleted,
        })
    }
}

impl EditCommand for DeleteFeatures {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        // 降序删除避免下标位移。
        for &i in self.indices.iter().rev() {
            if i >= c.features.len() {
                return Err(err(format!("要素下标越界: {i}")));
            }
            c.features.remove(i);
        }
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        // 升序按原下标插回。
        for (i, f) in &self.deleted {
            let pos = (*i).min(c.features.len());
            c.features.insert(pos, f.clone());
        }
        Ok(())
    }
    fn describe(&self) -> String {
        format!("删除 {} 个要素", self.indices.len())
    }
}

/// 插入要素。
#[derive(Debug, Clone)]
pub struct InsertFeature {
    /// 要素。
    pub feature: Feature,
    /// 插入位置（超过尾部则追加）。
    pub index: usize,
}

impl EditCommand for InsertFeature {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let pos = self.index.min(c.features.len());
        c.features.insert(pos, self.feature.clone());
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let pos = self.index.min(c.features.len().saturating_sub(1));
        if c.features.is_empty() {
            return Err(err("集合为空，无法逆回插入"));
        }
        c.features.remove(pos);
        Ok(())
    }
    fn describe(&self) -> String {
        format!("插入要素（位置 {}）", self.index)
    }
}

/// 属性整体更新（属性表单元格编辑的内核语义：整行属性 Map 置换）。
#[derive(Debug, Clone)]
pub struct UpdateProperties {
    /// 要素下标。
    pub index: usize,
    /// 旧属性（revert 用；None = 原无属性表）。
    pub old: Option<Map<String, Json>>,
    /// 新属性。
    pub new: Option<Map<String, Json>>,
}

impl EditCommand for UpdateProperties {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        f.properties = self.new.clone();
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        f.properties = self.old.clone();
        Ok(())
    }
    fn describe(&self) -> String {
        format!("更新属性（要素 #{}）", self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::Geometry;

    fn point_feature(x: f64, y: f64) -> Feature {
        Feature {
            bbox: None,
            geometry: Some(Geometry::new(GeoValue::Point(vec![x, y]))),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn polygon_feature() -> Feature {
        Feature {
            bbox: None,
            geometry: Some(Geometry::new(GeoValue::Polygon(vec![vec![
                vec![0.0, 0.0],
                vec![1.0, 0.0],
                vec![1.0, 1.0],
                vec![0.0, 0.0],
            ]]))),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn coll(features: Vec<Feature>) -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }
    }

    fn xy(f: &Feature) -> &Vec<f64> {
        match &f.geometry.as_ref().unwrap().value {
            GeoValue::Point(p) => p,
            _ => panic!(),
        }
    }

    #[test]
    fn move_vertex_point_and_polygon_ring() {
        let mut c = coll(vec![point_feature(1.0, 2.0), polygon_feature()]);
        let cmd = MoveVertex {
            index: 0,
            path: GeomPath {
                part: 0,
                ring: 0,
                vertex: 0,
            },
            old_pos: vec![1.0, 2.0],
            new_pos: vec![5.0, 6.0],
        };
        cmd.apply(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![5.0, 6.0]);
        cmd.revert(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![1.0, 2.0]);
        // 面外环顶点 1（[1,0] → [2,0]）。
        let cmd2 = MoveVertex {
            index: 1,
            path: GeomPath {
                part: 0,
                ring: 0,
                vertex: 1,
            },
            old_pos: vec![1.0, 0.0],
            new_pos: vec![2.0, 0.0],
        };
        cmd2.apply(&mut c).unwrap();
        match &c.features[1].geometry.as_ref().unwrap().value {
            GeoValue::Polygon(rings) => assert_eq!(rings[0][1], vec![2.0, 0.0]),
            _ => panic!(),
        }
        // 越界路径中文错误。
        let bad = MoveVertex {
            index: 0,
            path: GeomPath {
                part: 9,
                ring: 0,
                vertex: 0,
            },
            old_pos: vec![],
            new_pos: vec![],
        };
        assert!(bad.apply(&mut c).unwrap_err().to_string().contains("越界"));
    }

    #[test]
    fn move_feature_and_revert() {
        let mut c = coll(vec![point_feature(1.0, 1.0)]);
        let cmd = MoveFeature {
            index: 0,
            dx: 10.0,
            dy: -5.0,
        };
        cmd.apply(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![11.0, -4.0]);
        cmd.revert(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![1.0, 1.0]);
    }

    #[test]
    fn delete_and_revert_keeps_order() {
        let mut c = coll(vec![
            point_feature(0.0, 0.0),
            point_feature(1.0, 1.0),
            point_feature(2.0, 2.0),
        ]);
        let cmd = DeleteFeatures::new(&c, &[0, 2]).unwrap();
        cmd.apply(&mut c).unwrap();
        assert_eq!(c.features.len(), 1);
        assert_eq!(xy(&c.features[0]), &vec![1.0, 1.0]);
        cmd.revert(&mut c).unwrap();
        assert_eq!(c.features.len(), 3);
        assert_eq!(xy(&c.features[0]), &vec![0.0, 0.0]);
        assert_eq!(xy(&c.features[2]), &vec![2.0, 2.0]);
        // 越界下标构造即报错。
        assert!(DeleteFeatures::new(&c, &[9]).is_err());
    }

    #[test]
    fn insert_and_revert() {
        let mut c = coll(vec![point_feature(0.0, 0.0)]);
        let cmd = InsertFeature {
            feature: point_feature(9.0, 9.0),
            index: 0,
        };
        cmd.apply(&mut c).unwrap();
        assert_eq!(c.features.len(), 2);
        assert_eq!(xy(&c.features[0]), &vec![9.0, 9.0]);
        cmd.revert(&mut c).unwrap();
        assert_eq!(c.features.len(), 1);
    }

    #[test]
    fn update_properties_roundtrip() {
        let mut c = coll(vec![point_feature(0.0, 0.0)]);
        let mut new_props = Map::new();
        new_props.insert("name".to_string(), Json::from("甲"));
        let cmd = UpdateProperties {
            index: 0,
            old: None,
            new: Some(new_props),
        };
        cmd.apply(&mut c).unwrap();
        assert_eq!(
            c.features[0].properties.as_ref().unwrap().get("name"),
            Some(&Json::from("甲"))
        );
        cmd.revert(&mut c).unwrap();
        assert!(c.features[0].properties.is_none());
    }

    #[test]
    fn history_integration() {
        use crate::History;
        let mut h = History::default();
        let mut c = coll(vec![point_feature(0.0, 0.0)]);
        h.push(
            Box::new(MoveFeature {
                index: 0,
                dx: 1.0,
                dy: 1.0,
            }),
            &mut c,
        )
        .unwrap();
        assert_eq!(xy(&c.features[0]), &vec![1.0, 1.0]);
        h.undo(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![0.0, 0.0]);
        h.redo(&mut c).unwrap();
        assert_eq!(xy(&c.features[0]), &vec![1.0, 1.0]);
    }
}
