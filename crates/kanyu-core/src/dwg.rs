//! DWG 读取（acadrust 0.4 + 自持补丁层）。
//!
//! 背景与证据（acadrust 覆盖率 spike，现场 `%TEMP%\dwg_spike`，总规 §6.4）：
//! acadrust 0.4.1 对全部 143 个真实 R2000（AC1015）样本"打开成功但 0 实体"——
//! 其 AC15 objects 定位推断 `objects_size = handles_seeker - aux_header_end`
//! 在"AuxHeader 位于 Handles 之后"的合法 R2000 布局下为负，静默空文档。
//! 按 ODA 约定 AcDbObjects 位于 **Classes 结束与 Handles 开始之间**，手工定位后
//! 同一样本集 521,750 实体 100% 可读（object reader/builder 全链路健康，
//! 唯一缺陷即定位推断）。
//!
//! 本模块两层补丁：
//!
//! 1. **AC15 定位 workaround**：解析 DWG 头 locator records，以
//!    `[Classes_end, Handles_start)` 推断 objects 段，直接驱动 acadrust
//!    底层 pub API（handle_reader / object_reader / DwgDocumentBuilder）。
//!    非 AC15 版本走 acadrust 原生 `DwgReader::read`；空文档回退本层。
//! 2. **编码层**：DWG 字符串（图层名/Text/MText）的两种乱码形态修复——
//!    GBK 字节被 Latin-1 逐字节展开（`"¿±²â¶¨½çÍ¼…"`）与 MIF `\U+XXXX`
//!    转义未解码（`"\\U+754c\\U+5740\\U+7ebf"`）。`decode_dwg_string` 统一处理。

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

use acadrust::entities::EntityType;

use crate::error::{KanyuError, Result};

/// DWG 读取统计（写入 `foreign_members["kanyu:dwg"]`，与 buffer 的
/// `skipped` 上报模式一致）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DwgStats {
    /// DWG 版本（如 `AC1015`）。
    pub version: String,
    /// 按类型跳过的实体计数（INSERT/HATCH/MTEXT/TEXT/DIMENSION/ELLIPSE/SPLINE/…）。
    pub skipped_by_type: BTreeMap<String, usize>,
    /// 退化几何跳过计数（<2 点开放、<3 点闭合、半径 ≤0）。
    pub degenerate: usize,
}

/// DWG → FeatureCollection + 统计。实体映射（z 丢弃）：
/// POINT→Point、LINE→LineString、LWPOLYLINE/POLYLINE（2D/3D 取 xy，
/// 闭合→Polygon 单环）→同 dxf 口径、CIRCLE→Polygon(64 段)、
/// ARC→LineString(64 段，弧度)。ELLIPSE/SPLINE 本轮跳过+计数（近似 📋）；
/// INSERT/HATCH/MTEXT/TEXT/DIMENSION 系及其余跳过+按类型计数
///（MTEXT/TEXT 后续可作标注层 📋，保持几何图层语义纯净）。
pub fn dwg_to_collection(path: &str) -> Result<(geojson::FeatureCollection, DwgStats)> {
    let (doc, version, codepage) = read_document(path)?;
    let mut stats = DwgStats {
        version,
        ..Default::default()
    };

    let mut features = Vec::new();
    for entity in doc.entities() {
        match entity_to_geojson(entity) {
            EntityOutcome::Geometry(value) => {
                let mut properties = serde_json::Map::new();
                properties.insert(
                    "layer".to_string(),
                    serde_json::Value::String(decode_dwg_string(&entity.common().layer, codepage)),
                );
                features.push(geojson::Feature {
                    bbox: None,
                    geometry: Some(geojson::Geometry::new(value)),
                    id: None,
                    properties: Some(properties),
                    foreign_members: None,
                });
            }
            EntityOutcome::Degenerate => stats.degenerate += 1,
            EntityOutcome::Skip(type_name) => {
                *stats.skipped_by_type.entry(type_name).or_insert(0) += 1;
            }
        }
    }

    let mut foreign = serde_json::Map::new();
    foreign.insert(
        "kanyu:dwg".to_string(),
        serde_json::to_value(&stats)
            .map_err(|e| KanyuError::Other(format!("dwg 统计序列化失败: {e}")))?,
    );
    let collection = geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: Some(foreign),
    };
    Ok((collection, stats))
}

