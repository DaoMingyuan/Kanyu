//! 南方 CASS 联动 —— 坐标数据文件（.dat）读写 + CASS 兼容 DXF 导出。
//!
//! 移植/对齐堪舆工具箱（KanyuTools QGIS 插件）：
//! - `features/dat_tools.py`（.dat 读取）——本模块补齐写出与编码列保留；
//! - `features/cad_conversion/cass_profile.json`（图层/编码/线型约定，单一事实来源）；
//! - `features/realestate_map/exporters.py`（不动产 CASS DXF 直写语义）。
//!
//! ## CASS .dat 坐标数据文件（v1.0）
//!
//! ```text
//! 点号,编码,Y,X,H
//! J1,302001,39595462.533,4127300.446,0.000
//! ```
//!
//! - **轴序为 CASS 标准：Y=东（横坐标）在前、X=北（纵坐标）在后**（H 高程可空）；
//! - 编码列保留（CASS 地物编码，如界址点 302001），空编码允许；
//! - GeoJSON 位置取 `[Y(东), X(北)]`（与本仓库 parcel.rs 测绘惯例一致）；
//! - 兼容 UTF-8 BOM 与 `#` 注释行。
//!
//! ## CASS 兼容 DXF
//!
//! 宗地成果按 CASS 成图习惯分层：`ZD`（宗地面）、`JZX`（界址线，ACI 1，编码 302002）、
//! `JZD`（界址点 Ø2.0mm CIRCLE + 点号 TEXT，ACI 1，编码 302001）、`ZJ`（注记，ACI 7）。
//! 编码以 XDATA 挂载（组码 `1001=SOUTH` + `1000=<编码>`），APPID 表登记 SOUTH。
//! 纸面毫米要素（字号 2.4mm、圆圈 Ø2.0mm）按出图比例尺换算为模型单位：
//! `mu = mm × scale / 1000`。

use crate::cartography::{
    place_edge_labels, place_point_labels, text_extent_mm, BoundaryLineRecord, BoundaryPointRecord,
    EdgeLabelOptions, ParcelBoundary, Point2, PointLabelOptions,
};
use crate::error::{KanyuError, Result};

/// CASS .dat 坐标点。
#[derive(Debug, Clone, PartialEq)]
pub struct CassDatPoint {
    /// 点号。
    pub name: String,
    /// CASS 地物编码（可空串）。
    pub code: String,
    /// 东坐标（Y，横坐标）。
    pub east: f64,
    /// 北坐标（X，纵坐标）。
    pub north: f64,
    /// 高程（可空）。
    pub h: Option<f64>,
}

/// 是否 CASS .dat 文本（至少一行 `点号,编码,Y,X[,H]` 形态）。
pub fn is_cass_dat(text: &str) -> bool {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .any(|line| parse_dat_line_lenient(line).is_some())
}

/// 单行宽松解析（探测用；带行号的详细错误见 [`parse_cass_dat`]）。
fn parse_dat_line_lenient(line: &str) -> Option<CassDatPoint> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() < 4 {
        return None;
    }
    let east = parts[2].parse::<f64>().ok()?;
    let north = parts[3].parse::<f64>().ok()?;
    let h = match parts.get(4) {
        Some(raw) if !raw.is_empty() => Some(raw.parse::<f64>().ok()?),
        _ => None,
    };
    Some(CassDatPoint {
        name: parts[0].to_string(),
        code: parts[1].to_string(),
        east,
        north,
        h,
    })
}

