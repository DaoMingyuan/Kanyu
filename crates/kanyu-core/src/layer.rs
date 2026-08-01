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
            "fgb" => fgb_to_collection(path)?,
            "geoparquet" => parquet_to_collection(path)?,
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

    /// 导出为 FlatGeobuf 字节串：列 schema 从属性值推断（String→String、
    /// 整数→Long、浮点→Double、Bool→Bool；混合类型列退化为 String，空值跳过）。
    /// 单一几何类型按声明写出；混合几何按 FGB Unknown 异构声明（逐要素带类型）。
    pub fn to_fgb_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
        collection_to_fgb(collection)
    }

    /// 导出为 GeoParquet 字节串：几何列按 GeoParquet 1.x 规范的 WKB 编码
    /// （列名 `geometry`，geo 元数据由 geoparquet crate 生成，CRS 缺省 EPSG:4326）；
    /// 属性列 schema 推断规则同 [`Layer::to_fgb_bytes`]。
    pub fn to_geoparquet_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
        collection_to_geoparquet(collection)
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

/// FlatGeobuf → FeatureCollection：逐要素转换——几何经 geozero `ToJson`
/// 转为 geojson::Geometry，属性由自研 PropertyProcessor 按 FGB 列类型
/// 原生映射为 JSON 类型（数值/布尔/字符串不丢型）。
/// 注：不使用 geozero GeoJsonWriter 整体转换——其按列索引插逗号，
/// 稀疏属性（某要素缺第 0 列）会产生非法 JSON（`{, "name": …}`）。
fn fgb_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    use flatgeobuf::geozero::{FeatureProperties, ToJson};
    use flatgeobuf::FallibleStreamingIterator;

    let file = std::fs::File::open(path)?;
    let mut buf = std::io::BufReader::new(file);
    let mut fgb = flatgeobuf::FgbReader::open(&mut buf)
        .and_then(|r| r.select_all())
        .map_err(|e| {
            KanyuError::Other(format!(
                "flatgeobuf 读取失败（{path}）：{e}；文件可能损坏或不是有效的 FlatGeobuf"
            ))
        })?;

    let mut features = Vec::new();
    let mut rec_no = 0usize;
    while let Some(feature) = fgb.next().map_err(|e| {
        KanyuError::Other(format!(
            "flatgeobuf 第 {} 条要素解析失败（{path}）：{e}",
            rec_no + 1
        ))
    })? {
        rec_no += 1;
        let geometry = if feature.fbs_feature().geometry().is_some() {
            let text = feature.to_json().map_err(|e| {
                KanyuError::Other(format!(
                    "flatgeobuf 第 {rec_no} 条要素几何解析失败（{path}）：{e}"
                ))
            })?;
            let gj: geojson::GeoJson = text.parse()?;
            match gj {
                geojson::GeoJson::Geometry(g) => Some(g),
                _ => None,
            }
        } else {
            None
        };

        let mut props = JsonProperties::default();
        feature.process_properties(&mut props).map_err(|e| {
            KanyuError::Other(format!(
                "flatgeobuf 第 {rec_no} 条要素属性解析失败（{path}）：{e}"
            ))
        })?;

        features.push(geojson::Feature {
            bbox: None,
            geometry,
            id: None,
            properties: Some(props.0),
            foreign_members: None,
        });
    }

    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// FGB 属性处理器：列值 → 类型化 JSON properties（Binary 列跳过）。
#[derive(Default)]
struct JsonProperties(serde_json::Map<String, serde_json::Value>);

