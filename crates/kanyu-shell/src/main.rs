//! kanyu-shell —— 堪舆的桌面壳层（egui 桌面 UI MVP）。
//!
//! 布局遵循总规 §2.1/§2.2：TitleBar（40px）/ 图层面板（260px）/
//! MapCanvas / StatusBar（28px），晨山/夜观星双主题（总规 §1.2）。
//! 只做"看"：加载 / 渲染 / 缩放平移 / 主题；查询与分析不进 UI。
//!
//! 用法：
//! - `kanyu-shell`                     打开窗口（空状态引导）
//! - `kanyu-shell --load <file>`       启动即加载数据文件（可多次指定；.kyu 走工程恢复）
//! - `kanyu-shell --screenshot <out.png> [--load <file>]… [--theme dark] [--delay <秒>]`
//!   截图验证模式：启动 → 加载 → 渲染 → 保存窗口截图 → 退出
//!   （走 egui `ViewportCommand::Screenshot` → `Event::Screenshot` 原生管线，
//!   截取的是真实窗口内容，含 TitleBar / 面板 / 画布 / 状态栏）。

// release 构建为纯 GUI 子系统（双击不弹控制台黑窗）；debug 保留控制台便于调试。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod app;
mod attrtable;
mod canvas;
mod catalog;
mod commands;
mod console;
mod dialogs;
mod dock;
mod edit;
mod layoutview;
mod mapview;
mod panels;
mod ribbon;
mod scene3d;
mod scene3d_wgpu;
mod services;
mod settings;
mod symbology;
mod theme;
mod toc;
mod toolbox;
mod ui_kit;
mod uistate;
mod view;

use eframe::egui;
use kanyu_render::Theme;

/// 命令行参数（截图验证模式 + 常规启动）。
pub struct ShellArgs {
    /// 启动即加载的数据文件 / 工程（--load 可多次指定；.kyu 走工程恢复）。
    pub load: Vec<String>,
    /// 初始主题（默认晨山）。
    pub theme: Theme,
    /// 截图输出路径（Some 即进入截图验证模式）。
    pub screenshot: Option<String>,
    /// 截图前等待秒数（等窗口与纹理就绪）。
    pub delay_secs: f64,
    /// 隐藏验证参数：预设「右区停靠 + 浮动窗 + 已关闭」的停靠演示布局。
    pub dock_demo: bool,
    /// 隐藏验证参数：启动即打开设置对话框（截图验证）。
    pub open_settings: bool,
    /// 隐藏验证参数：启动即打开工具箱「缓冲区」参数对话框（截图验证）。
    pub tool_demo: bool,
    /// 隐藏验证参数：启动即打开属性表面板 + 字段计算器对话框（截图验证）。
    pub calc_demo: bool,
    /// 隐藏验证参数：启动即开一个三维「地图 2」浮动视图（截图验证）。
    pub view_demo: bool,
    /// 隐藏验证参数：启动界面缩放档（如 1.25/1.5，截图验证）。
    pub zoom: Option<f32>,
    /// 隐藏验证参数：错位停靠布局（属性表→窄右区、工具箱→宽底区，截图验证回流）。
    pub dock_demo2: bool,
    /// 隐藏验证参数：目录面板深层展开（截图验证滚动）。
    pub catalog_demo: bool,
    /// 隐藏验证参数：打开首图层的属性页（符号化页，截图验证）。
    pub props_demo: bool,
    /// 隐藏验证参数：展开全部图层节点（截图验证符号化分类行）。
    pub expand_layers: bool,
    /// 隐藏验证参数：启动即创建并激活一个布局页签（截图验证）。
    pub layout_demo: bool,
    /// 隐藏验证参数：开启首图层编辑会话（顶点工具，截图验证句柄）。
    pub edit_demo: bool,
    /// 隐藏验证参数：编辑会话「添加面」绘制中态（预置 3 顶点 + 橡皮筋，截图验证）。
    pub draw_demo: bool,
    /// 隐藏验证参数：顶点捕捉指示态（绘制中 + 演示光标贴近既有顶点，截图验证吸附圆环）。
    pub snap_demo: bool,
    /// 隐藏验证参数：编辑会话「分割要素」切割线绘制中态（预置 2 顶点切割线 + 橡皮筋，截图验证）。
    pub split_demo: bool,
    /// 隐藏验证参数：主框三维 + wgpu 后端（真管线截图验证）。
    pub wgpu_demo: bool,
    /// 隐藏验证参数：双三维框同开 wgpu（多视口分键截图验证）。
    pub wgpu_demo2: bool,
    /// 隐藏验证参数：AI 对话预置演示记录（意图面回复态截图验证）。
    pub ai_demo: bool,
    /// 隐藏验证参数：双地图框各自图层集（`load[0]`→主框，`load[1]`→新建三维场景框，截图验证切换联动）。
    pub frames_demo: bool,
    /// 隐藏验证参数：同 frames-demo 但场景框关闭（目录保留弱色行，截图验证关闭≠删除）。
    pub frames_demo2: bool,
    /// 隐藏验证参数：同 frames-demo 但场景框激活（三维场景 + 自有图层集，截图验证三维独立建立）。
    pub frames_demo3: bool,
    /// 隐藏验证参数：布局绑定场景框（激活主框后布局仍显示场景框内容，截图验证）。
    pub layout_bind_demo: bool,
    /// 隐藏验证参数：目录「服务链接」预置一条演示连接并展开分类（截图验证）。
    pub service_demo: bool,
    /// 隐藏验证参数：同 service_demo，另打开「新建服务链接」对话框（截图验证）。
    pub service_dlg_demo: bool,
    /// 隐藏验证参数：打开「编辑服务链接」对话框（WFS 连接回填态，截图验证）。
    pub service_edit_demo: bool,
    /// 隐藏验证参数：启动即打开不动产制图对话框（截图验证）。
    pub estatemap_demo: bool,
}

