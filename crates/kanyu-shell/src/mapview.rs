//! 多地图视图（ArcGIS Pro 文档页签范式）：主画布 = 默认视图「地图」（恒吸附、
//! 不可关闭），额外视图默认**吸附在中央页签条**，可弹出为浮动窗（egui::Window）
//! 并可经「停靠到中央」按钮或拖到页签条吸附回来。
//!
//! 每视图独立视口与二维/三维切换，内容绑定当前 merged（有效可见图层合并缓存）。

use eframe::egui;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

use crate::canvas::{CanvasInput, MapCanvas};
use crate::scene3d::Scene3D;
use crate::theme::{palette, Palette};
use crate::ui_kit::tokens::{radius, sizes, spacing, text};
use crate::view::BBox;

/// 视图维度。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewDim {
    /// 二维（render_png 纹理）。
    TwoD,
    /// 三维（实验性软件 3D，见 scene3d）。
    ThreeD,
}

impl ViewDim {
    /// 中文显示名（segmented 切换）。
    pub fn label(self) -> &'static str {
        match self {
            ViewDim::TwoD => "二维",
            ViewDim::ThreeD => "三维（实验性）",
        }
    }
}

/// 地图视图。
pub struct MapView {
    /// 视图序号（纹理 id/窗口 id 键）。
    pub id: usize,
    /// 标题（「地图 2」递增）。
    pub title: String,
    /// 独立视口。
    pub view_bbox: Option<BBox>,
    /// 待首帧嵌入。
    pub needs_fit: bool,
    /// 维度。
    pub dim: ViewDim,
    /// 二维画布态。
    pub canvas: MapCanvas,
    /// 三维场景态。
    pub scene: Scene3D,
    /// 是否吸附在中央页签（false = 浮动窗）。
    pub docked: bool,
    /// 窗口开关（浮动态 × 关闭）。
    pub open: bool,
    /// 浮动窗上一帧矩形（拖到页签条吸附的判定用；运行态不持久化）。
    pub win_rect: Option<Rect>,
}

impl MapView {
    /// 新建视图（默认二维、吸附中央）。
    pub fn new(id: usize) -> Self {
        Self {
            id,
            title: format!("地图 {id}"),
            view_bbox: None,
            needs_fit: true,
            dim: ViewDim::TwoD,
            canvas: MapCanvas::with_name(format!("map-view-{id}")),
            scene: Scene3D::default(),
            docked: true,
            open: true,
            win_rect: None,
        }
    }
}

/// 视图帧输入。
pub struct ViewInput<'a> {
    /// 有效可见图层切片（自下而上绘制；含符号化与主色）。
    pub layers: &'a [crate::canvas::LayerSlice<'a>],
    /// 地图主题（与主画布同一 effective_map_theme）。
    pub theme: kanyu_render::Theme,
    /// 数据范围（缩放到图层用）。
    pub data_extent: Option<BBox>,
    /// 工程坐标系（标题栏显示）。
    pub crs: &'a str,
}

/// 视图内容（工具条 + 画布区；窗口与吸附两种承载共用）。
pub fn content_ui(ui: &mut egui::Ui, view: &mut MapView, input: &ViewInput<'_>, in_window: bool) {
    // 工具条。
    ui.horizontal(|ui| {
        ui.selectable_value(&mut view.dim, ViewDim::TwoD, ViewDim::TwoD.label());
        ui.selectable_value(&mut view.dim, ViewDim::ThreeD, ViewDim::ThreeD.label());
        ui.separator();
        if ui
            .button(text::body("缩放到图层"))
            .on_hover_text("以全部可见图层范围适配")
            .clicked()
        {
            view.needs_fit = true;
        }
        if ui.button(text::body("复位视图")).clicked() {
            view.view_bbox = None;
            view.needs_fit = true;
        }
        if in_window {
            ui.separator();
            if ui
                .button(text::body("停靠到中央"))
                .on_hover_text("吸附回中央页签（或拖本窗标题栏到页签条）")
                .clicked()
            {
                view.docked = true;
            }
        }
    });
    ui.separator();
    // 画布区。
    match view.dim {
        ViewDim::TwoD => {
            let out = view.canvas.ui(
                ui,
                CanvasInput {
                    layers: input.layers,
                    theme: input.theme,
                    view_bbox: view.view_bbox,
                    needs_fit: view.needs_fit,
                    data_extent: input.data_extent,
                    empty_hint: "（无可见图层）",
                },
            );
            view.view_bbox = out.view_bbox;
            if out.fit_consumed {
                view.needs_fit = false;
            }
        }
        ViewDim::ThreeD => {
            let p = crate::theme::palette(if ui.visuals().dark_mode {
                kanyu_render::Theme::Dark
            } else {
                kanyu_render::Theme::Light
            });
            crate::scene3d::ui(
                ui,
                &mut view.scene,
                input.layers,
                &mut view.view_bbox,
                &mut view.needs_fit,
                input.data_extent,
                &p,
            );
        }
    }
}

