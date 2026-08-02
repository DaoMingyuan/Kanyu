//! 目录面板（Catalog）：ArcGIS Pro 目录窗格理念——浏览本机文件系统，
//! 双击打开数据文件为图层。与图层面板（Contents）**职责分离**：
//! 目录管"找数据"，图层管"已加载的数据现场"。

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{hint_caption, text};

/// 目录面板动作（app 分派）。
#[derive(Debug, Clone)]
pub enum CatalogAction {
    /// 打开数据文件为图层。
    LoadFile(PathBuf),
}

/// 目录面板状态。
pub struct CatalogPanel {
    /// 当前目录。
    current_dir: PathBuf,
    /// 目录条目缓存（进入/上级时重建）。
    entries: Vec<CatalogEntry>,
    /// 状态行（读取失败等）。
    note: Option<String>,
}

/// 一个目录条目。
struct CatalogEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    /// 文件大小（目录为 None）。
    size: Option<u64>,
}

/// 数据文件扩展名（与壳层打开能力对齐，另加工程/数据库）。
const DATA_EXTENSIONS: &[&str] = &[
    "shp", "geojson", "json", "fgb", "parquet", "dxf", "dwg", "kml", "kmz", "csv", "tsv", "xlsx",
    "kdb", "kyu",
];

impl Default for CatalogPanel {
    fn default() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut panel = Self {
            current_dir,
            entries: Vec::new(),
            note: None,
        };
        panel.refresh();
        panel
    }
}

impl CatalogPanel {
    /// 重建当前目录条目（目录优先、名称排序；只列数据文件）。
    fn refresh(&mut self) {
        self.entries.clear();
        self.note = None;
        let rd = match std::fs::read_dir(&self.current_dir) {
            Ok(rd) => rd,
            Err(e) => {
                self.note = Some(format!("无法读取 {}: {e}", self.current_dir.display()));
                return;
            }
        };
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = path.is_dir();
            if !is_dir {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if !DATA_EXTENSIONS.contains(&ext.as_str()) {
                    continue;
                }
            }
            let size = if is_dir {
                None
            } else {
                entry.metadata().ok().map(|m| m.len())
            };
            let item = CatalogEntry {
                name,
                path,
                is_dir,
                size,
            };
            if is_dir {
                dirs.push(item);
            } else {
                files.push(item);
            }
        }
        dirs.sort_by_key(|e| e.name.to_lowercase());
        files.sort_by_key(|e| e.name.to_lowercase());
        self.entries = dirs.into_iter().chain(files).collect();
    }

    /// 进入目录（若可读）。
    fn enter(&mut self, path: PathBuf) {
        self.current_dir = path;
        self.refresh();
    }

    /// 快捷位置（桌面/文档/下载/当前目录/磁盘根）。
    fn quick_locations() -> Vec<(&'static str, PathBuf)> {
        let mut locs = Vec::new();
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            let home = PathBuf::from(home);
            for (label, sub) in [
                ("桌面", "Desktop"),
                ("文档", "Documents"),
                ("下载", "Downloads"),
            ] {
                let p = home.join(sub);
                if p.is_dir() {
                    locs.push((label, p));
                }
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            locs.push(("项目目录", cwd));
        }
        if cfg!(windows) {
            for letter in ['C', 'D', 'E', 'F'] {
                let p = PathBuf::from(format!("{letter}:\\"));
                if p.is_dir() {
                    locs.push((
                        Box::leak(format!("{letter}:\\").into_boxed_str()) as &'static str,
                        p,
                    ));
                }
            }
        }
        locs
    }

    /// 面板 UI。返回产生的动作。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<CatalogAction> {
        let mut actions = Vec::new();

        // 快捷位置行（chip 式小按钮，当前位置高亮）。
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (label, path) in Self::quick_locations() {
                    let active = path == self.current_dir;
                    let chip = egui::Button::new(text::caption(label))
                        .fill(if active {
                            ui.visuals().faint_bg_color
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .corner_radius(egui::CornerRadius::same(9));
                    if ui.add(chip).clicked() {
                        self.enter(path);
                    }
                }
            });
        });
        ui.add_space(4.0);

        // 面包屑：上级按钮 + 当前路径。
        ui.horizontal(|ui| {
            let (rect, up) = ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::click());
            let color = if up.hovered() {
                crate::ui_kit::icons_color(ui)
            } else {
                ui.visuals().weak_text_color()
            };
            // 上移箭头（简易：三角朝上）。
            let c = rect.center();
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - 5.0, c.y + 3.0),
                    egui::pos2(c.x + 5.0, c.y + 3.0),
                    egui::pos2(c.x, c.y - 4.0),
                ],
                color,
                egui::Stroke::NONE,
            ));
            if up.clicked() {
                if let Some(parent) = self.current_dir.parent().map(Path::to_path_buf) {
                    self.enter(parent);
                }
            }
            up.on_hover_text("上级目录");
            let path_text = self.current_dir.display().to_string();
            ui.label(text::caption(path_text).color(ui.visuals().weak_text_color()))
                .on_hover_text(self.current_dir.display().to_string());
        });
        ui.separator();

        // 条目列表（双击进入/打开；目录变更延后到迭代后应用，避免借用冲突）。
        if let Some(note) = &self.note {
            hint_caption(ui, note);
        }
        if self.entries.is_empty() && self.note.is_none() {
            hint_caption(ui, "（空目录或无可识别数据文件）");
        }
        let mut enter_target: Option<PathBuf> = None;
        egui::ScrollArea::vertical()
            .id_salt("catalog_scroll")
            .show(ui, |ui| {
                for entry in &self.entries {
                    let (icon, tint) = entry_visual(entry);
                    let row = ui.horizontal(|ui| {
                        icons::icon_ui(ui, icon, 15.0, tint);
                        ui.add_space(2.0);
                        let label = match entry.size {
                            Some(size) if !entry.is_dir => {
                                format!("{}  （{}）", entry.name, human_size(size))
                            }
                            _ => entry.name.clone(),
                        };
                        let resp =
                            ui.add(egui::Label::new(text::body(label)).sense(egui::Sense::click()));
                        if entry.is_dir {
                            resp.on_hover_text("双击进入目录")
                        } else {
                            resp.on_hover_text("双击打开为图层")
                        }
                    });
                    let resp = row.response;
                    if resp.double_clicked() {
                        if entry.is_dir {
                            enter_target = Some(entry.path.clone());
                        } else {
                            actions.push(CatalogAction::LoadFile(entry.path.clone()));
                        }
                    }
                }
            });
        if let Some(path) = enter_target {
            self.enter(path);
        }
        actions
    }
}