impl Default for ShellArgs {
    fn default() -> Self {
        Self {
            load: Vec::new(),
            theme: Theme::Light,
            screenshot: None,
            delay_secs: 2.0,
            dock_demo: false,
            open_settings: false,
            tool_demo: false,
            calc_demo: false,
            view_demo: false,
            zoom: None,
            dock_demo2: false,
            catalog_demo: false,
            props_demo: false,
            expand_layers: false,
            layout_demo: false,
            edit_demo: false,
            draw_demo: false,
            snap_demo: false,
            split_demo: false,
            wgpu_demo: false,
            wgpu_demo2: false,
            ai_demo: false,
            frames_demo: false,
            frames_demo2: false,
            frames_demo3: false,
            layout_bind_demo: false,
            service_demo: false,
            service_dlg_demo: false,
            service_edit_demo: false,
            estatemap_demo: false,
        }
    }
}

fn usage() -> String {
    "用法: kanyu-shell [--load <数据文件|工程.kyu>]…（可多次指定） [--theme light|dark] \\\n\
     \x20             [--screenshot <out.png> [--delay <秒>]]\n\
     支持格式: shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/tsv/xlsx（.kyu 为堪舆工程）"
        .to_string()
}

/// 解析命令行；出错打印用法并以码 2 退出。
fn parse_args() -> ShellArgs {
    let mut args = ShellArgs::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--load" => {
                let path = it.next().unwrap_or_else(|| {
                    eprintln!("--load 缺少文件路径\n{}", usage());
                    std::process::exit(2);
                });
                args.load.push(path);
            }
            "--theme" => {
                let v = it.next().unwrap_or_else(|| {
                    eprintln!("--theme 缺少取值（light|dark）\n{}", usage());
                    std::process::exit(2);
                });
                args.theme = v.parse().unwrap_or_else(|e| {
                    eprintln!("{e}\n{}", usage());
                    std::process::exit(2);
                });
            }
            "--screenshot" => {
                args.screenshot = Some(it.next().unwrap_or_else(|| {
                    eprintln!("--screenshot 缺少输出路径\n{}", usage());
                    std::process::exit(2);
                }));
            }
            "--delay" => {
                let v = it.next().unwrap_or_else(|| {
                    eprintln!("--delay 缺少秒数\n{}", usage());
                    std::process::exit(2);
                });
                args.delay_secs = v.parse().unwrap_or_else(|_| {
                    eprintln!("--delay 须为数值秒: '{v}'\n{}", usage());
                    std::process::exit(2);
                });
            }
            "--dock-demo" => args.dock_demo = true,
            "--open-settings" => args.open_settings = true,
            "--tool-demo" => args.tool_demo = true,
            "--calc-demo" => args.calc_demo = true,
            "--view-demo" => args.view_demo = true,
            "--dock-demo2" => args.dock_demo2 = true,
            "--catalog-demo" => args.catalog_demo = true,
            "--props-demo" => args.props_demo = true,
            "--expand-layers" => args.expand_layers = true,
            "--layout-demo" => args.layout_demo = true,
            "--edit-demo" => args.edit_demo = true,
            "--draw-demo" => args.draw_demo = true,
            "--snap-demo" => args.snap_demo = true,
            "--split-demo" => args.split_demo = true,
            "--wgpu-demo" => args.wgpu_demo = true,
            "--wgpu-demo2" => args.wgpu_demo2 = true,
            "--ai-demo" => args.ai_demo = true,
            "--frames-demo" => args.frames_demo = true,
            "--frames-demo2" => args.frames_demo2 = true,
            "--frames-demo3" => args.frames_demo3 = true,
            "--layout-bind-demo" => args.layout_bind_demo = true,
            "--service-demo" => args.service_demo = true,
            "--service-dlg-demo" => args.service_dlg_demo = true,
            "--service-edit-demo" => args.service_edit_demo = true,
            "--estatemap-demo" => args.estatemap_demo = true,
            "--zoom" => {
                let v = it.next().unwrap_or_else(|| {
                    eprintln!("--zoom 缺少倍率（如 1.25）\n{}", usage());
                    std::process::exit(2);
                });
                args.zoom = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!("--zoom 须为数值倍率: '{v}'\n{}", usage());
                    std::process::exit(2);
                }));
            }
            "-h" | "--help" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            "--version" => {
                println!("kanyu-shell {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => {
                eprintln!("未知参数 '{other}'\n{}", usage());
                std::process::exit(2);
            }
        }
    }
    args
}

/// 窗口图标：assets/logo-256.png（编译期嵌入）→ eframe IconData。
/// tiny-skia 解码为预乘 RGBA，winit 图标约定直通 RGBA，逐像素还原。
fn load_icon() -> Option<egui::IconData> {
    let png = include_bytes!("../../../assets/logo-256.png");
    let pixmap = tiny_skia::Pixmap::decode_png(png).ok()?;
    let mut rgba = pixmap.data().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(egui::IconData {
        rgba,
        width: pixmap.width(),
        height: pixmap.height(),
    })
}

fn main() -> eframe::Result<()> {
    let args = parse_args();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("堪舆 Kanyu")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([800.0, 500.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "堪舆 Kanyu",
        options,
        Box::new(move |cc| Ok(Box::new(app::KanyuApp::new(cc, args)) as Box<dyn eframe::App>)),
    )
}