/// 读取结果：文档 + 版本名 + codepage（编码层输入）。
type LoadedDocument = (acadrust::document::CadDocument, String, u16);

/// 统一读取入口：AC15 系（R13/R14/R2000，线性布局）直接走 workaround
///（原生路径对该系必然空文档——spike 证据）；其他版本走 acadrust 原生
/// `DwgReader::read`，若空文档且版本属 AC15 系再回退 workaround。
fn read_document(path: &str) -> Result<LoadedDocument> {
    let head = read_head_bytes(path)?;
    let version = parse_version(&head)?;
    if is_ac15(version) {
        // AC15 系直接走 workaround（原生路径对本系为已知空结果）。
        return read_ac15_workaround(path, &head, version);
    }
    let mut reader = acadrust::io::dwg::dwg_reader::DwgReader::from_file(path)
        .map_err(|e| KanyuError::Other(format!("dwg 读取失败（{path}）：{e}")))?;
    let doc = reader.read().map_err(|e| {
        KanyuError::Other(format!(
            "dwg 解析失败（{path}）：{e}；文件可能损坏或版本不受支持"
        ))
    })?;
    let codepage = codepage_from_header_str(&doc.header.code_page);
    Ok((doc, format!("{version:?}"), codepage))
}

/// AC15 系（AC1012/AC1014/AC1015）workaround 读取。
fn read_ac15_workaround(
    path: &str,
    head: &[u8],
    version: acadrust::types::DxfVersion,
) -> Result<LoadedDocument> {
    use acadrust::io::dwg::dwg_document_builder::DwgDocumentBuilder;
    use acadrust::io::dwg::dwg_stream_readers::{handle_reader, object_reader};

    // locator records：0x15 record_count(4B LE)，0x19 起每条 9B
    //（number(1) + seeker(4) + size(4)）；number: 1=Classes, 2=Handles。
    if head.len() < 0x19 + 9 {
        return Err(KanyuError::Other(format!(
            "dwg 头部过短（{path}）：不是有效的 AC15 DWG"
        )));
    }
    let codepage = u16::from_le_bytes([head[0x13], head[0x14]]);
    let record_count = i32::from_le_bytes(head[0x15..0x19].try_into().unwrap()).min(6);
    let (mut classes_end, mut handles_seeker) = (0i64, 0i64);
    for i in 0..record_count as usize {
        let off = 0x19 + i * 9;
        if off + 9 > head.len() {
            break;
        }
        let number = head[off];
        let seeker = i32::from_le_bytes(head[off + 1..off + 5].try_into().unwrap()) as i64;
        let size = i32::from_le_bytes(head[off + 5..off + 9].try_into().unwrap()) as i64;
        match number {
            1 => classes_end = seeker + size,
            2 => handles_seeker = seeker,
            _ => {}
        }
    }
    if handles_seeker <= classes_end {
        return Err(KanyuError::Other(format!(
            "dwg 定位失败（{path}）：AC15 locator 布局异常（Handles @{handles_seeker} ≤ Classes 结束 @{classes_end}）"
        )));
    }

    let mut file = std::fs::File::open(path)?;
    let file_size = file.metadata()?.len() as i64;
    // Handles 段（到文件尾；read_handles 自带边界）。
    let mut handles_buf = vec![0u8; (file_size - handles_seeker) as usize];
    file.seek(SeekFrom::Start(handles_seeker as u64))?;
    file.read_exact(&mut handles_buf)?;
    let mut handle_map = handle_reader::read_handles(&handles_buf)
        .map_err(|e| KanyuError::Other(format!("dwg Handles 段解析失败（{path}）：{e}")))?;
    // AC15：handle 偏移为绝对文件位置，需减 objects 段基址转为 buffer 相对。
    for v in handle_map.values_mut() {
        *v -= classes_end;
    }

    // AcDbObjects 段 = [Classes_end, Handles_start)。
    let objects_size = (handles_seeker - classes_end) as usize;
    let mut objects_buf = vec![0u8; objects_size];
    file.seek(SeekFrom::Start(classes_end as u64))?;
    file.read_exact(&mut objects_buf)?;

    let obj_reader = object_reader::DwgObjectReader::new(objects_buf, version, handle_map)
        .map_err(|e| KanyuError::Other(format!("dwg 对象读取器初始化失败（{path}）：{e}")))?;
    let mut doc = acadrust::document::CadDocument::default();
    DwgDocumentBuilder::new(obj_reader).build(&mut doc);
    Ok((doc, format!("{version:?}"), codepage))
}

