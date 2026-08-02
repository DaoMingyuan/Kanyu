//! kanyu-shell —— 堪舆的桌面壳层（egui 桌面 UI MVP）。
//!
//! 布局遵循总规 §2.1/§2.2：TitleBar（40px）/ 图层面板（260px）/
//! MapCanvas / StatusBar（28px），晨山/夜观星双主题（总规 §1.2）。
//! 只做"看"：加载 / 渲染 / 缩放平移 / 主题；查询与分析不进 UI。
//!
//! 用法：
//! - `kanyu-shell`                     打开窗口（空状态引导）
//! - `kanyu-shell --load <file>`       启动即加载数据文件
//! - `kanyu-shell --screenshot <out.png> [--load <file>] [--theme dark] [--delay <秒>]`
//!   截图验证模式：启动 → 加载 → 渲染 → 保存窗口截图 → 退出
//!   （走 egui `ViewportCommand::Screenshot` → `Event::Screenshot` 原生管线，
//!   截取的是真实窗口内容，含 TitleBar / 面板 / 画布 / 状态栏）。

// release 构建为纯 GUI 子系统（双击不弹控制台黑窗）；debug 保留控制台便于调试。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod app;
mod canvas;
mod console;
mod dialogs;
mod panels;
mod ribbon;
mod theme;
mod ui_kit;
mod view;

use eframe::egui;
use kanyu_render::Theme;

/// 命令行参数（截图验证模式 + 常规启动）。
pub struct ShellArgs {
    /// 启动即加载的数据文件。
    pub load: Option<String>,
    /// 初始主题（默认晨山）。
    pub theme: Theme,
    /// 截图输出路径（Some 即进入截图验证模式）。
    pub screenshot: Option<String>,
    /// 截图前等待秒数（等窗口与纹理就绪）。
    pub delay_secs: f64,
}

impl Default for ShellArgs {
    fn default() -> Self {
        Self {
            load: None,
            theme: Theme::Light,
            screenshot: None,
            delay_secs: 2.0,
        }
    }
}

fn usage() -> String {
    "用法: kanyu-shell [--load <数据文件>] [--theme light|dark] \\\n\
     \x20             [--screenshot <out.png> [--delay <秒>]]\n\
     支持格式: shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/tsv/xlsx"
        .to_string()
}

/// 解析命令行；出错打印用法并以码 2 退出。
fn parse_args() -> ShellArgs {
    let mut args = ShellArgs::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--load" => {
                args.load = Some(it.next().unwrap_or_else(|| {
                    eprintln!("--load 缺少文件路径\n{}", usage());
                    std::process::exit(2);
                }));
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