impl flatgeobuf::geozero::PropertyProcessor for JsonProperties {
    fn property(
        &mut self,
        _i: usize,
        name: &str,
        value: &flatgeobuf::geozero::ColumnValue,
    ) -> flatgeobuf::geozero::error::Result<bool> {
        use flatgeobuf::geozero::ColumnValue;
        let json = match value {
            ColumnValue::Byte(v) => serde_json::Value::from(*v),
            ColumnValue::UByte(v) => serde_json::Value::from(*v),
            ColumnValue::Short(v) => serde_json::Value::from(*v),
            ColumnValue::UShort(v) => serde_json::Value::from(*v),
            ColumnValue::Int(v) => serde_json::Value::from(*v),
            ColumnValue::UInt(v) => serde_json::Value::from(*v),
            ColumnValue::Long(v) => serde_json::Value::from(*v),
            ColumnValue::ULong(v) => serde_json::Value::from(*v),
            ColumnValue::Float(v) => serde_json::Value::from(*v),
            ColumnValue::Double(v) => serde_json::Value::from(*v),
            ColumnValue::Bool(v) => serde_json::Value::Bool(*v),
            ColumnValue::String(v) | ColumnValue::DateTime(v) => {
                serde_json::Value::String(v.to_string())
            }
            ColumnValue::Json(v) => {
                serde_json::from_str(v).unwrap_or_else(|_| serde_json::Value::String(v.to_string()))
            }
            ColumnValue::Binary(_) => return Ok(false),
        };
        self.0.insert(name.to_string(), json);
        Ok(false)
    }
}

/// 列式格式（FGB/GeoParquet）列类型（schema 推断的中间表示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColKind {
    Int,
    Double,
    Bool,
    Str,
}

impl ColKind {
    /// 单个 JSON 值对应的列类型（Null 不贡献类型）。
    fn of_value(value: &serde_json::Value) -> Option<Self> {
        match value {
            // as_i64 对整数值的 f64（如 80.0）同样成立。
            serde_json::Value::Number(n) if n.as_i64().is_some() => Some(Self::Int),
            serde_json::Value::Number(_) => Some(Self::Double),
            serde_json::Value::Bool(_) => Some(Self::Bool),
            serde_json::Value::String(_) => Some(Self::Str),
            _ => None,
        }
    }

    /// 同列多个值的类型合并：数值内部 Int+Double→Double，其余冲突退化为 String。
    fn merge(self, other: Self) -> Self {
        use ColKind::*;
        match (self, other) {
            (Int, Double) | (Double, Int) => Double,
            (a, b) if a == b => a,
            _ => Str,
        }
    }

    fn column_type(self) -> flatgeobuf::ColumnType {
        match self {
            Self::Int => flatgeobuf::ColumnType::Long,
            Self::Double => flatgeobuf::ColumnType::Double,
            Self::Bool => flatgeobuf::ColumnType::Bool,
            Self::Str => flatgeobuf::ColumnType::String,
        }
    }

    fn arrow_data_type(self) -> arrow_schema::DataType {
        match self {
            Self::Int => arrow_schema::DataType::Int64,
            Self::Double => arrow_schema::DataType::Float64,
            Self::Bool => arrow_schema::DataType::Boolean,
            Self::Str => arrow_schema::DataType::Utf8,
        }
    }
}

/// 扫描全集合推断列 schema（按字段首次出现顺序，列序稳定）。
fn infer_property_schema(collection: &geojson::FeatureCollection) -> Vec<(String, ColKind)> {
    let mut schema: Vec<(String, ColKind)> = Vec::new();
    for feature in &collection.features {
        let Some(props) = &feature.properties else {
            continue;
        };
        for (name, value) in props {
            let Some(kind) = ColKind::of_value(value) else {
                continue;
            };
            match schema.iter_mut().find(|(n, _)| n == name) {
                Some((_, existing)) => *existing = existing.merge(kind),
                None => schema.push((name.clone(), kind)),
            }
        }
    }
    schema
}

/// geojson 几何类型名 → FGB 几何类型。
fn fgb_geometry_type(value: &geojson::Value) -> flatgeobuf::GeometryType {
    use flatgeobuf::GeometryType;
    match value {
        geojson::Value::Point(_) => GeometryType::Point,
        geojson::Value::MultiPoint(_) => GeometryType::MultiPoint,
        geojson::Value::LineString(_) => GeometryType::LineString,
        geojson::Value::MultiLineString(_) => GeometryType::MultiLineString,
        geojson::Value::Polygon(_) => GeometryType::Polygon,
        geojson::Value::MultiPolygon(_) => GeometryType::MultiPolygon,
        geojson::Value::GeometryCollection(_) => GeometryType::GeometryCollection,
    }
}