/// 浮动视图窗口帧。
pub fn window_ui(ctx: &egui::Context, view: &mut MapView, input: &ViewInput<'_>) {
    let mut open = view.open;
    let resp = egui::Window::new(format!("{} · {}", view.title, input.crs))
        .id(egui::Id::new(("map_view", view.id)))
        .default_size([420.0, 320.0])
        // 默认位置靠右，避免遮挡左侧目录/图层面板。
        .default_pos([640.0, 280.0])
        .open(&mut open)
        .show(ctx, |ui| {
            content_ui(ui, view, input, true);
        });
    if let Some(inner) = &resp {
        view.win_rect = Some(inner.response.rect);
    }
    view.open = open;
}

// ===== 中央视图页签条 =====

/// 页签条动作。
#[derive(Default)]
pub struct ViewStripActions {
    /// 激活页签（None = 主视图「地图」）。
    pub activated: Option<Option<usize>>,
    /// 关闭视图（下标）。
    pub closed: Option<usize>,
    /// 弹出为浮动窗（下标）。
    pub floated: Option<usize>,
    /// 「＋新建地图框」。
    pub new_view: bool,
}

/// 中央视图页签条：`＋新建地图框` + 主视图页签（不可关/不可弹出）+
/// 各吸附视图页签（⤢ 弹出 / × 关闭）。样式与 dock 页签条同一语言。
/// `docked` = (map_views 下标, 标题) 序列；`active` = 当前页签（None = 主视图）。
pub fn view_tab_strip(
    ui: &mut egui::Ui,
    docked: &[(usize, String)],
    active: Option<usize>,
) -> ViewStripActions {
    let p = palette_of(ui);
    let mut out = ViewStripActions::default();
    // 页签条铺底：eframe App::ui 的中央区无默认填充（透出窗口清屏色），
    // 条带按全可用宽度自铺面板底色。
    let bg_rect = egui::Rect::from_min_size(
        ui.cursor().min,
        Vec2::new(ui.available_width(), sizes::CONTROL_SM + 4.0),
    );
    ui.painter().rect_filled(bg_rect, 0.0, p.bg_primary);
    // 条带底部发丝线（替代 separator——其行背景无填充会透出清屏色）。
    ui.painter().line_segment(
        [bg_rect.left_bottom(), bg_rect.right_bottom()],
        Stroke::new(0.5, p.border),
    );
    egui::Frame::new()
        .fill(p.bg_primary)
        .inner_margin(egui::Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(spacing::SM);
                // ＋新建地图框。
                let (plus_rect, plus) =
                    ui.allocate_exact_size(Vec2::splat(sizes::CONTROL_SM), Sense::click());
                if plus.hovered() {
                    ui.painter().rect_filled(
                        plus_rect,
                        radius::SM,
                        crate::ui_kit::tokens::state::hover_bg(&p),
                    );
                }
                ui.painter().text(
                    plus_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "＋",
                    egui::FontId::proportional(text::SIZE_BODY),
                    p.text_primary,
                );
                if plus.clicked() {
                    out.new_view = true;
                }
                plus.on_hover_text("新建地图框");
                ui.add_space(spacing::XS);

                // 主视图页签（恒吸附、不可关闭/弹出）。
                if strip_tab(ui, &p, "地图", active.is_none(), false, false).clicked {
                    out.activated = Some(None);
                }

                // 吸附视图页签。
                for (idx, title) in docked {
                    let r = strip_tab(ui, &p, title, active == Some(*idx), true, true);
                    if r.clicked {
                        out.activated = Some(Some(*idx));
                    }
                    if r.close_clicked {
                        out.closed = Some(*idx);
                    }
                    if r.float_clicked {
                        out.floated = Some(*idx);
                    }
                }
            });
        });
    out
}

