//! wgpu 真管线 3D（Phase 2）：egui PaintCallback 自绘棱柱/线/点网格。
//!
//! ## 管线设计
//!
//! - **离屏渲染 + blit 合成**：egui 主渲染通道只有 color 附件、无深度附件，
//!   真深度缓冲必须离屏——prepare 阶段把场景渲进离屏 color（Rgba8UnormSrgb，
//!   与 egui sRGB 目标恒等往返）+ depth（Depth32Float）纹理，paint 阶段以
//!   全屏三角形把离屏图 blit 进 egui 通道（viewport/scissor 对齐画布区）。
//! - **多视口**：离屏纹理与顶点缓冲按视图 id 分键（与 canvas 纹理命名同策略），
//!   两个三维框可同时 wgpu 渲染互不串扰。
//! - **顶点缓冲缓存**：网格按（视图 id, 内容纪元 render_epoch）缓存——数据/符号化
//!   不变不重建（休眠框内容冻结，纪元失配才重建一次；语义见 scene3d.rs 调用注释）。
//!   网格在**数据范围归一化空间**构建（与视口解耦），平移缩放只动 MVP。
//! - **要素覆盖**：面棱柱（耳切三角化含洞桥接 + 侧壁）、线（世界空间窄带）、
//!   点（十字交叉竖直 quad 标记——任意方位角可读，免公告板逐帧更新）。
//! - **回退**：wgpu 不可用时 [`init`] 不执行、壳层保持软件路径（终端提示一次）。
//!
//! 纯函数（[`build_prism_mesh`]/[`build_linework_mesh`]/[`earcut`]/[`orbit_mvp`]/
//! [`orbit_mvp_at`]）配单测。

use std::sync::{Arc, Mutex};

use eframe::egui;
use egui_wgpu::wgpu::{self, util::DeviceExt};

/// WGSL 着色器（Lambert 光照 + 全屏 blit 两入口）。
const SHADER: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Vin {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) col: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) col: vec4<f32>,
};
@vertex
fn vs_prism(v: Vin) -> Vout {
    var o: Vout;
    o.pos = u.mvp * vec4<f32>(v.pos, 1.0);
    o.nrm = v.nrm;
    o.col = v.col;
    return o;
}
@fragment
fn fs_prism(v: Vout) -> @location(0) vec4<f32> {
    let l = normalize(u.light.xyz);
    // 两侧同光（abs）：线/点与侧壁不区分内外朝向，保证全部可读。
    let d = abs(dot(normalize(v.nrm), l));
    let shade = 0.35 + 0.65 * d;
    return vec4<f32>(v.col.rgb * shade, v.col.a);
}

struct BlitOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    // 全屏三角形：(-1,-1) (3,-1) (-1,3)。
    var o: BlitOut;
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return o;
}
@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var s_scene: sampler;
@fragment
fn fs_blit(v: BlitOut) -> @location(0) vec4<f32> {
    return textureSample(t_scene, s_scene, v.uv);
}
"#;

/// 棱柱顶点（位置/法线/颜色；bytemuck 直通顶点缓冲）。
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PrismVertex {
    /// 世界坐标（x 东、y 高、z 南——数据 y 翻转保持北向上）。
    pub pos: [f32; 3],
    /// 面法线。
    pub nrm: [f32; 3],
    /// 颜色（直线 sRGB 字节换算 0..1）。
    pub col: [f32; 4],
}

/// 棱柱部件（数据坐标环组 + 高度 + 颜色）。
pub struct PrismPart {
    /// 外环（不含闭合重复点）。
    pub outer: Vec<[f64; 2]>,
    /// 洞内环（不含闭合重复点；桥接进耳切）。
    pub holes: Vec<Vec<[f64; 2]>>,
    /// 高度（数据单位）。
    pub height: f32,
    /// 颜色 RGBA 0..1。
    pub color: [f32; 4],
}

/// 线部件（折线数据坐标 + 颜色）。
pub type LinePart = (Vec<[f64; 2]>, [f32; 4]);
/// 点部件（数据坐标 + 颜色）。
pub type PointPart = ([f64; 2], [f32; 4]);

// ===== 耳切三角化（纯函数）=====

/// 有向面积二倍（鞋带公式；CCW 为正）。
fn signed_area2(ring: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        a += p[0] * q[1] - q[0] * p[1];
    }
    a
}

