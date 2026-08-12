//! 服务链接（目录「服务链接」分类）：WFS GetFeature（v1）+ GetCapabilities
//! 图层发现 + WMS GetMap 底图（v2）。
//!
//! - [`WfsConnection`]：连接名 + 完整 GetFeature 请求地址（v1 语义不变——
//!   对话框现由「基址 + 图层发现」生成该 URL）；serde 派生，随 ui-state.json 持久化。
//! - [`WmsConnection`]：连接名 + 服务基址 + 图层名（GetMap 按视口现构造 URL）；
//!   **独立 Vec 持久化**（uistate 加 `wms` 字段，serde default——v1 老状态文件
//!   零迁移读取，取舍：不动既有 `services` 字段结构，比 serde tag 泛化更稳）。
//! - [`fetch_capabilities`]：GetCapabilities → **手写最小 XML 提取**（取舍：
//!   不引 quick-xml——`<FeatureType>` 块内 `<Name>/<Title>` 文本抽取 +
//!   实体反转义 + 命名空间前缀剥离，足够主流 WFS 1.1/2.0；完整解析无收益）。
//! - 网络调用均阻塞式（ureq，10s 超时、64MB 上限），壳层后台线程驱动（不卡 UI）。
//! - URL 构造/XML 提取为纯函数（[`build_getfeature_url`]/[`build_getmap_url`]/
//!   [`parse_capabilities`]），配单测（不触网）。

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

/// 服务连接编辑目标（对话框 editing 字段；None = 新建）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceEditTarget {
    /// 类型（WFS/WMS 清单）。
    pub kind: ServiceKind,
    /// 清单下标。
    pub index: usize,
}

/// 新建服务链接对话框状态（app 持有；Ok/Cancel 后清空）。
#[derive(Debug, Default)]
pub struct ServiceDialogState {
    /// 服务类型（WFS 要素 / WMS 底图）。
    pub kind: ServiceKind,
    /// 名称输入。
    pub name: String,
    /// 服务基址输入（?service=… 之前的部分；v1 的完整 URL 由确定时构造）。
    pub url: String,
    /// 图层名（WFS = typeNames；WMS = layers）。
    pub layer: String,
    /// 已获取的图层清单（GetCapabilities 结果）。
    pub caps: Vec<WfsLayerInfo>,
    /// 清单状态行（拉取中/错误提示）。
    pub caps_note: Option<String>,
    /// 清单后台拉取通道（Some = 拉取中）。
    pub caps_rx: Option<std::sync::mpsc::Receiver<Result<Vec<WfsLayerInfo>, String>>>,
    /// 编辑目标（Some = 编辑既有连接；None = 新建）。
    pub editing: Option<ServiceEditTarget>,
}

/// 服务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceKind {
    /// WFS GetFeature（矢量要素）。
    #[default]
    Wfs,
    /// WMS GetMap（影像底图）。
    Wms,
}

impl ServiceKind {
    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            ServiceKind::Wfs => "WFS 要素（GetFeature）",
            ServiceKind::Wms => "WMS 影像底图（GetMap）",
        }
    }
}

/// WMS 服务链接（GetMap 底图；持久化入 ui-state.json 的独立 `wms` 字段）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WmsConnection {
    /// 连接名（目录行文本）。
    pub name: String,
    /// 服务基址（不含查询串）。
    pub url: String,
    /// 图层名（GetMap 的 layers 参数）。
    pub layer: String,
}

/// WFS 图层信息（GetCapabilities 发现）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WfsLayerInfo {
    /// 类型名（typeNames 参数）。
    pub name: String,
    /// 标题（可空）。
    pub title: Option<String>,
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
    let mut resp = http_agent()
        .get(conn.url.trim())
        .call()
        .map_err(|e| match e {
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

/// 共享 ureq Agent（10s 全局超时）。
fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .build()
        .into()
}

// ===== URL 构造（纯函数）=====

/// 基址拼查询串的起始符（已有 '?' 则 '&'，尾部分隔符清洗）。
fn join_query(base: &str) -> String {
    let b = base.trim().trim_end_matches(['?', '&']);
    if b.contains('?') {
        format!("{b}&")
    } else {
        format!("{b}?")
    }
}

/// 构造完整 GetFeature 请求地址（GeoJSON 输出优先）。
/// 图层名原样拼接（主流 WFS 的 typeNames 为 `工作区:图层` 形态，无需转义）。
pub fn build_getfeature_url(base: &str, type_name: &str) -> String {
    format!(
        "{}service=WFS&request=GetFeature&version=2.0.0&typeNames={}&outputFormat=application/json",
        join_query(base),
        type_name.trim()
    )
}

/// 拆解完整 GetFeature 地址（编辑回填用，best-effort）：(基址, typeNames)。
/// 无 '?' 时整体作基址、图层空串；无 typeNames 参数时图层空串。
pub fn split_getfeature_url(url: &str) -> (String, String) {
    let Some((base, query)) = url.split_once('?') else {
        return (url.trim().to_string(), String::new());
    };
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k.eq_ignore_ascii_case("typenames") || k.eq_ignore_ascii_case("typename") {
                return (base.to_string(), v.to_string());
            }
        }
    }
    (base.to_string(), String::new())
}

