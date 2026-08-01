//! 图层内存模型。
//!
//! v0.1 以 GeoJSON FeatureCollection 为原生载体；后续版本将迁移到
//! GeoArrow RecordBatch 零拷贝模型（见 docs/MASTERPLAN.md 第三部分），
//! 本模块的 `Layer` API 面向该演进设计，调用方无需感知底层存储切换。

use serde::Serialize;

use crate::error::{KanyuError, Result};
use crate::format::FormatRegistry;

/// 图层概要信息（供 CLI / MCP 工具返回）。
#[derive(Debug, Clone, Serialize)]
pub struct LayerSummary {
    /// 图层标识。
    pub id: String,
    /// 来源格式。
    pub format: String,
    /// 要素数量。
    pub feature_count: usize,
    /// 几何类型集合（去重）。
    pub geometry_types: Vec<String>,
    /// 属性字段名集合（去重、排序）。
    pub fields: Vec<String>,
}

/// 一个已加载的矢量图层。
pub struct Layer {
    id: String,
    format: String,
    collection: geojson::FeatureCollection,
}

impl Layer {
    /// 从文件加载图层。格式自动探测；仅原生驱动格式可直接加载，
    /// 桥接驱动（GDAL/LibreDWG）格式返回 [`KanyuError::UnsupportedOperation`]，
    /// 等待对应 bridge feature 启用。
    pub fn load(id: impl Into<String>, path: &str) -> Result<Self> {
        let id = id.into();
        let registry = FormatRegistry::builtin();
        let caps = registry
            .detect(path)
            .ok_or_else(|| KanyuError::UnknownFormat(path.to_string()))?;
        registry.require(caps.id, "read")?;

        let collection = match caps.id {
            "geojson" => {
                let text = std::fs::read_to_string(path)?;
                let gj: geojson::GeoJson = text.parse()?;
                geojson::FeatureCollection::try_from(gj)
                    .map_err(|e| KanyuError::GeoJson(e.to_string()))?
            }
            "csv" => {
                if path.to_ascii_lowercase().ends_with(".xlsx") {
                    return Err(KanyuError::UnsupportedOperation {
                        format: "xlsx".to_string(),
                        operation: "native-load（xlsx 二进制解析待 calamine 集成，请先转 CSV）"
                            .to_string(),
                    });
                }
                let delimiter = if path.to_ascii_lowercase().ends_with(".tsv") {
                    b'\t'
                } else {
                    b','
                };
                let text = std::fs::read_to_string(path)?;
                csv_to_collection(&text, delimiter)?
            }
            "shp" => shp_to_collection(path)?,
            other => {
                return Err(KanyuError::UnsupportedOperation {
                    format: other.to_string(),
                    operation: "native-load (bridge driver not enabled)".to_string(),
                })
            }
        };
        Ok(Self {
            id,
            format: caps.id.to_string(),
            collection,
        })
    }

    /// 图层标识。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 要素数量。
    pub fn len(&self) -> usize {
        self.collection.features.len()
    }

    /// 是否为空图层。
    pub fn is_empty(&self) -> bool {
        self.collection.features.is_empty()
    }

    /// 概要信息。
    pub fn summary(&self) -> LayerSummary {
        let mut geometry_types: Vec<String> = self
            .collection
            .features
            .iter()
            .filter_map(|f| f.geometry.as_ref().map(|g| g.value.type_name().to_string()))
            .collect();
        geometry_types.sort();
        geometry_types.dedup();

        let mut fields: Vec<String> = self
            .collection
            .features
            .iter()
            .filter_map(|f| f.properties.as_ref())
            .flat_map(|p| p.keys().cloned())
            .collect();
        fields.sort();
        fields.dedup();

        LayerSummary {
            id: self.id.clone(),
            format: self.format.clone(),
            feature_count: self.len(),
            geometry_types,
            fields,
        }
    }

    /// 访问底层要素集合（只读）。
    pub fn collection(&self) -> &geojson::FeatureCollection {
        &self.collection
    }

