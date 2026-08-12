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

/// 环组（外环 + 既有内环）→ geo 多边形（geojson→geo_types 手工转换：
/// 坐标对直映，无需经 core——kanyu-edit 自持，避免 core 新增导出）。
fn geo_polygon(rings: &[Vec<Vec<f64>>]) -> Option<geo::Polygon<f64>> {
    let (outer, holes) = rings.split_first()?;
    let to_ls = |r: &[Vec<f64>]| {
        geo::LineString(r.iter().map(|p| geo::Coord { x: p[0], y: p[1] }).collect())
    };
    Some(geo::Polygon::new(
        to_ls(outer),
        holes.iter().map(|r| to_ls(r)).collect(),
    ))
}

/// 校验环可作为指定面 part 的洞（纯函数）：目标为面几何、part 在界内、
/// 环自动闭合后**完全位于面内**（geo Contains——越出外环或落入既有洞均判负）。
pub fn validate_hole(
    c: &FeatureCollection,
    index: usize,
    part: usize,
    ring: &[Vec<f64>],
) -> Result<(), KanyuError> {
    let f = c
        .features
        .get(index)
        .ok_or_else(|| err(format!("要素下标越界: {index}")))?;
    let geom = f
        .geometry
        .as_ref()
        .ok_or_else(|| err(format!("要素 #{index} 无几何")))?;
    let rings = match &geom.value {
        GeoValue::Polygon(rings) if part == 0 => rings,
        GeoValue::MultiPolygon(polys) => polys
            .get(part)
            .ok_or_else(|| err(format!("子面下标越界: part {part}")))?,
        GeoValue::Polygon(_) => return Err(err(format!("子面下标越界: part {part}（单面恒 0）"))),
        _ => {
            return Err(err(
                "目标要素不是面几何——洞只能加在 Polygon/MultiPolygon 上",
            ))
        }
    };
    if ring.len() < 4 {
        return Err(err("洞环至少需要 3 个顶点（闭合后 4 点）"));
    }
    let Some(poly) = geo_polygon(rings) else {
        return Err(err("目标面外环为空"));
    };
    let hole = geo::LineString(
        ring.iter()
            .map(|p| geo::Coord { x: p[0], y: p[1] })
            .collect(),
    );
    if !geo::Contains::contains(&poly, &hole) {
        return Err(err("洞环须完全位于面内（不越出外环、不与既有洞相交）"));
    }
    // 严格性补充：geo Contains 对边界重叠按 covers 语义放行——
    // 洞与外环/既有洞边界相接会产生非法多边形，显式判负。
    if geo::Intersects::intersects(poly.exterior(), &hole)
        || poly
            .interiors()
            .iter()
            .any(|r| geo::Intersects::intersects(r, &hole))
    {
        return Err(err("洞环不得与外环或既有洞的边界相接"));
    }
    Ok(())
}

/// 面内挖洞（追加内环；Polygon part 恒 0，MultiPolygon 为子面下标）。
/// apply 先经 [`validate_hole`] 校验（越界中文错误，不改动集合）。
#[derive(Debug, Clone)]
pub struct AddHole {
    /// 要素下标。
    pub index: usize,
    /// 子面下标（Polygon 恒 0）。
    pub part: usize,
    /// 洞环坐标（未闭合时 apply 自动闭合）。
    pub ring: Vec<Vec<f64>>,
}

