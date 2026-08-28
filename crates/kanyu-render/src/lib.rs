//! # kanyu-render —— 堪舆的眼睛（离屏地图渲染）
//!
//! 让 AI 代理"看见"地理数据：GeoJSON FeatureCollection → SVG（纯字符串，
//! 零依赖）或 PNG（tiny-skia 纯 Rust CPU 光栅化，CI 无 GPU 依赖）。
//!
//! 色彩系统取自总规 §1.2：亮色"晨山"（米白 #F0EDE8 画布、墨黑 #1A1A1A
//! 描边、远黛青 #2D6A5E 填充）与暗色"夜观星"（极暗 #0D0F12 画布、
//! 月白 #E8E4DF 描边、青玉 #4DB8A8 填充）。**分工说明**：本 crate 只做
//! 离屏渲染（MCP 回传图片）；wgpu 实时渲染管线（GeoArrow→SSBO 直通）
//! 属于交互壳层 kanyu-shell，不在此实现。

use geojson::{FeatureCollection, Value};

/// 打印布局排版器（SVG + PNG；见模块 rustdoc）。
pub mod layout;
/// 宗地图渲染器（GB/T 42547 图 L.3 版式；见模块 rustdoc）。
pub mod parcelmap;

/// 渲染错误。
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// 输出尺寸非法。
    #[error("渲染尺寸非法（{0}×{1}，宽高须 > 0 且 ≤ 8192）")]
    InvalidSize(u32, u32),
    /// 坐标范围异常。
    #[error("坐标范围异常：{0}")]
    InvalidExtent(String),
    /// PNG 编码失败。
    #[error("PNG 编码失败: {0}")]
    Encode(String),
    /// 颜色值非法。
    #[error("颜色值非法: {0}")]
    InvalidColor(String),
    /// 样式规则非法。
    #[error("样式规则非法：{0}")]
    InvalidStyle(String),
}

/// 渲染主题（总规 §1.2 色彩系统）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// 晨山（亮色）。
    #[default]
    Light,
    /// 夜观星（暗色）。
    Dark,
}

impl std::str::FromStr for Theme {
    type Err = RenderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => Err(RenderError::InvalidColor(format!(
                "未知主题 '{other}'（支持 light/dark）"
            ))),
        }
    }
}

impl Theme {
    /// 主题名（输出元数据用）。
    pub fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// 渲染选项。
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// 输出宽度（像素，1–8192）。
    pub width: u32,
    /// 输出高度（像素，1–8192）。
    pub height: u32,
    /// 四周边距（像素）。
    pub padding: f64,
    /// 主题。
    pub theme: Theme,
    /// 自定义背景色（`#RRGGBB`；缺省用主题画布色）。
    /// `"none"` / `"transparent"` = 透明背景（不铺画布色，供底图叠加场景）。
    pub background: Option<String>,
    /// 属性驱动样式规则（缺省走主题默认样式，行为与旧版一致）。
    pub style: Option<StyleRule>,
    /// 显式视口 `[minx, miny, maxx, maxy]`（数据坐标）：给出时跳过集合
    /// bbox 自动适配，直接以该范围等比缩放居中渲染——交互壳层（kanyu-shell）
    /// 的缩放/平移即通过每帧传入变化后的视口实现。缺省沿用自动适配。
    pub viewport: Option<[f64; 4]>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            padding: 20.0,
            theme: Theme::Light,
            background: None,
            style: None,
            viewport: None,
        }
    }
}

/// 属性驱动样式规则（总规 §3.4 符号定义子集，JSON `type` 判别）。
///
/// - `{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}`
///   数值字段分档：取**最后一个满足 `值 ≥ 阈值` 的档**（恰等阈值取该档，
///   低于首档走默认样式）；stops 须非空且阈值严格升序。
/// - `{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E"},"default":"#888888"}`
///   字符串字段类别映射；无匹配取 `default`（亦缺省则走默认样式）。
///
/// 颜色为 `#RRGGBB`；字段缺失或类型不符的要素走主题默认样式（不产生脏样式）。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StyleRule {
    /// 数值分档。
    Graduated {
        /// 数值字段名。
        field: String,
        /// `[[阈值, 颜色], …]`（严格升序）。
        stops: Vec<(f64, String)>,
    },
    /// 字符串类别。
    Categorical {
        /// 字符串字段名。
        field: String,
        /// 类别值 → 颜色。
        colors: std::collections::HashMap<String, String>,
        /// 无匹配时的颜色（可选）。
        default: Option<String>,
    },
}

