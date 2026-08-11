//! 目录面板（Catalog）：QGIS 浏览器面板式设计——可展开树：
//! 快捷根节点（主目录/桌面/文档/下载/项目目录/磁盘）→ 子目录懒加载展开；
//! 数据文件按类型着色，双击打开为图层。与图层面板职责分离：
//! 目录管"找数据"，图层管"数据现场"。

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

/// 数据文件扩展名（与壳层打开能力对齐，另加工程/数据库）。
const DATA_EXTENSIONS: &[&str] = &[
    "shp", "geojson", "json", "fgb", "parquet", "dxf", "dwg", "kml", "kmz", "csv", "tsv", "xlsx",
    "kdb", "kyu",
];

/// 目录树节点（目录懒加载：children 为 None 表示未展开读取过）。
struct CatalogNode {
    name: String,
    path: PathBuf,
    is_dir: bool,
    expanded: bool,
    /// 子节点缓存（目录展开时读取一次；None = 未读取）。
    children: Option<Vec<CatalogNode>>,
    /// 文件大小（目录为 None）。
    size: Option<u64>,
}

impl CatalogNode {
    fn dir(path: PathBuf, name: String) -> Self {
        Self {
            name,
            path,
            is_dir: true,
            expanded: false,
            children: None,
            size: None,
        }
    }
    fn file(path: PathBuf, name: String, size: u64) -> Self {
        Self {
            name,
            path,
            is_dir: false,
            expanded: false,
            children: None,
            size: Some(size),
        }
    }
}

/// 目录面板状态（QGIS 浏览器：根节点集合 + 懒加载树）。
pub struct CatalogPanel {
    roots: Vec<CatalogNode>,
    /// 状态行（读取失败等）。
    note: Option<String>,
}

impl Default for CatalogPanel {
    fn default() -> Self {
        let mut panel = Self {
            roots: Vec::new(),
            note: None,
        };
        panel.build_roots();
        panel
    }
}

impl CatalogPanel {
    /// 构造根节点（QGIS 浏览器的固定根：主目录/桌面/文档/下载/项目目录/磁盘）。
    fn build_roots(&mut self) {
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            let home = PathBuf::from(home);
            for (label, sub) in [
                ("主目录", ""),
                ("桌面", "Desktop"),
                ("文档", "Documents"),
                ("下载", "Downloads"),
            ] {
                let p = if sub.is_empty() {
                    home.clone()
                } else {
                    home.join(sub)
                };
                if p.is_dir() {
                    self.roots.push(CatalogNode::dir(p, label.to_string()));
                }
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            self.roots
                .push(CatalogNode::dir(cwd, "项目目录".to_string()));
        }
        if cfg!(windows) {
            for letter in ['C', 'D', 'E', 'F'] {
                let p = PathBuf::from(format!("{letter}:\\"));
                if p.is_dir() {
                    self.roots
                        .push(CatalogNode::dir(p.clone(), format!("磁盘 {letter}:\\")));
                }
            }
        }
    }

    /// 读取目录子节点（目录优先、名称排序；文件仅数据扩展名）。
    fn read_children(dir: &Path) -> Result<Vec<CatalogNode>, String> {
        let rd = std::fs::read_dir(dir).map_err(|e| format!("无法读取 {}: {e}", dir.display()))?;
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in rd.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                dirs.push(CatalogNode::dir(path, name));
            } else {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if DATA_EXTENSIONS.contains(&ext.as_str()) {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    files.push(CatalogNode::file(path, name, size));
                }
            }
        }
        dirs.sort_by_key(|n| n.name.to_lowercase());
        files.sort_by_key(|n| n.name.to_lowercase());
        Ok(dirs.into_iter().chain(files).collect())
    }

    /// 按索引路径取可变节点（索引路径 = 各层 children 下标序列）。
    fn node_at_mut<'a>(
        nodes: &'a mut [CatalogNode],
        idx_path: &[usize],
    ) -> Option<&'a mut CatalogNode> {
        let (first, rest) = idx_path.split_first()?;
        let node = nodes.get_mut(*first)?;
        if rest.is_empty() {
            Some(node)
        } else {
            Self::node_at_mut(node.children.as_mut()?.as_mut_slice(), rest)
        }
    }

    /// 面板 UI。返回产生的动作。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut crate::ui_kit::icons::IconCache,
    ) -> Vec<CatalogAction> {
        let mut actions = Vec::new();
        // 状态行。
        if let Some(note) = &self.note {
            hint_caption(ui, note);
        }
        // 展开/加载/打开动作延后收集（避免借用冲突），迭代后统一应用。
        let mut toggles: Vec<Vec<usize>> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("catalog_tree")
            .show(ui, |ui| {
                let mut path_buf = Vec::new();
                for (i, _) in self.roots.iter().enumerate() {
                    path_buf.clear();
                    path_buf.push(i);
                    render_node(
                        ui,
                        &self.roots[i],
                        0,
                        &mut path_buf,
                        &mut toggles,
                        &mut actions,
                        cache,
                    );
                }
            });
        // 应用展开/折叠（首次展开时懒加载子节点）。
        for idx_path in toggles {
            if let Some(node) = Self::node_at_mut(&mut self.roots, &idx_path) {
                if !node.expanded && node.children.is_none() {
                    match Self::read_children(&node.path) {
                        Ok(children) => node.children = Some(children),
                        Err(e) => self.note = Some(e),
                    }
                }
                node.expanded = !node.expanded;
            }
        }
        actions
    }
}

