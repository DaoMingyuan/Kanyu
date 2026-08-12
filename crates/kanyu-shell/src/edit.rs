//! 壳层编辑模式（Phase 3 可见闭环）：编辑会话 + 画布命中检测纯函数。
//!
//! - 会话（[`EditSession`]）：同一时刻仅一个图层处于编辑态（ArcGIS Pro 编辑
//!   会话语义）；命令即时应用到图层集合（画布即时可见），History 供撤销/重做；
//!   「放弃编辑」= 逐条逆回到会话起点。
//! - 命中检测（[`hit_vertex`]/[`hit_feature`]）：屏幕坐标 + 容差像素（统一
//!   在屏幕空间度量，跨缩放级别手感一致）；点=最近点、线=点到折线距离、
//!   面=射线法包含；纯函数配单测。
//! - 线/面绘制（[`DrawState`]）：单击加顶点 → 双击/Enter 完成（面自动闭合）、
//!   Backspace 撤最近顶点、Esc 放弃；状态机纯函数配单测。
//! - 工具/图层几何匹配（[`tool_geometry_match`]）：点/线/面添加工具仅可用于
//!   同型图层（空图层放行——首要素定型），不匹配给中文错误。

use geojson::{FeatureCollection, Value as GeoValue};
use kanyu_edit::{GeomPath, History};

use crate::scene3d::data_to_canvas;
use crate::view::BBox;

/// 命中容差（屏幕像素）。
pub const HIT_TOL_PX: f32 = 8.0;

/// 编辑工具。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditTool {
    /// 选择（点击选中要素）。
    Select,
    /// 顶点编辑（拖拽顶点句柄）。
    Vertex,
    /// 移动要素（整体拖动）。
    Move,
    /// 添加点（点击插入）。
    AddPoint,
    /// 添加线（单击加顶点，双击/Enter 完成）。
    AddLine,
    /// 添加面（单击加顶点，双击/Enter 完成并自动闭合）。
    AddPolygon,
    /// 删除要素（点击选中后删除）。
    Delete,
}

impl EditTool {
    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            EditTool::Select => "选择",
            EditTool::Vertex => "顶点编辑",
            EditTool::Move => "移动要素",
            EditTool::AddPoint => "添加点",
            EditTool::AddLine => "添加线",
            EditTool::AddPolygon => "添加面",
            EditTool::Delete => "删除要素",
        }
    }
}

/// 绘制种类（线/面）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DrawKind {
    /// 线（≥2 顶点）。
    Line,
    /// 面（≥3 顶点，完成时自动闭合）。
    Polygon,
}

impl DrawKind {
    /// 最少顶点数。
    pub fn min_verts(self) -> usize {
        match self {
            DrawKind::Line => 2,
            DrawKind::Polygon => 3,
        }
    }

    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            DrawKind::Line => "线",
            DrawKind::Polygon => "面",
        }
    }
}

/// 线/面绘制中状态（编辑会话持有；方法为纯数据操作，配单测）。
#[derive(Debug, Clone)]
pub struct DrawState {
    /// 绘制种类。
    pub kind: DrawKind,
    /// 已定顶点（数据坐标 [x, y]，按加点顺序）。
    pub verts: Vec<[f64; 2]>,
}

impl DrawState {
    /// 开始一次绘制。
    pub fn new(kind: DrawKind) -> Self {
        Self {
            kind,
            verts: Vec::new(),
        }
    }

    /// 加一个顶点。
    pub fn add(&mut self, pt: (f64, f64)) {
        self.verts.push([pt.0, pt.1]);
    }

    /// 撤最近顶点（无顶点可撤返回 false）。
    pub fn undo(&mut self) -> bool {
        self.verts.pop().is_some()
    }

    /// 完成构造几何：线 ≥2 点 / 面 ≥3 点且自动闭合；点数不足给中文错误。
    pub fn finish(&self) -> Result<GeoValue, String> {
        let n = self.verts.len();
        let min = self.kind.min_verts();
        if n < min {
            return Err(format!(
                "{}要素至少需要 {min} 个顶点（当前 {n} 个）",
                self.kind.label()
            ));
        }
        let pts: Vec<Vec<f64>> = self.verts.iter().map(|p| p.to_vec()).collect();
        Ok(match self.kind {
            DrawKind::Line => GeoValue::LineString(pts),
            DrawKind::Polygon => {
                let mut ring = pts;
                if ring.first() != ring.last() {
                    if let Some(first) = ring.first().cloned() {
                        ring.push(first);
                    }
                }
                GeoValue::Polygon(vec![ring])
            }
        })
    }
}