/// FeatureCollection → FlatGeobuf 字节串（Hilbert 排序 + 空间索引，
/// CRS 声明为 GeoJSON 默认的 EPSG:4326）。
fn collection_to_fgb(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
    use flatgeobuf::geozero::{geojson::GeoJson, ColumnValue, PropertyProcessor};
    use flatgeobuf::{FgbCrs, FgbWriter, FgbWriterOptions, GeometryType};

    let crs = FgbCrs {
        code: 4326,
        ..Default::default()
    };
    // 几何类型集合：唯一则按声明写出（对 QGIS 等读取方更友好）；
    // 混合（或空集合）按 Unknown 异构声明，逐要素携带类型。
    let mut geom_types: Vec<GeometryType> = Vec::new();
    for feature in &collection.features {
        let Some(geom) = &feature.geometry else {
            return Err(KanyuError::Other(
                "flatgeobuf 导出失败：存在无几何要素，FGB 要求每个要素都有几何".to_string(),
            ));
        };
        let t = fgb_geometry_type(&geom.value);
        if !geom_types.contains(&t) {
            geom_types.push(t);
        }
    }
    let mut fgb = if let [only] = geom_types[..] {
        FgbWriter::create_with_options(
            "kanyu",
            only,
            FgbWriterOptions {
                crs,
                ..Default::default()
            },
        )
    } else {
        FgbWriter::create_with_options(
            "kanyu",
            GeometryType::Unknown,
            FgbWriterOptions {
                crs,
                detect_type: false,
                promote_to_multi: false,
                ..Default::default()
            },
        )
    }
    .map_err(|e| KanyuError::Other(format!("flatgeobuf 写出失败: {e}")))?;

    let schema = infer_property_schema(collection);
    for (name, kind) in &schema {
        fgb.add_column(name, kind.column_type(), |_, _| {});
    }

    for feature in &collection.features {
        let geom = feature.geometry.as_ref().expect("前置检查已排除无几何要素");
        let geom_json = geom.to_string();
        fgb.add_feature_geom(GeoJson(&geom_json), |feat| {
            let Some(props) = &feature.properties else {
                return;
            };
            for (i, (name, kind)) in schema.iter().enumerate() {
                let Some(value) = props.get(name) else {
                    continue;
                };
                if value.is_null() {
                    continue;
                }
                // 退化列（Str）中的非字符串值需物化为 String 以延长借用。
                let owned;
                let colval = match kind {
                    ColKind::Int => ColumnValue::Long(value.as_i64().unwrap_or_default()),
                    ColKind::Double => ColumnValue::Double(value.as_f64().unwrap_or_default()),
                    ColKind::Bool => ColumnValue::Bool(value.as_bool().unwrap_or_default()),
                    ColKind::Str => {
                        owned = match value {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        ColumnValue::String(&owned)
                    }
                };
                let _ = feat.property(i, name, &colval);
            }
        })
        .map_err(|e| KanyuError::Other(format!("flatgeobuf 要素写出失败: {e}")))?;
    }

    let mut out: Vec<u8> = Vec::new();
    fgb.write(&mut out)
        .map_err(|e| KanyuError::Other(format!("flatgeobuf 写出失败: {e}")))?;
    Ok(out)
}

/// geojson 几何 → WKB 字节（ISO，小端，2D；z 维度丢弃，与 FGB 导出口径一致）。
fn wkb_encode_geom(value: &geojson::Value, out: &mut Vec<u8>) {
    out.push(1); // 小端字节序
    match value {
        geojson::Value::Point(pos) => {
            wkb_write_u32(out, 1);
            wkb_write_position(out, pos);
        }
        geojson::Value::MultiPoint(pts) => {
            wkb_write_u32(out, 4);
            wkb_write_u32(out, pts.len() as u32);
            for p in pts {
                wkb_encode_geom(&geojson::Value::Point(p.clone()), out);
            }
        }
        geojson::Value::LineString(line) => {
            wkb_write_u32(out, 2);
            wkb_write_positions(out, line);
        }
        geojson::Value::MultiLineString(lines) => {
            wkb_write_u32(out, 5);
            wkb_write_u32(out, lines.len() as u32);
            for l in lines {
                wkb_encode_geom(&geojson::Value::LineString(l.clone()), out);
            }
        }
        geojson::Value::Polygon(rings) => {
            wkb_write_u32(out, 3);
            wkb_write_u32(out, rings.len() as u32);
            for r in rings {
                wkb_write_positions(out, r);
            }
        }
        geojson::Value::MultiPolygon(polys) => {
            wkb_write_u32(out, 6);
            wkb_write_u32(out, polys.len() as u32);
            for p in polys {
                wkb_encode_geom(&geojson::Value::Polygon(p.clone()), out);
            }
        }
        geojson::Value::GeometryCollection(geoms) => {
            wkb_write_u32(out, 7);
            wkb_write_u32(out, geoms.len() as u32);
            for g in geoms {
                wkb_encode_geom(&g.value, out);
            }
        }
    }
}

fn wkb_write_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn wkb_write_position(out: &mut Vec<u8>, pos: &[f64]) {
    let x = pos.first().copied().unwrap_or(0.0);
    let y = pos.get(1).copied().unwrap_or(0.0);
    out.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(&y.to_le_bytes());
}

fn wkb_write_positions(out: &mut Vec<u8>, positions: &[Vec<f64>]) {
    wkb_write_u32(out, positions.len() as u32);
    for p in positions {
        wkb_write_position(out, p);
    }
}

/// WKB → geojson 几何（大小端均支持；严格校验长度，损坏数据报中文错误）。
fn wkb_decode_geom(bytes: &[u8]) -> Result<geojson::Value> {
    let mut cur = WkbCursor {
        bytes,
        pos: 0,
        le: true,
    };
    let value = cur.read_geometry()?;
    if cur.pos != bytes.len() {
        return Err(KanyuError::Other("WKB 几何存在尾部多余字节".to_string()));
    }
    Ok(value)
}

/// WKB 字节流游标（带边界校验）。
struct WkbCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    le: bool,
}

