//! 地图框（ArcGIS Pro 地图框范式）：每框绑定**自己的图层集**（layers/toc/渲染缓存/
//! 视口/维度），切换地图框 → 图层面板内容跟随切换。
//!
//! ## 交换模型
//!
//! 激活框的状态平铺在 app 字段（layers/toc/render_cache/merged/data_extent/
//! view_bbox/needs_fit/frame_dim/canvas/scene——保持既有字段级借用拆分），
//! 休眠框的状态驻留在自身 [`FrameSite`]；激活/休眠经 app 的 park/unpark 逐项交换。
//! 休眠框（含浮动窗）渲染一律读自身 site，与激活框互不干扰。
//!
//! ## 约定
//!
//! - **关闭 ≠ 删除**：页签/浮动窗 × 仅置 `open=false`（目录清单保留、可双击重开）；
//!   删除仅目录右键「删除」。
//! - 主框「地图」（`frames[0]`）唯一特权：**不可删除**（app::delete_frame 阻止）；
//!   可关闭、可重命名、可切三维，与后续框功能一致。
//! - **二维与三维分开建立**（新建即定型）；框内分段切换保留（一致性取舍：
//!   场景草稿可先二维配数据再切三维查看，免去重建）。
//! - 编辑会话仅作用激活框；会话进行中阻止切换/关闭/删除（app 侧拦截）。

use eframe::egui;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};
use geojson::FeatureCollection;
use kanyu_core::{Layer, LayerSummary};

use crate::canvas::{CanvasInput, MapCanvas};
use crate::scene3d::Scene3D;
use crate::theme::{palette, Palette};
use crate::toc::TocNode;
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

    /// 持久化键（.kyu / ui-state）。
    pub fn as_str(self) -> &'static str {
        match self {
            ViewDim::TwoD => "2d",
            ViewDim::ThreeD => "3d",
        }
    }

    /// 持久化键解析（不识值回退二维）。
    pub fn parse(s: &str) -> Self {
        match s {
            "3d" => ViewDim::ThreeD,
            _ => ViewDim::TwoD,
        }
    }

    /// 新建入口名（「新建二维地图框」/「新建三维场景」）。
    pub fn create_label(self) -> &'static str {
        match self {
            ViewDim::TwoD => "二维地图框",
            ViewDim::ThreeD => "三维场景",
        }
    }
}

/// 图层条目（壳层图层集单元；地图框绑定图层集后归属本模块）。
pub(crate) struct LayerEntry {
    pub(crate) layer: Layer,
    pub(crate) summary: LayerSummary,
    pub(crate) visible: bool,
    pub(crate) file_name: String,
    /// 骨架目录子节点展开。
    pub(crate) expanded: bool,
    /// 数据源路径（内存图层为 None，不入 .kyu 工程）。
    pub(crate) source_path: Option<String>,
    /// 符号化（默认单色按几何类型；属性页可改，.kyu 持久化）。
    pub(crate) symbology: crate::symbology::LayerSymbology,
}

/// 地图框状态位（激活时与 app 平铺字段逐项交换；休眠驻留）。
pub(crate) struct FrameSite {
    pub(crate) layers: Vec<LayerEntry>,
    pub(crate) toc: Vec<TocNode>,
    /// 逐图层渲染缓存（id + 集合 + 符号化；休眠框浮动窗渲染用）。
    pub(crate) render_cache: Vec<(String, FeatureCollection, crate::symbology::LayerSymbology)>,
    /// 有效可见图层合并缓存（导出用）。
    pub(crate) merged: FeatureCollection,
    pub(crate) data_extent: Option<BBox>,
    pub(crate) view_bbox: Option<BBox>,
    pub(crate) needs_fit: bool,
    pub(crate) dim: ViewDim,
    pub(crate) canvas: MapCanvas,
    pub(crate) scene: Scene3D,
}

impl Default for FrameSite {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            toc: Vec::new(),
            render_cache: Vec::new(),
            merged: FeatureCollection {
                bbox: None,
                features: Vec::new(),
                foreign_members: None,
            },
            data_extent: None,
            view_bbox: None,
            needs_fit: true,
            dim: ViewDim::TwoD,
            canvas: MapCanvas::default(),
            scene: Scene3D::default(),
        }
    }
}

