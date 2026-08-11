//! 服务链接（目录五分类之「服务链接」兑现 v1）：WFS GetFeature 接入。
//!
//! - [`WfsConnection`]：连接名 + 完整 GetFeature 请求地址（用户侧构造，
//!   含 `service=WFS&request=GetFeature&typeNames=…&outputFormat=application/json`）；
//!   serde 派生，随 ui-state.json 持久化（见 uistate.rs）。
//! - [`fetch_wfs`]：ureq GET（10s 全局超时、64MB 响应上限）→ GeoJSON 解析
//!   （不依赖 Content-Type，json 或文本响应皆试）；HTTP/解析错误中文结构化。
//!   调用为阻塞式，壳层以后台线程 + 进度模态驱动（复用工具运行模式，见 app.rs）。
//! - [`validate_connection`]/[`parse_geojson`] 为纯函数，配单元测试（不触网）。

use geojson::{FeatureCollection, GeoJson};
use serde::{Deserialize, Serialize};

/// 响应体上限（WFS 要素集规模防御；ureq read_to_string 默认 10MB，此处放宽并显式化）。
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

/// 请求全局超时（连接 + 读取全程）。
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// WFS 服务链接（目录「服务链接」行数据源；持久化入 ui-state.json）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WfsConnection {
    /// 连接名（目录行文本 / 结果图层的 file_name）。
    pub name: String,
    /// 完整 GetFeature 请求地址（用户侧构造）。
    pub url: String,
}

/// 新建服务链接对话框状态（app 持有；Ok/Cancel 后清空）。
#[derive(Debug, Default)]
pub struct ServiceDialogState {
    /// 名称输入。
    pub name: String,
    /// GetFeature URL 输入。
    pub url: String,
}

/// 校验连接参数：名称非空；地址非空且 http(s) 前缀。
pub fn validate_connection(name: &str, url: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("连接名称不能为空".to_string());
    }
    let u = url.trim();
    if u.is_empty() {
        return Err("GetFeature 地址不能为空".to_string());
    }
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err(format!("GetFeature 地址须以 http:// 或 https:// 开头：{u}"));
    }
    Ok(())
}

/// GeoJSON 文本 → 要素集合（FeatureCollection 直收；单 Feature 包一条；其余报错）。
pub fn parse_geojson(text: &str) -> Result<FeatureCollection, String> {
    let gj: GeoJson = text
        .parse()
        .map_err(|e| format!("响应不是合法 GeoJSON: {e}"))?;
    match gj {
        GeoJson::FeatureCollection(fc) => Ok(fc),
        GeoJson::Feature(f) => Ok(FeatureCollection {
            bbox: None,
            features: vec![f],
            foreign_members: None,
        }),
        GeoJson::Geometry(_) => {
            Err("响应是裸几何而非要素集合（请确认地址为 GetFeature 请求）".to_string())
        }
    }
}

/// 截断至前 limit 条（0 = 不截断）；返回是否发生了截断。
pub fn apply_limit(fc: &mut FeatureCollection, limit: usize) -> bool {
    if limit > 0 && fc.features.len() > limit {
        fc.features.truncate(limit);
        return true;
    }
    false
}

/// 拉取 WFS GetFeature（阻塞；调用方须后台线程执行，避免卡 UI）。
///
/// `limit` > 0 时结果截断至前 limit 条（超大结果集防御）。
pub fn fetch_wfs(conn: &WfsConnection, limit: usize) -> Result<FeatureCollection, String> {
    validate_connection(&conn.name, &conn.url)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let mut resp = agent.get(conn.url.trim()).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => {
            format!("WFS 服务器返回 HTTP {code}（请核对 GetFeature 地址与参数）")
        }
        other => format!("WFS 请求失败: {other}"),
    })?;
    let text = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_string()
        .map_err(|e| format!("读取 WFS 响应失败（或超过 64MB 上限）: {e}"))?;
    let mut fc = parse_geojson(&text)?;
    apply_limit(&mut fc, limit);
    Ok(fc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_connection_rules() {
        assert!(validate_connection("", "https://a/wfs").is_err());
        assert!(validate_connection("  ", "https://a/wfs").is_err());
        assert!(validate_connection("x", "").is_err());
        assert!(validate_connection("x", "ftp://a/wfs").is_err());
        assert!(validate_connection("x", "a.com/wfs?request=GetFeature").is_err());
        assert!(validate_connection("x", "http://a/wfs?request=GetFeature").is_ok());
        assert!(validate_connection("x", "https://a/wfs?request=GetFeature").is_ok());
    }

    #[test]
    fn parse_geojson_collection_and_single_feature() {
        // 示例数据做 fixture（不走网络）。
        let text = include_str!("../../../examples/buildings.geojson");
        let fc = parse_geojson(text).unwrap();
        assert!(!fc.features.is_empty());
        // 单 Feature 包一条。
        let single = r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[1.0,2.0]},"properties":{}}"#;
        let fc2 = parse_geojson(single).unwrap();
        assert_eq!(fc2.features.len(), 1);
    }

    #[test]
    fn parse_geojson_rejects_garbage_and_bare_geometry() {
        assert!(parse_geojson("{ 不是 json").is_err());
        let geom = r#"{"type":"Point","coordinates":[1.0,2.0]}"#;
        let e = parse_geojson(geom).unwrap_err();
        assert!(e.contains("GetFeature"), "应指引核对地址: {e}");
    }

    #[test]
    fn apply_limit_truncates() {
        let text = include_str!("../../../examples/buildings.geojson");
        let mut fc = parse_geojson(text).unwrap();
        let n = fc.features.len();
        assert!(!apply_limit(&mut fc, 0), "0 = 不截断");
        assert_eq!(fc.features.len(), n);
        assert!(!apply_limit(&mut fc, n + 10));
        assert!(apply_limit(&mut fc, 1));
        assert_eq!(fc.features.len(), 1);
    }
}