impl StyleRule {
    /// 语义校验（render 入口统一调用）：空 stops、非升序、非法颜色值
    /// 均报中文错误并指出出错项。
    pub fn validate(&self) -> Result<(), RenderError> {
        match self {
            Self::Graduated { field, stops } => {
                if stops.is_empty() {
                    return Err(RenderError::InvalidStyle(format!(
                        "graduated（字段 '{field}'）stops 不能为空"
                    )));
                }
                for (i, (threshold, color)) in stops.iter().enumerate() {
                    parse_hex_color(color).map_err(|e| {
                        RenderError::InvalidStyle(format!(
                            "graduated（字段 '{field}'）第 {} 档颜色非法: {e}",
                            i + 1
                        ))
                    })?;
                    if i > 0 && *threshold <= stops[i - 1].0 {
                        return Err(RenderError::InvalidStyle(format!(
                            "graduated（字段 '{field}'）stops 阈值须严格升序（第 {} 档 {threshold} 不大于前档）",
                            i + 1
                        )));
                    }
                }
            }
            Self::Categorical {
                field,
                colors,
                default,
            } => {
                for (key, color) in colors {
                    parse_hex_color(color).map_err(|e| {
                        RenderError::InvalidStyle(format!(
                            "categorical（字段 '{field}'）类别 '{key}' 颜色非法: {e}"
                        ))
                    })?;
                }
                if let Some(d) = default {
                    parse_hex_color(d).map_err(|e| {
                        RenderError::InvalidStyle(format!(
                            "categorical（字段 '{field}'）default 颜色非法: {e}"
                        ))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// 要素属性 → 命中颜色（`#RRGGBB` 原样返回；未命中走默认样式返回 None）。
    pub fn color_for(
        &self,
        properties: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<String> {
        match self {
            Self::Graduated { field, stops } => {
                let v = properties.get(field)?.as_f64()?;
                // 最后一个满足 v >= threshold 的档（升序遍历遇 false 即止）。
                let mut chosen = None;
                for (threshold, color) in stops {
                    if v >= *threshold {
                        chosen = Some(color.clone());
                    } else {
                        break;
                    }
                }
                chosen
            }
            Self::Categorical {
                field,
                colors,
                default,
            } => {
                let key = properties.get(field)?.as_str()?;
                colors.get(key).cloned().or_else(|| default.clone())
            }
        }
    }
}

/// 几何类别（样式分派键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeomKind {
    Polygon,
    Line,
    Point,
}

/// 一类几何的样式（单一事实来源：全部样式的唯一定义处）。
struct GeometryStyle {
    /// 填充色（`#RRGGBB`；None = 不填充）。
    fill: Option<String>,
    /// 填充不透明度（0–1；面 20%，点 100%）。
    fill_opacity: f64,
    /// 描边色。
    stroke: String,
    /// 描边宽度（像素）。
    stroke_width: f64,
    /// 点半径（像素）。
    point_radius: f64,
}

/// 主题画布色。
fn canvas_color(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => "#F0EDE8", // 米白
        Theme::Dark => "#0D0F12",  // 极暗
    }
}

/// 几何类别 × 主题 → 样式（总规 §1.2 色板）。
fn style_for(kind: GeomKind, theme: Theme) -> GeometryStyle {
    let (ink, accent) = match theme {
        Theme::Light => ("#1A1A1A", "#2D6A5E"), // 墨黑 / 远黛青
        Theme::Dark => ("#E8E4DF", "#4DB8A8"),  // 月白 / 青玉
    };
    match kind {
        GeomKind::Polygon => GeometryStyle {
            fill: Some(accent.to_string()),
            fill_opacity: 0.2,
            stroke: ink.to_string(),
            stroke_width: 1.5,
            point_radius: 0.0,
        },
        GeomKind::Line => GeometryStyle {
            fill: None,
            fill_opacity: 1.0,
            stroke: ink.to_string(),
            stroke_width: 2.0,
            point_radius: 0.0,
        },
        GeomKind::Point => GeometryStyle {
            fill: Some(accent.to_string()),
            fill_opacity: 1.0,
            stroke: ink.to_string(),
            stroke_width: 1.0,
            point_radius: 4.0,
        },
    }
}

/// 有效样式：属性驱动命中时按几何类型派生（面=该色 20% 透明填充 + 同色
/// 描边、线=该色描边、点=该色填充），未命中走主题默认（样式决策仍在一处）。
fn effective_style(kind: GeomKind, theme: Theme, override_color: Option<&str>) -> GeometryStyle {
    let base = style_for(kind, theme);
    let Some(color) = override_color else {
        return base;
    };
    match kind {
        GeomKind::Polygon => GeometryStyle {
            fill: Some(color.to_string()),
            stroke: color.to_string(),
            ..base
        },
        GeomKind::Line => GeometryStyle {
            stroke: color.to_string(),
            ..base
        },
        GeomKind::Point => GeometryStyle {
            fill: Some(color.to_string()),
            stroke: color.to_string(),
            ..base
        },
    }
}

/// 集合坐标范围。
#[derive(Debug, Clone, Copy)]
struct Extent {
    minx: f64,
    miny: f64,
    maxx: f64,
    maxy: f64,
}

/// 扫描全集合计算 bbox（忽略无几何要素；坐标必须有限）。
fn compute_extent(collection: &FeatureCollection) -> Result<Option<Extent>, RenderError> {
    let mut ext: Option<Extent> = None;
    let mut visit = |x: f64, y: f64| -> Result<(), RenderError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(RenderError::InvalidExtent(
                "存在非有限坐标（NaN/Inf），无法渲染".to_string(),
            ));
        }
        ext = Some(match ext {
            None => Extent {
                minx: x,
                miny: y,
                maxx: x,
                maxy: y,
            },
            Some(e) => Extent {
                minx: e.minx.min(x),
                miny: e.miny.min(y),
                maxx: e.maxx.max(x),
                maxy: e.maxy.max(y),
            },
        });
        Ok(())
    };
    for feature in &collection.features {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        visit_value(&geom.value, &mut visit)?;
    }
    Ok(ext)
}

/// 集合坐标范围 `[minx, miny, maxx, maxy]`（无几何要素返回 None；
/// 非有限坐标报错）。交互壳层据此设定初始视口。
pub fn collection_extent(collection: &FeatureCollection) -> Result<Option<[f64; 4]>, RenderError> {
    Ok(compute_extent(collection)?.map(|e| [e.minx, e.miny, e.maxx, e.maxy]))
}

/// 渲染入口统一的范围来源：显式视口优先，缺省扫描集合自适应。
fn resolve_extent(
    collection: &FeatureCollection,
    opts: &RenderOptions,
) -> Result<Option<Extent>, RenderError> {
    match opts.viewport {
        Some([minx, miny, maxx, maxy]) => {
            if ![minx, miny, maxx, maxy].iter().all(|v| v.is_finite()) {
                return Err(RenderError::InvalidExtent(
                    "显式视口含非有限坐标（NaN/Inf）".to_string(),
                ));
            }
            if minx > maxx || miny > maxy {
                return Err(RenderError::InvalidExtent(format!(
                    "显式视口最小值大于最大值（[{minx}, {miny}, {maxx}, {maxy}]）"
                )));
            }
            Ok(Some(Extent {
                minx,
                miny,
                maxx,
                maxy,
            }))
        }
        None => compute_extent(collection),
    }
}

/// 递归遍历全部坐标。
fn visit_value(
    value: &Value,
    visit: &mut dyn FnMut(f64, f64) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    match value {
        Value::Point(p) => visit(p[0], p[1]),
        Value::MultiPoint(ps) | Value::LineString(ps) => {
            for p in ps {
                visit(p[0], p[1])?;
            }
            Ok(())
        }
        Value::MultiLineString(ls) | Value::Polygon(ls) => {
            for l in ls {
                visit_value(&Value::LineString(std::mem::take(&mut l.clone())), visit)?;
            }
            Ok(())
        }
        Value::MultiPolygon(ps) => {
            for poly in ps {
                visit_value(&Value::Polygon(poly.clone()), visit)?;
            }
            Ok(())
        }
        Value::GeometryCollection(gs) => {
            for g in gs {
                visit_value(&g.value, visit)?;
            }
            Ok(())
        }
    }
}

/// 视口变换：bbox 等比缩放、居中、留边距；y 轴翻转（屏幕坐标向下）。
#[derive(Debug, Clone, Copy)]
struct Viewport {
    scale: f64,
    minx: f64,
    maxy: f64,
    offset_x: f64,
    offset_y: f64,
}

impl Viewport {
    fn new(
        extent: Option<Extent>,
        width: u32,
        height: u32,
        padding: f64,
    ) -> Result<Self, RenderError> {
        let (w, h) = (width as f64, height as f64);
        let (inner_w, inner_h) = ((w - 2.0 * padding).max(1.0), (h - 2.0 * padding).max(1.0));
        let e = extent.unwrap_or(Extent {
            minx: 0.0,
            miny: 0.0,
            maxx: 1.0,
            maxy: 1.0,
        });
        let span_x = e.maxx - e.minx;
        let span_y = e.maxy - e.miny;
        if span_x > 350.0 {
            return Err(RenderError::InvalidExtent(format!(
                "经度跨度 {span_x:.1}° > 350°（疑似跨日期变更线数据，暂不支持自动适配）"
            )));
        }
        // 单点/零跨度退化：给默认视野（约 0.001 度），避免除零。
        let span_x = if span_x < 1e-9 { 0.001 } else { span_x };
        let span_y = if span_y < 1e-9 { 0.001 } else { span_y };
        let scale = (inner_w / span_x).min(inner_h / span_y);
        let offset_x = padding + (inner_w - span_x * scale) / 2.0;
        let offset_y = padding + (inner_h - span_y * scale) / 2.0;
        // 零跨度时以点为中心居中。
        let (minx, maxy) =
            if extent.is_none() || (e.maxx - e.minx) < 1e-9 && (e.maxy - e.miny) < 1e-9 {
                let cx = if extent.is_some() {
                    (e.minx + e.maxx) / 2.0
                } else {
                    0.5
                };
                let cy = if extent.is_some() {
                    (e.miny + e.maxy) / 2.0
                } else {
                    0.5
                };
                (cx - 0.0005, cy + 0.0005)
            } else {
                (e.minx, e.maxy)
            };
        Ok(Self {
            scale,
            minx,
            maxy,
            offset_x,
            offset_y,
        })
    }

    /// 地理坐标 → 屏幕坐标。
    fn project(&self, x: f64, y: f64) -> (f32, f32) {
        (
            ((x - self.minx) * self.scale + self.offset_x) as f32,
            ((self.maxy - y) * self.scale + self.offset_y) as f32,
        )
    }
}

/// 透明背景判定（`"none"` / `"transparent"`，大小写不敏感）。
fn transparent_bg(opts: &RenderOptions) -> bool {
    matches!(
        opts.background
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("none") | Some("transparent")
    )
}

fn validate_options(opts: &RenderOptions) -> Result<(), RenderError> {
    if opts.width == 0 || opts.height == 0 || opts.width > 8192 || opts.height > 8192 {
        return Err(RenderError::InvalidSize(opts.width, opts.height));
    }
    if let Some(bg) = &opts.background {
        if !transparent_bg(opts) {
            parse_hex_color(bg)?;
        }
    }
    if let Some(rule) = &opts.style {
        rule.validate()?;
    }
    Ok(())
}

/// 要素命中色（有 style 规则时按属性求色）。
fn feature_color(opts: &RenderOptions, feature: &geojson::Feature) -> Option<String> {
    opts.style
        .as_ref()
        .and_then(|rule| feature.properties.as_ref().and_then(|p| rule.color_for(p)))
}

/// `#RRGGBB` → (r, g, b)。
fn parse_hex_color(hex: &str) -> Result<(u8, u8, u8), RenderError> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return Err(RenderError::InvalidColor(format!(
            "颜色须为 #RRGGBB 形式: '{hex}'"
        )));
    }
    let parse = |s: &str| {
        u8::from_str_radix(s, 16)
            .map_err(|_| RenderError::InvalidColor(format!("颜色含非法十六进制: '{hex}'")))
    };
    Ok((parse(&h[0..2])?, parse(&h[2..4])?, parse(&h[4..6])?))
}

fn fmt(x: f32) -> String {
    let s = format!("{x:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ===== SVG 渲染 =====

/// 渲染为 SVG 字符串（viewBox + 背景 rect + 注释头）。
pub fn render_svg(
    collection: &FeatureCollection,
    opts: &RenderOptions,
) -> Result<String, RenderError> {
    validate_options(opts)?;
    let viewport = Viewport::new(
        resolve_extent(collection, opts)?,
        opts.width,
        opts.height,
        opts.padding,
    )?;
    let background = opts
        .background
        .clone()
        .unwrap_or_else(|| canvas_color(opts.theme).to_string());

    let mut out = String::with_capacity(4096);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">\n",
        opts.width, opts.height, opts.width, opts.height
    ));
    out.push_str(&format!(
        "<!-- kanyu-render | theme: {} | features: {} -->\n",
        opts.theme.name(),
        collection.features.len()
    ));
    // 透明背景（none/transparent）不铺背景 rect，供底图叠加（SVG 默认透明）
    if !transparent_bg(opts) {
        out.push_str(&format!(
            "<rect width=\"100%\" height=\"100%\" fill=\"{background}\"/>\n"
        ));
    }
    // 绘制顺序：面 → 线 → 点（点最上层）。
    for kind in [GeomKind::Polygon, GeomKind::Line, GeomKind::Point] {
        for feature in &collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            let style = effective_style(kind, opts.theme, feature_color(opts, feature).as_deref());
            svg_value(&mut out, &geom.value, kind, &style, &viewport);
        }
    }
    out.push_str("</svg>\n");
    Ok(out)
}