/// 点在线段左侧（b×c 叉积符号判定）。
fn is_convex(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) > 1e-12
}

/// 点在三角形内（含边界；边界在內可挡下「对角线穿凹点」的退化耳）。
/// 耳切调用方按下标身份跳过三角自身顶点（桥接重复顶点同 id——见 earcut）。
fn point_in_tri(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let cross = |u: [f64; 2], v: [f64; 2], w: [f64; 2]| {
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    let has_neg = d1 < -1e-12 || d2 < -1e-12 || d3 < -1e-12;
    let has_pos = d1 > 1e-12 || d2 > 1e-12 || d3 > 1e-12;
    !(has_neg && has_pos)
}

/// 线段相交（严格相交，不含共端点）。
fn seg_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let d1 = {
        let (u, v, w) = (c, d, a);
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    let d2 = {
        let (u, v, w) = (c, d, b);
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    let d3 = {
        let (u, v, w) = (a, b, c);
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    let d4 = {
        let (u, v, w) = (a, b, d);
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    ((d1 > 1e-12) != (d2 > 1e-12)) && ((d3 > 1e-12) != (d4 > 1e-12))
}

/// 耳切三角化（含洞桥接；纯函数）。
///
/// 输入外环与洞内环（均不含闭合重复点；绕向任意——内部归一化为外 CCW、洞 CW）。
/// 返回相对「外环 + 各洞顺序拼接」统一点表的三角形下标三元组。
/// 桥接策略：洞的最右顶点向 +x 投射线找可见外顶点（线段不穿任何边）后缝合；
/// 病态自交输入可能丢洞（注释即取舍：GIS 清洗后数据罕见）。
pub fn earcut(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> Vec<u32> {
    if outer.len() < 3 {
        return Vec::new();
    }
    // 统一点表（外 CCW、洞 CW 归一化）。
    let mut pts: Vec<[f64; 2]> = outer.to_vec();
    if signed_area2(&pts) < 0.0 {
        pts.reverse();
    }
    // 工作多边形（点表下标，CCW）。
    let mut poly: Vec<u32> = (0..pts.len() as u32).collect();
    for hole in holes {
        if hole.len() < 3 {
            continue;
        }
        let mut h: Vec<[f64; 2]> = hole.clone();
        if signed_area2(&h) > 0.0 {
            h.reverse(); // 洞 CW
        }
        let base = pts.len() as u32;
        // 洞最右顶点（x 最大，并列取 y 大者）。
        let (hidx, _) = h
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a[0].partial_cmp(&b[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
            })
            .expect("非空洞");
        let hp = h[hidx];
        // 可见外顶点：v 须在 h 右侧（+x 方向，与经典射线桥接同语义——
        // 左向缝合可能横穿洞内部）；h→v 不穿多边形任何边、不穿洞任何边；取最近者。
        let mut best: Option<(usize, f64)> = None;
        for (vi, &pi) in poly.iter().enumerate() {
            let v = pts[pi as usize];
            if v[0] < hp[0] - 1e-12 {
                continue; // 仅右向候选
            }
            let d2 = (v[0] - hp[0]).powi(2) + (v[1] - hp[1]).powi(2);
            if best.as_ref().is_some_and(|(_, bd)| d2 >= *bd) {
                continue;
            }
            // 穿边检查（共端点边跳过——h→v 恒与 v 的邻边相接）。
            let n = poly.len();
            let crosses_poly = (0..n).any(|i| {
                if poly[i] == poly[vi] || poly[(i + 1) % n] == poly[vi] {
                    return false;
                }
                let (a, b) = (pts[poly[i] as usize], pts[poly[(i + 1) % n] as usize]);
                seg_cross(hp, v, a, b)
            });
            // 洞边检查（h 的邻边跳过）。
            let hn = h.len();
            let crosses_hole = (0..hn).any(|i| {
                if i == hidx || (i + 1) % hn == hidx {
                    return false;
                }
                seg_cross(hp, v, h[i], h[(i + 1) % hn])
            });
            if !crosses_poly && !crosses_hole {
                best = Some((vi, d2));
            }
        }
        let Some((vi, _)) = best else {
            continue; // 病态：无可见顶点，丢洞（见文档注释取舍）
        };
        // 缝合：…v, [h, h+1, …, h-1, h], v…（v 与 h 各重复一次）。
        let hole_idx: Vec<u32> = (0..h.len() as u32)
            .map(|i| base + (hidx as u32 + i) % h.len() as u32)
            .collect();
        pts.extend(h);
        let v_id = poly[vi];
        let mut new_poly: Vec<u32> = Vec::with_capacity(poly.len() + hole_idx.len() + 2);
        new_poly.extend_from_slice(&poly[..=vi]);
        new_poly.extend_from_slice(&hole_idx);
        new_poly.push(base + hidx as u32);
        new_poly.push(v_id);
        new_poly.extend_from_slice(&poly[vi + 1..]);
        poly = new_poly;
    }
    // 耳切主循环（O(n²)；凸性 + 空三角判定）。
    let mut tris = Vec::new();
    let mut guard = 0usize;
    while poly.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = poly.len();
        let mut clipped = false;
        for i in 0..n {
            let (ia, ib, ic) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            let (a, b, c) = (pts[ia as usize], pts[ib as usize], pts[ic as usize]);
            if !is_convex(a, b, c) {
                continue;
            }
            let empty = poly.iter().all(|&pj| {
                // 按下标身份跳过三角自身顶点——桥接会在 v 与 h 处产生同 id
                // 重复点（恒在三角边上），不跳过会堵死所有耳。
                if pj == ia || pj == ib || pj == ic {
                    return true;
                }
                !point_in_tri(pts[pj as usize], a, b, c)
            });
            if empty {
                tris.extend_from_slice(&[ia, ib, ic]);
                poly.remove(i);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break; // 退化（自交/共线）：尽力输出已有三角
        }
    }
    if poly.len() == 3 {
        tris.extend_from_slice(&[poly[0], poly[1], poly[2]]);
    }
    tris
}

// ===== 网格构建（纯函数）=====

/// 几何法线（叉积，未归一化；背剔绕向断言与网格定向共用）。
pub fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let (u, v) = (
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    );
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

/// 按期望法线定向后输出三角（几何叉积与期望法线点积为负则交换绕向——
/// 背剔开启后绕向即正误，顶面/外壁/内壁统一在本函数收口）。
fn push_face(
    out: &mut Vec<PrismVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    nrm: [f32; 3],
    col: [f32; 4],
) {
    let n = face_normal(a, b, c);
    let dot = n[0] * nrm[0] + n[1] * nrm[1] + n[2] * nrm[2];
    let (b, c) = if dot < 0.0 { (c, b) } else { (b, c) };
    for p in [a, b, c] {
        out.push(PrismVertex { pos: p, nrm, col });
    }
}

/// 侧壁 quad（两个三角；`nrm` 为期望朝向——外环朝外、洞环朝洞心）。
#[allow(clippy::too_many_arguments)]
fn push_wall(
    out: &mut Vec<PrismVertex>,
    wa0: [f32; 3],
    wb0: [f32; 3],
    wa1: [f32; 3],
    wb1: [f32; 3],
    nrm: [f32; 3],
    col: [f32; 4],
) {
    push_face(out, wa0, wb0, wb1, nrm, col);
    push_face(out, wa0, wb1, wa1, nrm, col);
}

/// 棱柱网格顶点（非索引三角面片）。
/// 顶面耳切三角化（含洞桥接）；侧壁外环朝外、**洞内环拉伸为内壁（法线朝洞心）**。
/// `center`/`scale`：数据坐标归一化到世界系（数据范围跨度 → 约 2 世界单位，
/// 与视口解耦——平移缩放只动 MVP，见 [`orbit_mvp_at`]）。
pub fn build_prism_mesh(
    parts: &[PrismPart],
    center: (f64, f64),
    scale: f64,
    hscale: f32,
) -> Vec<PrismVertex> {
    let mut out = Vec::new();
    for part in parts {
        let n = part.outer.len();
        if n < 3 {
            continue;
        }
        let top = part.height * hscale;
        let col = part.color;
        let w = |p: [f64; 2], y: f32| {
            [
                ((p[0] - center.0) * scale) as f32,
                y,
                ((center.1 - p[1]) * scale) as f32, // 数据 y 翻转（北向上）
            ]
        };
        // 顶面（耳切；绕向经 push_face 统一为 +y）。点表与 earcut 内部
        // 归一化一致（外 CCW / 洞 CW 后拼接——返回下标相对该序列）。
        let mut pts: Vec<[f64; 2]> = part.outer.clone();
        if signed_area2(&pts) < 0.0 {
            pts.reverse();
        }
        for hole in &part.holes {
            let mut h = hole.clone();
            if signed_area2(&h) > 0.0 {
                h.reverse();
            }
            pts.extend(h);
        }
        for tri in earcut(&part.outer, &part.holes).chunks_exact(3) {
            push_face(
                &mut out,
                w(pts[tri[0] as usize], top),
                w(pts[tri[1] as usize], top),
                w(pts[tri[2] as usize], top),
                [0.0, 1.0, 0.0],
                col,
            );
        }
        // 外环侧壁（外法线）。
        for i in 0..n {
            let (a, b) = (part.outer[i], part.outer[(i + 1) % n]);
            let (wa0, wb0) = (w(a, 0.0), w(b, 0.0));
            let (wa1, wb1) = (w(a, top), w(b, top));
            let (dx, dz) = (wb0[0] - wa0[0], wb0[2] - wa0[2]);
            let len = (dx * dx + dz * dz).sqrt().max(1e-9);
            push_wall(
                &mut out,
                wa0,
                wb0,
                wa1,
                wb1,
                [dz / len, 0.0, -dx / len],
                col,
            );
        }
        // 洞内环内壁（法线朝洞心：边中点→洞心方向与基法线取同号）。
        for hole in &part.holes {
            let m = hole.len();
            if m < 3 {
                continue;
            }
            // 洞心（世界系水平质心）。
            let (ccx, ccz) = hole.iter().fold((0.0, 0.0), |(ax, az), p| {
                let wp = w(*p, 0.0);
                (ax + wp[0] / m as f32, az + wp[2] / m as f32)
            });
            for i in 0..m {
                let (a, b) = (hole[i], hole[(i + 1) % m]);
                let (wa0, wb0) = (w(a, 0.0), w(b, 0.0));
                let (wa1, wb1) = (w(a, top), w(b, top));
                let (dx, dz) = (wb0[0] - wa0[0], wb0[2] - wa0[2]);
                let len = (dx * dx + dz * dz).sqrt().max(1e-9);
                let mut nrm = [dz / len, 0.0, -dx / len];
                let (mx, mz) = ((wa0[0] + wb0[0]) * 0.5, (wa0[2] + wb0[2]) * 0.5);
                if nrm[0] * (ccx - mx) + nrm[2] * (ccz - mz) < 0.0 {
                    nrm = [-nrm[0], 0.0, -nrm[2]]; // 翻向洞心
                }
                push_wall(&mut out, wa0, wb0, wa1, wb1, nrm, col);
            }
        }
    }
    out
}

/// 线/点要素网格（线 = 世界空间窄带贴地；点 = 十字交叉竖直 quad 标记）。
/// `line_w`/`lift` 为世界单位（extent 归一化系；跨度约 2）。
pub fn build_linework_mesh(
    lines: &[LinePart],
    points: &[PointPart],
    center: (f64, f64),
    scale: f64,
    line_w: f32,
    lift: f32,
) -> Vec<PrismVertex> {
    let mut out = Vec::new();
    let w = |p: [f64; 2], y: f32| {
        [
            ((p[0] - center.0) * scale) as f32,
            y,
            ((center.1 - p[1]) * scale) as f32,
        ]
    };
    // 线：逐段窄带（水平面 quad，法线 +y 走近纯色光照）。
    for (line, col) in lines {
        for seg in line.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let (wa, wb) = (w(a, lift), w(b, lift));
            let (dx, dz) = (wb[0] - wa[0], wb[2] - wa[2]);
            let len = (dx * dx + dz * dz).sqrt();
            if len < 1e-9 {
                continue;
            }
            let (px, pz) = (dz / len * line_w * 0.5, -dx / len * line_w * 0.5);
            let q = [
                [wa[0] + px, lift, wa[2] + pz],
                [wa[0] - px, lift, wa[2] - pz],
                [wb[0] - px, lift, wb[2] - pz],
                [wb[0] + px, lift, wb[2] + pz],
            ];
            for p in [q[0], q[1], q[2], q[0], q[2], q[3]] {
                out.push(PrismVertex {
                    pos: p,
                    nrm: [0.0, 1.0, 0.0],
                    col: *col,
                });
            }
        }
    }
    // 点：十字交叉双 quad（任意方位角可读；背剔开启后正反绕向各发一份——
    // 双份几何换恒可见，点要素量级小可承受）。
    for (pt, col) in points {
        let c = w(*pt, 0.0);
        let (sx, sy2) = (line_w * 1.5, line_w * 6.0); // 半宽/高
        for (dx, dz) in [(sx, 0.0), (0.0, sx)] {
            let q = [
                [c[0] - dx, 0.0, c[2] - dz],
                [c[0] + dx, 0.0, c[2] + dz],
                [c[0] + dx, sy2, c[2] + dz],
                [c[0] - dx, sy2, c[2] - dz],
            ];
            for (nrm, rev) in [([0.0, 0.0, 1.0], false), ([0.0, 0.0, -1.0], true)] {
                let quad = if rev { [q[0], q[3], q[2], q[1]] } else { q };
                for p in [quad[0], quad[1], quad[2], quad[0], quad[2], quad[3]] {
                    out.push(PrismVertex {
                        pos: p,
                        nrm,
                        col: *col,
                    });
                }
            }
        }
    }
    out
}

// ===== 相机（纯函数）=====

/// 轨道相机 MVP（正交投影；yaw 绕 y 轴、pitch 绕 x 轴；scale = 半高世界单位，
/// aspect = 画布宽/高）。返回列主序 4×4（wgpu uniform 直通）。
pub fn orbit_mvp(yaw: f32, pitch: f32, scale: f32, aspect: f32) -> [[f32; 4]; 4] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // R = Ry(yaw)·Rx(pitch)（行主序推导）。
    let r = [
        [cy, sy * sp, sy * cp],
        [0.0, cp, -sp],
        [-sy, cy * sp, cy * cp],
    ];
    let (sx, syz) = (scale * aspect.max(1e-6), scale.max(1e-6));
    // 正交：x/=sx，y/=syz，z∈[-1000,1000] → [0,1]。
    let rows = [
        [r[0][0] / sx, r[0][1] / sx, r[0][2] / sx, 0.0],
        [r[1][0] / syz, r[1][1] / syz, r[1][2] / syz, 0.0],
        [r[2][0] / 2000.0, r[2][1] / 2000.0, r[2][2] / 2000.0, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    // 行主序 → 列主序。
    let mut cols = [[0.0; 4]; 4];
    for (c, col) in cols.iter_mut().enumerate() {
        for (r_i, cell) in col.iter_mut().enumerate() {
            *cell = rows[r_i][c];
        }
    }
    cols
}

/// 视口感知 MVP（网格在数据范围归一化空间恒定，视口变化只动本矩阵）：
/// `rel_zoom` = 当前视口跨度 / 数据范围跨度；`cam` = 视口中心的世界坐标。
pub fn orbit_mvp_at(
    yaw: f32,
    pitch: f32,
    rel_zoom: f32,
    aspect: f32,
    cam: (f32, f32),
) -> [[f32; 4]; 4] {
    let mut m = orbit_mvp(yaw, pitch, rel_zoom.max(1e-6), aspect);
    // 平移并入第 4 列：ndc -= R·cam（cam 为 (x, z) 世界坐标，y 不受影响）。
    let adj: [f32; 3] = std::array::from_fn(|r| m[0][r] * cam.0 + m[2][r] * cam.1);
    for (cell, d) in m[3].iter_mut().zip(adj) {
        *cell -= d;
    }
    m
}

// ===== 管线与回调 =====

/// uniform（MVP + 光源方向）。
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    light: [f32; 4],
}

/// 离屏渲染目标（尺寸变化重建）。
struct Offscreen {
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    w: u32,
    h: u32,
}

/// 视图缓存（离屏纹理 + 顶点缓冲按内容纪元缓存）。
struct ViewCache {
    off: Option<Offscreen>,
    vbuf: Option<wgpu::Buffer>,
    /// 顶点缓冲内容纪元（失配重建）。
    epoch: u64,
    vcount: u32,
}

/// 场景管线（启动时挂入 callback_resources；各视图缓存按 id 分键）。
struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    views: std::collections::HashMap<usize, ViewCache>,
}

/// 帧资源（blit 绑定组；存于回调自身——多回调共存不互踩）。
struct FrameRes {
    blit_bind: wgpu::BindGroup,
}

/// 初始化 wgpu 3D 管线（eframe wgpu 后端可用时调用一次）；资源挂进
/// renderer 的 callback_resources。返回是否就绪（false = 保持软件路径）。
pub fn init(render_state: &egui_wgpu::RenderState) -> bool {
    let device = &render_state.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kanyu-3d"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    // 场景管线（group 0 = uniform）。
    let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kanyu-3d-scene"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kanyu-3d-uniform"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kanyu-3d-bind"),
        layout: &scene_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("kanyu-3d-scene-layout"),
        bind_group_layouts: &[Some(&scene_bgl)],
        immediate_size: 0,
    });
    let vbuf_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PrismVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kanyu-3d-prism"),
        layout: Some(&scene_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_prism"),
            buffers: &[vbuf_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_prism"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            // 背面剔除：全部面片经 push_face 按期望法线统一绕向（顶 +y、
            // 外壁朝外、洞内壁朝洞心）——世界系 CCW 顶视，front 恒 CCW。
            cull_mode: Some(wgpu::Face::Back),
            front_face: wgpu::FrontFace::Ccw,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    // blit 管线（group 0 = 纹理 + 采样器；无顶点缓冲——全屏三角形由 vertex_index 生成）。
    let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kanyu-3d-blit-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("kanyu-3d-blit-layout"),
        bind_group_layouts: &[Some(&blit_bgl)],
        immediate_size: 0,
    });
    let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kanyu-3d-blit"),
        layout: Some(&blit_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_blit"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_blit"),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("kanyu-3d-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(ScenePipeline {
            pipeline,
            uniform,
            bind,
            blit_pipeline,
            blit_bgl,
            sampler,
            views: std::collections::HashMap::new(),
        });
    true
}

