//! 视图数学（纯函数，可单测）：MapCanvas 的缩放 / 平移 / 坐标逆变换。
//!
//! 核心不变式：view_bbox 的宽高比始终等于画布像素宽高比（`fit_view` 建立、
//! `zoom_at` 等比维持、`pan` 平移不改变比例）。渲染内核收到与画布同比例的
//! 视口后等比缩放不产生额外居中偏移（letterbox 为零），因此
//! `screen_to_data` 是简单的线性映射。

/// 数据坐标 bbox：`[minx, miny, maxx, maxy]`。
pub type BBox = [f64; 4];

/// 单点/零跨度退化时的默认视野跨度（约 0.001 度，与 render 内核一致）。
const DEGENERATE_SPAN: f64 = 0.001;
/// 视口跨度下限：防止无限放大越过 render 内核的零跨度防护（1e-9）。
const MIN_SPAN: f64 = 1e-7;
/// 视口跨度上限：render 内核拒绝经度跨度 > 350° 的数据。
const MAX_SPAN: f64 = 340.0;

/// 把数据范围等比嵌入画布比例（以数据中心为准扩边）。
///
/// 画布之外的"留白"发生在数据空间而非屏幕空间：扩边后的 bbox 与画布
/// 同比例，渲染时恰好铺满。零跨度输入按 [`DEGENERATE_SPAN`] 给默认视野。
pub fn fit_view(extent: BBox, width_px: f64, height_px: f64) -> BBox {
    let [minx, miny, maxx, maxy] = extent;
    let mut span_x = (maxx - minx).abs();
    let mut span_y = (maxy - miny).abs();
    if span_x < 1e-12 {
        span_x = DEGENERATE_SPAN;
    }
    if span_y < 1e-12 {
        span_y = DEGENERATE_SPAN;
    }
    let canvas_ratio = (width_px.abs() / height_px.abs().max(1e-9)).max(1e-9);
    if span_x / span_y < canvas_ratio {
        span_x = span_y * canvas_ratio; // 数据太窄：横向扩边
    } else {
        span_y = span_x / canvas_ratio; // 数据太扁：纵向扩边
    }
    let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    [
        cx - span_x / 2.0,
        cy - span_y / 2.0,
        cx + span_x / 2.0,
        cy + span_y / 2.0,
    ]
}

/// 以数据锚点为不动点缩放（`factor > 1` 放大，跨度变为 1/factor）。
///
/// 等比缩放保持画布比例不变式；锚点（通常为鼠标处的数据坐标）在视口
/// 内的相对位置不变。
pub fn zoom_at(bbox: BBox, anchor: (f64, f64), factor: f64) -> BBox {
    let f = factor.clamp(1e-9, 1e9);
    let [minx, miny, maxx, maxy] = bbox;
    let (ax, ay) = anchor;
    clamp_span([
        ax - (ax - minx) / f,
        ay - (ay - miny) / f,
        ax + (maxx - ax) / f,
        ay + (maxy - ay) / f,
    ])
}

/// 抓取式平移：屏幕位移 `(dx_px, dy_px)`（右/下为正）时内容跟随鼠标，
/// 视口反向移动；屏幕 y 向下而数据 y 向上，故 y 方向符号翻转。
pub fn pan(bbox: BBox, dx_px: f64, dy_px: f64, width_px: f64, height_px: f64) -> BBox {
    let [minx, miny, maxx, maxy] = bbox;
    let ux = (maxx - minx) / width_px.abs().max(1e-9);
    let uy = (maxy - miny) / height_px.abs().max(1e-9);
    [
        minx - dx_px * ux,
        miny + dy_px * uy,
        maxx - dx_px * ux,
        maxy + dy_px * uy,
    ]
}

/// 屏幕坐标（画布左上角原点，像素）→ 数据坐标（线性映射，
/// 依赖 view_bbox 与画布同比例的不变式；y 轴翻转）。
pub fn screen_to_data(sx: f64, sy: f64, bbox: BBox, width_px: f64, height_px: f64) -> (f64, f64) {
    let [minx, miny, maxx, maxy] = bbox;
    (
        minx + sx / width_px.abs().max(1e-9) * (maxx - minx),
        maxy - sy / height_px.abs().max(1e-9) * (maxy - miny),
    )
}

/// 视口跨度约束到 `[MIN_SPAN, MAX_SPAN]`（等比、以中心为准），
/// 防止缩放越过渲染内核的安全域。
pub fn clamp_span(bbox: BBox) -> BBox {
    let [minx, miny, maxx, maxy] = bbox;
    let (cx, cy) = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
    let clamp_one = |span: f64| span.clamp(MIN_SPAN, MAX_SPAN);
    let (sx, sy) = (clamp_one(maxx - minx), clamp_one(maxy - miny));
    [cx - sx / 2.0, cy - sy / 2.0, cx + sx / 2.0, cy + sy / 2.0]
}