/// 按类别绘制单个几何（Multi* 逐部件，GeometryCollection 递归）。
fn svg_value(
    out: &mut String,
    value: &Value,
    kind: GeomKind,
    style: &GeometryStyle,
    viewport: &Viewport,
) {
    match (value, kind) {
        (Value::Polygon(rings), GeomKind::Polygon) => svg_polygon(out, rings, style, viewport),
        (Value::MultiPolygon(polys), GeomKind::Polygon) => {
            for rings in polys {
                svg_polygon(out, rings, style, viewport);
            }
        }
        (Value::LineString(line), GeomKind::Line) => svg_polyline(out, line, style, viewport),
        (Value::MultiLineString(lines), GeomKind::Line) => {
            for line in lines {
                svg_polyline(out, line, style, viewport);
            }
        }
        (Value::Point(pos), GeomKind::Point) => svg_point(out, pos, style, viewport),
        (Value::MultiPoint(pts), GeomKind::Point) => {
            for pos in pts {
                svg_point(out, pos, style, viewport);
            }
        }
        (Value::GeometryCollection(geoms), _) => {
            for g in geoms {
                svg_value(out, &g.value, kind, style, viewport);
            }
        }
        _ => {}
    }
}

fn svg_common(style: &GeometryStyle) -> String {
    let mut s = format!(
        "stroke=\"{}\" stroke-width=\"{}\"",
        style.stroke,
        fmt(style.stroke_width as f32)
    );
    if let Some(fill) = &style.fill {
        s.push_str(&format!(
            " fill=\"{fill}\" fill-opacity=\"{}\"",
            fmt(style.fill_opacity as f32)
        ));
    } else {
        s.push_str(" fill=\"none\"");
    }
    s
}

