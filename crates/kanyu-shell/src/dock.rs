//! 停靠系统（ArcGIS Pro 停靠窗格范式）：面板注册表 + 停靠状态（[`DockState`]）+
//! 拖放落区判定（纯函数，可测）+ 停靠区页签条（可拖动 / 可关闭）。
//!
//! ## 架构约定
//!
//! - **注册表式面板清单**：[`PanelId`] 枚举 + [`PanelId::ALL`] / [`PanelId::title`] /
//!   [`PanelId::default_zone`]。新增面板（如第三阶段「工具箱」）只需：枚举加变体
//!   + 两处方法补分支 + app 渲染分支加一行——停靠/拖动/关闭/重开逻辑零改动。
//! - **状态集中**：[`DockState`] 持有每面板所在区（左/右/底/浮动）与开关、
//!   每停靠区当前页签、拖拽中的面板、各停靠区/浮动窗的上一帧矩形
//!   （运行时投放判定用，不持久化）。
//! - **落区判定为纯函数**（[`drop_target_at`] / [`resolve_drop`]），配单元测试。
//! - 内容区 = Ribbon（[`sizes::RIBBON`]）与状态栏（[`sizes::STATUS_BAR`]）之间
//!   的矩形，由调用方（app）按帧计算传入。

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::theme::{palette, Palette};
use crate::ui_kit::tokens::{radius, sizes, spacing, text};

/// 可停靠面板（注册表条目 = 变体 + [`PanelId::title`] + [`PanelId::default_zone`]）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PanelId {
    /// 目录（Catalog 文件浏览）。
    Catalog,
    /// 图层（Contents 目录树）。
    Layers,
    /// 工具箱（QGIS Processing 式算法清单）。
    Toolbox,
    /// 属性表（要素字段表格 + 字段计算器）。
    AttrTable,
    /// 独立终端。
    Console,
    /// AI 对话。
    AiChat,
}

impl PanelId {
    /// 全部面板（注册表顺序即页签排列顺序）。
    pub const ALL: [PanelId; 6] = [
        PanelId::Catalog,
        PanelId::Layers,
        PanelId::Toolbox,
        PanelId::AttrTable,
        PanelId::Console,
        PanelId::AiChat,
    ];

    /// 面板标题（页签/浮动窗标题栏）。
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Catalog => "目录",
            PanelId::Layers => "图层",
            PanelId::Toolbox => "工具箱",
            PanelId::AttrTable => "属性表",
            PanelId::Console => "终端",
            PanelId::AiChat => "AI 对话",
        }
    }

    /// 默认停靠区（首次启动/未配置时）。
    pub fn default_zone(self) -> DockZone {
        match self {
            PanelId::Catalog | PanelId::Layers => DockZone::Left,
            PanelId::Toolbox => DockZone::Right,
            PanelId::AttrTable | PanelId::Console | PanelId::AiChat => DockZone::Bottom,
        }
    }

    /// 数组下标（DockState 内定长表）。
    pub fn index(self) -> usize {
        match self {
            PanelId::Catalog => 0,
            PanelId::Layers => 1,
            PanelId::Toolbox => 2,
            PanelId::AttrTable => 3,
            PanelId::Console => 4,
            PanelId::AiChat => 5,
        }
    }
}

/// 停靠区（Floating = 浮动窗口，不占停靠边）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockZone {
    /// 左停靠区。
    Left,
    /// 右停靠区。
    Right,
    /// 底停靠区。
    Bottom,
    /// 浮动（egui::Window）。
    Floating,
}

impl DockZone {
    /// 三个停靠边（不含浮动）。
    pub const DOCKED: [DockZone; 3] = [DockZone::Left, DockZone::Right, DockZone::Bottom];

    /// 停靠边下标（仅 DOCKED 内有效）。
    pub fn docked_index(self) -> usize {
        match self {
            DockZone::Left => 0,
            DockZone::Right => 1,
            DockZone::Bottom => 2,
            DockZone::Floating => unreachable!("浮动区无停靠边下标"),
        }
    }
}

/// 单面板状态。
#[derive(Clone, Copy, Debug)]
struct PanelState {
    zone: DockZone,
    open: bool,
}

