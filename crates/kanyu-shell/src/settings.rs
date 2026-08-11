//! 设置对话框（ArcGIS Pro「选项」范式）：左侧导航 + 右侧内容页。
//!
//! 坐标系与渲染等全局设置收敛于此，不占功能区。页面：
//! - **坐标系**：工程坐标系（常用 EPSG 下拉 + 手动定义，可解析性经
//!   [`kanyu_core::crs::validate_crs`] 预检）；保存进 .kyu（KanyuProject.crs），
//!   并作为投影变换的默认目标。
//! - **渲染**：地图导出尺寸/符号化样式（自旧「渲染设置」对话框迁入）+
//!   地图色彩模式三态单选。

use eframe::egui;
use egui::Vec2;
use kanyu_render::StyleRule;

use crate::app::MapThemeMode;
use crate::ui_kit::tokens::{spacing, text};
use crate::ui_kit::{button, combo, error_caption, text_area, text_input, ButtonVariant};

/// 设置页。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsPage {
    /// 坐标系。
    Crs,
    /// 渲染。
    Render,
}

impl SettingsPage {
    const ALL: [SettingsPage; 2] = [SettingsPage::Crs, SettingsPage::Render];
    fn label(self) -> &'static str {
        match self {
            SettingsPage::Crs => "坐标系",
            SettingsPage::Render => "渲染",
        }
    }
}

/// 常用坐标系（定义串 + 中文注释；手动输入不在此列的 EPSG/proj4 亦可）。
const COMMON_CRS: &[(&str, &str)] = &[
    ("EPSG:4326", "WGS84 经纬度"),
    ("EPSG:3857", "Web 墨卡托"),
    ("EPSG:4490", "CGCS2000 经纬度"),
    ("EPSG:4526", "CGCS2000 3°分带 38带"),
    ("EPSG:4527", "CGCS2000 3°分带 39带"),
];

/// 设置结果（「确定」且校验通过时产出；app 应用到工程状态）。
#[derive(Debug)]
pub struct SettingsOutcome {
    /// 工程坐标系定义串。
    pub crs: String,
    /// 地图导出尺寸（宽, 高）。
    pub export_size: (u32, u32),
    /// 符号化样式（空 = 无）。
    pub export_style: Option<StyleRule>,
    /// 地图色彩模式。
    pub map_theme: MapThemeMode,
}

/// 设置 UI 结果。
pub enum SettingsUi {
    /// 继续显示。
    Open,
    /// 取消/关闭。
    Closed,
    /// 应用。
    Applied(Box<SettingsOutcome>),
}

/// 设置对话框状态（输入缓冲随打开时快照工程当前值）。
pub struct SettingsDialog {
    page: SettingsPage,
    /// 坐标系：常用项下拉值。
    crs_choice: String,
    /// 坐标系：手动定义（非空时优先于下拉）。
    crs_manual: String,
    /// 渲染：输出宽/高（像素文本）。
    width: String,
    height: String,
    /// 渲染：符号化样式 JSON。
    style: String,
    /// 渲染：地图色彩模式。
    map_theme: MapThemeMode,
    /// 校验错误（error_caption 红字）。
    err: Option<String>,
}

impl SettingsDialog {
    /// 以工程当前值打开（符号化样式不回填——StyleRule 仅 Deserialize，
    /// 与旧「渲染设置」对话框一致：每次打开从空开始，「确定」覆盖）。
    pub fn open_with(crs: &str, export_size: (u32, u32), map_theme: MapThemeMode) -> Self {
        // 当前 CRS 命中常用项则预选下拉，否则进手动框。
        let (choice, manual) = match COMMON_CRS.iter().find(|(def, _)| *def == crs) {
            Some((def, _)) => (crs_label(def), String::new()),
            None => (crs_label(COMMON_CRS[0].0), crs.to_string()),
        };
        Self {
            page: SettingsPage::Crs,
            crs_choice: choice,
            crs_manual: manual,
            width: export_size.0.to_string(),
            height: export_size.1.to_string(),
            style: String::new(),
            map_theme,
            err: None,
        }
    }

