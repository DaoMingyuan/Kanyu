//! 统一格式注册表与能力矩阵。
//!
//! 对应总规《附录：格式支持矩阵》。每种格式声明其对五种操作的支持级别：
//! 读取、写入、编辑、符号化保留、布局保留。AI 与 CLI 通过注册表决策，
//! 而不是把格式知识散落在各处。

use serde::Serialize;

use crate::error::{KanyuError, Result};

/// 单项能力支持级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Support {
    /// 完整支持。
    Full,
    /// 部分支持（如高版本 DWG 需 ODA 转换）。
    Partial,
    /// 不支持。
    None,
}

impl Support {
    /// 是否可用（Full 或 Partial）。
    pub fn usable(self) -> bool {
        !matches!(self, Support::None)
    }
}

/// 一种格式的能力画像。
#[derive(Debug, Clone, Serialize)]
pub struct FormatCapabilities {
    /// 格式短名，如 `shp`、`dwg`。
    pub id: &'static str,
    /// 显示名，如 `ESRI Shapefile`。
    pub name: &'static str,
    /// 关联扩展名（小写、不含点）。
    pub extensions: &'static [&'static str],
    /// 读取。
    pub read: Support,
    /// 写入。
    pub write: Support,
    /// 编辑。
    pub edit: Support,
    /// 符号化保留。
    pub symbol: Support,
    /// 布局保留。
    pub layout: Support,
    /// 实现驱动（如 `native`、`gdal-bridge`、`libredwg`）。
    pub driver: &'static str,
    /// 备注。
    pub note: &'static str,
}