/// 地图框（身份 + 停靠/开闭 + 休眠状态位）。
pub struct MapFrame {
    /// 框序号（纹理 id/窗口 id 键；主框恒 0）。
    pub id: usize,
    /// 标题（目录行/页签；可重命名）。
    pub title: String,
    /// 是否吸附在中央页签（false = 浮动窗）。
    pub docked: bool,
    /// 打开状态（false = 已关闭：目录保留，可重开）。
    pub open: bool,
    /// 浮动窗上一帧矩形（拖到页签条吸附的判定用；运行态不持久化）。
    pub win_rect: Option<Rect>,
    /// WMS 底图连接名（None = 无底图；框身份属性——激活/休眠不换出）。
    pub wms_base: Option<String>,
    /// 状态位（休眠框的全部现场；激活框此处为交换余量，勿直读）。
    pub(crate) site: FrameSite,
}

impl MapFrame {
    /// 新建地图框（默认吸附中央、打开；dim 创建定型）。
    pub fn new(id: usize, title: String, dim: ViewDim) -> Self {
        let site = FrameSite {
            dim,
            canvas: MapCanvas::with_name(format!("map-frame-{id}")),
            ..Default::default()
        };
        Self {
            id,
            title,
            docked: true,
            open: true,
            win_rect: None,
            wms_base: None,
            site,
        }
    }

    /// 默认主框（id 0，标题「地图」；纹理名与 app 初始画布一致）。
    pub fn main() -> Self {
        Self {
            id: 0,
            title: "地图".to_string(),
            docked: true,
            open: true,
            win_rect: None,
            wms_base: None,
            site: FrameSite::default(),
        }
    }
}

/// 渲染缓存 → 画布切片（自下而上序；激活框读 app.render_cache、
/// 休眠框读自身 site.render_cache，共用本函数）。
pub(crate) fn build_layer_slices(
    cache: &[(String, FeatureCollection, crate::symbology::LayerSymbology)],
) -> Vec<crate::canvas::LayerSlice<'_>> {
    cache
        .iter()
        .map(|rc| crate::canvas::LayerSlice {
            id: &rc.0,
            collection: &rc.1,
            style: Some(crate::symbology::to_style_rule(&rc.2)),
            color: {
                let c = crate::symbology::primary_color(&rc.2);
                egui::Color32::from_rgb(c[0], c[1], c[2])
            },
        })
        .collect()
}

/// 框状态借出（激活框借 app 平铺字段；休眠框借自身 site——调用方组装）。
pub struct FrameState<'a> {
    pub dim: &'a mut ViewDim,
    pub view_bbox: &'a mut Option<BBox>,
    pub needs_fit: &'a mut bool,
    pub canvas: &'a mut MapCanvas,
    pub scene: &'a mut Scene3D,
    pub docked: &'a mut bool,
}

/// 视图帧输入。
pub struct ViewInput<'a> {
    /// 有效可见图层切片（自下而上绘制；含符号化与主色）。
    pub layers: &'a [crate::canvas::LayerSlice<'a>],
    /// 地图主题（与主画布同一 effective_map_theme）。
    pub theme: kanyu_render::Theme,
    /// 数据范围（缩放到图层用）。
    pub data_extent: Option<BBox>,
    /// 编辑态（仅激活框二维时 Some；编辑会话作用激活框）。
    pub edit: Option<crate::canvas::EditView<'a>>,
    /// 空态提示（无可见图层时；激活空框给拖入引导）。
    pub empty_hint: &'a str,
    /// wgpu 3D 真管线可用（三维态工具条呈现 软件/wgpu 开关）。
    pub wgpu3d: bool,
    /// 视图 id（wgpu 离屏纹理/顶点缓冲分键）。
    pub view_id: usize,
    /// 内容纪元（wgpu 网格缓存键）。
    pub epoch: u64,
}

/// 视图帧输出（编辑动作/鼠标坐标/渲染错误回传 app）。
#[derive(Default)]
pub struct FrameOut {
    /// 编辑手势产出。
    pub edit_action: Option<crate::edit::EditAction>,
    /// 鼠标数据坐标（状态栏）。
    pub mouse_data: Option<(f64, f64)>,
    /// 渲染错误。
    pub render_error: Option<String>,
    /// needs_fit 已消费。
    pub fit_consumed: bool,
}

