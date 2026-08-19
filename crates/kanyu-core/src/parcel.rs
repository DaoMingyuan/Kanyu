//! 宗地 TXT（界址点坐标）格式 —— 自堪舆工具箱（KanyuTools QGIS 插件）
//! `features/txt_feature.py` 移植，行为与之一致（同格式互认）。
//!
//! ## 格式（v1.0）
//!
//! ```text
//! [属性描述]
//! 格式版本号=1.0
//! 数据产生单位=...
//! 坐标系=CGCS2000...
//! （其余 key=value 自由扩展）
//! [地块坐标]
//! 坐标行数,面积,地块编号,地块名称,面,图幅号,用途,,@
//! 点号,圈号,X,Y
//! ...
//! ```
//!
//! - 地块说明行：≥9 列、第 5 列为「面」、含 `@` 结束符；
//! - 界址点行：4 列；**X=纵坐标（北）、Y=横坐标（东）**——中国测绘惯例，
//!   GeoJSON 位置取 `[Y, X]`（即 [东, 北]）；
//! - 圈号：1 = 外环，≥2 = 内环（洞）；每圈 ≥4 行且首尾点同名同坐标；
//! - 兼容全角括号段标（【地块坐标】/［属性描述］）与 UTF-8 BOM。

use std::collections::BTreeMap;

use crate::error::{KanyuError, Result};

/// 界址点。
#[derive(Debug, Clone, PartialEq)]
pub struct BoundaryPoint {
    /// 点号。
    pub name: String,
    /// 圈号（1=外环，≥2=洞）。
    pub ring: i64,
    /// 纵坐标（北）。
    pub x: f64,
    /// 横坐标（东）。
    pub y: f64,
}

/// 一个地块。
#[derive(Debug, Clone)]
pub struct Parcel {
    /// 说明行声明的坐标行数。
    pub declared_rows: i64,
    /// 面积（㎡，可空）。
    pub area: Option<f64>,
    /// 地块编号。
    pub parcel_id: String,
    /// 地块名称。
    pub parcel_name: String,
    /// 图幅号。
    pub map_sheet: String,
    /// 用途。
    pub usage: String,
    /// 界址点（按文件顺序）。
    pub points: Vec<BoundaryPoint>,
}

/// 宗地 TXT 文档。
#[derive(Debug, Clone, Default)]
pub struct ParcelDoc {
    /// 表头（`[属性描述]` key=value）。
    pub header: BTreeMap<String, String>,
    /// 地块清单。
    pub parcels: Vec<Parcel>,
}

/// 质检问题。
#[derive(Debug, Clone, serde::Serialize)]
pub struct QualityIssue {
    /// 行号（0 = 文档级）。
    pub line: usize,
    /// 问题描述。
    pub message: String,
    /// 级别（错误/警告）。
    pub level: String,
}

/// 是否宗地 TXT（含两段段标，兼容全角括号）。
pub fn is_parcel_txt(text: &str) -> bool {
    let normalized = normalize_brackets(text);
    normalized.contains("[地块坐标]") && normalized.contains("[属性描述]")
}

/// 归一化行：去 BOM 与首尾空白。
fn normalize_line(line: &str) -> &str {
    line.trim_start_matches('\u{feff}').trim()
}

/// 全角段标 → 半角。
fn normalize_brackets(text: &str) -> String {
    text.replace('【', "[")
        .replace('】', "]")
        .replace('［', "[")
        .replace('］', "]")
}

/// 段标识别。
fn section_name(line: &str) -> Option<String> {
    let value = normalize_brackets(normalize_line(line));
    if !value.starts_with('[') || !value.ends_with(']') {
        return None;
    }
    Some(value)
}

/// 是否地块说明行（≥9 列、第 5 列为「面」、含 @）。
fn is_parcel_description(line: &str) -> bool {
    let parts: Vec<&str> = line.split(',').collect();
    parts.len() >= 9
        && parts[4].trim().eq_ignore_ascii_case("面")
        && parts.iter().any(|p| p.trim() == "@")
}

