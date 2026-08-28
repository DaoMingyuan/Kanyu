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

use kanyu_core::{analysis, attrcalc, crs, geoprocess, tooldef, toolrun, KanyuError, Layer};
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
        "dat" => std::fs::write(
            out,
            Layer::to_cass_dat_string(&collection, 3).map_err(to_py)?,
        )
        .map_err(io_err)?,
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

// ===== geoprocess 第二批 =====

/// 边界（QGIS Boundary）：面→环转线、开放线→端点 MultiPoint，属性随行。
#[pyfunction]
fn boundary(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(&geoprocess::boundary(&collection).map_err(to_py)?))
}

/// 包络矩形（QGIS Bounding boxes）：逐要素最小外接矩形面。
#[pyfunction]
fn bounding_boxes(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::bounding_boxes(&collection).map_err(to_py)?,
    ))
}

/// 合并矢量图层（QGIS Merge vector layers）：多图层 GeoJSON 文本列表顺序拼接。
#[pyfunction]
fn merge(fcs: Vec<String>) -> PyResult<String> {
    let cols: Vec<geojson::FeatureCollection> =
        fcs.iter().map(|s| parse_fc(s)).collect::<PyResult<_>>()?;
    let refs: Vec<&geojson::FeatureCollection> = cols.iter().collect();
    Ok(fc_json(&geoprocess::merge(&refs).map_err(to_py)?))
}

/// 按属性提取（QGIS Extract by attribute）：表达式 "field op value"。
#[pyfunction]
fn extract_by_attribute(fc: &str, expression: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::extract_by_attribute(&collection, expression).map_err(to_py)?,
    ))
}

/// 按位置提取（QGIS Extract by location；predicate: intersects/contains/within）。
#[pyfunction]
fn extract_by_location(fc: &str, mask: &str, predicate: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let mask = parse_fc(mask)?;
    Ok(fc_json(
        &geoprocess::extract_by_location(&collection, &mask, predicate).map_err(to_py)?,
    ))
}

/// 面内点计数（QGIS Count points in polygon）：追加 NUMPOINTS 整数属性。
#[pyfunction]
fn count_points_in_polygon(polygons: &str, points: &str) -> PyResult<String> {
    let polys = parse_fc(polygons)?;
    let pts = parse_fc(points)?;
    Ok(fc_json(
        &geoprocess::count_points_in_polygon(&polys, &pts).map_err(to_py)?,
    ))
}

/// 字段基本统计（QGIS Basic statistics for fields），返回 JSON 文本。
#[pyfunction]
fn field_stats(fc: &str, field: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let report = geoprocess::field_stats(&collection, field).map_err(to_py)?;
    serde_json::to_string(&report).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 平均坐标（QGIS Mean coordinate(s)）：weight=None 不加权，单点要素含 MEAN_X/MEAN_Y。
#[pyfunction]
#[pyo3(signature = (fc, weight=None))]
fn mean_coordinates(fc: &str, weight: Option<&str>) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::mean_coordinates(&collection, weight).map_err(to_py)?,
    ))
}

// ===== geoprocess 第三批 =====

/// 距离矩阵（QGIS Distance matrix，测地线米），返回 JSON 文本。
#[pyfunction]
fn distance_matrix(a: &str, b: &str) -> PyResult<String> {
    let ca = parse_fc(a)?;
    let cb = parse_fc(b)?;
    let m = geoprocess::distance_matrix(&ca, &cb).map_err(to_py)?;
    serde_json::to_string(&m).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 最近邻分析（QGIS Nearest neighbour analysis），返回 JSON 文本。
#[pyfunction]
fn nearest_neighbor(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let r = geoprocess::nearest_neighbor(&collection).map_err(to_py)?;
    serde_json::to_string(&r).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 多环缓冲区（QGIS Multi-ring buffer）：距离列表严格递增非负，属性 RING/DISTANCE。
#[pyfunction]
fn multi_ring_buffer(fc: &str, distances: Vec<f64>) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::multi_ring_buffer(&collection, &distances).map_err(to_py)?,
    ))
}

/// 按字段缓冲区（QGIS Variable distance buffer）：距离取自数值字段，
/// 缺失/非数值/负值要素跳过（计入 foreign_members.skipped）。
#[pyfunction]
#[pyo3(signature = (fc, field, segments=16))]
fn variable_buffer(fc: &str, field: &str, segments: u32) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::variable_buffer(&collection, field, segments).map_err(to_py)?,
    ))
}

/// 分割矢量图层（QGIS Split vector layer）：按字段值分组，返回 JSON 文本
/// `[{"key": 组值, "collection": {...}}, …]`（BTreeMap 字典序，缺字段归空串组）。
#[pyfunction]
fn split_by_field(fc: &str, field: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let groups = geoprocess::split_by_field(&collection, field).map_err(to_py)?;
    let mut arr = Vec::with_capacity(groups.len());
    for (key, c) in groups {
        arr.push(serde_json::json!({
            "key": key,
            "collection": serde_json::to_value(&c)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?,
        }));
    }
    serde_json::to_string(&arr).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 添加几何属性（QGIS Add geometry attributes）：面 AREA_M2/PERIMETER_M、
/// 线 LENGTH_M（测地口径），点不追加。
#[pyfunction]
fn add_geometry_attributes(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::add_geometry_attributes(&collection).map_err(to_py)?,
    ))
}