    /// 属性查询：支持简单比较表达式 `"field op value"`，
    /// op ∈ `==` `!=` `>` `>=` `<` `<=`。数值字段按数值比较，其余按字符串。
    ///
    /// 例：`"height > 50"`、`"usage == residential"`。
    pub fn query(&self, expression: &str) -> Result<geojson::FeatureCollection> {
        let predicate = Predicate::parse(expression)?;
        let features = self
            .collection
            .features
            .iter()
            .filter(|f| predicate.matches(f))
            .cloned()
            .collect();
        Ok(geojson::FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        })
    }

    /// 导出为 GeoJSON 字符串。
    pub fn to_geojson_string(collection: &geojson::FeatureCollection) -> String {
        geojson::GeoJson::from(collection.clone()).to_string()
    }

    /// 导出为 CSV 字符串：`x,y` 两列坐标（仅 Point 几何取值，其余留空），
    /// 后接全部属性字段的并集（排序去重）。
    pub fn to_csv_string(collection: &geojson::FeatureCollection) -> Result<String> {
        collection_to_csv(collection)
    }
}

/// CSV/TSV → FeatureCollection：自动识别坐标列（lon/lat/x/y/经度/纬度），
/// 其余列作为属性；数值型单元格自动转为 JSON 数值。
fn csv_to_collection(text: &str, delimiter: u8) -> Result<geojson::FeatureCollection> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = rdr
        .headers()
        .map_err(|e| KanyuError::Other(format!("csv 表头解析失败: {e}")))?
        .clone();
    let (x_idx, y_idx) = detect_coord_columns(&headers).ok_or_else(|| {
        KanyuError::Other(format!(
            "CSV 缺少可识别的坐标列（支持 lon/lat、longitude/latitude、x/y、经度/纬度）；实际表头: {}",
            headers.iter().collect::<Vec<_>>().join(", ")
        ))
    })?;

    let mut features = Vec::new();
    for (row_no, record) in rdr.records().enumerate() {
        let record = record
            .map_err(|e| KanyuError::Other(format!("csv 第 {} 行解析失败: {e}", row_no + 2)))?;
        let x: f64 = record
            .get(x_idx)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| KanyuError::Other(format!("csv 第 {} 行 X 坐标不是数值", row_no + 2)))?;
        let y: f64 = record
            .get(y_idx)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| KanyuError::Other(format!("csv 第 {} 行 Y 坐标不是数值", row_no + 2)))?;

        let mut properties = serde_json::Map::new();
        for (i, header) in headers.iter().enumerate() {
            if i == x_idx || i == y_idx {
                continue;
            }
            if let Some(raw) = record.get(i) {
                // 数值优先，退化为字符串。
                let value = raw
                    .parse::<f64>()
                    .map(serde_json::Value::from)
                    .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
                properties.insert(header.to_string(), value);
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

    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// Shapefile → FeatureCollection：几何按类型映射（Point/MultiPoint/Polyline/
/// Polygon 及其 M/Z 变体；PointZ 的 z 保留为第三坐标，其余 M/Z 附加量舍弃），
/// dbase 属性类型化为 JSON（空值跳过）。不支持的几何（Multipatch/NullShape）
/// 整条要素跳过，不留脏属性。
fn shp_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    let mut reader = shapefile::Reader::from_path(path).map_err(|e| {
        KanyuError::Other(format!(
            "shapefile 读取失败（{path}）：{e}；请确认 .shp/.shx/.dbf 侧边文件齐备且未损坏"
        ))
    })?;

    let mut features = Vec::new();
    for (rec_no, item) in reader.iter_shapes_and_records().enumerate() {
        let (shape, record) = item.map_err(|e| {
            KanyuError::Other(format!("shapefile 第 {} 条记录解析失败: {e}", rec_no + 1))
        })?;
        let Some(geometry) = shp_shape_to_geojson(&shape) else {
            continue;
        };
        let mut properties = serde_json::Map::new();
        for (name, value) in record {
            if let Some(json) = dbase_field_to_json(&value) {
                properties.insert(name, json);
            }
        }
        features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(geometry)),
            id: None,
            properties: Some(properties),
            foreign_members: None,
        });
    }

    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// Shape → GeoJSON 几何值；不支持的类型返回 None。