/// 解析宗地 TXT（含完整校验，中文错误带行号）。
pub fn parse_parcel_txt(text: &str) -> Result<ParcelDoc> {
    let mut doc = ParcelDoc::default();
    let mut section = String::new();

    for (line_idx, raw) in text.lines().enumerate() {
        let line_number = line_idx + 1;
        let line = normalize_brackets(normalize_line(raw));
        if line.is_empty() {
            continue;
        }
        if let Some(name) = section_name(&line) {
            section = name;
            continue;
        }
        if section == "[属性描述]" {
            if let Some((key, value)) = line.split_once('=') {
                if !key.trim().is_empty() {
                    doc.header
                        .insert(key.trim().to_string(), value.trim().to_string());
                }
            }
            continue;
        }
        if section != "[地块坐标]" {
            continue;
        }
        if is_parcel_description(&line) {
            let parcel = parse_parcel_description(&line, line_number)?;
            doc.parcels.push(parcel);
            continue;
        }
        let point = parse_boundary_point(&line, line_number)?;
        match doc.parcels.last_mut() {
            Some(current) => current.points.push(point),
            None => {
                return Err(KanyuError::Other(format!(
                    "第 {line_number} 行坐标缺少地块说明行"
                )))
            }
        }
    }
    validate_parcel_doc(&doc)?;
    Ok(doc)
}

/// 地块说明行解析。
fn parse_parcel_description(line: &str, line_number: usize) -> Result<Parcel> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 9 {
        return Err(KanyuError::Other(format!(
            "第 {line_number} 行地块说明格式不正确（需 ≥9 列）"
        )));
    }
    let declared_rows = parts[0]
        .trim()
        .parse::<i64>()
        .map_err(|_| KanyuError::Other(format!("第 {line_number} 行地块坐标行数不是整数")))?;
    Ok(Parcel {
        declared_rows,
        area: {
            let t = parts[1].trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        },
        parcel_id: parts[2].trim().to_string(),
        parcel_name: parts[3].trim().to_string(),
        map_sheet: parts[5].trim().to_string(),
        usage: parts[6].trim().to_string(),
        points: Vec::new(),
    })
}

/// 界址点行解析（点号,圈号,X,Y）。
fn parse_boundary_point(line: &str, line_number: usize) -> Result<BoundaryPoint> {
    let parts: Vec<&str> = line.split(',').collect();
    if parts.len() < 4 {
        return Err(KanyuError::Other(format!(
            "第 {line_number} 行界址点坐标格式不正确（需 4 列）"
        )));
    }
    let ring = parts[1]
        .trim()
        .parse::<i64>()
        .map_err(|_| KanyuError::Other(format!("第 {line_number} 行圈号不是整数")))?;
    let x = parts[2]
        .trim()
        .parse::<f64>()
        .map_err(|_| KanyuError::Other(format!("第 {line_number} 行 X 轴坐标不是数字")))?;
    let y = parts[3]
        .trim()
        .parse::<f64>()
        .map_err(|_| KanyuError::Other(format!("第 {line_number} 行 Y 轴坐标不是数字")))?;
    Ok(BoundaryPoint {
        name: parts[0].trim().to_string(),
        ring,
        x,
        y,
    })
}

/// 文档校验（与工具箱 validate_txt_parcel_document 一致）。
pub fn validate_parcel_doc(doc: &ParcelDoc) -> Result<()> {
    if doc.header.is_empty() {
        return Err(KanyuError::Other("TXT 缺少 [属性描述] 表头".to_string()));
    }
    if doc.parcels.is_empty() {
        return Err(KanyuError::Other(
            "TXT 缺少 [地块坐标] 地块内容".to_string(),
        ));
    }
    for parcel in &doc.parcels {
        if parcel.points.len() < 4 {
            return Err(KanyuError::Other(format!(
                "地块「{}」界址点少于 4 行，无法构面",
                parcel.parcel_name
            )));
        }
        if parcel.declared_rows != parcel.points.len() as i64 {
            return Err(KanyuError::Other(format!(
                "地块「{}」说明行坐标数量与实际界址点数量不一致",
                parcel.parcel_name
            )));
        }
        for (ring_no, ring) in rings_of(parcel) {
            if ring.len() < 4 {
                return Err(KanyuError::Other(format!(
                    "地块「{}」第 {ring_no} 圈界址点少于 4 行",
                    parcel.parcel_name
                )));
            }
            let (first, last) = (ring.first().unwrap(), ring.last().unwrap());
            if !first.name.eq_ignore_ascii_case(&last.name)
                || (first.x - last.x).abs() > 1e-8
                || (first.y - last.y).abs() > 1e-8
            {
                return Err(KanyuError::Other(format!(
                    "地块「{}」第 {ring_no} 圈最后一个界址点必须与第一个界址点相同",
                    parcel.parcel_name
                )));
            }
            if twice_ring_area(&ring).abs() <= 1e-8 {
                return Err(KanyuError::Other(format!(
                    "地块「{}」第 {ring_no} 圈界址点共线或面积为 0",
                    parcel.parcel_name
                )));
            }
        }
    }
    Ok(())
}