/// 绘制工具 → 种类（非绘制工具为 None）。
pub fn draw_kind_of(tool: EditTool) -> Option<DrawKind> {
    match tool {
        EditTool::AddLine => Some(DrawKind::Line),
        EditTool::AddPolygon => Some(DrawKind::Polygon),
        _ => None,
    }
}

/// 工具与图层几何类型匹配判定（类型名取图层 summary.geometry_types 的 WKB 名）。
/// 空图层（无类型记录）放行——首要素定型；不匹配给中文错误。
pub fn tool_geometry_match(tool: EditTool, geometry_types: &[String]) -> Result<(), String> {
    let (expected, zh) = match tool {
        EditTool::AddPoint => (&["Point", "MultiPoint"][..], "点"),
        EditTool::AddLine => (&["LineString", "MultiLineString"][..], "线"),
        EditTool::AddPolygon => (&["Polygon", "MultiPolygon"][..], "面"),
        _ => return Ok(()),
    };
    if geometry_types.is_empty()
        || geometry_types
            .iter()
            .any(|t| expected.contains(&t.as_str()))
    {
        return Ok(());
    }
    Err(format!(
        "「{}」工具仅适用于{zh}图层（当前图层几何类型：{}）",
        tool.label(),
        geometry_types.join("/")
    ))
}

/// 编辑会话（app 持有；`target` = 目标图层 id）。
pub struct EditSession {
    /// 目标图层 id。
    pub target: String,
    /// 目标图层显示名（状态栏用）。
    pub target_name: String,
    /// 撤销/重做历史。
    pub history: History,
    /// 当前工具。
    pub tool: EditTool,
    /// 选中要素（Select/Delete 用）。
    pub selected: Option<usize>,
    /// 线/面绘制中状态（绘制工具激活且已加点时 Some）。
    pub drawing: Option<DrawState>,
}

impl EditSession {
    /// 开启会话（默认选择工具）。
    pub fn new(target: String, target_name: String) -> Self {
        Self {
            target,
            target_name,
            history: History::default(),
            tool: EditTool::Select,
            selected: None,
            drawing: None,
        }
    }
}

/// 画布编辑手势产出（app 映射为 kanyu-edit 命令入 History）。
#[derive(Debug, Clone)]
pub enum EditAction {
    /// 顶点移动（old 为拖拽开始时捕获）。
    MoveVertex {
        feature: usize,
        path: GeomPath,
        old: Vec<f64>,
        new: Vec<f64>,
    },
    /// 整要素平移。
    MoveFeature { feature: usize, dx: f64, dy: f64 },
    /// 插入点。
    InsertPoint { pos: (f64, f64) },
    /// 选中（None = 点空白取消）。
    Select(Option<usize>),
    /// 请求删除选中要素。
    DeleteSelected,
    /// 绘制：加一个顶点（数据坐标）。
    DrawAddVertex { pos: (f64, f64) },
    /// 绘制：撤最近顶点。
    DrawUndoVertex,
    /// 绘制：放弃本次绘制。
    DrawCancel,
    /// 绘制：完成（构造几何 → 插入要素；点数不足由 app 给中文提示并保留现场）。
    DrawFinish,
}

// ===== 命中检测（纯函数）=====

/// 屏幕坐标命中顶点：返回最近的容差内顶点（要素下标 + 几何路径 + 数据坐标）。
pub fn hit_vertex(
    collection: &FeatureCollection,
    bbox: BBox,
    w: f64,
    h: f64,
    pos: (f32, f32),
    tol_px: f32,
) -> Option<(usize, GeomPath, Vec<f64>)> {
    let mut best: Option<(f32, usize, GeomPath, Vec<f64>)> = None;
    walk_vertices(collection, bbox, w, h, |fi, path, screen, data| {
        let d = ((screen.0 - pos.0).powi(2) + (screen.1 - pos.1).powi(2)).sqrt();
        if d <= tol_px && best.as_ref().is_none_or(|(bd, _, _, _)| d < *bd) {
            best = Some((d, fi, path, data.to_vec()));
        }
    });
    best.map(|(_, fi, path, data)| (fi, path, data))
}

/// 屏幕坐标命中要素：点=最近容差内；线=点到折线距离；面=射线法包含。
pub fn hit_feature(
    collection: &FeatureCollection,
    bbox: BBox,
    w: f64,
    h: f64,
    pos: (f32, f32),
    tol_px: f32,
) -> Option<usize> {
    for (fi, feature) in collection.features.iter().enumerate() {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        if feature_hit(&geom.value, bbox, w, h, pos, tol_px) {
            return Some(fi);
        }
    }
    None
}

