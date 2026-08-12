//! 实验性 3D 场景（egui painter 直绘软件 3D，无 GPU 管线）。
//!
//! **实验性**：仅供预览——斜投影（方位角 yaw 可调、俯仰 30°–45° 档位），
//! 面要素按高度字段拉伸棱柱（顶面亮色 + 侧面两档暗色，背面剔除 + 质心深度
//! 排序），线/点贴地投影。视口平移/缩放与二维共用 BBox 语义
//! （投影前先做视口裁剪）。
//!
//! 投影链（纯函数，可单测）：
//! 1. 数据坐标 → 画布 2D 坐标（与 view.rs 同一线性映射）；
//! 2. 绕画布中心旋转 yaw；
//! 3. 纵向乘 sin(pitch) 压缩，高度 z（归一化像素）向上抬升。

use eframe::egui;
use egui::{Color32, Pos2, Stroke};
use geojson::{FeatureCollection, Value as GeoValue};

use crate::theme::Palette;
use crate::ui_kit::tokens::text;
use crate::view::{self, BBox};

/// 场景状态（方位角/俯仰，弧度）。
pub struct Scene3D {
    /// 方位角（左键拖拽调节，弧度）。
    pub yaw: f32,
    /// 俯仰角（30°–45° 档位内拖调，弧度）。
    pub pitch: f32,
    /// 渲染后端（软件 painter / wgpu 真管线；wgpu 不可用时 wgpu 选项不呈现）。
    pub backend: SceneBackend,
    /// wgpu 网格缓存（内容纪元 + 网格；数据范围归一化空间——与视口解耦，
    /// 平移缩放只动 MVP 不重建；休眠框驻留 site.scene 随之保活）。
    pub mesh: Option<(u64, std::sync::Arc<Vec<crate::scene3d_wgpu::PrismVertex>>)>,
}

/// 3D 渲染后端。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SceneBackend {
    /// 软件（egui painter 斜投影棱柱——稳定默认）。
    #[default]
    Software,
    /// wgpu 真管线（PaintCallback 离屏深度 + blit；Phase 2 探针）。
    Wgpu,
}

impl Default for Scene3D {
    fn default() -> Self {
        Self {
            yaw: -0.5,
            pitch: 35f32.to_radians(),
            backend: SceneBackend::Software,
            mesh: None,
        }
    }
}

/// 俯仰档位上下限（弧度）。
const PITCH_MIN: f32 = 30f32 * std::f32::consts::PI / 180.0;
const PITCH_MAX: f32 = 45f32 * std::f32::consts::PI / 180.0;
/// 棱柱最大高度占画布高度比例（高度归一化：米/度混单位不可比，
/// 以数据最大高度锚定画布 1/4 高，保证任何数据都可读）。
const MAX_HEIGHT_FRAC: f32 = 0.25;

// ===== 纯函数（投影/棱柱/排序）=====

/// 数据坐标 → 画布 2D 坐标（view.rs 线性映射的逆；依赖视口/画布同比例不变式）。
pub fn data_to_canvas(x: f64, y: f64, bbox: BBox, w: f64, h: f64) -> (f32, f32) {
    let [minx, _, maxx, maxy] = bbox;
    (
        ((x - minx) / (maxx - minx).abs().max(1e-9) * w.abs()) as f32,
        ((maxy - y) / (maxy - bbox[1]).abs().max(1e-9) * h.abs()) as f32,
    )
}

/// 绕中心旋转（yaw，弧度；x 右 y 下的画布坐标系）。
pub fn rotate_yaw(x: f32, y: f32, cx: f32, cy: f32, yaw: f32) -> (f32, f32) {
    let (dx, dy) = (x - cx, y - cy);
    let (s, c) = yaw.sin_cos();
    (cx + dx * c - dy * s, cy + dx * s + dy * c)
}

/// 3D 投影：地面点 (gx, gy)（画布 2D 坐标）+ 高度 z_px → 屏幕点。
/// 俯仰压缩 + 高度向上抬升。
pub fn project(gx: f32, gy: f32, z_px: f32, cx: f32, cy: f32, yaw: f32, pitch: f32) -> Pos2 {
    let (rx, ry) = rotate_yaw(gx, gy, cx, cy, yaw);
    Pos2::new(rx, cy + (ry - cy) * pitch.sin() - z_px)
}