fn shp_shape_to_geojson(shape: &shapefile::Shape) -> Option<geojson::Value> {
    use shapefile::Shape;
    match shape {
        Shape::Point(p) => Some(geojson::Value::Point(vec![p.x, p.y])),
        Shape::PointM(p) => Some(geojson::Value::Point(vec![p.x, p.y])),
        Shape::PointZ(p) => Some(geojson::Value::Point(vec![p.x, p.y, p.z])),
        Shape::Multipoint(mp) => Some(geojson::Value::MultiPoint(shp_positions(mp.points()))),
        Shape::MultipointM(mp) => Some(geojson::Value::MultiPoint(shp_positions(mp.points()))),
        Shape::MultipointZ(mp) => Some(geojson::Value::MultiPoint(shp_positions(mp.points()))),
        Shape::Polyline(pl) => Some(shp_parts_to_geojson(pl.parts())),
        Shape::PolylineM(pl) => Some(shp_parts_to_geojson(pl.parts())),
        Shape::PolylineZ(pl) => Some(shp_parts_to_geojson(pl.parts())),
        Shape::Polygon(pg) => Some(shp_rings_to_geojson(pg.rings())),
        Shape::PolygonM(pg) => Some(shp_rings_to_geojson(pg.rings())),
        Shape::PolygonZ(pg) => Some(shp_rings_to_geojson(pg.rings())),
        // Multipatch / NullShape：无 GeoJSON 对应，跳过。
        _ => None,
    }
}

/// 点列 → GeoJSON 位置数组（仅取 x/y）。
fn shp_positions<P: shapefile::record::traits::HasXY>(points: &[P]) -> Vec<Vec<f64>> {
    points.iter().map(|p| vec![p.x(), p.y()]).collect()
}

/// Polyline parts → 单 part 为 LineString，多 part 为 MultiLineString。
fn shp_parts_to_geojson<P: shapefile::record::traits::HasXY>(parts: &[Vec<P>]) -> geojson::Value {
    let lines: Vec<Vec<Vec<f64>>> = parts.iter().map(|part| shp_positions(part)).collect();
    if lines.len() == 1 {
        geojson::Value::LineString(lines.into_iter().next().unwrap_or_default())
    } else {
        geojson::Value::MultiLineString(lines)
    }
}

/// Polygon rings → 每个外环开启一个多边形，后续内环（洞）挂到最近的外环；
/// 单外环为 Polygon，多外环为 MultiPolygon。畸形文件（内环先于外环）丢弃该环。
fn shp_rings_to_geojson<P: shapefile::record::traits::HasXY>(
    rings: &[shapefile::PolygonRing<P>],
) -> geojson::Value {
    let mut polygons: Vec<Vec<Vec<Vec<f64>>>> = Vec::new();
    for ring in rings {
        let (points, is_outer) = match ring {
            shapefile::PolygonRing::Outer(pts) => (pts, true),
            shapefile::PolygonRing::Inner(pts) => (pts, false),
        };
        let line = shp_positions(points);
        if is_outer {
            polygons.push(vec![line]);
        } else if let Some(last) = polygons.last_mut() {
            last.push(line);
        }
    }
    match polygons.len() {
        0 => geojson::Value::Polygon(Vec::new()),
        1 => geojson::Value::Polygon(polygons.pop().unwrap_or_default()),
        _ => geojson::Value::MultiPolygon(polygons),
    }
}

/// dbase 字段值 → JSON 值；空值（NULL）返回 None 以跳过。
fn dbase_field_to_json(value: &shapefile::dbase::FieldValue) -> Option<serde_json::Value> {
    use shapefile::dbase::FieldValue;
    match value {
        FieldValue::Numeric(v) => v.map(serde_json::Value::from),
        FieldValue::Float(v) => v.map(serde_json::Value::from),
        FieldValue::Double(v) => Some(serde_json::Value::from(*v)),
        FieldValue::Character(v) => v.clone().map(serde_json::Value::String),
        FieldValue::Logical(v) => v.map(serde_json::Value::Bool),
        FieldValue::Date(v) => v.map(|d| serde_json::Value::String(d.to_string())),
        FieldValue::DateTime(dt) => {
            let t = dt.time();
            Some(serde_json::Value::String(format!(
                "{} {:02}:{:02}:{:02}",
                dt.date(),
                t.hours(),
                t.minutes(),
                t.seconds()
            )))
        }
        FieldValue::Memo(s) => Some(serde_json::Value::String(s.clone())),
        FieldValue::Integer(i) => Some(serde_json::Value::from(*i)),
        FieldValue::Currency(c) => Some(serde_json::Value::from(*c)),
    }
}

