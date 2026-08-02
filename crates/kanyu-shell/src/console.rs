//! 独立终端面板：ArcGIS Pro Python 窗口式设计理念——命令直达内核，
//! 与界面操作共享同一数据现场（终端产出的图层即刻出现在图层面板）。
//!
//! 命令不走子进程：经 [`ConsoleHost`] 特征直接调用内核，零 I/O 损耗。

use eframe::egui;
use egui::{Color32, FontFamily, FontId, RichText};

/// 终端视图状态（输入框 + 滚动历史 + 历史命令导航）。
pub struct ConsolePanel {
    /// 当前输入。
    pub input: String,
    /// 输出行历史（含命令回显）。
    pub lines: Vec<ConsoleLine>,
    /// 命令历史（↑/↓ 导航），新命令压栈。
    pub cmd_history: Vec<String>,
    /// 历史导航游标（None = 未在导航）。
    nav: Option<usize>,
}

/// 一行终端输出。
pub struct ConsoleLine {
    /// 行类型（决定着色）。
    pub kind: LineKind,
    /// 文本。
    pub text: String,
}

/// 行类型。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    /// 命令回显（`kanyu> ...`）。
    Cmd,
    /// 正常输出。
    Ok,
    /// 错误输出。
    Err,
    /// 系统提示（欢迎语、操作反馈）。
    Info,
}

impl Default for ConsolePanel {
    fn default() -> Self {
        Self {
            input: String::new(),
            lines: vec![ConsoleLine {
                kind: LineKind::Info,
                text: "堪舆终端 v0.2 —— 输入 help 查看命令；终端与界面共享数据现场。".to_string(),
            }],
            cmd_history: Vec::new(),
            nav: None,
        }
    }
}

impl ConsolePanel {
    /// 追加输出。
    pub fn push(&mut self, kind: LineKind, text: impl Into<String>) {
        self.lines.push(ConsoleLine {
            kind,
            text: text.into(),
        });
    }

    /// 追加信息行（界面动作的反馈通道）。
    pub fn info(&mut self, text: impl Into<String>) {
        self.push(LineKind::Info, text);
    }

    /// 执行一条命令并回显。host 为应用宿主（实现见 app.rs）。
    pub fn execute(&mut self, host: &mut dyn ConsoleHost, cmd: &str) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        self.push(LineKind::Cmd, format!("kanyu> {cmd}"));
        self.cmd_history.push(cmd.to_string());
        self.nav = None;
        for line in run_command(host, cmd) {
            self.lines.push(line);
        }
        // 历史上限：防内存膨胀（超出丢弃最旧）。
        const MAX_LINES: usize = 2000;
        if self.lines.len() > MAX_LINES {
            self.lines.drain(..self.lines.len() - MAX_LINES);
        }
    }

    /// 终端面板 UI。返回是否有内容变化（用于自动滚底）。
    pub fn ui(&mut self, ui: &mut egui::Ui, host: &mut dyn ConsoleHost) {
        // 输出区（卡片化由调用方面板提供，此处只画内容）。
        let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let output_height = (ui.available_height() - 34.0).max(text_height * 3.0);
        egui::ScrollArea::vertical()
            .id_salt("console_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .max_height(output_height)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for line in &self.lines {
                    let (prefix, color) = match line.kind {
                        LineKind::Cmd => ("", accent_of(ui)),
                        LineKind::Ok => ("", ui.visuals().text_color()),
                        LineKind::Err => ("", err_color_of(ui)),
                        LineKind::Info => ("", ui.visuals().weak_text_color()),
                    };
                    let _ = prefix;
                    ui.label(
                        RichText::new(&line.text)
                            .font(FontId::new(12.0, FontFamily::Monospace))
                            .color(color),
                    );
                }
            });

        ui.add_space(4.0);
        // 输入区：提示符 + 单行输入，Enter 执行，↑/↓ 历史导航。
        let mut run: Option<String> = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("kanyu>")
                    .font(FontId::new(12.0, FontFamily::Monospace))
                    .color(accent_of(ui)),
            );
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .font(FontId::new(12.0, FontFamily::Monospace))
                    .desired_width(f32::INFINITY)
                    .hint_text("输入命令，Enter 执行，↑↓ 翻历史"),
            );
            // 历史导航。
            if resp.has_focus() {
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    let next = match self.nav {
                        None => self.cmd_history.len().checked_sub(1),
                        Some(0) => Some(0),
                        Some(n) => Some(n - 1),
                    };
                    if let Some(n) = next {
                        self.nav = Some(n);
                        if let Some(h) = self.cmd_history.get(n) {
                            self.input = h.clone();
                        }
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    match self.nav {
                        Some(n) if n + 1 < self.cmd_history.len() => {
                            self.nav = Some(n + 1);
                            self.input = self.cmd_history[n + 1].clone();
                        }
                        _ => {
                            self.nav = None;
                            self.input.clear();
                        }
                    }
                }
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                run = Some(std::mem::take(&mut self.input));
            }
        });
        if let Some(cmd) = run {
            self.execute(host, &cmd);
        }
    }
}