/// 侧面剔除：地面边 a→b 的外法线朝观众（旋转后 ny < 0）才绘制。
pub fn face_visible(ax: f32, ay: f32, bx: f32, by: f32, yaw: f32) -> bool {
    // 边方向 (dx,dy)（数据系 y 向上 → 画布系 y 向下，此处输入为画布 2D 坐标，
    // 旋转前先把 y 翻回"向上"语义做法线）。
    let (dx, dy) = (bx - ax, -(by - ay));
    // 外法线（CCW 环右侧）。
    let (nx, ny) = (dy, -dx);
    // 旋转后法线的"深度分量"：画布 y 向下，旋转后 ny' 向下为正 → 朝观众 = ny' < 0。
    let (s, c) = yaw.sin_cos();
    let ny_rot = nx * s + ny * c;
    ny_rot < 0.0
}

/// 棱柱深度键：环质心的旋转后纵深（ry 越大越远，先绘）。
pub fn prism_depth(ring: &[(f32, f32)], cx: f32, cy: f32, yaw: f32) -> f32 {
    if ring.is_empty() {
        return 0.0;
    }
    let (sx, sy) = ring
        .iter()
        .fold((0.0, 0.0), |(ax, ay), &(x, y)| (ax + x, ay + y));
    let (mx, my) = (sx / ring.len() as f32, sy / ring.len() as f32);
    let (_, ry) = rotate_yaw(mx, my, cx, cy, yaw);
    ry
}

/// 高度归一化比例（最大高度 → 画布高 × MAX_HEIGHT_FRAC）。
pub fn height_scale(max_h: f64, canvas_h: f32) -> f32 {
    if max_h <= 0.0 || !max_h.is_finite() {
        return 0.0;
    }
    canvas_h * MAX_HEIGHT_FRAC / max_h as f32
}

/// 取首个数值字段名作高度字段（无则 None → 默认常量）。
pub fn height_field(collection: &FeatureCollection) -> Option<String> {
    for f in &collection.features {
        if let Some(p) = &f.properties {
            for (k, v) in p {
                if v.is_number() {
                    return Some(k.clone());
                }
            }
        }
    }
    None
}

/// 棱柱（单面要素的绘制单元）。
struct Prism {
    /// 深度键（质心纵深，大者先绘）。
    depth: f32,
    /// 顶面多边形。
    top: Vec<Pos2>,
    /// 侧面四边形（已剔除背面）+ 明暗档。
    sides: Vec<(Vec<Pos2>, bool)>,
    /// 基色（顶面原色，侧面明暗派生）。
    color: Color32,
}

// ===== 场景 UI =====

