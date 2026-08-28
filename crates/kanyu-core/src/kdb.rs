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
//!
//! ## 设计（KDB v2，多图层容器）
//!
//! v2 = **zip 容器**（deflate，纯 Rust 后端），承载**多命名图层**——
//! 面向《不动产登记数据库标准》的多表形态（ZDJBXX/JZD/JZX/ZJ… 单文件建库）：
//!
//! ```text
//! manifest.json   {"kanyu:format":"kdb","kanyu:format_version":"2",
//!                  "kanyu:producer":…,"layers":[{"name","path","rows"}]}
//! layers/<图层名>.kdb   每图层一个 v1 Arrow IPC 文件（逐层独立校验）
//! ```
//!
//! - 嗅探：zip 魔数 `PK\x03\x04` → v2；否则按 v1 单批次解析（**v1 完全兼容**）；
//! - 图层名唯一、禁含 `/` `\` `..`（zip 路径安全）；
//! - `kdb_to_batch` 遇 v2 明确报错指路 `kdb_to_layers`；`kdb_to_layers` 对
//!   v1 返回单图层（名 `"layer"`），两版统一入口。

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
/// KDB v2 版本值。
pub const KDB_V2_VERSION_VALUE: &str = "2";
/// v2 清单条目名。
pub const KDB_V2_MANIFEST: &str = "manifest.json";
/// v2 图层文件目录前缀。
pub const KDB_V2_LAYER_DIR: &str = "layers/";

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
    if is_kdb_v2(bytes) {
        return Err(KanyuError::Other(
            "该文件是 kdb v2 多图层容器（zip），不是 v1 单批次；\
             请改用 kdb_to_layers（CLI `kanyu data info` 自动展开图层清单）"
                .to_string(),
        ));
    }
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

// ===== KDB v2：多图层容器（zip）=====

/// 命名图层（KDB v2 容器成员）。
#[derive(Debug, Clone)]
pub struct KdbLayer {
    /// 图层名（如 ZDJBXX/JZD/JZX；唯一、禁含 `/` `\` `..`）。
    pub name: String,
    /// 图层数据（GeoArrow RecordBatch）。
    pub batch: RecordBatch,
}

/// 是否 KDB v2 容器（zip 魔数 `PK\x03\x04` 嗅探）。
pub fn is_kdb_v2(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// 命名校验（zip 路径安全：非空、唯一由调用方查重、禁含路径分隔与父引用）。
fn validate_layer_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(KanyuError::Other("kdb v2 图层名不能为空".to_string()));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(KanyuError::Other(format!(
            "kdb v2 图层名「{name}」非法：禁含 '/'、'\\\\'、'..'（zip 路径安全）"
        )));
    }
    Ok(())
}