impl EditCommand for AddHole {
    fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let mut ring = self.ring.clone();
        if ring.first() != ring.last() {
            if let Some(first) = ring.first().cloned() {
                ring.push(first); // 自动闭合（壳层绘制状态机已闭合，此处兜底）
            }
        }
        validate_hole(c, self.index, self.part, &ring)?;
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        match geometry_mut(f, self.index)? {
            GeoValue::Polygon(rings) => rings.push(ring),
            GeoValue::MultiPolygon(polys) => polys
                .get_mut(self.part)
                .ok_or_else(|| err(format!("子面下标越界: part {}", self.part)))?
                .push(ring),
            _ => return Err(err("目标要素不是面几何")),
        }
        Ok(())
    }
    fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
        let f = c
            .features
            .get_mut(self.index)
            .ok_or_else(|| err(format!("要素下标越界: {}", self.index)))?;
        let rings = match geometry_mut(f, self.index)? {
            GeoValue::Polygon(rings) => rings,
            GeoValue::MultiPolygon(polys) => polys
                .get_mut(self.part)
                .ok_or_else(|| err(format!("子面下标越界: part {}", self.part)))?,
            _ => return Err(err("目标要素不是面几何")),
        };
        if rings.len() < 2 {
            return Err(err("面无内环，无法逆回挖洞"));
        }
        rings.pop(); // apply 追加在尾部，逆回即弹出末环
        Ok(())
    }
    fn describe(&self) -> String {
        format!("面内挖洞（要素 #{}）", self.index)
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

    fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec<f64>> {
        vec![
            vec![x0, y0],
            vec![x1, y0],
            vec![x1, y1],
            vec![x0, y1],
            vec![x0, y0],
        ]
    }

    #[test]
    fn add_hole_apply_revert_and_validation() {
        // 单面（0..10 方块）。
        let mut c = coll(vec![Feature {
            bbox: None,
            geometry: Some(Geometry::new(GeoValue::Polygon(vec![square(
                0.0, 0.0, 10.0, 10.0,
            )]))),
            id: None,
            properties: None,
            foreign_members: None,
        }]);
        // 未闭合环自动闭合；apply 后多一条内环。
        let mut ring = square(2.0, 2.0, 4.0, 4.0);
        ring.pop();
        let cmd = AddHole {
            index: 0,
            part: 0,
            ring,
        };
        cmd.apply(&mut c).unwrap();
        match &c.features[0].geometry.as_ref().unwrap().value {
            GeoValue::Polygon(rings) => {
                assert_eq!(rings.len(), 2);
                assert_eq!(rings[1].first(), rings[1].last(), "自动闭合");
            }
            _ => panic!(),
        }
        cmd.revert(&mut c).unwrap();
        match &c.features[0].geometry.as_ref().unwrap().value {
            GeoValue::Polygon(rings) => assert_eq!(rings.len(), 1),
            _ => panic!(),
        }
        // 越出外环 → 中文校验错误且不改动集合。
        let e = AddHole {
            index: 0,
            part: 0,
            ring: square(20.0, 20.0, 22.0, 22.0),
        }
        .apply(&mut c)
        .unwrap_err();
        assert!(e.to_string().contains("完全位于面内"), "{e}");
        match &c.features[0].geometry.as_ref().unwrap().value {
            GeoValue::Polygon(rings) => assert_eq!(rings.len(), 1, "校验失败不得改动"),
            _ => panic!(),
        }
        // 压外环边界（非严格内含）也判负。
        assert!(AddHole {
            index: 0,
            part: 0,
            ring: square(0.0, 0.0, 5.0, 5.0),
        }
        .apply(&mut c)
        .is_err());
        // 非面目标。
        let cp = coll(vec![point_feature(1.0, 1.0)]);
        assert!(validate_hole(&cp, 0, 0, &square(0.0, 0.0, 1.0, 1.0)).is_err());
        // 落入既有洞判负。
        AddHole {
            index: 0,
            part: 0,
            ring: square(2.0, 2.0, 4.0, 4.0),
        }
        .apply(&mut c)
        .unwrap();
        assert!(AddHole {
            index: 0,
            part: 0,
            ring: square(2.5, 2.5, 3.5, 3.5), // 全在既有洞内
        }
        .apply(&mut c)
        .is_err());
    }

    #[test]
    fn add_hole_multipolygon_part() {
        let mp = GeoValue::MultiPolygon(vec![
            vec![square(0.0, 0.0, 10.0, 10.0)],
            vec![square(100.0, 100.0, 110.0, 110.0)],
        ]);
        let mut c = coll(vec![Feature {
            bbox: None,
            geometry: Some(Geometry::new(mp)),
            id: None,
            properties: None,
            foreign_members: None,
        }]);
        // part 1 挖洞成功；part 9 越界中文错误。
        AddHole {
            index: 0,
            part: 1,
            ring: square(102.0, 102.0, 104.0, 104.0),
        }
        .apply(&mut c)
        .unwrap();
        match &c.features[0].geometry.as_ref().unwrap().value {
            GeoValue::MultiPolygon(polys) => {
                assert_eq!(polys[0].len(), 1);
                assert_eq!(polys[1].len(), 2);
            }
            _ => panic!(),
        }
        assert!(AddHole {
            index: 0,
            part: 9,
            ring: square(0.0, 0.0, 1.0, 1.0),
        }
        .apply(&mut c)
        .is_err());
        // part 0 的环投到 part 1 坐标域判负（环不在该子面内）。
        assert!(AddHole {
            index: 0,
            part: 1,
            ring: square(1.0, 1.0, 2.0, 2.0),
        }
        .apply(&mut c)
        .is_err());
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
