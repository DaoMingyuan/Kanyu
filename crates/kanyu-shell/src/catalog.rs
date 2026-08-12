//! 目录面板（Catalog）：ArcGIS Pro 工程目录范式——固定五分类根节点：
//! **地图框**（当前打开的地图视图）/ **布局框**（占位）/ **数据库**（.kdb）/
//! **服务链接**（WFS GetFeature 连接，见 services.rs）/ **本机数据**（QGIS 式文件浏览器子树，懒加载）。
//! 与图层面板职责分离：目录管"找数据与资源"，图层管"数据现场"。

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{badge, hint_caption, text, tree_row, BadgeLevel};

/// 目录面板动作（app 分派）。
#[derive(Debug, Clone)]
pub enum CatalogAction {
    /// 打开数据文件为图层（.kyu 由 app 改走工程恢复；.kdb 直接加载）。
    LoadFile(PathBuf),
    /// 激活地图框（frames 下标；已关闭则重开、浮动则吸附）。
    ActivateFrame(usize),
    /// 新建二维地图框。
    NewFrame2D,
    /// 新建三维场景。
    NewFrame3D,
    /// 重命名地图框（打开对话框）。
    RenameFrame(usize),
    /// 删除地图框（主框/最后一框由 app 阻止并中文提示）。
    DeleteFrame(usize),
    /// 激活布局页签（layouts 下标；已关闭则重开）。
    ActivateLayout(usize),
    /// 删除布局。
    DeleteLayout(usize),
    /// 新建布局框（打开规格对话框）。
    NewLayout,
    /// 新建服务链接（打开对话框）。
    NewService,
    /// 删除服务链接（services 下标）。
    DeleteService(usize),
    /// 连接服务并加载为图层（services 下标）。
    ConnectService(usize),
}

/// 地图框行（app 注入，地图框分类的数据源）。
pub struct FrameRow {
    /// frames 下标（0 = 主框「地图」）。
    pub index: usize,
    /// 标题（可重命名）。
    pub title: String,
    /// 维度角标（二维/三维）。
    pub dim_label: &'static str,
    /// 打开状态（false = 已关闭：弱色行，双击重开）。
    pub open: bool,
}

/// 布局行（app 注入，布局框分类的数据源）。
pub struct LayoutRow {
    /// 标题。
    pub title: String,
    /// 打开状态（false = 已关闭：弱色行）。
    pub open: bool,
}

/// 分类元信息（计数徽标/空态提示）。
pub struct CategoryMeta {
    /// 名称。
    pub name: &'static str,
    /// 图标。
    pub icon: Icon,
    /// 计数徽标。
    pub count: usize,
    /// 空态提示（None = 有内容分类）。
    pub placeholder: Option<&'static str>,
}