/// 解析 CASS .dat（轴序：点号,编码,Y东,X北[,H]；`#` 注释行；中文错误带行号）。
pub fn parse_cass_dat(text: &str) -> Result<Vec<CassDatPoint>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut points = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
        if parts.len() < 4 {
            return Err(KanyuError::Other(format!(
                "CASS .dat 第 {line_no} 行：列数不足（需 点号,编码,Y,X[,H]），实际 {} 列",
                parts.len()
            )));
        }
        let east = parts[2].parse::<f64>().map_err(|_| {
            KanyuError::Other(format!(
                "CASS .dat 第 {line_no} 行：Y(东)坐标不是数值: '{}'",
                parts[2]
            ))
        })?;
        let north = parts[3].parse::<f64>().map_err(|_| {
            KanyuError::Other(format!(
                "CASS .dat 第 {line_no} 行：X(北)坐标不是数值: '{}'",
                parts[3]
            ))
        })?;
        let h = match parts.get(4) {
            Some(raw) if !raw.is_empty() => Some(raw.parse::<f64>().map_err(|_| {
                KanyuError::Other(format!(
                    "CASS .dat 第 {line_no} 行：H 高程不是数值: '{raw}'"
                ))
            })?),
            _ => None,
        };
        points.push(CassDatPoint {
            name: parts[0].to_string(),
            code: parts[1].to_string(),
            east,
            north,
            h,
        });
    }
    if points.is_empty() {
        return Err(KanyuError::Other(
            "CASS .dat 没有任何有效数据行（需 点号,编码,Y,X[,H]）".to_string(),
        ));
    }
    Ok(points)
}

/// CASS 点列 → FeatureCollection：Point 要素，属性 `name`/`code`/`h`（可空跳过）；
/// GeoJSON 位置 = `[east, north]`。
pub fn cass_points_to_collection(points: &[CassDatPoint]) -> geojson::FeatureCollection {
    let features = points
        .iter()
        .map(|p| {
            let mut properties = serde_json::Map::new();
            properties.insert(
                "name".to_string(),
                serde_json::Value::String(p.name.clone()),
            );
            properties.insert(
                "code".to_string(),
                serde_json::Value::String(p.code.clone()),
            );
            if let Some(h) = p.h {
                properties.insert("h".to_string(), serde_json::Value::from(h));
            }
            geojson::Feature {
                bbox: None,
                geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![
                    p.east, p.north,
                ]))),
                id: None,
                properties: Some(properties),
                foreign_members: None,
            }
        })
        .collect();
    geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }
}

/// FeatureCollection（点要素）→ CASS .dat 文本：
/// 点号取属性 `name`（缺省 J{n} 顺编），编码取 `code`（可空），高程取 `h`/`z`（可空）。
pub fn collection_to_cass_dat(
    collection: &geojson::FeatureCollection,
    decimals: usize,
) -> Result<String> {
    let mut out = String::new();
    // 输出序号（仅点要素参与顺编，非点要素跳过）。
    let mut seq = 0usize;
    for feature in &collection.features {
        let pos = match feature.geometry.as_ref().map(|g| &g.value) {
            Some(geojson::Value::Point(pos)) => pos,
            _ => continue,
        };
        seq += 1;
        let props = feature.properties.as_ref();
        let str_prop = |key: &str| {
            props
                .and_then(|p| p.get(key))
                .and_then(|v| v.as_str())
                .unwrap_or("")
        };
        let raw_name = str_prop("name");
        let name = if raw_name.is_empty() {
            format!("J{seq}")
        } else {
            raw_name.to_string()
        };
        let code = str_prop("code");
        let h = props
            .and_then(|p| p.get("h").or_else(|| p.get("z")))
            .and_then(|v| v.as_f64());
        let east = pos.first().copied().unwrap_or(0.0);
        let north = pos.get(1).copied().unwrap_or(0.0);
        out.push_str(&format!(
            "{name},{code},{east:.decimals$},{north:.decimals$}"
        ));
        if let Some(h) = h {
            out.push_str(&format!(",{h:.decimals$}"));
        }
        out.push('\n');
    }
    if seq == 0 {
        return Err(KanyuError::Other(
            "dat 导出失败：集合中没有点要素（CASS .dat 仅承载 Point 几何）".to_string(),
        ));
    }
    Ok(out)
}

/// CASS 兼容 DXF 导出参数。
#[derive(Debug, Clone)]
pub struct CassDxfSpec {
    /// 出图比例尺分母（纸面毫米要素换算模型单位；默认 1000）。
    pub scale: u32,
    /// 宗地代码（宗地号分式分子取末 7 位）。
    pub parcel_code: String,
    /// 地类编码（分式分母）。
    pub land_use: String,
    /// 土地权利人（ZJ 注记）。
    pub owner: String,
    /// 是否挂载 SOUTH XDATA 编码（默认 true）。
    pub xdata: bool,
}

impl Default for CassDxfSpec {
    fn default() -> Self {
        Self {
            scale: 1000,
            parcel_code: String::new(),
            land_use: String::new(),
            owner: String::new(),
            xdata: true,
        }
    }
}