fn svg_polygon(
    out: &mut String,
    rings: &[Vec<Vec<f64>>],
    style: &GeometryStyle,
    viewport: &Viewport,
) {
    if rings.is_empty() {
        return;
    }
    let mut d = String::new();
    for ring in rings {
        for (i, p) in ring.iter().enumerate() {
            let (x, y) = viewport.project(p[0], p[1]);
            d.push_str(if i == 0 { "M" } else { "L" });
            d.push_str(&format!("{} {}", fmt(x), fmt(y)));
        }
        d.push('Z');
    }
    out.push_str(&format!(
        "<path d=\"{d}\" fill-rule=\"evenodd\" {} />\n",
        svg_common(style)
    ));
}

fn svg_polyline(out: &mut String, line: &[Vec<f64>], style: &GeometryStyle, viewport: &Viewport) {
    if line.len() < 2 {
        return;
    }
    let mut d = String::new();
    for (i, p) in line.iter().enumerate() {
        let (x, y) = viewport.project(p[0], p[1]);
        d.push_str(if i == 0 { "M" } else { "L" });
        d.push_str(&format!("{} {}", fmt(x), fmt(y)));
    }
    out.push_str(&format!(
        "<path d=\"{d}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" {} />\n",
        svg_common(style)
    ));
}