/// 读文件头（locator 解析所需的最小字节数）。
fn read_head_bytes(path: &str) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut head = vec![0u8; 0x19 + 6 * 9 + 8];
    let n = file.read(&mut head)?;
    head.truncate(n);
    Ok(head)
}

/// 解析 DWG 版本串（前 6 字节）。
fn parse_version(head: &[u8]) -> Result<acadrust::types::DxfVersion> {
    if head.len() < 6 {
        return Err(KanyuError::Other("dwg 文件过短：缺版本串".to_string()));
    }
    let version_str = String::from_utf8_lossy(&head[..6]);
    acadrust::types::DxfVersion::parse(&version_str)
        .ok_or_else(|| KanyuError::Other(format!("dwg 无法识别的版本串 '{version_str}'")))
}

/// 是否 AC15 线性布局系（R13/R14/R2000）。
fn is_ac15(version: acadrust::types::DxfVersion) -> bool {
    use acadrust::types::DxfVersion;
    matches!(
        version,
        DxfVersion::AC1012 | DxfVersion::AC1014 | DxfVersion::AC1015
    )
}

/// header 的 DWGCODEPAGE 字符串（如 "ANSI_936"）→ 数值 codepage。
fn codepage_from_header_str(s: &str) -> u16 {
    s.strip_prefix("ANSI_")
        .or_else(|| s.strip_prefix("CP"))
        .and_then(|n| n.parse().ok())
        .unwrap_or(1252)
}

// ===== 实体映射 =====

/// 实体映射结果。
enum EntityOutcome {
    Geometry(geojson::Value),
    /// 退化几何（dxf 同口径：不计要素、单独计数）。
    Degenerate,
    /// 按类型跳过（类型名）。
    Skip(String),
}

fn entity_to_geojson(entity: &EntityType) -> EntityOutcome {
    match entity {
        EntityType::Point(p) => {
            EntityOutcome::Geometry(geojson::Value::Point(vec![p.location.x, p.location.y]))
        }
        EntityType::Line(l) => EntityOutcome::Geometry(geojson::Value::LineString(vec![
            vec![l.start.x, l.start.y],
            vec![l.end.x, l.end.y],
        ])),
        EntityType::LwPolyline(p) => {
            let positions: Vec<Vec<f64>> = p
                .vertices
                .iter()
                .map(|v| vec![v.location.x, v.location.y])
                .collect();
            polyline_outcome(positions, p.is_closed)
        }
        EntityType::Polyline(p) => {
            let positions: Vec<Vec<f64>> = p
                .vertices
                .iter()
                .map(|v| vec![v.location.x, v.location.y])
                .collect();
            polyline_outcome(positions, p.is_closed())
        }
        EntityType::Polyline2D(p) => {
            let positions: Vec<Vec<f64>> = p
                .vertices
                .iter()
                .map(|v| vec![v.location.x, v.location.y])
                .collect();
            polyline_outcome(positions, p.is_closed())
        }
        EntityType::Polyline3D(p) => {
            let positions: Vec<Vec<f64>> = p
                .vertices
                .iter()
                .map(|v| vec![v.position.x, v.position.y])
                .collect();
            polyline_outcome(positions, p.is_closed())
        }
        EntityType::Circle(c) => {
            if c.radius <= 0.0 {
                return EntityOutcome::Degenerate;
            }
            EntityOutcome::Geometry(geojson::Value::Polygon(vec![arc_positions(
                c.center.x,
                c.center.y,
                c.radius,
                0.0,
                std::f64::consts::TAU,
            )]))
        }
        EntityType::Arc(a) => {
            if a.radius <= 0.0 {
                return EntityOutcome::Degenerate;
            }
            EntityOutcome::Geometry(geojson::Value::LineString(arc_positions(
                a.center.x,
                a.center.y,
                a.radius,
                a.start_angle,
                a.end_angle,
            )))
        }
        other => EntityOutcome::Skip(
            format!("{other:?}")
                .split('(')
                .next()
                .unwrap_or("Unknown")
                .to_string(),
        ),
    }
}

