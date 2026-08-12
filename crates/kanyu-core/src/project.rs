//! 堪舆工程（`.kyu`）—— 项目存档格式。
//!
//! ## 设计（KYU v1）
//!
//! `.kyu` 为 JSON 清单：项目元数据 + 图层引用（按路径引用外部数据源）+
//! 界面状态（视口、地图色彩、可见性）。不内嵌数据——数据源变动工程即跟随；
//! 无来源的内存图层（分析产出）不入工程，保存方应明确告知（调用方职责）。
//!
//! ```json
//! {
//!   "kanyu_project": 1,
//!   "name": "示例工程",
//!   "crs": "EPSG:4326",
//!   "created": "2026-08-03T12:00:00Z",
//!   "kanyu_version": "0.14.0",
//!   "viewport": [116.3, 39.8, 116.5, 40.0],
//!   "map_theme": "fixed_light",
//!   "layers": [
//!     {"id": "buildings", "source": "data/buildings.geojson", "visible": true, "style": null, "group": "基底"}
//!   ]
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::{KanyuError, Result};

/// 格式标识键。
pub const KYU_FORMAT_KEY: &str = "kanyu_project";
/// KYU v1 版本号。
pub const KYU_VERSION: u32 = 1;

/// 堪舆工程文件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KanyuProject {
    /// 格式标识（恒为 KYU_VERSION）。
    pub kanyu_project: u32,
    /// 项目名。
    pub name: String,
    /// 声明的坐标参考系。
    #[serde(default = "default_crs")]
    pub crs: String,
    /// 创建时间（ISO 8601 文本，由调用方提供）。
    #[serde(default)]
    pub created: String,
    /// 保存时的 kanyu 版本。
    #[serde(default)]
    pub kanyu_version: String,
    /// 视口 `[minx, miny, maxx, maxy]`（可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<[f64; 4]>,
    /// 地图色彩模式：fixed_light | fixed_dark | follow_ui。
    #[serde(default = "default_map_theme")]
    pub map_theme: String,
    /// 图层清单（按打开顺序）。
    #[serde(default)]
    pub layers: Vec<ProjectLayer>,
    /// 地图框清单（首项为默认「地图」框；缺省 = 单主框，全部图层归主框——
    /// 旧版工程文件无此字段，serde default 向后兼容）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<ProjectFrame>,
}

/// 工程中的地图框（标题即身份——[`ProjectLayer::map`] 以其引用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFrame {
    /// 地图框标题。
    pub title: String,
    /// 维度：2d | 3d（缺省 2d）。
    #[serde(default = "default_2d")]
    pub dim: String,
    /// 视口 `[minx, miny, maxx, maxy]`（可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<[f64; 4]>,
    /// 打开状态（关闭 ≠ 删除：目录清单保留，可重开）。
    #[serde(default = "default_true")]
    pub open: bool,
    /// 吸附中央页签（false = 浮动窗）。
    #[serde(default = "default_true")]
    pub docked: bool,
}

/// 工程中的一个图层引用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLayer {
    /// 图层 id。
    pub id: String,
    /// 数据源路径（相对工程文件目录或绝对路径）。
    pub source: String,
    /// 可见性。
    #[serde(default = "default_true")]
    pub visible: bool,
    /// 符号化样式 JSON（可空，原样透传）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<serde_json::Value>,
    /// 分组路径（目录树组，嵌套组以 "/" 连接，如 "基底/参考"；None/缺省 = 根级）。
    /// `#[serde(default)]` 保证旧版工程文件（无此字段）正常反序列化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 所属地图框标题（None/缺省 = 默认主框「地图」——旧工程全部图层归主框）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<String>,
}

fn default_crs() -> String {
    "EPSG:4326".to_string()
}
fn default_map_theme() -> String {
    "fixed_light".to_string()
}
fn default_true() -> bool {
    true
}
fn default_2d() -> String {
    "2d".to_string()
}

