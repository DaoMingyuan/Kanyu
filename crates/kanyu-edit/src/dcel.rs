//! DCEL 增量拓扑内核 v1（MASTERPLAN Phase 3 深水区首个切片）。
//!
//! ## 表结构（usize 下标指针）
//!
//! - `vertices: Vec<VertexRec>`——坐标 + 出边清单（`outgoing`，构建期收集）；
//! - `half_edges: Vec<HalfEdgeRec>`——`origin/twin/next/prev/left_face` 五指针；
//! - `faces: Vec<FaceRec>`——0 号恒为**外部无界面**（[`OUTER_FACE`]）。
//!
//! ## 孔洞与边界约定（简单可靠方案，rustdoc 即契约）
//!
//! 不桥接。每个多边形外环（CCW 归一化）与内环（CW 归一化）各成一条有向
//! 环边链，环边 `left_face` 恒为**多边形面**（孔环的左侧同样是多边形实体）。
//! 孔洞内部区域建为 `FaceKind::Hole` 虚面（记录属主 `polygon`），外部区域即
//! 0 号外面。数据集边界与孔环的对侧边为 **stub 半**边（`stub=true`，
//! `next=prev=自身`，不参与绕面遍历）——绕外面/孔面的边界遍历为 v2 项
//! （需顶点角度排序接线，见 [`Dcel::outgoing_edges_ccw`] 的角度序基础）。
//!
//! ## 顶点合并与自检
//!
//! 顶点键控合并：坐标 f64 位模式精确相等（`to_bits`）。`E = 半 边数 / 2`，
//! `F` 含外面与孔虚面；[`Dcel::euler_characteristic`] 与
//! [`Dcel::components`]（并查集）满足平面连通公式 `V−E+F = 1+C`
//! （C 为连通分量数），测试以此断言。
//!
//! ## undo 友好性
//!
//! v1 不提供增量逆操作：结构为纯值表（`Clone`），分裂/编辑前整表克隆即可
//! 复原（编辑会话亦可经 [`crate::delta`] 快照通道留存前态）；增量逆操作
//! （`merge_faces_by_diagonal`）留作后续增量。

use std::collections::HashMap;

use geojson::{FeatureCollection, Value as GeoValue};
use kanyu_core::KanyuError;

/// 外部无界面固定下标（faces[0]）。
pub const OUTER_FACE: usize = 0;

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::Other(msg.into())
}

/// 面类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceKind {
    /// 外部无界面（faces[0]）。
    Outer,
    /// 多边形面（要素实体）。
    Polygon,
    /// 孔洞虚面（内环所围区域；属主见 `FaceRec::polygon`）。
    Hole,
}

/// 顶点记录。
#[derive(Debug, Clone)]
pub struct VertexRec {
    /// 坐标（x=经度, y=纬度）。
    pub coord: [f64; 2],
    /// 出边清单（构建期收集；绕点一圈见 [`Dcel::outgoing_edges_ccw`]）。
    pub outgoing: Vec<usize>,
}

/// 半边记录。
#[derive(Debug, Clone)]
pub struct HalfEdgeRec {
    /// 起点顶点。
    pub origin: usize,
    /// 对偶半边（反向同边）。
    pub twin: usize,
    /// 左侧面边界上的下一条（stub 边为自身）。
    pub next: usize,
    /// 左侧面边界上的上一条（stub 边为自身）。
    pub prev: usize,
    /// 左侧面。
    pub left_face: usize,
    /// 是否占位边（数据集边界/孔环对侧；不参与绕面遍历）。
    pub stub: bool,
    /// 是否已删除（merge 墓碑——保持下标稳定，计数/遍历时跳过）。
    pub deleted: bool,
}

/// 面记录。
#[derive(Debug, Clone)]
pub struct FaceRec {
    /// 外边界上的一条半边（外面为 None；Hole 面指向其内环的真实环边——
    /// 其 `left_face` 为所属多边形面，见模块头孔洞约定）。
    pub boundary: Option<usize>,
    /// 面类型。
    pub kind: FaceKind,
    /// 孔洞虚面清单（Polygon 面持有，Hole 面 id）。
    pub holes: Vec<usize>,
    /// 属主多边形面（Hole 面持有）。
    pub polygon: Option<usize>,
    /// 是否已删除（merge 墓碑——保持下标稳定，计数/遍历时跳过）。
    pub deleted: bool,
}

/// DCEL 结构（纯值表，Clone 即可整体快照）。
#[derive(Debug, Clone, Default)]
pub struct Dcel {
    /// 顶点表。
    pub vertices: Vec<VertexRec>,
    /// 半边表（真实边与 stub 边合计；E = len/2）。
    pub half_edges: Vec<HalfEdgeRec>,
    /// 面表（0 号为外面）。
    pub faces: Vec<FaceRec>,
}

/// 对角线分裂结果（新面与两条新半边 id，供调用方记录/断言）。
#[derive(Debug, Clone, Copy)]
pub struct SplitResult {
    /// 新面（分裂出的第二个面）。
    pub new_face: usize,
    /// 新半边 v_a → v_b（左侧面为 new_face）。
    pub edge_ab: usize,
    /// 新半边 v_b → v_a（左侧面为原面）。
    pub edge_ba: usize,
}

/// 面合并结果（[`Dcel::merge_faces`] 返回）。
#[derive(Debug, Clone, Copy)]
pub struct MergeResult {
    /// 保留面（twin 左侧面）。
    pub survivor: usize,
    /// 被吸收面（edge 左侧面，已墓碑化）。
    pub absorbed: usize,
}

