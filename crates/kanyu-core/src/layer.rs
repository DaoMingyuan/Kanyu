//! 图层内存模型。
//!
//! 以 GeoArrow RecordBatch 为原生载体（WKB 几何列 + 类型化属性列；
//! 见 docs/MASTERPLAN.md 第三部分）。各格式解析器在边界统一转为
//! FeatureCollection 后一次性入列，导出时按需转回——格式代码零感知。

use arrow_array::RecordBatch;
use serde::Serialize;

use crate::error::{KanyuError, Result};
use crate::format::FormatRegistry;

/// 几何列名（GeoParquet 惯例；Field 携带 geoarrow.wkb 扩展元数据）。
const GEOMETRY_COL: &str = "geometry";

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

/// 一个已加载的矢量图层（GeoArrow RecordBatch 列式载体）。
pub struct Layer {
    id: String,
    format: String,
    batch: RecordBatch,
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
                    xlsx_to_collection(path)?
                } else {
                    let delimiter = if path.to_ascii_lowercase().ends_with(".tsv") {
                        b'\t'
                    } else {
                        b','
                    };
                    let text = std::fs::read_to_string(path)?;
                    csv_to_collection(&text, delimiter)?
                }
            }
            "shp" => shp_to_collection(path)?,
            "fgb" => fgb_to_collection(path)?,
            "geoparquet" => parquet_to_collection(path)?,
            "dxf" => dxf_to_collection(path)?,
            "kml" => kml_to_collection(path)?,
            other => {
                return Err(KanyuError::UnsupportedOperation {
                    format: other.to_string(),
                    operation: "native-load (bridge driver not enabled)".to_string(),
                })
            }
        };
        let batch = collection_to_batch(&collection)?;
        Ok(Self {
            id,
            format: caps.id.to_string(),
            batch,
        })
    }

    /// 图层标识。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 要素数量。
    pub fn len(&self) -> usize {
        self.batch.num_rows()
    }

    /// 是否为空图层。
    pub fn is_empty(&self) -> bool {
        self.batch.num_rows() == 0
    }

    /// 概要信息（直接在 batch 上统计：几何类型读 WKB 头部，
    /// 字段取属性列名，无需物化 GeoJSON）。
    pub fn summary(&self) -> LayerSummary {
        let schema = self.batch.schema();
        let mut geometry_types: Vec<String> = match schema.index_of(GEOMETRY_COL) {
            Ok(idx) => {
                let arr = self.batch.column(idx);
                (0..self.batch.num_rows())
                    .filter(|row| !arr.is_null(*row))
                    .filter_map(|row| wkb_type_name(parquet_binary_value(arr, row).ok()?))
                    .map(str::to_string)
                    .collect()
            }
            Err(_) => Vec::new(),
        };
        geometry_types.sort();
        geometry_types.dedup();

        let mut fields: Vec<String> = schema
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .filter(|n| n != GEOMETRY_COL)
            .collect();
        fields.sort();

        LayerSummary {
            id: self.id.clone(),
            format: self.format.clone(),
            feature_count: self.len(),
            geometry_types,
            fields,
        }
    }

    /// 访问要素集合（按需从 RecordBatch 转换的**拥有值**；
    /// 零拷贝访问请用 [`Layer::batch`]）。
    pub fn collection(&self) -> geojson::FeatureCollection {
        batch_to_collection(&self.batch).expect("内核构造的 batch 必然可逆转换")
    }

    /// 零拷贝访问底层 GeoArrow RecordBatch（WKB 几何列 + 类型化属性列）。
    pub fn batch(&self) -> &RecordBatch {
        &self.batch
    }

    /// 属性查询：支持简单比较表达式 `"field op value"`，
    /// op ∈ `==` `!=` `>` `>=` `<` `<=`。数值字段按数值比较，其余按字符串。
    /// 直接在 batch 列上求值（谓词语义与 GeoJSON 载体时代逐比特一致；
    /// 列不存在或单元格为空的行不匹配），命中行经 arrow take 取子集。
    ///
    /// 例：`"height > 50"`、`"usage == residential"`。
    pub fn query(&self, expression: &str) -> Result<geojson::FeatureCollection> {
        let predicate = Predicate::parse(expression)?;
        let indices: Vec<u64> = match self.batch.schema().index_of(&predicate.field) {
            Ok(col_idx) => {
                let arr = self.batch.column(col_idx);
                let mut matched = Vec::new();
                for row in 0..self.batch.num_rows() {
                    if arr.is_null(row) {
                        continue;
                    }
                    if let Some(actual) = arrow_value_to_json(arr, row, &predicate.field)? {
                        if predicate.matches_value(&actual) {
                            matched.push(row as u64);
                        }
                    }
                }
                matched
            }
            Err(_) => Vec::new(),
        };
        let take_idx = arrow_array::UInt64Array::from(indices);
        let columns = self
            .batch
            .columns()
            .iter()
            .map(|c| arrow_select::take::take(c, &take_idx, None))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| KanyuError::Other(format!("查询结果子集构造失败: {e}")))?;
        let sub = RecordBatch::try_new(self.batch.schema(), columns)
            .map_err(|e| KanyuError::Other(format!("查询结果子集构造失败: {e}")))?;
        batch_to_collection(&sub)
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

    /// 导出为 DXF 字符串：Point/MultiPoint→POINT、LineString/MultiLineString→
    /// 开放 LWPOLYLINE、Polygon/MultiPolygon→闭合 LWPOLYLINE（仅外环，洞舍弃）。
    /// 所有要素写到默认图层 "0"；properties/XDATA 写出 📋 暂不支持；z 丢弃。
    pub fn to_dxf_string(collection: &geojson::FeatureCollection) -> Result<String> {
        collection_to_dxf(collection)
    }

    /// 导出为 KML 字符串：每要素一个 Placemark（全六类型，Multi* → MultiGeometry，
    /// Polygon 含洞保留为内环）；`name`/`description` 属性写为同名字段，
    /// 其余属性写入 ExtendedData/SimpleData（值转字符串）；z 丢弃。
    pub fn to_kml_string(collection: &geojson::FeatureCollection) -> Result<String> {
        collection_to_kml(collection)
    }

    /// 导出为 KMZ 字节串（zip 容器，deflate 压缩）：内含 doc.kml 单条目
    ///（内容同 [`Layer::to_kml_string`]）。
    pub fn to_kmz_bytes(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
        collection_to_kmz(collection)
    }
}