/// 单帧绘制回调（网格 Arc 共享；帧资源经 Mutex 自持）。
struct FrameCallback {
    view_id: usize,
    epoch: u64,
    mesh: Arc<Vec<PrismVertex>>,
    mvp: [[f32; 4]; 4],
    size_px: (u32, u32),
    frame_res: Mutex<Option<FrameRes>>,
}

impl egui_wgpu::CallbackTrait for FrameCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if self.mesh.is_empty() || self.size_px.0 == 0 || self.size_px.1 == 0 {
            return Vec::new();
        }
        let Some(sp) = resources.get_mut::<ScenePipeline>() else {
            return Vec::new();
        };
        let vc = sp.views.entry(self.view_id).or_insert(ViewCache {
            off: None,
            vbuf: None,
            epoch: 0,
            vcount: 0,
        });
        // 离屏纹理按视口物理尺寸重建。
        let need_new = vc
            .off
            .as_ref()
            .is_none_or(|o| o.w != self.size_px.0 || o.h != self.size_px.1);
        if need_new {
            let make = |format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("kanyu-3d-offscreen"),
                        size: wgpu::Extent3d {
                            width: self.size_px.0,
                            height: self.size_px.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            vc.off = Some(Offscreen {
                color_view: make(
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                ),
                depth_view: make(
                    wgpu::TextureFormat::Depth32Float,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                ),
                w: self.size_px.0,
                h: self.size_px.1,
            });
        }
        // 顶点缓冲按内容纪元缓存（不变不重建）。
        if vc.epoch != self.epoch || vc.vbuf.is_none() {
            vc.vbuf = Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("kanyu-3d-vbuf"),
                    contents: bytemuck::cast_slice(&self.mesh),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            );
            vc.epoch = self.epoch;
            vc.vcount = self.mesh.len() as u32;
        }
        queue.write_buffer(
            &sp.uniform,
            0,
            bytemuck::bytes_of(&Uniforms {
                mvp: self.mvp,
                light: [-0.5, 1.0, -0.35, 0.0], // 左上前方来光
            }),
        );
        let (Some(off), Some(vbuf)) = (&vc.off, &vc.vbuf) else {
            return Vec::new();
        };
        let vcount = vc.vcount;
        // 离屏渲染（真深度缓冲；清屏纯白与画布底色一致）。
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanyu-3d-scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &off.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &off.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&sp.pipeline);
            pass.set_bind_group(0, &sp.bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..vcount, 0..1);
        }
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kanyu-3d-blit-bind"),
            layout: &sp.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&off.color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sp.sampler),
                },
            ],
        });
        *self.frame_res.lock().expect("帧资源锁") = Some(FrameRes { blit_bind });
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(sp) = resources.get::<ScenePipeline>() else {
            return;
        };
        let guard = self.frame_res.lock().expect("帧资源锁");
        let Some(fr) = guard.as_ref() else {
            return;
        };
        let vp = info.viewport_in_pixels();
        render_pass.set_viewport(
            vp.left_px as f32,
            vp.top_px as f32,
            vp.width_px as f32,
            vp.height_px as f32,
            0.0,
            1.0,
        );
        let clip = info.clip_rect_in_pixels();
        render_pass.set_scissor_rect(
            clip.left_px.max(0) as u32,
            clip.top_px.max(0) as u32,
            clip.width_px.max(0) as u32,
            clip.height_px.max(0) as u32,
        );
        render_pass.set_pipeline(&sp.blit_pipeline);
        render_pass.set_bind_group(0, &fr.blit_bind, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

/// 发射一帧 wgpu 3D 绘制（scene3d 三维态的后端分支）。
/// `mesh` 为内容纪元缓存的共享网格；`mvp` 由 [`orbit_mvp_at`] 按当前视口计算。
pub fn paint_scene(
    painter: &egui::Painter,
    rect: egui::Rect,
    pixels_per_point: f32,
    view_id: usize,
    epoch: u64,
    mesh: Arc<Vec<PrismVertex>>,
    mvp: [[f32; 4]; 4],
) {
    let cb = FrameCallback {
        view_id,
        epoch,
        mesh,
        mvp,
        size_px: (
            (rect.width() * pixels_per_point).round().max(1.0) as u32,
            (rect.height() * pixels_per_point).round().max(1.0) as u32,
        ),
        frame_res: Mutex::new(None),
    };
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, cb));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    /// 测试用射线法（点在内）。
    fn in_ring(ring: &[[f64; 2]], (x, y): (f64, f64)) -> bool {
        let mut inside = false;
        let mut j = ring.len() - 1;
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

    /// 三角形面积和。
    fn tri_area_sum(pts: &[[f64; 2]], tris: &[u32]) -> f64 {
        tris.chunks_exact(3)
            .map(|t| {
                let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() / 2.0
            })
            .sum()
    }

    #[test]
    fn earcut_convex_and_concave() {
        // 矩形：n-2 = 2 三角，面积守恒。
        let pts = square();
        let tris = earcut(&pts, &[]);
        assert_eq!(tris.len(), 6);
        assert!((tri_area_sum(&pts, &tris) - 1.0).abs() < 1e-9);
        // L 形凹多边形（单位方缺右上 0.5×0.5）：6 顶点 → 4 三角。
        let l_shape = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 0.5],
            [0.5, 0.5],
            [0.5, 1.0],
            [0.0, 1.0],
        ];
        let tris = earcut(&l_shape, &[]);
        assert_eq!(tris.len(), (l_shape.len() - 2) * 3);
        assert!(
            (tri_area_sum(&l_shape, &tris) - 0.75).abs() < 1e-9,
            "凹面面积守恒"
        );
        // 每个三角质心都在面内。
        for t in tris.chunks_exact(3) {
            let (a, b, c) = (
                l_shape[t[0] as usize],
                l_shape[t[1] as usize],
                l_shape[t[2] as usize],
            );
            let centroid = ((a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0);
            assert!(in_ring(&l_shape, centroid), "质心须在面内: {centroid:?}");
        }
        // CW 输入（反绕）同样正确。
        let mut rev = l_shape.clone();
        rev.reverse();
        assert_eq!(earcut(&rev, &[]).len(), (l_shape.len() - 2) * 3);
    }

    #[test]
    fn earcut_with_hole() {
        // 4×4 外环 + 居中 1×1 洞：面积 = 16-1 = 15。
        let outer = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let hole = vec![[1.5, 1.5], [2.5, 1.5], [2.5, 2.5], [1.5, 2.5]];
        let tris = earcut(&outer, std::slice::from_ref(&hole));
        assert!(!tris.is_empty());
        // 点表重建须与 earcut 内部归一化一致（外 CCW / 洞 CW——洞反绕后拼接）。
        let mut pts = outer.clone();
        let mut h_norm = hole.clone();
        if signed_area2(&h_norm) > 0.0 {
            h_norm.reverse();
        }
        pts.extend(h_norm.iter().copied());
        let area = tri_area_sum(&pts, &tris);
        assert!((area - 15.0).abs() < 1e-9, "带洞面积守恒: {area}");
        // 三角质心在外环内且不在洞内。
        for t in tris.chunks_exact(3) {
            let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            let centroid = ((a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0);
            assert!(in_ring(&outer, centroid));
            assert!(!in_ring(&hole, centroid), "质心不得落洞: {centroid:?}");
        }
    }

    #[test]
    fn prism_mesh_counts_and_normals() {
        // 单矩形无洞：顶面 2 三角（6 顶点）+ 4 侧壁 × 2 三角（24 顶点）= 30。
        let part = PrismPart {
            outer: square(),
            holes: Vec::new(),
            height: 10.0,
            color: [0.2, 0.5, 0.4, 1.0],
        };
        let mesh = build_prism_mesh(&[part], (0.5, 0.5), 2.0, 0.05);
        assert_eq!(mesh.len(), 30);
        // 顶面法线 +y、高度 = 10×0.05 = 0.5。
        assert_eq!(mesh[0].nrm, [0.0, 1.0, 0.0]);
        assert!((mesh[0].pos[1] - 0.5).abs() < 1e-6);
        // 侧壁法线水平（y=0）；底面顶点 y=0。
        let side = &mesh[6];
        assert_eq!(side.nrm[1], 0.0);
        assert_eq!(side.pos[1], 0.0);
        // 带洞部件：顶面三角数 = 外 4 + 洞 4 + 2 桥 - 2 = 8 三角（24 顶点）+ 外侧壁 24
        // + 洞内壁 24（洞环 4 壁 × 2 三角，背剔开启后内壁朝洞心）。
        let holed = PrismPart {
            outer: square(),
            holes: vec![vec![[0.25, 0.25], [0.5, 0.25], [0.5, 0.5], [0.25, 0.5]]],
            height: 1.0,
            color: [1.0; 4],
        };
        let mesh2 = build_prism_mesh(&[holed], (0.5, 0.5), 1.0, 1.0);
        assert_eq!(mesh2.len(), 24 + 24 + 24);
    }

    #[test]
    fn linework_mesh_lines_and_points() {
        // 一段线 = 6 顶点（窄带）；一个点 = 十字双 quad × 正反两绕向（背剔后恒可见）
        // = 24 顶点；合计 30。
        let mesh = build_linework_mesh(
            &[(vec![[0.0, 0.0], [1.0, 1.0]], [1.0, 0.0, 0.0, 1.0])],
            &[([0.5, 0.5], [0.0, 0.0, 1.0, 1.0])],
            (0.5, 0.5),
            1.0,
            0.01,
            0.002,
        );
        assert_eq!(mesh.len(), 30);
        // 线顶点抬离地面（防与棱柱底 z-fight）。
        assert!(mesh[0].pos[1] > 0.0);
    }

    #[test]
    fn orbit_mvp_identity_rotation_and_view() {
        // yaw=0/pitch=0/scale=1/aspect=1：x'=x，y'=y，z'=z/2000+0.5。
        let m = orbit_mvp(0.0, 0.0, 1.0, 1.0);
        let x = m[0][0] + m[3][0];
        let y = m[0][1] + m[3][1];
        let z = m[0][2] + m[3][2];
        assert!((x - 1.0).abs() < 1e-6 && y.abs() < 1e-6 && (z - 0.5).abs() < 1e-6);
        // yaw=90°：世界 x 不再贡献 ndc.x，世界 z 贡献 ndc.x。
        let m2 = orbit_mvp(std::f32::consts::FRAC_PI_2, 0.0, 1.0, 1.0);
        assert!(m2[0][0].abs() < 1e-6);
        assert!((m2[2][0] - 1.0).abs() < 1e-6);
        // 视口平移：cam=(0.5, 0.0) 时世界点 (0.5, y, 0) 的 ndc.x = 0。
        let m3 = orbit_mvp_at(0.0, 0.0, 1.0, 1.0, (0.5, 0.0));
        let ndc_x = m3[0][0] * 0.5 + m3[3][0];
        assert!(ndc_x.abs() < 1e-6, "相机中心应映到 ndc 0: {ndc_x}");
        // 相对缩放：rel_zoom=0.5（放大 2 倍）时 x 系数加倍。
        let m4 = orbit_mvp_at(0.0, 0.0, 0.5, 1.0, (0.0, 0.0));
        assert!((m4[0][0] - 2.0).abs() < 1e-6);
    }
}