/// 按圈号分组（升序）。
fn rings_of(parcel: &Parcel) -> Vec<(i64, Vec<BoundaryPoint>)> {
    let mut rings: BTreeMap<i64, Vec<BoundaryPoint>> = BTreeMap::new();
    for point in &parcel.points {
        rings.entry(point.ring).or_default().push(point.clone());
    }
    rings.into_iter().collect()
}

/// 两倍面积（鞋带公式；首尾重复点自动去重）。
fn twice_ring_area(points: &[BoundaryPoint]) -> f64 {
    // GIS 坐标 = [Y(东), X(北)]；鞋带对轴序不敏感（只差符号）。
    let mut coords: Vec<(f64, f64)> = points.iter().map(|p| (p.y, p.x)).collect();
    if coords.len() > 1 && coords.first() == coords.last() {
        coords.pop();
    }
    let mut total = 0.0;
    for i in 0..coords.len() {
        let (x1, y1) = coords[i];
        let (x2, y2) = coords[(i + 1) % coords.len()];
        total += x1 * y2 - x2 * y1;
    }
    total
}

/// ParcelDoc → FeatureCollection：每地块一个 Polygon（外环 + 洞），
/// 属性：parcel_id/parcel_name/parcel_use/map_sheet/area（可空跳过）；
/// 表头写入 `foreign_members["kanyu:parcel"]`。
pub fn parcel_doc_to_collection(doc: &ParcelDoc) -> geojson::FeatureCollection {
    let mut features = Vec::new();
    for parcel in &doc.parcels {
        let rings = rings_of(parcel);
        let mut polygon_rings: Vec<Vec<Vec<f64>>> = Vec::new();
        for (_, ring) in rings {
            // GeoJSON 位置 = [Y(东), X(北)]（测绘惯例 X 北 Y 东）。
            polygon_rings.push(ring.iter().map(|p| vec![p.y, p.x]).collect());
        }
        let mut properties = serde_json::Map::new();
        properties.insert("parcel_id".to_string(), parcel.parcel_id.clone().into());
        properties.insert("parcel_name".to_string(), parcel.parcel_name.clone().into());
        properties.insert("parcel_use".to_string(), parcel.usage.clone().into());
        properties.insert("map_sheet".to_string(), parcel.map_sheet.clone().into());
        if let Some(area) = parcel.area {
            properties.insert("area".to_string(), area.into());
        }
        features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Polygon(
                polygon_rings,
            ))),
            id: None,
            properties: Some(properties),
            foreign_members: None,
        });
    }
    let mut foreign = serde_json::Map::new();
    foreign.insert(
        "kanyu:parcel".to_string(),
        serde_json::json!({
            "header": doc.header,
            "format": "宗地TXT/1.0",
        }),
    );
    geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: Some(foreign),
    }
}