/// 遍历全部顶点（屏幕坐标 + 数据坐标回调）。
fn walk_vertices(
    collection: &FeatureCollection,
    bbox: BBox,
    w: f64,
    h: f64,
    mut f: impl FnMut(usize, GeomPath, (f32, f32), &[f64]),
) {
    let to_screen = |pt: &[f64]| data_to_canvas(pt[0], pt[1], bbox, w, h);
    for (fi, feature) in collection.features.iter().enumerate() {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        let path = GeomPath {
            part: 0,
            ring: 0,
            vertex: 0,
        };
        walk_value(&geom.value, path, fi, &to_screen, &mut f);
    }
}

/// 顶点访问回调：（要素序号，几何路径，屏幕坐标，数据坐标）。
type VertexVisit<'a> = dyn FnMut(usize, GeomPath, (f32, f32), &[f64]) + 'a;

fn walk_value(
    v: &GeoValue,
    path: GeomPath,
    fi: usize,
    to_screen: &dyn Fn(&[f64]) -> (f32, f32),
    f: &mut VertexVisit<'_>,
) {
    match v {
        GeoValue::Point(p) => f(fi, path, to_screen(p), p),
        GeoValue::MultiPoint(ps) => {
            for (i, p) in ps.iter().enumerate() {
                f(
                    fi,
                    GeomPath {
                        part: i,
                        ring: 0,
                        vertex: 0,
                    },
                    to_screen(p),
                    p,
                );
            }
        }
        GeoValue::LineString(line) => {
            for (i, p) in line.iter().enumerate() {
                f(
                    fi,
                    GeomPath {
                        part: 0,
                        ring: 0,
                        vertex: i,
                    },
                    to_screen(p),
                    p,
                );
            }
        }
        GeoValue::MultiLineString(lines) => {
            for (pi, line) in lines.iter().enumerate() {
                for (i, p) in line.iter().enumerate() {
                    f(
                        fi,
                        GeomPath {
                            part: pi,
                            ring: 0,
                            vertex: i,
                        },
                        to_screen(p),
                        p,
                    );
                }
            }
        }
        GeoValue::Polygon(rings) => {
            for (ri, ring) in rings.iter().enumerate() {
                for (i, p) in ring.iter().enumerate() {
                    f(
                        fi,
                        GeomPath {
                            part: 0,
                            ring: ri,
                            vertex: i,
                        },
                        to_screen(p),
                        p,
                    );
                }
            }
        }
        GeoValue::MultiPolygon(polys) => {
            for (pi, rings) in polys.iter().enumerate() {
                for (ri, ring) in rings.iter().enumerate() {
                    for (i, p) in ring.iter().enumerate() {
                        f(
                            fi,
                            GeomPath {
                                part: pi,
                                ring: ri,
                                vertex: i,
                            },
                            to_screen(p),
                            p,
                        );
                    }
                }
            }
        }
        GeoValue::GeometryCollection(_) => {}
    }
}

/// 单要素命中判定。
fn feature_hit(v: &GeoValue, bbox: BBox, w: f64, h: f64, pos: (f32, f32), tol: f32) -> bool {
    let to_s = |p: &[f64]| data_to_canvas(p[0], p[1], bbox, w, h);
    match v {
        GeoValue::Point(p) => dist(to_s(p), pos) <= tol,
        GeoValue::MultiPoint(ps) => ps.iter().any(|p| dist(to_s(p), pos) <= tol),
        GeoValue::LineString(line) => line_hit(line, &to_s, pos, tol),
        GeoValue::MultiLineString(lines) => lines.iter().any(|l| line_hit(l, &to_s, pos, tol)),
        GeoValue::Polygon(_) | GeoValue::MultiPolygon(_) => {
            // 面：射线法（数据坐标即可，与屏幕映射单调一致）。
            let data = crate::view::screen_to_data(f64::from(pos.0), f64::from(pos.1), bbox, w, h);
            point_in_feature(v, data)
        }
        GeoValue::GeometryCollection(gs) => gs
            .iter()
            .any(|g| feature_hit(&g.value, bbox, w, h, pos, tol)),
    }
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// 点到折线距离（屏幕空间）。
fn line_hit(
    line: &[Vec<f64>],
    to_s: &dyn Fn(&[f64]) -> (f32, f32),
    pos: (f32, f32),
    tol: f32,
) -> bool {
    line.windows(2).any(|seg| {
        let a = to_s(&seg[0]);
        let b = to_s(&seg[1]);
        point_seg_dist(pos, a, b) <= tol
    })
}

/// 点到线段距离。
fn point_seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    if len2 < 1e-12 {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0);
    dist(p, (a.0 + abx * t, a.1 + aby * t))
}