/// 创建网格（QGIS Create grid 矩形格网）：extent=[minx,miny,maxx,maxy]，属性 ROW/COL。
#[pyfunction]
fn create_grid(extent: Vec<f64>, cell_size: f64) -> PyResult<String> {
    let extent: [f64; 4] = extent
        .try_into()
        .map_err(|e: Vec<f64>| PyValueError::new_err(format!("extent 须为 4 个数值: {e:?}")))?;
    Ok(fc_json(
        &geoprocess::create_grid(extent, cell_size).map_err(to_py)?,
    ))
}

/// 沿线等距点（QGIS Points along geometry）：每 distance 米一点（含起点），
/// 属性 DISTANCE 里程。
#[pyfunction]
fn points_along_lines(fc: &str, distance: f64) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::points_along_lines(&collection, distance).map_err(to_py)?,
    ))
}

/// 凹包（QGIS Concave hull）：整层点集凹包单面；concavity 越小越凹。
#[pyfunction]
fn concave_hull(fc: &str, concavity: f64) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::concave_hull(&collection, concavity).map_err(to_py)?,
    ))
}

/// 定向最小包络矩形（QGIS Oriented minimum bounding box）：逐要素矩形面。
#[pyfunction]
fn minimum_rotated_rect(fc: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &geoprocess::minimum_rotated_rect(&collection).map_err(to_py)?,
    ))
}

// ===== attrcalc（属性表字段计算）=====

/// 字段计算器：表达式写入目标字段（不存在则新建）。支持 [字段] 引用、
/// $area/$length/$x/$y 几何虚列与算术/函数/条件表达式。
#[pyfunction]
fn calc_field(fc: &str, target: &str, expression: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &attrcalc::calc_field(&collection, target, expression).map_err(to_py)?,
    ))
}

/// 新建字段：default 为 JSON 文本（如 "0"、"\"未命名\""），None 得 Null 默认。
#[pyfunction]
#[pyo3(signature = (fc, name, default=None))]
fn add_field(fc: &str, name: &str, default: Option<&str>) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    let default = default
        .map(|s| {
            serde_json::from_str::<serde_json::Value>(s)
                .map_err(|e| PyValueError::new_err(format!("default 须为 JSON 文本: {e}")))
        })
        .transpose()?;
    Ok(fc_json(
        &attrcalc::add_field(&collection, name, default).map_err(to_py)?,
    ))
}

/// 删除字段（不存在亦不报错）。
#[pyfunction]
fn delete_field(fc: &str, name: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &attrcalc::delete_field(&collection, name).map_err(to_py)?,
    ))
}

/// 重命名字段。
#[pyfunction]
fn rename_field(fc: &str, old: &str, new: &str) -> PyResult<String> {
    let collection = parse_fc(fc)?;
    Ok(fc_json(
        &attrcalc::rename_field(&collection, old, new).map_err(to_py)?,
    ))
}

// ===== CRS 检索 =====