impl<'a> WkbCursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| KanyuError::Other("WKB 声明的长度溢出，数据损坏".to_string()))?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| KanyuError::Other("WKB 数据长度不足，数据损坏".to_string()))?;
        self.pos = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().expect("长度已校验");
        Ok(if self.le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }

    fn read_f64(&mut self) -> Result<f64> {
        let b: [u8; 8] = self.take(8)?.try_into().expect("长度已校验");
        Ok(if self.le {
            f64::from_le_bytes(b)
        } else {
            f64::from_be_bytes(b)
        })
    }

    fn read_position(&mut self) -> Result<Vec<f64>> {
        Ok(vec![self.read_f64()?, self.read_f64()?])
    }

    fn read_positions(&mut self) -> Result<Vec<Vec<f64>>> {
        let n = self.read_u32()?;
        (0..n).map(|_| self.read_position()).collect()
    }

    fn read_geometry(&mut self) -> Result<geojson::Value> {
        self.le = match self.read_u8()? {
            0 => false,
            1 => true,
            other => {
                return Err(KanyuError::Other(format!(
                    "WKB 字节序标记非法（{other}），数据损坏"
                )))
            }
        };
        match self.read_u32()? {
            1 => Ok(geojson::Value::Point(self.read_position()?)),
            2 => Ok(geojson::Value::LineString(self.read_positions()?)),
            3 => {
                let n = self.read_u32()?;
                let rings = (0..n)
                    .map(|_| self.read_positions())
                    .collect::<Result<_>>()?;
                Ok(geojson::Value::Polygon(rings))
            }
            4 => {
                // MultiPoint：n 个完整 Point 子 WKB。
                let n = self.read_u32()?;
                let mut pts = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    match self.read_geometry()? {
                        geojson::Value::Point(p) => pts.push(p),
                        other => {
                            return Err(KanyuError::Other(format!(
                                "WKB MultiPoint 内出现非法子类型（{}）",
                                other.type_name()
                            )))
                        }
                    }
                }
                Ok(geojson::Value::MultiPoint(pts))
            }
            5 => {
                let n = self.read_u32()?;
                let mut lines = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    match self.read_geometry()? {
                        geojson::Value::LineString(l) => lines.push(l),
                        other => {
                            return Err(KanyuError::Other(format!(
                                "WKB MultiLineString 内出现非法子类型（{}）",
                                other.type_name()
                            )))
                        }
                    }
                }
                Ok(geojson::Value::MultiLineString(lines))
            }
            6 => {
                let n = self.read_u32()?;
                let mut polys = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    match self.read_geometry()? {
                        geojson::Value::Polygon(p) => polys.push(p),
                        other => {
                            return Err(KanyuError::Other(format!(
                                "WKB MultiPolygon 内出现非法子类型（{}）",
                                other.type_name()
                            )))
                        }
                    }
                }
                Ok(geojson::Value::MultiPolygon(polys))
            }
            7 => {
                let n = self.read_u32()?;
                let geoms = (0..n)
                    .map(|_| Ok(geojson::Geometry::new(self.read_geometry()?)))
                    .collect::<Result<_>>()?;
                Ok(geojson::Value::GeometryCollection(geoms))
            }
            other => Err(KanyuError::Other(format!(
                "WKB 几何类型码不支持（{other}），仅支持 1-7（2D）"
            ))),
        }
    }
}