/// 条目图标与着色（目录=文件夹/工程=信息/数据库=六边形/其余按几何意象）。
fn entry_visual(entry: &CatalogEntry) -> (Icon, egui::Color32) {
    if entry.is_dir {
        return (Icon::Folder, egui::Color32::from_rgb(0xD4, 0xA8, 0x43));
    }
    let ext = entry
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "kyu" => (Icon::Info, egui::Color32::from_rgb(0xC7, 0x5B, 0x3A)),
        "kdb" => (Icon::Gene, egui::Color32::from_rgb(0x2D, 0x6A, 0x5E)),
        "dwg" | "dxf" => (Icon::Ruler, egui::Color32::from_rgb(0x4A, 0x7C, 0x9B)),
        _ => (Icon::Layers, egui::Color32::from_rgb(0x2D, 0x6A, 0x5E)),
    }
}

/// 文件大小单位。
const KB: u64 = 1024;
/// MB。
const MB: u64 = KB * 1024;
/// GB。
const GB: u64 = MB * 1024;

/// 文件大小友好显示。
fn human_size(bytes: u64) -> String {
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_scales() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(3 * MB), "3.0 MB");
        assert_eq!(human_size(2 * GB), "2.0 GB");
    }

    #[test]
    fn refresh_lists_dirs_and_data_files_only() {
        let dir = std::env::temp_dir().join("kanyu_catalog_test");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.geojson"), "{}").unwrap();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        std::fs::write(dir.join("c.kdb"), b"k").unwrap();
        let mut panel = CatalogPanel {
            current_dir: dir.clone(),
            entries: Vec::new(),
            note: None,
        };
        panel.refresh();
        let names: Vec<&str> = panel.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sub"), "目录应列出");
        assert!(names.contains(&"a.geojson"));
        assert!(names.contains(&"c.kdb"));
        assert!(!names.contains(&"b.txt"), "非数据文件应过滤");
        // 目录排前。
        assert_eq!(panel.entries[0].name, "sub");
    }
}
