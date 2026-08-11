//! 多地图视图（ArcGIS Pro 多视图范式）：主画布 = 默认视图「地图」（二维），
//! 额外视图以 egui::Window 承载（可移动/缩放/关闭），每视图独立视口与
//! 二维/三维切换，内容绑定当前 merged（有效可见图层合并缓存）。

use eframe::egui;

use crate::canvas::{CanvasInput, MapCanvas};
use crate::scene3d::Scene3D;
use crate::ui_kit::text;
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

/// 地图视图（窗口承载）。
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
    /// 窗口开关（× 关闭）。
    pub open: bool,
}

impl MapView {
    /// 新建视图（默认二维）。
    pub fn new(id: usize) -> Self {
        Self {
            id,
            title: format!("地图 {id}"),
            view_bbox: None,
            needs_fit: true,
            dim: ViewDim::TwoD,
            canvas: MapCanvas::with_name(format!("map-view-{id}")),
            scene: Scene3D::default(),
            open: true,
        }
    }
}

/// 视图帧输入。
pub struct ViewInput<'a> {
    /// 可见图层合并缓存。
    pub merged: &'a geojson::FeatureCollection,
    /// 地图主题（与主画布同一 effective_map_theme）。
    pub theme: kanyu_render::Theme,
    /// 数据范围（缩放到图层用）。
    pub data_extent: Option<BBox>,
    /// 工程坐标系（标题栏显示）。
    pub crs: &'a str,
}

/// 视图窗口帧：顶部工具条（维度切换/缩放/复位/提示）+ 画布区。
pub fn window_ui(ctx: &egui::Context, view: &mut MapView, input: &ViewInput<'_>) {
    let mut open = view.open;
    egui::Window::new(format!("{} · {}", view.title, input.crs))
        .id(egui::Id::new(("map_view", view.id)))
        .default_size([420.0, 320.0])
        .open(&mut open)
        .show(ctx, |ui| {
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
            });
            ui.separator();
            // 画布区。
            match view.dim {
                ViewDim::TwoD => {
                    let out = view.canvas.ui(
                        ui,
                        CanvasInput {
                            merged: input.merged,
                            theme: input.theme,
                            view_bbox: view.view_bbox,
                            needs_fit: view.needs_fit,
                            data_extent: input.data_extent,
                            style: None,
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
                        input.merged,
                        &mut view.view_bbox,
                        &mut view.needs_fit,
                        input.data_extent,
                        &p,
                    );
                }
            }
        });
    view.open = open;
}