/// 终端命令宿主：由 KanyuApp 实现，命令经此触达应用现场。
pub trait ConsoleHost {
    /// 加载数据文件为图层。
    fn host_load(&mut self, path: &str) -> Result<String, String>;
    /// 图层清单（id / 格式 / 要素数）。
    fn host_layers(&self) -> Vec<(String, String, usize)>;
    /// 图层概要文本。
    fn host_info(&self, id: &str) -> Result<String, String>;
    /// 属性查询：结果存为新图层，返回反馈。
    fn host_query(&mut self, id: &str, expr: &str) -> Result<String, String>;
    /// 缓冲区：结果存为新图层。
    fn host_buffer(&mut self, id: &str, distance: f64) -> Result<String, String>;
    /// 测地线度量（length|area）。
    fn host_measure(&self, id: &str, kind: &str) -> Result<String, String>;
    /// 拓扑检查（no_overlap）。
    fn host_topology(&self, id: &str) -> Result<String, String>;
    /// 投影变换：结果存为新图层。
    fn host_reproject(&mut self, id: &str, from: &str, to: &str) -> Result<String, String>;
    /// 导出图层到文件。
    fn host_export(&self, id: &str, out: &str, fmt: &str) -> Result<String, String>;
    /// 视图缩放到数据范围。
    fn host_fit(&mut self);
    /// 切换主题。
    fn host_toggle_theme(&mut self);
}

/// 命令速查（help 输出，也是 docs 的单一事实来源）。
pub const HELP_TEXT: &str = "命令：
  load <路径>                    加载数据（shp/geojson/fgb/parquet/dxf/dwg/kml/kmz/csv/xlsx）
  layers                         图层清单
  info <图层id>                  图层概要（要素数/几何类型/字段）
  query <图层id> <表达式>        属性查询，结果存为新图层（如 height > 50）
  buffer <图层id> <距离>         缓冲区（CRS 单位，结果存为新图层）
  measure <图层id> length|area   测地线度量（米/平方米）
  topology <图层id>              拓扑检查（no_overlap）
  reproject <图层id> <from> <to> 投影变换（如 EPSG:4326 EPSG:3857，存为新图层）
  export <图层id> <路径> [格式]  导出图层到文件（格式默认按扩展名）
  fit                            视图缩放到数据范围
  theme                          切换晨山/夜观星
  clear                          清空终端历史
  help                           本速查";

/// 命令解析与执行（纯函数式分派，便于测试）。
fn run_command(host: &mut dyn ConsoleHost, cmd: &str) -> Vec<ConsoleLine> {
    let mut out = Vec::new();
    let mut parts = cmd.split_whitespace();
    let Some(name) = parts.next() else { return out };
    let args: Vec<&str> = parts.collect();

    let ok = |text: String| ConsoleLine {
        kind: LineKind::Ok,
        text,
    };
    let err = |text: String| ConsoleLine {
        kind: LineKind::Err,
        text,
    };
    let usage = |u: &str| err(format!("用法: {u}（输入 help 查看全部命令）"));

    match name {
        "help" | "?" => out.push(ok(HELP_TEXT.to_string())),
        "clear" | "cls" => out.push(ConsoleLine {
            kind: LineKind::Info,
            text: "（历史已由界面清空）".to_string(),
        }),
        "load" => match args.as_slice() {
            [path] => match host.host_load(path) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("load <路径>")),
        },
        "layers" => {
            let layers = host.host_layers();
            if layers.is_empty() {
                out.push(ok("（无图层；load <路径> 加载）".to_string()));
            } else {
                for (id, fmt, n) in layers {
                    out.push(ok(format!("{id:<24} {fmt:<10} {n} 要素")));
                }
            }
        }
        "info" => match args.as_slice() {
            [id] => match host.host_info(id) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("info <图层id>")),
        },
        "query" => {
            if args.len() < 2 {
                out.push(usage("query <图层id> <表达式>"));
            } else {
                let expr = args[1..].join(" ");
                match host.host_query(args[0], &expr) {
                    Ok(msg) => out.push(ok(msg)),
                    Err(e) => out.push(err(e)),
                }
            }
        }
        "buffer" => match args.as_slice() {
            [id, dist] => match dist.parse::<f64>() {
                Ok(d) if d.is_finite() && d != 0.0 => match host.host_buffer(id, d) {
                    Ok(msg) => out.push(ok(msg)),
                    Err(e) => out.push(err(e)),
                },
                _ => out.push(err(format!("距离须为非零数值: {dist}"))),
            },
            _ => out.push(usage("buffer <图层id> <距离>")),
        },
        "measure" => match args.as_slice() {
            [id, kind] => match host.host_measure(id, kind) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("measure <图层id> length|area")),
        },
        "topology" => match args.as_slice() {
            [id] => match host.host_topology(id) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("topology <图层id>")),
        },
        "reproject" => match args.as_slice() {
            [id, from, to] => match host.host_reproject(id, from, to) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("reproject <图层id> <from> <to>")),
        },
        "export" => match args.as_slice() {
            [id, out_path] => {
                let fmt = out_path.rsplit('.').next().unwrap_or("").to_string();
                match host.host_export(id, out_path, &fmt) {
                    Ok(msg) => out.push(ok(msg)),
                    Err(e) => out.push(err(e)),
                }
            }
            [id, out_path, fmt] => match host.host_export(id, out_path, fmt) {
                Ok(msg) => out.push(ok(msg)),
                Err(e) => out.push(err(e)),
            },
            _ => out.push(usage("export <图层id> <路径> [格式]")),
        },
        "fit" => {
            host.host_fit();
            out.push(ok("已缩放到数据范围".to_string()));
        }
        "theme" => {
            host.host_toggle_theme();
            out.push(ok("已切换主题".to_string()));
        }
        other => out.push(err(format!("未知命令: {other}（输入 help 查看全部命令）"))),
    }
    out
}