/// 停靠布局状态（集中于 app；浮动窗位置尺寸由 egui 记忆，不入此结构）。
pub struct DockState {
    panels: [PanelState; PanelId::ALL.len()],
    /// 每停靠边当前页签。
    active: [Option<PanelId>; DockZone::DOCKED.len()],
    /// 正在拖拽的面板（页签或浮动窗标题条发起）。
    pub dragging: Option<PanelId>,
    /// 各停靠边上一帧矩形（投放判定的"源区取消"规则用）。
    pub zone_rects: [Option<Rect>; DockZone::DOCKED.len()],
    /// 浮动窗上一帧矩形（标题条拖拽命中判定用）。
    pub float_rects: [Option<Rect>; PanelId::ALL.len()],
}

impl Default for DockState {
    fn default() -> Self {
        let mut s = Self {
            panels: [PanelState {
                zone: DockZone::Floating,
                open: false,
            }; PanelId::ALL.len()],
            active: [None; DockZone::DOCKED.len()],
            dragging: None,
            zone_rects: [None; DockZone::DOCKED.len()],
            float_rects: [None; PanelId::ALL.len()],
        };
        for id in PanelId::ALL {
            s.panels[id.index()] = PanelState {
                zone: id.default_zone(),
                open: true,
            };
        }
        // 工具箱默认关闭（按需经「视图 → 面板 → 工具箱」开启；保持首启布局简洁，
        // 右停靠区在其打开前整体隐藏）。属性表同理默认关闭（经图层右键打开）。
        s.panels[PanelId::Toolbox.index()].open = false;
        s.panels[PanelId::AttrTable.index()].open = false;
        // 默认页签：左区落「图层」（主工作区），底区落「终端」。
        s.active[DockZone::Left.docked_index()] = Some(PanelId::Layers);
        s.active[DockZone::Bottom.docked_index()] = Some(PanelId::Console);
        s
    }
}

impl DockState {
    /// 面板所在区。
    pub fn zone_of(&self, id: PanelId) -> DockZone {
        self.panels[id.index()].zone
    }

    /// 面板是否打开。
    pub fn is_open(&self, id: PanelId) -> bool {
        self.panels[id.index()].open
    }

    /// 面板是否以浮动窗呈现。
    pub fn is_floating(&self, id: PanelId) -> bool {
        self.panels[id.index()].open && self.panels[id.index()].zone == DockZone::Floating
    }

    /// 某停靠边内已打开的面板（注册表顺序）。
    pub fn panels_in(&self, zone: DockZone) -> Vec<PanelId> {
        PanelId::ALL
            .into_iter()
            .filter(|&id| self.panels[id.index()].open && self.panels[id.index()].zone == zone)
            .collect()
    }

    /// 停靠边是否有已打开面板（无则整区隐藏）。
    pub fn zone_has_panels(&self, zone: DockZone) -> bool {
        PanelId::ALL
            .iter()
            .any(|&id| self.panels[id.index()].open && self.panels[id.index()].zone == zone)
    }

    /// 停靠边当前页签：活跃页签仍开且在本区则保留，否则回落到本区首个打开面板。
    pub fn active_in(&self, zone: DockZone) -> Option<PanelId> {
        debug_assert!(zone != DockZone::Floating);
        let cur = self.active[zone.docked_index()];
        if let Some(id) = cur {
            if self.panels[id.index()].open && self.panels[id.index()].zone == zone {
                return Some(id);
            }
        }
        self.panels_in(zone).into_iter().next()
    }

    /// 切换停靠边当前页签。
    pub fn set_active(&mut self, zone: DockZone, id: PanelId) {
        if zone != DockZone::Floating {
            self.active[zone.docked_index()] = Some(id);
        }
    }

    /// 打开面板（在原所在区/默认区呈现，并置为该边当前页签）。
    pub fn open_panel(&mut self, id: PanelId) {
        self.panels[id.index()].open = true;
        let zone = self.panels[id.index()].zone;
        if zone != DockZone::Floating {
            self.set_active(zone, id);
        }
    }