/// GeoParquet → FeatureCollection：几何列按 geo 元数据定位（v0.1 要求 WKB 编码），
/// 属性列按 Arrow 类型原生映射 JSON 类型（数值/布尔/字符串不丢型）。
fn parquet_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    use arrow_array::Array;
    use geoparquet::metadata::GeoParquetColumnEncoding;
    use geoparquet::reader::GeoParquetReaderBuilder;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|e| {
        KanyuError::Other(format!(
            "geoparquet 读取失败（{path}）：{e}；文件可能损坏或不是有效的 Parquet"
        ))
    })?;

    let meta = builder
        .geoparquet_metadata()
        .ok_or_else(|| {
            KanyuError::Other(format!(
                "geoparquet 读取失败（{path}）：缺少 geo 元数据，不是 GeoParquet 文件"
            ))
        })?
        .map_err(|e| KanyuError::Other(format!("geoparquet 元数据解析失败（{path}）：{e}")))?;
    let geom_col = meta.primary_column.clone();
    if let Some(col_meta) = meta.columns.get(&geom_col) {
        if col_meta.encoding != GeoParquetColumnEncoding::WKB {
            return Err(KanyuError::Other(format!(
                "geoparquet 读取失败（{path}）：几何列 '{geom_col}' 编码为 {:?}，v0.1 仅支持 WKB 编码",
                col_meta.encoding
            )));
        }
    }

    let reader = builder
        .build()
        .map_err(|e| KanyuError::Other(format!("geoparquet 读取失败（{path}）：{e}")))?;
    let mut features = Vec::new();
    for batch in reader {
        let batch = batch
            .map_err(|e| KanyuError::Other(format!("geoparquet 数据页解析失败（{path}）：{e}")))?;
        let schema = batch.schema();
        let geom_idx = schema.index_of(&geom_col).map_err(|_| {
            KanyuError::Other(format!(
                "geoparquet 读取失败（{path}）：缺少几何列 '{geom_col}'"
            ))
        })?;
        let geom_arr = batch.column(geom_idx);
        for row in 0..batch.num_rows() {
            let geometry = if geom_arr.is_null(row) {
                None
            } else {
                let wkb = parquet_binary_value(geom_arr, row)?;
                let value = wkb_decode_geom(wkb).map_err(|e| {
                    KanyuError::Other(format!(
                        "geoparquet 第 {} 行几何 WKB 解析失败（{path}）：{e}",
                        row + 1
                    ))
                })?;
                Some(geojson::Geometry::new(value))
            };
            let mut properties = serde_json::Map::new();
            for (idx, field) in schema.fields().iter().enumerate() {
                if idx == geom_idx {
                    continue;
                }
                if let Some(v) = arrow_value_to_json(batch.column(idx), row, field.name())? {
                    properties.insert(field.name().clone(), v);
                }
            }
            features.push(geojson::Feature {
                bbox: None,
                geometry,
                id: None,
                properties: Some(properties),
                foreign_members: None,
            });
        }
    }

    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// 读取 Binary/LargeBinary 列的单行字节。
