//! # kanyu-py —— 堪舆 Python 桥接（PyO3 扩展模块 `kanyu`）
//!
//! 裁决 #20：核心算法全部在 Rust（kanyu-core/kanyu-render），Python 经本模块
//! 调用——数据交换为 GeoJSON 文本（零宿主特定类型，跨语言最稳契约）。
//!
//! Python 侧用法：
//! ```python
//! import kanyu
//! fc = kanyu.load("buildings.geojson")
//! high = kanyu.query(fc, "height > 50")
//! buf = kanyu.buffer(high, 500.0)
//! kanyu.export(buf, "high_buffer.fgb", "fgb")
//! ```

use kanyu_core::{analysis, crs, geoprocess, KanyuError, Layer};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

/// 内核错误 → Python 异常。
fn to_py(e: KanyuError) -> PyErr {
    match e {
        KanyuError::InvalidQuery(_) => PyValueError::new_err(e.to_string()),
        other => PyRuntimeError::new_err(other.to_string()),
    }
}

/// JSON 文本 → FeatureCollection。
fn parse_fc(json: &str) -> PyResult<geojson::FeatureCollection> {
    let gj: geojson::GeoJson = json
        .parse()
        .map_err(|_| PyValueError::new_err("输入不是合法 GeoJSON 文本".to_string()))?;
    geojson::FeatureCollection::try_from(gj)
        .map_err(|e| PyValueError::new_err(format!("GeoJSON 非 FeatureCollection: {e}")))
}

/// FeatureCollection → JSON 文本。
fn fc_json(collection: &geojson::FeatureCollection) -> String {
    geojson::GeoJson::from(collection.clone()).to_string()
}

/// 加载数据文件，返回 GeoJSON 文本（格式自动探测：shp/geojson/fgb/
/// parquet/dxf/dwg/kml/kmz/csv/tsv/xlsx/txt/kdb）。
#[pyfunction]
fn load(path: &str) -> PyResult<String> {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string();
    let layer = Layer::load(stem, path).map_err(to_py)?;
    Ok(fc_json(&layer.collection()))
}

/// 属性查询（如 `height > 50`），返回结果 GeoJSON 文本。
#[pyfunction]
fn query(fc: &str, expression: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let layer = Layer::from_collection("py", collection);
    Ok(fc_json(&layer.query(expression).map_err(to_py)?))
}

/// 缓冲区分析（distance 为 CRS 单位；米制请先 reproject）。
#[pyfunction]
#[pyo3(signature = (fc, distance, segments=16))]
fn buffer(fc: &str, distance: f64, segments: usize) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &analysis::buffer(&collection, distance, segments).map_err(to_py)?,
    ))
}

/// 叠加分析（op: union/intersection/difference/xor；仅面要素）。
#[pyfunction]
fn overlay(target: &str, overlay: &str, operation: &str) -> PyResult<String> {
    let t = parse_fc(target)?;
    let o = parse_fc(overlay)?;
    let op = operation.parse::<analysis::OverlayOp>().map_err(to_py)?;
    Ok(fc_json(&analysis::overlay(&t, &o, op).map_err(to_py)?))
}

/// 拓扑检查（no_overlap），返回报告 JSON 文本。
#[pyfunction]
fn topology(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let report = analysis::topology_check(&collection, &[analysis::TopologyRule::NoOverlap])
        .map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 空间连接（左连接 + explode；predicate: intersects/contains/within）。
#[pyfunction]
fn sjoin(target: &str, join: &str, predicate: &str) -> PyResult<String> {
    let t = parse_fc(target)?;
    let j = parse_fc(join)?;
    let pred = predicate
        .parse::<analysis::SpatialPredicate>()
        .map_err(to_py)?;
    Ok(fc_json(&analysis::sjoin(&t, &j, pred).map_err(to_py)?))
}

/// 分区统计（zones 面 + values 值图层 + 数值字段 + 统计项列表）。
#[pyfunction]
fn zonal_stats(zones: &str, values: &str, field: &str, stats: Vec<String>) -> PyResult<String> {
    let z = parse_fc(zones)?;
    let v = parse_fc(values)?;
    let stats: std::result::Result<Vec<_>, _> = stats
        .iter()
        .map(|s| s.parse::<analysis::ZonalStat>())
        .collect();
    Ok(fc_json(
        &analysis::zonal_stats(&z, &v, field, &stats.map_err(to_py)?).map_err(to_py)?,
    ))
}

/// 融合（QGIS Dissolve）：按字段分组并集；field=None 全组融合。
#[pyfunction]
#[pyo3(signature = (fc, field=None))]
fn dissolve(fc: &str, field: Option<&str>) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::dissolve(&collection, field).map_err(to_py)?,
    ))
}