    /// 关闭面板（若为当前页签，下次 active_in 自动回落）。
    pub fn close_panel(&mut self, id: PanelId) {
        self.panels[id.index()].open = false;
        if self.dragging == Some(id) {
            self.dragging = None;
        }
    }

    /// 改停靠到指定边（同时打开并置为当前页签）。
    pub fn dock_to(&mut self, id: PanelId, zone: DockZone) {
        debug_assert!(zone != DockZone::Floating, "dock_to 只接受停靠边");
        self.panels[id.index()].zone = zone;
        self.panels[id.index()].open = true;
        self.set_active(zone, id);
    }

    /// 置为浮动窗（同时打开）。
    pub fn float(&mut self, id: PanelId) {
        self.panels[id.index()].zone = DockZone::Floating;
        self.panels[id.index()].open = true;
    }

    /// 全部打开/全部关闭。
    pub fn set_all_open(&mut self, open: bool) {
        for p in &mut self.panels {
            p.open = open;
        }
        if !open {
            self.dragging = None;
        }
    }

    /// 浮动窗标题条命中（拖拽改停靠的发起判定）：标题条 = 窗矩形顶部 28px。
    pub fn hit_floating_title(&self, pos: Pos2) -> Option<PanelId> {
        /// 标题条命中高度（egui 默认标题栏约 24px，留余量）。
        const TITLE_GRAB: f32 = 28.0;
        PanelId::ALL.into_iter().find(|&id| {
            self.is_floating(id)
                && self.float_rects[id.index()].is_some_and(|r| {
                    Rect::from_min_max(r.min, Pos2::new(r.max.x, r.min.y + TITLE_GRAB))
                        .contains(pos)
                })
        })
    }
}

// ===== 投放区判定（纯函数）=====

/// 边缘投放区宽度（px）。
pub const DROP_EDGE: f32 = 160.0;

/// 内容区三个边缘投放条：左/右为全高条，底为左右之间的横条（角落归左右）。
pub fn drop_strips(area: Rect) -> [(DockZone, Rect); 3] {
    let edge = DROP_EDGE.min(area.width() / 4.0);
    let left = Rect::from_min_max(area.min, Pos2::new(area.min.x + edge, area.max.y));
    let right = Rect::from_min_max(Pos2::new(area.max.x - edge, area.min.y), area.max);
    let bottom = Rect::from_min_max(
        Pos2::new(area.min.x + edge, area.max.y - edge),
        Pos2::new(area.max.x - edge, area.max.y),
    );
    [
        (DockZone::Left, left),
        (DockZone::Right, right),
        (DockZone::Bottom, bottom),
    ]
}

/// 指针落点 → 目标停靠区；中央 = Floating（浮动窗）；区外 = None。
pub fn drop_target_at(pos: Pos2, area: Rect) -> Option<DockZone> {
    if !area.contains(pos) {
        return None;
    }
    for (zone, rect) in drop_strips(area) {
        if rect.contains(pos) {
            return Some(zone);
        }
    }
    Some(DockZone::Floating)
}

/// 投放结算（拖拽松开）。
///
/// 规则：落点在**源停靠区当前矩形**内时视为「未移动」——宽的停靠面板内
/// 小幅拖动页签不应误触发浮动（落点同时落在中央区与源面板矩形重叠处）。
///
/// `origin` = (拖拽面板的源区, 源区上一帧矩形)；浮动窗源不参与取消判定。
pub fn resolve_drop(pos: Pos2, area: Rect, origin: Option<(DockZone, Rect)>) -> Option<DockZone> {
    let target = drop_target_at(pos, area)?;
    if target == DockZone::Floating {
        if let Some((zone, rect)) = origin {
            if zone != DockZone::Floating && rect.contains(pos) {
                return Some(zone);
            }
        }
    }
    Some(target)
}

// ===== 停靠区页签条（可拖动改停靠 / × 关闭 / 右键全部开关）=====

/// 页签条动作（app 结算）。
#[derive(Default)]
pub struct StripActions {
    /// 点击激活页签。
    pub activated: Option<PanelId>,
    /// × 关闭面板。
    pub closed: Option<PanelId>,
    /// 拖拽开始（面板 id）。
    pub drag_started: Option<PanelId>,
    /// 右键菜单：全部打开/全部关闭。
    pub set_all_open: Option<bool>,
}