/// 单元格值（CSV 与 xlsx 共用的中间表示）：原生类型优先，文本兜底。
enum CellValue {
    /// 原生数值（xlsx Int/Float）。
    Number(f64),
    /// 原生布尔（xlsx Bool）。
    Bool(bool),
    /// 文本（CSV 全部单元格；xlsx String/DateTime 等）。
    Text(String),
}

impl CellValue {
    /// 数值化规则（与 CSV 一致）：原生数值直取，文本尝试 parse f64，退化字符串。
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Number(n) => serde_json::Value::from(*n),
            Self::Bool(b) => serde_json::Value::Bool(*b),
            Self::Text(raw) => raw
                .parse::<f64>()
                .map(serde_json::Value::from)
                .unwrap_or_else(|_| serde_json::Value::String(raw.clone())),
        }
    }

    /// 坐标值：原生数值直取，文本尝试 parse。
    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Bool(_) => None,
            Self::Text(raw) => raw.parse().ok(),
        }
    }
}

/// 行记录 → FeatureCollection（CSV 与 xlsx 共用）：自动识别坐标列
///（lon/lat/x/y/经度/纬度），其余列作为属性（数值化规则见 CellValue::to_json）。
/// `rows` 的 usize 为数据行号（错误消息用；CSV 传 row_no+2）。
fn rows_to_collection(
    format: &str,
    headers: &[String],
    rows: impl Iterator<Item = (usize, Vec<CellValue>)>,
) -> Result<geojson::FeatureCollection> {
    let headers_record = csv::StringRecord::from(headers);
    let (x_idx, y_idx) = detect_coord_columns(&headers_record).ok_or_else(|| {
        KanyuError::Other(format!(
            "{format} 缺少可识别的坐标列（支持 lon/lat、longitude/latitude、x/y、经度/纬度）；实际表头: {}",
            headers.join(", ")
        ))
    })?;

    let mut features = Vec::new();
    for (row_no, record) in rows {
        let x: f64 = record
            .get(x_idx)
            .and_then(CellValue::as_f64)
            .ok_or_else(|| KanyuError::Other(format!("{format} 第 {row_no} 行 X 坐标不是数值")))?;
        let y: f64 = record
            .get(y_idx)
            .and_then(CellValue::as_f64)
            .ok_or_else(|| KanyuError::Other(format!("{format} 第 {row_no} 行 Y 坐标不是数值")))?;

        let mut properties = serde_json::Map::new();
        for (i, header) in headers.iter().enumerate() {
            if i == x_idx || i == y_idx {
                continue;
            }
            if let Some(cell) = record.get(i) {
                properties.insert(header.to_string(), cell.to_json());
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

/// CSV/TSV → FeatureCollection（薄壳：解析文本后委托 rows_to_collection）。
fn csv_to_collection(text: &str, delimiter: u8) -> Result<geojson::FeatureCollection> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| KanyuError::Other(format!("csv 表头解析失败: {e}")))?
        .iter()
        .map(|h| h.to_string())
        .collect();
    let rows = rdr.records().enumerate().map(|(row_no, record)| {
        let record = record
            .map_err(|e| KanyuError::Other(format!("csv 第 {} 行解析失败: {e}", row_no + 2)))?;
        Ok((
            row_no + 2,
            record
                .iter()
                .map(|s| CellValue::Text(s.to_string()))
                .collect(),
        ))
    });
    rows_to_collection(
        "csv",
        &headers,
        rows.collect::<Result<Vec<_>>>()?.into_iter(),
    )
}

/// xlsx → FeatureCollection：首个 worksheet，表头行 + 数据行
///（calamine 原生类型：Int/Float→Number、Bool→Bool、String/DateTime→文本后
/// 按 CSV 同款规则数值化）；空表/无坐标列中文错误。范围：只读（写出 📋）。
fn xlsx_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    use calamine::Reader;
    let mut workbook: calamine::Xlsx<_> = calamine::open_workbook(path).map_err(|e| {
        KanyuError::Other(format!(
            "xlsx 读取失败（{path}）：{e}；文件可能损坏或不是有效的 Excel"
        ))
    })?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| KanyuError::Other(format!("xlsx 为空工作簿（{path}）")))?
        .map_err(|e| KanyuError::Other(format!("xlsx 首个 worksheet 读取失败（{path}）：{e}")))?;

    let mut rows = range.rows();
    let headers: Vec<String> = rows
        .next()
        .ok_or_else(|| KanyuError::Other(format!("xlsx 为空表（{path}）")))?
        .iter()
        .map(|c| match c {
            calamine::Data::String(s) => s.trim().to_string(),
            other => other.to_string().trim().to_string(),
        })
        .collect();
    let data_rows = rows.enumerate().map(|(row_no, row)| {
        (
            row_no + 2,
            row.iter()
                .map(|c| match c {
                    calamine::Data::Int(i) => CellValue::Number(*i as f64),
                    calamine::Data::Float(f) => CellValue::Number(*f),
                    calamine::Data::Bool(b) => CellValue::Bool(*b),
                    calamine::Data::String(s) => CellValue::Text(s.trim().to_string()),
                    calamine::Data::Empty => CellValue::Text(String::new()),
                    other => CellValue::Text(other.to_string().trim().to_string()),
                })
                .collect::<Vec<_>>(),
        )
    });
    rows_to_collection("xlsx", &headers, data_rows)
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
            "暂不支持读取列 '{name}' 的类型 {}",
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

/// FeatureCollection → GeoArrow RecordBatch：属性列 schema 推断
/// （Int64/Float64/Boolean/Utf8），几何列 `geometry` 为 WKB BinaryArray +
/// geoarrow.wkb 扩展元数据。所有格式加载后的统一入列入口。
fn collection_to_batch(collection: &geojson::FeatureCollection) -> Result<RecordBatch> {
    use arrow_array::{ArrayRef, BinaryArray, BooleanArray, Float64Array, Int64Array, StringArray};
    use arrow_schema::{Field, Schema};
    use geoarrow_schema::{GeoArrowType, Metadata as GeoMetadata, WkbType};
    use std::sync::Arc;

    let prop_schema = infer_property_schema(collection);
    let mut fields: Vec<Field> = prop_schema
        .iter()
        .map(|(name, kind)| Field::new(name, kind.arrow_data_type(), true))
        .collect();
    let geom_type = GeoArrowType::Wkb(WkbType::new(Arc::new(GeoMetadata::default())));
    fields.push(geom_type.to_field(GEOMETRY_COL, true));
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

    RecordBatch::try_new(schema, columns)
        .map_err(|e| KanyuError::Other(format!("batch 构造失败: {e}")))
}

/// GeoArrow RecordBatch → FeatureCollection：几何列按列名 + Binary 类型识别
/// （不依赖扩展元数据），属性列按 Arrow 类型原生映射 JSON 类型。
fn batch_to_collection(batch: &RecordBatch) -> Result<geojson::FeatureCollection> {
    let schema = batch.schema();
    let geom_idx = schema
        .index_of(GEOMETRY_COL)
        .map_err(|_| KanyuError::Other(format!("batch 缺少几何列 '{GEOMETRY_COL}'")))?;
    let geom_arr = batch.column(geom_idx);
    let mut features = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let geometry = if geom_arr.is_null(row) {
            None
        } else {
            Some(geojson::Geometry::new(wkb_decode_geom(
                parquet_binary_value(geom_arr, row)?,
            )?))
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
    Ok(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    })
}

/// WKB 头部的几何类型名（只读类型码，不解析坐标）。
fn wkb_type_name(bytes: &[u8]) -> Option<&'static str> {
    let le = match bytes.first()? {
        0 => false,
        1 => true,
        _ => return None,
    };
    let b: [u8; 4] = bytes.get(1..5)?.try_into().ok()?;
    let code = if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    };
    Some(match code {
        1 => "Point",
        2 => "LineString",
        3 => "Polygon",
        4 => "MultiPoint",
        5 => "MultiLineString",
        6 => "MultiPolygon",
        7 => "GeometryCollection",
        _ => return None,
    })
}

