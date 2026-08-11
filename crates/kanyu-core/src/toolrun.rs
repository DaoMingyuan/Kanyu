//! 工具执行（自 kanyu-shell 下沉）：统一入口 [`run_tool`] + 参数校验
//! （[`validate`]/[`validate_param`]）+ 纯解析器（[`parse_number_list`]/
//! [`parse_extent`]/[`parse_positive`]）。
//!
//! `run_tool` 不触碰任何 UI 状态：图层访问经 `get_layer` 闭包注入，产出
//! [`ToolOutcome`]（新图层 / 多图层 / 终端报告）由调用方结算；
//! 「导出图层」分支直接经 [`crate::FormatRegistry`] + `Layer::to_*` 写盘。

use geojson::FeatureCollection;

use crate::tooldef::{find, ParamKind, ToolDef, ToolParam};
use crate::{analysis, crs, geoprocess, FormatRegistry, KanyuError, Layer};

/// 工具产出（调用方结算）。
#[derive(Debug)]
pub enum ToolOutcome {
    /// 新图层（要素集合 + 命名前缀 + 中文动词）。
    NewLayer {
        collection: FeatureCollection,
        base: String,
        verb: String,
    },
    /// 多产出新图层（如分割矢量图层：逐组一个，base 已含组值）。
    NewLayers {
        layers: Vec<(String, FeatureCollection)>,
        verb: String,
    },
    /// 报告文本（统计/检查/导出结果，调用方输出终端）。
    Report(String),
}

