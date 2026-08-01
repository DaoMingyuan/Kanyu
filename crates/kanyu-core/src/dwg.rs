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
/// ARC→LineString(64 段，弧度)、ELLIPSE→64 段参数方程近似
/// （全角→Polygon、部分弧→LineString）。
/// TEXT/MTEXT→标注要素（插入点 Point + `feature_kind: "annotation"` +
/// 解码清洗后 `text` 等属性——消费者可据 feature_kind 过滤，几何图层
/// 语义不被污染，文档即契约）。SPLINE/INSERT/HATCH/DIMENSION 系及其余
/// 跳过+按类型计数（近似/拆解 📋）。
pub fn dwg_to_collection(path: &str) -> Result<(geojson::FeatureCollection, DwgStats)> {
    let (doc, version, codepage) = read_document(path)?;
    let mut stats = DwgStats {
        version,
        ..Default::default()
    };

    let mut features = Vec::new();
    for entity in doc.entities() {
        match entity_to_geojson(entity, codepage) {
            EntityOutcome::Geometry(value) => {
                features.push(build_feature(
                    value,
                    decode_dwg_string(&entity.common().layer, codepage),
                    Vec::new(),
                ));
            }
            EntityOutcome::Annotated(value, extra) => {
                features.push(build_feature(
                    value,
                    decode_dwg_string(&entity.common().layer, codepage),
                    extra,
                ));
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

/// 构造要素（layer + 可选扩展属性）。
fn build_feature(
    geometry: geojson::Value,
    layer: String,
    extra: Vec<(String, serde_json::Value)>,
) -> geojson::Feature {
    let mut properties = serde_json::Map::new();
    properties.insert("layer".to_string(), serde_json::Value::String(layer));
    for (key, value) in extra {
        properties.insert(key, value);
    }
    geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(geometry)),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    }
}

/// 实体映射结果。
enum EntityOutcome {
    Geometry(geojson::Value),
    /// 标注要素（TEXT/MTEXT）：几何 + 扩展属性（feature_kind/text/height/rotation）。
    Annotated(geojson::Value, Vec<(String, serde_json::Value)>),
    /// 退化几何（dxf 同口径：不计要素、单独计数）。
    Degenerate,
    /// 按类型跳过（类型名）。
    Skip(String),
}

fn entity_to_geojson(entity: &EntityType, codepage: u16) -> EntityOutcome {
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
        EntityType::Ellipse(e) => ellipse_outcome(e),
        EntityType::Text(t) => annotation_outcome(
            &t.value,
            (t.insertion_point.x, t.insertion_point.y),
            t.height,
            t.rotation,
            codepage,
        ),
        EntityType::MText(t) => annotation_outcome(
            &t.value,
            (t.insertion_point.x, t.insertion_point.y),
            t.height,
            t.rotation,
            codepage,
        ),
        other => EntityOutcome::Skip(
            format!("{other:?}")
                .split('(')
                .next()
                .unwrap_or("Unknown")
                .to_string(),
        ),
    }
}

/// ELLIPSE → 64 段参数方程近似：全角（start≈end 或跨满 2π）→ 闭合
/// Polygon；部分弧 → LineString（与 ARC 同口径，acadrust 弧度制）。
/// 退化（ratio≤0、长轴≤0）计入 Degenerate。
fn ellipse_outcome(e: &acadrust::entities::Ellipse) -> EntityOutcome {
    let major = (e.major_axis.x, e.major_axis.y);
    let a = (major.0 * major.0 + major.1 * major.1).sqrt();
    if e.minor_axis_ratio <= 0.0 || a <= 0.0 {
        return EntityOutcome::Degenerate;
    }
    let full = (e.end_parameter - e.start_parameter).abs() >= std::f64::consts::TAU - 1e-9
        || (e.end_parameter - e.start_parameter).abs() < 1e-9;
    let positions = ellipse_to_positions(
        (e.center.x, e.center.y),
        major,
        e.minor_axis_ratio,
        e.start_parameter,
        if full {
            e.start_parameter + std::f64::consts::TAU
        } else {
            e.end_parameter
        },
        64,
    );
    if full {
        // 近似首尾同点，天然闭合。
        EntityOutcome::Geometry(geojson::Value::Polygon(vec![positions]))
    } else {
        EntityOutcome::Geometry(geojson::Value::LineString(positions))
    }
}

/// 椭圆参数方程采样：长半轴 a=|major|、短半轴 b=a·ratio、旋转角
/// α=atan2(major.y, major.x)；P(t)=C+R(α)·(a·cos t, b·sin t)。
/// 独立成 pure fn 便于数学正确性单测。
fn ellipse_to_positions(
    center: (f64, f64),
    major: (f64, f64),
    ratio: f64,
    start: f64,
    end: f64,
    segments: usize,
) -> Vec<Vec<f64>> {
    let a = (major.0 * major.0 + major.1 * major.1).sqrt();
    let b = a * ratio;
    let alpha = major.1.atan2(major.0);
    let (sin_a, cos_a) = alpha.sin_cos();
    (0..=segments)
        .map(|i| {
            let t = start + (end - start) * i as f64 / segments as f64;
            let (sin_t, cos_t) = t.sin_cos();
            let (ex, ey) = (a * cos_t, b * sin_t);
            vec![
                center.0 + ex * cos_a - ey * sin_a,
                center.1 + ex * sin_a + ey * cos_a,
            ]
        })
        .collect()
}

/// TEXT/MTEXT → 标注要素：插入点 Point；`feature_kind: "annotation"`、
/// 解码清洗后 `text`（decode_dwg_string + clean_mtext）、`height`、
/// `rotation`（弧度→度）。空文本（trim 后为空）计入 Degenerate（不产生脏数据）。
fn annotation_outcome(
    raw_text: &str,
    point: (f64, f64),
    height: f64,
    rotation_rad: f64,
    codepage: u16,
) -> EntityOutcome {
    let text = clean_mtext(&decode_dwg_string(raw_text, codepage));
    let text = text.trim();
    if text.is_empty() {
        return EntityOutcome::Degenerate;
    }
    EntityOutcome::Annotated(
        geojson::Value::Point(vec![point.0, point.1]),
        vec![
            (
                "feature_kind".to_string(),
                serde_json::Value::String("annotation".to_string()),
            ),
            (
                "text".to_string(),
                serde_json::Value::String(text.to_string()),
            ),
            ("height".to_string(), serde_json::Value::from(height)),
            (
                "rotation".to_string(),
                serde_json::Value::from(rotation_rad.to_degrees()),
            ),
        ],
    )
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

/// MTEXT 内联格式码**最小清洗**（保留内容、去控制码；注释即码表）：
/// - `{...}` 分组：去花括号保留内容（嵌套按深度配对）；
/// - `\P`/`\p` → 换行 `\n`；`~` 与 `\~` → 空格；`\\` → `\`；
/// - 样式参数码 `\f..\H..\W..\A..\C..\Q..\T..\X..\p` 及其 `;` 前缀整体丢弃
///   （如 `\fSimSun|b0|i0;`）；
/// - `\S上/下;` 堆叠：保留 `上/下` 内容（如 `\S1/2;` → `1/2`）；
/// - 其余 `\X` 单字符码丢反斜杠保字符。
///
/// 不求全覆盖（`\A` 对齐、`\C` 颜色等外观参数一律丢弃只留文本）——
/// 覆盖中文图纸常见码表，未知码以"保内容"兜底。
pub fn clean_mtext(raw: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Control {
        Newline,
        Space,
        Backslash,
        SkipToSemi, // 丢弃到 ';'（样式参数）
        KeepToSemi, // \S 堆叠：保留到 ';'
        Bare,       // 单字符码：丢 '\' 保字符
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let mut depth = 0i32;
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            '~' => out.push(' '),
            '\\' => {
                let ctrl = match chars.next() {
                    Some('P') | Some('p') => Some(Control::Newline),
                    Some('\\') => Some(Control::Backslash),
                    Some('~') => Some(Control::Space),
                    Some('S') => Some(Control::KeepToSemi),
                    Some('f' | 'F' | 'H' | 'W' | 'A' | 'C' | 'Q' | 'T' | 'X') => {
                        Some(Control::SkipToSemi)
                    }
                    Some(_) => Some(Control::Bare),
                    None => None,
                };
                match ctrl {
                    Some(Control::Newline) => out.push('\n'),
                    Some(Control::Space) => out.push(' '),
                    Some(Control::Backslash) => out.push('\\'),
                    Some(Control::SkipToSemi) => {
                        // 丢弃至 ';'（无 ';' 则丢至下一分隔符或结尾）。
                        for nc in chars.by_ref() {
                            if nc == ';' {
                                break;
                            }
                        }
                    }
                    Some(Control::KeepToSemi) => {
                        // \S 堆叠：保留到 ';' 的内容。
                        for nc in chars.by_ref() {
                            if nc == ';' {
                                break;
                            }
                            out.push(nc);
                        }
                    }
                    Some(Control::Bare) => {} // 单字符码已消费，丢 '\'。
                    None => out.push('\\'),
                }
            }
            _ => out.push(c),
        }
    }
    out
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
        // 跳过类型计数（spike 实测：insert 524 / hatch 99）。
        assert_eq!(stats.skipped_by_type.get("Insert"), Some(&524));
        assert_eq!(stats.skipped_by_type.get("Hatch"), Some(&99));
        // TEXT/MTEXT 已要素化：标注要素 == 645（spike 实测该文件 text 645），
        // 且不再出现在 skipped_by_type。
        assert!(!stats.skipped_by_type.contains_key("Text"));
        assert!(!stats.skipped_by_type.contains_key("MText"));
        let annotations: Vec<_> = collection
            .features
            .iter()
            .filter(|f| {
                f.properties
                    .as_ref()
                    .and_then(|p| p.get("feature_kind"))
                    .and_then(|v| v.as_str())
                    == Some("annotation")
            })
            .collect();
        assert_eq!(
            annotations.len(),
            645,
            "标注要素数应与 spike text 计数一致: {}",
            annotations.len()
        );
        for f in &annotations {
            let props = f.properties.as_ref().unwrap();
            let text = props["text"].as_str().unwrap_or_default();
            assert!(!text.trim().is_empty(), "标注 text 不应为空");
            assert!(props.get("height").is_some(), "标注应带 height");
            assert!(props.get("rotation").is_some(), "标注应带 rotation");
            assert!(props.get("layer").is_some(), "标注应带 layer");
            assert!(matches!(
                f.geometry.as_ref().unwrap().value,
                geojson::Value::Point(_)
            ));
        }
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

    #[test]
    fn clean_mtext_strips_format_codes() {
        // 字体码 + 分组。
        assert_eq!(clean_mtext("{\\fSimSun|b0|i0;界址线}"), "界址线");
        // \P 换行。
        assert_eq!(clean_mtext("A\\PB"), "A\nB");
        // ~ 与 \~ 空格。
        assert_eq!(clean_mtext("a~b"), "a b");
        assert_eq!(clean_mtext("a\\~b"), "a b");
        // \\ 反斜杠。
        assert_eq!(clean_mtext("a\\\\b"), "a\\b");
        // 嵌套分组（去花括号、全部内容保留——AutoCAD 分组只是格式化作用域）。
        assert_eq!(clean_mtext("{外{内}层}"), "外内层");
        // \H 字高参数码丢弃、内容保留。
        assert_eq!(clean_mtext("{\\H2.5;面积}"), "面积");
        // \S 堆叠保留内容。
        assert_eq!(clean_mtext("\\S1/2;"), "1/2");
        // 混合：字体 + 换行 + 分组。
        assert_eq!(clean_mtext("{\\f宋体;勘测定界\\P图廓}"), "勘测定界\n图廓");
    }

    #[test]
    fn ellipse_positions_math_and_closure() {
        // 标准椭圆：a=2、b=1（major 沿 x 轴），64+1 点、首点在 +x 端。
        let pts = ellipse_to_positions((0.0, 0.0), (2.0, 0.0), 0.5, 0.0, std::f64::consts::TAU, 64);
        assert_eq!(pts.len(), 65);
        assert!((pts[0][0] - 2.0).abs() < 1e-9 && pts[0][1].abs() < 1e-9);
        // t=π/2 点应在 +y 短轴端。
        assert!((pts[16][0]).abs() < 1e-9 && (pts[16][1] - 1.0).abs() < 1e-9);
        // 全角首尾闭合（浮点近似，sin(2π) 非精确 0）。
        assert!((pts[0][0] - pts[64][0]).abs() < 1e-9 && (pts[0][1] - pts[64][1]).abs() < 1e-9);
        // 旋转 90° 的椭圆（major 沿 y 轴）：首点在 +y 端。
        let rot = ellipse_to_positions((0.0, 0.0), (0.0, 2.0), 0.5, 0.0, std::f64::consts::TAU, 64);
        assert!(rot[0][0].abs() < 1e-9 && (rot[0][1] - 2.0).abs() < 1e-9);
        // 部分弧：0..π/2 → LineString 点数与端点。
        let arc = ellipse_to_positions(
            (1.0, 1.0),
            (2.0, 0.0),
            0.5,
            0.0,
            std::f64::consts::FRAC_PI_2,
            64,
        );
        assert!((arc[64][0] - 1.0).abs() < 1e-9 && (arc[64][1] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn dwg_ellipse_fixture_maps_to_geometry() {
        const ELLIPSE_FIXTURE: &str =
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sample_ellipse.dwg");
        let (collection, stats) = dwg_to_collection(ELLIPSE_FIXTURE).unwrap();
        // 该文件 16 实体含 1 个 ELLIPSE——椭圆近似为闭合 Polygon。
        let polys: Vec<_> = collection
            .features
            .iter()
            .filter(|f| {
                matches!(
                    f.geometry.as_ref().map(|g| &g.value),
                    Some(geojson::Value::Polygon(_))
                )
            })
            .collect();
        assert!(!polys.is_empty(), "应至少有 1 个 Polygon（椭圆近似）");
        assert!(!stats.skipped_by_type.contains_key("Ellipse"));
    }
}