/// 道格拉斯简化（tolerance 为 CRS 单位）。
#[pyfunction]
fn simplify(fc: &str, tolerance: f64) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::simplify(&collection, tolerance).map_err(to_py)?,
    ))
}

/// 质心（逐要素 Point，属性随行）。
#[pyfunction]
fn centroid(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(&geoprocess::centroid(&collection).map_err(to_py)?))
}

/// 凸包（逐要素 Polygon）。
#[pyfunction]
fn convex_hull(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::convex_hull(&collection).map_err(to_py)?,
    ))
}

/// 删洞（min_area=None 删全部洞；否则仅删 < min_area 的洞）。
#[pyfunction]
#[pyo3(signature = (fc, min_area=None))]
fn delete_holes(fc: &str, min_area: Option<f64>) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::delete_holes(&collection, min_area).map_err(to_py)?,
    ))
}

/// 多部件炸开（Multi* → 单部件逐要素，属性复制）。
#[pyfunction]
fn explode(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(&geoprocess::explode(&collection).map_err(to_py)?))
}

/// 图层统计（测地线口径；含亩/公顷/平方千米），返回 JSON 文本。
#[pyfunction]
fn stats(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let report = geoprocess::stats(&collection).map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 测地线度量（kind: length|area，Karney 2013，米/平方米），返回 JSON 文本。
#[pyfunction]
fn measure(fc: &str, kind: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let kind = kind.parse::<crs::MeasureKind>().map_err(to_py)?;
    let report = crs::measure(&collection, kind).map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 投影变换（EPSG 全库，如 EPSG:4326 → EPSG:3857）。
#[pyfunction]
fn reproject(fc: &str, from: &str, to: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &crs::reproject(&collection, from, to).map_err(to_py)?,
    ))
}