/// 宗地成果 → CASS 兼容 DXF 文本。
///
/// **版本与编码选择**：写 `$ACADVER=AC1024`（R2010）+ UTF-8 字节，对齐 exporters.py
/// 的 ezdxf R2010 产物（R2007 起 DXF 文本按 UTF-8 编码）；`$DWGCODEPAGE` 仍声明
/// ANSI_936 作 GBK 语义提示。不走 `dxf` crate 写出（其 XDATA 支持不确定），
/// 按组码流自包含直写（CRLF 行尾、子类标记齐全，可用 `dxf` crate / ezdxf 回读）。
///
/// 分层与实体（对齐 cass_profile.json / exporters.py 语义）：
/// - `ZD`：宗地外环闭合 LWPOLYLINE；
/// - `JZX`：逐边 LWPOLYLINE（两点）+ 边长 TEXT（中点法线外移、角度沿线），编码 302002；
/// - `JZD`：界址点 CIRCLE（半径 1.0mm 换算）+ 点号 TEXT（角平分线朝外），编码 302001；
/// - `ZJ`：宗地号/地类分式（三 TEXT + 分数线，宗地中央）与权利人注记。
///   注记位置经 [`crate::cartography::place_edge_labels`] / [`crate::cartography::place_point_labels`]
///   计算（勘测定界图注记契约）。
pub fn parcel_to_cass_dxf(
    boundary: &ParcelBoundary,
    points: &[BoundaryPointRecord],
    lines: &[BoundaryLineRecord],
    spec: &CassDxfSpec,
) -> Result<String> {
    // 比例尺分母 0 无意义，按 1 兜底（防零尺寸）。
    let scale = spec.scale.max(1);
    // 纸面毫米 → 模型单位：mu = mm × scale / 1000。
    let mu = |mm: f64| mm * f64::from(scale) / 1000.0;

    // 注记排版（勘测定界图注记契约）：先点号，点号矩形作为边长注记的附加障碍。
    let point_labels = place_point_labels(
        boundary,
        points,
        &PointLabelOptions {
            scale,
            ..Default::default()
        },
        &[],
    );
    let point_rects: Vec<_> = point_labels.labels.iter().map(|l| l.rect).collect();
    // 仅环索引有效的边参与排版与写出（place_edge_labels 同款过滤）。
    let valid_lines: Vec<BoundaryLineRecord> = lines
        .iter()
        .filter(|l| l.ring_index < boundary.rings().len())
        .cloned()
        .collect();
    let edge_labels = place_edge_labels(
        boundary,
        &valid_lines,
        points,
        &EdgeLabelOptions {
            scale,
            ..Default::default()
        },
        &point_rects,
    );

    let mut buf = String::new();
    // HEADER 段：最小化（版本 + 代码页）。
    dxf_pair(&mut buf, 0, "SECTION");
    dxf_pair(&mut buf, 2, "HEADER");
    dxf_pair(&mut buf, 9, "$ACADVER");
    dxf_pair(&mut buf, 1, "AC1024");
    dxf_pair(&mut buf, 9, "$DWGCODEPAGE");
    dxf_pair(&mut buf, 3, "ANSI_936");
    dxf_pair(&mut buf, 0, "ENDSEC");
    // TABLES 段：LAYER 四层 + SOUTH APPID（挂 XDATA 时登记）。
    dxf_pair(&mut buf, 0, "SECTION");
    dxf_pair(&mut buf, 2, "TABLES");
    dxf_pair(&mut buf, 0, "TABLE");
    dxf_pair(&mut buf, 2, "LAYER");
    dxf_pair(&mut buf, 70, "4");
    for (name, aci) in [("ZD", 7), ("JZX", 1), ("JZD", 1), ("ZJ", 7)] {
        dxf_pair(&mut buf, 0, "LAYER");
        dxf_pair(&mut buf, 2, name);
        dxf_pair(&mut buf, 70, "0");
        dxf_pair(&mut buf, 62, &aci.to_string());
        dxf_pair(&mut buf, 6, "CONTINUOUS");
    }
    dxf_pair(&mut buf, 0, "ENDTAB");
    if spec.xdata {
        dxf_pair(&mut buf, 0, "TABLE");
        dxf_pair(&mut buf, 2, "APPID");
        dxf_pair(&mut buf, 70, "1");
        dxf_pair(&mut buf, 0, "APPID");
        dxf_pair(&mut buf, 2, "SOUTH");
        dxf_pair(&mut buf, 70, "0");
        dxf_pair(&mut buf, 0, "ENDTAB");
    }
    dxf_pair(&mut buf, 0, "ENDSEC");

    // ENTITIES 段。
    dxf_pair(&mut buf, 0, "SECTION");
    dxf_pair(&mut buf, 2, "ENTITIES");
    let south = |code: &'static str| spec.xdata.then_some(code);

    // ZD：宗地外环闭合 LWPOLYLINE（70 bit1 闭合；重复闭合点不写出）。
    let exterior: &[Point2] = if boundary.exterior.points.len() > 1 {
        &boundary.exterior.points[..boundary.exterior.points.len() - 1]
    } else {
        &boundary.exterior.points
    };
    dxf_push_lwpolyline(&mut buf, "ZD", exterior, true, None);

    // JZX：逐边两顶点 LWPOLYLINE（编码 302002）+ 边长 TEXT（位置/角度取排版结果）。
    let coord_of = |no: usize| points.iter().find(|p| p.point_no == no).map(|p| (p.x, p.y));
    for (line, label) in valid_lines.iter().zip(&edge_labels.labels) {
        let (Some(a), Some(b)) = (coord_of(line.start_no), coord_of(line.end_no)) else {
            continue;
        };
        dxf_push_lwpolyline(&mut buf, "JZX", &[a, b], false, south("302002"));
        // PlacedLabel.rotation_deg 顺时针为正；DXF TEXT 50 组码为数学角逆时针为正，取负。
        dxf_push_text(
            &mut buf,
            "JZX",
            (label.rect.cx, label.rect.cy),
            mu(2.4),
            -label.rotation_deg,
            &label.text,
            None,
        );
    }

    // JZD：界址点 CIRCLE（Ø2.0mm 换算半径）+ 点号 TEXT（水平）；两者同挂 302001。
    let ring_count = boundary.rings().len();
    let ordered_points: Vec<&BoundaryPointRecord> = (0..ring_count)
        .flat_map(|ri| points.iter().filter(move |p| p.ring_index == ri))
        .collect();
    for (rec, label) in ordered_points.iter().zip(&point_labels.labels) {
        dxf_push_circle(&mut buf, "JZD", (rec.x, rec.y), mu(1.0), south("302001"));
        dxf_push_text(
            &mut buf,
            "JZD",
            (label.rect.cx, label.rect.cy),
            mu(2.4),
            0.0,
            &rec.label(),
            south("302001"),
        );
    }

    // ZJ：宗地号/地类分式（分子=宗地代码末 7 位）+ 分数线 + 权利人注记，宗地中央。
    let pcode = spec.parcel_code.trim();
    let pcode_chars: Vec<char> = pcode.chars().collect();
    let fz: String = pcode_chars[pcode_chars.len().saturating_sub(7)..]
        .iter()
        .collect();
    let fm = spec.land_use.trim();
    let owner = spec.owner.trim();
    if !fz.is_empty() || !fm.is_empty() || !owner.is_empty() {
        let (cx, cy) = ring_centroid(&boundary.exterior.points);
        let height = mu(3.0);
        // 左下对齐下按估算字宽水平居中（对齐 exporters.py MIDDLE_CENTER 观感）。
        let centered_x = |text: &str| cx - mu(text_extent_mm(text, 3.0).0) / 2.0;
        if !fz.is_empty() {
            // 分子中线在中心上 2.0mm → 基线 = cy + 0.5mm（字高一半 1.5mm）。
            dxf_push_text(
                &mut buf,
                "ZJ",
                (centered_x(&fz), cy + mu(0.5)),
                height,
                0.0,
                &fz,
                None,
            );
        }
        if !fm.is_empty() {
            // 分母中线在中心下 2.0mm → 基线 = cy - 3.5mm。
            dxf_push_text(
                &mut buf,
                "ZJ",
                (centered_x(fm), cy - mu(3.5)),
                height,
                0.0,
                fm,
                None,
            );
        }
        if !fz.is_empty() && !fm.is_empty() {
            let half_w = mu(fz.chars().count().max(fm.chars().count()) as f64 * 3.0 / 2.0);
            dxf_push_line(&mut buf, "ZJ", (cx - half_w, cy), (cx + half_w, cy));
        }
        if !owner.is_empty() {
            // 权利人注记置于分式之下（行距 4.0mm）；无分式时居中于宗地中央。
            let base_y = if !fz.is_empty() || !fm.is_empty() {
                cy - mu(7.5)
            } else {
                cy
            };
            dxf_push_text(
                &mut buf,
                "ZJ",
                (centered_x(owner), base_y),
                height,
                0.0,
                owner,
                None,
            );
        }
    }

    dxf_pair(&mut buf, 0, "ENDSEC");
    dxf_pair(&mut buf, 0, "EOF");
    Ok(buf)
}