/// 多边形环组（外环 + 内环清单）。
type PolygonRings = (Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>);

/// 环带符号面积（>0 为 CCW）。
fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for i in 0..ring.len() {
        let (x1, y1) = (ring[i][0], ring[i][1]);
        let (x2, y2) = (ring[(i + 1) % ring.len()][0], ring[(i + 1) % ring.len()][1]);
        sum += x1 * y2 - x2 * y1;
    }
    sum / 2.0
}

/// 射线法点入环判定（不含边界语义，仅用于孔洞归属/分裂重归属）。
fn point_in_ring(pt: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let (px, py) = (pt[0], pt[1]);
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let (x1, y1) = (ring[i][0], ring[i][1]);
        let (x2, y2) = (ring[(i + 1) % n][0], ring[(i + 1) % n][1]);
        if (y1 > py) != (y2 > py) && px < (x2 - x1) * (py - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    inside
}

impl Dcel {
    /// 从要素集合构建（Polygon/MultiPolygon 的外环+内环成面；其余几何
    /// 类型跳过；环不足 3 个相异点或含退化边报中文错误）。
    pub fn build(collection: &FeatureCollection) -> Result<Self, KanyuError> {
        let mut dcel = Dcel::default();
        // 0 号外面。
        dcel.faces.push(FaceRec {
            boundary: None,
            kind: FaceKind::Outer,
            holes: Vec::new(),
            polygon: None,
            deleted: false,
        });
        let mut vmap: HashMap<(u64, u64), usize> = HashMap::new();
        let mut emap: HashMap<(usize, usize), usize> = HashMap::new();

        // 收集环组（外环 CCW 归一、内环 CW 归一）。
        let mut polygon_rings: Vec<PolygonRings> = Vec::new();
        for feature in &collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            let mut polys: Vec<Vec<Vec<Vec<f64>>>> = Vec::new();
            match &geom.value {
                GeoValue::Polygon(rings) => polys.push(rings.clone()),
                GeoValue::MultiPolygon(ps) => polys.extend(ps.clone()),
                _ => continue, // 非面要素跳过
            }
            for rings in polys {
                let mut iter = rings.into_iter();
                let Some(exterior) = iter.next() else {
                    continue;
                };
                let exterior = normalize_ring(exterior, true)?;
                let mut holes = Vec::new();
                for hole in iter {
                    holes.push(normalize_ring(hole, false)?);
                }
                polygon_rings.push((exterior, holes));
            }
        }

        let mut vertex_of = |dcel: &mut Dcel, c: [f64; 2]| -> usize {
            let key = (c[0].to_bits(), c[1].to_bits());
            if let Some(&id) = vmap.get(&key) {
                return id;
            }
            let id = dcel.vertices.len();
            dcel.vertices.push(VertexRec {
                coord: c,
                outgoing: Vec::new(),
            });
            vmap.insert(key, id);
            id
        };

        for (exterior, holes) in &polygon_rings {
            let p_face = dcel.faces.len();
            dcel.faces.push(FaceRec {
                boundary: None,
                kind: FaceKind::Polygon,
                holes: Vec::new(),
                polygon: None,
                deleted: false,
            });
            let boundary_edge = wire_ring(
                &mut dcel,
                &mut emap,
                &mut vertex_of,
                exterior,
                p_face,
                OUTER_FACE,
            )?;
            dcel.faces[p_face].boundary = Some(boundary_edge);
            for hole in holes {
                let h_face = dcel.faces.len();
                dcel.faces.push(FaceRec {
                    boundary: None,
                    kind: FaceKind::Hole,
                    holes: Vec::new(),
                    polygon: Some(p_face),
                    deleted: false,
                });
                let edge = wire_ring(&mut dcel, &mut emap, &mut vertex_of, hole, p_face, h_face)?;
                dcel.faces[h_face].boundary = Some(edge);
                dcel.faces[p_face].holes.push(h_face);
            }
        }

        // 顶点出边清单（含 stub 边——它们代表与外面/孔面的邻接）。
        for (id, he) in dcel.half_edges.iter().enumerate() {
            let origin = he.origin;
            dcel.vertices[origin].outgoing.push(id);
        }
        Ok(dcel)
    }

    /// 绕面一圈（外边界，next 链序）；仅真实边（孔洞环边经
    /// [`Dcel::face_hole_boundaries`] 另行取）。
    pub fn face_boundary(&self, face: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let Some(start) = self.faces.get(face).and_then(|f| f.boundary) else {
            return out;
        };
        let mut e = start;
        loop {
            out.push(e);
            e = self.half_edges[e].next;
            if e == start || out.len() > self.half_edges.len() {
                break;
            }
        }
        out
    }

    /// 面的孔洞边界（每条内环一组真实环边，next 链序）。
    pub fn face_hole_boundaries(&self, face: usize) -> Vec<Vec<usize>> {
        self.faces
            .get(face)
            .map(|f| f.holes.iter().map(|&h| self.face_boundary(h)).collect())
            .unwrap_or_default()
    }

    /// 绕点一圈（出边，CCW 角度序——以 (1,0) 方向为 0 弧度逆时针）。
    pub fn outgoing_edges_ccw(&self, vertex: usize) -> Vec<usize> {
        let mut out = self.vertices[vertex].outgoing.clone();
        let origin = self.vertices[vertex].coord;
        let angle = |e: usize| {
            let c = self.vertices[self.half_edges[self.half_edges[e].twin].origin].coord;
            (c[1] - origin[1]).atan2(c[0] - origin[0])
        };
        out.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
        out
    }

    /// 相邻面查询（外边界各边经 twin 的左侧面，去重；含外面/孔虚面）。
    pub fn adjacent_faces(&self, face: usize) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for e in self.face_boundary(face) {
            let f = self.half_edges[self.half_edges[e].twin].left_face;
            if f != face && !out.contains(&f) {
                out.push(f);
            }
        }
        // 多边形面的孔洞虚面亦相邻（孔环边 left 为本面）。
        if let Some(f) = self.faces.get(face) {
            for &h in &f.holes {
                if !out.contains(&h) {
                    out.push(h);
                }
            }
        }
        out
    }

    /// 欧拉示性数 V−E+F（E/F 仅计未删除项；F 含外面与孔虚面）。
    pub fn euler_characteristic(&self) -> i64 {
        let e = self.half_edges.iter().filter(|h| !h.deleted).count() / 2;
        let f = self.faces.iter().filter(|f| !f.deleted).count();
        self.vertices.len() as i64 - e as i64 + f as i64
    }

    /// 连通分量数（并查集，按未删除无向边合并顶点）。
    pub fn components(&self) -> usize {
        let mut parent: Vec<usize> = (0..self.vertices.len()).collect();
        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }
        for he in &self.half_edges {
            if he.deleted {
                continue;
            }
            let a = he.origin;
            let b = self.half_edges[he.twin].origin;
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if ra != rb {
                parent[ra] = rb;
            }
        }
        (0..self.vertices.len())
            .filter(|&v| find(&mut parent, v) == v)
            .count()
    }

    /// 结构自检（twin 对合、next/prev 互逆、left_face 合法、面边界环闭合
    /// 且环边 left_face 回指本面）；任一违例报中文错误。墓碑项跳过。
    pub fn check_invariants(&self) -> Result<(), KanyuError> {
        for (i, he) in self.half_edges.iter().enumerate() {
            if he.deleted {
                continue;
            }
            let tw = &self.half_edges[he.twin];
            if tw.twin != i {
                return Err(err(format!("半边 {i} 的 twin 不对合")));
            }
            if self.half_edges[he.next].prev != i || self.half_edges[he.prev].next != i {
                return Err(err(format!("半边 {i} 的 next/prev 不互逆")));
            }
            if he.left_face >= self.faces.len() {
                return Err(err(format!("半边 {i} 的 left_face 越界")));
            }
            if he.stub && (he.next != i || he.prev != i) {
                return Err(err(format!("半边 {i} 为 stub 但已接线")));
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            if f.kind == FaceKind::Outer || f.deleted {
                continue;
            }
            for e in self.face_boundary(fi) {
                // Hole 面的 boundary 环边 left_face 为其属主多边形面（模块头约定）。
                let expect = match f.kind {
                    FaceKind::Hole => f.polygon.expect("Hole 面必有属主"),
                    _ => fi,
                };
                if self.half_edges[e].left_face != expect {
                    return Err(err(format!(
                        "面 {fi} 边界半边 {e} 的 left_face 应为 {expect}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// 面对角线分裂（v1 增量操作）：同一 Polygon 面外边界上两顶点 v_a/v_b
    /// 连边，面一分为二。拓扑链更新：新建对偶半边 ab/ba 接入两环的
    /// next/prev，第二环全体边 left_face 改指新面；孔洞虚面按射线法重归属。
    /// 约束（中文错误）：面须为 Polygon、外边界 ≥4 条边、v_a≠v_b 且均在外
    /// 边界上、不得已为相邻顶点（对角线即现存边）。
    pub fn split_face_by_diagonal(
        &mut self,
        face: usize,
        v_a: usize,
        v_b: usize,
    ) -> Result<SplitResult, KanyuError> {
        if self.faces.get(face).map(|f| f.kind) != Some(FaceKind::Polygon) {
            return Err(err(format!("面 {face} 非多边形面，不能对角线分裂")));
        }
        if v_a == v_b {
            return Err(err("对角线两端顶点相同"));
        }
        let cycle = self.face_boundary(face);
        if cycle.len() < 4 {
            return Err(err(format!("面 {face} 外边界不足 4 条边，无对角线可分")));
        }
        let ea = cycle
            .iter()
            .find(|&&e| self.half_edges[e].origin == v_a)
            .copied()
            .ok_or_else(|| err(format!("顶点 {v_a} 不在面 {face} 外边界上")))?;
        let eb = cycle
            .iter()
            .find(|&&e| self.half_edges[e].origin == v_b)
            .copied()
            .ok_or_else(|| err(format!("顶点 {v_b} 不在面 {face} 外边界上")))?;
        if self.half_edges[ea].next == eb || self.half_edges[eb].next == ea {
            return Err(err("两顶点已相邻（对角线即现存边）"));
        }
        // 环链：cycle1 = v_a →…→ v_b（ea..b_in），cycle2 = v_b →…→ v_a（eb..a_in）。
        let b_in = self.half_edges[eb].prev;
        let a_in = self.half_edges[ea].prev;
        // 新建对偶半边。
        let ab = self.half_edges.len();
        let ba = ab + 1;
        self.half_edges.push(HalfEdgeRec {
            origin: v_a,
            twin: ba,
            next: eb,
            prev: a_in,
            left_face: usize::MAX, // 随即改指新面
            stub: false,
            deleted: false,
        });
        self.half_edges.push(HalfEdgeRec {
            origin: v_b,
            twin: ab,
            next: ea,
            prev: b_in,
            left_face: face,
            stub: false,
            deleted: false,
        });
        // 接线：cycle1 + ba 留原面；cycle2 + ab 成新面。
        self.half_edges[a_in].next = ab;
        self.half_edges[eb].prev = ab;
        self.half_edges[b_in].next = ba;
        self.half_edges[ea].prev = ba;
        // 原面边界指针改指 cycle1 内边（cycle1 + ba 留原面）。
        self.faces[face].boundary = Some(ea);
        let new_face = self.faces.len();
        self.faces.push(FaceRec {
            boundary: Some(ab),
            kind: FaceKind::Polygon,
            holes: Vec::new(),
            polygon: None,
            deleted: false,
        });
        self.half_edges[ab].left_face = new_face;
        // cycle2 全体边改指新面。
        let mut e = eb;
        loop {
            self.half_edges[e].left_face = new_face;
            e = self.half_edges[e].next;
            if e == ab {
                break;
            }
        }
        self.vertices[v_a].outgoing.push(ab);
        self.vertices[v_b].outgoing.push(ba);
        // 孔洞重归属：射线法判定归属新面/原面，并改写孔环边 left_face。
        let ring1 = self.ring_coords(face);
        let ring2 = self.ring_coords(new_face);
        let holes = std::mem::take(&mut self.faces[face].holes);
        for h in holes {
            let h_edge = self.faces[h].boundary.expect("Hole 面必有边界边");
            let pt = self.vertices[self.half_edges[h_edge].origin].coord;
            let owner = if point_in_ring(pt, &ring2) {
                new_face
            } else if point_in_ring(pt, &ring1) {
                face
            } else {
                return Err(err(format!("孔洞虚面 {h} 分裂后无所归属")));
            };
            self.faces[h].polygon = Some(owner);
            for e in self.face_boundary(h) {
                self.half_edges[e].left_face = owner;
            }
            self.faces[owner].holes.push(h);
        }
        Ok(SplitResult {
            new_face,
            edge_ab: ab,
            edge_ba: ba,
        })
    }

    /// 面外边界环坐标（origin 序，不闭合——供分裂重归属）。
    fn ring_coords(&self, face: usize) -> Vec<[f64; 2]> {
        self.face_boundary(face)
            .iter()
            .map(|&e| self.vertices[self.half_edges[e].origin].coord)
            .collect()
    }

    // ===== v2：外面/虚面遍历与 merge =====

    /// 沿 stub 边走一圈（外面/孔虚面边界遍历）。
    ///
    /// 算法（角度序转向规则）：stub 的 next=prev=自身（v1 约定未接线），
    /// 绕行靠顶点出边角度序——到达当前边 e 的终点 v 后，取 twin(e)（v 的
    /// 出边之一）在 v 的 CCW 出边序中的**上一条**（即顺时针方向的续边）
    /// 作为续边。该规则等价于「保持左侧面不变的几何面行走」（对真实环边
    /// 它与环接线 next 一致，故对 stub 环同样成立）。`start` 须为 stub 边。
    pub fn stub_cycle(&self, start: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut e = start;
        loop {
            out.push(e);
            let twin = self.half_edges[e].twin;
            let v = self.half_edges[twin].origin; // e 的终点
            let outs = self.outgoing_edges_ccw(v);
            let pos = outs
                .iter()
                .position(|&x| x == twin)
                .expect("twin 必为终点出边");
            e = outs[(pos + outs.len() - 1) % outs.len()];
            if e == start || out.len() > self.half_edges.len() {
                break;
            }
        }
        out
    }

    /// 全部连通分量的外边界（每条为 stub 环，left_face 恒为外面）。
    pub fn outer_boundaries(&self) -> Vec<Vec<usize>> {
        let mut visited = vec![false; self.half_edges.len()];
        let mut out = Vec::new();
        for i in 0..self.half_edges.len() {
            let he = &self.half_edges[i];
            if !he.stub || he.deleted || he.left_face != OUTER_FACE || visited[i] {
                continue;
            }
            let cycle = self.stub_cycle(i);
            for &e in &cycle {
                visited[e] = true;
            }
            out.push(cycle);
        }
        out
    }

    /// 孔虚面内侧边界（left_face 为该孔面的 stub 环）。
    pub fn hole_interior_boundary(&self, hole_face: usize) -> Vec<usize> {
        let start = self
            .half_edges
            .iter()
            .position(|h| h.stub && !h.deleted && h.left_face == hole_face);
        start.map(|s| self.stub_cycle(s)).unwrap_or_default()
    }

    /// 合并两面（[`Dcel::split_face_by_diagonal`] 的逆操作，v2）：删除共享
    /// 半边对 `edge`/twin，被吸收面（`edge` 左侧面）并入保留面（twin 左侧面）。
    /// 半边与面均**墓碑化**（`deleted=true`，下标保持稳定），欧拉示性数
    /// 随之恢复（E−1、F−1，χ 不变）。被吸收面的孔洞虚面改属保留面
    /// （含孔环边 left_face 归属校验改写）。
    ///
    /// 约束（全中文错误）：边须存在且未删除、非 stub；两侧面均须为
    /// Polygon 且不同；两面共享边**仅允许此一条**（多条共享边的合并
    /// 会改变孔的连通性，超出 v1 分裂语义，拒绝）。
    pub fn merge_faces(&mut self, edge: usize) -> Result<MergeResult, KanyuError> {
        let he = self
            .half_edges
            .get(edge)
            .ok_or_else(|| err(format!("半边 {edge} 不存在")))?;
        if he.deleted {
            return Err(err(format!("半边 {edge} 已删除")));
        }
        if he.stub {
            return Err(err(format!("半边 {edge} 为占位边（stub），不能合并")));
        }
        let twin = he.twin;
        let f_absorbed = he.left_face;
        let f_keep = self.half_edges[twin].left_face;
        if f_absorbed == f_keep {
            return Err(err(format!("半边 {edge} 两侧同面（{f_keep}），不能合并")));
        }
        for (f, tag) in [(f_absorbed, "被吸收面"), (f_keep, "保留面")] {
            if self.faces[f].deleted || self.faces[f].kind != FaceKind::Polygon {
                return Err(err(format!("{tag} {f} 非多边形面，不能合并")));
            }
        }
        // 共享边计数：被吸收面边界上 twin 左侧为保留面的边须仅此一条。
        let shared: Vec<usize> = self
            .face_boundary(f_absorbed)
            .into_iter()
            .filter(|&e| {
                let t = &self.half_edges[self.half_edges[e].twin];
                !t.deleted && t.left_face == f_keep
            })
            .collect();
        if shared != [edge] {
            return Err(err(format!(
                "两面共享边不止一条（{} 条），超出 v2 单边合并语义",
                shared.len()
            )));
        }
        // 重接：e（v_a→v_b，被吸收面）与 twin（v_b→v_a，保留面）摘除，
        // 两环合为保留面单环。
        let e_next = self.half_edges[edge].next;
        let e_prev = self.half_edges[edge].prev;
        let t_next = self.half_edges[twin].next;
        let t_prev = self.half_edges[twin].prev;
        self.half_edges[t_prev].next = e_next;
        self.half_edges[e_next].prev = t_prev;
        self.half_edges[e_prev].next = t_next;
        self.half_edges[t_next].prev = e_prev;
        // 被吸收面环全体边 left_face 归并（e/twin 除外——随即墓碑化）。
        let mut cur = e_next;
        loop {
            self.half_edges[cur].left_face = f_keep;
            cur = self.half_edges[cur].next;
            if cur == t_next {
                break;
            }
        }
        self.faces[f_keep].boundary = Some(t_next);
        // 孔洞虚面改属（校验孔环边 left_face 当前确为被吸收面后改写）。
        let holes = std::mem::take(&mut self.faces[f_absorbed].holes);
        for h in holes {
            for he_id in self.face_boundary(h) {
                if self.half_edges[he_id].left_face != f_absorbed {
                    return Err(err(format!(
                        "孔洞虚面 {h} 环边 {he_id} 归属异常（非被吸收面）"
                    )));
                }
                self.half_edges[he_id].left_face = f_keep;
            }
            self.faces[h].polygon = Some(f_keep);
            self.faces[f_keep].holes.push(h);
        }
        // 墓碑化：半边对出顶点出边清单，面清空边界。
        self.vertices[self.half_edges[edge].origin]
            .outgoing
            .retain(|&x| x != edge);
        self.vertices[self.half_edges[twin].origin]
            .outgoing
            .retain(|&x| x != twin);
        self.half_edges[edge].deleted = true;
        self.half_edges[twin].deleted = true;
        self.faces[f_absorbed].deleted = true;
        self.faces[f_absorbed].boundary = None;
        Ok(MergeResult {
            survivor: f_keep,
            absorbed: f_absorbed,
        })
    }
}

/// 环归一化：去闭合重复点、退化边检查、绕向归一（外环 CCW/内环 CW）。
fn normalize_ring(ring: Vec<Vec<f64>>, ccw: bool) -> Result<Vec<[f64; 2]>, KanyuError> {
    let mut pts: Vec<[f64; 2]> = ring.iter().map(|p| [p[0], p[1]]).collect();
    // 去闭合重复点。
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() < 3 {
        return Err(err(format!("环不足 3 个相异点: {}", pts.len())));
    }
    if pts.windows(2).any(|w| w[0] == w[1]) {
        return Err(err("环含退化边（相邻重复点）"));
    }
    let area = signed_area(&pts);
    if (ccw && area < 0.0) || (!ccw && area > 0.0) {
        pts.reverse();
    }
    Ok(pts)
}

/// 环接线：逐段建真实半边（left_face=左侧面），对侧按需建 stub 占位边；
/// 共享边复用既有 stub（改指本面并接线）。返回首条边 id。
fn wire_ring(
    dcel: &mut Dcel,
    emap: &mut HashMap<(usize, usize), usize>,
    vertex_of: &mut impl FnMut(&mut Dcel, [f64; 2]) -> usize,
    ring: &[[f64; 2]],
    left: usize,
    other: usize,
) -> Result<usize, KanyuError> {
    let ids: Vec<usize> = ring.iter().map(|&c| vertex_of(dcel, c)).collect();
    let m = ids.len();
    let mut edges = Vec::with_capacity(m);
    for i in 0..m {
        let (a, b) = (ids[i], ids[(i + 1) % m]);
        if a == b {
            return Err(err("环含退化边（相邻重复顶点）"));
        }
        let he = match emap.get(&(a, b)) {
            Some(&existing) => {
                if !dcel.half_edges[existing].stub {
                    return Err(err(format!("重合边（顶点 {a} → {b} 同向重复）")));
                }
                // 共享边：复用占位 stub，改指本面。
                dcel.half_edges[existing].left_face = left;
                dcel.half_edges[existing].stub = false;
                existing
            }
            None => {
                // 新边 + 对侧 stub 占位（对侧面见模块头约定）。
                let e = dcel.half_edges.len();
                let s = e + 1;
                dcel.half_edges.push(HalfEdgeRec {
                    origin: a,
                    twin: s,
                    next: usize::MAX,
                    prev: usize::MAX,
                    left_face: left,
                    stub: false,
                    deleted: false,
                });
                dcel.half_edges.push(HalfEdgeRec {
                    origin: b,
                    twin: e,
                    next: s,
                    prev: s,
                    left_face: other,
                    stub: true,
                    deleted: false,
                });
                emap.insert((a, b), e);
                emap.insert((b, a), s);
                e
            }
        };
        edges.push(he);
    }
    for i in 0..m {
        let (cur, nxt) = (edges[i], edges[(i + 1) % m]);
        dcel.half_edges[cur].next = nxt;
        dcel.half_edges[nxt].prev = cur;
    }
    Ok(edges[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造面要素集合（rings 为 geojson Polygon 坐标结构）。
    fn collection_of(polys: Vec<Vec<Vec<Vec<f64>>>>) -> FeatureCollection {
        let features = polys
            .into_iter()
            .map(|rings| geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(GeoValue::Polygon(rings))),
                id: None,
                properties: None,
                foreign_members: None,
            })
            .collect();
        FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }
    }

    fn ring(pts: &[[f64; 2]]) -> Vec<Vec<f64>> {
        let mut v: Vec<Vec<f64>> = pts.iter().map(|p| vec![p[0], p[1]]).collect();
        v.push(vec![pts[0][0], pts[0][1]]);
        v
    }

    const SQ: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    #[test]
    fn build_square_counts_and_traversal() {
        let d = Dcel::build(&collection_of(vec![vec![ring(&SQ)]])).unwrap();
        // V=4，E=4（8 半边），F=2（多边形 + 外面）。
        assert_eq!(d.vertices.len(), 4);
        assert_eq!(d.half_edges.len(), 8);
        assert_eq!(d.faces.len(), 2);
        assert_eq!(d.euler_characteristic(), 2);
        assert_eq!(d.components(), 1);
        // 平面公式 V−E+F = 1+C。
        assert_eq!(d.euler_characteristic(), 1 + d.components() as i64);
        d.check_invariants().unwrap();
        // 绕面一圈 = 4 条真实边；绕点一圈 = 2 出边（真实 + stub）。
        assert_eq!(d.face_boundary(1).len(), 4);
        assert_eq!(d.outgoing_edges_ccw(0).len(), 2);
        // 相邻面：多边形仅邻外面。
        assert_eq!(d.adjacent_faces(1), vec![OUTER_FACE]);
    }

    #[test]
    fn build_holed_square_hole_face_and_euler() {
        let hole: [[f64; 2]; 4] = [[0.2, 0.2], [0.4, 0.2], [0.4, 0.4], [0.2, 0.4]];
        // 外环故意给 CW（构建应归一为 CCW），内环给 CCW（归一为 CW）。
        let mut ext = SQ;
        ext.reverse();
        let d = Dcel::build(&collection_of(vec![vec![ring(&ext), ring(&hole)]])).unwrap();
        // V=8，E=8，F=3（多边形 + 孔虚面 + 外面）。
        assert_eq!(d.vertices.len(), 8);
        assert_eq!(d.half_edges.len() / 2, 8);
        assert_eq!(d.faces.len(), 3);
        // 孔环与外环无边相连 → 2 个连通分量；V−E+F = 8−8+3 = 3 = 1+2。
        assert_eq!(d.components(), 2);
        assert_eq!(d.euler_characteristic(), 3);
        d.check_invariants().unwrap();
        // 孔虚面：属主为多边形面 1；多边形面 holes 含孔面 2。
        assert_eq!(d.faces[2].kind, FaceKind::Hole);
        assert_eq!(d.faces[2].polygon, Some(1));
        assert_eq!(d.faces[1].holes, vec![2]);
        // 孔环边界遍历（经孔虚面 boundary）：4 条真实边且 left_face 为多边形面。
        let hb = d.face_hole_boundaries(1);
        assert_eq!(hb.len(), 1);
        assert_eq!(hb[0].len(), 4);
        for &e in &hb[0] {
            assert_eq!(d.half_edges[e].left_face, 1);
        }
        // 相邻面：多边形邻外面 + 孔虚面。
        let adj = d.adjacent_faces(1);
        assert!(adj.contains(&OUTER_FACE) && adj.contains(&2), "{adj:?}");
    }

    #[test]
    fn build_two_adjacent_squares_share_edge() {
        let sq2: [[f64; 2]; 4] = [[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]];
        let d = Dcel::build(&collection_of(vec![vec![ring(&SQ)], vec![ring(&sq2)]])).unwrap();
        // V=6，E=7（共享 1 边），F=3（两面 + 外面）。
        assert_eq!(d.vertices.len(), 6, "共享边顶点键控合并");
        assert_eq!(d.half_edges.len() / 2, 7);
        assert_eq!(d.faces.len(), 3);
        assert_eq!(d.euler_characteristic(), 2);
        assert_eq!(d.components(), 1);
        d.check_invariants().unwrap();
        // 相邻面互查（经 twin）。
        assert!(d.adjacent_faces(1).contains(&2));
        assert!(d.adjacent_faces(2).contains(&1));
        // 共享边：两半边的 left_face 分别为两面。
        let shared = d
            .half_edges
            .iter()
            .position(|he| {
                !he.stub && {
                    let tw = &d.half_edges[he.twin];
                    !tw.stub && he.left_face != tw.left_face && tw.left_face != OUTER_FACE
                }
            })
            .expect("应存在共享边");
        let faces = [
            d.half_edges[shared].left_face,
            d.half_edges[d.half_edges[shared].twin].left_face,
        ];
        assert!(faces.contains(&1) && faces.contains(&2), "{faces:?}");
        // 共享边端点出边 3 条（两真实 + 一 stub）。
        let v = d.half_edges[shared].origin;
        assert_eq!(d.outgoing_edges_ccw(v).len(), 3);
    }

    #[test]
    fn split_square_by_diagonal_preserves_euler() {
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ)]])).unwrap();
        let (euler_before, v_before, e_before) = (
            d.euler_characteristic(),
            d.vertices.len(),
            d.half_edges.len() / 2,
        );
        // 顶点 0(0,0) 与 2(1,1) 连对角线。
        let res = d.split_face_by_diagonal(1, 0, 2).unwrap();
        assert_eq!(d.vertices.len(), v_before, "分裂不增顶点");
        assert_eq!(d.half_edges.len() / 2, e_before + 1, "分裂增一条边");
        assert_eq!(d.faces.len(), 3, "分裂增一个面");
        assert_eq!(d.euler_characteristic(), euler_before, "欧拉示性数保持");
        d.check_invariants().unwrap();
        // 两个三角形：绕面各 3 条边。
        assert_eq!(d.face_boundary(1).len(), 3);
        assert_eq!(d.face_boundary(res.new_face).len(), 3);
        // 对角线两半边的 left_face 分指两面且互邻。
        assert_eq!(d.half_edges[res.edge_ab].left_face, res.new_face);
        assert_eq!(d.half_edges[res.edge_ba].left_face, 1);
        assert!(d.adjacent_faces(1).contains(&res.new_face));
        assert!(d.adjacent_faces(res.new_face).contains(&1));
        // undo 友好性：整表克隆即快照（v1 约定）。
        let snapshot = d.clone();
        assert_eq!(snapshot.faces.len(), d.faces.len());
    }

    #[test]
    fn split_holed_square_reassigns_hole() {
        // 孔洞位于对角线 (0,0)-(1,1) 的左上半区。
        let hole: [[f64; 2]; 4] = [[0.1, 0.6], [0.3, 0.6], [0.3, 0.8], [0.1, 0.8]];
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ), ring(&hole)]])).unwrap();
        let res = d.split_face_by_diagonal(1, 0, 2).unwrap();
        d.check_invariants().unwrap();
        assert_eq!(d.euler_characteristic(), 3, "V−E+F 保持（孔面仍在）");
        // 孔环边 left_face 与其属主一致：恰归属一个面。
        let owner = d.faces[2].polygon.unwrap();
        assert!(owner == 1 || owner == res.new_face);
        for &e in &d.face_boundary(2) {
            assert_eq!(d.half_edges[e].left_face, owner, "孔环边须随属主改指");
        }
        // 属主面的 holes 含孔面，另一面不含。
        assert!(d.faces[owner].holes.contains(&2));
        let other = if owner == 1 { res.new_face } else { 1 };
        assert!(!d.faces[other].holes.contains(&2));
    }

    #[test]
    fn split_rejects_bad_diagonals() {
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ)]])).unwrap();
        // 相邻顶点（0→1 已是边）。
        let e = d.split_face_by_diagonal(1, 0, 1).unwrap_err();
        assert!(e.to_string().contains("相邻"), "{e}");
        // 顶点相同。
        assert!(d.split_face_by_diagonal(1, 0, 0).is_err());
        // 顶点不在面上。
        let e = d.split_face_by_diagonal(1, 0, 99).unwrap_err();
        assert!(e.to_string().contains("不在面"), "{e}");
        // 三角形无对角线。
        let tri: [[f64; 2]; 3] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let mut d2 = Dcel::build(&collection_of(vec![vec![ring(&tri)]])).unwrap();
        let e = d2.split_face_by_diagonal(1, 0, 2).unwrap_err();
        assert!(e.to_string().contains("不足 4 条边"), "{e}");
        // 非多边形面（外面）。
        let e = d.split_face_by_diagonal(OUTER_FACE, 0, 2).unwrap_err();
        assert!(e.to_string().contains("非多边形面"), "{e}");
    }
}

