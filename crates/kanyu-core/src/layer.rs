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

        let text = std::fs::read_to_string(path)?;
        let collection = match caps.id {
            "geojson" => {
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
                csv_to_collection(&text, delimiter)?
            }
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
        let err = Layer::load("bad", path.to_str().unwrap()).err().expect("缺少坐标列应报错");
        assert!(
            err.to_string().contains("坐标列"),
            "错误应指出坐标列问题: {err}"
        );
    }
}