/// 视图内容（工具条 + 画布区；窗口与吸附两种承载共用）。
pub fn content_ui(
    ui: &mut egui::Ui,
    state: FrameState<'_>,
    input: ViewInput<'_>,
    in_window: bool,
) -> FrameOut {
    let mut out = FrameOut::default();
    // 内容区铺底（中央区/浮动窗内容无默认填充——防透出窗口清屏色形成黑缝；
    // 画布区随后自铺纯白覆盖）。
    let bg = palette_of(ui).bg_primary;
    ui.painter()
        .rect_filled(ui.available_rect_before_wrap(), 0.0, bg);
    // 工具条。
    ui.horizontal(|ui| {
        // 框内二维/三维切换保留（创建定型，切换免重建——见模块头约定）。
        ui.selectable_value(state.dim, ViewDim::TwoD, ViewDim::TwoD.label());
        ui.selectable_value(state.dim, ViewDim::ThreeD, ViewDim::ThreeD.label());
        ui.separator();
        if ui
            .button(text::body("缩放到图层"))
            .on_hover_text("以全部可见图层范围适配")
            .clicked()
        {
            *state.needs_fit = true;
        }
        if ui.button(text::body("复位视图")).clicked() {
            *state.view_bbox = None;
            *state.needs_fit = true;
        }
        if in_window {
            ui.separator();
            if ui
                .button(text::body("停靠到中央"))
                .on_hover_text("吸附回中央页签（或拖本窗标题栏到页签条）")
                .clicked()
            {
                *state.docked = true;
            }
        }
        // 三维态 + wgpu 可用：渲染后端开关（软件/wgpu 真管线）。
        if matches!(*state.dim, ViewDim::ThreeD) && input.wgpu3d {
            ui.separator();
            ui.selectable_value(
                &mut state.scene.backend,
                crate::scene3d::SceneBackend::Software,
                text::body("软件"),
            );
            ui.selectable_value(
                &mut state.scene.backend,
                crate::scene3d::SceneBackend::Wgpu,
                text::body("wgpu"),
            );
        }
    });
    ui.separator();
    // 画布区。
    match *state.dim {
        ViewDim::TwoD => {
            let co = state.canvas.ui(
                ui,
                CanvasInput {
                    layers: input.layers,
                    theme: input.theme,
                    view_bbox: *state.view_bbox,
                    needs_fit: *state.needs_fit,
                    data_extent: input.data_extent,
                    empty_hint: input.empty_hint,
                    edit: input.edit,
                },
            );
            *state.view_bbox = co.view_bbox;
            if co.fit_consumed {
                *state.needs_fit = false;
                out.fit_consumed = true;
            }
            out.mouse_data = co.mouse_data;
            out.render_error = co.render_error;
            out.edit_action = co.edit_action;
        }
        ViewDim::ThreeD => {
            let p = crate::theme::palette(if ui.visuals().dark_mode {
                kanyu_render::Theme::Dark
            } else {
                kanyu_render::Theme::Light
            });
            crate::scene3d::ui(
                ui,
                state.scene,
                input.layers,
                state.view_bbox,
                state.needs_fit,
                input.data_extent,
                &p,
                input.wgpu3d,
                input.view_id,
                input.epoch,
            );
        }
    }
    out
}

/// 浮动视图窗口帧（仅休眠框：浮动即非激活——激活框恒吸附中央）。
pub fn window_ui(
    ctx: &egui::Context,
    view: &mut MapFrame,
    theme: kanyu_render::Theme,
    crs: &str,
    wgpu3d: bool,
    epoch: u64,
) {
    // 休眠框图层切片/数据范围读自身 site（与激活框互不干扰）。
    let slices = build_layer_slices(&view.site.render_cache);
    let input = ViewInput {
        layers: &slices,
        theme,
        data_extent: view.site.data_extent,
        edit: None,
        empty_hint: "（本地图框无可见图层——激活后加载数据即归属此框）",
        wgpu3d,
        view_id: view.id,
        epoch,
    };
    let mut open = view.open;
    let resp = egui::Window::new(format!("{} · {}", view.title, crs))
        .id(egui::Id::new(("map_view", view.id)))
        .default_size([420.0, 320.0])
        // 默认位置靠右，避免遮挡左侧目录/图层面板。
        .default_pos([640.0, 280.0])
        .open(&mut open)
        .show(ctx, |ui| {
            content_ui(
                ui,
                FrameState {
                    dim: &mut view.site.dim,
                    view_bbox: &mut view.site.view_bbox,
                    needs_fit: &mut view.site.needs_fit,
                    canvas: &mut view.site.canvas,
                    scene: &mut view.site.scene,
                    docked: &mut view.docked,
                },
                input,
                true,
            )
        });
    if let Some(inner) = &resp {
        view.win_rect = Some(inner.response.rect);
    }
    view.open = open;
}

// ===== 中央视图页签条 =====