/// FeatureCollection → CSV：`x,y` 坐标列（仅 Point 取值）+ 属性字段并集。
fn collection_to_csv(collection: &geojson::FeatureCollection) -> Result<String> {
    // 属性字段并集（排序去重），保证列序稳定。
    let mut fields: Vec<String> = collection
        .features
        .iter()
        .filter_map(|f| f.properties.as_ref())
        .flat_map(|p| p.keys().cloned())
        .collect();
    fields.sort();
    fields.dedup();

    let mut wtr = csv::Writer::from_writer(Vec::new());
    let mut headers = vec!["x".to_string(), "y".to_string()];
    headers.extend(fields.iter().cloned());
    wtr.write_record(&headers)
        .map_err(|e| KanyuError::Other(format!("csv 写出失败: {e}")))?;

    for feature in &collection.features {
        let (x, y) = match feature.geometry.as_ref().map(|g| &g.value) {
            Some(geojson::Value::Point(coords)) => (
                coords.first().map(|v| v.to_string()).unwrap_or_default(),
                coords.get(1).map(|v| v.to_string()).unwrap_or_default(),
            ),
            _ => (String::new(), String::new()),
        };
        let mut record = vec![x, y];
        for field in &fields {
            let cell = feature
                .properties
                .as_ref()
                .and_then(|p| p.get(field))
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            record.push(cell);
        }
        wtr.write_record(&record)
            .map_err(|e| KanyuError::Other(format!("csv 写出失败: {e}")))?;
    }

    let bytes = wtr
        .into_inner()
        .map_err(|e| KanyuError::Other(format!("csv 写出失败: {e}")))?;
    String::from_utf8(bytes).map_err(|e| KanyuError::Other(format!("csv 编码失败: {e}")))
}

/// 在表头中定位 X/Y 坐标列（大小写不敏感）。
fn detect_coord_columns(headers: &csv::StringRecord) -> Option<(usize, usize)> {
    const X_NAMES: [&str; 5] = ["lon", "lng", "longitude", "x", "经度"];
    const Y_NAMES: [&str; 4] = ["lat", "latitude", "y", "纬度"];
    let find = |names: &[&str]| {
        headers.iter().position(|h| {
            let h = h.trim().to_ascii_lowercase();
            names.contains(&h.as_str())
        })
    };
    Some((find(&X_NAMES)?, find(&Y_NAMES)?))
}

/// 比较运算符。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

/// 编译后的属性谓词。
struct Predicate {
    field: String,
    op: Op,
    value: serde_json::Value,
}

impl Predicate {
    fn parse(expression: &str) -> Result<Self> {
        const OPS: [(&str, Op); 6] = [
            (">=", Op::Ge),
            ("<=", Op::Le),
            ("!=", Op::Ne),
            ("==", Op::Eq),
            (">", Op::Gt),
            ("<", Op::Lt),
        ];
        for (token, op) in OPS {
            if let Some((lhs, rhs)) = expression.split_once(token) {
                let field = lhs.trim().to_string();
                let raw = rhs.trim().trim_matches('"').trim_matches('\'');
                if field.is_empty() || raw.is_empty() {
                    break;
                }
                // 数值优先，其次布尔，退化为字符串。
                let value = raw
                    .parse::<f64>()
                    .map(serde_json::Value::from)
                    .or_else(|_| raw.parse::<bool>().map(serde_json::Value::from))
                    .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
                return Ok(Self { field, op, value });
            }
        }
        Err(KanyuError::InvalidQuery(expression.to_string()))
    }