impl KanyuProject {
    /// 新建空工程。
    pub fn new(name: impl Into<String>, crs: impl Into<String>) -> Self {
        Self {
            kanyu_project: KYU_VERSION,
            name: name.into(),
            crs: crs.into(),
            created: String::new(),
            kanyu_version: env!("CARGO_PKG_VERSION").to_string(),
            viewport: None,
            map_theme: default_map_theme(),
            layers: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// 序列化为 JSON 文本。
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| KanyuError::Other(format!("kyu 序列化失败: {e}")))
    }

    /// 从 JSON 文本解析（校验格式标识与版本）。
    pub fn from_json(text: &str) -> Result<Self> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| KanyuError::Other(format!("不是合法的 kyu 工程文件: {e}")))?;
        match value.get(KYU_FORMAT_KEY).and_then(|v| v.as_u64()) {
            Some(v) if v as u32 == KYU_VERSION => {}
            Some(v) => {
                return Err(KanyuError::Other(format!(
                    "kyu 工程版本 {v} 高于当前支持的 {KYU_VERSION}（请升级堪舆）"
                )))
            }
            None => {
                return Err(KanyuError::Other(
                    "缺少 kanyu_project 标识——这不是堪舆工程（.kyu）文件".to_string(),
                ))
            }
        }
        serde_json::from_value(value)
            .map_err(|e| KanyuError::Other(format!("kyu 工程字段解析失败: {e}")))
    }

    /// 保存到文件。
    pub fn save(&self, path: &str) -> Result<()> {
        std::fs::write(path, self.to_json()?)
            .map_err(|e| KanyuError::Other(format!("写入工程文件 {path} 失败: {e}")))
    }

    /// 从文件加载。
    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| KanyuError::Other(format!("读取工程文件 {path} 失败: {e}")))?;
        Self::from_json(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_fields() {
        let mut p = KanyuProject::new("测试工程", "EPSG:4526");
        p.created = "2026-08-03T12:00:00Z".to_string();
        p.viewport = Some([116.3, 39.8, 116.5, 40.0]);
        p.map_theme = "follow_ui".to_string();
        p.layers.push(ProjectLayer {
            id: "buildings".to_string(),
            source: "data/buildings.geojson".to_string(),
            visible: true,
            style: None,
            group: None,
            map: None,
        });
        p.layers.push(ProjectLayer {
            id: "roads".to_string(),
            source: "data/roads.fgb".to_string(),
            visible: false,
            style: Some(serde_json::json!({"type":"graduated"})),
            group: Some("基底/道路".to_string()),
            map: None,
        });

        let text = p.to_json().unwrap();
        let back = KanyuProject::from_json(&text).unwrap();
        assert_eq!(back.name, "测试工程");
        assert_eq!(back.crs, "EPSG:4526");
        assert_eq!(back.kanyu_project, 1);
        assert_eq!(back.map_theme, "follow_ui");
        assert_eq!(back.layers.len(), 2);
        assert!(!back.layers[1].visible);
        assert!(back.layers[1].style.is_some());
        assert_eq!(back.viewport, Some([116.3, 39.8, 116.5, 40.0]));
        // 分组路径往返（含无分组混合）。
        assert_eq!(back.layers[0].group, None);
        assert_eq!(back.layers[1].group.as_deref(), Some("基底/道路"));
    }

    /// 向后兼容：旧版工程文件无 group 字段，须正常反序列化（group = None）。
    #[test]
    fn legacy_without_group_parses() {
        let text = r#"{
            "kanyu_project": 1,
            "name": "旧工程",
            "layers": [
                {"id": "a", "source": "a.geojson", "visible": true},
                {"id": "b", "source": "b.geojson"}
            ]
        }"#;
        let p = KanyuProject::from_json(text).unwrap();
        assert_eq!(p.layers.len(), 2);
        assert_eq!(p.layers[0].group, None);
        assert_eq!(p.layers[1].group, None);
        assert!(p.layers[1].visible); // visible 缺省 = true（既有约定）
    }

    /// 分组字段序列化：None 省略键、Some 写入；往返一致。
    #[test]
    fn group_field_roundtrip() {
        let mut p = KanyuProject::new("分组工程", "EPSG:4326");
        p.layers.push(ProjectLayer {
            id: "x".to_string(),
            source: "x.geojson".to_string(),
            visible: true,
            style: None,
            group: Some("一组/子组".to_string()),
            map: None,
        });
        p.layers.push(ProjectLayer {
            id: "y".to_string(),
            source: "y.geojson".to_string(),
            visible: false,
            style: None,
            group: None,
            map: None,
        });
        let text = p.to_json().unwrap();
        assert!(text.contains("\"group\": \"一组/子组\""));
        // None 不写键（保持文件干净，旧版本读取无干扰）。
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value["layers"][1].get("group").is_none());
        assert!(value["layers"][0].get("group").is_some());
        let back = KanyuProject::from_json(&text).unwrap();
        assert_eq!(back.layers[0].group.as_deref(), Some("一组/子组"));
        assert_eq!(back.layers[1].group, None);
    }

    /// 向后兼容：旧版工程文件无 frames/map 字段，须正常反序列化
    /// （frames = 空清单、图层 map = None 即全部归默认主框）。
    #[test]
    fn legacy_without_frames_and_map_parses() {
        let text = r#"{
            "kanyu_project": 1,
            "name": "旧工程",
            "layers": [{"id": "a", "source": "a.geojson", "visible": true}]
        }"#;
        let p = KanyuProject::from_json(text).unwrap();
        assert!(p.frames.is_empty());
        assert_eq!(p.layers[0].map, None);
    }

    /// 地图框清单 + 图层 map 归属往返；空 frames 不写键（保持文件干净）。
    #[test]
    fn frames_and_layer_map_roundtrip() {
        let mut p = KanyuProject::new("多框工程", "EPSG:4326");
        p.frames = vec![
            ProjectFrame {
                title: "地图".to_string(),
                dim: "2d".to_string(),
                viewport: Some([0.0, 0.0, 1.0, 1.0]),
                open: true,
                docked: true,
            },
            ProjectFrame {
                title: "场景 2".to_string(),
                dim: "3d".to_string(),
                viewport: None,
                open: false,
                docked: false,
            },
        ];
        p.layers.push(ProjectLayer {
            id: "blocks".to_string(),
            source: "b.geojson".to_string(),
            visible: true,
            style: None,
            group: None,
            map: Some("场景 2".to_string()),
        });
        let text = p.to_json().unwrap();
        let back = KanyuProject::from_json(&text).unwrap();
        assert_eq!(back.frames.len(), 2);
        assert_eq!(back.frames[1].dim, "3d");
        assert!(!back.frames[1].open);
        assert!(!back.frames[1].docked);
        assert_eq!(back.frames[0].viewport, Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(back.layers[0].map.as_deref(), Some("场景 2"));
        // 空 frames 省略键（旧版本读取无干扰）。
        let empty = KanyuProject::new("空", "EPSG:4326").to_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&empty).unwrap();
        assert!(value.get("frames").is_none());
    }

    #[test]
    fn rejects_non_kyu_and_newer_version() {
        let err = KanyuProject::from_json("{}").unwrap_err();
        assert!(err.to_string().contains("kanyu_project"));
        let err = KanyuProject::from_json(r#"{"kanyu_project": 99}"#).unwrap_err();
        assert!(err.to_string().contains("升级堪舆"));
        assert!(KanyuProject::from_json("not json").is_err());
    }

    #[test]
    fn file_save_load_roundtrip() {
        let dir = std::env::temp_dir().join("kanyu_kyu_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.kyu");
        let mut p = KanyuProject::new("文件工程", "EPSG:4326");
        p.layers.push(ProjectLayer {
            id: "a".to_string(),
            source: "a.geojson".to_string(),
            visible: true,
            style: None,
            group: None,
            map: None,
        });
        p.save(path.to_str().unwrap()).unwrap();
        let back = KanyuProject::load(path.to_str().unwrap()).unwrap();
        assert_eq!(back.name, "文件工程");
        assert_eq!(back.layers[0].source, "a.geojson");
    }
}