/// 构造 WMS GetMap 请求地址（1.3.0 + CRS=EPSG:4326；bbox = [minx,miny,maxx,maxy]
/// 经度/纬度序——宽限服务器（GeoServer 等）通用；严格 1.3.0 轴序服务器属已知边界）。
pub fn build_getmap_url(base: &str, layer: &str, bbox: [f64; 4], w: u32, h: u32) -> String {
    format!(
        "{}service=WMS&request=GetMap&version=1.3.0&layers={}&styles=&format=image/png\
         &transparent=false&crs=EPSG:4326&bbox={:.6},{:.6},{:.6},{:.6}&width={w}&height={h}",
        join_query(base),
        layer.trim(),
        bbox[0],
        bbox[1],
        bbox[2],
        bbox[3],
    )
}

/// 校验 WMS 连接参数（名称/基址/图层名；复用 http(s) 前缀规则）。
pub fn validate_wms(name: &str, url: &str, layer: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("连接名称不能为空".to_string());
    }
    let u = url.trim();
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err(format!("服务基址须以 http:// 或 https:// 开头：{u}"));
    }
    if layer.trim().is_empty() {
        return Err("WMS 图层名不能为空".to_string());
    }
    Ok(())
}

// ===== GetCapabilities（最小 XML 提取）=====

/// 拉取并解析 WFS GetCapabilities（阻塞；后台线程调用）。
pub fn fetch_capabilities(base: &str) -> Result<Vec<WfsLayerInfo>, String> {
    let url = format!("{}service=WFS&request=GetCapabilities", join_query(base));
    let mut resp = http_agent().get(&url).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => format!("服务返回 HTTP {code}（请核对服务基址）"),
        other => format!("GetCapabilities 请求失败: {other}"),
    })?;
    let text = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_string()
        .map_err(|e| format!("读取响应失败: {e}"))?;
    let layers = parse_capabilities(&text);
    if layers.is_empty() {
        return Err("未在响应中发现任何图层（FeatureType）——请核对基址为 WFS 服务".to_string());
    }
    Ok(layers)
}

/// 解析 capabilities XML → 图层清单（最小提取：`<FeatureType>` 块内
/// 首个 `<Name>`/`<Title>`；命名空间前缀剥离；实体反转义）。
pub fn parse_capabilities(xml: &str) -> Vec<WfsLayerInfo> {
    let mut out = Vec::new();
    for block in extract_blocks(xml, "FeatureType") {
        let names = extract_blocks(block, "Name");
        let Some(name) = names.first() else {
            continue;
        };
        let name = unescape_xml(name.trim());
        if name.is_empty() {
            continue;
        }
        let title = extract_blocks(block, "Title")
            .first()
            .map(|t| unescape_xml(t.trim()));
        out.push(WfsLayerInfo { name, title });
    }
    out
}

/// 按本地名抽取元素内容（忽略命名空间前缀；非通用解析器——
/// 目标文档结构扁平、无同名嵌套，见模块头取舍）。
fn extract_blocks<'a>(xml: &'a str, local: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // 标签名（< 之后到空白/> 之一）。
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && !matches!(bytes[end], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/')
        {
            end += 1;
        }
        let tag = &xml[start..end];
        let is_close = tag.starts_with('/');
        let local_name = tag.trim_start_matches('/').rsplit(':').next().unwrap_or("");
        if !is_close && local_name == local {
            if let Some(gt) = xml[end..].find('>') {
                let open_end = end + gt; // '>' 的下标
                                         // 自闭合（<tag …/>）无内容。
                if open_end > start && xml.as_bytes()[open_end - 1] == b'/' {
                    i = open_end + 1;
                    continue;
                }
                let content_start = open_end + 1;
                let close = format!("</{tag}>");
                if let Some(c) = xml[content_start..].find(&close) {
                    out.push(&xml[content_start..content_start + c]);
                    i = content_start + c + close.len();
                    continue;
                }
                i = content_start;
                continue;
            }
        }
        i = end.max(i + 1);
    }
    out
}

/// XML 实体反转义（最小集）。
fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

// ===== WMS GetMap =====