    /// 对话框 UI。
    pub fn ui(&mut self, ctx: &egui::Context) -> SettingsUi {
        let mut out = SettingsUi::Open;
        let mut open = true;
        egui::Window::new(text::heading("设置"))
            .collapsible(false)
            .resizable(false)
            .default_size([560.0, 360.0])
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 左侧导航（ArcGIS 选项页签列）。
                    ui.allocate_ui_with_layout(
                        Vec2::new(110.0, 260.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.add_space(spacing::XS);
                            for page in SettingsPage::ALL {
                                if ui
                                    .selectable_label(self.page == page, text::body(page.label()))
                                    .clicked()
                                {
                                    self.page = page;
                                    self.err = None;
                                }
                                ui.add_space(spacing::XS);
                            }
                        },
                    );
                    ui.separator();
                    // 右侧内容页。
                    ui.vertical(|ui| {
                        ui.set_max_width(400.0);
                        match self.page {
                            SettingsPage::Crs => self.crs_page(ui),
                            SettingsPage::Render => self.render_page(ui),
                        }
                    });
                });
                ui.add_space(spacing::MD);
                ui.separator();
                ui.horizontal(|ui| {
                    if button(ui, "确 定", ButtonVariant::Primary, true).clicked() {
                        match self.validate() {
                            Ok(o) => out = SettingsUi::Applied(Box::new(o)),
                            Err(e) => self.err = Some(e),
                        }
                    }
                    if button(ui, "取 消", ButtonVariant::Secondary, true).clicked() {
                        out = SettingsUi::Closed;
                    }
                    if let Some(e) = &self.err {
                        error_caption(ui, e);
                    }
                });
            });
        if !open {
            return SettingsUi::Closed;
        }
        out
    }

    /// 坐标系页。
    fn crs_page(&mut self, ui: &mut egui::Ui) {
        ui.label(text::body("工程坐标系（保存进 .kyu，并作投影变换默认目标）：").strong());
        ui.add_space(spacing::SM);
        let options: Vec<String> = COMMON_CRS.iter().map(|(d, _)| crs_label(d)).collect();
        combo(ui, "常用坐标系", &mut self.crs_choice, &options, true);
        text_input(
            ui,
            "手动定义",
            &mut self.crs_manual,
            "如 EPSG:4547 或 +proj=…（非空时优先）",
            true,
        );
    }

    /// 渲染页。
    fn render_page(&mut self, ui: &mut egui::Ui) {
        ui.label(text::body("地图导出（PNG/SVG 输出）：").strong());
        ui.add_space(spacing::SM);
        text_input(ui, "宽度 px", &mut self.width, "64–8192", true);
        text_input(ui, "高度 px", &mut self.height, "64–8192", true);
        ui.label(text::body("符号化样式 JSON（可空）："));
        text_area(
            ui,
            &mut self.style,
            4,
            r##"{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"]]}"##,
            true,
        );
        ui.add_space(spacing::SM);
        ui.label(text::body("地图色彩模式：").strong());
        for mode in [
            MapThemeMode::FixedLight,
            MapThemeMode::FixedDark,
            MapThemeMode::FollowUi,
        ] {
            ui.radio_value(&mut self.map_theme, mode, mode.label());
        }
    }

    /// 校验并采集（纯输入 → 结果；错误中文）。
    fn validate(&self) -> Result<SettingsOutcome, String> {
        let crs = if self.crs_manual.trim().is_empty() {
            // 下拉值 "EPSG:xxxx —— 注释" 取首段定义串。
            self.crs_choice
                .split(' ')
                .next()
                .unwrap_or("EPSG:4326")
                .to_string()
        } else {
            self.crs_manual.trim().to_string()
        };
        kanyu_core::crs::validate_crs(&crs).map_err(|e| e.to_string())?;
        let w = self
            .width
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("宽度须为整数像素: {}", self.width))?
            .clamp(64, 8192);
        let h = self
            .height
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("高度须为整数像素: {}", self.height))?
            .clamp(64, 8192);
        let style = if self.style.trim().is_empty() {
            None
        } else {
            Some(
                serde_json::from_str::<StyleRule>(&self.style)
                    .map_err(|e| format!("样式 JSON 解析失败: {e}"))?,
            )
        };
        Ok(SettingsOutcome {
            crs,
            export_size: (w, h),
            export_style: style,
            map_theme: self.map_theme,
        })
    }
}

/// 常用项下拉标签："EPSG:4326 —— WGS84 经纬度"。
fn crs_label(def: &str) -> String {
    let note = COMMON_CRS
        .iter()
        .find(|(d, _)| *d == def)
        .map(|(_, n)| *n)
        .unwrap_or("");
    if note.is_empty() {
        def.to_string()
    } else {
        format!("{def} —— {note}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dlg() -> SettingsDialog {
        SettingsDialog::open_with("EPSG:4326", (1200, 800), MapThemeMode::FixedLight)
    }

    /// 打开快照：常用 CRS 预选下拉，非常用进手动框。
    #[test]
    fn open_with_prefills() {
        let d = dlg();
        assert!(d.crs_choice.starts_with("EPSG:4326"));
        assert!(d.crs_manual.is_empty());
        let d2 = SettingsDialog::open_with("EPSG:4547", (800, 600), MapThemeMode::FollowUi);
        assert_eq!(d2.crs_manual, "EPSG:4547");
        assert_eq!(d2.width, "800");
        assert_eq!(d2.map_theme, MapThemeMode::FollowUi);
    }

    /// 校验：下拉默认路径。
    #[test]
    fn validate_combo_path() {
        let o = dlg().validate().unwrap();
        assert_eq!(o.crs, "EPSG:4326");
        assert_eq!(o.export_size, (1200, 800));
        assert!(o.export_style.is_none());
    }

    /// 校验：手动定义优先 + 非法定义中文报错。
    #[test]
    fn validate_manual_overrides_and_rejects_bad() {
        let mut d = dlg();
        d.crs_manual = "EPSG:4490".to_string();
        assert_eq!(d.validate().unwrap().crs, "EPSG:4490");
        d.crs_manual = "EPSG:foo".to_string();
        assert!(d.validate().unwrap_err().contains("无法解析 CRS 定义"));
    }

    /// 校验：尺寸非法报错、越界钳制；样式 JSON 非法报错。
    #[test]
    fn validate_size_and_style() {
        let mut d = dlg();
        d.width = "abc".to_string();
        assert!(d.validate().unwrap_err().contains("宽度"));
        d.width = "99999".to_string();
        assert_eq!(d.validate().unwrap().export_size.0, 8192); // 钳制
        d.width = "1200".to_string();
        d.style = "{bad json".to_string();
        assert!(d.validate().unwrap_err().contains("样式 JSON"));
        d.style = r##"{"type":"graduated","field":"h","stops":[]}"##.to_string();
        // 结构是否合法由 StyleRule 反序列化决定——此处只验证管道不炸。
        let _ = d.validate();
    }
}