/// FeatureCollection → GeoParquet 字节串：复用统一入列的 RecordBatch，
/// geo 元数据、geometry_types 与 bbox 由 geoparquet crate 编码器生成。
fn collection_to_geoparquet(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
    use geoparquet::writer::{GeoParquetRecordBatchEncoder, GeoParquetWriterOptionsBuilder};
    use parquet::arrow::ArrowWriter;

    let batch = collection_to_batch(collection)?;
    let schema = batch.schema();
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

/// DXF → FeatureCollection：POINT/LINE/LWPOLYLINE/POLYLINE/CIRCLE/ARC 映射为
/// GeoJSON 几何（CIRCLE/ARC 按 64 分段折线近似），其余实体跳过不报错。
/// 每个要素写属性 `layer`（DXF 图层名）；实体颜色非 ByLayer 时写 `color_index`（ACI）。
fn dxf_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    let drawing = dxf::Drawing::load_file(path).map_err(|e| {
        KanyuError::Other(format!(
            "dxf 读取失败（{path}）：{e}；文件可能损坏或不是有效的 DXF"
        ))
    })?;

    let mut features = Vec::new();
    for entity in drawing.entities() {
        let Some(value) = dxf_entity_to_geojson(&entity.specific) else {
            continue;
        };
        let mut properties = serde_json::Map::new();
        properties.insert(
            "layer".to_string(),
            serde_json::Value::String(entity.common.layer.clone()),
        );
        if !entity.common.color.is_by_layer() {
            if let Some(idx) = entity.common.color.index() {
                properties.insert("color_index".to_string(), serde_json::Value::from(idx));
            }
        }
        features.push(geojson::Feature {
            bbox: None,
            geometry: Some(geojson::Geometry::new(value)),
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

/// DXF 实体 → GeoJSON 几何；不支持的实体类型返回 None（跳过）。
fn dxf_entity_to_geojson(specific: &dxf::entities::EntityType) -> Option<geojson::Value> {
    use dxf::entities::EntityType;
    match specific {
        EntityType::ModelPoint(p) => Some(geojson::Value::Point(vec![p.location.x, p.location.y])),
        EntityType::Line(l) => Some(geojson::Value::LineString(vec![
            vec![l.p1.x, l.p1.y],
            vec![l.p2.x, l.p2.y],
        ])),
        EntityType::LwPolyline(pl) => {
            let positions: Vec<Vec<f64>> = pl.vertices.iter().map(|v| vec![v.x, v.y]).collect();
            dxf_polyline_to_geojson(positions, pl.is_closed())
        }
        EntityType::Polyline(pl) => {
            // 跳过 3D 多边形网格（16）与多面网格（64）：非线/面几何。
            if pl.flags & (0x10 | 0x40) != 0 {
                return None;
            }
            let positions: Vec<Vec<f64>> = pl
                .vertices()
                .map(|v| vec![v.location.x, v.location.y])
                .collect();
            dxf_polyline_to_geojson(positions, pl.is_closed())
        }
        EntityType::Circle(c) => {
            if c.radius <= 0.0 {
                return None;
            }
            let ring = dxf_arc_positions(c.center.x, c.center.y, c.radius, 0.0, 360.0);
            Some(geojson::Value::Polygon(vec![ring]))
        }
        EntityType::Arc(a) => {
            if a.radius <= 0.0 {
                return None;
            }
            Some(geojson::Value::LineString(dxf_arc_positions(
                a.center.x,
                a.center.y,
                a.radius,
                a.start_angle,
                a.end_angle,
            )))
        }
        _ => None,
    }
}

/// 折线顶点列 → 开放 LineString / 闭合 Polygon（单环，首尾自动闭合）。
/// 顶点不足以构成线（<2）或面（<3）时返回 None。
fn dxf_polyline_to_geojson(mut positions: Vec<Vec<f64>>, closed: bool) -> Option<geojson::Value> {
    if closed {
        if positions.len() < 3 {
            return None;
        }
        if positions.first() != positions.last() {
            positions.push(positions[0].clone());
        }
        Some(geojson::Value::Polygon(vec![positions]))
    } else {
        if positions.len() < 2 {
            return None;
        }
        Some(geojson::Value::LineString(positions))
    }
}

/// 圆/弧按 64 分段折线近似（DXF 角度为度数，逆时针；end<=start 时按跨 360° 处理）。
fn dxf_arc_positions(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> Vec<Vec<f64>> {
    const SEGMENTS: usize = 64;
    let sweep = if end_deg > start_deg {
        end_deg - start_deg
    } else {
        end_deg + 360.0 - start_deg
    };
    (0..=SEGMENTS)
        .map(|i| {
            let theta = (start_deg + sweep * i as f64 / SEGMENTS as f64).to_radians();
            vec![cx + r * theta.cos(), cy + r * theta.sin()]
        })
        .collect()
}

/// FeatureCollection → DXF 字符串（显式 R2000 版本写出：crate 默认 R12
/// 不支持 LWPOLYLINE（MinVersion=R14），实体将被静默跳过）。
/// 无几何要素与 GeometryCollection 跳过；Polygon 仅写外环（洞舍弃）。
fn collection_to_dxf(collection: &geojson::FeatureCollection) -> Result<String> {
    let mut drawing = dxf::Drawing::new();
    drawing.header.version = dxf::enums::AcadVersion::R2000;
    for feature in &collection.features {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        dxf_push_geometry(&mut drawing, &geom.value);
    }

    let mut buf: Vec<u8> = Vec::new();
    drawing
        .save(&mut buf)
        .map_err(|e| KanyuError::Other(format!("dxf 写出失败: {e}")))?;
    String::from_utf8(buf).map_err(|e| KanyuError::Other(format!("dxf 编码失败: {e}")))
}

/// 单个 geojson 几何 → DXF 实体（Multi* 拆为多个实体）。
fn dxf_push_geometry(drawing: &mut dxf::Drawing, value: &geojson::Value) {
    use dxf::entities::{Entity, EntityType, ModelPoint};

    match value {
        geojson::Value::Point(pos) => {
            drawing.add_entity(Entity::new(EntityType::ModelPoint(ModelPoint {
                location: dxf::Point::new(pos[0], pos[1], 0.0),
                ..Default::default()
            })));
        }
        geojson::Value::MultiPoint(pts) => {
            for pos in pts {
                dxf_push_geometry(drawing, &geojson::Value::Point(pos.clone()));
            }
        }
        geojson::Value::LineString(line) => {
            if line.len() < 2 {
                return;
            }
            drawing.add_entity(Entity::new(EntityType::LwPolyline(dxf_lwpolyline(
                line, false,
            ))));
        }
        geojson::Value::MultiLineString(lines) => {
            for line in lines {
                dxf_push_geometry(drawing, &geojson::Value::LineString(line.clone()));
            }
        }
        geojson::Value::Polygon(rings) => {
            if let Some(outer) = rings.first() {
                drawing.add_entity(Entity::new(EntityType::LwPolyline(dxf_lwpolyline(
                    outer, true,
                ))));
            }
        }
        geojson::Value::MultiPolygon(polys) => {
            for rings in polys {
                dxf_push_geometry(drawing, &geojson::Value::Polygon(rings.clone()));
            }
        }
        // GeometryCollection 暂不展开（见 to_dxf_string 文档）。
        geojson::Value::GeometryCollection(_) => {}
    }
}

/// 顶点列 → LwPolyline（closed 时设闭合标志并去掉重复的首尾点）。
fn dxf_lwpolyline(positions: &[Vec<f64>], closed: bool) -> dxf::entities::LwPolyline {
    let mut positions = positions;
    if closed && positions.len() > 1 && positions.first() == positions.last() {
        positions = &positions[..positions.len() - 1];
    }
    let mut polyline = dxf::entities::LwPolyline {
        vertices: positions
            .iter()
            .map(|p| dxf::LwPolylineVertex {
                x: p[0],
                y: p[1],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };
    if closed {
        polyline.set_is_closed(true);
    }
    polyline
}

/// KML → FeatureCollection：Document/Folder 嵌套展平取全部 Placemark；
/// Point/LineString/LinearRing/Polygon（含内环洞）/MultiGeometry 映射，
/// name/description/ExtendedData（Data/SimpleData/SchemaData）写为同名属性
/// （ExtendedData 值按 CSV 规则数值化）。无几何的 Placemark 跳过。
/// KMZ（zip 容器）本轮不支持，返回待集成错误。
fn kml_to_collection(path: &str) -> Result<geojson::FeatureCollection> {
    if path.to_ascii_lowercase().ends_with(".kmz") {
        // KMZ（zip 容器）：内存解包 → 主 doc.kml（或首个 .kml 条目）→ KML 路径。
        let kml_text = kmz_extract_kml(path)?;
        return kml_text_to_collection(&kml_text, path);
    }
    let mut reader = kml::KmlReader::from_path(path)
        .map_err(|e| KanyuError::Other(format!("kml 读取失败（{path}）：{e}")))?;
    let doc: kml::Kml<f64> = reader.read().map_err(|e| {
        KanyuError::Other(format!(
            "kml 解析失败（{path}）：{e}；文件可能损坏或不是有效的 KML"
        ))
    })?;
    Ok(kml_doc_to_collection(&doc))
}

/// KMZ → KML 文本：主条目 doc.kml 优先，否则首个 .kml 条目（多 KML 条目
/// 只取首个，注释即契约）；zip 损坏/无 KML 条目中文结构化错误。
fn kmz_extract_kml(path: &str) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        KanyuError::Other(format!(
            "kmz 解包失败（{path}）：{e}；文件可能损坏或不是有效的 zip 容器"
        ))
    })?;
    // doc.kml 优先；否则按文件名排序后的首个 .kml 条目（确定性）。
    let mut candidates: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .filter(|name| name.to_ascii_lowercase().ends_with(".kml"))
        .collect();
    candidates.sort();
    let entry_name = if candidates.iter().any(|n| n == "doc.kml") {
        "doc.kml".to_string()
    } else {
        match candidates.into_iter().next() {
            Some(name) => name,
            None => {
                return Err(KanyuError::Other(format!(
                    "kmz 容器内无 .kml 条目（{path}）：不是有效的 KMZ"
                )))
            }
        }
    };
    let mut entry = archive.by_name(&entry_name).map_err(|e| {
        KanyuError::Other(format!("kmz 条目 '{entry_name}' 读取失败（{path}）：{e}"))
    })?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).map_err(|e| {
        KanyuError::Other(format!(
            "kmz 条目 '{entry_name}' 非合法 UTF-8（{path}）：{e}"
        ))
    })?;
    Ok(text)
}

/// KML 文本 → FeatureCollection（kmz 解包路径与直接 .kml 路径共用）。
fn kml_text_to_collection(text: &str, path: &str) -> Result<geojson::FeatureCollection> {
    let mut reader = kml::KmlReader::from_string(text);
    let doc: kml::Kml<f64> = reader.read().map_err(|e| {
        KanyuError::Other(format!(
            "kml 解析失败（{path}）：{e}；文件可能损坏或不是有效的 KML"
        ))
    })?;
    Ok(kml_doc_to_collection(&doc))
}

/// KML 文档树 → FeatureCollection（Placemark 展平）。
fn kml_doc_to_collection(doc: &kml::Kml<f64>) -> geojson::FeatureCollection {
    let mut features = Vec::new();
    kml_collect_placemarks(doc, &mut features);
    geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }
}

/// 递归遍历 KML 文档树，收集 Placemark 要素。
fn kml_collect_placemarks(kml: &kml::Kml<f64>, out: &mut Vec<geojson::Feature>) {
    match kml {
        kml::Kml::KmlDocument(d) => {
            for e in &d.elements {
                kml_collect_placemarks(e, out);
            }
        }
        kml::Kml::Document { elements, .. } => {
            for e in elements {
                kml_collect_placemarks(e, out);
            }
        }
        kml::Kml::Folder(f) => {
            for e in &f.elements {
                kml_collect_placemarks(e, out);
            }
        }
        kml::Kml::Placemark(p) => {
            if let Some(feature) = kml_placemark_to_feature(p) {
                out.push(feature);
            }
        }
        _ => {}
    }
}

/// Placemark → Feature；无几何返回 None（跳过，不产生脏数据）。
fn kml_placemark_to_feature(p: &kml::types::Placemark<f64>) -> Option<geojson::Feature> {
    let geometry = p.geometry.as_ref().and_then(kml_geometry_to_geojson)?;
    let mut properties = serde_json::Map::new();
    if let Some(name) = &p.name {
        properties.insert("name".to_string(), serde_json::Value::String(name.clone()));
    }
    if let Some(desc) = &p.description {
        properties.insert(
            "description".to_string(),
            serde_json::Value::String(desc.clone()),
        );
    }
    for el in &p.children {
        if el.name == "ExtendedData" {
            kml_extended_data_props(el, &mut properties);
        }
    }
    Some(geojson::Feature {
        bbox: None,
        geometry: Some(geojson::Geometry::new(geometry)),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    })
}

/// ExtendedData 元素 → 属性：支持 Data/value、SimpleData 与 SchemaData 三种
/// 标准形式；值按 CSV 规则数值化（可解析为 f64 则 Number，否则 String）。
fn kml_extended_data_props(
    el: &kml::types::Element,
    properties: &mut serde_json::Map<String, serde_json::Value>,
) {
    for child in &el.children {
        match child.name.as_str() {
            "Data" => {
                if let Some(name) = child.attrs.get("name") {
                    let text = child
                        .children
                        .iter()
                        .find(|c| c.name == "value")
                        .and_then(|c| c.content.as_deref());
                    if let Some(text) = text {
                        properties.insert(name.clone(), csv_like_value(text));
                    }
                }
            }
            "SimpleData" => {
                if let (Some(name), Some(text)) = (child.attrs.get("name"), &child.content) {
                    properties.insert(name.clone(), csv_like_value(text));
                }
            }
            "SchemaData" => kml_extended_data_props(child, properties),
            _ => {}
        }
    }
}

/// CSV 同款规则：数值优先，退化为字符串。
fn csv_like_value(raw: &str) -> serde_json::Value {
    raw.parse::<f64>()
        .map(serde_json::Value::from)
        .unwrap_or_else(|_| serde_json::Value::String(raw.to_string()))
}

/// KML 几何 → geojson 几何（z 丢弃；LinearRing 按闭合环 → 单环 Polygon）。
fn kml_geometry_to_geojson(g: &kml::types::Geometry<f64>) -> Option<geojson::Value> {
    use kml::types::Geometry;
    match g {
        Geometry::Point(p) => Some(geojson::Value::Point(vec![p.coord.x, p.coord.y])),
        Geometry::LineString(l) => Some(geojson::Value::LineString(kml_positions(&l.coords))),
        Geometry::LinearRing(r) => Some(geojson::Value::Polygon(vec![kml_positions(&r.coords)])),
        Geometry::Polygon(p) => {
            let mut rings = vec![kml_positions(&p.outer.coords)];
            rings.extend(p.inner.iter().map(|r| kml_positions(&r.coords)));
            Some(geojson::Value::Polygon(rings))
        }
        Geometry::MultiGeometry(m) => kml_multi_to_geojson(m),
        // Element（Model 占位）及未来非穷举变体：跳过。
        _ => None,
    }
}

fn kml_positions(coords: &[kml::types::Coord<f64>]) -> Vec<Vec<f64>> {
    coords.iter().map(|c| vec![c.x, c.y]).collect()
}

/// MultiGeometry：递归展平嵌套后，同类子几何合并为对应 Multi*，
/// 异类合并为 GeometryCollection（不丢数据，1 Placemark 保持 1 Feature）。
fn kml_multi_to_geojson(m: &kml::types::MultiGeometry<f64>) -> Option<geojson::Value> {
    let mut flat = Vec::new();
    kml_flatten_geometries(&m.geometries, &mut flat);
    let values: Vec<geojson::Value> = flat
        .into_iter()
        .filter_map(kml_geometry_to_geojson)
        .collect();
    if values.is_empty() {
        return None;
    }
    let all = |f: fn(&geojson::Value) -> bool| values.iter().all(f);
    if all(|v| matches!(v, geojson::Value::Point(_))) {
        let pts = values
            .into_iter()
            .map(|v| match v {
                geojson::Value::Point(p) => p,
                _ => unreachable!(),
            })
            .collect();
        Some(geojson::Value::MultiPoint(pts))
    } else if all(|v| matches!(v, geojson::Value::LineString(_))) {
        let lines = values
            .into_iter()
            .map(|v| match v {
                geojson::Value::LineString(l) => l,
                _ => unreachable!(),
            })
            .collect();
        Some(geojson::Value::MultiLineString(lines))
    } else if all(|v| matches!(v, geojson::Value::Polygon(_))) {
        let polys = values
            .into_iter()
            .map(|v| match v {
                geojson::Value::Polygon(p) => p,
                _ => unreachable!(),
            })
            .collect();
        Some(geojson::Value::MultiPolygon(polys))
    } else {
        let geoms = values.into_iter().map(geojson::Geometry::new).collect();
        Some(geojson::Value::GeometryCollection(geoms))
    }
}

/// 递归展开嵌套的 MultiGeometry。
fn kml_flatten_geometries<'a>(
    geoms: &'a [kml::types::Geometry<f64>],
    out: &mut Vec<&'a kml::types::Geometry<f64>>,
) {
    for g in geoms {
        match g {
            kml::types::Geometry::MultiGeometry(m) => kml_flatten_geometries(&m.geometries, out),
            _ => out.push(g),
        }
    }
}

/// FeatureCollection → KML 字符串（KML 2.2；kml > Document > Placemark*）。
/// 无几何要素跳过（KML Placemark 允许无几何，但与读取侧"跳过"口径保持一致）。
fn collection_to_kml(collection: &geojson::FeatureCollection) -> Result<String> {
    use std::collections::HashMap;

    let placemarks: Vec<kml::Kml<f64>> = collection
        .features
        .iter()
        .filter_map(kml_feature_to_placemark)
        .map(kml::Kml::Placemark)
        .collect();
    let doc = kml::Kml::KmlDocument(kml::types::KmlDocument {
        version: kml::KmlVersion::V22,
        attrs: HashMap::new(),
        elements: vec![kml::Kml::Document {
            attrs: HashMap::new(),
            elements: placemarks,
        }],
    });

    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = kml::KmlWriter::from_writer(&mut buf);
        writer
            .write(&doc)
            .map_err(|e| KanyuError::Other(format!("kml 写出失败: {e}")))?;
    }
    String::from_utf8(buf).map_err(|e| KanyuError::Other(format!("kml 编码失败: {e}")))
}