/// 命名图层序列 → KDB v2 字节流（zip 容器：manifest.json + layers/<名>.kdb
/// 各为一个 v1 Arrow IPC 文件，逐层独立携带 `kanyu.*` 元数据）。
pub fn layers_to_kdb(layers: &[KdbLayer]) -> Result<Vec<u8>> {
    if layers.is_empty() {
        return Err(KanyuError::Other(
            "kdb v2 至少需要一个图层（空图层序列）".to_string(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for layer in layers {
        validate_layer_name(&layer.name)?;
        if !seen.insert(layer.name.as_str()) {
            return Err(KanyuError::Other(format!(
                "kdb v2 图层名重复：「{}」",
                layer.name
            )));
        }
    }

    let manifest = serde_json::json!({
        KDB_FORMAT_KEY: KDB_FORMAT_VALUE,
        KDB_VERSION_KEY: KDB_V2_VERSION_VALUE,
        KDB_PRODUCER_KEY: format!("kanyu-core {}", env!("CARGO_PKG_VERSION")),
        "layers": layers
            .iter()
            .map(|l| {
                serde_json::json!({
                    "name": l.name,
                    "path": format!("{KDB_V2_LAYER_DIR}{}.kdb", l.name),
                    "rows": l.batch.num_rows(),
                })
            })
            .collect::<Vec<_>>(),
    });

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file(KDB_V2_MANIFEST, options)
            .map_err(|e| KanyuError::Other(format!("kdb v2 写出失败: {e}")))?;
        std::io::Write::write_all(
            &mut writer,
            serde_json::to_string_pretty(&manifest)
                .map_err(|e| KanyuError::Other(format!("kdb v2 清单序列化失败: {e}")))?
                .as_bytes(),
        )
        .map_err(|e| KanyuError::Other(format!("kdb v2 写出失败: {e}")))?;
        for layer in layers {
            let entry = format!("{KDB_V2_LAYER_DIR}{}.kdb", layer.name);
            writer
                .start_file(entry, options)
                .map_err(|e| KanyuError::Other(format!("kdb v2 写出失败: {e}")))?;
            let bytes = batch_to_kdb(&layer.batch)?;
            std::io::Write::write_all(&mut writer, &bytes)
                .map_err(|e| KanyuError::Other(format!("kdb v2 写出失败: {e}")))?;
        }
        writer
            .finish()
            .map_err(|e| KanyuError::Other(format!("kdb v2 收尾失败: {e}")))?;
    }
    Ok(cursor.into_inner())
}

/// KDB 字节流 → 命名图层序列（v1 单批次 → 单图层（名 `"layer"`）；
/// v2 → 按 manifest 清单逐层展开并独立校验）。
pub fn kdb_to_layers(bytes: &[u8]) -> Result<Vec<KdbLayer>> {
    if !is_kdb_v2(bytes) {
        return Ok(vec![KdbLayer {
            name: "layer".to_string(),
            batch: kdb_to_batch(bytes)?,
        }]);
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| KanyuError::Other(format!("kdb v2 解包失败（zip 损坏）: {e}")))?;
    let manifest_text = {
        let mut entry = archive.by_name(KDB_V2_MANIFEST).map_err(|_| {
            KanyuError::Other(format!(
                "kdb v2 容器缺少 {KDB_V2_MANIFEST}（不是合法的堪舆多图层数据库）"
            ))
        })?;
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text)
            .map_err(|e| KanyuError::Other(format!("kdb v2 清单读取失败: {e}")))?;
        text
    };
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text)
        .map_err(|e| KanyuError::Other(format!("kdb v2 清单不是合法 JSON: {e}")))?;
    match manifest.get(KDB_FORMAT_KEY).and_then(|v| v.as_str()) {
        Some(KDB_FORMAT_VALUE) => {}
        other => {
            return Err(KanyuError::Other(format!(
                "kdb v2 清单 {KDB_FORMAT_KEY} 标识异常（期望 {KDB_FORMAT_VALUE}，实际 {other:?}）"
            )))
        }
    }
    let entries = manifest
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| KanyuError::Other("kdb v2 清单缺少 layers 数组".to_string()))?
        .clone();
    if entries.is_empty() {
        return Err(KanyuError::Other("kdb v2 清单 layers 为空".to_string()));
    }
    let mut layers = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KanyuError::Other("kdb v2 清单图层缺 name".to_string()))?;
        let path = entry
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| KanyuError::Other(format!("kdb v2 图层「{name}」缺 path")))?;
        validate_layer_name(name)?;
        let mut file = archive.by_name(path).map_err(|_| {
            KanyuError::Other(format!("kdb v2 图层「{name}」的数据条目缺失（{path}）"))
        })?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf)
            .map_err(|e| KanyuError::Other(format!("kdb v2 图层「{name}」读取失败: {e}")))?;
        let batch = kdb_to_batch(&buf)
            .map_err(|e| KanyuError::Other(format!("kdb v2 图层「{name}」校验失败: {e}")))?;
        layers.push(KdbLayer {
            name: name.to_string(),
            batch,
        });
    }
    Ok(layers)
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

    /// 第二样例图层（异构 schema：不同字段集）。
    fn sample_layer_b() -> Layer {
        let gj: geojson::GeoJson = r#"{
            "type": "FeatureCollection",
            "features": [
                {"type":"Feature","geometry":{"type":"Point","coordinates":[39595462.5,4127300.4]},
                 "properties":{"JZDH":"J1","SXH":1,"XZBZ":4127300.4,"YZBZ":39595462.5}}
            ]
        }"#
        .parse()
        .unwrap();
        Layer::from_collection("jzd", geojson::FeatureCollection::try_from(gj).unwrap())
    }

    #[test]
    fn kdb_v2_roundtrip_multiple_layers() {
        let a = sample_layer();
        let b = sample_layer_b();
        let layers = vec![
            KdbLayer {
                name: "ZDJBXX".to_string(),
                batch: a.batch().clone(),
            },
            KdbLayer {
                name: "JZD".to_string(),
                batch: b.batch().clone(),
            },
        ];
        let bytes = layers_to_kdb(&layers).unwrap();
        assert!(is_kdb_v2(&bytes), "应为 zip 魔数");

        let back = kdb_to_layers(&bytes).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].name, "ZDJBXX");
        assert_eq!(back[1].name, "JZD");
        assert_eq!(back[0].batch.num_rows(), 2);
        assert_eq!(back[1].batch.num_rows(), 1);
        // 类型保真：逐层与原始 schema 一致。
        for (orig, rt) in [&a, &b].iter().zip(back.iter()) {
            let orig_schema = orig.batch().schema();
            let rt_schema = rt.batch.schema();
            assert_eq!(orig_schema.fields().len(), rt_schema.fields().len());
        }
        // 几何列扩展元数据保留。
        let schema_b = back[1].batch.schema();
        let geom = schema_b.field_with_name("geometry").unwrap();
        assert_eq!(
            geom.metadata()
                .get("ARROW:extension:name")
                .map(String::as_str),
            Some("geoarrow.wkb")
        );
    }

    #[test]
    fn kdb_v2_rejects_bad_names_and_empty() {
        let a = sample_layer();
        // 空序列。
        assert!(layers_to_kdb(&[]).is_err());
        // 重名。
        let dup = vec![
            KdbLayer {
                name: "ZD".to_string(),
                batch: a.batch().clone(),
            },
            KdbLayer {
                name: "ZD".to_string(),
                batch: a.batch().clone(),
            },
        ];
        assert!(layers_to_kdb(&dup)
            .unwrap_err()
            .to_string()
            .contains("重复"));
        // 路径分隔符。
        let bad = vec![KdbLayer {
            name: "a/b".to_string(),
            batch: a.batch().clone(),
        }];
        assert!(layers_to_kdb(&bad).is_err());
        // 空名。
        let empty = vec![KdbLayer {
            name: String::new(),
            batch: a.batch().clone(),
        }];
        assert!(layers_to_kdb(&empty).is_err());
    }

    #[test]
    fn kdb_v1_bytes_read_as_single_layer() {
        let layer = sample_layer();
        let bytes = batch_to_kdb(layer.batch()).unwrap();
        assert!(!is_kdb_v2(&bytes));
        let layers = kdb_to_layers(&bytes).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].name, "layer");
        assert_eq!(layers[0].batch.num_rows(), 2);
    }

    #[test]
    fn kdb_to_batch_rejects_v2_with_clear_error() {
        let a = sample_layer();
        let bytes = layers_to_kdb(&[KdbLayer {
            name: "ZD".to_string(),
            batch: a.batch().clone(),
        }])
        .unwrap();
        let err = kdb_to_batch(&bytes).unwrap_err();
        assert!(
            err.to_string().contains("v2") && err.to_string().contains("kdb_to_layers"),
            "应指路 kdb_to_layers: {err}"
        );
    }
}