fn svg_point(out: &mut String, pos: &[f64], style: &GeometryStyle, viewport: &Viewport) {
    let (x, y) = viewport.project(pos[0], pos[1]);
    out.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" {} />\n",
        fmt(x),
        fmt(y),
        fmt(style.point_radius as f32),
        svg_common(style)
    ));
}

// ===== PNG 渲染（tiny-skia CPU 光栅化） =====

/// 渲染为 PNG 字节串（tiny-skia Pixmap 绘制 → encode_png）。
pub fn render_png(
    collection: &FeatureCollection,
    opts: &RenderOptions,
) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{Color, Pixmap};

    validate_options(opts)?;
    let viewport = Viewport::new(
        resolve_extent(collection, opts)?,
        opts.width,
        opts.height,
        opts.padding,
    )?;
    let mut pixmap = Pixmap::new(opts.width, opts.height)
        .ok_or(RenderError::InvalidSize(opts.width, opts.height))?;
    // 透明背景（none/transparent）：Pixmap 新建即全透明，跳过画布色填充
    if !transparent_bg(opts) {
        let (r, g, b) = parse_hex_color(
            &opts
                .background
                .clone()
                .unwrap_or_else(|| canvas_color(opts.theme).to_string()),
        )?;
        pixmap.fill(Color::from_rgba8(r, g, b, 255));
    }

    for kind in [GeomKind::Polygon, GeomKind::Line, GeomKind::Point] {
        for feature in &collection.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            let style = effective_style(kind, opts.theme, feature_color(opts, feature).as_deref());
            let stroke_rgb = parse_hex_color(&style.stroke)?;
            png_value(
                &mut pixmap,
                &geom.value,
                kind,
                &style,
                stroke_rgb,
                &viewport,
            );
        }
    }
    pixmap
        .encode_png()
        .map_err(|e| RenderError::Encode(e.to_string()))
}

/// 按类别光栅化单个几何。
fn png_value(
    pixmap: &mut tiny_skia::Pixmap,
    value: &Value,
    kind: GeomKind,
    style: &GeometryStyle,
    stroke_rgb: (u8, u8, u8),
    viewport: &Viewport,
) {
    use tiny_skia::{Color, Paint, PathBuilder, Stroke, Transform};

    let fill_paint = || {
        let mut p = Paint::default();
        if let Some(fill) = &style.fill {
            let (r, g, b) = parse_hex_color(fill.as_str()).unwrap_or((0, 0, 0));
            p.set_color(Color::from_rgba8(
                r,
                g,
                b,
                (style.fill_opacity * 255.0).round() as u8,
            ));
        }
        p
    };
    let stroke_paint = || {
        let mut p = Paint::default();
        p.set_color(Color::from_rgba8(
            stroke_rgb.0,
            stroke_rgb.1,
            stroke_rgb.2,
            255,
        ));
        p
    };
    let stroke = Stroke {
        width: style.stroke_width as f32,
        ..Default::default()
    };

    match (value, kind) {
        (Value::Polygon(rings), GeomKind::Polygon) => {
            if let Some(path) = build_polygon_path(rings, viewport) {
                pixmap.fill_path(
                    &path,
                    &fill_paint(),
                    tiny_skia::FillRule::EvenOdd,
                    Transform::default(),
                    None,
                );
                pixmap.stroke_path(&path, &stroke_paint(), &stroke, Transform::default(), None);
            }
        }
        (Value::MultiPolygon(polys), GeomKind::Polygon) => {
            for rings in polys {
                png_value(
                    pixmap,
                    &Value::Polygon(rings.clone()),
                    kind,
                    style,
                    stroke_rgb,
                    viewport,
                );
            }
        }
        (Value::LineString(line), GeomKind::Line) => {
            if let Some(path) = build_line_path(line, viewport) {
                pixmap.stroke_path(&path, &stroke_paint(), &stroke, Transform::default(), None);
            }
        }
        (Value::MultiLineString(lines), GeomKind::Line) => {
            for line in lines {
                png_value(
                    pixmap,
                    &Value::LineString(line.clone()),
                    kind,
                    style,
                    stroke_rgb,
                    viewport,
                );
            }
        }
        (Value::Point(pos), GeomKind::Point) => {
            let (x, y) = viewport.project(pos[0], pos[1]);
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, style.point_radius as f32);
            if let Some(path) = pb.finish() {
                pixmap.fill_path(
                    &path,
                    &fill_paint(),
                    tiny_skia::FillRule::Winding,
                    Transform::default(),
                    None,
                );
                pixmap.stroke_path(&path, &stroke_paint(), &stroke, Transform::default(), None);
            }
        }
        (Value::MultiPoint(pts), GeomKind::Point) => {
            for pos in pts {
                png_value(
                    pixmap,
                    &Value::Point(pos.clone()),
                    kind,
                    style,
                    stroke_rgb,
                    viewport,
                );
            }
        }
        (Value::GeometryCollection(geoms), _) => {
            for g in geoms {
                png_value(pixmap, &g.value, kind, style, stroke_rgb, viewport);
            }
        }
        _ => {}
    }
}