/// FeatureCollection → KMZ 字节串：zip 容器（deflate）+ doc.kml 单条目。
fn collection_to_kmz(collection: &geojson::FeatureCollection) -> Result<Vec<u8>> {
    let kml_text = collection_to_kml(collection)?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file("doc.kml", options)
            .map_err(|e| KanyuError::Other(format!("kmz 写出失败: {e}")))?;
        std::io::Write::write_all(&mut writer, kml_text.as_bytes())
            .map_err(|e| KanyuError::Other(format!("kmz 写出失败: {e}")))?;
        writer
            .finish()
            .map_err(|e| KanyuError::Other(format!("kmz 写出失败: {e}")))?;
    }
    Ok(cursor.into_inner())
}

/// Feature → Placemark：`name`/`description` 写为同名字段，其余属性写入
/// ExtendedData/SimpleData（值转字符串）；无几何返回 None。
fn kml_feature_to_placemark(feature: &geojson::Feature) -> Option<kml::types::Placemark<f64>> {
    use std::collections::HashMap;

    let geometry = kml_geometry_from_geojson(&feature.geometry.as_ref()?.value);
    let mut name = None;
    let mut description = None;
    let mut simple_data = Vec::new();
    if let Some(props) = &feature.properties {
        for (k, v) in props {
            match k.as_str() {
                "name" => name = v.as_str().map(str::to_string),
                "description" => description = v.as_str().map(str::to_string),
                _ => {
                    if !v.is_null() {
                        simple_data.push(kml::types::Element {
                            name: "SimpleData".to_string(),
                            attrs: HashMap::from([("name".to_string(), k.clone())]),
                            content: Some(match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            }),
                            children: vec![],
                        });
                    }
                }
            }
        }
    }
    let children = if simple_data.is_empty() {
        vec![]
    } else {
        vec![kml::types::Element {
            name: "ExtendedData".to_string(),
            attrs: HashMap::new(),
            content: None,
            children: simple_data,
        }]
    };
    Some(kml::types::Placemark {
        name,
        description,
        geometry: Some(geometry),
        style_url: None,
        attrs: HashMap::new(),
        children,
    })
}