/// 组码写出（码右对齐 3 位 + CRLF 行尾，与 ezdxf 产物观感一致）。
fn dxf_pair(buf: &mut String, code: i32, value: &str) {
    buf.push_str(&format!("{code:>3}\r\n{value}\r\n"));
}

/// 浮点组码值：保留 10 位小数后去尾零（避免浮点尾噪，坐标精度远超毫米级）。
fn fmt_num(v: f64) -> String {
    let mut s = format!("{v:.10}");
    while s.ends_with('0') && s.contains('.') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

/// 浮点组码写出。
fn dxf_num(buf: &mut String, code: i32, v: f64) {
    dxf_pair(buf, code, &fmt_num(v));
}

/// SOUTH 实体编码 XDATA（`1001=SOUTH` + `1000=<编码>`，置于实体全部组码之后）。
fn dxf_push_xdata(buf: &mut String, code: &str) {
    dxf_pair(buf, 1001, "SOUTH");
    dxf_pair(buf, 1000, code);
}

/// LWPOLYLINE 实体（`closed` 置 70 bit1 闭合标志）。
fn dxf_push_lwpolyline(
    buf: &mut String,
    layer: &str,
    pts: &[Point2],
    closed: bool,
    xdata: Option<&str>,
) {
    dxf_pair(buf, 0, "LWPOLYLINE");
    dxf_pair(buf, 100, "AcDbEntity");
    dxf_pair(buf, 8, layer);
    dxf_pair(buf, 100, "AcDbPolyline");
    dxf_pair(buf, 90, &pts.len().to_string());
    dxf_pair(buf, 70, if closed { "1" } else { "0" });
    for &(x, y) in pts {
        dxf_num(buf, 10, x);
        dxf_num(buf, 20, y);
    }
    if let Some(code) = xdata {
        dxf_push_xdata(buf, code);
    }
}

/// CIRCLE 实体（界址点符号）。
fn dxf_push_circle(
    buf: &mut String,
    layer: &str,
    center: Point2,
    radius: f64,
    xdata: Option<&str>,
) {
    dxf_pair(buf, 0, "CIRCLE");
    dxf_pair(buf, 100, "AcDbEntity");
    dxf_pair(buf, 8, layer);
    dxf_pair(buf, 100, "AcDbCircle");
    dxf_num(buf, 10, center.0);
    dxf_num(buf, 20, center.1);
    dxf_num(buf, 40, radius);
    if let Some(code) = xdata {
        dxf_push_xdata(buf, code);
    }
}

/// TEXT 实体（默认左下对齐，72/73 省略；`rotation_deg` 为数学角、逆时针为正，
/// 为 0 时省略 50 组码）。
fn dxf_push_text(
    buf: &mut String,
    layer: &str,
    at: Point2,
    height: f64,
    rotation_deg: f64,
    text: &str,
    xdata: Option<&str>,
) {
    dxf_pair(buf, 0, "TEXT");
    dxf_pair(buf, 100, "AcDbEntity");
    dxf_pair(buf, 8, layer);
    dxf_pair(buf, 100, "AcDbText");
    dxf_num(buf, 10, at.0);
    dxf_num(buf, 20, at.1);
    dxf_num(buf, 40, height);
    dxf_pair(buf, 1, text);
    if rotation_deg != 0.0 {
        dxf_num(buf, 50, rotation_deg);
    }
    if let Some(code) = xdata {
        dxf_push_xdata(buf, code);
    }
}

/// LINE 实体（分式分数线）。
fn dxf_push_line(buf: &mut String, layer: &str, a: Point2, b: Point2) {
    dxf_pair(buf, 0, "LINE");
    dxf_pair(buf, 100, "AcDbEntity");
    dxf_pair(buf, 8, layer);
    dxf_pair(buf, 100, "AcDbLine");
    dxf_num(buf, 10, a.0);
    dxf_num(buf, 20, a.1);
    dxf_num(buf, 11, b.0);
    dxf_num(buf, 21, b.1);
}

/// 外环面积质心（鞋带公式；退化环回退顶点均值）。
fn ring_centroid(pts: &[Point2]) -> Point2 {
    let mut cross_sum = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for w in pts.windows(2) {
        let cross = w[0].0 * w[1].1 - w[1].0 * w[0].1;
        cross_sum += cross;
        cx += (w[0].0 + w[1].0) * cross;
        cy += (w[0].1 + w[1].1) * cross;
    }
    if cross_sum.abs() < 1e-12 {
        let n = pts.len().max(1) as f64;
        return (
            pts.iter().map(|p| p.0).sum::<f64>() / n,
            pts.iter().map(|p| p.1).sum::<f64>() / n,
        );
    }
    (cx / (3.0 * cross_sum), cy / (3.0 * cross_sum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartography::{generate_boundary_lines, generate_boundary_points};

    /// 矩形宗地夹具：10×20 外环（CCW）→ 4 界址点 / 4 界址线。
    fn rect_fixture() -> (
        ParcelBoundary,
        Vec<BoundaryPointRecord>,
        Vec<BoundaryLineRecord>,
    ) {
        let geom = geojson::Value::Polygon(vec![vec![
            vec![0.0, 0.0],
            vec![10.0, 0.0],
            vec![10.0, 20.0],
            vec![0.0, 20.0],
            vec![0.0, 0.0],
        ]]);
        let boundary = ParcelBoundary::from_geometry(&geom).unwrap();
        let points = generate_boundary_points(&boundary, "J");
        let lines = generate_boundary_lines(&boundary, &points);
        (boundary, points, lines)
    }

    #[test]
    fn is_cass_dat_detects_shape() {
        assert!(is_cass_dat("J1,302001,39595462.533,4127300.446,0.000"));
        // BOM + 注释 + 无 H 列
        assert!(is_cass_dat("\u{feff}# 表头\nJ1,,100.5,200.25"));
        // 列数不足、坐标非数值、纯注释均不算
        assert!(!is_cass_dat("x,y\n1,2"));
        assert!(!is_cass_dat("J1,302001,abc,123"));
        assert!(!is_cass_dat("# 只有注释\n"));
    }

    #[test]
    fn parse_axis_order_and_optional_columns() {
        // 轴序契约：第 3 列 = Y(东)、第 4 列 = X(北)；BOM/注释/空编码/无 H 列。
        let text = "\u{feff}# 界址点成果\r\nJ1,302001,39595462.533,4127300.446,0.000\r\nJ2,,100.5,200.25\r\n";
        let pts = parse_cass_dat(text).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].name, "J1");
        assert_eq!(pts[0].code, "302001");
        assert_eq!(pts[0].east, 39595462.533);
        assert_eq!(pts[0].north, 4127300.446);
        assert_eq!(pts[0].h, Some(0.0));
        // 空编码允许、H 列缺省为 None。
        assert_eq!(pts[1].code, "");
        assert_eq!(pts[1].h, None);
    }

    #[test]
    fn parse_error_reports_chinese_line_number() {
        // 注释行计入物理行号：坏行在第 3 行。
        let err = parse_cass_dat("J1,,1,2\n# 注释\nJ2,,abc,3\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("第 3 行"), "{err}");
        assert!(err.contains("Y(东)"), "{err}");
        // 列数不足。
        let err = parse_cass_dat("J1,,100").unwrap_err().to_string();
        assert!(err.contains("第 1 行") && err.contains("列数不足"), "{err}");
        // H 非数值。
        let err = parse_cass_dat("J1,,1,2,高").unwrap_err().to_string();
        assert!(err.contains("H 高程"), "{err}");
    }

    #[test]
    fn collection_roundtrip_preserves_semantics() {
        let pts = vec![
            CassDatPoint {
                name: "J1".to_string(),
                code: "302001".to_string(),
                east: 100.125,
                north: 200.25,
                h: Some(5.5),
            },
            CassDatPoint {
                name: "J2".to_string(),
                code: String::new(),
                east: 101.125,
                north: 201.25,
                h: None,
            },
        ];
        let collection = cass_points_to_collection(&pts);
        // GeoJSON 位置 = [east, north]；属性 name/code/h（h 可空跳过）。
        let f0 = &collection.features[0];
        let geojson::Value::Point(pos) = &f0.geometry.as_ref().unwrap().value else {
            panic!("应为 Point 几何");
        };
        assert_eq!(pos, &vec![100.125, 200.25]);
        let props = f0.properties.as_ref().unwrap();
        assert_eq!(props["name"], serde_json::Value::from("J1"));
        assert_eq!(props["code"], serde_json::Value::from("302001"));
        assert_eq!(props["h"].as_f64(), Some(5.5));
        // 无 H 的点不带 h 属性。
        let f1_props = collection.features[1].properties.as_ref().unwrap();
        assert!(!f1_props.contains_key("h"));
        // collection → .dat → 点列：语义逐字段往返一致。
        let text = collection_to_cass_dat(&collection, 3).unwrap();
        let back = parse_cass_dat(&text).unwrap();
        assert_eq!(back, pts);
    }

    #[test]
    fn collection_to_cass_dat_defaults_and_decimals() {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[1.0,2.0]},
                 "properties":{"code":"302001","z":5.5}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[3.0,4.0]},
                 "properties":{"name":"A9","h":0.25}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{}}
            ]
        }"#
        .parse()
        .unwrap();
        let collection = geojson::FeatureCollection::try_from(gj).unwrap();
        let text = collection_to_cass_dat(&collection, 2).unwrap();
        let rows: Vec<&str> = text.lines().collect();
        // 非点要素跳过；缺省点号 J{n} 顺编（只对输出行编号）。
        assert_eq!(rows.len(), 2);
        // z 属性作高程回退；decimals 生效。
        assert_eq!(rows[0], "J1,302001,1.00,2.00,5.50");
        // name 属性拾取；无编码 → 空列；h 属性拾取。
        assert_eq!(rows[1], "A9,,3.00,4.00,0.25");
        // 无点要素 → 中文错误。
        let empty = geojson::FeatureCollection {
            bbox: None,
            features: vec![],
            foreign_members: None,
        };
        assert!(collection_to_cass_dat(&empty, 3).is_err());
    }

    #[test]
    fn cass_dxf_layers_xdata_and_entity_counts() {
        let (boundary, points, lines) = rect_fixture();
        let spec = CassDxfSpec {
            scale: 1000,
            parcel_code: "430103002007GB00025".to_string(),
            land_use: "0701".to_string(),
            owner: "张三".to_string(),
            xdata: true,
        };
        let dxf_text = parcel_to_cass_dxf(&boundary, &points, &lines, &spec).unwrap();
        // 头与四层定义、APPID SOUTH、实体编码。
        assert!(dxf_text.contains("AC1024"));
        for layer in ["ZD", "JZX", "JZD", "ZJ"] {
            assert!(dxf_text.contains(layer), "缺图层 {layer}");
        }
        assert!(dxf_text.contains("APPID"));
        assert!(dxf_text.contains("SOUTH"));
        assert!(dxf_text.contains("302001"));
        assert!(dxf_text.contains("302002"));
        // 实体计数（文本层）：CIRCLE == 界址点数；LWPOLYLINE == 边数 + 1（ZD）；
        // TEXT == 边长 4 + 点号 4 + 分式 2 + 权利人 1；LINE == 分数线 1。
        assert_eq!(dxf_text.matches("\r\nCIRCLE\r\n").count(), points.len());
        assert_eq!(
            dxf_text.matches("\r\nLWPOLYLINE\r\n").count(),
            lines.len() + 1
        );
        assert_eq!(
            dxf_text.matches("\r\nTEXT\r\n").count(),
            lines.len() + points.len() + 3
        );
        assert_eq!(dxf_text.matches("\r\nLINE\r\n").count(), 1);
        // 分式分子 = 宗地代码末 7 位。
        assert!(dxf_text.contains("GB00025"));
        // 竖直边注记：rotation_deg=-90（顺时针为正）→ 50 组码 = +90（数学角）。
        assert!(dxf_text.contains("\r\n 50\r\n90\r\n"));
    }

    #[test]
    fn cass_dxf_readback_by_dxf_crate() {
        let (boundary, points, lines) = rect_fixture();
        let spec = CassDxfSpec {
            scale: 1000,
            parcel_code: "430103002007GB00025".to_string(),
            land_use: "0701".to_string(),
            owner: "张三".to_string(),
            xdata: true,
        };
        let dxf_text = parcel_to_cass_dxf(&boundary, &points, &lines, &spec).unwrap();
        // 用 dxf crate 回读：结构合法、版本/图层/APPID/实体数吻合。
        let drawing = dxf::Drawing::load(&mut dxf_text.as_bytes()).expect("dxf crate 应能回读");
        assert_eq!(drawing.header.version, dxf::enums::AcadVersion::R2010);
        let layer_names: Vec<String> = drawing.layers().map(|l| l.name.clone()).collect();
        for name in ["ZD", "JZX", "JZD", "ZJ"] {
            assert!(layer_names.iter().any(|n| n == name), "缺图层 {name}");
        }
        assert!(drawing.app_ids().any(|a| a.name == "SOUTH"));

        let mut n_circle = 0;
        let mut n_lwpoly = 0;
        let mut n_text = 0;
        let mut n_line = 0;
        let mut zd_closed = false;
        for entity in drawing.entities() {
            match &entity.specific {
                dxf::entities::EntityType::Circle(_) => n_circle += 1,
                dxf::entities::EntityType::LwPolyline(pl) => {
                    n_lwpoly += 1;
                    if entity.common.layer == "ZD" {
                        zd_closed = pl.is_closed();
                        // ZD 外环顶点数 = 矩形 4 角（重复闭合点不写出）。
                        assert_eq!(pl.vertices.len(), 4);
                    }
                }
                dxf::entities::EntityType::Text(_) => n_text += 1,
                dxf::entities::EntityType::Line(_) => n_line += 1,
                _ => {}
            }
        }
        assert_eq!(n_circle, points.len());
        assert_eq!(n_lwpoly, lines.len() + 1);
        assert_eq!(n_text, lines.len() + points.len() + 3);
        assert_eq!(n_line, 1);
        assert!(zd_closed, "ZD 外环应为闭合 LWPOLYLINE");
        // XDATA 回读：JZD CIRCLE 挂 SOUTH/302001。
        let circle = drawing
            .entities()
            .find(|e| matches!(e.specific, dxf::entities::EntityType::Circle(_)))
            .expect("应有 CIRCLE");
        let xdata = circle
            .common
            .x_data
            .iter()
            .find(|x| x.application_name == "SOUTH")
            .expect("CIRCLE 应挂 SOUTH XDATA");
        assert!(xdata.items.iter().any(|item| matches!(
            item,
            dxf::XDataItem::Str(s) if s == "302001"
        )));
    }

    #[test]
    fn cass_dxf_without_xdata_omits_south() {
        let (boundary, points, lines) = rect_fixture();
        // xdata=false 且分式字段为空：无 APPID/SOUTH/编码，ZJ 无分式注记。
        let spec = CassDxfSpec {
            xdata: false,
            ..Default::default()
        };
        let dxf_text = parcel_to_cass_dxf(&boundary, &points, &lines, &spec).unwrap();
        assert!(!dxf_text.contains("SOUTH"));
        assert!(!dxf_text.contains("APPID"));
        assert!(!dxf_text.contains("302001"));
        assert!(!dxf_text.contains("302002"));
        // TEXT 仅剩边长 + 点号。
        assert_eq!(
            dxf_text.matches("\r\nTEXT\r\n").count(),
            lines.len() + points.len()
        );
        // 仍可被 dxf crate 回读。
        let drawing = dxf::Drawing::load(&mut dxf_text.as_bytes()).expect("dxf crate 应能回读");
        assert!(drawing.entities().count() > 0);
    }
}