#[cfg(test)]
mod tests_v2 {
    use super::*;

    fn collection_of(polys: Vec<Vec<Vec<Vec<f64>>>>) -> FeatureCollection {
        let features = polys
            .into_iter()
            .map(|rings| geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(GeoValue::Polygon(rings))),
                id: None,
                properties: None,
                foreign_members: None,
            })
            .collect();
        FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }
    }

    fn ring(pts: &[[f64; 2]]) -> Vec<Vec<f64>> {
        let mut v: Vec<Vec<f64>> = pts.iter().map(|p| vec![p[0], p[1]]).collect();
        v.push(vec![pts[0][0], pts[0][1]]);
        v
    }

    const SQ: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    #[test]
    fn outer_boundary_of_two_adjacent_squares_is_hexagon() {
        let sq2: [[f64; 2]; 4] = [[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]];
        let d = Dcel::build(&collection_of(vec![vec![ring(&SQ)], vec![ring(&sq2)]])).unwrap();
        let outers = d.outer_boundaries();
        assert_eq!(outers.len(), 1, "连通分量外边界应唯一");
        assert_eq!(outers[0].len(), 6, "2×1 矩形外边界为 6 边环");
        for &e in &outers[0] {
            assert!(d.half_edges[e].stub && d.half_edges[e].left_face == OUTER_FACE);
        }
        // 绕环闭合性：逐边经 stub_cycle 规则应回到起点（长度一致即证）。
        let again = d.stub_cycle(outers[0][0]);
        assert_eq!(again, outers[0], "重走应得同一环");
    }

    #[test]
    fn outer_and_hole_interior_cycles_of_holed_square() {
        let hole: [[f64; 2]; 4] = [[0.2, 0.2], [0.4, 0.2], [0.4, 0.4], [0.2, 0.4]];
        let d = Dcel::build(&collection_of(vec![vec![ring(&SQ), ring(&hole)]])).unwrap();
        // 外边界 4 边环（左侧面恒为外面）。
        let outers = d.outer_boundaries();
        assert_eq!(outers.len(), 1);
        assert_eq!(outers[0].len(), 4);
        // 孔虚面内侧 stub 环 4 边。
        let inner = d.hole_interior_boundary(2);
        assert_eq!(inner.len(), 4);
        for &e in &inner {
            assert!(d.half_edges[e].stub && d.half_edges[e].left_face == 2);
        }
    }

    #[test]
    fn split_then_merge_restores_structure() {
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ)]])).unwrap();
        let res = d.split_face_by_diagonal(1, 0, 2).unwrap();
        assert_eq!(d.faces.len(), 3);
        let m = d.merge_faces(res.edge_ab).unwrap();
        assert_eq!(m.survivor, 1, "twin 左侧原面保留");
        assert_eq!(m.absorbed, res.new_face);
        // 拓扑量复原（墓碑不计数）。
        assert_eq!(d.euler_characteristic(), 2);
        assert_eq!(d.components(), 1);
        assert_eq!(d.face_boundary(1).len(), 4, "合并后回到正方环");
        assert!(d.faces[res.new_face].deleted);
        assert!(d.half_edges[res.edge_ab].deleted && d.half_edges[res.edge_ba].deleted);
        d.check_invariants().unwrap();
        // 合并后可再分裂（墓碑不影响新操作）。
        let res2 = d.split_face_by_diagonal(1, 0, 2).unwrap();
        d.check_invariants().unwrap();
        assert_eq!(d.face_boundary(res2.new_face).len(), 3);
    }

    #[test]
    fn merge_reassigns_holes_back() {
        let hole: [[f64; 2]; 4] = [[0.1, 0.6], [0.3, 0.6], [0.3, 0.8], [0.1, 0.8]];
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ), ring(&hole)]])).unwrap();
        let res = d.split_face_by_diagonal(1, 0, 2).unwrap();
        let m = d.merge_faces(res.edge_ab).unwrap();
        d.check_invariants().unwrap();
        assert_eq!(d.euler_characteristic(), 3, "孔面仍在（V−E+F 恢复）");
        // 孔虚面回到保留面。
        assert_eq!(d.faces[2].polygon, Some(1));
        assert_eq!(d.faces[1].holes, vec![2]);
        for &e in &d.face_boundary(2) {
            assert_eq!(d.half_edges[e].left_face, m.survivor);
        }
    }

    #[test]
    fn merge_error_branches() {
        let mut d = Dcel::build(&collection_of(vec![vec![ring(&SQ)]])).unwrap();
        // 边不存在。
        assert!(d
            .merge_faces(999)
            .unwrap_err()
            .to_string()
            .contains("不存在"));
        // stub 边（外面侧占位边）。
        let stub = d.half_edges.iter().position(|h| h.stub).expect("必有 stub");
        assert!(d
            .merge_faces(stub)
            .unwrap_err()
            .to_string()
            .contains("占位边"));
        // 已删除边。
        let res = d.split_face_by_diagonal(1, 0, 2).unwrap();
        d.merge_faces(res.edge_ab).unwrap();
        let e = d.merge_faces(res.edge_ab).unwrap_err();
        assert!(e.to_string().contains("已删除"), "{e}");
        // 多共享边拒绝：带孔方环 + 孔内方（共享 4 边）。
        let hole: [[f64; 2]; 4] = [[0.2, 0.2], [0.4, 0.2], [0.4, 0.4], [0.2, 0.4]];
        let inner: [[f64; 2]; 4] = [[0.2, 0.2], [0.4, 0.2], [0.4, 0.4], [0.2, 0.4]];
        let mut d2 = Dcel::build(&collection_of(vec![
            vec![ring(&SQ), ring(&hole)],
            vec![ring(&inner)],
        ]))
        .unwrap();
        // 找内方的一条外边界边（其 twin 左侧为带孔方环面 1）。
        let shared_edge = (0..d2.half_edges.len())
            .find(|&i| {
                let h = &d2.half_edges[i];
                !h.stub && h.left_face == 3 && {
                    let t = &d2.half_edges[h.twin];
                    !t.stub && t.left_face == 1
                }
            })
            .expect("应有共享边");
        let e = d2.merge_faces(shared_edge).unwrap_err();
        assert!(e.to_string().contains("不止一条"), "{e}");
    }
}