/// 递归渲染节点（QGIS 浏览器树行）。
fn render_node(
    ui: &mut egui::Ui,
    node: &CatalogNode,
    depth: usize,
    idx_path: &mut Vec<usize>,
    toggles: &mut Vec<Vec<usize>>,
    actions: &mut Vec<CatalogAction>,
    cache: &mut crate::ui_kit::icons::IconCache,
) {
    let my_path = idx_path.clone();
    ui.horizontal(|ui| {
        // 缩进（每级 14px 参考线）。
        for _ in 0..depth {
            let (rect, _) =
                ui.allocate_exact_size(egui::Vec2::new(14.0, 20.0), egui::Sense::hover());
            ui.painter().line_segment(
                [
                    egui::pos2(rect.center().x, rect.min.y),
                    egui::pos2(rect.center().x, rect.max.y),
                ],
                egui::Stroke::new(0.5, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );
        }
        // 展开箭头（仅目录；文件占位）。
        if node.is_dir {
            let (rect, resp) =
                ui.allocate_exact_size(egui::Vec2::new(16.0, 20.0), egui::Sense::click());
            let c = rect.center();
            let pts = if node.expanded {
                vec![
                    egui::pos2(c.x - 4.0, c.y - 2.0),
                    egui::pos2(c.x + 4.0, c.y - 2.0),
                    egui::pos2(c.x, c.y + 4.0),
                ]
            } else {
                vec![
                    egui::pos2(c.x - 2.0, c.y - 4.0),
                    egui::pos2(c.x + 4.0, c.y),
                    egui::pos2(c.x - 2.0, c.y + 4.0),
                ]
            };
            ui.painter().add(egui::Shape::convex_polygon(
                pts,
                ui.visuals().weak_text_color(),
                egui::Stroke::NONE,
            ));
            if resp.clicked() {
                toggles.push(my_path.clone());
            }
        } else {
            ui.allocate_exact_size(egui::Vec2::new(16.0, 20.0), egui::Sense::hover());
        }
        // 图标 + 名称（位图优先、手绘回退；tint 为手绘回退色）。
        let (icon, tint) = node_visual(node);
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
        icons::draw_or_image(ui, cache, icon, rect, tint);
        ui.add_space(2.0);
        let label = match node.size {
            Some(size) => format!("{}  （{}）", node.name, human_size(size)),
            None => node.name.clone(),
        };
        let resp = ui.add(egui::Label::new(text::body(label)).sense(egui::Sense::click()));
        if node.is_dir {
            if resp.double_clicked() {
                toggles.push(my_path.clone());
            }
            resp.on_hover_text(node.path.display().to_string());
        } else {
            if resp.double_clicked() {
                actions.push(CatalogAction::LoadFile(node.path.clone()));
            }
            resp.on_hover_text("双击打开为图层");
        }
    });
    // 子节点。
    if node.is_dir && node.expanded {
        if let Some(children) = &node.children {
            if children.is_empty() {
                ui.horizontal(|ui| {
                    for _ in 0..=depth {
                        ui.allocate_exact_size(egui::Vec2::new(14.0, 16.0), egui::Sense::hover());
                    }
                    hint_caption(ui, "（空）");
                });
            }
            for (i, child) in children.iter().enumerate() {
                idx_path.push(i);
                render_node(ui, child, depth + 1, idx_path, toggles, actions, cache);
                idx_path.pop();
            }
        }
    }
}

/// 节点图标与着色。
fn node_visual(node: &CatalogNode) -> (Icon, egui::Color32) {
    if node.is_dir {
        return (Icon::FolderPlain, egui::Color32::from_rgb(0xD4, 0xA8, 0x43));
    }
    let ext = node
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "kyu" => (Icon::Project, egui::Color32::from_rgb(0xC7, 0x5B, 0x3A)),
        "kdb" => (Icon::Database, egui::Color32::from_rgb(0x2D, 0x6A, 0x5E)),
        "dwg" | "dxf" => (Icon::Cad, egui::Color32::from_rgb(0x4A, 0x7C, 0x9B)),
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
    fn read_children_filters_and_sorts() {
        let dir = std::env::temp_dir().join("kanyu_catalog_tree_test");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.geojson"), "{}").unwrap();
        std::fs::write(dir.join("b.txt"), "x").unwrap();
        std::fs::write(dir.join("c.kdb"), b"k").unwrap();
        let children = CatalogPanel::read_children(&dir).unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"a.geojson"));
        assert!(names.contains(&"c.kdb"));
        assert!(!names.contains(&"b.txt"), "非数据文件应过滤");
        assert_eq!(children[0].name, "sub", "目录排前");
    }

    #[test]
    fn node_at_mut_walks_index_path() {
        let mut roots = vec![CatalogNode::dir(PathBuf::from("/a"), "a".to_string())];
        roots[0].children = Some(vec![CatalogNode::dir(
            PathBuf::from("/a/b"),
            "b".to_string(),
        )]);
        let node = CatalogPanel::node_at_mut(&mut roots, &[0, 0]).unwrap();
        assert_eq!(node.name, "b");
        node.expanded = true;
        assert!(roots[0].children.as_ref().unwrap()[0].expanded);
    }
}