/// 停靠区页签条：页签（单击切换、可拖拽发起改停靠、选中下划线）+
/// 每页签 × 小按钮 + 尾部空白右键菜单（全部打开/全部关闭）。
/// 样式出自 tokens/palette（与 ui_kit::tab_strip 同一下划线语言）。
pub fn dock_tab_strip(
    ui: &mut egui::Ui,
    panels: &[PanelId],
    active: PanelId,
    dragging: Option<PanelId>,
) -> StripActions {
    let p = palette_of(ui);
    let mut out = StripActions::default();
    // horizontal_wrapped：窄停靠区（200px 下限）内页签过多时换行而非截断溢出。
    ui.horizontal_wrapped(|ui| {
        ui.add_space(spacing::SM);
        for &id in panels {
            let is_active = id == active;
            let galley = ui.painter().layout_no_wrap(
                id.title().to_string(),
                egui::FontId::proportional(text::SIZE_BODY),
                Color32::WHITE,
            );
            // 页签宽 = 内边距 + 文本 + 关闭钮槽（16px）。
            let w = spacing::SM + galley.size().x + spacing::SM + 16.0;
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(w, sizes::CONTROL_SM), Sense::click_and_drag());
            // 选中底色 0.12s 淡入（HOVER_SECS 复用）；悬停即 hover 底。
            let act_t = ui.ctx().animate_bool_with_time(
                resp.id.with("active"),
                is_active,
                crate::ui_kit::tokens::animation::HOVER_SECS,
            );
            if act_t > 0.0 {
                let sel = crate::ui_kit::tokens::state::selection_bg(&p);
                ui.painter().rect_filled(
                    rect,
                    radius::SM,
                    egui::Color32::from_rgba_unmultiplied(
                        sel.r(),
                        sel.g(),
                        sel.b(),
                        (f32::from(sel.a()) * act_t) as u8,
                    ),
                );
            } else if resp.hovered() || dragging == Some(id) {
                ui.painter().rect_filled(
                    rect,
                    radius::SM,
                    crate::ui_kit::tokens::state::hover_bg(&p),
                );
            }
            let t = if is_active {
                p.text_primary
            } else {
                p.text_weak
            };
            ui.painter().text(
                Pos2::new(rect.min.x + spacing::SM, rect.center().y),
                egui::Align2::LEFT_CENTER,
                id.title(),
                egui::FontId::proportional(text::SIZE_BODY),
                t,
            );
            if is_active {
                let y = rect.max.y - 1.0;
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.min.x + spacing::XS, y),
                        Pos2::new(rect.max.x - spacing::XS, y),
                    ],
                    Stroke::new(2.0, p.accent),
                );
            }
            if resp.clicked() {
                out.activated = Some(id);
            }
            if resp.drag_started() {
                out.drag_started = Some(id);
            }

            // × 关闭小按钮（弱色常态，悬停朱砂——关闭语义）。
            let close_rect = Rect::from_center_size(
                Pos2::new(rect.max.x - spacing::SM - 8.0, rect.center().y),
                Vec2::splat(16.0),
            );
            let close_resp = ui.interact(
                close_rect,
                ui.id().with(("dock_tab_close", id.index())),
                Sense::click(),
            );
            ui.painter().text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "×",
                egui::FontId::proportional(text::SIZE_BODY),
                if close_resp.hovered() {
                    p.accent_secondary
                } else {
                    p.text_weak
                },
            );
            if close_resp.clicked() {
                out.closed = Some(id);
            }
            close_resp.on_hover_text("关闭面板");
            ui.add_space(spacing::XS);
        }
        // 尾部空白：右键「全部打开/全部关闭」。
        let (_r, blank) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        blank.context_menu(|ui| {
            if ui.button("全部打开").clicked() {
                out.set_all_open = Some(true);
                ui.close();
            }
            if ui.button("全部关闭").clicked() {
                out.set_all_open = Some(false);
                ui.close();
            }
        });
    });
    out
}