/// geojson 几何 → KML 几何（全六类型；Multi* 与 GeometryCollection → MultiGeometry；z 丢弃）。
fn kml_geometry_from_geojson(value: &geojson::Value) -> kml::types::Geometry<f64> {
    use kml::types::{Geometry, LineString, LinearRing, MultiGeometry, Point, Polygon};

    let point = |pos: &[f64]| {
        Geometry::Point(Point {
            coord: kml_coord(pos),
            ..Default::default()
        })
    };
    let line = |positions: &[Vec<f64>]| {
        Geometry::LineString(LineString {
            coords: positions.iter().map(|p| kml_coord(p)).collect(),
            ..Default::default()
        })
    };
    let polygon = |rings: &[Vec<Vec<f64>>]| {
        let mut iter = rings.iter();
        let outer = LinearRing {
            coords: iter
                .next()
                .map(|r| r.iter().map(|p| kml_coord(p)).collect())
                .unwrap_or_default(),
            ..Default::default()
        };
        let inner = iter
            .map(|r| LinearRing {
                coords: r.iter().map(|p| kml_coord(p)).collect(),
                ..Default::default()
            })
            .collect();
        Geometry::Polygon(Polygon::new(outer, inner))
    };

    match value {
        geojson::Value::Point(pos) => point(pos),
        geojson::Value::MultiPoint(pts) => Geometry::MultiGeometry(MultiGeometry {
            geometries: pts.iter().map(|p| point(p)).collect(),
            ..Default::default()
        }),
        geojson::Value::LineString(l) => line(l),
        geojson::Value::MultiLineString(lines) => Geometry::MultiGeometry(MultiGeometry {
            geometries: lines.iter().map(|l| line(l)).collect(),
            ..Default::default()
        }),
        geojson::Value::Polygon(rings) => polygon(rings),
        geojson::Value::MultiPolygon(polys) => Geometry::MultiGeometry(MultiGeometry {
            geometries: polys.iter().map(|p| polygon(p)).collect(),
            ..Default::default()
        }),
        geojson::Value::GeometryCollection(geoms) => Geometry::MultiGeometry(MultiGeometry {
            geometries: geoms
                .iter()
                .map(|g| kml_geometry_from_geojson(&g.value))
                .collect(),
            ..Default::default()
        }),
    }
}