/// FeatureCollection → 宗地 TXT 文本（面要素；点号 J1.. 地块内连续编号，
/// 圈号 1=外环、2+=洞；说明行 declared = 实际点行数）。
pub fn collection_to_parcel_txt(
    collection: &geojson::FeatureCollection,
    decimal_places: usize,
    crs_name: &str,
) -> Result<String> {
    let decimals = decimal_places.clamp(0, 8);
    let mut rows: Vec<String> = vec!["[属性描述]".to_string()];
    for (key, value) in [
        ("格式版本号", "1.0".to_string()),
        ("数据产生单位", String::new()),
        ("数据产生日期", String::new()),
        ("坐标系", crs_name.to_string()),
        ("几度分带", String::new()),
        ("投影类型", String::new()),
        ("计量单位", "米".to_string()),
        ("带号", String::new()),
        ("精度", precision_text(decimals)),
        ("转换参数", String::new()),
    ] {
        rows.push(format!("{key}={value}"));
    }
    rows.push("[地块坐标]".to_string());

    let mut feature_index = 0usize;
    for feature in &collection.features {
        let Some(geometry) = &feature.geometry else {
            continue;
        };
        // 统一为「面集合」：Polygon = 单面，MultiPolygon = 逐子面独立成地块。
        let polys: Vec<&Vec<Vec<Vec<f64>>>> = match &geometry.value {
            geojson::Value::Polygon(_) => vec![],
            geojson::Value::MultiPolygon(polys) => polys.iter().collect(),
            _ => continue, // 非面要素跳过（宗地 TXT 仅面）
        };
        let poly_list: Vec<Vec<Vec<Vec<f64>>>> = match &geometry.value {
            geojson::Value::Polygon(rings) => vec![rings.clone()],
            _ => polys.iter().map(|p| (*p).clone()).collect(),
        };
        for (sub_idx, rings) in poly_list.iter().enumerate() {
            feature_index += 1;
            let sub = if poly_list.len() > 1 {
                Some(sub_idx + 1)
            } else {
                None
            };
            let point_rows: usize = rings.iter().map(|r| r.len()).sum();
            rows.push(parcel_description_row(
                feature,
                feature_index,
                point_rows,
                sub,
            ));
            let mut counter = 0usize;
            for (ring_no, ring) in rings.iter().enumerate() {
                push_ring_points(&mut rows, ring, ring_no as i64 + 1, decimals, &mut counter);
            }
        }
    }
    Ok(rows.join("\n") + "\n")
}

/// 地块说明行（编号/名称取属性 parcel_id/parcel_name，缺省生成）。
fn parcel_description_row(
    feature: &geojson::Feature,
    index: usize,
    point_rows: usize,
    sub_index: Option<usize>,
) -> String {
    let props = feature.properties.as_ref();
    let get = |key: &str| {
        props
            .and_then(|p| p.get(key))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default()
    };
    let mut id = get("parcel_id");
    if id.is_empty() {
        id = index.to_string();
    }
    if let Some(sub) = sub_index {
        id = format!("{id}_{sub}");
    }
    let mut name = get("parcel_name");
    if name.is_empty() {
        name = format!("地块{index}");
    }
    if let Some(sub) = sub_index {
        name = format!("{name}_{sub}");
    }
    let use_ = get("parcel_use");
    let sheet = get("map_sheet");
    let area = props
        .and_then(|p| p.get("area"))
        .and_then(|v| v.as_f64())
        .map(|a| format!("{a:.4}"))
        .unwrap_or_default();
    format!("{point_rows},{area},{id},{name},面,{sheet},{use_},,@")
}

/// 写出一圈界址点（点号 J{n} 地块内连续；写回 X=北=pos[1]，Y=东=pos[0]；
/// 闭合点（与首点同坐标）复用首点编号——格式校验要求首尾点名一致）。
fn push_ring_points(
    rows: &mut Vec<String>,
    ring: &[Vec<f64>],
    ring_no: i64,
    decimals: usize,
    counter: &mut usize,
) {
    let first_pos = ring.first().cloned();
    let mut first_name = String::new();
    for pos in ring {
        let is_closure = first_pos.as_ref() == Some(pos) && *counter > 0;
        let name = if is_closure {
            first_name.clone()
        } else {
            *counter += 1;
            let n = format!("J{}", *counter);
            if first_name.is_empty() {
                first_name = n.clone();
            }
            n
        };
        rows.push(format!(
            "{name},{ring_no},{x:.prec$},{y:.prec$}",
            ring_no = ring_no,
            x = pos[1],
            y = pos[0],
            prec = decimals
        ));
    }
}