/// 折线 → 开放 LineString / 闭合 Polygon（单环，首尾自动闭合）。
/// 退化（<2 开放、<3 闭合）返回 Degenerate——与 dxf 口径一致。
fn polyline_outcome(mut positions: Vec<Vec<f64>>, closed: bool) -> EntityOutcome {
    if closed {
        if positions.len() < 3 {
            return EntityOutcome::Degenerate;
        }
        if positions.first() != positions.last() {
            positions.push(positions[0].clone());
        }
        EntityOutcome::Geometry(geojson::Value::Polygon(vec![positions]))
    } else {
        if positions.len() < 2 {
            return EntityOutcome::Degenerate;
        }
        EntityOutcome::Geometry(geojson::Value::LineString(positions))
    }
}

/// 圆/弧 64 段折线近似（acadrust 弧度制；end<=start 时按跨 2π 处理）。
fn arc_positions(cx: f64, cy: f64, r: f64, start_rad: f64, end_rad: f64) -> Vec<Vec<f64>> {
    const SEGMENTS: usize = 64;
    let sweep = if end_rad > start_rad {
        end_rad - start_rad
    } else {
        end_rad + std::f64::consts::TAU - start_rad
    };
    (0..=SEGMENTS)
        .map(|i| {
            let theta = start_rad + sweep * i as f64 / SEGMENTS as f64;
            vec![cx + r * theta.cos(), cy + r * theta.sin()]
        })
        .collect()
}

// ===== 编码层 =====

/// DWG 字符串解码（编码层，见模块文档）：
/// 1. 全 ASCII → 仅 MIF 解码。
/// 2. 含高位字节（Latin-1 展开的 GBK 字节，spike 实证）→ 逐 char 取低 8 位
///    还原字节序列，按 GBK 解码；codepage 为已知 CJK 页（936 GBK / 950 BIG5 /
///    932 Shift-JIS / 949 EUC-KR）时优先按该页。
/// 3. 统一 MIF `\U+XXXX` 解码。
///
/// 启发式局限：真 Latin-1 文本（如法语 "é"）在非 CJK codepage 下会被误判为
/// GBK——v0.1 面向中文图纸场景，注释即契约；非 GBK 系数据可后续按
/// codepage 精确分派。
pub fn decode_dwg_string(raw: &str, codepage: u16) -> String {
    let restored: String = if raw.is_ascii() {
        raw.to_string()
    } else {
        let bytes: Vec<u8> = raw.chars().map(|c| (c as u32) as u8).collect();
        let (text, _, _) = match codepage {
            950 => encoding_rs::BIG5.decode(&bytes),
            932 => encoding_rs::SHIFT_JIS.decode(&bytes),
            949 => encoding_rs::EUC_KR.decode(&bytes),
            // 936（GBK）与兜底：spike 实证中国 R2000 图纸为 GBK。
            _ => encoding_rs::GBK.decode(&bytes),
        };
        text.into_owned()
    };
    decode_mif(&restored)
}