fn parquet_binary_value(array: &arrow_array::ArrayRef, row: usize) -> Result<&[u8]> {
    use arrow_array::{BinaryArray, LargeBinaryArray};
    use arrow_schema::DataType;
    match array.data_type() {
        DataType::Binary => Ok(array
            .as_any()
            .downcast_ref::<BinaryArray>()
            .expect("类型已匹配")
            .value(row)),
        DataType::LargeBinary => Ok(array
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .expect("类型已匹配")
            .value(row)),
        dt => Err(KanyuError::Other(format!(
            "geoparquet 几何列类型应为 Binary（WKB），实际为 {dt}"
        ))),
    }
}

/// Arrow 单元格 → JSON 值；Null 与 NaN/Inf 浮点返回 None 以跳过。
/// 不支持的列类型返回带列名的中文错误（拒绝静默丢列）。
fn arrow_value_to_json(
    array: &arrow_array::ArrayRef,
    row: usize,
    name: &str,
) -> Result<Option<serde_json::Value>> {
    use arrow_array::*;
    use arrow_schema::DataType;

    /// 按数组类型 downcast 并取值的宏（match 已保证类型一致）。
    macro_rules! val {
        ($t:ty) => {
            array
                .as_any()
                .downcast_ref::<$t>()
                .expect("类型已匹配")
                .value(row)
        };
    }

    if array.is_null(row) {
        return Ok(None);
    }
    let unsupported = || {
        KanyuError::Other(format!(
            "geoparquet 暂不支持读取列 '{name}' 的类型 {}",
            array.data_type()
        ))
    };
    let v = match array.data_type() {
        DataType::Utf8 => serde_json::Value::from(val!(StringArray)),
        DataType::LargeUtf8 => serde_json::Value::from(val!(LargeStringArray)),
        DataType::Utf8View => serde_json::Value::from(val!(StringViewArray)),
        DataType::Int8 => serde_json::Value::from(val!(Int8Array)),
        DataType::Int16 => serde_json::Value::from(val!(Int16Array)),
        DataType::Int32 => serde_json::Value::from(val!(Int32Array)),
        DataType::Int64 => serde_json::Value::from(val!(Int64Array)),
        DataType::UInt8 => serde_json::Value::from(val!(UInt8Array)),
        DataType::UInt16 => serde_json::Value::from(val!(UInt16Array)),
        DataType::UInt32 => serde_json::Value::from(val!(UInt32Array)),
        DataType::UInt64 => serde_json::Value::from(val!(UInt64Array)),
        DataType::Float32 => match serde_json::Number::from_f64(val!(Float32Array) as f64) {
            Some(n) => serde_json::Value::Number(n),
            None => return Ok(None),
        },
        DataType::Float64 => match serde_json::Number::from_f64(val!(Float64Array)) {
            Some(n) => serde_json::Value::Number(n),
            None => return Ok(None),
        },
        DataType::Boolean => serde_json::Value::Bool(val!(BooleanArray)),
        _ => return Err(unsupported()),
    };
    Ok(Some(v))
}

/// 取要素的非 Null 属性值。
fn json_prop<'a>(feature: &'a geojson::Feature, name: &str) -> Option<&'a serde_json::Value> {
    feature
        .properties
        .as_ref()
        .and_then(|p| p.get(name))
        .filter(|v| !v.is_null())
}

