//! 工具箱面板：顶部「最近使用」（内存态，最多 8 条）与「收藏」（右键/★ 切换）
//! 两区块 + 筛选框 + 分类可折叠树（工具行双击或右键「运行」开参数对话框）。

use eframe::egui;

use super::{find, ToolCategory, TOOLS};
use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::tokens::spacing;
use crate::ui_kit::{menu_button, text, tree_row};

/// 最近使用上限。
const RECENT_MAX: usize = 8;

/// 工具箱面板状态。
#[derive(Default)]
pub struct ToolboxPanel {
    /// 筛选框（按中文名/说明过滤）。
    filter: String,
    /// 分类折叠态（默认全展开）。
    collapsed: [bool; 5],
    /// 最近使用（新→旧，去重，上限 RECENT_MAX；内存态）。
    recent: Vec<&'static str>,
    /// 收藏（注册表序；内存态）。
    favorites: Vec<&'static str>,
    /// 状态变更计数（ui-state 防抖写盘的脏判定）。
    state_version: u64,
}

impl ToolboxPanel {
    /// 记录一次成功执行（去重置首，截断上限）。
    pub fn note_run(&mut self, id: &'static str) {
        self.recent.retain(|&r| r != id);
        self.recent.insert(0, id);
        self.recent.truncate(RECENT_MAX);
        self.state_version += 1;
    }

    /// 收藏开关。
    pub fn toggle_favorite(&mut self, id: &'static str) {
        if self.favorites.contains(&id) {
            self.favorites.retain(|&f| f != id);
        } else {
            self.favorites.push(id);
            self.favorites.sort();
        }
        self.state_version += 1;
    }

    /// 状态版本（收藏/最近变化即递增；app 据此刻意写盘）。
    pub fn state_version(&self) -> u64 {
        self.state_version
    }

    /// 收藏清单快照（ui-state 保存用，英文 id）。
    pub fn favorites_snapshot(&self) -> Vec<String> {
        self.favorites.iter().map(|s| s.to_string()).collect()
    }

    /// 最近使用清单快照（ui-state 保存用，英文 id）。
    pub fn recent_snapshot(&self) -> Vec<String> {
        self.recent.iter().map(|s| s.to_string()).collect()
    }

    /// 恢复收藏/最近（ui-state 加载；非法 id 过滤丢弃）。
    pub fn restore(&mut self, favorites: &[String], recent: &[String]) {
        self.favorites = favorites
            .iter()
            .filter_map(|s| super::find(s).map(|t| t.id))
            .collect();
        self.recent = recent
            .iter()
            .filter_map(|s| super::find(s).map(|t| t.id))
            .collect();
    }

    /// 是否已收藏。
    fn is_favorite(&self, id: &str) -> bool {
        self.favorites.contains(&id)
    }