/// 多边形 → Path（外环 + 内环，EvenOdd 填充出洞）。
fn build_polygon_path(rings: &[Vec<Vec<f64>>], viewport: &Viewport) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for ring in rings {
        for (i, p) in ring.iter().enumerate() {
            let (x, y) = viewport.project(p[0], p[1]);
            if i == 0 {
                pb.move_to(x, y);
            } else {
                pb.line_to(x, y);
            }
        }
        pb.close();
    }
    pb.finish()
}

/// 折线 → Path。
fn build_line_path(line: &[Vec<f64>], viewport: &Viewport) -> Option<tiny_skia::Path> {
    if line.len() < 2 {
        return None;
    }
    let mut pb = tiny_skia::PathBuilder::new();
    for (i, p) in line.iter().enumerate() {
        let (x, y) = viewport.project(p[0], p[1]);
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection_from_str(text: &str) -> FeatureCollection {
        let gj: geojson::GeoJson = text.parse().unwrap();
        FeatureCollection::try_from(gj).unwrap()
    }

    fn mixed_collection() -> FeatureCollection {
        collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},"properties":{}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[116.39,39.90],[116.41,39.92]]},"properties":{}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[116.39,39.90],[116.39,39.92],[116.41,39.92],[116.41,39.90],[116.39,39.90]]]},"properties":{}}
            ]}"#,
        )
    }

    #[test]
    fn svg_contains_viewbox_background_and_elements() {
        let svg = render_svg(&mixed_collection(), &RenderOptions::default()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 800 600\""));
        assert!(svg.contains("#F0EDE8"), "晨山画布色: {svg}");
        assert!(svg.contains("features: 3"));
        assert!(svg.contains("<circle"), "点元素: {svg}");
        assert!(svg.contains("<path"), "线/面元素: {svg}");
        assert!(
            svg.contains("fill-rule=\"evenodd\""),
            "面（含洞规则）: {svg}"
        );
        assert!(svg.contains("#2D6A5E"), "远黛青: {svg}");
    }

    #[test]
    fn svg_dark_theme_uses_night_colors() {
        let opts = RenderOptions {
            theme: Theme::Dark,
            ..Default::default()
        };
        let svg = render_svg(&mixed_collection(), &opts).unwrap();
        assert!(svg.contains("#0D0F12"), "夜观星画布: {svg}");
        assert!(svg.contains("#E8E4DF"), "月白描边: {svg}");
        assert!(svg.contains("#4DB8A8"), "青玉填充: {svg}");
        assert!(svg.contains("theme: dark"));
    }

    #[test]
    fn png_has_magic_and_non_background_pixels() {
        let opts = RenderOptions {
            width: 200,
            height: 150,
            ..Default::default()
        };
        let png = render_png(&mixed_collection(), &opts).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG 魔数");
        // 像素层面：背景为 #F0EDE8，应存在非背景像素（要素可见）。
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        let (br, bg, bb) = (0xF0u8, 0xEDu8, 0xE8u8);
        let has_feature_pixel = pixmap.pixels().iter().any(|p| {
            let (r, g, b) = (p.red(), p.green(), p.blue());
            (r as i16 - br as i16).abs() > 8
                || (g as i16 - bg as i16).abs() > 8
                || (b as i16 - bb as i16).abs() > 8
        });
        assert!(has_feature_pixel, "PNG 应含非背景色要素像素");
    }

    #[test]
    fn png_empty_collection_renders_background_only() {
        let empty = collection_from_str(r#"{"type":"FeatureCollection","features":[]}"#);
        let png = render_png(&empty, &RenderOptions::default()).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(pixmap.pixels().iter().all(|p| {
            (p.red() as i16 - 0xF0).abs() <= 1
                && (p.green() as i16 - 0xED).abs() <= 1
                && (p.blue() as i16 - 0xE8).abs() <= 1
        }));
    }

    #[test]
    fn background_override_white_corners() {
        // 纯白背景覆盖（壳层地图框约束）：空集合渲染后四角像素 = 纯白。
        let empty = collection_from_str(r#"{"type":"FeatureCollection","features":[]}"#);
        let opts = RenderOptions {
            background: Some("#FFFFFF".to_string()),
            ..Default::default()
        };
        let png = render_png(&empty, &opts).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        let (w, h) = (pixmap.width(), pixmap.height());
        for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
            let p = pixmap.pixel(x, y).unwrap();
            assert_eq!(
                (p.red(), p.green(), p.blue(), p.alpha()),
                (255, 255, 255, 255),
                "角像素 ({x},{y}) 应为纯白"
            );
        }
        // SVG 同步覆盖。
        let svg = render_svg(&empty, &opts).unwrap();
        assert!(svg.contains("#FFFFFF"), "SVG 背景应纯白: {svg}");
    }

    #[test]
    fn transparent_background_png_corners_alpha_zero() {
        // 透明背景（底图叠加场景）：空集合渲染后四角像素 alpha=0（不铺画布色）。
        let empty = collection_from_str(r#"{"type":"FeatureCollection","features":[]}"#);
        for bg in ["none", "transparent", "NONE"] {
            let opts = RenderOptions {
                background: Some(bg.to_string()),
                ..Default::default()
            };
            let png = render_png(&empty, &opts).unwrap();
            let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
            let (w, h) = (pixmap.width(), pixmap.height());
            for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
                assert_eq!(
                    pixmap.pixel(x, y).unwrap().alpha(),
                    0,
                    "背景 {bg}：角像素 ({x},{y}) 应全透明"
                );
            }
            // SVG 同步：不输出背景 rect
            let svg = render_svg(&empty, &opts).unwrap();
            assert!(
                !svg.contains("<rect width=\"100%\""),
                "背景 {bg}：SVG 不应含背景 rect: {svg}"
            );
        }
    }

    #[test]
    fn transparent_background_still_renders_features() {
        // 透明背景下要素照常绘制（仅省去画布底色）。
        let opts = RenderOptions {
            background: Some("none".to_string()),
            ..Default::default()
        };
        let png = render_png(&mixed_collection(), &opts).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(
            pixmap.pixels().iter().any(|p| p.alpha() > 0),
            "透明背景下应存在要素像素"
        );
    }

    #[test]
    fn viewport_single_point_does_not_divide_by_zero() {
        let single = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},"properties":{}}
            ]}"#,
        );
        let svg = render_svg(&single, &RenderOptions::default()).unwrap();
        assert!(svg.contains("<circle"), "单点应正常渲染: {svg}");
    }

    #[test]
    fn viewport_preserves_aspect_ratio() {
        // 宽扁数据（span_x=10, span_y=1）：等比缩放 → 有效高度应远小于画布，
        // 即数据在 y 方向居中而非拉伸满屏。
        let wide = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[10,1]]},"properties":{}}
            ]}"#,
        );
        let viewport = Viewport::new(compute_extent(&wide).unwrap(), 800, 600, 20.0).unwrap();
        // 等比：scale 由宽决定（760/10=76 < 560/1=560），y 方向有居中偏移。
        assert!((viewport.scale - 76.0).abs() < 1e-9);
        assert!(
            viewport.offset_y > 200.0,
            "宽扁数据应垂直居中: {:?}",
            viewport
        );
    }

    #[test]
    fn collection_extent_returns_bbox_array() {
        let ext = collection_extent(&mixed_collection()).unwrap().unwrap();
        assert_eq!(ext, [116.39, 39.90, 116.41, 39.92]);
        let empty = collection_from_str(r#"{"type":"FeatureCollection","features":[]}"#);
        assert!(collection_extent(&empty).unwrap().is_none());
    }

    #[test]
    fn explicit_viewport_shifts_content_off_canvas() {
        // 数据在北京，显式视口放在 [0,0]-[1,1]：要素全部落在画布外，
        // PNG 应只剩背景色（证明视口参数真实生效而非走自动适配）。
        let opts = RenderOptions {
            viewport: Some([0.0, 0.0, 1.0, 1.0]),
            ..Default::default()
        };
        let png = render_png(&mixed_collection(), &opts).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(pixmap.pixels().iter().all(|p| {
            (p.red() as i16 - 0xF0).abs() <= 1
                && (p.green() as i16 - 0xED).abs() <= 1
                && (p.blue() as i16 - 0xE8).abs() <= 1
        }));
    }

    #[test]
    fn explicit_viewport_rejects_inverted_or_nan() {
        let opts = RenderOptions {
            viewport: Some([2.0, 0.0, 1.0, 1.0]),
            ..Default::default()
        };
        let err = render_svg(&mixed_collection(), &opts).unwrap_err();
        assert!(err.to_string().contains("最小值大于最大值"), "{err}");
        let opts = RenderOptions {
            viewport: Some([0.0, f64::NAN, 1.0, 1.0]),
            ..Default::default()
        };
        let err = render_svg(&mixed_collection(), &opts).unwrap_err();
        assert!(err.to_string().contains("非有限坐标"), "{err}");
    }

    #[test]
    fn extent_rejects_nan_coordinates() {
        // geojson 解析层拒绝文本态 null/NaN 坐标，故用代码构造 NaN 验证
        // 渲染层的非有限坐标防护。
        let mut bad = mixed_collection();
        bad.features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(Value::Point(vec![f64::NAN, 0.0]))),
            id: None,
            properties: None,
            foreign_members: None,
        });
        let err = render_svg(&bad, &RenderOptions::default()).unwrap_err();
        assert!(
            err.to_string().contains("非有限坐标"),
            "应报非有限坐标错误: {err}"
        );
    }

    fn height_graduated_rule() -> StyleRule {
        serde_json::from_str(
            r##"{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}"##,
        )
        .unwrap()
    }

    fn props(pairs: &[(&str, serde_json::Value)]) -> serde_json::Map<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn style_rule_parses_and_validates() {
        // 合法 graduated / categorical。
        let g = height_graduated_rule();
        assert!(g.validate().is_ok());
        let c: StyleRule = serde_json::from_str(
            r##"{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E"},"default":"#888888"}"##,
        )
        .unwrap();
        assert!(c.validate().is_ok());

        // 坏 hex（指出出错档）。
        let bad_hex: StyleRule =
            serde_json::from_str(r##"{"type":"graduated","field":"h","stops":[[0,"red"]]}"##)
                .unwrap();
        let err = bad_hex.validate().unwrap_err();
        assert!(err.to_string().contains("第 1 档颜色非法"), "{err}");

        // 空 stops。
        let empty: StyleRule =
            serde_json::from_str(r#"{"type":"graduated","field":"h","stops":[]}"#).unwrap();
        assert!(empty
            .validate()
            .unwrap_err()
            .to_string()
            .contains("不能为空"));

        // 非升序 stops（指出出错档与值）。
        let unordered: StyleRule = serde_json::from_str(
            r##"{"type":"graduated","field":"h","stops":[[50,"#2D6A5E"],[50,"#D4A843"]]}"##,
        )
        .unwrap();
        let err = unordered.validate().unwrap_err();
        assert!(err.to_string().contains("严格升序"), "{err}");
    }

    #[test]
    fn graduated_color_for_boundaries() {
        let rule = height_graduated_rule();
        // 恰等阈值 → 该档；恰等上一档 → 上一档。
        assert_eq!(
            rule.color_for(&props(&[("height", serde_json::Value::from(50))])),
            Some("#D4A843".to_string())
        );
        assert_eq!(
            rule.color_for(&props(&[("height", serde_json::Value::from(0))])),
            Some("#2D6A5E".to_string())
        );
        // 超最大档 → 最后一档。
        assert_eq!(
            rule.color_for(&props(&[("height", serde_json::Value::from(999))])),
            Some("#C75B3A".to_string())
        );
        // 低于首档 → None（走默认样式）。
        assert_eq!(
            rule.color_for(&props(&[("height", serde_json::Value::from(-1))])),
            None
        );
        // 缺失字段 / 非数值字段 → None。
        assert_eq!(rule.color_for(&props(&[])), None);
        assert_eq!(
            rule.color_for(&props(&[("height", serde_json::Value::from("abc"))])),
            None
        );
    }

    #[test]
    fn categorical_color_for_hit_default_miss() {
        let rule: StyleRule = serde_json::from_str(
            r##"{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E"},"default":"#888888"}"##,
        )
        .unwrap();
        assert_eq!(
            rule.color_for(&props(&[("usage", serde_json::Value::from("office"))])),
            Some("#2D6A5E".to_string())
        );
        // 无匹配 → default。
        assert_eq!(
            rule.color_for(&props(&[("usage", serde_json::Value::from("park"))])),
            Some("#888888".to_string())
        );
        // 缺失字段 → None。
        assert_eq!(rule.color_for(&props(&[])), None);
        // 无 default 且无匹配 → None。
        let no_default: StyleRule = serde_json::from_str(
            r##"{"type":"categorical","field":"usage","colors":{"office":"#2D6A5E"}}"##,
        )
        .unwrap();
        assert_eq!(
            no_default.color_for(&props(&[("usage", serde_json::Value::from("park"))])),
            None
        );
    }

    #[test]
    fn svg_graduated_renders_distinct_fills() {
        let collection = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[0,2],[2,2],[2,0],[0,0]]]},
                 "properties":{"height":30}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[3,0],[3,2],[5,2],[5,0],[3,0]]]},
                 "properties":{"height":120}}
            ]}"#,
        );
        let opts = RenderOptions {
            style: Some(height_graduated_rule()),
            ..Default::default()
        };
        let svg = render_svg(&collection, &opts).unwrap();
        assert!(svg.contains("#2D6A5E"), "30m 应低档色: {svg}");
        assert!(svg.contains("#C75B3A"), "120m 应高档色: {svg}");
        assert!(!svg.contains("fill=\"#D4A843\""), "无中档要素: {svg}");
    }

    #[test]
    fn png_graduated_renders_distinct_colors() {
        let collection = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[0,2],[2,2],[2,0],[0,0]]]},
                 "properties":{"height":10}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[3,0],[3,2],[5,2],[5,0],[3,0]]]},
                 "properties":{"height":120}}
            ]}"#,
        );
        let opts = RenderOptions {
            width: 200,
            height: 100,
            padding: 5.0,
            style: Some(height_graduated_rule()),
            ..Default::default()
        };
        let png = render_png(&collection, &opts).unwrap();
        let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
        // 两多边形中心采样（x 各 1/3、2/3 附近，y 居中）：颜色应分别为
        // 低档 #2D6A5E（20% 透明叠底）与高档 #C75B3A 的混合色，且互不相同。
        let at = |fx: f32| {
            let x = (200.0 * fx) as u32;
            let p = pixmap.pixel(x, 50).unwrap();
            (p.red(), p.green(), p.blue())
        };
        let left = at(0.25);
        let right = at(0.75);
        assert_ne!(left, right, "两档要素应异色: left={left:?} right={right:?}");
        // 高档色偏红（R 明显大于 G；20% 透明叠底后差值仍显著）。
        assert!(right.0 > right.1 + 15, "右侧应偏红(#C75B3A 系): {right:?}");
        // 低档色偏青绿（G 最大）。
        assert!(
            left.1 >= left.0 && left.1 >= left.2,
            "左侧应偏青绿(#2D6A5E 系): {left:?}"
        );
    }
}