/// EPSG 全库检索（7507 条）：代码子串或名称（大小写不敏感）；空查询返回常用精选。
#[pyfunction]
#[pyo3(signature = (query, limit=20))]
fn search_crs(query: &str, limit: usize) -> PyResult<String> {
    let found = crs::search_crs(query, limit);
    serde_json::to_string(&found).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 按代码查 EPSG 条目；不存在返回 "null"。
#[pyfunction]
fn crs_info(code: u32) -> PyResult<String> {
    serde_json::to_string(&crs::crs_info(code)).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 校验 CRS 定义可解析（EPSG:xxxx / proj4 串 / WGS84）。
#[pyfunction]
fn validate_crs(def: &str) -> bool {
    crs::validate_crs(def).is_ok()
}

// ===== 工具注册表与统一执行 =====

/// 工具注册表 JSON（37 个 ToolDef：id/中文名/分类/参数表/是否报告类）。
#[pyfunction]
fn toolbox_registry() -> PyResult<String> {
    serde_json::to_string(tooldef::TOOLS).map_err(|e| PyRuntimeError::new_err(e.to_string()))
}

/// 统一执行工具（toolrun 下沉入口）。
///
/// - `params_json`：`{参数键: 值}` 对象（按注册表参数序对齐；缺省取参数默认值；
///   枚举参数取**中文标签**，如图层参数取图层 id）；
/// - `layers_json`：`{图层id: GeoJSON 对象}` 映射；
/// - 返回 JSON：`{"type": "new_layer"|"new_layers", "verb": 中文动词,
///   "layers": {名称: GeoJSON 对象}}`（新图层集合由调用方命名落层），
///   报告类工具为 `{"type": "report", "report": 文本}`。
#[pyfunction]
fn run_tool(tool_id: &str, params_json: &str, layers_json: &str) -> PyResult<String> {
    let params: serde_json::Map<String, serde_json::Value> = serde_json::from_str(params_json)
        .map_err(|e| PyValueError::new_err(format!("params_json 须为对象: {e}")))?;
    let layers: serde_json::Map<String, serde_json::Value> = serde_json::from_str(layers_json)
        .map_err(|e| PyValueError::new_err(format!("layers_json 须为对象: {e}")))?;
    // 图层映射预解析为 FeatureCollection（get_layer 闭包克隆消费）。
    let mut collections: std::collections::HashMap<String, geojson::FeatureCollection> =
        std::collections::HashMap::new();
    for (id, v) in &layers {
        let text = serde_json::to_string(v).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        collections.insert(id.clone(), parse_fc(&text)?);
    }
    let def = tooldef::find(tool_id)
        .ok_or_else(|| PyValueError::new_err(format!("未知工具: {tool_id}")))?;
    // 注册表参数序 → values：显式值优先（字符串取本体，其余 JSON 文本化），
    // 缺省取参数默认值。
    let values: Vec<String> = def
        .params
        .iter()
        .map(|p| match params.get(p.key) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => p.default.to_string(),
        })
        .collect();
    let outcome = toolrun::run_tool(tool_id, &values, |id| collections.get(id).cloned())
        .map_err(PyRuntimeError::new_err)?;
    // 产出结算：新图层（单个/多个）以 {名称: GeoJSON 对象} 返回，报告为文本。
    let layer_value = |c: &geojson::FeatureCollection| {
        serde_json::to_value(c).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    };
    let json = match outcome {
        toolrun::ToolOutcome::NewLayer {
            collection,
            base,
            verb,
        } => serde_json::json!({
            "type": "new_layer",
            "verb": verb,
            "layers": { base: layer_value(&collection)? },
        }),
        toolrun::ToolOutcome::NewLayers { layers, verb } => {
            let mut map = serde_json::Map::new();
            for (name, c) in &layers {
                map.insert(name.clone(), layer_value(c)?);
            }
            serde_json::json!({ "type": "new_layers", "verb": verb, "layers": map })
        }
        toolrun::ToolOutcome::Report(text) => {
            serde_json::json!({ "type": "report", "report": text })
        }
    };
    serde_json::to_string(&json).map_err(|e| PyRuntimeError::new_err(e.to_string()))
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
    // geoprocess 第二批。
    m.add_function(wrap_pyfunction!(boundary, m)?)?;
    m.add_function(wrap_pyfunction!(bounding_boxes, m)?)?;
    m.add_function(wrap_pyfunction!(merge, m)?)?;
    m.add_function(wrap_pyfunction!(extract_by_attribute, m)?)?;
    m.add_function(wrap_pyfunction!(extract_by_location, m)?)?;
    m.add_function(wrap_pyfunction!(count_points_in_polygon, m)?)?;
    m.add_function(wrap_pyfunction!(field_stats, m)?)?;
    m.add_function(wrap_pyfunction!(mean_coordinates, m)?)?;
    // geoprocess 第三批。
    m.add_function(wrap_pyfunction!(distance_matrix, m)?)?;
    m.add_function(wrap_pyfunction!(nearest_neighbor, m)?)?;
    m.add_function(wrap_pyfunction!(multi_ring_buffer, m)?)?;
    m.add_function(wrap_pyfunction!(variable_buffer, m)?)?;
    m.add_function(wrap_pyfunction!(split_by_field, m)?)?;
    m.add_function(wrap_pyfunction!(add_geometry_attributes, m)?)?;
    m.add_function(wrap_pyfunction!(create_grid, m)?)?;
    m.add_function(wrap_pyfunction!(points_along_lines, m)?)?;
    m.add_function(wrap_pyfunction!(concave_hull, m)?)?;
    m.add_function(wrap_pyfunction!(minimum_rotated_rect, m)?)?;
    // attrcalc / crs 检索 / 工具注册表与统一执行。
    m.add_function(wrap_pyfunction!(calc_field, m)?)?;
    m.add_function(wrap_pyfunction!(add_field, m)?)?;
    m.add_function(wrap_pyfunction!(delete_field, m)?)?;
    m.add_function(wrap_pyfunction!(rename_field, m)?)?;
    m.add_function(wrap_pyfunction!(search_crs, m)?)?;
    m.add_function(wrap_pyfunction!(crs_info, m)?)?;
    m.add_function(wrap_pyfunction!(validate_crs, m)?)?;
    m.add_function(wrap_pyfunction!(toolbox_registry, m)?)?;
    m.add_function(wrap_pyfunction!(run_tool, m)?)?;
    m.add(
        "__doc__",
        "堪舆 Kanyu —— AI 原生地理空间操作系统（Rust 内核）",
    )?;
    Ok(())
}