    /// 单工具行（树行 + 悬停说明 + 双击运行 + 右键 运行/收藏切换）。
    fn tool_row(
        &mut self,
        ui: &mut egui::Ui,
        cache: &mut icons::IconCache,
        depth: usize,
        id: &'static str,
    ) -> Option<&'static str> {
        let t = find(id)?;
        let mut run = None;
        let fav = self.is_favorite(id);
        let label = if fav {
            format!("{} ★", t.name)
        } else {
            t.name.to_string()
        };
        let (resp, _) = tree_row(ui, cache, depth, Some(Icon::Play), &label, None, |_ui| {});
        let tip = if t.report {
            format!("{}\n结果输出终端；双击运行", t.desc)
        } else {
            format!("{}\n双击运行", t.desc)
        };
        let resp = resp.on_hover_text(tip);
        if resp.double_clicked() {
            run = Some(id);
        }
        resp.context_menu(|ui| {
            if ui.button("运行").clicked() {
                run = Some(id);
                ui.close();
            }
            let fav_label = if fav { "取消收藏" } else { "收藏" };
            if ui.button(fav_label).clicked() {
                self.toggle_favorite(id);
                ui.close();
            }
        });
        run
    }

    /// 面板 UI；返回待运行的工具 id。
    pub fn ui(&mut self, ui: &mut egui::Ui, cache: &mut icons::IconCache) -> Option<&'static str> {
        let mut run = None;
        // 顶行：选项菜单（全部展开/折叠）+ 筛选框（殿后填充余宽——
        // INFINITY 宽控件放中间会把后续控件顶出面板，切勿调换顺序）。
        ui.horizontal(|ui| {
            let menu = menu_button(ui, "选项", &["全部展开", "全部折叠"]);
            if let Some(i) = menu.selected {
                self.collapsed = [i == 1; 5];
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .desired_width(f32::INFINITY)
                    .hint_text("筛选工具…"),
            );
        });
        ui.separator();
        let filter = self.filter.trim().to_lowercase();
        // auto_shrink([false, true])：滚动条出现/消失不引发布局宽度跳动。
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // 「最近使用」区（有记录才显示）。
                if !self.recent.is_empty() {
                    ui.label(text::caption("最近使用").color(ui.visuals().weak_text_color()));
                    let recent = self.recent.clone();
                    for id in recent {
                        if let Some(r) = self.tool_row(ui, cache, 0, id) {
                            run = Some(r);
                        }
                    }
                    ui.add_space(spacing::SM);
                }
                // 「收藏」区（有收藏才显示）。
                if !self.favorites.is_empty() {
                    ui.label(text::caption("收藏").color(ui.visuals().weak_text_color()));
                    let favorites = self.favorites.clone();
                    for id in favorites {
                        if let Some(r) = self.tool_row(ui, cache, 0, id) {
                            run = Some(r);
                        }
                    }
                    ui.add_space(spacing::SM);
                }
                // 分类树。
                for cat in ToolCategory::ALL {
                    let tools: Vec<&'static str> = TOOLS
                        .iter()
                        .filter(|t| t.category == cat)
                        .filter(|t| {
                            filter.is_empty()
                                || t.name.to_lowercase().contains(&filter)
                                || t.desc.to_lowercase().contains(&filter)
                        })
                        .map(|t| t.id)
                        .collect();
                    if tools.is_empty() {
                        continue;
                    }
                    // 筛选时强制展开（命中直接可见）。
                    let expanded = if filter.is_empty() {
                        !self.collapsed[cat.index()]
                    } else {
                        true
                    };
                    let label = format!("{} ({} 项)", cat.label(), tools.len());
                    let (_r, toggled) = tree_row(
                        ui,
                        cache,
                        0,
                        Some(Icon::Folder),
                        &label,
                        Some(expanded),
                        |_ui| {},
                    );
                    if toggled {
                        self.collapsed[cat.index()] = !self.collapsed[cat.index()];
                    }
                    if !expanded {
                        continue;
                    }
                    for id in tools {
                        if let Some(r) = self.tool_row(ui, cache, 1, id) {
                            run = Some(r);
                        }
                    }
                }
            });
        run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_dedupes_and_caps() {
        let mut p = ToolboxPanel::default();
        for id in ["buffer", "centroid", "buffer"] {
            p.note_run(find(id).unwrap().id);
        }
        // 去重置首：buffer 只出现一次且在最前。
        assert_eq!(p.recent, vec!["buffer", "centroid"]);
        for i in 0..12 {
            p.note_run(TOOLS[i % TOOLS.len()].id);
        }
        assert!(p.recent.len() <= RECENT_MAX);
    }

    #[test]
    fn favorites_toggle() {
        let mut p = ToolboxPanel::default();
        p.toggle_favorite("buffer");
        assert!(p.is_favorite("buffer"));
        p.toggle_favorite("centroid");
        p.toggle_favorite("buffer"); // 再点取消
        assert!(!p.is_favorite("buffer"));
        assert!(p.is_favorite("centroid"));
        assert_eq!(p.favorites, vec!["centroid"]);
    }
}