/// 精度文本（0.0001 风格）。
fn precision_text(decimals: usize) -> String {
    if decimals == 0 {
        return "1".to_string();
    }
    format!("0.{}1", "0".repeat(decimals - 1))
}

/// 质检（移植 check_txt_parcel_quality 的规则子集）：返回问题清单
/// （空 = 通过）。不抛错，逐条收集。
pub fn validate_parcel_txt(text: &str) -> Vec<QualityIssue> {
    let mut issues = Vec::new();
    let normalized = normalize_brackets(text);

    // 表头规则：必备 key 缺/空。
    let required = [
        "格式版本号",
        "数据产生单位",
        "数据产生日期",
        "坐标系",
        "几度分带",
        "投影类型",
        "计量单位",
        "带号",
        "精度",
        "转换参数",
    ];
    let mut header_keys: BTreeMap<String, String> = BTreeMap::new();
    let mut in_header = false;
    for raw in normalized.lines() {
        let line = normalize_line(raw);
        if let Some(name) = section_name(line) {
            in_header = name == "[属性描述]";
            continue;
        }
        if in_header {
            if let Some((k, v)) = line.split_once('=') {
                header_keys.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    if header_keys.is_empty() {
        issues.push(QualityIssue {
            line: 0,
            message: "[属性描述]缺失".to_string(),
            level: "错误".to_string(),
        });
    } else {
        for key in required {
            match header_keys.get(key) {
                None => issues.push(QualityIssue {
                    line: 0,
                    message: format!("缺少属性行：[{key}]"),
                    level: "错误".to_string(),
                }),
                Some(v) if v.is_empty() => issues.push(QualityIssue {
                    line: 0,
                    message: format!("[{key}]的属性值为空"),
                    level: "警告".to_string(),
                }),
                _ => {}
            }
        }
    }

    // 行级规则：中文逗号 / 空格。
    for (i, raw) in text.lines().enumerate() {
        let line = normalize_line(raw);
        if line.is_empty() {
            continue;
        }
        if line.contains('，') {
            issues.push(QualityIssue {
                line: i + 1,
                message: format!("当前行[{line}]：存在中文逗号"),
                level: "错误".to_string(),
            });
        }
        if line.contains(' ') {
            issues.push(QualityIssue {
                line: i + 1,
                message: format!("当前行[{line}]：存在空格"),
                level: "警告".to_string(),
            });
        }
    }

    // 结构规则：整体解析失败原因并入。
    if let Err(e) = parse_parcel_txt(text) {
        issues.push(QualityIssue {
            line: 0,
            message: format!("结构校验：{e}"),
            level: "错误".to_string(),
        });
    }
    issues
}

/// 简单点表解析（工具箱 parse_txt_points 移植）：`名称 X Y [Z]`，
/// `#` 注释行，逗号/空格/Tab 分隔 → Point 要素（属性 name，z 可空随行）。
pub fn parse_points_txt(text: &str) -> Result<geojson::FeatureCollection> {
    let mut features = Vec::new();
    for (line_idx, raw) in text.lines().enumerate() {
        let line_number = line_idx + 1;
        let line = normalize_line(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let spaced = line.replace(',', " ");
        let parts: Vec<&str> = spaced
            .split_whitespace()
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() < 3 {
            return Err(KanyuError::Other(format!(
                "第 {line_number} 行至少需要 名称 X Y 三列"
            )));
        }
        let x: f64 = parts[1]
            .parse()
            .map_err(|_| KanyuError::Other(format!("第 {line_number} 行坐标不是有效数字")))?;
        let y: f64 = parts[2]
            .parse()
            .map_err(|_| KanyuError::Other(format!("第 {line_number} 行坐标不是有效数字")))?;
        let mut properties = serde_json::Map::new();
        properties.insert("name".to_string(), parts[0].to_string().into());
        if let Some(z) = parts.get(3) {
            if let Ok(z) = z.parse::<f64>() {
                properties.insert("z".to_string(), z.into());
            }
        }
        features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geojson::Value::Point(vec![x, y]))),
            id: None,
            properties: Some(properties),
            foreign_members: None,
        });
    }
    if features.is_empty() {
        return Err(KanyuError::Other(
            "TXT 中未解析到任何点记录（宗地格式需含 [地块坐标] 段；点表为 名称 X Y [Z]）"
                .to_string(),
        ));
    }
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[属性描述]\n格式版本号=1.0\n数据产生单位=测试单位\n数据产生日期=2026-08-03\n坐标系=CGCS2000\n几度分带=3\n投影类型=高斯克吕格\n计量单位=米\n带号=39\n精度=0.0001\n转换参数=\n[地块坐标]\n5,100.0,1,测试地块,面,图幅A,住宅,,@\nJ1,1,4100000.0,39580000.0\nJ2,1,4100000.0,39580010.0\nJ3,1,4100010.0,39580010.0\nJ4,1,4100010.0,39580000.0\nJ1,1,4100000.0,39580000.0\n";

    #[test]
    fn parse_sample_parcel() {
        let doc = parse_parcel_txt(SAMPLE).unwrap();
        assert_eq!(doc.parcels.len(), 1);
        assert_eq!(doc.header.get("坐标系").unwrap(), "CGCS2000");
        let parcel = &doc.parcels[0];
        assert_eq!(parcel.parcel_name, "测试地块");
        assert_eq!(parcel.points.len(), 5);
        assert_eq!(parcel.declared_rows, 5);
    }

    #[test]
    fn collection_roundtrip_preserves_polygon() {
        let doc = parse_parcel_txt(SAMPLE).unwrap();
        let collection = parcel_doc_to_collection(&doc);
        assert_eq!(collection.features.len(), 1);
        let props = collection.features[0].properties.as_ref().unwrap();
        assert_eq!(props["parcel_name"].as_str().unwrap(), "测试地块");
        // GeoJSON 位置 = [Y(东), X(北)]。
        if let geojson::Value::Polygon(rings) =
            &collection.features[0].geometry.as_ref().unwrap().value
        {
            assert_eq!(rings[0][0], vec![39580000.0, 4100000.0]);
        } else {
            panic!("应为 Polygon");
        }
        // 写出再解析。
        let text = collection_to_parcel_txt(&collection, 4, "CGCS2000").unwrap();
        let doc2 = parse_parcel_txt(&text).unwrap();
        assert_eq!(doc2.parcels.len(), 1);
        assert_eq!(doc2.parcels[0].points.len(), 5);
    }

    #[test]
    fn validate_rules_fire() {
        // 缺表头必备项 + 中文逗号。
        let bad = "[属性描述]\n格式版本号=1.0\n[地块坐标]\n4，,1,地块,面,,,,@\nJ1,1,0,0\n";
        let issues = validate_parcel_txt(bad);
        assert!(issues.iter().any(|i| i.message.contains("缺少属性行")));
        assert!(issues.iter().any(|i| i.message.contains("中文逗号")));
    }

    #[test]
    fn parse_rejects_unclosed_ring() {
        // 去掉末行闭合点，并把声明行数改为 4（与实际行数一致，直触首尾不一致校验）。
        let lines: Vec<&str> = SAMPLE.lines().collect();
        let mut text = lines[..lines.len() - 1].join("\n");
        text = text.replace("5,100.0", "4,100.0");
        let err = parse_parcel_txt(&text).unwrap_err();
        assert!(err.to_string().contains("必须与第一个界址点相同"));
    }

    #[test]
    fn points_txt_parses_simple_table() {
        let text = "# 注释行\nP1 116.39 39.90 55.0\nP2,116.40,39.91\n";
        let collection = parse_points_txt(text).unwrap();
        assert_eq!(collection.features.len(), 2);
        let props = collection.features[0].properties.as_ref().unwrap();
        assert_eq!(props["name"].as_str().unwrap(), "P1");
        assert_eq!(props["z"].as_f64().unwrap(), 55.0);
        assert!(collection.features[1]
            .properties
            .as_ref()
            .unwrap()
            .get("z")
            .is_none());
    }

    #[test]
    fn points_txt_rejects_garbage() {
        assert!(parse_points_txt("only two\n").is_err());
    }
}