    fn matches(&self, feature: &geojson::Feature) -> bool {
        let Some(actual) = feature.properties.as_ref().and_then(|p| p.get(&self.field)) else {
            return false;
        };
        match (actual, &self.value) {
            (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                let (a, b) = (
                    a.as_f64().unwrap_or(f64::NAN),
                    b.as_f64().unwrap_or(f64::NAN),
                );
                match self.op {
                    Op::Eq => a == b,
                    Op::Ne => a != b,
                    Op::Gt => a > b,
                    Op::Ge => a >= b,
                    Op::Lt => a < b,
                    Op::Le => a <= b,
                }
            }
            _ => {
                let ord = actual.to_string().cmp(&self.value.to_string());
                match self.op {
                    Op::Eq => actual == &self.value,
                    Op::Ne => actual != &self.value,
                    Op::Gt => ord.is_gt(),
                    Op::Ge => ord.is_gt() || ord.is_eq(),
                    Op::Lt => ord.is_lt(),
                    Op::Le => ord.is_lt() || ord.is_eq(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_layer() -> Layer {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},
                 "properties":{"name":"a","height":80.0,"usage":"office"}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.40,39.91]},
                 "properties":{"name":"b","height":30.0,"usage":"residential"}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{"name":"c","height":55.0,"usage":"office"}}
            ]
        }"#
        .parse()
        .unwrap();
        let collection = geojson::FeatureCollection::try_from(gj).unwrap();
        Layer {
            id: "buildings".into(),
            format: "geojson".into(),
            collection,
        }
    }

    #[test]
    fn summary_reports_counts_and_fields() {
        let s = sample_layer().summary();
        assert_eq!(s.feature_count, 3);
        assert_eq!(s.fields, vec!["height", "name", "usage"]);
        assert_eq!(s.geometry_types, vec!["LineString", "Point"]);
    }

    #[test]
    fn query_numeric_comparison() {
        let layer = sample_layer();
        let out = layer.query("height > 50").unwrap();
        assert_eq!(out.features.len(), 2);
    }

    #[test]
    fn query_string_equality() {
        let layer = sample_layer();
        let out = layer.query("usage == residential").unwrap();
        assert_eq!(out.features.len(), 1);
    }

    #[test]
    fn query_rejects_garbage() {
        assert!(sample_layer().query("not a query").is_err());
    }

    #[test]
    fn csv_load_detects_coord_columns() {
        let dir = std::env::temp_dir().join("kanyu_core_csv_ok");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pts.csv");
        std::fs::write(
            &path,
            "name,lon,lat,height\n甲,116.39,39.90,80\n乙,116.40,39.91,30\n",
        )
        .unwrap();
        let layer = Layer::load("pts", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 2);
        let s = layer.summary();
        assert_eq!(s.format, "csv");
        assert_eq!(s.geometry_types, vec!["Point"]);
        // 坐标列不进入属性字段。
        assert_eq!(s.fields, vec!["height", "name"]);
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 1);
    }

    #[test]
    fn csv_without_coord_columns_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_csv_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("no_coords.csv");
        std::fs::write(&path, "a,b\n1,2\n").unwrap();
        let err = Layer::load("bad", path.to_str().unwrap())
            .err()
            .expect("缺少坐标列应报错");
        assert!(
            err.to_string().contains("坐标列"),
            "错误应指出坐标列问题: {err}"
        );
    }

    #[test]
    fn csv_export_roundtrips_through_loader() {
        let layer = sample_layer();
        // CSV 仅承载 Point 几何：先过滤掉 LineString（name=c）。
        let points_only = layer.query("name != c").unwrap();
        let text = Layer::to_csv_string(&points_only).unwrap();
        // 表头：x,y + 属性并集。
        assert!(text.lines().next().unwrap().starts_with("x,y,"));
        assert!(text.contains("height"));
        // 写回临时文件再加载，闭环验证。
        let dir = std::env::temp_dir().join("kanyu_core_csv_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rt.csv");
        std::fs::write(&path, &text).unwrap();
        let reloaded = Layer::load("rt", path.to_str().unwrap()).unwrap();
        assert_eq!(reloaded.len(), 2);
        // 数值属性在往返后仍可比较查询。
        assert_eq!(reloaded.query("height > 50").unwrap().features.len(), 1);
    }

    /// 在临时目录写出测试 shapefile：sites.shp（2 个 Point，name/height 字段）
    /// 与 zones.shp（1 个带洞 Polygon）。shapefile 规范单文件单几何类型，故分两个文件。
    fn write_test_shps(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
        use shapefile::dbase::{FieldValue, Record, TableWriterBuilder};

        std::fs::create_dir_all(dir).unwrap();
        let table = || {
            TableWriterBuilder::new()
                .add_character_field("name".try_into().unwrap(), 50)
                .add_numeric_field("height".try_into().unwrap(), 18, 5)
        };
        let record = |name: &str, height: f64| {
            let mut rec = Record::default();
            rec.insert(
                "name".to_string(),
                FieldValue::Character(Some(name.to_string())),
            );
            rec.insert("height".to_string(), FieldValue::Numeric(Some(height)));
            rec
        };

        let pts_path = dir.join("sites.shp");
        let mut wtr = shapefile::Writer::from_path(&pts_path, table()).unwrap();
        wtr.write_shape_and_record(&shapefile::Point::new(116.39, 39.90), &record("甲", 80.0))
            .unwrap();
        wtr.write_shape_and_record(&shapefile::Point::new(116.40, 39.91), &record("乙", 30.0))
            .unwrap();

        let poly_path = dir.join("zones.shp");
        let mut wtr = shapefile::Writer::from_path(&poly_path, table()).unwrap();
        // 外环 + 内环（洞）；with_rings 自动闭合环并按 ESRI 规范整向，无需手写绕向。
        let polygon = shapefile::Polygon::with_rings(vec![
            shapefile::PolygonRing::Outer(vec![
                shapefile::Point::new(0.0, 0.0),
                shapefile::Point::new(0.0, 10.0),
                shapefile::Point::new(10.0, 10.0),
                shapefile::Point::new(10.0, 0.0),
            ]),
            shapefile::PolygonRing::Inner(vec![
                shapefile::Point::new(2.0, 2.0),
                shapefile::Point::new(2.0, 4.0),
                shapefile::Point::new(4.0, 4.0),
                shapefile::Point::new(4.0, 2.0),
            ]),
        ]);
        wtr.write_shape_and_record(&polygon, &record("丙", 55.0))
            .unwrap();

        (pts_path, poly_path)
    }

    #[test]
    fn shp_load_points_with_typed_dbase_fields() {
        let dir = std::env::temp_dir().join("kanyu_core_shp_points");
        let (pts_path, _) = write_test_shps(&dir);
        let layer = Layer::load("sites", pts_path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 2);
        let s = layer.summary();
        assert_eq!(s.format, "shp");
        assert_eq!(s.geometry_types, vec!["Point"]);
        assert_eq!(s.fields, vec!["height", "name"]);
        // dbase Numeric → JSON 数值，可参与数值比较查询。
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 1);
        // dbase Character → JSON 字符串。
        let props = layer.collection().features[0].properties.as_ref().unwrap();
        assert_eq!(props["name"].as_str().unwrap().trim_end(), "甲");
    }

    #[test]
    fn shp_load_polygon_preserves_holes() {
        let dir = std::env::temp_dir().join("kanyu_core_shp_hole");
        let (_, poly_path) = write_test_shps(&dir);
        let layer = Layer::load("zones", poly_path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 1);
        assert_eq!(layer.summary().geometry_types, vec!["Polygon"]);
        let geojson::Value::Polygon(rings) = &layer.collection().features[0]
            .geometry
            .as_ref()
            .unwrap()
            .value
        else {
            panic!("应为 Polygon 几何");
        };
        // 外环 + 1 个内环（洞）：interiors 非空。
        assert_eq!(rings.len(), 2);
    }

    #[test]
    fn shp_missing_dbf_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_shp_nodbf");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("noattr.shp");
        let mut wtr = shapefile::ShapeWriter::from_path(&path).unwrap();
        wtr.write_shape(&shapefile::Point::new(1.0, 1.0)).unwrap();
        drop(wtr);
        assert!(!path.with_extension("dbf").exists());
        let err = Layer::load("noattr", path.to_str().unwrap())
            .err()
            .expect("缺少 .dbf 应报错");
        assert!(
            err.to_string().contains("侧边文件"),
            "错误应提示侧边文件齐备问题: {err}"
        );
    }
}