/// 点在要素内（射线法，数据坐标）。
fn point_in_feature(v: &GeoValue, pt: (f64, f64)) -> bool {
    match v {
        GeoValue::Polygon(rings) => in_rings(rings, pt),
        GeoValue::MultiPolygon(polys) => polys.iter().any(|r| in_rings(r, pt)),
        _ => false,
    }
}

/// 外环包含且不在洞内。
fn in_rings(rings: &[Vec<Vec<f64>>], pt: (f64, f64)) -> bool {
    match rings.split_first() {
        Some((outer, holes)) => in_ring(outer, pt) && !holes.iter().any(|hole| in_ring(hole, pt)),
        None => false,
    }
}

/// 射线法。
fn in_ring(ring: &[Vec<f64>], (x, y): (f64, f64)) -> bool {
    let mut inside = false;
    let mut j = ring.len().saturating_sub(1);
    for i in 0..ring.len() {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// 顶点屏幕位置清单（编辑态句柄绘制用）。
pub fn vertex_positions(
    collection: &FeatureCollection,
    bbox: BBox,
    w: f64,
    h: f64,
) -> Vec<((usize, GeomPath), (f32, f32))> {
    let mut out = Vec::new();
    walk_vertices(collection, bbox, w, h, |fi, path, screen, _data| {
        out.push(((fi, path), screen));
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::{Feature, Geometry};

    fn feat(v: GeoValue) -> Feature {
        Feature {
            bbox: None,
            geometry: Some(Geometry::new(v)),
            id: None,
            properties: None,
            foreign_members: None,
        }
    }

    fn coll(vs: Vec<GeoValue>) -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: vs.into_iter().map(feat).collect(),
            foreign_members: None,
        }
    }

    /// 视口 [0,0,10,10]，画布 100×100（1px = 0.1 数据单位）。
    const BBOX: BBox = [0.0, 0.0, 10.0, 10.0];

    #[test]
    fn hit_vertex_tolerance_and_path() {
        let c = coll(vec![GeoValue::Polygon(vec![vec![
            vec![0.0, 0.0],
            vec![5.0, 0.0],
            vec![5.0, 5.0],
            vec![0.0, 0.0],
        ]])]);
        // 顶点 (5,5) 在屏幕 (50, 50)（y 翻转：数据 y=5 → 屏幕 50）。
        let hit = hit_vertex(&c, BBOX, 100.0, 100.0, (51.0, 49.0), HIT_TOL_PX).unwrap();
        assert_eq!(hit.0, 0);
        assert_eq!(hit.1.vertex, 2);
        assert_eq!(hit.2, vec![5.0, 5.0]);
        // 远处不命中。
        assert!(hit_vertex(&c, BBOX, 100.0, 100.0, (90.0, 90.0), HIT_TOL_PX).is_none());
    }

    #[test]
    fn hit_feature_point_line_polygon() {
        let c = coll(vec![
            GeoValue::Point(vec![2.0, 8.0]), // 屏幕 (20,20)
            GeoValue::LineString(vec![vec![0.0, 5.0], vec![10.0, 5.0]]), // 横线
            GeoValue::Polygon(vec![vec![
                vec![6.0, 6.0],
                vec![9.0, 6.0],
                vec![9.0, 9.0],
                vec![6.0, 6.0],
            ]]),
        ]);
        // 点附近命中。
        assert_eq!(
            hit_feature(&c, BBOX, 100.0, 100.0, (21.0, 21.0), HIT_TOL_PX),
            Some(0)
        );
        // 线上命中。
        assert_eq!(
            hit_feature(&c, BBOX, 100.0, 100.0, (50.0, 52.0), HIT_TOL_PX),
            Some(1)
        );
        // 面内命中（屏幕 y 翻转：面数据 y6..9 → 屏幕 y10..40）。
        assert_eq!(
            hit_feature(&c, BBOX, 100.0, 100.0, (75.0, 25.0), HIT_TOL_PX),
            Some(2)
        );
        // 空白不命中。
        assert_eq!(hit_feature(&c, BBOX, 100.0, 100.0, (1.0, 99.0), 2.0), None);
    }

    #[test]
    fn point_in_ring_with_hole() {
        let rings = vec![
            vec![
                vec![0.0, 0.0],
                vec![10.0, 0.0],
                vec![10.0, 10.0],
                vec![0.0, 0.0],
            ],
            vec![
                vec![4.0, 4.0],
                vec![6.0, 4.0],
                vec![6.0, 6.0],
                vec![4.0, 4.0],
            ],
        ];
        assert!(in_rings(&rings, (2.0, 2.0))); // 外环内、洞外
        assert!(!in_rings(&rings, (5.0, 5.0))); // 洞内
        assert!(!in_rings(&rings, (20.0, 2.0))); // 外环外
    }

    #[test]
    fn session_basics() {
        let mut s = EditSession::new("lyr".into(), "示例图层".into());
        assert_eq!(s.tool, EditTool::Select);
        assert!(!s.history.can_undo());
        s.tool = EditTool::Vertex;
        assert_eq!(s.tool.label(), "顶点编辑");
        assert!(s.drawing.is_none());
    }

    #[test]
    fn draw_state_machine_line() {
        let mut d = DrawState::new(DrawKind::Line);
        // 点数不足：完成给中文错误（当前 0/1 个）。
        assert!(d.finish().unwrap_err().contains("至少需要 2 个顶点"));
        d.add((0.0, 0.0));
        assert!(d.finish().is_err());
        d.add((1.0, 1.0));
        d.add((2.0, 0.0));
        // 撤最近顶点。
        assert!(d.undo());
        assert_eq!(d.verts.len(), 2);
        // 完成 → LineString（不闭合）。
        match d.finish().unwrap() {
            GeoValue::LineString(line) => {
                assert_eq!(line.len(), 2);
                assert_eq!(line[0], vec![0.0, 0.0]);
            }
            other => panic!("应为 LineString: {other:?}"),
        }
        // 撤空后 false。
        assert!(d.undo());
        assert!(d.undo());
        assert!(!d.undo());
    }

    #[test]
    fn draw_state_machine_polygon_auto_close() {
        let mut d = DrawState::new(DrawKind::Polygon);
        d.add((0.0, 0.0));
        d.add((4.0, 0.0));
        assert!(d.finish().unwrap_err().contains("至少需要 3 个顶点"));
        d.add((4.0, 4.0));
        match d.finish().unwrap() {
            GeoValue::Polygon(rings) => {
                assert_eq!(rings.len(), 1);
                assert_eq!(rings[0].len(), 4, "自动闭合");
                assert_eq!(rings[0].first(), rings[0].last());
            }
            other => panic!("应为 Polygon: {other:?}"),
        }
        // 已闭合（首==尾）不重复补点。
        let mut d2 = DrawState::new(DrawKind::Polygon);
        d2.add((0.0, 0.0));
        d2.add((4.0, 0.0));
        d2.add((0.0, 0.0));
        match d2.finish().unwrap() {
            GeoValue::Polygon(rings) => assert_eq!(rings[0].len(), 3),
            other => panic!("应为 Polygon: {other:?}"),
        }
    }

    #[test]
    fn draw_kind_mapping_and_tool_match() {
        assert_eq!(draw_kind_of(EditTool::AddLine), Some(DrawKind::Line));
        assert_eq!(draw_kind_of(EditTool::AddPolygon), Some(DrawKind::Polygon));
        assert_eq!(draw_kind_of(EditTool::AddPoint), None);
        assert_eq!(draw_kind_of(EditTool::Select), None);
        // 空图层放行（首要素定型）。
        assert!(tool_geometry_match(EditTool::AddPoint, &[]).is_ok());
        // 同型（含多部件）放行。
        let pts = vec!["Point".to_string()];
        assert!(tool_geometry_match(EditTool::AddPoint, &pts).is_ok());
        let polys = vec!["MultiPolygon".to_string()];
        assert!(tool_geometry_match(EditTool::AddPolygon, &polys).is_ok());
        // 异型阻止 + 中文提示。
        let e = tool_geometry_match(EditTool::AddPoint, &polys).unwrap_err();
        assert!(e.contains("仅适用于点图层"), "{e}");
        assert!(e.contains("MultiPolygon"), "{e}");
        let lines = vec!["LineString".to_string()];
        assert!(tool_geometry_match(EditTool::AddPolygon, &lines).is_err());
        assert!(tool_geometry_match(EditTool::AddLine, &lines).is_ok());
        // 非添加工具恒放行。
        assert!(tool_geometry_match(EditTool::Vertex, &polys).is_ok());
    }
}