/// 五分类（固定序；纯结构函数，可测）。
pub fn categories(
    view_count: usize,
    layout_count: usize,
    has_default_kdb: bool,
    service_count: usize,
    root_count: usize,
) -> [CategoryMeta; 5] {
    [
        CategoryMeta {
            name: "地图框",
            icon: Icon::Image,
            count: view_count,
            placeholder: None,
        },
        CategoryMeta {
            name: "布局框",
            icon: Icon::List,
            count: layout_count,
            placeholder: None,
        },
        CategoryMeta {
            name: "数据库",
            icon: Icon::Database,
            count: usize::from(has_default_kdb),
            placeholder: None,
        },
        CategoryMeta {
            name: "服务链接",
            icon: Icon::Link,
            count: service_count,
            placeholder: Some("暂无服务链接——＋新建 WFS GetFeature 连接"),
        },
        CategoryMeta {
            name: "本机数据",
            icon: Icon::FolderPlain,
            count: root_count,
            placeholder: None,
        },
    ]
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

/// 目录面板状态（五分类树 + 本机数据懒加载子树）。
pub struct CatalogPanel {
    roots: Vec<CatalogNode>,
    /// 状态行（读取失败等）。
    note: Option<String>,
    /// 分类展开态（默认仅「本机数据」展开）。
    expanded: [bool; 5],
}

impl Default for CatalogPanel {
    fn default() -> Self {
        let mut panel = Self {
            roots: Vec::new(),
            note: None,
            expanded: [false, false, false, false, true],
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

    /// 演示/验证（截图）：展开首个根节点及其首个子目录（深层展开滚动验证）+
    /// 全部分类展开（分类布局验证）。
    pub fn demo_expand_first(&mut self) {
        self.expanded = [true; 5];
        fn expand(node: &mut CatalogNode) {
            if node.children.is_none() {
                node.children = CatalogPanel::read_children(&node.path).ok();
            }
            node.expanded = true;
        }
        if let Some(root) = self.roots.first_mut() {
            expand(root);
            if let Some(children) = &mut root.children {
                if let Some(child) = children.iter_mut().find(|c| c.is_dir) {
                    expand(child);
                }
            }
        }
    }

    /// 演示/验证（截图）：仅展开「服务链接」分类。
    pub fn demo_expand_services(&mut self) {
        self.expanded = [false, false, false, true, false];
    }

    /// 演示/验证（截图）：仅展开「地图框」分类。
    pub fn demo_expand_frames(&mut self) {
        self.expanded = [true, false, false, false, false];
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

    /// 默认工程数据库路径（项目目录/kanyu.kdb）。
    fn default_kdb() -> Option<PathBuf> {
        let p = std::env::current_dir().ok()?.join("kanyu.kdb");
        p.is_file().then_some(p)
    }

    /// 面板 UI。`frames` = 地图框分类数据（全部已建框，含已关闭）、
    /// `layouts` = 布局行清单、`services` = 服务链接清单（app 注入）。返回产生的动作。
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut crate::ui_kit::icons::IconCache,
        frames: &[FrameRow],
        layouts: &[LayoutRow],
        services: &[crate::services::WfsConnection],
    ) -> Vec<CatalogAction> {
        let mut actions = Vec::new();
        // 状态行。
        if let Some(note) = &self.note {
            hint_caption(ui, note);
        }
        let default_kdb = Self::default_kdb();
        let cats = categories(
            frames.len(),
            layouts.len(),
            default_kdb.is_some(),
            services.len(),
            self.roots.len(),
        );
        // 展开/加载/打开动作延后收集（避免借用冲突），迭代后统一应用。
        let mut toggles: Vec<Vec<usize>> = Vec::new();
        // auto_shrink([false, true])：滚动条出现/消失不引发布局宽度跳动。
        egui::ScrollArea::vertical()
            .id_salt("catalog_tree")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (ci, cat) in cats.iter().enumerate() {
                    // 分类行：图标 + 名称 + 计数徽标 + 展开箭头。
                    let (_r, toggled) = tree_row(
                        ui,
                        cache,
                        0,
                        Some(cat.icon),
                        cat.name,
                        Some(self.expanded[ci]),
                        |ui| {
                            badge(ui, &cat.count.to_string(), BadgeLevel::Stable);
                        },
                    );
                    if toggled {
                        self.expanded[ci] = !self.expanded[ci];
                    }
                    if !self.expanded[ci] {
                        continue;
                    }
                    // 分类内容。
                    match ci {
                        0 => {
                            // 地图框：全部已建框（含已关闭——关闭≠删除，关闭行弱色）；
                            // 单击激活（已关闭则重开），右键重命名/删除；新建二维/三维分开。
                            for v in frames {
                                let label = if v.open {
                                    format!("{}（{}）", v.title, v.dim_label)
                                } else {
                                    format!("{}（{}·已关闭）", v.title, v.dim_label)
                                };
                                let (resp, _) = if v.open {
                                    tree_row(
                                        ui,
                                        cache,
                                        1,
                                        Some(Icon::Image),
                                        &label,
                                        None,
                                        |_ui| {},
                                    )
                                } else {
                                    crate::ui_kit::tree_row_weak(
                                        ui,
                                        cache,
                                        1,
                                        Some(Icon::Image),
                                        &label,
                                        None,
                                        |_ui| {},
                                    )
                                };
                                if resp.clicked() {
                                    actions.push(CatalogAction::ActivateFrame(v.index));
                                }
                                let resp = resp.on_hover_text(if v.open {
                                    "单击激活；右键重命名/删除"
                                } else {
                                    "已关闭——单击重新打开；右键重命名/删除"
                                });
                                resp.context_menu(|ui| {
                                    if ui.button("重命名…").clicked() {
                                        actions.push(CatalogAction::RenameFrame(v.index));
                                        ui.close();
                                    }
                                    if ui.button("删除").clicked() {
                                        actions.push(CatalogAction::DeleteFrame(v.index));
                                        ui.close();
                                    }
                                });
                            }
                            let (resp, _) = tree_row(
                                ui,
                                cache,
                                1,
                                Some(Icon::Play),
                                "＋ 新建二维地图框",
                                None,
                                |_ui| {},
                            );
                            if resp.clicked() {
                                actions.push(CatalogAction::NewFrame2D);
                            }
                            let (resp, _) = tree_row(
                                ui,
                                cache,
                                1,
                                Some(Icon::Play),
                                "＋ 新建三维场景",
                                None,
                                |_ui| {},
                            );
                            if resp.clicked() {
                                actions.push(CatalogAction::NewFrame3D);
                            }
                        }
                        1 => {
                            // 布局框：全部布局行（含已关闭，弱色；单击激活/重开；
                            // 右键删除）+ 新建入口。
                            for (i, l) in layouts.iter().enumerate() {
                                let label = if l.open {
                                    l.title.clone()
                                } else {
                                    format!("{}（已关闭）", l.title)
                                };
                                let (resp, _) = if l.open {
                                    tree_row(ui, cache, 1, Some(Icon::List), &label, None, |_ui| {})
                                } else {
                                    crate::ui_kit::tree_row_weak(
                                        ui,
                                        cache,
                                        1,
                                        Some(Icon::List),
                                        &label,
                                        None,
                                        |_ui| {},
                                    )
                                };
                                if resp.clicked() {
                                    actions.push(CatalogAction::ActivateLayout(i));
                                }
                                resp.context_menu(|ui| {
                                    if ui.button("删除").clicked() {
                                        actions.push(CatalogAction::DeleteLayout(i));
                                        ui.close();
                                    }
                                });
                            }
                            let (resp, _) = tree_row(
                                ui,
                                cache,
                                1,
                                Some(Icon::Play),
                                "＋ 新建布局框",
                                None,
                                |_ui| {},
                            );
                            if resp.clicked() {
                                actions.push(CatalogAction::NewLayout);
                            }
                        }
                        3 => {
                            // 服务链接：连接清单（双击连接加载）+ 新建入口。
                            if services.is_empty() {
                                if let Some(ph) = cat.placeholder {
                                    ui.horizontal(|ui| {
                                        ui.add_space(16.0);
                                        hint_caption(ui, ph);
                                    });
                                }
                            }
                            for (i, conn) in services.iter().enumerate() {
                                let (resp, _) = tree_row(
                                    ui,
                                    cache,
                                    1,
                                    Some(Icon::Link),
                                    &conn.name,
                                    None,
                                    |_ui| {},
                                );
                                if resp.double_clicked() {
                                    actions.push(CatalogAction::ConnectService(i));
                                }
                                let resp = resp
                                    .on_hover_text(format!("{}\n双击连接并加载为图层", conn.url));
                                resp.context_menu(|ui| {
                                    if ui.button("连接").clicked() {
                                        actions.push(CatalogAction::ConnectService(i));
                                        ui.close();
                                    }
                                    if ui.button("删除").clicked() {
                                        actions.push(CatalogAction::DeleteService(i));
                                        ui.close();
                                    }
                                });
                            }
                            let (resp, _) = tree_row(
                                ui,
                                cache,
                                1,
                                Some(Icon::Play),
                                "＋ 新建服务链接",
                                None,
                                |_ui| {},
                            );
                            if resp.clicked() {
                                actions.push(CatalogAction::NewService);
                            }
                        }
                        2 => {
                            // 数据库：默认工程数据库入口。
                            match &default_kdb {
                                Some(p) => {
                                    let (resp, _) = tree_row(
                                        ui,
                                        cache,
                                        1,
                                        Some(Icon::Database),
                                        "默认工程数据库（kanyu.kdb）",
                                        None,
                                        |_ui| {},
                                    );
                                    if resp.double_clicked() {
                                        actions.push(CatalogAction::LoadFile(p.clone()));
                                    }
                                    resp.on_hover_text("双击打开为图层");
                                }
                                None => {
                                    ui.horizontal(|ui| {
                                        ui.add_space(16.0);
                                        hint_caption(
                                            ui,
                                            "无默认工程数据库——可经工具箱「导出图层」创建 .kdb",
                                        );
                                    });
                                }
                            }
                        }
                        _ => {
                            // 本机数据：QGIS 式文件浏览器子树。
                            let mut path_buf = Vec::new();
                            for (i, _) in self.roots.iter().enumerate() {
                                path_buf.clear();
                                path_buf.push(i);
                                render_node(
                                    ui,
                                    &self.roots[i],
                                    1,
                                    &mut path_buf,
                                    &mut toggles,
                                    &mut actions,
                                    cache,
                                );
                            }
                        }
                    }
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
    fn categories_fixed_order_and_counts() {
        let cats = categories(3, 2, true, 1, 6);
        let names: Vec<&str> = cats.iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec!["地图框", "布局框", "数据库", "服务链接", "本机数据"]
        );
        assert_eq!(cats[0].count, 3);
        assert_eq!(cats[1].count, 2); // 布局框计数兑现
        assert_eq!(cats[2].count, 1);
        assert_eq!(cats[3].count, 1); // 服务链接计数兑现
        assert!(cats[3].placeholder.is_some());
        assert_eq!(cats[4].count, 6);
        let cats2 = categories(1, 0, false, 0, 0);
        assert_eq!(cats2[2].count, 0);
        assert_eq!(cats2[3].count, 0);
    }

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