/// geojson 位置 → KML 坐标（z 丢弃）。
fn kml_coord(pos: &[f64]) -> kml::types::Coord<f64> {
    kml::types::Coord {
        x: pos.first().copied().unwrap_or(0.0),
        y: pos.get(1).copied().unwrap_or(0.0),
        z: None,
    }
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

    /// 对单个属性 JSON 值求值（行缺失/空值由调用方短路）。
    /// 语义与 GeoJSON 载体时代逐比特一致：双数值按 f64 数值比较，
    /// 其余按 to_string 字典序（Eq/Ne 用 JSON 值相等）。
    fn matches_value(&self, actual: &serde_json::Value) -> bool {
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
            batch: collection_to_batch(&collection).unwrap(),
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

    /// rust_xlsxwriter 生成 xlsx fixture（可复现；calamine 无写出侧）。
    fn write_xlsx_fixture(path: &std::path::Path) {
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        for (col, name) in ["name", "lon", "lat", "height", "active"]
            .iter()
            .enumerate()
        {
            sheet.write_string(0, col as u16, *name).unwrap();
        }
        let rows: [(f64, f64, f64, &str, bool); 3] = [
            (116.39, 39.90, 80.0, "甲", true),
            (116.40, 39.91, 30.0, "乙", false),
            (116.41, 39.92, 55.5, "丙", true),
        ];
        for (r, (lon, lat, height, name, active)) in rows.iter().enumerate() {
            let r = (r + 1) as u32;
            sheet.write_string(r, 0, *name).unwrap();
            sheet.write_number(r, 1, *lon).unwrap();
            sheet.write_number(r, 2, *lat).unwrap();
            sheet.write_number(r, 3, *height).unwrap();
            sheet.write_boolean(r, 4, *active).unwrap();
        }
        workbook.save(path).unwrap();
    }

    #[test]
    fn xlsx_load_detects_coords_and_types() {
        let dir = std::env::temp_dir().join("kanyu_core_xlsx_ok");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pts.xlsx");
        write_xlsx_fixture(&path);

        let layer = Layer::load("pts", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        // format 归属 csv 家族（csv/xlsx/tsv 同一条目）。
        assert_eq!(s.format, "csv");
        assert_eq!(s.geometry_types, vec!["Point"]);
        assert_eq!(s.fields, vec!["active", "height", "name"]);
        // 原生数值类型化：height 可数值比较查询。
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 2);
        // 原生布尔类型化。
        let a = find_by_name(&layer, "甲");
        assert_eq!(
            a.properties.as_ref().unwrap()["active"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            a.properties.as_ref().unwrap()["height"].as_f64(),
            Some(80.0)
        );
    }

    #[test]
    fn xlsx_without_coord_columns_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_xlsx_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("no_coords.xlsx");
        let mut workbook = rust_xlsxwriter::Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.write_string(0, 0, "a").unwrap();
        sheet.write_string(0, 1, "b").unwrap();
        sheet.write_number(1, 0, 1.0).unwrap();
        sheet.write_number(1, 1, 2.0).unwrap();
        workbook.save(&path).unwrap();

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
        let collection = layer.collection();
        let props = collection.features[0].properties.as_ref().unwrap();
        assert_eq!(props["name"].as_str().unwrap().trim_end(), "甲");
    }

    #[test]
    fn shp_load_polygon_preserves_holes() {
        let dir = std::env::temp_dir().join("kanyu_core_shp_hole");
        let (_, poly_path) = write_test_shps(&dir);
        let layer = Layer::load("zones", poly_path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 1);
        assert_eq!(layer.summary().geometry_types, vec!["Polygon"]);
        let collection = layer.collection();
        let geojson::Value::Polygon(rings) =
            &collection.features[0].geometry.as_ref().unwrap().value
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

    #[test]
    fn dxf_roundtrip_preserves_geometries() {
        let text = Layer::to_dxf_string(&fgb_test_collection()).unwrap();
        let dir = std::env::temp_dir().join("kanyu_core_dxf_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.dxf");
        std::fs::write(&path, &text).unwrap();

        let layer = Layer::load("mixed", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        assert_eq!(s.format, "dxf");
        assert_eq!(s.geometry_types, vec!["LineString", "Point", "Polygon"]);
        // 每个要素都应带回图层属性（写出统一落图层 "0"）。
        for f in &layer.collection().features {
            let props = f.properties.as_ref().unwrap();
            assert_eq!(props["layer"], serde_json::Value::String("0".to_string()));
        }
    }

    #[test]
    fn dxf_load_circle_approximates_polygon() {
        let dir = std::env::temp_dir().join("kanyu_core_dxf_circle");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("circle.dxf");
        let mut drawing = dxf::Drawing::new();
        drawing.add_entity(dxf::entities::Entity::new(
            dxf::entities::EntityType::Circle(dxf::entities::Circle {
                center: dxf::Point::new(5.0, 5.0, 0.0),
                radius: 2.0,
                ..Default::default()
            }),
        ));
        drawing.save_file(&path).unwrap();

        let layer = Layer::load("circle", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 1);
        let collection = layer.collection();
        let geojson::Value::Polygon(rings) =
            &collection.features[0].geometry.as_ref().unwrap().value
        else {
            panic!("CIRCLE 应近似为 Polygon");
        };
        // 64 分段 + 首尾闭合 = 65 个顶点。
        assert!(
            rings[0].len() >= 60,
            "近似顶点数应 >= 60，实际 {}",
            rings[0].len()
        );
    }

    #[test]
    fn dxf_skips_unknown_entities() {
        let dir = std::env::temp_dir().join("kanyu_core_dxf_mtext");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mtext.dxf");
        let mut drawing = dxf::Drawing::new();
        drawing.add_entity(dxf::entities::Entity::new(
            dxf::entities::EntityType::MText(dxf::entities::MText {
                text: "标注文字".to_string(),
                ..Default::default()
            }),
        ));
        drawing.save_file(&path).unwrap();

        // MTEXT 读取不报错，且不产生要素。
        let layer = Layer::load("mtext", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 0);
    }

    #[test]
    fn dxf_corrupt_file_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_dxf_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.dxf");
        std::fs::write(&path, b"this is not a dxf file\x00\x01\x02").unwrap();
        let err = Layer::load("garbage", path.to_str().unwrap())
            .err()
            .expect("损坏文件应报错");
        assert!(
            err.to_string().contains("dxf"),
            "错误应指出 dxf 读取问题: {err}"
        );
    }

    #[test]
    fn kml_roundtrip_preserves_features_and_props() {
        let text = Layer::to_kml_string(&fgb_test_collection()).unwrap();
        let dir = std::env::temp_dir().join("kanyu_core_kml_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.kml");
        std::fs::write(&path, &text).unwrap();

        let layer = Layer::load("mixed", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        assert_eq!(s.format, "kml");
        assert_eq!(s.geometry_types, vec!["LineString", "Point", "Polygon"]);
        assert!(s.fields.contains(&"height".to_string()));
        assert!(s.fields.contains(&"name".to_string()));
        // height 经 SimpleData 写出为文本，读回按规则数值化为 Number。
        let a = find_by_name(&layer, "甲");
        let props = a.properties.as_ref().unwrap();
        assert_eq!(props["height"].as_f64(), Some(80.0));
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 2);
    }

    #[test]
    fn kml_load_polygon_preserves_holes() {
        let dir = std::env::temp_dir().join("kanyu_core_kml_hole");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hole.kml");
        std::fs::write(
            &path,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Placemark>
<name>带洞地块</name>
<Polygon>
  <outerBoundaryIs><LinearRing><coordinates>0,0 0,10 10,10 10,0 0,0</coordinates></LinearRing></outerBoundaryIs>
  <innerBoundaryIs><LinearRing><coordinates>2,2 2,4 4,4 4,2 2,2</coordinates></LinearRing></innerBoundaryIs>
</Polygon>
</Placemark></Document></kml>"#,
        )
        .unwrap();

        let layer = Layer::load("hole", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 1);
        let collection = layer.collection();
        let geojson::Value::Polygon(rings) =
            &collection.features[0].geometry.as_ref().unwrap().value
        else {
            panic!("应为 Polygon 几何");
        };
        // 外环 + 1 个内环（洞）：interiors 非空。
        assert_eq!(rings.len(), 2);
        assert_eq!(
            collection.features[0].properties.as_ref().unwrap()["name"],
            serde_json::Value::String("带洞地块".to_string())
        );
    }

    #[test]
    fn kml_corrupt_file_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_kml_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.kml");
        std::fs::write(&path, b"this is not xml at all <<<").unwrap();
        let err = Layer::load("garbage", path.to_str().unwrap())
            .err()
            .expect("损坏文件应报错");
        assert!(
            err.to_string().contains("kml"),
            "错误应指出 kml 读取问题: {err}"
        );
    }

    #[test]
    fn kmz_roundtrip_preserves_features() {
        let bytes = Layer::to_kmz_bytes(&fgb_test_collection()).unwrap();
        let dir = std::env::temp_dir().join("kanyu_core_kmz_roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.kmz");
        std::fs::write(&path, &bytes).unwrap();

        let layer = Layer::load("mixed", path.to_str().unwrap()).unwrap();
        assert_eq!(layer.len(), 3);
        let s = layer.summary();
        assert_eq!(s.format, "kml");
        assert_eq!(s.geometry_types, vec!["LineString", "Point", "Polygon"]);
        // 属性经 SimpleData 数值化读回。
        let a = find_by_name(&layer, "甲");
        assert_eq!(
            a.properties.as_ref().unwrap()["height"].as_f64(),
            Some(80.0)
        );
    }

    #[test]
    fn kmz_corrupt_zip_gives_clear_error() {
        let dir = std::env::temp_dir().join("kanyu_core_kmz_corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.kmz");
        std::fs::write(&path, b"this is not a zip file").unwrap();
        let err = Layer::load("garbage", path.to_str().unwrap())
            .err()
            .expect("损坏 zip 应报错");
        assert!(
            err.to_string().contains("kmz 解包失败"),
            "错误应指出 kmz 解包问题: {err}"
        );
    }

    #[test]
    fn kmz_without_kml_entry_gives_clear_error() {
        // 合法 zip 但无 .kml 条目。
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("readme.txt", options).unwrap();
            std::io::Write::write_all(&mut writer, b"no kml here").unwrap();
            writer.finish().unwrap();
        }
        let dir = std::env::temp_dir().join("kanyu_core_kmz_nokml");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("nokml.kmz");
        std::fs::write(&path, cursor.into_inner()).unwrap();
        let err = Layer::load("nokml", path.to_str().unwrap())
            .err()
            .expect("无 kml 条目应报错");
        assert!(
            err.to_string().contains("无 .kml 条目"),
            "错误应指出无 kml 条目: {err}"
        );
    }

    /// 迁移专项测试集合：属性覆盖 Int64（grade）/Float64（height，全非整数）/
    /// Utf8（name、usage）/Boolean（active），几何 Point/LineString/Polygon。
    fn batch_test_collection() -> geojson::FeatureCollection {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},
                 "properties":{"name":"甲","height":80.5,"grade":2,"usage":"office","active":true}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{"name":"乙","height":30.5,"grade":1,"usage":"residential","active":false}},
                {"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[0,0],[0,4],[4,4],[4,0],[0,0]]]},
                 "properties":{"name":"丙","height":55.5,"grade":2,"usage":"office","active":true}}
            ]
        }"#
        .parse()
        .unwrap();
        geojson::FeatureCollection::try_from(gj).unwrap()
    }

    fn batch_test_layer() -> Layer {
        Layer {
            id: "batch_test".into(),
            format: "geojson".into(),
            batch: collection_to_batch(&batch_test_collection()).unwrap(),
        }
    }

    #[test]
    fn batch_schema_carries_geoarrow_metadata() {
        let schema = batch_test_layer().batch().schema();
        let geom = schema.field_with_name(GEOMETRY_COL).unwrap();
        assert_eq!(geom.data_type(), &arrow_schema::DataType::Binary);
        // geoarrow.wkb 扩展元数据（真 GeoArrow，非裸 Binary 列）。
        assert_eq!(
            geom.metadata().get("ARROW:extension:name"),
            Some(&"geoarrow.wkb".to_string())
        );
        let ty = |name: &str| schema.field_with_name(name).unwrap().data_type().clone();
        assert_eq!(ty("grade"), arrow_schema::DataType::Int64);
        assert_eq!(ty("height"), arrow_schema::DataType::Float64);
        assert_eq!(ty("name"), arrow_schema::DataType::Utf8);
        assert_eq!(ty("active"), arrow_schema::DataType::Boolean);
    }

    #[test]
    fn batch_roundtrip_is_lossless() {
        let original = batch_test_collection();
        let roundtripped = batch_to_collection(&collection_to_batch(&original).unwrap()).unwrap();
        // 属性无一整数（height 全带小数），Double/Bool/Utf8 表示稳定，
        // 几何坐标 f64 位级精确——全集合可字面比较。
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn query_semantics_hold_per_column_type() {
        let layer = batch_test_layer();
        // Int64 列数值比较。
        assert_eq!(layer.query("grade >= 2").unwrap().features.len(), 2);
        // Float64 列数值比较。
        assert_eq!(layer.query("height > 50").unwrap().features.len(), 2);
        // Utf8 列字符串相等。
        assert_eq!(layer.query("usage == office").unwrap().features.len(), 2);
        // Boolean 列。
        assert_eq!(layer.query("active == true").unwrap().features.len(), 2);
        // 不存在的列：全部不匹配（与原语义一致）。
        assert_eq!(layer.query("nonexist == 1").unwrap().features.len(), 0);
    }

    #[test]
    fn batch_and_collection_views_agree() {
        let layer = batch_test_layer();
        let collection = layer.collection();
        let batch = layer.batch();
        assert_eq!(batch.num_rows(), collection.features.len());
        // 同一行的列值与 feature 属性一致（以 height 为例）。
        let height_idx = batch.schema().index_of("height").unwrap();
        let heights = batch
            .column(height_idx)
            .as_any()
            .downcast_ref::<arrow_array::Float64Array>()
            .unwrap();
        for (row, feature) in collection.features.iter().enumerate() {
            let expected = feature
                .properties
                .as_ref()
                .and_then(|p| p.get("height"))
                .and_then(|v| v.as_f64());
            assert_eq!(Some(heights.value(row)), expected);
        }
        // batch 属性列集合 == summary().fields。
        let mut cols: Vec<String> = batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .filter(|n| n != GEOMETRY_COL)
            .collect();
        cols.sort();
        assert_eq!(cols, layer.summary().fields);
    }
}