/// 强调色（随主题）。
fn accent_of(ui: &egui::Ui) -> Color32 {
    crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
    .accent
}

/// 错误色（随主题）。
fn err_color_of(ui: &egui::Ui) -> Color32 {
    crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    })
    .accent_secondary
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小宿主替身：记录调用并返回固定结果。
    struct FakeHost {
        loaded: Vec<String>,
    }
    impl ConsoleHost for FakeHost {
        fn host_load(&mut self, path: &str) -> Result<String, String> {
            self.loaded.push(path.to_string());
            Ok(format!("已加载 {path}（4 要素，geojson）"))
        }
        fn host_layers(&self) -> Vec<(String, String, usize)> {
            vec![("buildings".into(), "geojson".into(), 4)]
        }
        fn host_info(&self, id: &str) -> Result<String, String> {
            Ok(format!("图层 {id}: 4 要素"))
        }
        fn host_query(&mut self, _id: &str, _e: &str) -> Result<String, String> {
            Ok("已生成图层 q_buildings（2 要素）".to_string())
        }
        fn host_buffer(&mut self, _id: &str, _d: f64) -> Result<String, String> {
            Ok("已生成图层 buf（4 要素）".to_string())
        }
        fn host_measure(&self, _id: &str, _k: &str) -> Result<String, String> {
            Err("图层无长度".to_string())
        }
        fn host_topology(&self, _id: &str) -> Result<String, String> {
            Ok("0 条违规".to_string())
        }
        fn host_reproject(&mut self, _id: &str, _f: &str, _t: &str) -> Result<String, String> {
            Ok("已生成图层 rp（4 要素）".to_string())
        }
        fn host_export(&self, _id: &str, _o: &str, _f: &str) -> Result<String, String> {
            Ok("已导出 4 要素".to_string())
        }
        fn host_fit(&mut self) {}
        fn host_toggle_theme(&mut self) {}
    }

    #[test]
    fn help_lists_commands() {
        let mut host = FakeHost { loaded: vec![] };
        let out = run_command(&mut host, "help");
        assert!(out[0].text.contains("buffer"));
        assert!(out[0].text.contains("reproject"));
    }

    #[test]
    fn load_roundtrip_and_error() {
        let mut host = FakeHost { loaded: vec![] };
        let out = run_command(&mut host, "load examples/buildings.geojson");
        assert_eq!(host.loaded.len(), 1);
        assert!(out[0].text.contains("已加载"));
        let out = run_command(&mut host, "load");
        assert_eq!(out[0].kind, LineKind::Err);
    }

    #[test]
    fn query_joins_expression_with_spaces() {
        let mut host = FakeHost { loaded: vec![] };
        let out = run_command(&mut host, "query buildings height > 50");
        assert!(out[0].text.contains("2 要素"));
        let out = run_command(&mut host, "measure buildings length");
        assert_eq!(out[0].kind, LineKind::Err);
        let out = run_command(&mut host, "frobnicate");
        assert!(out[0].text.contains("未知命令"));
    }
}