/// FeatureCollection → GeoParquet 字节串：属性列 schema 推断（同 FGB），
/// 几何列 `geometry` 为 WKB Binary + geoarrow 扩展元数据；geo 元数据、
/// geometry_types 与 bbox 由 geoparquet crate 编码器生成。
fn collection_to_geoparquet(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
    use arrow_array::{
        ArrayRef, BinaryArray, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
    };
    use arrow_schema::{Field, Schema};
    use geoarrow_schema::{GeoArrowType, Metadata as GeoMetadata, WkbType};
    use geoparquet::writer::{GeoParquetRecordBatchEncoder, GeoParquetWriterOptionsBuilder};
    use parquet::arrow::ArrowWriter;
    use std::sync::Arc;

    let prop_schema = infer_property_schema(collection);
    let mut fields: Vec<Field> = prop_schema
        .iter()
        .map(|(name, kind)| Field::new(name, kind.arrow_data_type(), true))
        .collect();
    let geom_type = GeoArrowType::Wkb(WkbType::new(Arc::new(GeoMetadata::default())));
    fields.push(geom_type.to_field("geometry", true));
    let schema = Arc::new(Schema::new(fields));

    let mut columns: Vec<ArrayRef> = Vec::new();
    for (name, kind) in &prop_schema {
        let col: ArrayRef = match kind {
            ColKind::Int => Arc::new(Int64Array::from(
                collection
                    .features
                    .iter()
                    .map(|f| json_prop(f, name).and_then(|v| v.as_i64()))
                    .collect::<Vec<_>>(),
            )),
            ColKind::Double => Arc::new(Float64Array::from(
                collection
                    .features
                    .iter()
                    .map(|f| json_prop(f, name).and_then(|v| v.as_f64()))
                    .collect::<Vec<_>>(),
            )),
            ColKind::Bool => Arc::new(BooleanArray::from(
                collection
                    .features
                    .iter()
                    .map(|f| json_prop(f, name).and_then(|v| v.as_bool()))
                    .collect::<Vec<_>>(),
            )),
            ColKind::Str => Arc::new(StringArray::from(
                collection
                    .features
                    .iter()
                    .map(|f| {
                        json_prop(f, name).map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                    })
                    .collect::<Vec<_>>(),
            )),
        };
        columns.push(col);
    }
    let wkb_owned: Vec<Option<Vec<u8>>> = collection
        .features
        .iter()
        .map(|f| {
            f.geometry.as_ref().map(|g| {
                let mut buf = Vec::new();
                wkb_encode_geom(&g.value, &mut buf);
                buf
            })
        })
        .collect();
    columns.push(Arc::new(BinaryArray::from(
        wkb_owned.iter().map(|o| o.as_deref()).collect::<Vec<_>>(),
    )));

    let batch = RecordBatch::try_new(schema.clone(), columns)
        .map_err(|e| KanyuError::Other(format!("geoparquet 批次构造失败: {e}")))?;

    let options = GeoParquetWriterOptionsBuilder::default().build();
    let mut encoder = GeoParquetRecordBatchEncoder::try_new(&schema, &options)
        .map_err(|e| KanyuError::Other(format!("geoparquet 编码器构造失败: {e}")))?;
    let target_schema = encoder.target_schema();
    let encoded = encoder
        .encode_record_batch(&batch)
        .map_err(|e| KanyuError::Other(format!("geoparquet 几何编码失败: {e}")))?;
    let kv = encoder
        .into_keyvalue()
        .map_err(|e| KanyuError::Other(format!("geoparquet 元数据生成失败: {e}")))?;

    let mut out: Vec<u8> = Vec::new();
    {
        let mut writer = ArrowWriter::try_new(&mut out, target_schema, None)
            .map_err(|e| KanyuError::Other(format!("geoparquet 写出失败: {e}")))?;
        writer
            .write(&encoded)
            .map_err(|e| KanyuError::Other(format!("geoparquet 写出失败: {e}")))?;
        writer.append_key_value_metadata(kv);
        writer
            .close()
            .map_err(|e| KanyuError::Other(format!("geoparquet 写出失败: {e}")))?;
    }
    Ok(out)
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

    /// 混合几何测试集合：Point/LineString/Polygon 各一，
    /// 属性覆盖 string（name）/number（height，整浮混合）/bool（active）。
    fn fgb_test_collection() -> geojson::FeatureCollection {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},
                 "properties":{"name":"甲","height":80,"active":true}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{"name":"乙","height":30,"active":false}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[0,4],[4,4],[4,0],[0,0]]]},
                 "properties":{"name":"丙","height":55.5,"active":true}}
            ]
        }"#
        .parse()
        .unwrap();
        geojson::FeatureCollection::try_from(gj).unwrap()
    }

    /// 按 name 属性找回要素（FGB 写出按 Hilbert 空间排序，不能假设读回顺序）。
    fn find_by_name(layer: &Layer, name: &str) -> geojson::Feature {
        layer
            .collection()
            .features
            .iter()
            .find(|f| {
                f.properties
                    .as_ref()
                    .and_then(|p| p.get("name"))
                    .and_then(|v| v.as_str())
                    == Some(name)
            })
            .unwrap_or_else(|| panic!("应存在 name={name} 的要素"))
            .clone()
    }

    #[test]
    fn fgb_roundtrip_preserves_features_and_types() {
        let bytes = Layer::to_fgb_bytes(&fgb_test_collection()).unwrap();
        let dir = std::env::temp_dir().join("kanyu_core_fgb_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.fgb");
        std::fs::write(&path, &bytes).unwrap();

        let layer = Layer::load("mixed", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        assert_eq!(s.format, "fgb");
        assert_eq!(s.geometry_types, vec!["LineString", "Point", "Polygon"]);
        assert_eq!(s.fields, vec!["active", "height", "name"]);
        // 整浮混合列合并为 Double，数值比较查询结果一致（80 与 55.5 > 50）。
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 2);
        // Bool/String 列读回后 JSON 类型保持；数值列按数值断言
        //（JSON 文本层面 80.0 会写作 80，不区分整浮表示）。
        let a = find_by_name(&layer, "甲");
        let props = a.properties.as_ref().unwrap();
        assert_eq!(props["active"], serde_json::Value::Bool(true));
        assert_eq!(props["height"].as_f64(), Some(80.0));
    }

    #[test]
    fn fgb_export_bytes_start_with_magic() {
        let bytes = Layer::to_fgb_bytes(&fgb_test_collection()).unwrap();
        // FGB 规范魔数：ASCII "fgb" + 主版本号 3 + "fgb" + 0x00。
        assert_eq!(&bytes[..8], &[b'f', b'g', b'b', 3, b'f', b'g', b'b', 0]);
    }

    #[test]
    fn fgb_corrupt_file_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_fgb_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.fgb");
        std::fs::write(&path, b"this is not a flatgeobuf file").unwrap();
        let err = Layer::load("garbage", path.to_str().unwrap())
            .err()
            .expect("损坏文件应报错");
        assert!(
            err.to_string().contains("flatgeobuf"),
            "错误应指出 flatgeobuf 读取问题: {err}"
        );
    }

    #[test]
    fn geoparquet_roundtrip_preserves_features_and_types() {
        let bytes = Layer::to_geoparquet_bytes(&fgb_test_collection()).unwrap();
        let dir = std::env::temp_dir().join("kanyu_core_geoparquet_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.parquet");
        std::fs::write(&path, &bytes).unwrap();

        let layer = Layer::load("mixed", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        assert_eq!(s.format, "geoparquet");
        assert_eq!(s.geometry_types, vec!["LineString", "Point", "Polygon"]);
        assert_eq!(s.fields, vec!["active", "height", "name"]);
        // 整浮混合列合并为 Double，数值比较查询结果一致（80 与 55.5 > 50）。
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 2);
        // Bool/数值列读回后 JSON 类型保持。
        let a = find_by_name(&layer, "甲");
        let props = a.properties.as_ref().unwrap();
        assert_eq!(props["active"], serde_json::Value::Bool(true));
        assert_eq!(props["height"].as_f64(), Some(80.0));
    }

    #[test]
    fn geoparquet_export_bytes_have_parquet_magic() {
        let bytes = Layer::to_geoparquet_bytes(&fgb_test_collection()).unwrap();
        // Parquet 规范：文件以 "PAR1" 开头并以 "PAR1" 结尾。
        assert_eq!(&bytes[..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }

    #[test]
    fn geoparquet_corrupt_file_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_geoparquet_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.parquet");
        std::fs::write(&path, b"this is not a parquet file").unwrap();
        let err = Layer::load("garbage", path.to_str().unwrap())
            .err()
            .expect("损坏文件应报错");
        assert!(
            err.to_string().contains("geoparquet"),
            "错误应指出 geoparquet 读取问题: {err}"
        );
    }
}