/// 单页签响应。
struct TabResp {
    clicked: bool,
    close_clicked: bool,
    float_clicked: bool,
}

/// 单个页签（文本 + 可选 ⤢/× 小钮）。
fn strip_tab(
    ui: &mut egui::Ui,
    p: &Palette,
    title: &str,
    is_active: bool,
    closable: bool,
    floatable: bool,
) -> TabResp {
    let mut out = TabResp {
        clicked: false,
        close_clicked: false,
        float_clicked: false,
    };
    let galley = ui.painter().layout_no_wrap(
        title.to_string(),
        egui::FontId::proportional(text::SIZE_BODY),
        egui::Color32::WHITE,
    );
    let btns = closable as usize + floatable as usize;
    let w = spacing::SM + galley.size().x + spacing::SM + btns as f32 * 18.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, sizes::CONTROL_SM), Sense::click());
    // 选中底色 0.12s 淡入（HOVER_SECS 复用）；悬停即 hover 底。
    let act_t = ui.ctx().animate_bool_with_time(
        resp.id.with("active"),
        is_active,
        crate::ui_kit::tokens::animation::HOVER_SECS,
    );
    if act_t > 0.0 {
        let sel = crate::ui_kit::tokens::state::selection_bg(p);
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
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, radius::SM, crate::ui_kit::tokens::state::hover_bg(p));
    }
    ui.painter().text(
        Pos2::new(rect.min.x + spacing::SM, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(text::SIZE_BODY),
        if is_active {
            p.text_primary
        } else {
            p.text_weak
        },
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
        out.clicked = true;
    }
    // 尾部小钮（⤢ 弹出 / × 关闭）。
    let mut bx = rect.max.x - spacing::SM - 16.0;
    for (glyph, tip, is_close) in [("⤢", "弹出为浮动窗", false), ("×", "关闭视图", true)]
        .into_iter()
        .filter(|(_, _, c)| !(*c) || closable)
        .filter(|(_, _, c)| *c || floatable)
    {
        let btn_rect =
            Rect::from_center_size(Pos2::new(bx + 8.0, rect.center().y), Vec2::splat(16.0));
        let btn = ui.interact(
            btn_rect,
            ui.id().with(("view_tab", title, glyph)),
            Sense::click(),
        );
        ui.painter().text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(text::SIZE_BODY),
            if btn.hovered() {
                p.accent_secondary
            } else {
                p.text_weak
            },
        );
        if btn.clicked() {
            if is_close {
                out.close_clicked = true;
            } else {
                out.float_clicked = true;
            }
        }
        btn.on_hover_text(tip);
        bx -= 18.0;
    }
    out
}

/// 拖到页签条的吸附提示（页签条矩形高亮 + 文案）。
pub fn paint_dock_hint(ui: &egui::Ui, strip: Rect) {
    let p = palette_of(ui);
    let painter = ui.painter();
    painter.rect_filled(strip, radius::SM, p.accent.gamma_multiply(0.18));
    painter.rect_stroke(
        strip.shrink(1.0),
        radius::SM,
        Stroke::new(1.5, p.accent),
        egui::StrokeKind::Middle,
    );
    painter.text(
        strip.center(),
        egui::Align2::CENTER_CENTER,
        "松开停靠到中央",
        egui::FontId::proportional(text::SIZE_BODY),
        p.accent,
    );
}

/// 当前色板。
fn palette_of(ui: &egui::Ui) -> Palette {
    palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
}