/// 拉取 WMS GetMap 影像（阻塞；后台线程调用）。返回 PNG 字节。
pub fn fetch_wms_map(url: &str) -> Result<Vec<u8>, String> {
    let mut resp = http_agent().get(url).call().map_err(|e| match e {
        ureq::Error::StatusCode(code) => format!("WMS 服务器返回 HTTP {code}"),
        other => format!("WMS 请求失败: {other}"),
    })?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY_BYTES)
        .read_to_vec()
        .map_err(|e| format!("读取 WMS 响应失败: {e}"))?;
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("WMS 响应不是 PNG 影像（可能为 ServiceException——请核对图层名）".to_string());
    }
    Ok(bytes)
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

    #[test]
    fn build_getfeature_url_contract() {
        let u = build_getfeature_url("https://svc/geoserver/wfs", "demo:blocks");
        assert_eq!(
            u,
            "https://svc/geoserver/wfs?service=WFS&request=GetFeature&version=2.0.0\
             &typeNames=demo:blocks&outputFormat=application/json"
        );
        // 基址带既有查询串/尾部 '?' 的拼接清洗。
        let u2 = build_getfeature_url("https://svc/wfs?token=abc", "ns:lyr");
        assert!(
            u2.starts_with("https://svc/wfs?token=abc&service=WFS"),
            "{u2}"
        );
        let u3 = build_getfeature_url("https://svc/wfs?", "ns:lyr");
        assert!(u3.starts_with("https://svc/wfs?service=WFS"), "{u3}");
    }

    #[test]
    fn split_getfeature_url_roundtrip_and_fallbacks() {
        // 构造→拆解往返。
        let u = build_getfeature_url("https://svc/geoserver/wfs", "demo:blocks");
        let (base, layer) = split_getfeature_url(&u);
        assert_eq!(base, "https://svc/geoserver/wfs");
        assert_eq!(layer, "demo:blocks");
        // 单数 typename 也识别（WFS 1.x 风格参数名）。
        let (_, l2) = split_getfeature_url("https://a/wfs?service=WFS&typeName=topp:states");
        assert_eq!(l2, "topp:states");
        // 无查询串/无 typeNames 的兜底。
        assert_eq!(split_getfeature_url("https://a/wfs").1, "");
        assert_eq!(
            split_getfeature_url("https://a/wfs?request=GetFeature").1,
            ""
        );
        assert_eq!(
            split_getfeature_url("https://a/wfs?request=GetFeature").0,
            "https://a/wfs"
        );
    }

    #[test]
    fn build_getmap_url_contract() {
        let u = build_getmap_url(
            "https://svc/geoserver/wms",
            "ne:countries",
            [116.0, 39.5, 117.0, 40.5],
            800,
            600,
        );
        assert!(
            u.contains("service=WMS&request=GetMap&version=1.3.0"),
            "{u}"
        );
        assert!(u.contains("layers=ne:countries"), "{u}");
        assert!(u.contains("crs=EPSG:4326"), "{u}");
        assert!(
            u.contains("bbox=116.000000,39.500000,117.000000,40.500000"),
            "{u}"
        );
        assert!(u.contains("width=800&height=600"), "{u}");
        assert!(u.contains("format=image/png"), "{u}");
    }

    #[test]
    fn validate_wms_rules() {
        assert!(validate_wms("", "https://a/wms", "x").is_err());
        assert!(validate_wms("x", "ftp://a/wms", "y").is_err());
        assert!(validate_wms("x", "https://a/wms", " ").is_err());
        assert!(validate_wms("x", "https://a/wms", "ne:countries").is_ok());
    }

    #[test]
    fn parse_capabilities_fixture() {
        // 内嵌样例（WFS 2.0 命名空间 + 1.1 无前缀 + 实体转义混合）。
        let xml = r#"<?xml version="1.0"?>
<wfs:WFS_Capabilities xmlns:wfs="http://www.opengis.net/wfs/2.0">
  <FeatureTypeList>
    <wfs:FeatureType>
      <wfs:Name>demo:blocks</wfs:Name>
      <wfs:Title>示范街区 &amp; 地块</wfs:Title>
      <wfs:Abstract>ignored</wfs:Abstract>
    </wfs:FeatureType>
    <wfs:FeatureType>
      <wfs:Name>demo:roads</wfs:Name>
    </wfs:FeatureType>
  </FeatureTypeList>
</wfs:WFS_Capabilities>"#;
        let caps = parse_capabilities(xml);
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].name, "demo:blocks");
        assert_eq!(caps[0].title.as_deref(), Some("示范街区 & 地块"));
        assert_eq!(caps[1].name, "demo:roads");
        assert_eq!(caps[1].title, None);
        // 无前缀（WFS 1.1 风格）。
        let xml11 = "<WFS_Capabilities><FeatureTypeList><FeatureType>\
                     <Name>topp:states</Name><Title>States</Title></FeatureType>\
                     </FeatureTypeList></WFS_Capabilities>";
        let caps2 = parse_capabilities(xml11);
        assert_eq!(caps2.len(), 1);
        assert_eq!(caps2[0].name, "topp:states");
        assert_eq!(caps2[0].title.as_deref(), Some("States"));
        // 空清单（非 capabilities 文档）。
        assert!(parse_capabilities("<html>not wfs</html>").is_empty());
    }
}
