//! UI 状态持久化（ui-state.json）：停靠布局/工具箱收藏与最近/设置/视图清单。
//!
//! - 路径：`%LOCALAPPDATA%\kanyu\ui-state.json`（其他平台取 HOME 下 .config/kanyu）；
//!   找不到配置目录退回 exe 旁，再退当前目录；写不进仅告警（不 panic）。
//! - 健壮回退：文件损坏或版本不符 → 全部默认 + 备份坏文件为 ui-state.bad.json；
//!   缺字段逐项 `#[serde(default)]` 默认；不识别字段忽略。
//! - 目录面板展开状态**不存**（取舍：文件系统内容随环境变化大，展开态恢复
//!   价值低且易过期，见 ARCHITECTURE §9.1 持久化路线备注）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 状态文件版本（前向不兼容时递增；加载方版本不符即回退默认）。
pub const UI_STATE_VERSION: u32 = 1;

/// 面板停靠状态（id 为英文键）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelStateJson {
    pub id: String,
    /// 停靠区：left/right/bottom/floating。
    pub zone: String,
    pub open: bool,
}

/// 地图视图状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewStateJson {
    pub title: String,
    /// 维度：2d/3d。
    pub dim: String,
    /// 视口（可空）。
    pub bbox: Option<[f64; 4]>,
    /// 吸附中央页签 / 浮动窗。
    pub docked: bool,
}

/// 持久化 UI 状态（全字段可缺省；Default 即 v1 基线：缩放 1.0）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub version: u32,
    pub panels: Vec<PanelStateJson>,
    /// 每停靠区当前页签（left/right/bottom 序；面板英文键）。
    pub active_tabs: Vec<Option<String>>,
    pub toolbox_favorites: Vec<String>,
    pub toolbox_recent: Vec<String>,
    pub ui_zoom: f32,
    pub map_theme: String,
    pub project_crs: String,
    pub views: Vec<ViewStateJson>,
    pub active_view: Option<usize>,
    /// 服务链接清单（WFS GetFeature 连接；v1 起持久化）。
    pub services: Vec<crate::services::WfsConnection>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: UI_STATE_VERSION,
            panels: Vec::new(),
            active_tabs: Vec::new(),
            toolbox_favorites: Vec::new(),
            toolbox_recent: Vec::new(),
            ui_zoom: 1.0,
            map_theme: String::new(),
            project_crs: String::new(),
            views: Vec::new(),
            active_view: None,
            services: Vec::new(),
        }
    }
}

impl UiState {
    /// 新建（版本号打头）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 序列化为 JSON 文本。
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// 从 JSON 解析（宽松：缺字段默认、不识字段忽略；结构损坏报 Err）。
    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut s: UiState =
            serde_json::from_str(text).map_err(|e| format!("ui-state 解析失败: {e}"))?;
        if s.version != UI_STATE_VERSION {
            return Err(format!(
                "ui-state 版本 {} 高于/不符当前 {}",
                s.version, UI_STATE_VERSION
            ));
        }
        if s.ui_zoom <= 0.2 || s.ui_zoom > 3.0 || !s.ui_zoom.is_finite() {
            s.ui_zoom = 1.0; // 异常缩放档回退
        }
        Ok(s)
    }

    /// 从文件加载：不存在 → 默认；损坏/版本不符 → 备份为 .bad.json 后默认。
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::new(); // 不存在：全新默认
        };
        match Self::from_json(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}（已回退默认布局）");
                let backup = path.with_file_name("ui-state.bad.json");
                let _ = std::fs::rename(path, &backup);
                Self::new()
            }
        }
    }

    /// 写盘（失败仅告警，不 panic）。
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("ui-state 目录创建失败: {e}");
                return;
            }
        }
        if let Err(e) = std::fs::write(path, self.to_json()) {
            eprintln!("ui-state 写入失败: {e}");
        }
    }
}

/// 状态文件路径：%LOCALAPPDATA%\kanyu\ui-state.json（Win）；
/// 其他平台 $HOME/.config/kanyu/；再退 exe 旁，再退当前目录。
pub fn state_path() -> PathBuf {
    let file = "ui-state.json";
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(base).join("kanyu").join(file);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("kanyu").join(file);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.join(file);
        }
    }
    PathBuf::from(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UiState {
        let mut s = UiState::new();
        s.panels = vec![PanelStateJson {
            id: "toolbox".into(),
            zone: "right".into(),
            open: true,
        }];
        s.active_tabs = vec![None, Some("toolbox".into()), None];
        s.toolbox_favorites = vec!["buffer".into()];
        s.toolbox_recent = vec!["buffer".into(), "centroid".into()];
        s.ui_zoom = 1.25;
        s.map_theme = "fixed_dark".into();
        s.project_crs = "EPSG:4490".into();
        s.views = vec![ViewStateJson {
            title: "地图 2".into(),
            dim: "3d".into(),
            bbox: Some([0.0, 0.0, 1.0, 1.0]),
            docked: false,
        }];
        s.active_view = Some(0);
        s.services = vec![crate::services::WfsConnection {
            name: "示例 WFS".into(),
            url: "https://example.com/wfs?request=GetFeature".into(),
        }];
        s
    }

    #[test]
    fn roundtrip() {
        let s = sample();
        let back = UiState::from_json(&s.to_json()).unwrap();
        assert_eq!(back.panels[0].id, "toolbox");
        assert_eq!(back.active_tabs[1].as_deref(), Some("toolbox"));
        assert_eq!(back.ui_zoom, 1.25);
        assert_eq!(back.views[0].dim, "3d");
        assert_eq!(back.views[0].bbox, Some([0.0, 0.0, 1.0, 1.0]));
        assert_eq!(back.active_view, Some(0));
        assert_eq!(back.services.len(), 1);
        assert_eq!(back.services[0].name, "示例 WFS");
    }

    #[test]
    fn missing_fields_default() {
        // 只有 version 的“老文件”：逐项默认。
        let s = UiState::from_json(r#"{"version":1}"#).unwrap();
        assert!(s.panels.is_empty());
        assert!(s.services.is_empty()); // 老文件无 services 字段：默认空
        assert_eq!(s.ui_zoom, 1.0); // Default 基线 1.0
        let s2 = UiState::from_json(r#"{"version":1,"ui_zoom":1.5}"#).unwrap();
        assert_eq!(s2.ui_zoom, 1.5);
    }

    #[test]
    fn corrupt_and_version_mismatch_fallback() {
        assert!(UiState::from_json("{ 不是 json").is_err());
        assert!(UiState::from_json(r#"{"version":99}"#).is_err());
        // 异常缩放档钳回 1.0。
        let s = UiState::from_json(r#"{"version":1,"ui_zoom":50.0}"#).unwrap();
        assert_eq!(s.ui_zoom, 1.0);
    }

    #[test]
    fn load_save_file_and_bad_backup() {
        let dir = std::env::temp_dir().join("kanyu_uistate_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ui-state.json");
        // 不存在 → 默认。
        let _ = std::fs::remove_file(&path);
        assert!(UiState::load(&path).panels.is_empty());
        // 写读往返。
        sample().save(&path);
        let back = UiState::load(&path);
        assert_eq!(back.toolbox_recent, vec!["buffer", "centroid"]);
        // 坏文件 → 默认 + .bad.json 备份。
        std::fs::write(&path, "{ 损坏").unwrap();
        let s = UiState::load(&path);
        assert!(s.panels.is_empty());
        assert!(dir.join("ui-state.bad.json").exists());
    }
}