/// 离屏渲染 PNG（返回 PNG 字节；theme: light|dark）。
#[pyfunction]
#[pyo3(signature = (fc, width=1200, height=800, theme="light", style=None))]
fn render_png(
    fc: &str,
    width: u32,
    height: u32,
    theme: &str,
    style: Option<&str>,
) -> PyResult<Vec<u8>> {
    let collection = parse_fc(fc)?;
    let theme = match theme {
        "dark" => kanyu_render::Theme::Dark,
        _ => kanyu_render::Theme::Light,
    };
    let style = match style {
        Some(s) if !s.trim().is_empty() => Some(
            serde_json::from_str::<kanyu_render::StyleRule>(s)
                .map_err(|e| PyValueError::new_err(format!("样式 JSON 解析失败: {e}")))?,
        ),
        _ => None,
    };
    let opts = kanyu_render::RenderOptions {
        width,
        height,
        theme,
        style,
        ..Default::default()
    };
    kanyu_render::render_png(&collection, &opts).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 离屏渲染 SVG（返回 SVG 文本）。
#[pyfunction]
#[pyo3(signature = (fc, width=1200, height=800, theme="light", style=None))]
fn render_svg(
    fc: &str,
    width: u32,
    height: u32,
    theme: &str,
    style: Option<&str>,
) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let theme = match theme {
        "dark" => kanyu_render::Theme::Dark,
        _ => kanyu_render::Theme::Light,
    };
    let style = match style {
        Some(s) if !s.trim().is_empty() => Some(
            serde_json::from_str::<kanyu_render::StyleRule>(s)
                .map_err(|e| PyValueError::new_err(format!("样式 JSON 解析失败: {e}")))?,
        ),
        _ => None,
    };
    let opts = kanyu_render::RenderOptions {
        width,
        height,
        theme,
        style,
        ..Default::default()
    };
    kanyu_render::render_svg(&collection, &opts).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 导出到文件（格式按 format 短名或 out 扩展名）。
#[pyfunction]
fn export(fc: &str, out: &str, format: &str) -> PyResult<()> {
    let collection = parse_fc(fc)?;
    let registry = kanyu_core::FormatRegistry::builtin();
    let caps = registry.require(format, "write").map_err(to_py)?;
    // 文件写入错误（io）与内核错误（KanyuError）分通道映射。
    let io_err = |e: std::io::Error| PyRuntimeError::new_err(format!("写入 {out} 失败: {e}"));
    match caps.id {
        "geojson" => std::fs::write(out, fc_json(&collection)).map_err(io_err)?,
        "csv" => std::fs::write(out, Layer::to_csv_string(&collection).map_err(to_py)?)
            .map_err(io_err)?,
        "fgb" => {
            std::fs::write(out, Layer::to_fgb_bytes(&collection).map_err(to_py)?).map_err(io_err)?
        }
        "geoparquet" => {
            std::fs::write(out, Layer::to_geoparquet_bytes(&collection).map_err(to_py)?)
                .map_err(io_err)?
        }
        "dxf" => std::fs::write(out, Layer::to_dxf_string(&collection).map_err(to_py)?)
            .map_err(io_err)?,
        "kml" => std::fs::write(out, Layer::to_kml_string(&collection).map_err(to_py)?)
            .map_err(io_err)?,
        "kmz" => {
            std::fs::write(out, Layer::to_kmz_bytes(&collection).map_err(to_py)?).map_err(io_err)?
        }
        "kdb" => {
            let layer = Layer::from_collection("py", collection);
            std::fs::write(out, layer.to_kdb_bytes().map_err(to_py)?).map_err(io_err)?
        }
        "shp" => Layer::write_shp(&collection, out.trim_end_matches(".shp")).map_err(to_py)?,
        other => {
            return Err(PyRuntimeError::new_err(format!(
                "格式 '{other}' 的导出未启用（driver: {}）",
                caps.driver
            )))
        }
    }
    Ok(())
}

/// 内核版本。
#[pyfunction]
fn version() -> &'static str {
    kanyu_core::VERSION
}

/// `kanyu` 扩展模块。
#[pymodule]
fn kanyu(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load, m)?)?;
    m.add_function(wrap_pyfunction!(query, m)?)?;
    m.add_function(wrap_pyfunction!(buffer, m)?)?;
    m.add_function(wrap_pyfunction!(overlay, m)?)?;
    m.add_function(wrap_pyfunction!(topology, m)?)?;
    m.add_function(wrap_pyfunction!(sjoin, m)?)?;
    m.add_function(wrap_pyfunction!(zonal_stats, m)?)?;
    m.add_function(wrap_pyfunction!(dissolve, m)?)?;
    m.add_function(wrap_pyfunction!(simplify, m)?)?;
    m.add_function(wrap_pyfunction!(centroid, m)?)?;
    m.add_function(wrap_pyfunction!(convex_hull, m)?)?;
    m.add_function(wrap_pyfunction!(delete_holes, m)?)?;
    m.add_function(wrap_pyfunction!(explode, m)?)?;
    m.add_function(wrap_pyfunction!(stats, m)?)?;
    m.add_function(wrap_pyfunction!(measure, m)?)?;
    m.add_function(wrap_pyfunction!(reproject, m)?)?;
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;
    m.add_function(wrap_pyfunction!(export, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add(
        "__doc__",
        "堪舆 Kanyu —— AI 原生地理空间操作系统（Rust 内核）",
    )?;
    Ok(())
}