/// MIF `\U+XXXX` 序列解码（`\U+754c` → 界）。
fn decode_mif(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("\\U+") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 3..];
        let hex: String = tail.chars().take(4).collect();
        let mif_char = if hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
        } else {
            None
        };
        if let Some(cp) = mif_char {
            out.push(cp);
            rest = &tail[4..];
        } else {
            // 非合法 MIF 序列：原样保留反斜杠段。
            out.push_str("\\U+");
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sample_r2000.dwg");

    #[test]
    fn dwg_load_parses_fixture_with_types_and_stats() {
        let (collection, stats) = dwg_to_collection(FIXTURE).unwrap();
        assert_eq!(stats.version, "AC1015");
        // 可映射要素存在（spike 实测该文件 1769 实体 / 500 可映射）。
        assert!(
            collection.features.len() >= 400,
            "可映射要素数异常: {}",
            collection.features.len()
        );
        // 几何类型齐全（该文件含 LwPolyline 面/线 与 Circle 面）。
        let mut types: Vec<String> = collection
            .features
            .iter()
            .filter_map(|f| f.geometry.as_ref().map(|g| g.value.type_name().to_string()))
            .collect();
        types.sort();
        types.dedup();
        assert!(
            types.contains(&"Polygon".to_string()),
            "应含 Polygon: {types:?}"
        );
        // 跳过类型计数（spike 实测：insert 524 / hatch 99 / text 645）。
        assert_eq!(stats.skipped_by_type.get("Insert"), Some(&524));
        assert_eq!(stats.skipped_by_type.get("Hatch"), Some(&99));
        assert_eq!(
            stats.skipped_by_type.get("Text").copied().unwrap_or(0)
                + stats.skipped_by_type.get("MText").copied().unwrap_or(0),
            645
        );
        // 统计写入 foreign_members。
        assert!(collection.foreign_members.as_ref().unwrap()["kanyu:dwg"].is_object());
        // 坐标合理（该文件为高斯投影量级，非 NaN 且 |v| < 1e9）。
        for f in &collection.features {
            let Some(g) = &f.geometry else { continue };
            fn check(v: &geojson::Value) -> bool {
                fn coords(v: &geojson::Value) -> Vec<f64> {
                    match v {
                        geojson::Value::Point(p) => p.clone(),
                        geojson::Value::LineString(l) => l.iter().flatten().cloned().collect(),
                        geojson::Value::Polygon(r) => {
                            r.iter().flatten().flatten().cloned().collect()
                        }
                        _ => Vec::new(),
                    }
                }
                coords(v).iter().all(|c| c.is_finite() && c.abs() < 1e9)
            }
            assert!(check(&g.value), "坐标越界或 NaN: {:?}", g.value);
        }
    }

    #[test]
    fn dwg_load_decodes_chinese_layer_names() {
        let (collection, _) = dwg_to_collection(FIXTURE).unwrap();
        // spike 实测该文件含 GBK 图层（勘测定界图_DLWK_图廓__国家2017地形图图廓__*）。
        let has_chinese = collection.features.iter().any(|f| {
            f.properties
                .as_ref()
                .and_then(|p| p.get("layer"))
                .and_then(|v| v.as_str())
                .map(|l| l.contains("图廓"))
                .unwrap_or(false)
        });
        assert!(has_chinese, "图层名应正确解码出中文（含'图廓'）");
    }

    #[test]
    fn decode_dwg_string_handles_gbk_mif_ascii() {
        // GBK 字节 Latin-1 展开 → 中文（"图廓" GBK = CD BC C0 AA）。
        let raw = "\u{CD}\u{BC}\u{C0}\u{AA}";
        assert_eq!(decode_dwg_string(raw, 936), "图廓");
        // MIF 转义 → 中文（界址线 = 754c 5740 7ebf）。
        assert_eq!(
            decode_dwg_string("\\U+754c\\U+5740\\U+7ebf", 1252),
            "界址线"
        );
        // ASCII 原样。
        assert_eq!(decode_dwg_string("Defpoints", 1252), "Defpoints");
        // 非合法 MIF 原样保留。
        assert_eq!(decode_dwg_string("a\\U+zz", 1252), "a\\U+zz");
    }
}