/// 拖拽中的投放提示（Foreground 层）：三个边缘投放区半透明高亮
/// （当前落区加深）+ 中央「松开 → 浮动窗口」提示。色彩出自 palette。
pub fn paint_drop_hints(
    ctx: &egui::Context,
    p: &Palette,
    area: Rect,
    pointer: Option<Pos2>,
    origin: Option<(DockZone, Rect)>,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("dock_drop"),
    ));
    let target = pointer.and_then(|pos| resolve_drop(pos, area, origin));
    for (zone, rect) in drop_strips(area) {
        let hot = target == Some(zone);
        let label = match zone {
            DockZone::Left => "停靠到左区",
            DockZone::Right => "停靠到右区",
            DockZone::Bottom => "停靠到底部",
            DockZone::Floating => unreachable!(),
        };
        painter.rect_filled(
            rect,
            0.0,
            // 热区用强调浅派生（accent_light）提亮。
            (if hot { p.accent_light } else { p.accent }).gamma_multiply(if hot {
                0.35
            } else {
                0.10
            }),
        );
        painter.rect_stroke(
            rect.shrink(1.0),
            0.0,
            Stroke::new(1.0, p.accent.gamma_multiply(if hot { 0.9 } else { 0.4 })),
            egui::StrokeKind::Middle,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(text::SIZE_BODY),
            if hot { p.accent } else { p.text_weak },
        );
    }
    if target == Some(DockZone::Floating) {
        painter.text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            "松开 → 浮动窗口",
            egui::FontId::proportional(text::SIZE_HEADING),
            p.text_weak,
        );
    }
}