/// 格式注册表。内置总规矩阵中的所有格式。
pub struct FormatRegistry {
    formats: Vec<FormatCapabilities>,
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl FormatRegistry {
    /// 内置格式矩阵（与 docs/MASTERPLAN.md 附录 A 保持一致）。
    pub fn builtin() -> Self {
        use Support::*;
        let formats = vec![
            FormatCapabilities {
                id: "shp",
                name: "ESRI Shapefile",
                extensions: &["shp"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Partial,
                layout: None,
                driver: "native",
                note: "原生读写（shapefile crate）：读取 Point/MultiPoint/Polyline/Polygon 含洞 + dbase 类型化；写出单一几何类型三件套 + 字段名 10 字节截断",
            },
            FormatCapabilities {
                id: "gpkg",
                name: "GeoPackage",
                extensions: &["gpkg"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: Partial,
                driver: "gdal-bridge",
                note: "样式存入 gpkg_contents 扩展表",
            },
            FormatCapabilities {
                id: "geojson",
                name: "GeoJSON",
                extensions: &["geojson", "json"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Partial,
                layout: None,
                driver: "native",
                note: "RFC 7946；样式存入 style 属性",
            },
            FormatCapabilities {
                id: "fgb",
                name: "FlatGeobuf",
                extensions: &["fgb"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: None,
                driver: "native",
                note: "原生读写（flatgeobuf crate）；内部首选交换格式，列 schema 自动推断，混合几何按 Unknown 异构声明",
            },
            FormatCapabilities {
                id: "kdb",
                name: "堪舆数据库 (KanyuDB)",
                extensions: &["kdb"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: None,
                driver: "native",
                note: "自研存档：Arrow IPC + geoarrow.wkb + kanyu.* 元数据；与内存模型同构，类型保真零拷贝，任何 Arrow 工具链可读",
            },
            FormatCapabilities {
                id: "geoparquet",
                name: "GeoParquet",
                extensions: &["parquet"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: None,
                driver: "native",
                note: "原生读写（geoparquet crate）；云原生列式，WKB 几何编码 + geo 元数据，列 schema 自动推断",
            },
            FormatCapabilities {
                id: "dwg",
                name: "AutoCAD DWG",
                extensions: &["dwg"],
                read: Full,
                write: Partial,
                edit: Partial,
                symbol: Full,
                layout: Partial,
                driver: "acadrust",
                note: "acadrust + 自持补丁层（AC15 locator workaround + GBK/MIF 编码层）；七类几何 + TEXT/MTEXT 标注要素化 + ELLIPSE 近似（实体级 INSERT/HATCH/SPLINE 跳过+计数 📋）；R2018+ 待样本复测",
            },
            FormatCapabilities {
                id: "dxf",
                name: "AutoCAD DXF",
                extensions: &["dxf"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: Full,
                driver: "native",
                note: "原生读写（dxf crate）：POINT/LINE/LWPOLYLINE/POLYLINE/CIRCLE/ARC 映射，图层→layer 属性；写出 R2000 仅外环，属性/XDATA 📋",
            },
            FormatCapabilities {
                id: "dgn",
                name: "MicroStation DGN",
                extensions: &["dgn"],
                read: Full,
                write: Partial,
                edit: None,
                symbol: None,
                layout: None,
                driver: "gdal-bridge",
                note: "v7/v8 读取",
            },
            FormatCapabilities {
                id: "tab",
                name: "MapInfo TAB/MIF",
                extensions: &["tab", "mif"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Partial,
                layout: None,
                driver: "gdal-bridge",
                note: "",
            },
            FormatCapabilities {
                id: "postgis",
                name: "PostGIS",
                extensions: &[],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: None,
                driver: "gdal-bridge",
                note: "样式存元数据表",
            },
            FormatCapabilities {
                id: "spatialite",
                name: "SpatiaLite",
                extensions: &["sqlite"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: Full,
                layout: None,
                driver: "gdal-bridge",
                note: "",
            },
            FormatCapabilities {
                id: "wfs",
                name: "OGC WFS",
                extensions: &[],
                read: Full,
                write: None,
                edit: None,
                symbol: None,
                layout: None,
                driver: "gdal-bridge",
                note: "v1.1/v2.0 只读",
            },
            FormatCapabilities {
                id: "kml",
                name: "KML/KMZ",
                extensions: &["kml", "kmz"],
                read: Full,
                write: Full,
                edit: Partial,
                symbol: Partial,
                layout: None,
                driver: "native",
                note: "原生读写（kml crate）：Placemark 展平、ExtendedData 属性、含洞 Polygon；KMZ（zip 容器）✅ 原生读写",
            },
            FormatCapabilities {
                id: "gml",
                name: "GML",
                extensions: &["gml"],
                read: Full,
                write: Partial,
                edit: None,
                symbol: None,
                layout: None,
                driver: "gdal-bridge",
                note: "CityGML 子集",
            },
            FormatCapabilities {
                id: "csv",
                name: "CSV/Excel",
                extensions: &["csv", "xlsx", "tsv"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: None,
                layout: None,
                driver: "native",
                note: "自动坐标列识别 (lon/lat/x/y/经度/纬度)；xlsx 经 calamine 只读（写出 📋）",
            },
            FormatCapabilities {
                id: "txt",
                name: "宗地 TXT（界址点坐标）",
                extensions: &["txt"],
                read: Full,
                write: Full,
                edit: Full,
                symbol: None,
                layout: None,
                driver: "native",
                note: "移植自堪舆工具箱 txt_feature.py：宗地/点表双格式互认（X北Y东测绘惯例）；质检 kanyu data validate",
            },
            FormatCapabilities {
                id: "pdf",
                name: "PDF",
                extensions: &["pdf"],
                read: Partial,
                write: Full,
                edit: None,
                symbol: Full,
                layout: Full,
                driver: "native",
                note: "导出地图册/多页",
            },
            FormatCapabilities {
                id: "svg",
                name: "SVG",
                extensions: &["svg"],
                read: Partial,
                write: Full,
                edit: None,
                symbol: Full,
                layout: Full,
                driver: "native",
                note: "矢量图形，支持交互热区",
            },
        ];
        Self { formats }
    }

    /// 按格式短名查询。
    pub fn by_id(&self, id: &str) -> Option<&FormatCapabilities> {
        let id = id.to_ascii_lowercase();
        self.formats.iter().find(|f| f.id == id)
    }

    /// 按文件扩展名探测格式（如 `data.SHP` → `shp`）。
    pub fn detect(&self, path: &str) -> Option<&FormatCapabilities> {
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        self.formats
            .iter()
            .find(|f| f.extensions.contains(&ext.as_str()))
    }

    /// 全部格式。
    pub fn all(&self) -> &[FormatCapabilities] {
        &self.formats
    }

    /// 断言某格式支持某操作，否则返回结构化错误。
    pub fn require(&self, format_id: &str, operation: &str) -> Result<&FormatCapabilities> {
        let caps = self
            .by_id(format_id)
            .ok_or_else(|| KanyuError::UnknownFormat(format_id.to_string()))?;
        let ok = match operation {
            "read" => caps.read.usable(),
            "write" => caps.write.usable(),
            "edit" => caps.edit.usable(),
            _ => true,
        };
        if ok {
            Ok(caps)
        } else {
            Err(KanyuError::UnsupportedOperation {
                format: format_id.to_string(),
                operation: operation.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_by_extension_is_case_insensitive() {
        let reg = FormatRegistry::builtin();
        assert_eq!(reg.detect("a/b/c.SHP").unwrap().id, "shp");
        assert_eq!(reg.detect("roads.geojson").unwrap().id, "geojson");
        assert!(reg.detect("no_extension").is_none());
    }

    #[test]
    fn require_blocks_unsupported_write() {
        let reg = FormatRegistry::builtin();
        assert!(reg.require("wfs", "read").is_ok());
        let err = reg.require("wfs", "write").unwrap_err();
        assert!(matches!(err, KanyuError::UnsupportedOperation { .. }));
    }

    #[test]
    fn matrix_covers_masterplan_formats() {
        // 总规附录 A.1 列出的矢量格式必须全部在册。
        let reg = FormatRegistry::builtin();
        for id in [
            "shp",
            "gpkg",
            "geojson",
            "fgb",
            "geoparquet",
            "dwg",
            "dxf",
            "dgn",
            "tab",
            "postgis",
            "spatialite",
            "wfs",
            "kml",
            "gml",
            "csv",
            "pdf",
            "svg",
        ] {
            assert!(reg.by_id(id).is_some(), "missing format: {id}");
        }
    }
}
