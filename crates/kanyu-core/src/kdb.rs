//! 堪舆数据库（KanyuDB，`.kdb`）—— 自研存档格式。
//!
//! ## 设计（KDB v1）
//!
//! KDB 文件 = **Arrow IPC 文件**，其 schema 元数据携带 `kanyu.*` 键：
//!
//! | 键 | 值 |
//! |----|----|
//! | `kanyu:format` | 恒为 `"kdb"`（读取校验） |
//! | `kanyu:format_version` | 恒为 `"1"` |
//! | `kanyu:producer` | 如 `"kanyu-core 0.14.0"` |
//!
//! 几何列为 WKB Binary 并携带 `geoarrow.wkb` 扩展元数据（与内存模型同构）。
//!
//! 这一选择的理由：① 与堪舆内存模型（GeoArrow RecordBatch）同构，
//! 读写零转换、**类型保真**（Int64/Float64/Utf8/Boolean 列原样往返，
//! 不经 GeoJSON 中间层）；② Arrow IPC 是开放标准，任何 Arrow 工具链
//! （Python pyarrow、DuckDB、Polars）均可直接读取——自研但不自闭；
//! ③ 流式 IPC 天然支持后续多批次大文件。
//!
//! v1 约束：单批次（一个 RecordBatch）。

use std::collections::HashMap;
use std::io::Cursor;

use arrow_array::RecordBatch;
use arrow_ipc::reader::FileReader;
use arrow_ipc::writer::FileWriter;
use arrow_schema::Schema;

use crate::error::{KanyuError, Result};

/// schema 元数据键：格式标识。
pub const KDB_FORMAT_KEY: &str = "kanyu:format";
/// schema 元数据键：格式版本。
pub const KDB_VERSION_KEY: &str = "kanyu:format_version";
/// schema 元数据键：生产者。
pub const KDB_PRODUCER_KEY: &str = "kanyu:producer";
/// KDB v1 格式标识值。
pub const KDB_FORMAT_VALUE: &str = "kdb";
/// KDB v1 版本值。
pub const KDB_VERSION_VALUE: &str = "1";

/// RecordBatch → KDB 字节流（注入 kanyu.* 元数据）。
pub fn batch_to_kdb(batch: &RecordBatch) -> Result<Vec<u8>> {
    let mut metadata: HashMap<String, String> = batch.schema().metadata().clone();
    metadata.insert(KDB_FORMAT_KEY.to_string(), KDB_FORMAT_VALUE.to_string());
    metadata.insert(KDB_VERSION_KEY.to_string(), KDB_VERSION_VALUE.to_string());
    metadata.insert(
        KDB_PRODUCER_KEY.to_string(),
        format!("kanyu-core {}", env!("CARGO_PKG_VERSION")),
    );
    let schema = Schema::new_with_metadata(batch.schema().fields().clone(), metadata);

    let mut buf = Vec::new();
    {
        let mut writer = FileWriter::try_new(&mut buf, &schema)
            .map_err(|e| KanyuError::Other(format!("kdb 写出初始化失败: {e}")))?;
        writer
            .write(batch)
            .map_err(|e| KanyuError::Other(format!("kdb 写出失败: {e}")))?;
        writer
            .finish()
            .map_err(|e| KanyuError::Other(format!("kdb 收尾失败: {e}")))?;
    }
    Ok(buf)
}

/// KDB 字节流 → RecordBatch（校验 kanyu:format；v1 单批次）。
pub fn kdb_to_batch(bytes: &[u8]) -> Result<RecordBatch> {
    let reader = FileReader::try_new(Cursor::new(bytes), None).map_err(|e| {
        KanyuError::Other(format!("不是合法的 kdb 文件（Arrow IPC 解析失败）: {e}"))
    })?;
    let schema = reader.schema();
    match schema.metadata().get(KDB_FORMAT_KEY).map(String::as_str) {
        Some(KDB_FORMAT_VALUE) => {}
        Some(other) => {
            return Err(KanyuError::Other(format!(
                "kanyu:format 标识异常: {other}（期望 {KDB_FORMAT_VALUE}）"
            )))
        }
        None => {
            return Err(KanyuError::Other(
                "缺少 kanyu:format 元数据——这不是堪舆数据库（.kdb）文件".to_string(),
            ))
        }
    }

    let mut batches = reader.into_iter();
    let first = match batches.next() {
        Some(Ok(b)) => b,
        Some(Err(e)) => return Err(KanyuError::Other(format!("kdb 批次读取失败: {e}"))),
        None => return Err(KanyuError::Other("kdb 文件不含任何批次".to_string())),
    };
    if batches.next().is_some() {
        return Err(KanyuError::Other(
            "kdb v1 仅支持单批次文件（该文件含多批次）".to_string(),
        ));
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::Layer;

    fn sample_layer() -> Layer {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},
                 "properties":{"name":"甲","height":80.5,"active":true,"rank":3}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{"name":"乙","height":30.25,"active":false,"rank":7}}
            ]
        }"#
        .parse()
        .unwrap();
        Layer::from_collection("t", geojson::FeatureCollection::try_from(gj).unwrap())
    }

    #[test]
    fn kdb_roundtrip_preserves_schema_and_types() {
        let layer = sample_layer();
        let bytes = batch_to_kdb(layer.batch()).unwrap();
        let batch = kdb_to_batch(&bytes).unwrap();

        assert_eq!(batch.num_rows(), 2);
        // schema 元数据：kanyu.* 注入 + geoarrow.wkb 扩展保留。
        let rt_schema = batch.schema();
        let md = rt_schema.metadata();
        assert_eq!(md.get(KDB_FORMAT_KEY).unwrap(), KDB_FORMAT_VALUE);
        assert_eq!(md.get(KDB_VERSION_KEY).unwrap(), KDB_VERSION_VALUE);
        let geom_field = rt_schema.field_with_name("geometry").unwrap();
        assert_eq!(
            geom_field
                .metadata()
                .get("ARROW:extension:name")
                .map(String::as_str),
            Some("geoarrow.wkb")
        );
        // 类型保真：与原始 batch 的列类型一致（不经 GeoJSON 中间层）。
        let orig_schema = layer.batch().schema();
        for name in ["height", "active", "rank", "name"] {
            let orig = orig_schema.field_with_name(name).unwrap().data_type();
            let rt = rt_schema.field_with_name(name).unwrap().data_type();
            assert_eq!(orig, rt, "列 {name} 类型不保真");
        }
        // 几何可读回：经 Layer 逆转换要素数一致。
        let back = Layer::from_batch("t2", batch);
        assert_eq!(back.len(), 2);
        assert_eq!(back.summary().fields, layer.summary().fields);
    }

    #[test]
    fn kdb_rejects_non_kdb_bytes() {
        let err = kdb_to_batch(b"garbage").unwrap_err();
        assert!(err.to_string().contains("kdb"), "错误应指明 kdb: {err}");
        // 合法 IPC 但无 kanyu:format（用普通 schema 写一个）。
        let layer = sample_layer();
        let schema = layer.batch().schema();
        let mut buf = Vec::new();
        {
            let mut w = FileWriter::try_new(&mut buf, &schema).unwrap();
            w.write(layer.batch()).unwrap();
            w.finish().unwrap();
        }
        let err = kdb_to_batch(&buf).unwrap_err();
        assert!(
            err.to_string().contains("kanyu:format"),
            "应报缺失格式标识: {err}"
        );
    }
}