/// 3D 场景帧（实验性）。逐图层按符号化主色绘制（返回是否有视口变化）。
#[allow(clippy::too_many_arguments)]
pub fn ui(
    ui: &mut egui::Ui,
    scene: &mut Scene3D,
    layers: &[crate::canvas::LayerSlice<'_>],
    view_bbox: &mut Option<BBox>,
    needs_fit: &mut bool,
    data_extent: Option<BBox>,
    p: &Palette,
    wgpu_available: bool,
    view_id: usize,
    epoch: u64,
) {
    let rect = ui.available_rect_before_wrap();
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let (w, h) = (f64::from(rect.width()), f64::from(rect.height()));
    if w < 1.0 || h < 1.0 {
        return;
    }
    // 首帧嵌入。
    if *needs_fit || view_bbox.is_none() {
        if let Some(ext) = data_extent {
            *view_bbox = Some(view::fit_view(ext, w, h));
        }
        *needs_fit = false;
    }
    let Some(bbox) = *view_bbox else {
        // 无数据：铺底纯白（地图框背景纯白约束，与界面主题解耦）+ 引导提示。
        ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "（实验性 3D：无可见图层）",
            egui::FontId::proportional(text::SIZE_BODY),
            // 白底上用晨山弱色（夜观星弱色在白底对比不足）。
            crate::theme::palette(kanyu_render::Theme::Light).text_weak,
        );
        return;
    };

    // 交互：左键旋转（方位角/俯仰档位）、右键平移、滚轮缩放（与二维同语义）。
    if response.dragged_by(egui::PointerButton::Primary) {
        let d = response.drag_delta();
        scene.yaw += d.x * 0.01;
        scene.pitch = (scene.pitch - d.y * 0.005).clamp(PITCH_MIN, PITCH_MAX);
    }
    if response.dragged_by(egui::PointerButton::Secondary) {
        let d = response.drag_delta();
        *view_bbox = Some(view::pan(bbox, f64::from(d.x), f64::from(d.y), w, h));
    }
    let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
    if response.hovered() && scroll != 0.0 {
        if let Some(pos) = response.hover_pos() {
            let anchor = view::screen_to_data(
                f64::from(pos.x - rect.min.x),
                f64::from(pos.y - rect.min.y),
                bbox,
                w,
                h,
            );
            *view_bbox = Some(view::zoom_at(
                bbox,
                anchor,
                (f64::from(scroll) * 0.002).exp(),
            ));
        }
    }
    let bbox = view_bbox.expect("早退分支已保证有视口");

    // 高度归一化（全图层统一：首个数值字段；无则常量 10）。
    let hf = layers.iter().find_map(|l| height_field(l.collection));
    let height_of = |f: &geojson::Feature| -> f64 {
        hf.as_ref()
            .and_then(|k| f.properties.as_ref()?.get(k)?.as_f64())
            .unwrap_or(10.0)
    };
    let max_h = layers
        .iter()
        .flat_map(|l| l.collection.features.iter())
        .map(&height_of)
        .fold(0.0_f64, f64::max);
    let scale = height_scale(max_h, rect.height());
    let (cx, cy) = (rect.center().x, rect.center().y);

    // 背景：恒纯白（地图框背景纯白约束，与界面主题解耦）。
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, Color32::WHITE);

    // wgpu 真管线路径（棱柱耳切顶面含洞 + 线窄带 + 点十字标记；真深度缓冲）。
    if matches!(scene.backend, SceneBackend::Wgpu) && wgpu_available {
        // 网格缓存：内容纪元失配才重建（休眠框内容冻结——激活框编辑会让
        // 全局纪元递增，浮动 3D 窗随之重建一次，语义注释即契约）。
        let mesh = match &scene.mesh {
            Some((e, m)) if *e == epoch => m.clone(),
            _ => {
                let m = std::sync::Arc::new(build_scene_mesh(layers, data_extent));
                scene.mesh = Some((epoch, m.clone()));
                m
            }
        };
        // 视口 → 相机（extent 归一化系：网格恒定，平移缩放只动 MVP）。
        let ext = data_extent.unwrap_or(bbox);
        let span_ext = (ext[2] - ext[0])
            .abs()
            .max((ext[3] - ext[1]).abs())
            .max(1e-9);
        let span_view = (bbox[2] - bbox[0])
            .abs()
            .max((bbox[3] - bbox[1]).abs())
            .max(1e-9);
        let scale0 = 2.0 / span_ext; // 数据 → 世界（与 mesh 构建同约定）
        let cam = (
            (f64::midpoint(bbox[0], bbox[2]) - f64::midpoint(ext[0], ext[2])) * scale0,
            (f64::midpoint(ext[3], ext[1]) - f64::midpoint(bbox[1], bbox[3])) * scale0,
        );
        let mvp = crate::scene3d_wgpu::orbit_mvp_at(
            scene.yaw,
            scene.pitch,
            (span_view / span_ext) as f32,
            rect.width() / rect.height().max(1.0),
            (cam.0 as f32, cam.1 as f32),
        );
        crate::scene3d_wgpu::paint_scene(
            &painter,
            rect,
            ui.ctx().pixels_per_point(),
            view_id,
            epoch,
            mesh,
            mvp,
        );
        // 状态角标（wgpu 后端标识）。
        painter.text(
            egui::pos2(rect.min.x + 8.0, rect.max.y - 8.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "wgpu 3D · 方位角 {:.0}° · 左键旋转 / 右键平移 / 滚轮缩放",
                scene.yaw.to_degrees()
            ),
            egui::FontId::proportional(text::SIZE_CAPTION),
            p.text_weak,
        );
        return;
    }

    // 收集棱柱（视口裁剪在环级做：全部顶点出界才跳过）。
    let mut prisms: Vec<Prism> = Vec::new();
    let mut ground_lines: Vec<(Vec<Pos2>, Color32)> = Vec::new();
    let mut ground_points: Vec<(Pos2, Color32)> = Vec::new();

    let ground = |x: f64, y: f64| data_to_canvas(x, y, bbox, w, h);
    let to_screen = |gx: f32, gy: f32| (rect.min.x + gx, rect.min.y + gy);

    // 逐图层按符号化主色（LayerSlice.color）绘制。
    for slice in layers {
        let layer_color = slice.color;
        for feature in &slice.collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            match &geom.value {
                GeoValue::Polygon(rings) => {
                    collect_polygon(
                        rings,
                        &ground,
                        &to_screen,
                        scale,
                        height_of(feature),
                        scene,
                        cx,
                        cy,
                        layer_color,
                        &mut prisms,
                        bbox,
                    );
                }
                GeoValue::MultiPolygon(polys) => {
                    for rings in polys {
                        collect_polygon(
                            rings,
                            &ground,
                            &to_screen,
                            scale,
                            height_of(feature),
                            scene,
                            cx,
                            cy,
                            layer_color,
                            &mut prisms,
                            bbox,
                        );
                    }
                }
                GeoValue::LineString(line) => {
                    ground_lines.push((
                        project_line(line, &ground, &to_screen, scene, cx, cy),
                        layer_color,
                    ));
                }
                GeoValue::MultiLineString(lines) => {
                    for line in lines {
                        ground_lines.push((
                            project_line(line, &ground, &to_screen, scene, cx, cy),
                            layer_color,
                        ));
                    }
                }
                GeoValue::Point(pt) => {
                    let (gx, gy) = ground(pt[0], pt[1]);
                    let (sx, sy) = to_screen(gx, gy);
                    ground_points.push((
                        project(sx, sy, 0.0, cx, cy, scene.yaw, scene.pitch),
                        layer_color,
                    ));
                }
                GeoValue::MultiPoint(pts) => {
                    for pt in pts {
                        let (gx, gy) = ground(pt[0], pt[1]);
                        let (sx, sy) = to_screen(gx, gy);
                        ground_points.push((
                            project(sx, sy, 0.0, cx, cy, scene.yaw, scene.pitch),
                            layer_color,
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    // 深度排序（远 → 近）绘制棱柱：侧面先、顶面后。
    prisms.sort_by(|a, b| b.depth.total_cmp(&a.depth));
    for prism in &prisms {
        for (quad, dark) in &prism.sides {
            let shade = if *dark { 0.55 } else { 0.75 };
            painter.add(egui::Shape::convex_polygon(
                quad.clone(),
                prism.color.gamma_multiply(shade),
                Stroke::NONE,
            ));
        }
        if prism.top.len() >= 3 {
            painter.add(egui::Shape::convex_polygon(
                prism.top.clone(),
                prism.color,
                Stroke::new(0.5, p.border),
            ));
        }
    }
    // 贴地线/点（按层色）。
    for (line, color) in &ground_lines {
        if line.len() >= 2 {
            painter.add(egui::Shape::line(line.clone(), Stroke::new(1.5, *color)));
        }
    }
    for (pt, color) in &ground_points {
        painter.circle_filled(*pt, 3.0, *color);
    }

    // 状态角标（实验性 + 方位角提示）。
    painter.text(
        egui::pos2(rect.min.x + 8.0, rect.max.y - 8.0),
        egui::Align2::LEFT_BOTTOM,
        format!(
            "实验性 3D · 方位角 {:.0}° · 左键旋转 / 右键平移 / 滚轮缩放",
            scene.yaw.to_degrees()
        ),
        egui::FontId::proportional(text::SIZE_CAPTION),
        p.text_weak,
    );
}

/// 环清洗（去闭合重复点 → [f64;2] 序列）。
fn clean_ring(r: &[Vec<f64>]) -> Vec<[f64; 2]> {
    let mut v: Vec<[f64; 2]> = r.iter().map(|p| [p[0], p[1]]).collect();
    if v.first() == v.last() {
        v.pop();
    }
    v
}

/// wgpu 场景网格构建（棱柱含洞 + 线窄带 + 点标记；extent 归一化空间——
/// 与视口解耦，内容纪元缓存的构建产物）。
fn build_scene_mesh(
    layers: &[crate::canvas::LayerSlice<'_>],
    data_extent: Option<BBox>,
) -> Vec<crate::scene3d_wgpu::PrismVertex> {
    use crate::scene3d_wgpu as sw;
    let Some(ext) = data_extent else {
        return Vec::new();
    };
    let center = (f64::midpoint(ext[0], ext[2]), f64::midpoint(ext[1], ext[3]));
    let scale = 2.0
        / (ext[2] - ext[0])
            .abs()
            .max((ext[3] - ext[1]).abs())
            .max(1e-9);
    let hf = layers.iter().find_map(|l| height_field(l.collection));
    let height_of = |f: &geojson::Feature| -> f64 {
        hf.as_ref()
            .and_then(|k| f.properties.as_ref()?.get(k)?.as_f64())
            .unwrap_or(10.0)
    };
    let max_h = layers
        .iter()
        .flat_map(|l| l.collection.features.iter())
        .map(&height_of)
        .fold(0.0_f64, f64::max);
    let hscale = if max_h > 0.0 { 0.5 / max_h } else { 0.0 } as f32;
    let mut prisms: Vec<sw::PrismPart> = Vec::new();
    let mut lines: Vec<sw::LinePart> = Vec::new();
    let mut points: Vec<sw::PointPart> = Vec::new();
    for slice in layers {
        let c = slice.color;
        let col = [
            f32::from(c.r()) / 255.0,
            f32::from(c.g()) / 255.0,
            f32::from(c.b()) / 255.0,
            1.0,
        ];
        for feature in &slice.collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            let mut push_prism = |rings: &[Vec<Vec<f64>>]| {
                let outer = rings.first().map(|r| clean_ring(r)).unwrap_or_default();
                if outer.len() >= 3 {
                    let holes = rings[1..]
                        .iter()
                        .map(|r| clean_ring(r))
                        .filter(|h| h.len() >= 3)
                        .collect();
                    prisms.push(sw::PrismPart {
                        outer,
                        holes,
                        height: height_of(feature) as f32,
                        color: col,
                    });
                }
            };
            match &geom.value {
                GeoValue::Polygon(rings) => push_prism(rings),
                GeoValue::MultiPolygon(polys) => {
                    for rings in polys {
                        push_prism(rings);
                    }
                }
                GeoValue::LineString(l) => {
                    let v = clean_ring(l);
                    if v.len() >= 2 {
                        lines.push((v, col));
                    }
                }
                GeoValue::MultiLineString(ls) => {
                    for l in ls {
                        let v = clean_ring(l);
                        if v.len() >= 2 {
                            lines.push((v, col));
                        }
                    }
                }
                GeoValue::Point(p) => points.push(([p[0], p[1]], col)),
                GeoValue::MultiPoint(ps) => {
                    for p in ps {
                        points.push(([p[0], p[1]], col));
                    }
                }
                _ => {}
            }
        }
    }
    let mut mesh = sw::build_prism_mesh(&prisms, center, scale, hscale);
    // 线宽/抬升（世界单位，extent 跨度约 2）。
    mesh.extend(sw::build_linework_mesh(
        &lines, &points, center, scale, 0.005, 0.002,
    ));
    mesh
}

/// 单个面要素 → 棱柱（仅外环；视口外跳过）。颜色 = 图层符号化主色。
#[allow(clippy::too_many_arguments)]
fn collect_polygon(
    rings: &[Vec<Vec<f64>>],
    ground: &dyn Fn(f64, f64) -> (f32, f32),
    to_screen: &dyn Fn(f32, f32) -> (f32, f32),
    scale: f32,
    height: f64,
    scene: &Scene3D,
    cx: f32,
    cy: f32,
    color: Color32,
    prisms: &mut Vec<Prism>,
    bbox: BBox,
) {
    let Some(ring) = rings.first() else { return };
    if ring.len() < 4 {
        return;
    }
    // 视口裁剪（环外接矩形与视口相交判定）。
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for pt in ring {
        minx = minx.min(pt[0]);
        miny = miny.min(pt[1]);
        maxx = maxx.max(pt[0]);
        maxy = maxy.max(pt[1]);
    }
    if maxx < bbox[0] || minx > bbox[2] || maxy < bbox[1] || miny > bbox[3] {
        return;
    }
    // 地面环（画布 2D → 屏幕坐标）。
    let ground_ring: Vec<(f32, f32)> = ring
        .iter()
        .map(|pt| {
            let (gx, gy) = ground(pt[0], pt[1]);
            to_screen(gx, gy)
        })
        .collect();
    let z = height.max(0.0) as f32 * scale;
    let proj = |(sx, sy): (f32, f32), z: f32| project(sx, sy, z, cx, cy, scene.yaw, scene.pitch);
    let top: Vec<Pos2> = ground_ring.iter().map(|&g| proj(g, z)).collect();
    let mut sides = Vec::new();
    for i in 0..ground_ring.len() - 1 {
        let (a, b) = (ground_ring[i], ground_ring[i + 1]);
        // 背面剔除（a→b 以地面画布坐标判定）。
        if !face_visible(a.0, a.1, b.0, b.1, scene.yaw) {
            continue;
        }
        let quad = vec![proj(a, 0.0), proj(b, 0.0), proj(b, z), proj(a, z)];
        // 明暗两档：按边方向的旋转后 x 分量分档。
        let dark = {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let (s, c) = scene.yaw.sin_cos();
            (dx * c - dy * s) > 0.0
        };
        sides.push((quad, dark));
    }
    let depth = prism_depth(&ground_ring, cx, cy, scene.yaw);
    prisms.push(Prism {
        depth,
        top,
        sides,
        color,
    });
}

/// 折线 → 贴地投影。
fn project_line(
    line: &[Vec<f64>],
    ground: &dyn Fn(f64, f64) -> (f32, f32),
    to_screen: &dyn Fn(f32, f32) -> (f32, f32),
    scene: &Scene3D,
    cx: f32,
    cy: f32,
) -> Vec<Pos2> {
    line.iter()
        .map(|pt| {
            let (gx, gy) = ground(pt[0], pt[1]);
            let (sx, sy) = to_screen(gx, gy);
            project(sx, sy, 0.0, cx, cy, scene.yaw, scene.pitch)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_yaw_identity_and_quarter() {
        let (x, y) = rotate_yaw(10.0, 5.0, 0.0, 0.0, 0.0);
        assert!((x - 10.0).abs() < 1e-5 && (y - 5.0).abs() < 1e-5);
        // 旋转 90°：(1,0) → (0,1)。
        let (x, y) = rotate_yaw(1.0, 0.0, 0.0, 0.0, std::f32::consts::FRAC_PI_2);
        assert!(x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5);
        // 绕中心旋转不动中心。
        let (x, y) = rotate_yaw(100.0, 50.0, 100.0, 50.0, 1.234);
        assert!((x - 100.0).abs() < 1e-4 && (y - 50.0).abs() < 1e-4);
    }

    #[test]
    fn project_height_lifts_up() {
        let base = project(100.0, 100.0, 0.0, 0.0, 0.0, 0.0, 0.6);
        let lifted = project(100.0, 100.0, 50.0, 0.0, 0.0, 0.0, 0.6);
        assert_eq!(base.x, lifted.x);
        assert!((base.y - lifted.y - 50.0).abs() < 1e-4); // 高度向上抬
                                                          // 俯仰压缩：pitch=90° 不压缩（直达原 2D 位置），档位内 y 介于中心与未压缩之间。
        let full = project(0.0, 200.0, 0.0, 0.0, 0.0, 0.0, std::f32::consts::FRAC_PI_2);
        assert!((full.y - 200.0).abs() < 1e-4);
        let tilted = project(0.0, 200.0, 0.0, 0.0, 0.0, 0.0, 0.8);
        assert!(tilted.y > 0.0 && tilted.y < full.y);
    }

    #[test]
    fn face_visibility_backface_culling() {
        // CCW 环（画布坐标 y 向下）：南边左→右 (0,1)→(1,1) 朝观众（yaw=0）可见；
        // 北边右→左 (1,0)→(0,0) 背对观众剔除。
        assert!(face_visible(0.0, 1.0, 1.0, 1.0, 0.0));
        assert!(!face_visible(1.0, 0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn prism_depth_far_first() {
        let near = prism_depth(&[(0.0, 10.0), (1.0, 10.0)], 0.0, 0.0, 0.0);
        let far = prism_depth(&[(0.0, 90.0), (1.0, 90.0)], 0.0, 0.0, 0.0);
        // yaw=0：ry = y；y 大者远。
        assert!(far > near);
    }

    #[test]
    fn height_scale_normalizes() {
        assert_eq!(height_scale(100.0, 800.0), 2.0); // 100 → 200px
        assert_eq!(height_scale(0.0, 800.0), 0.0);
        assert_eq!(height_scale(f64::NAN, 800.0), 0.0);
    }

    #[test]
    fn height_field_first_numeric() {
        let c: FeatureCollection = serde_json::from_str(
            r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":null,
            "properties":{"name":"甲","height":88.5}}]}"#,
        )
        .unwrap();
        assert_eq!(height_field(&c).as_deref(), Some("height"));
        let empty = FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        assert!(height_field(&empty).is_none());
    }
}