/// 中央页签键。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CentralTabKey {
    /// 地图框（frames 下标；0 = 主框「地图」）。
    Map(usize),
    /// 布局视图（layouts 下标）。
    Layout(usize),
}

/// 页签条动作。
#[derive(Default)]
pub struct ViewStripActions {
    /// 激活页签。
    pub activated: Option<CentralTabKey>,
    /// 关闭页签（关闭 ≠ 删除：目录清单保留）。
    pub closed: Option<CentralTabKey>,
    /// 弹出为浮动窗（frames 下标；主框不可弹出）。
    pub floated: Option<usize>,
    /// 「新建二维地图框」。
    pub new_view_2d: bool,
    /// 「新建三维场景」。
    pub new_view_3d: bool,
}

/// 中央视图页签条：`＋`（新建菜单：二维地图框/三维场景）+ 各打开且吸附的
/// 页签（地图框：⤢ 弹出 / × 关闭，主框不可弹出；布局：× 关闭）。
/// 样式与 dock 页签条同一语言。
/// `docked` = (页签键, 标题) 序列；`active` = 当前页签。
pub fn view_tab_strip(
    ui: &mut egui::Ui,
    docked: &[(CentralTabKey, String)],
    active: Option<CentralTabKey>,
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
                // ＋新建（菜单：二维地图框 / 三维场景——二维与三维分开建立）。
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
                egui::Popup::menu(&plus).show(|ui| {
                    if ui.button(text::body("新建二维地图框")).clicked() {
                        out.new_view_2d = true;
                        ui.close();
                    }
                    if ui.button(text::body("新建三维场景")).clicked() {
                        out.new_view_3d = true;
                        ui.close();
                    }
                });
                plus.on_hover_text("新建地图框（二维/三维分开建立）");
                ui.add_space(spacing::XS);

                // 吸附页签（主框不可弹出；全部可关闭——关闭 ≠ 删除）。
                for (key, title) in docked {
                    let floatable = matches!(key, CentralTabKey::Map(i) if *i != 0);
                    let r = strip_tab(ui, &p, title, active == Some(*key), true, floatable);
                    if r.clicked {
                        out.activated = Some(*key);
                    }
                    if r.close_clicked {
                        out.closed = Some(*key);
                    }
                    if r.float_clicked {
                        if let CentralTabKey::Map(i) = key {
                            out.floated = Some(*i);
                        }
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
    // 尾部小钮（↗ 弹出 / × 关闭；↗ 取常见字形——⤢ 在 CJK 字体栈缺字形显豆腐块）。
    let mut bx = rect.max.x - spacing::SM - 16.0;
    for (glyph, tip, is_close) in [
        ("↗", "弹出为浮动窗", false),
        ("×", "关闭（目录中保留）", true),
    ]
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

/// 删除/移除后的激活下标调整（纯函数）：删中激活项 → None；
/// 激活项在被删项之后 → 前移一位。
pub(crate) fn adjust_active_after_remove(active: Option<usize>, removed: usize) -> Option<usize> {
    match active {
        Some(a) if a == removed => None,
        Some(a) if a > removed => Some(a - 1),
        other => other,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dim_labels_and_parse() {
        assert_eq!(ViewDim::TwoD.as_str(), "2d");
        assert_eq!(ViewDim::ThreeD.as_str(), "3d");
        assert_eq!(ViewDim::parse("3d"), ViewDim::ThreeD);
        assert_eq!(ViewDim::parse("2d"), ViewDim::TwoD);
        assert_eq!(ViewDim::parse("未知"), ViewDim::TwoD);
        assert_eq!(ViewDim::ThreeD.create_label(), "三维场景");
    }

    #[test]
    fn frame_construct_and_site_default() {
        let main = MapFrame::main();
        assert_eq!(main.id, 0);
        assert_eq!(main.title, "地图");
        assert!(main.open && main.docked);
        assert_eq!(main.site.dim, ViewDim::TwoD);
        assert!(main.site.layers.is_empty());
        let f = MapFrame::new(2, "场景 2".to_string(), ViewDim::ThreeD);
        assert_eq!(f.site.dim, ViewDim::ThreeD);
        assert!(f.site.needs_fit);
    }

    #[test]
    fn adjust_active_after_remove_cases() {
        assert_eq!(adjust_active_after_remove(Some(2), 2), None);
        assert_eq!(adjust_active_after_remove(Some(3), 1), Some(2));
        assert_eq!(adjust_active_after_remove(Some(0), 2), Some(0));
        assert_eq!(adjust_active_after_remove(None, 1), None);
    }
}