/// 当前色板（Ui 内，与 ui_kit 控件同一取法）。
fn palette_of(ui: &egui::Ui) -> Palette {
    palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_max(Pos2::new(0.0, 126.0), Pos2::new(1280.0, 772.0))
    }

    // —— 默认布局 ——
    #[test]
    fn default_layout_matches_current_app() {
        let d = DockState::default();
        assert_eq!(
            d.panels_in(DockZone::Left),
            vec![PanelId::Catalog, PanelId::Layers]
        );
        assert_eq!(
            d.panels_in(DockZone::Bottom),
            vec![PanelId::Console, PanelId::AiChat]
        );
        assert!(d.panels_in(DockZone::Right).is_empty());
        assert_eq!(d.active_in(DockZone::Left), Some(PanelId::Layers));
        assert_eq!(d.active_in(DockZone::Bottom), Some(PanelId::Console));
        assert!(!d.zone_has_panels(DockZone::Right));
    }

    // —— 关闭后页签回落 / 区自动隐藏 ——
    #[test]
    fn close_active_tab_falls_back() {
        let mut d = DockState::default();
        d.close_panel(PanelId::Layers);
        assert_eq!(d.active_in(DockZone::Left), Some(PanelId::Catalog));
        d.close_panel(PanelId::Catalog);
        assert_eq!(d.active_in(DockZone::Left), None);
        assert!(!d.zone_has_panels(DockZone::Left)); // 全关 → 区隐藏
    }

    // —— 改停靠：移动 + 打开 + 置为当前页签 ——
    #[test]
    fn dock_to_moves_and_activates() {
        let mut d = DockState::default();
        d.dock_to(PanelId::AiChat, DockZone::Right);
        assert_eq!(d.panels_in(DockZone::Right), vec![PanelId::AiChat]);
        assert_eq!(d.panels_in(DockZone::Bottom), vec![PanelId::Console]);
        assert_eq!(d.active_in(DockZone::Right), Some(PanelId::AiChat));
        // 再移回底部。
        d.dock_to(PanelId::AiChat, DockZone::Bottom);
        assert_eq!(
            d.panels_in(DockZone::Bottom),
            vec![PanelId::Console, PanelId::AiChat]
        );
    }

    // —— 浮动 / 重开 ——
    #[test]
    fn float_and_reopen() {
        let mut d = DockState::default();
        d.float(PanelId::Console);
        assert!(d.is_floating(PanelId::Console));
        assert_eq!(d.active_in(DockZone::Bottom), Some(PanelId::AiChat)); // 回落
        d.close_panel(PanelId::Console);
        assert!(!d.is_open(PanelId::Console));
        assert!(!d.is_floating(PanelId::Console)); // 关闭的浮动窗不再呈现
        d.open_panel(PanelId::Console);
        assert!(d.is_floating(PanelId::Console)); // 重开回到浮动（记住所在区）
        d.dock_to(PanelId::Console, DockZone::Left);
        assert_eq!(d.zone_of(PanelId::Console), DockZone::Left);
    }

    // —— 全部开关 ——
    #[test]
    fn set_all_open_toggles_everything() {
        let mut d = DockState::default();
        d.set_all_open(false);
        assert!(!d.zone_has_panels(DockZone::Left));
        assert!(!d.zone_has_panels(DockZone::Bottom));
        assert!(PanelId::ALL.iter().all(|&id| !d.is_open(id)));
        d.set_all_open(true);
        assert!(PanelId::ALL.iter().all(|&id| d.is_open(id)));
    }

    // —— 落区几何 ——
    #[test]
    fn drop_target_geometry() {
        let a = area();
        assert_eq!(
            drop_target_at(Pos2::new(10.0, 400.0), a),
            Some(DockZone::Left)
        );
        assert_eq!(
            drop_target_at(Pos2::new(1270.0, 400.0), a),
            Some(DockZone::Right)
        );
        assert_eq!(
            drop_target_at(Pos2::new(640.0, 760.0), a),
            Some(DockZone::Bottom)
        );
        // 角落归左右（底条不含角落）。
        assert_eq!(
            drop_target_at(Pos2::new(5.0, 770.0), a),
            Some(DockZone::Left)
        );
        assert_eq!(
            drop_target_at(Pos2::new(640.0, 400.0), a),
            Some(DockZone::Floating)
        );
        assert_eq!(drop_target_at(Pos2::new(2000.0, 400.0), a), None); // 区外
        assert_eq!(drop_target_at(Pos2::new(640.0, 10.0), a), None); // Ribbon 上
    }

    // —— 源区取消规则：在源停靠面板矩形内松开不触发浮动 ——
    #[test]
    fn resolve_drop_cancels_float_over_origin() {
        let a = area();
        let left_rect = Rect::from_min_max(Pos2::new(0.0, 126.0), Pos2::new(280.0, 772.0));
        let origin = Some((DockZone::Left, left_rect));
        // 左区内部中央偏右（x>160 → 几何上属中央区），但因源区取消规则回落左区。
        assert_eq!(
            resolve_drop(Pos2::new(200.0, 400.0), a, origin),
            Some(DockZone::Left)
        );
        // 拖到画布中央（远离源区）→ 浮动。
        assert_eq!(
            resolve_drop(Pos2::new(700.0, 400.0), a, origin),
            Some(DockZone::Floating)
        );
        // 拖到右边缘 → 右区（源区取消不影响边缘判定）。
        assert_eq!(
            resolve_drop(Pos2::new(1270.0, 400.0), a, origin),
            Some(DockZone::Right)
        );
        // 无源（新拖入）→ 中央即浮动。
        assert_eq!(
            resolve_drop(Pos2::new(200.0, 400.0), a, None),
            Some(DockZone::Floating)
        );
    }

    // —— 浮动窗标题条命中 ——
    #[test]
    fn floating_title_hit() {
        let mut d = DockState::default();
        d.float(PanelId::Console);
        d.float_rects[PanelId::Console.index()] = Some(Rect::from_min_max(
            Pos2::new(400.0, 300.0),
            Pos2::new(760.0, 620.0),
        ));
        assert_eq!(
            d.hit_floating_title(Pos2::new(500.0, 310.0)),
            Some(PanelId::Console)
        ); // 标题条内
        assert_eq!(d.hit_floating_title(Pos2::new(500.0, 500.0)), None); // 内容区
        assert_eq!(d.hit_floating_title(Pos2::new(100.0, 100.0)), None); // 窗外
        d.close_panel(PanelId::Console);
        assert_eq!(d.hit_floating_title(Pos2::new(500.0, 310.0)), None); // 已关不命中
    }
}