/// 合并多个 bbox（全 None 时返回 None）。
pub fn union(bboxes: impl IntoIterator<Item = BBox>) -> Option<BBox> {
    bboxes
        .into_iter()
        .reduce(|[a0, a1, a2, a3], [b0, b1, b2, b3]| {
            [a0.min(b0), a1.min(b1), a2.max(b2), a3.max(b3)]
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_bbox_close(a: BBox, b: BBox) {
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x - y).abs() < 1e-9, "bbox 不符: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn fit_view_wide_data_expands_height_around_center() {
        // 宽扁数据（10×1）嵌入 2:1 画布：比例 10 > 2，纵向扩边，中心不变。
        let bbox = fit_view([0.0, 0.0, 10.0, 1.0], 800.0, 400.0);
        let (cx, cy) = ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
        assert!((cx - 5.0).abs() < 1e-9 && (cy - 0.5).abs() < 1e-9);
        let ratio = (bbox[2] - bbox[0]) / (bbox[3] - bbox[1]);
        assert!(
            (ratio - 2.0).abs() < 1e-9,
            "bbox 比例应等于画布比例: {ratio}"
        );
        // 横向保持原跨度（数据铺满宽）。
        assert!((bbox[2] - bbox[0] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fit_view_tall_data_expands_width() {
        // 高瘦数据（1×10）嵌入 2:1 画布：横向扩边。
        let bbox = fit_view([0.0, 0.0, 1.0, 10.0], 800.0, 400.0);
        let ratio = (bbox[2] - bbox[0]) / (bbox[3] - bbox[1]);
        assert!((ratio - 2.0).abs() < 1e-9);
        assert!((bbox[3] - bbox[1] - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fit_view_single_point_gets_default_span() {
        let bbox = fit_view([116.39, 39.90, 116.39, 39.90], 800.0, 800.0);
        assert!((bbox[2] - bbox[0] - DEGENERATE_SPAN).abs() < 1e-12);
        let (cx, cy) = ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
        assert!((cx - 116.39).abs() < 1e-9 && (cy - 39.90).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_keeps_anchor_and_halves_span() {
        let bbox: BBox = [0.0, 0.0, 10.0, 10.0];
        let anchor = (2.5, 5.0);
        let z = zoom_at(bbox, anchor, 2.0);
        // 跨度减半。
        assert!((z[2] - z[0] - 5.0).abs() < 1e-9);
        assert!((z[3] - z[1] - 5.0).abs() < 1e-9);
        // 锚点在视口内的相对位置不变：(anchor - min) / span 缩放前后相等。
        let rel_before = (anchor.0 - bbox[0]) / (bbox[2] - bbox[0]);
        let rel_after = (anchor.0 - z[0]) / (z[2] - z[0]);
        assert!((rel_before - rel_after).abs() < 1e-9);
    }

    #[test]
    fn pan_moves_view_opposite_to_drag_with_y_flip() {
        let bbox: BBox = [0.0, 0.0, 100.0, 100.0];
        // 画布 500px 宽 → 0.2 数据单位/px；右拖 50px → 视口左移 10。
        let p = pan(bbox, 50.0, 0.0, 500.0, 500.0);
        assert_bbox_close(p, [-10.0, 0.0, 90.0, 100.0]);
        // 下拖 50px（屏幕向下）→ 视口数据 y 上移 10（y 翻转）。
        let p = pan(bbox, 0.0, 50.0, 500.0, 500.0);
        assert_bbox_close(p, [0.0, 10.0, 100.0, 110.0]);
    }

    #[test]
    fn screen_to_data_maps_corners_and_roundtrips() {
        let bbox: BBox = [10.0, 20.0, 110.0, 220.0];
        let (w, h) = (400.0, 200.0);
        assert_eq!(screen_to_data(0.0, 0.0, bbox, w, h), (10.0, 220.0)); // 左上 → (minx, maxy)
        assert_eq!(screen_to_data(w, h, bbox, w, h), (110.0, 20.0)); // 右下 → (maxx, miny)
                                                                     // 画布中心 → bbox 中心。
        let (cx, cy) = screen_to_data(w / 2.0, h / 2.0, bbox, w, h);
        assert!((cx - 60.0).abs() < 1e-9 && (cy - 120.0).abs() < 1e-9);
    }

    #[test]
    fn clamp_span_enforces_limits() {
        // 过小 → 拉回 MIN_SPAN（中心不变）。
        let tiny = clamp_span([0.0, 0.0, 1e-12, 1e-12]);
        assert!((tiny[2] - tiny[0] - MIN_SPAN).abs() < 1e-15);
        // 过大 → 拉回 MAX_SPAN。
        let huge = clamp_span([-500.0, -500.0, 500.0, 500.0]);
        assert!((huge[2] - huge[0] - MAX_SPAN).abs() < 1e-9);
        assert!((huge[3] - huge[1] - MAX_SPAN).abs() < 1e-9);
    }

    #[test]
    fn union_merges_and_empty_is_none() {
        assert_eq!(union([]), None);
        let u = union([[0.0, 0.0, 1.0, 1.0], [-2.0, 0.5, 0.5, 3.0]]).unwrap();
        assert_bbox_close(u, [-2.0, 0.0, 1.0, 3.0]);
    }
}