/// 图层导出落盘（格式按注册表能力矩阵；壳层/CLI 共用）。
fn export_collection(
    collection: &FeatureCollection,
    out: &str,
    fmt: &str,
) -> Result<String, String> {
    let registry = FormatRegistry::builtin();
    let caps = registry.require(fmt, "write").map_err(|e| e.to_string())?;
    match caps.id {
        "geojson" => {
            std::fs::write(out, Layer::to_geojson_string(collection)).map_err(|e| e.to_string())?
        }
        "csv" => std::fs::write(
            out,
            Layer::to_csv_string(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "fgb" => std::fs::write(
            out,
            Layer::to_fgb_bytes(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "geoparquet" => std::fs::write(
            out,
            Layer::to_geoparquet_bytes(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "dxf" => std::fs::write(
            out,
            Layer::to_dxf_string(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "kml" => std::fs::write(
            out,
            Layer::to_kml_string(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "kmz" => std::fs::write(
            out,
            Layer::to_kmz_bytes(collection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?,
        "shp" => {
            Layer::write_shp(collection, out.trim_end_matches(".shp")).map_err(|e| e.to_string())?
        }
        other => {
            return Err(format!(
                "格式 '{other}' 的导出在壳层未启用（driver: {}）",
                caps.driver
            ))
        }
    }
    Ok(format!(
        "已导出 {} 要素 → {out}（{fmt}）",
        collection.features.len()
    ))
}

// ===== 参数校验（纯函数）=====

/// 单参数校验（内联红字用）：值合法返回 None，否则返回首个中文错误。
/// 可选参数留空 = 合法（取默认语义）。
pub fn validate_param(p: &ToolParam, value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return if p.required {
            Some(format!("「{}」为必填参数", p.label))
        } else {
            None
        };
    }
    match &p.kind {
        ParamKind::Number => {
            if v.parse::<f64>().map(|f| !f.is_finite()).unwrap_or(true) {
                return Some(format!("「{}」须为数值: {v}", p.label));
            }
            None
        }
        ParamKind::NumberList => parse_number_list(v).map(|_| ()).err(),
        ParamKind::Enum(options) => {
            if options.iter().any(|(_, label)| *label == v) {
                None
            } else {
                Some(format!("「{}」取值非法: {v}", p.label))
            }
        }
        _ => None,
    }
}

/// 整表校验（执行前）：个数对齐 + 逐参数校验，返回首个错误。
pub fn validate(def: &ToolDef, values: &[String]) -> Result<(), String> {
    if values.len() != def.params.len() {
        return Err(format!(
            "参数个数不符（{} 个值 / {} 个参数）",
            values.len(),
            def.params.len()
        ));
    }
    for (p, v) in def.params.iter().zip(values) {
        if let Some(e) = validate_param(p, v) {
            return Err(e);
        }
    }
    Ok(())
}

/// 取参数值（按 key；调用前须已过 validate）。
fn value_of(def: &ToolDef, values: &[String], key: &str) -> String {
    def.params
        .iter()
        .zip(values)
        .find(|(p, _)| p.key == key)
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default()
}

/// 枚举参数的中文标签 → 内核值。
fn enum_value(def: &ToolDef, values: &[String], key: &str) -> String {
    let label = value_of(def, values, key);
    def.params
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match &p.kind {
            ParamKind::Enum(options) => options
                .iter()
                .find(|(_, l)| *l == label)
                .map(|(v, _)| v.to_string()),
            _ => None,
        })
        .unwrap_or(label)
}

// ===== 纯解析/校验辅助（注册表参数 → 内核入参）=====

/// 解析数值列表（逗号分隔）：非空、全部有限、非负、严格递增（多环缓冲语义）。
pub fn parse_number_list(raw: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let v: f64 = part
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("须为逗号分隔的数值列表，遇到: '{part}'"))?;
        if v < 0.0 {
            return Err(format!("距离须非负: {v}"));
        }
        if let Some(&prev) = out.last() {
            if v <= prev {
                return Err(format!("距离列表须严格递增: {prev} 之后出现 {v}"));
            }
        }
        out.push(v);
    }
    if out.is_empty() {
        return Err("距离列表不能为空".to_string());
    }
    Ok(out)
}

/// 解析范围四数 "minx,miny,maxx,maxy"（创建网格）。
pub fn parse_extent(raw: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
    if parts.len() != 4 {
        return Err(format!("范围须为 minx,miny,maxx,maxy 四个数值: '{raw}'"));
    }
    let mut out = [0.0; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("范围第 {} 个数须为数值: '{p}'", i + 1))?;
    }
    if out[0] >= out[2] || out[1] >= out[3] {
        return Err(format!("范围须满足 min<max: '{raw}'"));
    }
    Ok(out)
}

/// 解析"须为正数"的数值参数（凹度/间距/格距等）。
pub fn parse_positive(raw: &str, label: &str) -> Result<f64, String> {
    let v: f64 = raw
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}须为数值: {raw}"))?;
    if v <= 0.0 {
        return Err(format!("{label}须为正数: {v}"));
    }
    Ok(v)
}

// ===== 执行 =====

/// 统一执行入口。图层访问经 `get_layer(id)` 注入（返回图层要素集合的克隆）。
pub fn run_tool(
    id: &str,
    values: &[String],
    get_layer: impl Fn(&str) -> Option<FeatureCollection>,
) -> Result<ToolOutcome, String> {
    let def = find(id).ok_or_else(|| format!("未知工具: {id}"))?;
    validate(def, values)?;
    // 取图层（FeatureCollection 克隆——内核函数均为只读消费）。
    let layer = |key: &str| -> Result<FeatureCollection, String> {
        let lid = value_of(def, values, key);
        get_layer(&lid).ok_or_else(|| format!("图层不存在: {lid}"))
    };
    let new_layer = |collection: FeatureCollection, base: String| {
        Ok(ToolOutcome::NewLayer {
            collection,
            base,
            verb: def.name.to_string(),
        })
    };
    let src = value_of(def, values, "layer");
    match id {
        "buffer" => {
            let d: f64 = value_of(def, values, "distance")
                .parse()
                .map_err(|_| "距离须为数值".to_string())?;
            let c = analysis::buffer(&layer("layer")?, d, 16).map_err(|e| e.to_string())?;
            new_layer(c, format!("buf_{src}"))
        }
        // 裁剪/差值/对称差/联合 → 同一内核 overlay（op 不同）。
        "overlay_union" | "overlay_intersection" | "overlay_difference" | "overlay_xor" => {
            let op = match id {
                "overlay_union" => analysis::OverlayOp::Union,
                "overlay_intersection" => analysis::OverlayOp::Intersection,
                "overlay_difference" => analysis::OverlayOp::Difference,
                _ => analysis::OverlayOp::Xor,
            };
            let c = analysis::overlay(&layer("layer")?, &layer("overlay")?, op)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("ov_{src}"))
        }
        "sjoin" => {
            let pred: analysis::SpatialPredicate = enum_value(def, values, "predicate")
                .parse()
                .map_err(|e: KanyuError| e.to_string())?;
            let c = analysis::sjoin(&layer("layer")?, &layer("join")?, pred)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("sj_{src}"))
        }
        "count_points_in_polygon" => {
            let c = geoprocess::count_points_in_polygon(&layer("layer")?, &layer("points")?)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("cnt_{src}"))
        }
        "mean_coordinates" => {
            let w = value_of(def, values, "weight");
            let c = geoprocess::mean_coordinates(
                &layer("layer")?,
                if w.is_empty() { None } else { Some(w.as_str()) },
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("mean_{src}"))
        }
        "distance_matrix" => {
            let m = geoprocess::distance_matrix(&layer("layer")?, &layer("layer2")?)
                .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "距离矩阵 {} × {}（米）:\n{}",
                value_of(def, values, "layer"),
                value_of(def, values, "layer2"),
                serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?
            )))
        }
        "nearest_neighbor" => {
            let r = geoprocess::nearest_neighbor(&layer("layer")?).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "最近邻分析 {src}:\n{}",
                serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
            )))
        }
        "multi_ring_buffer" => {
            let distances = parse_number_list(&value_of(def, values, "distances"))?;
            let c = geoprocess::multi_ring_buffer(&layer("layer")?, &distances)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("rings_{src}"))
        }
        "variable_buffer" => {
            let segments = parse_positive(&value_of(def, values, "segments"), "圆弧分段数")? as u32;
            let c = geoprocess::variable_buffer(
                &layer("layer")?,
                &value_of(def, values, "field"),
                segments,
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("vbuf_{src}"))
        }
        "dissolve" => {
            let f = value_of(def, values, "field");
            let c = geoprocess::dissolve(
                &layer("layer")?,
                if f.is_empty() { None } else { Some(f.as_str()) },
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("dis_{src}"))
        }
        "centroid" => {
            let c = geoprocess::centroid(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("cen_{src}"))
        }
        "convex_hull" => {
            let c = geoprocess::convex_hull(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("hull_{src}"))
        }
        "simplify" => {
            let t: f64 = value_of(def, values, "tolerance")
                .parse()
                .map_err(|_| "容差须为数值".to_string())?;
            let c = geoprocess::simplify(&layer("layer")?, t).map_err(|e| e.to_string())?;
            new_layer(c, format!("sim_{src}"))
        }
        "delete_holes" => {
            let raw = value_of(def, values, "min_area");
            let min = if raw.is_empty() {
                None
            } else {
                Some(
                    raw.parse::<f64>()
                        .map_err(|_| "最小面积须为数值".to_string())?,
                )
            };
            let c = geoprocess::delete_holes(&layer("layer")?, min).map_err(|e| e.to_string())?;
            new_layer(c, format!("dh_{src}"))
        }
        "explode" => {
            let c = geoprocess::explode(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("exp_{src}"))
        }
        "boundary" => {
            let c = geoprocess::boundary(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("bnd_{src}"))
        }
        "bounding_boxes" => {
            let c = geoprocess::bounding_boxes(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("bbox_{src}"))
        }
        "points_along_lines" => {
            let d = parse_positive(&value_of(def, values, "distance"), "间距")?;
            let c =
                geoprocess::points_along_lines(&layer("layer")?, d).map_err(|e| e.to_string())?;
            new_layer(c, format!("pal_{src}"))
        }
        "concave_hull" => {
            let k = parse_positive(&value_of(def, values, "concavity"), "凹度")?;
            let c = geoprocess::concave_hull(&layer("layer")?, k).map_err(|e| e.to_string())?;
            new_layer(c, format!("chull_{src}"))
        }
        "minimum_rotated_rect" => {
            let c =
                geoprocess::minimum_rotated_rect(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("mrr_{src}"))
        }
        "topology_check" => {
            let report =
                analysis::topology_check(&layer("layer")?, &[analysis::TopologyRule::NoOverlap])
                    .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "拓扑检查 {src}:\n{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            )))
        }
        "extract_by_attribute" => {
            let c =
                geoprocess::extract_by_attribute(&layer("layer")?, &value_of(def, values, "expr"))
                    .map_err(|e| e.to_string())?;
            new_layer(c, format!("xat_{src}"))
        }
        "extract_by_location" => {
            let c = geoprocess::extract_by_location(
                &layer("layer")?,
                &layer("mask")?,
                &enum_value(def, values, "predicate"),
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("xloc_{src}"))
        }
        "query" => {
            // Layer::query 为方法：以临时 Layer 包装集合调用。
            let tmp = Layer::from_collection("q".to_string(), layer("layer")?);
            let c = tmp
                .query(&value_of(def, values, "expr"))
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("q_{src}"))
        }
        "merge" => {
            let (a, b) = (layer("layer")?, layer("layer2")?);
            let c = geoprocess::merge(&[&a, &b]).map_err(|e| e.to_string())?;
            new_layer(c, format!("mrg_{src}"))
        }
        "split_by_field" => {
            let groups =
                geoprocess::split_by_field(&layer("layer")?, &value_of(def, values, "field"))
                    .map_err(|e| e.to_string())?;
            // 多产出：每组一个新图层（split_源图层_组值；空串组用「空值」）。
            let layers = groups
                .into_iter()
                .map(|(g, c)| {
                    let g = if g.is_empty() {
                        "空值".to_string()
                    } else {
                        g
                    };
                    (format!("split_{src}_{g}"), c)
                })
                .collect();
            Ok(ToolOutcome::NewLayers {
                layers,
                verb: def.name.to_string(),
            })
        }
        "add_geometry_attributes" => {
            let c =
                geoprocess::add_geometry_attributes(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("gattr_{src}"))
        }
        "create_grid" => {
            let extent = parse_extent(&value_of(def, values, "extent"))?;
            let cell = parse_positive(&value_of(def, values, "cell_size"), "格距")?;
            let c = geoprocess::create_grid(extent, cell).map_err(|e| e.to_string())?;
            new_layer(c, "grid".to_string())
        }
        "reproject" => {
            let c = crs::reproject(
                &layer("layer")?,
                &value_of(def, values, "from"),
                &value_of(def, values, "to"),
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("rp_{src}"))
        }
        "export" => {
            // 导出分支落 core：注册表能力 + Layer::to_* 直接写盘。
            let c = layer("layer")?;
            let out = value_of(def, values, "out");
            let fmt = out.rsplit('.').next().unwrap_or("").to_string();
            Ok(ToolOutcome::Report(export_collection(&c, &out, &fmt)?))
        }
        "zonal_stats" => {
            let stats: Result<Vec<_>, _> = value_of(def, values, "stats")
                .split(',')
                .map(|s| s.trim().parse::<analysis::ZonalStat>())
                .collect();
            let stats = stats.map_err(|e: KanyuError| e.to_string())?;
            let c = analysis::zonal_stats(
                &layer("zones")?,
                &layer("values")?,
                &value_of(def, values, "field"),
                &stats,
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("zs_{}", value_of(def, values, "zones")))
        }
        "stats" => {
            let s = geoprocess::stats(&layer("layer")?).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "图层统计 {src}:\n{}",
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
            )))
        }
        "field_stats" => {
            let s = geoprocess::field_stats(&layer("layer")?, &value_of(def, values, "field"))
                .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "字段统计 {src}.{}:\n{}",
                value_of(def, values, "field"),
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
            )))
        }
        "measure" => {
            let kind: crs::MeasureKind = enum_value(def, values, "kind")
                .parse()
                .map_err(|e: KanyuError| e.to_string())?;
            let report = crs::measure(&layer("layer")?, kind).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "测地度量 {src}:\n{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            )))
        }
        other => Err(format!("工具未实现: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tooldef::find;

    fn empty() -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        }
    }

    /// 校验：必填留空报错。
    #[test]
    fn validate_required() {
        let buf = find("buffer").unwrap();
        let err = validate(buf, &[String::new(), "100".into()]).unwrap_err();
        assert!(err.contains("必填"));
        assert!(validate(buf, &["buildings".into(), "100".into()]).is_ok());
    }

    /// 校验：数值参数拒绝非数值；可选数值留空放行。
    #[test]
    fn validate_number() {
        let buf = find("buffer").unwrap();
        assert!(validate(buf, &["a".into(), "abc".into()]).is_err());
        let dh = find("delete_holes").unwrap();
        assert!(validate(dh, &["a".into(), String::new()]).is_ok());
        assert!(validate(dh, &["a".into(), "12.5".into()]).is_ok());
        assert!(validate(dh, &["a".into(), "xyz".into()]).is_err());
    }

    /// 单参数校验（内联红字语义）。
    #[test]
    fn validate_param_inline() {
        let buf = find("buffer").unwrap();
        assert!(validate_param(&buf.params[0], "").is_some()); // 必填空
        assert!(validate_param(&buf.params[1], "abc").is_some());
        assert!(validate_param(&buf.params[1], "100").is_none());
        let dh = find("delete_holes").unwrap();
        assert!(validate_param(&dh.params[1], "").is_none()); // 可选空
    }

    /// 校验：枚举只收中文标签；枚举值映射回内核值。
    #[test]
    fn validate_enum_and_mapping() {
        let sj = find("sjoin").unwrap();
        assert!(validate(sj, &["a".into(), "b".into(), "相交".into()]).is_ok());
        assert!(validate(sj, &["a".into(), "b".into(), "intersects".into()]).is_err());
        assert_eq!(
            enum_value(sj, &["a".into(), "b".into(), "相交".into()], "predicate"),
            "intersects"
        );
    }

    /// 校验：参数个数不符报错。
    #[test]
    fn validate_arity() {
        let cen = find("centroid").unwrap();
        assert!(validate(cen, &[]).is_err());
        assert!(validate(cen, &["a".into()]).is_ok());
    }

    /// 执行：未知工具与缺失图层的中文错误。
    #[test]
    fn run_tool_errors() {
        assert!(run_tool("nope", &[], |_| None)
            .unwrap_err()
            .contains("未知工具"));
        let err = run_tool("centroid", &["ghost".into()], |_| None).unwrap_err();
        assert!(err.contains("图层不存在"), "{err}");
    }

    /// 执行：report 类工具走终端报告分支（以空集合驱动 stats）。
    #[test]
    fn run_tool_report_branch() {
        let out = run_tool("stats", &["x".into()], |_| Some(empty())).unwrap();
        match out {
            ToolOutcome::Report(text) => assert!(text.contains("图层统计"), "{text}"),
            _ => panic!("stats 应为终端报告"),
        }
    }

    /// 执行：新图层分支命名前缀（以空集合驱动 centroid）。
    #[test]
    fn run_tool_new_layer_branch() {
        let out = run_tool("centroid", &["buildings".into()], |_| Some(empty())).unwrap();
        match out {
            ToolOutcome::NewLayer { base, verb, .. } => {
                assert_eq!(base, "cen_buildings");
                assert_eq!(verb, "质心");
            }
            _ => panic!("centroid 应为新图层"),
        }
    }

    /// 纯解析：数值列表（非空/非负/严格递增）。
    #[test]
    fn number_list_rules() {
        assert_eq!(
            parse_number_list("100, 200,300").unwrap(),
            vec![100.0, 200.0, 300.0]
        );
        assert!(parse_number_list("").is_err());
        assert!(parse_number_list("100,abc").is_err());
        assert!(parse_number_list("100,-5").is_err());
        assert!(parse_number_list("100,100").is_err());
        assert!(parse_number_list("200,100").is_err());
        let mrb = find("multi_ring_buffer").unwrap();
        assert!(validate(mrb, &["a".into(), "1,2,3".into()]).is_ok());
        assert!(validate(mrb, &["a".into(), "3,2".into()]).is_err());
    }

    /// 纯解析：范围四数与正数参数。
    #[test]
    fn extent_and_positive_rules() {
        assert_eq!(parse_extent("0,0, 10, 20").unwrap(), [0.0, 0.0, 10.0, 20.0]);
        assert!(parse_extent("0,0,10").is_err());
        assert!(parse_extent("0,0,10,x").is_err());
        assert!(parse_extent("10,0,0,20").is_err());
        assert_eq!(parse_positive("2.0", "凹度").unwrap(), 2.0);
        assert!(parse_positive("0", "凹度").is_err());
        assert!(parse_positive("-1", "格距").is_err());
        assert!(parse_positive("abc", "间距").is_err());
        let err =
            run_tool("concave_hull", &["a".into(), "0".into()], |_| Some(empty())).unwrap_err();
        assert!(err.contains("凹度"), "{err}");
    }

    /// 执行：分割矢量图层走多产出分支；空集合 → 0 组。
    #[test]
    fn run_tool_split_multi_output() {
        let out = run_tool("split_by_field", &["a".into(), "usage".into()], |_| {
            Some(empty())
        })
        .unwrap();
        match out {
            ToolOutcome::NewLayers { layers, verb } => {
                assert!(layers.is_empty());
                assert_eq!(verb, "分割矢量图层");
            }
            _ => panic!("split_by_field 应为多产出"),
        }
    }

    /// 执行：导出分支在 core 直接写盘（geojson 往返验证）。
    #[test]
    fn run_tool_export_writes_file() {
        let dir = std::env::temp_dir().join("kanyu_toolrun_test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("exp.geojson");
        let path = out.to_string_lossy().to_string();
        let result = run_tool("export", &["x".into(), path.clone()], |_| Some(empty())).unwrap();
        match result {
            ToolOutcome::Report(msg) => assert!(msg.contains("已导出"), "{msg}"),
            _ => panic!("export 应为报告"),
        }
        // 文件确实写出且为合法 GeoJSON。
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("FeatureCollection"));
        // 未知格式报中文错。
        let err = run_tool("export", &["x".into(), "a.unknown".into()], |_| {
            Some(empty())
        })
        .unwrap_err();
        assert!(!err.is_empty());
    }
}
