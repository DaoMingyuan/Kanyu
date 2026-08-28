//! 宗地图渲染器 —— GB/T 42547-2023《地籍调查规程》图 L.3（土地使用权宗地图）版式。
//!
//! 页面（A4，竖/横自动）：
//! - 顶部居中标题「宗 地 图」+ 右上「单位：m㎡」；
//! - 头部信息框：宗地代码 / 所在图幅号 / 宗地面积 / 土地权利人；
//! - 地图框：界址线 0.3mm 红、界址点 Ø2.0mm 黑圆圈（圆心 0.2mm）、
//!   J 点号注记与边长注记（位置由 `kanyu_core::cartography` 勘测定界图注记契约计算）、
//!   宗地中央宗地号/地类编码分式（中等线 3.0mm）；
//! - 右侧界址点坐标表（点号 | X | Y | 边长；白底黑线 0.15mm；**X=纵坐标（北）、Y=横坐标（东）**
//!   测绘惯例；末行重复起点坐标闭合；边长挂接起点行；表尾宗地面积行）；
//! - 图框外下中比例尺 1:N（分母取整百）；右上指北针；
//! - 左下「XXXX年XX月解析法测绘界址点」+ 制图/审核日期；右下制图者/审核者；
//! - 左侧竖排单位名。
//!
//! 比例尺缺省自动求解：宗地 bbox 适配地图框后分母向上取整百。
//! 坐标表长表自动折列（right_to_left 列流，开发计划 §6/勘测定界图坐标表契约）。
//! SVG 全量（文字/旋转齐全）；PNG 经 `layout::TextBackend` 系统字体栈
//! （旋转注记走离屏 pixmap 旋转合成，与 SVG rotate() 同角）。

use crate::layout::{PageSize, TextBackend};
use crate::RenderError;
use kanyu_core::cartography::{
    self, BoundaryLineRecord, BoundaryPointRecord, ParcelBoundary, PlacementReport, RealestateRing,
};

/// 平面点/向量（地图单位）。
type P2 = (f64, f64);

/// 线段裁剪到矩形（Liang-Barsky；返回可见段两端，全在框外为 None）。
fn clip_segment_to_rect(a: P2, b: P2, rect: [f64; 4]) -> Option<(P2, P2)> {
    let (rx, ry, rw, rh) = (rect[0], rect[1], rect[2], rect[3]);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.0 - rx),     // 左界
        (dx, rx + rw - a.0), // 右界
        (-dy, a.1 - ry),     // 上界
        (dy, ry + rh - a.1), // 下界
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None; // 平行且在外侧
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return None;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }
    Some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
    ))
}

/// 道路线要素提取（CLI/MCP 共用）：LineString/MultiLineString 要素 → [`RoadLine`]；
/// 路名按候选键拾取（[`cartography::feature_prop_str`]），空串仅绘线。
pub fn roads_from_collection(
    collection: &geojson::FeatureCollection,
    name_keys: &[&str],
) -> Vec<RoadLine> {
    let mut roads = Vec::new();
    for feature in &collection.features {
        let Some(geom) = &feature.geometry else {
            continue;
        };
        let name = feature
            .properties
            .as_ref()
            .and_then(|p| cartography::feature_prop_str(p, name_keys))
            .unwrap_or_default();
        let push_line = |roads: &mut Vec<RoadLine>, line: &[Vec<f64>]| {
            if line.len() >= 2 {
                roads.push(RoadLine {
                    path: line.iter().map(|p| (p[0], p[1])).collect(),
                    name: name.clone(),
                });
            }
        };
        match &geom.value {
            geojson::Value::LineString(line) => push_line(&mut roads, line),
            geojson::Value::MultiLineString(lines) => {
                for line in lines {
                    push_line(&mut roads, line);
                }
            }
            _ => {}
        }
    }
    roads
}

/// 宗地图出图参数。
#[derive(Debug, Clone)]
pub struct ParcelMapSpec {
    /// 宗地代码（完整 19 位或任意长度；分式分子取末 7 位）。
    pub parcel_code: String,
    /// 土地权利人。
    pub owner: String,
    /// 所在图幅号。
    pub map_sheet: String,
    /// 宗地面积（㎡；None 按几何现算）。
    pub area_sqm: Option<f64>,
    /// 地类编码（分式分母）。
    pub land_use: String,
    /// 左侧竖排单位名（如 XXX自然资源局）。
    pub unit_name: String,
    /// 测绘说明（左下，如「2026年08月解析法测绘界址点」）。
    pub survey_note: String,
    /// 制图者 / 审核者。
    pub drawer: String,
    /// 审核者。
    pub reviewer: String,
    /// 制图日期 / 审核日期。
    pub draw_date: String,
    /// 审核日期。
    pub review_date: String,
    /// 比例尺分母（None 自动取整百）。
    pub scale: Option<u32>,
    /// 分辨率（默认 150）。
    pub dpi: f64,
    /// 界址点号前缀（默认 J）。
    pub point_prefix: String,
    /// 四至注记（东/南/西/北，即邻宗地注记；空串不绘；`\n` 分行，
    /// 如「山东省长途电信淄博传输局\nGB00029」；不动产登记数据库标准
    /// ZDJBXX 四至字段 ZDSZD/ZDSZN/ZDSZX/ZDSZB）。
    pub sizhi_e: String,
    /// 南至注记。
    pub sizhi_s: String,
    /// 西至注记。
    pub sizhi_w: String,
    /// 北至注记。
    pub sizhi_n: String,
    /// 相邻道路（线 + 路名；图 L.3 样图「南大街」要素；0.15mm 黑线按地图框
    /// 裁剪，路名沿可见段中点、角度沿线（字头向北允许向西）；空表不绘）。
    pub roads: Vec<RoadLine>,
}

/// 相邻道路（线要素 + 路名）。
#[derive(Debug, Clone)]
pub struct RoadLine {
    /// 线坐标串（地图单位）。
    pub path: Vec<P2>,
    /// 路名（如「南大街」；空串仅绘线）。
    pub name: String,
}

impl Default for ParcelMapSpec {
    fn default() -> Self {
        Self {
            parcel_code: String::new(),
            owner: String::new(),
            map_sheet: String::new(),
            area_sqm: None,
            land_use: String::new(),
            unit_name: String::new(),
            survey_note: String::new(),
            drawer: String::new(),
            reviewer: String::new(),
            draw_date: String::new(),
            review_date: String::new(),
            scale: None,
            dpi: 150.0,
            point_prefix: "J".to_string(),
            sizhi_e: String::new(),
            sizhi_s: String::new(),
            sizhi_w: String::new(),
            sizhi_n: String::new(),
            roads: Vec::new(),
        }
    }
}

/// 出图结果（含实际比例尺与排版诊断）。
#[derive(Debug, Clone)]
pub struct ParcelMapOutput {
    /// 实际采用的比例尺分母。
    pub scale: u32,
    /// 排版诊断行（每条注记一行：文本/途径/净空/残余压盖）。
    pub diagnostics: Vec<String>,
    /// SVG 或 PNG 产物。
    pub data: ParcelMapData,
}

/// 产物载体。
#[derive(Debug, Clone)]
pub enum ParcelMapData {
    /// SVG 文本。
    Svg(String),
    /// PNG 字节。
    Png(Vec<u8>),
}

/// 渲染宗地图为 SVG（完整排版）。
pub fn render_parcel_map_svg(
    boundary: &ParcelBoundary,
    spec: &ParcelMapSpec,
) -> Result<ParcelMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    Ok(ParcelMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Svg(scene_to_svg(&scene.prims)),
    })
}

/// 渲染宗地图为 PNG（tiny-skia 光栅链 + TextBackend 系统字体栈）。
pub fn render_parcel_map_png(
    boundary: &ParcelBoundary,
    spec: &ParcelMapSpec,
) -> Result<ParcelMapOutput, RenderError> {
    let scene = build_scene(boundary, spec)?;
    let png = scene_to_png(&scene.prims, spec.dpi, &TextBackend::system())?;
    Ok(ParcelMapOutput {
        scale: scene.scale,
        diagnostics: scene.diagnostics,
        data: ParcelMapData::Png(png),
    })
}

// ---------------------------------------------------------------------------
// 版式常量（毫米；A4 竖 210×297，图 L.3 首版固定竖向）
// ---------------------------------------------------------------------------

/// 页宽 / 页高。
const PAGE_W: f64 = 210.0;
const PAGE_H: f64 = 297.0;
/// 外图廓（0.5mm）/ 内图廓（0.2mm）。
const OUTER_RECT: [f64; 4] = [8.0, 8.0, 194.0, 281.0];
const INNER_RECT: [f64; 4] = [11.0, 11.0, 188.0, 275.0];
/// 标题基线 y 与字号（图名大号粗体）。
const TITLE_Y: f64 = 22.0;
const TITLE_FONT: f64 = 6.5;
/// 右上「单位：m㎡」。
const UNIT_NOTE_Y: f64 = 16.5;
const UNIT_NOTE_FONT: f64 = 2.6;
/// 头部信息框纵向区间与四栏分界（宗地代码 | 所在图幅号 | 宗地面积 | 土地权利人）。
/// 栏宽按内容估算分配：代码 53 / 图幅号 34 / 面积 36 / 权利人 65（毫米）。
const HEADER_Y0: f64 = 25.0;
const HEADER_Y1: f64 = 36.0;
const HEADER_COLS: [f64; 5] = [11.0, 64.0, 98.0, 134.0, 199.0];
/// 地图框（主体；即内图廓 y 36→272 段，底为签注带）。
const MAP_RECT: [f64; 4] = [11.0, 36.0, 188.0, 236.0];
/// 宗地适配留白（地图框内沿）。
const MAP_PAD: f64 = 10.0;
/// 底部签注带上沿。
const FOOT_Y0: f64 = 272.0;
/// 指北针（地图框内右上）中心 x / 三角顶 y。
const NORTH_X: f64 = 190.0;
const NORTH_TOP_Y: f64 = 42.0;
/// 坐标表字号 / 题行字号 / 行高 / 题行高 / 单元格横向留白 / 锚定内边距。
const TABLE_FONT: f64 = 2.2;
const TABLE_TITLE_FONT: f64 = 3.2;
const TABLE_ROW_H: f64 = 4.2;
const TABLE_TITLE_H: f64 = 5.0;
const TABLE_CELL_PAD: f64 = 1.5;
const TABLE_ANCHOR_PAD: f64 = 3.0;
/// 坐标表最大高度（地图框内高扣除上下锚定边距；超出触发长表折列）。
const TABLE_MAX_H: f64 = MAP_RECT[3] - 2.0 * TABLE_ANCHOR_PAD;
/// 折列列间间隔（对齐勘测定界图坐标表 column_gap_mm=4.0 契约）。
const TABLE_COL_GAP: f64 = 4.0;
/// 坐标表与宗地适配区间的竖向间隔。
const TABLE_FIT_GAP: f64 = 4.0;
/// 适配区最小高（点极多时兜底）。
const FIT_MIN_H: f64 = 40.0;
/// 颜色（黑 / 界址线红 / 白）。
const BLACK: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];

// ---------------------------------------------------------------------------
// 场景图元（毫米纸面坐标；SVG/PNG 双后端共用一份几何）
// ---------------------------------------------------------------------------

/// 描边（毫米线宽 + RGB）。
#[derive(Debug, Clone, Copy)]
struct Stroke {
    width: f64,
    color: [u8; 3],
}

/// 文本锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Start,
    Middle,
    End,
}

/// 场景图元。
#[derive(Debug, Clone)]
enum Prim {
    /// 矩形（图廓 / 表格外框）。
    Rect {
        rect: [f64; 4],
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 折线 / 多边形（界址线、表格线、分数线、指北针三角）。
    Path {
        pts: Vec<(f64, f64)>,
        close: bool,
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 圆（界址点符号 / 圆心点）。
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Option<[u8; 3]>,
        stroke: Option<Stroke>,
    },
    /// 文本（vcenter 时 y 为竖向中心，否则 y 为基线）。
    Text {
        x: f64,
        y: f64,
        font: f64,
        text: String,
        anchor: Anchor,
        rotate_deg: f64,
        vcenter: bool,
        bold: bool,
    },
}

/// 排版场景（图元 + 实际比例尺 + 诊断）。
struct Scene {
    prims: Vec<Prim>,
    scale: u32,
    diagnostics: Vec<String>,
}

// ---------------------------------------------------------------------------
// 界址点坐标表（开发计划 §6：点号 | X | Y | 边长；X=纵坐标（北）、Y=横坐标（东））
// ---------------------------------------------------------------------------

/// 坐标表模型。
struct CoordTable {
    /// 表头。
    headers: [String; 4],
    /// 数据行（末行为重复 1 号点坐标的闭合行，无边长）。
    rows: Vec<[String; 4]>,
    /// 表尾面积行标签（跨前三栏）。
    area_label: String,
    /// 面积值（2 位小数）。
    area_value: String,
    /// 栏宽（毫米，含两侧留白）。
    col_w: [f64; 4],
}

impl CoordTable {
    /// 组表：按点号升序逐行；边长挂接起点行（start_no == point_no）；末行重复 1 号点闭合。
    fn build(points: &[BoundaryPointRecord], lines: &[BoundaryLineRecord], area: f64) -> Self {
        let mut rows: Vec<[String; 4]> = points
            .iter()
            .map(|p| {
                let edge = lines
                    .iter()
                    .find(|l| l.start_no == p.point_no)
                    .map(|l| cartography::format_edge_length(l.length_m))
                    .unwrap_or_default();
                [
                    p.label(),
                    format!("{:.3}", p.y), // X = 纵坐标（北）
                    format!("{:.3}", p.x), // Y = 横坐标（东）
                    edge,
                ]
            })
            .collect();
        // 闭合行：重复 1 号点坐标（点号仍写 1 号点号），无边长
        if let Some(first) = points.first() {
            rows.push([
                first.label(),
                format!("{:.3}", first.y),
                format!("{:.3}", first.x),
                String::new(),
            ]);
        }
        let headers = [
            "点号".to_string(),
            "X坐标(m)".to_string(),
            "Y坐标(m)".to_string(),
            "边长(m)".to_string(),
        ];
        let area_label = "宗地面积(平方米)".to_string();
        let area_value = format!("{area:.2}");
        // 栏宽：内容估算宽 + 两侧留白（点号栏保底）
        let mut col_w = [0.0_f64; 4];
        for (c, w) in col_w.iter_mut().enumerate() {
            let mut width = cartography::text_extent_mm(&headers[c], TABLE_FONT).0;
            for row in &rows {
                width = width.max(cartography::text_extent_mm(&row[c], TABLE_FONT).0);
            }
            *w = width + TABLE_CELL_PAD * 2.0;
        }
        col_w[0] = col_w[0].max(9.0);
        col_w[3] = col_w[3]
            .max(cartography::text_extent_mm(&area_value, TABLE_FONT).0 + TABLE_CELL_PAD * 2.0);
        // 面积行标签跨前三栏：不足时补到第 3 栏
        let label_w = cartography::text_extent_mm(&area_label, TABLE_FONT).0 + TABLE_CELL_PAD * 2.0;
        let first3: f64 = col_w[..3].iter().sum();
        if first3 < label_w {
            col_w[2] += label_w - first3;
        }
        Self {
            headers,
            rows,
            area_label,
            area_value,
            col_w,
        }
    }

    /// 表总宽（毫米）。
    fn width(&self) -> f64 {
        self.col_w.iter().sum()
    }

    /// 表总行数（题行 + 表头 + 数据/闭合行 + 面积行）。
    fn row_count(&self) -> usize {
        self.rows.len() + 3
    }

    /// 表总高（毫米，未折列时）。
    fn height(&self) -> f64 {
        TABLE_TITLE_H + TABLE_ROW_H * (self.row_count() - 1) as f64
    }

    /// 折列布局（开发计划 §6：长表自动折列、right_to_left 列流）。
    /// 返回每列数据行数（首列=最右列含题行，末列=最左列含面积行）；
    /// 未超 [`TABLE_MAX_H`] 时单列（含全部数据行）。
    fn block_rows(&self) -> Vec<usize> {
        if self.height() <= TABLE_MAX_H {
            return vec![self.rows.len()];
        }
        // 首列容量（题行+表头扣除）；后续列容量（仅表头扣除）。
        let cap_first =
            (((TABLE_MAX_H - TABLE_TITLE_H - TABLE_ROW_H) / TABLE_ROW_H).floor() as usize).max(2);
        let cap_next = (((TABLE_MAX_H - TABLE_ROW_H) / TABLE_ROW_H).floor() as usize).max(2);
        let mut chunks = Vec::new();
        let mut idx = 0;
        let total = self.rows.len();
        let mut cap = cap_first;
        while idx < total {
            let remaining = total - idx;
            // 末列须余 1 行给面积行：remaining == cap 时少取一行，保证面积行落点。
            let take = if remaining < cap {
                remaining
            } else if remaining == cap {
                cap - 1
            } else {
                cap
            };
            let take = take.max(1);
            chunks.push(take);
            idx += take;
            cap = cap_next;
        }
        chunks
    }

    /// 折列后实际展示高度（各列中最高的列；参与适配区/比例尺求解）。
    fn display_height(&self) -> f64 {
        let chunks = self.block_rows();
        let n = chunks.len();
        chunks
            .iter()
            .enumerate()
            .map(|(b, &take)| {
                let title = usize::from(b == 0);
                let area = usize::from(b == n - 1);
                // 题行高×题行数 + 行高×（表头+数据+面积行数），与 emit 的块高一致
                TABLE_TITLE_H * title as f64 + TABLE_ROW_H * (1 + take + area) as f64
            })
            .fold(0.0_f64, f64::max)
    }

    /// 出图元：白底黑线 0.15mm，锚定地图框右下角；长表折列（右起列流，
    /// 题行仅首列、表头每列重复、面积行仅末列——勘测定界图坐标表契约）。
    fn emit(&self, prims: &mut Vec<Prim>) {
        let x1 = MAP_RECT[0] + MAP_RECT[2] - TABLE_ANCHOR_PAD;
        let y1 = MAP_RECT[1] + MAP_RECT[3] - TABLE_ANCHOR_PAD;
        let chunks = self.block_rows();
        let n_blocks = chunks.len();
        let thin = Some(Stroke {
            width: 0.15,
            color: BLACK,
        });
        let mut start = 0usize;
        for (b, &take) in chunks.iter().enumerate() {
            let first_col = b == 0;
            let last_col = b == n_blocks - 1;
            let title_rows = usize::from(first_col);
            let area_rows = usize::from(last_col);
            let n = title_rows + 1 + take + area_rows; // 表头恒 1 行
            let hb = TABLE_TITLE_H * title_rows as f64 + TABLE_ROW_H * (n - title_rows) as f64;
            let x1_b = x1 - b as f64 * (self.width() + TABLE_COL_GAP);
            let x0_b = x1_b - self.width();
            let y0_b = y1 - hb;
            prims.push(Prim::Rect {
                rect: [x0_b, y0_b, self.width(), hb],
                fill: Some(WHITE),
                stroke: thin,
            });
            // 栏分界 x（左起累计）
            let mut col_x = [0.0_f64; 5];
            col_x[0] = x0_b;
            for c in 0..4 {
                col_x[c + 1] = col_x[c] + self.col_w[c];
            }
            // 行界 y（第 r 行上沿）：首列 r=1 为题行界（题行高），其后行高均等
            let row_y = |r: usize| {
                if r == 0 {
                    y0_b
                } else if first_col {
                    y0_b + TABLE_TITLE_H + (r - 1) as f64 * TABLE_ROW_H
                } else {
                    y0_b + r as f64 * TABLE_ROW_H
                }
            };
            // 横线（逐行分界）
            for r in 1..n {
                let y = row_y(r);
                prims.push(Prim::Path {
                    pts: vec![(x0_b, y), (x1_b, y)],
                    close: false,
                    fill: None,
                    stroke: thin,
                });
            }
            // 竖线：表头行起；面积行仅值栏分界（前三栏通栏）
            let head_y = row_y(title_rows);
            let area_y = if last_col { row_y(n - 1) } else { y1 };
            for (i, &x) in col_x[1..4].iter().enumerate() {
                let yb = if i == 2 { y1 } else { area_y };
                prims.push(Prim::Path {
                    pts: vec![(x, head_y), (x, yb)],
                    close: false,
                    fill: None,
                    stroke: thin,
                });
            }
            // 单元格文本（通栏居中）
            let cell =
                |prims: &mut Vec<Prim>, text: &str, xa: f64, xb: f64, r: usize, font: f64| {
                    prims.push(Prim::Text {
                        x: (xa + xb) / 2.0,
                        y: (row_y(r) + row_y(r + 1)) / 2.0,
                        font,
                        text: text.to_string(),
                        anchor: Anchor::Middle,
                        rotate_deg: 0.0,
                        vcenter: true,
                        bold: false,
                    });
                };
            if first_col {
                cell(prims, "界址点坐标表", x0_b, x1_b, 0, TABLE_TITLE_FONT);
            }
            for c in 0..4 {
                cell(
                    prims,
                    &self.headers[c],
                    col_x[c],
                    col_x[c + 1],
                    title_rows,
                    TABLE_FONT,
                );
            }
            for (r, row) in self.rows[start..start + take].iter().enumerate() {
                for c in 0..4 {
                    cell(
                        prims,
                        &row[c],
                        col_x[c],
                        col_x[c + 1],
                        title_rows + 1 + r,
                        TABLE_FONT,
                    );
                }
            }
            if last_col {
                cell(prims, &self.area_label, x0_b, col_x[3], n - 1, TABLE_FONT);
                cell(prims, &self.area_value, col_x[3], x1_b, n - 1, TABLE_FONT);
            }
            start += take;
        }
    }
}

// ---------------------------------------------------------------------------
// 场景构建
// ---------------------------------------------------------------------------

/// 比例尺分母向上取整百（786→800；下限 100）。
fn round_up_hundred(raw: f64) -> u32 {
    if !raw.is_finite() || raw <= 100.0 {
        return 100;
    }
    (raw / 100.0).ceil() as u32 * 100
}

/// 环 bbox（min_x, min_y, max_x, max_y）。
fn ring_bbox(ring: &RealestateRing) -> (f64, f64, f64, f64) {
    ring.points.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(a, b, c, d), &(x, y)| (a.min(x), b.min(y), c.max(x), d.max(y)),
    )
}

/// 外环质心（面积加权；退化时回退 bbox 中心）。
fn ring_centroid(ring: &RealestateRing) -> (f64, f64) {
    let (mut cross_sum, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for w in ring.points.windows(2) {
        let cross = w[0].0 * w[1].1 - w[1].0 * w[0].1;
        cross_sum += cross;
        cx += (w[0].0 + w[1].0) * cross;
        cy += (w[0].1 + w[1].1) * cross;
    }
    if cross_sum.abs() < 1e-12 {
        let (min_x, min_y, max_x, max_y) = ring_bbox(ring);
        return ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    }
    (cx / (3.0 * cross_sum), cy / (3.0 * cross_sum))
}

/// 四至注记方位边选取：外环上外法线与方位 `dir` 点积 > 0.5（夹角 <60°）
/// 的候选边中**取最长边**（主方位边，避免短边/齿边吸收注记）；
/// 无候选时回退点积最大者。返回（边中点, 外法线, 边方向单位向量）。
/// 外法线按环走向判定（[`cartography::ring_edge_outward_normal`]，
/// 锯齿/凹角宗地同样正确）。
fn sizhi_side(ring: &RealestateRing, dir: P2) -> Option<(P2, P2, P2)> {
    let mut fallback: Option<(f64, P2, P2, P2)> = None;
    let mut best: Option<(f64, P2, P2, P2)> = None;
    for (a, b) in ring.segments() {
        let n = cartography::ring_edge_outward_normal(ring, a, b);
        let dot = n.0 * dir.0 + n.1 * dir.1;
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let norm = if len == 0.0 { 1.0 } else { len };
        let tv = ((b.0 - a.0) / norm, (b.1 - a.1) / norm);
        if fallback.map(|(d, _, _, _)| dot > d).unwrap_or(true) {
            fallback = Some((dot, mid, n, tv));
        }
        if dot > 0.5 && best.map(|(l, _, _, _)| len > l).unwrap_or(true) {
            best = Some((len, mid, n, tv));
        }
    }
    best.or(fallback).map(|(_, m, n, tv)| (m, n, tv))
}

/// 排版诊断汇总（每条注记一行：text reason=… clearance=…mm overlap=…）。
fn diagnostics_of(reports: &[&PlacementReport]) -> Vec<String> {
    let mut out = Vec::new();
    for report in reports {
        for l in &report.labels {
            out.push(format!(
                "{} reason={} clearance={:.2}mm overlap={}",
                l.text, l.reason, l.clearance_mm, l.overlap
            ));
        }
    }
    out
}

/// 组场景：版面几何 + 注记排版 + 诊断（SVG/PNG 共用）。
fn build_scene(boundary: &ParcelBoundary, spec: &ParcelMapSpec) -> Result<Scene, RenderError> {
    let points = cartography::generate_boundary_points(boundary, &spec.point_prefix);
    if points.is_empty() {
        return Err(RenderError::InvalidStyle("宗地几何无界址点".to_string()));
    }
    let lines = cartography::generate_boundary_lines(boundary, &points);
    // 宗地面积：缺省按鞋带面积现算（外环 − 内环）
    let area = spec.area_sqm.unwrap_or_else(|| {
        let ext = cartography::ring_area(&boundary.exterior).abs();
        let holes: f64 = boundary
            .interiors
            .iter()
            .map(|r| cartography::ring_area(r).abs())
            .sum();
        (ext - holes).max(0.0)
    });
    // 坐标表（先组表：表高参与适配区与自动比例尺求解；长表按折列后展示高度）
    let table = CoordTable::build(&points, &lines, area);
    // 宗地适配区：地图框扣除留白与底部坐标表带
    let fit_w = MAP_RECT[2] - 2.0 * MAP_PAD;
    let fit_h =
        (MAP_RECT[3] - 2.0 * MAP_PAD - table.display_height() - TABLE_FIT_GAP).max(FIT_MIN_H);
    let (min_x, min_y, max_x, max_y) = ring_bbox(&boundary.exterior);
    let span_x = (max_x - min_x).max(1e-6);
    let span_y = (max_y - min_y).max(1e-6);
    // 比例尺：spec 给定直接用；缺省取宽/高方向较大 raw 分母向上整百
    let scale = match spec.scale {
        Some(s) => s,
        None => round_up_hundred((span_x * 1000.0 / fit_w).max(span_y * 1000.0 / fit_h)),
    };
    // 地图单位（米）→ 纸面毫米：mu = 1000 / scale；bbox 中心对适配区中心，北朝上
    let mu = 1000.0 / f64::from(scale);
    let fit_cx = MAP_RECT[0] + MAP_PAD + fit_w / 2.0;
    let fit_cy = MAP_RECT[1] + MAP_PAD + fit_h / 2.0;
    let bbox_c = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let to_page = |(x, y): (f64, f64)| (fit_cx + (x - bbox_c.0) * mu, fit_cy - (y - bbox_c.1) * mu);

    // 注记排版（点号先排，其矩形作为边长注记的附加障碍）
    let point_report = cartography::place_point_labels(
        boundary,
        &points,
        &cartography::PointLabelOptions {
            scale,
            ..Default::default()
        },
        &[],
    );
    let point_rects: Vec<_> = point_report.labels.iter().map(|l| l.rect).collect();
    let edge_report = cartography::place_edge_labels(
        boundary,
        &lines,
        &points,
        &cartography::EdgeLabelOptions {
            scale,
            ..Default::default()
        },
        &point_rects,
    );
    let diagnostics = diagnostics_of(&[&point_report, &edge_report]);

    let thin = Some(Stroke {
        width: 0.15,
        color: BLACK,
    });
    let mut prims: Vec<Prim> = Vec::with_capacity(128);
    // —— 图廓（外粗内细）+ 头部/签注带分隔线 ——
    prims.push(Prim::Rect {
        rect: OUTER_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.5,
            color: BLACK,
        }),
    });
    prims.push(Prim::Rect {
        rect: INNER_RECT,
        fill: None,
        stroke: Some(Stroke {
            width: 0.2,
            color: BLACK,
        }),
    });
    for y in [HEADER_Y1, FOOT_Y0] {
        prims.push(Prim::Path {
            pts: vec![(INNER_RECT[0], y), (INNER_RECT[0] + INNER_RECT[2], y)],
            close: false,
            fill: None,
            stroke: thin,
        });
    }
    // —— 标题（顶部居中）+ 右上「单位：m㎡」——
    prims.push(Prim::Text {
        x: PAGE_W / 2.0,
        y: TITLE_Y,
        font: TITLE_FONT,
        text: "宗 地 图".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: false,
        bold: true,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 2.0,
        y: UNIT_NOTE_Y,
        font: UNIT_NOTE_FONT,
        text: "单位：m㎡".to_string(),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 头部信息框（面积一栏按金样加大字号）——
    let header_texts = [
        format!("宗地代码：{}", spec.parcel_code),
        format!("所在图幅号：{}", spec.map_sheet),
        format!("宗地面积：{area:.2}"),
        format!("土地权利人：{}", spec.owner),
    ];
    for (c, text) in header_texts.iter().enumerate() {
        prims.push(Prim::Text {
            x: (HEADER_COLS[c] + HEADER_COLS[c + 1]) / 2.0,
            y: (HEADER_Y0 + HEADER_Y1) / 2.0,
            font: if c == 2 { 3.4 } else { 2.7 },
            text: text.clone(),
            anchor: Anchor::Middle,
            rotate_deg: 0.0,
            vcenter: true,
            bold: false,
        });
    }
    // —— 指北针（地图框内右上：「北」字 + 细长三角，对齐 GB/T 42547 图 L.3 样图）——
    prims.push(Prim::Text {
        x: NORTH_X,
        y: NORTH_TOP_Y + 1.6,
        font: 3.4,
        text: "北".to_string(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Path {
        pts: vec![
            (NORTH_X, NORTH_TOP_Y + 4.0),
            (NORTH_X - 1.4, NORTH_TOP_Y + 14.0),
            (NORTH_X, NORTH_TOP_Y + 12.0),
            (NORTH_X + 1.4, NORTH_TOP_Y + 14.0),
        ],
        close: true,
        fill: Some(BLACK),
        stroke: None,
    });
    // —— 相邻道路（0.15mm 黑线按地图框裁剪，绘于界址线下层；
    // 路名沿最长可见段中点、角度沿线（字头向北允许向西），可见段过短仅绘线）——
    for road in &spec.roads {
        let page_pts: Vec<P2> = road.path.iter().map(|&p| to_page(p)).collect();
        let mut pieces: Vec<(P2, P2)> = Vec::new();
        for w in page_pts.windows(2) {
            if let Some(seg) = clip_segment_to_rect(w[0], w[1], MAP_RECT) {
                pieces.push(seg);
                prims.push(Prim::Path {
                    pts: vec![seg.0, seg.1],
                    close: false,
                    fill: None,
                    stroke: Some(Stroke {
                        width: 0.15,
                        color: BLACK,
                    }),
                });
            }
        }
        if road.name.trim().is_empty() {
            continue;
        }
        let Some((p0, p1)) = pieces.iter().max_by(|a, b| {
            let la = (a.1 .0 - a.0 .0).hypot(a.1 .1 - a.0 .1);
            let lb = (b.1 .0 - b.0 .0).hypot(b.1 .1 - b.0 .1);
            la.partial_cmp(&lb).unwrap_or(std::cmp::Ordering::Equal)
        }) else {
            continue;
        };
        let seg_len = (p1.0 - p0.0).hypot(p1.1 - p0.1);
        let name_w = cartography::text_extent_mm(road.name.trim(), 2.8).0 + 1.0;
        if seg_len < name_w {
            continue; // 可见段太短：仅绘线，注记让位（诚实不压）
        }
        let angle = (p1.1 - p0.1).atan2(p1.0 - p0.0).to_degrees();
        prims.push(Prim::Text {
            x: (p0.0 + p1.0) / 2.0,
            y: (p0.1 + p1.1) / 2.0,
            font: 2.8,
            text: road.name.trim().to_string(),
            anchor: Anchor::Middle,
            rotate_deg: cartography::upright_rotation(angle),
            vcenter: true,
            bold: false,
        });
    }
    // —— 界址线（0.3mm 红）——
    for ring in boundary.rings() {
        prims.push(Prim::Path {
            pts: ring.points.iter().map(|&p| to_page(p)).collect(),
            close: true,
            fill: None,
            stroke: Some(Stroke {
                width: 0.3,
                color: RED,
            }),
        });
    }
    // —— 界址点（Ø2.0mm 黑圆圈 + Ø0.2mm 圆心点，线粗 0.15mm）——
    for p in &points {
        let (x, y) = to_page((p.x, p.y));
        prims.push(Prim::Circle {
            cx: x,
            cy: y,
            r: 1.0,
            fill: Some(WHITE),
            stroke: thin,
        });
        prims.push(Prim::Circle {
            cx: x,
            cy: y,
            r: 0.1,
            fill: Some(BLACK),
            stroke: None,
        });
    }
    // —— J 点号注记 / 边长注记（2.4mm，位置与旋转按 cartography 排版结果）——
    for l in point_report.labels.iter().chain(edge_report.labels.iter()) {
        let (x, y) = to_page((l.rect.cx, l.rect.cy));
        prims.push(Prim::Text {
            x,
            y,
            font: 2.4,
            text: l.text.clone(),
            anchor: Anchor::Middle,
            rotate_deg: l.rotation_deg,
            vcenter: true,
            bold: false,
        });
    }
    // —— 宗地中央分式（分子=宗地代码末 7 位 / 分母=地类编码，3.0mm 中等线，分数线横贯）——
    let code_tail: String = {
        let chars: Vec<char> = spec.parcel_code.chars().collect();
        chars.iter().skip(chars.len().saturating_sub(7)).collect()
    };
    let (gcx, gcy) = to_page(ring_centroid(&boundary.exterior));
    let half = (cartography::text_extent_mm(&code_tail, 3.0)
        .0
        .max(cartography::text_extent_mm(&spec.land_use, 3.0).0))
        / 2.0
        + 1.0;
    prims.push(Prim::Text {
        x: gcx,
        y: gcy - 2.3,
        font: 3.0,
        text: code_tail,
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Path {
        pts: vec![(gcx - half, gcy), (gcx + half, gcy)],
        close: false,
        fill: None,
        stroke: thin,
    });
    prims.push(Prim::Text {
        x: gcx,
        y: gcy + 2.3,
        font: 3.0,
        text: spec.land_use.clone(),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    // —— 四至/邻宗地注记（GB/T 42547 图 L.3：主方位最长边外侧 2.0mm 起，
    // 宽文本块沿边**切向**滑移避让（法线方向对宽块无效——同边边长注记
    // 恒在法线上；金样邻宗注记亦沿边错开摆放）；障碍=已放置点号/边长/
    // 四至注记与权属环；全候选残余压盖时取最小碰撞位（外部输入文本，
    // 不再诊断标记）；字头向北，`\n` 分行居中堆叠；空串不绘）——
    let mut label_obstacles: Vec<cartography::LabelRect> = point_report
        .labels
        .iter()
        .chain(edge_report.labels.iter())
        .map(|l| l.rect)
        .collect();
    for (dir, text) in [
        ((1.0, 0.0), &spec.sizhi_e),
        ((0.0, -1.0), &spec.sizhi_s),
        ((-1.0, 0.0), &spec.sizhi_w),
        ((0.0, 1.0), &spec.sizhi_n),
    ] {
        let lines: Vec<&str> = text.lines().filter(|s| !s.trim().is_empty()).collect();
        if lines.is_empty() {
            continue;
        }
        const SIZHI_LH: f64 = 3.2; // 行高（毫米）
        let Some((mid, n, tv)) = sizhi_side(&boundary.exterior, dir) else {
            continue;
        };
        // 文本块尺寸（地图单位）：宽 = 最长行估算宽，高 = 行数 × 行高
        let mm_to_map = f64::from(scale) / 1000.0;
        let h_mm = lines.len() as f64 * SIZHI_LH;
        let w_mm = lines
            .iter()
            .map(|l| cartography::text_extent_mm(l.trim(), 2.4).0)
            .fold(0.0_f64, f64::max);
        let (w_map, h_map) = (w_mm * mm_to_map, h_mm * mm_to_map);
        // 二维逃逸：法向 k×0.5mm 抬升（外层）× 切向 0、±0.5…±25mm 滑移（内层）——
        // 宽文本块在短边上会被两角点号注记封锁切向全程，抬高法向净空后
        // 方可越过角点（金样邻宗注记亦远离边中点高悬）；首个零碰撞即取
        let mut rect = cartography::LabelRect {
            cx: 0.0,
            cy: 0.0,
            w: w_map,
            h: h_map,
            rot_rad: 0.0,
        };
        let mut chosen: Option<cartography::LabelRect> = None;
        let mut least: Option<(usize, cartography::LabelRect)> = None;
        'escape: for k in 0..=6 {
            let off_n = (2.0 + h_mm / 2.0 + k as f64 * 0.5) * mm_to_map;
            for i in 0..=50 {
                for sign in [1.0_f64, -1.0] {
                    let off_t = i as f64 * sign * 0.5 * mm_to_map;
                    rect.cx = mid.0 + n.0 * off_n + tv.0 * off_t;
                    rect.cy = mid.1 + n.1 * off_n + tv.1 * off_t;
                    let hits = label_obstacles
                        .iter()
                        .filter(|r| cartography::rects_overlap(&rect, r))
                        .count()
                        + usize::from(cartography::rect_ring_overlap(
                            &rect,
                            &boundary.exterior.points,
                        ));
                    if hits == 0 {
                        chosen = Some(rect);
                        break 'escape;
                    }
                    if least.as_ref().map(|(h, _)| hits < *h).unwrap_or(true) {
                        least = Some((hits, rect));
                    }
                }
            }
        }
        let rect = chosen.or(least.map(|(_, r)| r)).unwrap_or(rect);
        label_obstacles.push(rect); // 四至块互为障碍
        let (sx, sy) = to_page((rect.cx, rect.cy));
        for (i, line) in lines.iter().enumerate() {
            prims.push(Prim::Text {
                x: sx,
                y: sy + (i as f64 - (lines.len() as f64 - 1.0) / 2.0) * SIZHI_LH,
                font: 2.4,
                text: line.trim().to_string(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    // —— 界址点坐标表（地图框右下锚定）——
    table.emit(&mut prims);
    // —— 底部签注带 ——
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 277.2,
        font: 2.5,
        text: spec.survey_note.clone(),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 281.2,
        font: 2.5,
        text: format!("制图日期：{}", spec.draw_date),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + 3.0,
        y: 285.0,
        font: 2.5,
        text: format!("审核日期：{}", spec.review_date),
        anchor: Anchor::Start,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: PAGE_W / 2.0,
        y: (FOOT_Y0 + INNER_RECT[1] + INNER_RECT[3]) / 2.0,
        font: 3.6,
        text: format!("1:{scale}"),
        anchor: Anchor::Middle,
        rotate_deg: 0.0,
        vcenter: true,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 3.0,
        y: 278.5,
        font: 2.6,
        text: format!("制图：{}", spec.drawer),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    prims.push(Prim::Text {
        x: INNER_RECT[0] + INNER_RECT[2] - 3.0,
        y: 283.5,
        font: 2.6,
        text: format!("审核：{}", spec.reviewer),
        anchor: Anchor::End,
        rotate_deg: 0.0,
        vcenter: false,
        bold: false,
    });
    // —— 左侧竖排单位名（地图框内左缘，逐字纵向堆叠）——
    let unit_chars: Vec<char> = spec.unit_name.chars().collect();
    if !unit_chars.is_empty() {
        let step = 3.4;
        let mid = MAP_RECT[1] + MAP_RECT[3] / 2.0;
        let y_start = mid - (unit_chars.len() as f64 - 1.0) * step / 2.0;
        for (i, ch) in unit_chars.iter().enumerate() {
            prims.push(Prim::Text {
                x: MAP_RECT[0] + 3.0,
                y: y_start + i as f64 * step,
                font: 2.8,
                text: ch.to_string(),
                anchor: Anchor::Middle,
                rotate_deg: 0.0,
                vcenter: true,
                bold: false,
            });
        }
    }
    Ok(Scene {
        prims,
        scale,
        diagnostics,
    })
}

// ---------------------------------------------------------------------------
// SVG 后端
// ---------------------------------------------------------------------------

/// 文本 XML 转义。
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// RGB → #RRGGBB。
fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// fill 属性。
fn fill_attr(fill: Option<[u8; 3]>) -> String {
    match fill {
        Some(c) => format!("fill=\"{}\"", hex(c)),
        None => "fill=\"none\"".to_string(),
    }
}

/// stroke 属性串。
fn stroke_attrs(stroke: Option<Stroke>) -> String {
    match stroke {
        Some(s) => format!(
            " stroke=\"{}\" stroke-width=\"{:.2}\"",
            hex(s.color),
            s.width
        ),
        None => String::new(),
    }
}

/// 场景 → SVG 文档（viewBox 即毫米坐标系）。
fn scene_to_svg(prims: &[Prim]) -> String {
    let mut out = String::with_capacity(8192);
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {PAGE_W:.0} {PAGE_H:.0}\" width=\"{PAGE_W:.0}mm\" height=\"{PAGE_H:.0}mm\">\n"
    ));
    out.push_str("<!-- kanyu-render parcelmap · GB/T 42547-2023 图 L.3 -->\n");
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#FFFFFF\"/>\n");
    for p in prims {
        match p {
            Prim::Rect { rect, fill, stroke } => {
                out.push_str(&format!(
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" {}{}/>\n",
                    rect[0],
                    rect[1],
                    rect[2],
                    rect[3],
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Path {
                pts,
                close,
                fill,
                stroke,
            } => {
                let tag = if *close { "polygon" } else { "polyline" };
                let points: Vec<String> =
                    pts.iter().map(|(x, y)| format!("{x:.2},{y:.2}")).collect();
                out.push_str(&format!(
                    "<{tag} points=\"{}\" {}{}/>\n",
                    points.join(" "),
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Circle {
                cx,
                cy,
                r,
                fill,
                stroke,
            } => {
                out.push_str(&format!(
                    "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r:.2}\" {}{}/>\n",
                    fill_attr(*fill),
                    stroke_attrs(*stroke)
                ));
            }
            Prim::Text {
                x,
                y,
                font,
                text,
                anchor,
                rotate_deg,
                vcenter,
                bold,
            } => {
                let anchor_s = match anchor {
                    Anchor::Start => "start",
                    Anchor::Middle => "middle",
                    Anchor::End => "end",
                };
                let mut attrs = String::new();
                if *vcenter {
                    attrs.push_str(" dominant-baseline=\"central\"");
                }
                if *bold {
                    attrs.push_str(" font-weight=\"bold\"");
                }
                if rotate_deg.abs() > 1e-9 {
                    attrs.push_str(&format!(
                        " transform=\"rotate({rotate_deg:.2} {x:.2} {y:.2})\""
                    ));
                }
                out.push_str(&format!(
                    "<text x=\"{x:.2}\" y=\"{y:.2}\" font-size=\"{font:.2}\" font-family=\"sans-serif\" text-anchor=\"{anchor_s}\" fill=\"#000000\"{attrs}>{}</text>\n",
                    esc(text)
                ));
            }
        }
    }
    out.push_str("</svg>\n");
    out
}

// ---------------------------------------------------------------------------
// PNG 后端（tiny-skia + TextBackend 系统字体栈；毫米 → 像素 k = dpi/25.4）
// ---------------------------------------------------------------------------

/// 场景 → PNG 字节（A4 竖；旋转注记按已知限制水平绘制于排版中心）。
fn scene_to_png(prims: &[Prim], dpi: f64, tb: &TextBackend) -> Result<Vec<u8>, RenderError> {
    use tiny_skia::{
        Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Shader, Stroke as SkStroke, Transform,
    };
    let (pw, ph) = PageSize::A4Portrait.pixels(dpi);
    let k = dpi / 25.4; // 毫米 → 像素
    let mut page = Pixmap::new(pw, ph).ok_or(RenderError::InvalidSize(pw, ph))?;
    page.fill(Color::from_rgba8(255, 255, 255, 255));
    let paint = |c: [u8; 3]| Paint {
        shader: Shader::SolidColor(Color::from_rgba8(c[0], c[1], c[2], 255)),
        anti_alias: true,
        ..Default::default()
    };
    // 路径填充 + 描边（图元公共尾段）
    let fill_stroke = |page: &mut Pixmap,
                       path: &tiny_skia::Path,
                       fill: &Option<[u8; 3]>,
                       stroke: &Option<Stroke>| {
        if let Some(c) = fill {
            page.fill_path(
                path,
                &paint(*c),
                FillRule::Winding,
                Transform::default(),
                None,
            );
        }
        if let Some(s) = stroke {
            page.stroke_path(
                path,
                &paint(s.color),
                &SkStroke {
                    width: (s.width * k) as f32,
                    ..Default::default()
                },
                Transform::default(),
                None,
            );
        }
    };
    for p in prims {
        match p {
            Prim::Rect { rect, fill, stroke } => {
                let Some(r) = Rect::from_xywh(
                    (rect[0] * k) as f32,
                    (rect[1] * k) as f32,
                    (rect[2] * k) as f32,
                    (rect[3] * k) as f32,
                ) else {
                    continue;
                };
                let mut pb = PathBuilder::new();
                pb.push_rect(r);
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page, &path, fill, stroke);
                }
            }
            Prim::Path {
                pts,
                close,
                fill,
                stroke,
            } => {
                if pts.len() < 2 {
                    continue;
                }
                let mut pb = PathBuilder::new();
                pb.move_to((pts[0].0 * k) as f32, (pts[0].1 * k) as f32);
                for &(x, y) in &pts[1..] {
                    pb.line_to((x * k) as f32, (y * k) as f32);
                }
                if *close {
                    pb.close();
                }
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page, &path, fill, stroke);
                }
            }
            Prim::Circle {
                cx,
                cy,
                r,
                fill,
                stroke,
            } => {
                let mut pb = PathBuilder::new();
                pb.push_circle((cx * k) as f32, (cy * k) as f32, (r * k) as f32);
                if let Some(path) = pb.finish() {
                    fill_stroke(&mut page, &path, fill, stroke);
                }
            }
            Prim::Text {
                x,
                y,
                font,
                text,
                anchor,
                vcenter,
                rotate_deg,
                ..
            } => {
                let px = (font * k) as f32;
                let w = tb.measure(text, px);
                let x_px = (*x * k) as f32;
                // 旋转注记（边长沿线）：离屏小 pixmap 写字后旋转合成，
                // 与 SVG rotate() 同角（顺时针为正）
                if rotate_deg.abs() > 1e-6 {
                    draw_rotated_text(&mut page, tb, text, x_px, (*y * k) as f32, px, *rotate_deg);
                    continue;
                }
                let sx = match anchor {
                    Anchor::Start => x_px,
                    Anchor::Middle => x_px - w / 2.0,
                    Anchor::End => x_px - w,
                };
                // vcenter：基线 ≈ 中心 + 0.35em（cap 高近似）
                let baseline = if *vcenter {
                    ((*y + font * 0.35) * k) as f32
                } else {
                    (*y * k) as f32
                };
                tb.draw(
                    &mut page,
                    text,
                    sx,
                    baseline,
                    px,
                    Color::from_rgba8(0, 0, 0, 255),
                );
            }
        }
    }
    page.encode_png()
        .map_err(|e| RenderError::InvalidStyle(format!("宗地图 PNG 编码失败: {e}")))
}

/// 旋转文本绘制（PNG）：文本先绘入透明离屏 pixmap，再绕中心旋转移植。
/// 中心语义与排版引擎输出一致（anchor=Middle + vcenter）；`deg` 顺时针为正
/// （y 向下屏幕系，与 SVG rotate()/QGIS 存储同号）。
fn draw_rotated_text(
    page: &mut tiny_skia::Pixmap,
    tb: &TextBackend,
    text: &str,
    cx: f32,
    cy: f32,
    px: f32,
    deg: f64,
) {
    use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};
    let pad = 2_u32;
    let ow = (tb.measure(text, px).ceil() as u32 + pad * 2).max(4);
    let oh = ((px * 1.4).ceil() as u32 + pad * 2).max(4);
    let Some(mut off) = Pixmap::new(ow, oh) else {
        return;
    };
    // 基线：cap 中心对 pixmap 中心（cap 高 ≈0.7em，中心 ≈ 基线 − 0.35em；
    // pixmap 中心 = pad + 0.7em → 基线 = pad + 1.05em）
    let baseline = pad as f32 + px * 1.05;
    tb.draw(
        &mut off,
        text,
        pad as f32,
        baseline,
        px,
        Color::from_rgba8(0, 0, 0, 255),
    );
    let (owf, ohf) = (ow as f32, oh as f32);
    let rad = (deg as f32).to_radians();
    let (sn, cs) = rad.sin_cos();
    // off 中心 → (cx, cy)：t = c − R(θ)·(ow/2, oh/2)
    let tx = cx - (owf / 2.0) * cs + (ohf / 2.0) * sn;
    let ty = cy - (owf / 2.0) * sn - (ohf / 2.0) * cs;
    let ts = Transform::from_row(cs, sn, -sn, cs, tx, ty);
    page.draw_pixmap(0, 0, off.as_ref(), &PixmapPaint::default(), ts, None);
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kanyu_core::cartography::RingRole;

    /// 测试宗地：40m×30m 矩形（投影坐标，东 39595000 起、北 4127000 起，CCW 存储）。
    fn test_boundary() -> ParcelBoundary {
        ParcelBoundary {
            exterior: RealestateRing::new(
                vec![
                    (39595000.0, 4127000.0),
                    (39595040.0, 4127000.0),
                    (39595040.0, 4127030.0),
                    (39595000.0, 4127030.0),
                ],
                RingRole::Exterior,
            ),
            interiors: vec![],
        }
    }

    /// 完整 spec（比例尺缺省自动求解、面积缺省现算）。
    fn full_spec() -> ParcelMapSpec {
        ParcelMapSpec {
            parcel_code: "371602113005GB00032".to_string(),
            owner: "中国联合网络通信有限公司滨州市分公司".to_string(),
            map_sheet: "27.00-95.25".to_string(),
            area_sqm: None,
            land_use: "0801".to_string(),
            unit_name: "某某市自然资源局".to_string(),
            survey_note: "2026年08月解析法测绘界址点".to_string(),
            drawer: "XXX".to_string(),
            reviewer: "XXX".to_string(),
            draw_date: "2026年08月25日".to_string(),
            review_date: "2026年08月25日".to_string(),
            scale: None,
            dpi: 150.0,
            point_prefix: "J".to_string(),
            sizhi_e: String::new(),
            sizhi_s: String::new(),
            sizhi_w: String::new(),
            sizhi_n: String::new(),
            roads: Vec::new(),
        }
    }

    /// 从产物取 SVG 文本（断言辅助）。
    fn svg_of(out: &ParcelMapOutput) -> &str {
        match &out.data {
            ParcelMapData::Svg(s) => s,
            ParcelMapData::Png(_) => panic!("应为 SVG 产物"),
        }
    }

    #[test]
    fn svg_rect_parcel_full_spec() {
        let out = render_parcel_map_svg(&test_boundary(), &full_spec()).unwrap();
        // 自动比例尺手算验证：坐标表高 5.0+4.2×7=34.4mm → 适配区 168×177.6mm；
        // raw = max(40×1000/168, 30×1000/177.6) = max(238.1, 168.9) = 238.1 → 整百 300
        assert_eq!(out.scale, 300);
        let svg = svg_of(&out);
        // 版式要素：标题 / 单位注记 / 头部信息框 / 点号 / 比例尺 / 分式 / 坐标表
        for frag in [
            "宗 地 图",
            "单位：m㎡",
            "371602113005GB00032",
            "J1",
            "J4",
            "1:300",
            "GB00032", // 分式分子 = 宗地代码末 7 位
            "0801",    // 分式分母 = 地类编码
            "界址点坐标表",
            "宗地面积",
            "1200.00", // 面积缺省现算：40×30=1200.00
            "土地权利人：中国联合网络通信有限公司滨州市分公司",
            "2026年08月解析法测绘界址点",
        ] {
            assert!(svg.contains(frag), "SVG 缺少要素: {frag}");
        }
        // 坐标表 X 列 = 北坐标（测绘惯例）：J1 为西北角（东 39595000、北 4127030）
        assert!(svg.contains("4127030.000"), "X 列应为北坐标");
        assert!(svg.contains("39595000.000"), "Y 列应为东坐标");
        // 排版诊断非空（点号 + 边长逐条摘要）
        assert!(!out.diagnostics.is_empty());
        assert!(out.diagnostics.iter().any(|d| d.starts_with("J1 reason=")
            && d.contains("clearance=")
            && d.contains("overlap=")));
    }

    #[test]
    fn coord_table_rows_rules() {
        let boundary = test_boundary();
        let points = cartography::generate_boundary_points(&boundary, "J");
        let lines = cartography::generate_boundary_lines(&boundary, &points);
        let table = CoordTable::build(&points, &lines, 1200.0);
        // 行数 = 4 点 + 1 闭合行
        assert_eq!(table.rows.len(), 5);
        // 首行 J1（西北角起编）：X=北 4127030.000、Y=东 39595000.000；边长挂接 J1→J2 = 30.00
        assert_eq!(
            table.rows[0],
            ["J1", "4127030.000", "39595000.000", "30.00"]
        );
        // J2→J3 边长 40.00 挂在 J2 行
        assert_eq!(table.rows[1][0], "J2");
        assert_eq!(table.rows[1][3], "40.00");
        // 末行重复 1 号点坐标闭合（点号仍写 1 号点号），无边长
        assert_eq!(table.rows[4], ["J1", "4127030.000", "39595000.000", ""]);
        // 表尾面积行（2 位小数）
        assert_eq!(table.area_value, "1200.00");
    }

    #[test]
    fn given_scale_used_directly() {
        let spec = ParcelMapSpec {
            scale: Some(500),
            ..full_spec()
        };
        let out = render_parcel_map_svg(&test_boundary(), &spec).unwrap();
        assert_eq!(out.scale, 500);
        assert!(svg_of(&out).contains("1:500"));
    }

    #[test]
    fn png_encodes_and_dump() {
        let out = render_parcel_map_png(&test_boundary(), &full_spec()).unwrap();
        assert_eq!(out.scale, 300);
        let png = match &out.data {
            ParcelMapData::Png(b) => b,
            ParcelMapData::Svg(_) => panic!("应为 PNG 产物"),
        };
        // PNG 魔数
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
        // 红色界址线存在
        assert!(pixmap
            .pixels()
            .iter()
            .any(|p| p.red() > 200 && p.green() < 80 && p.blue() < 80));
        // 落盘目检（仿 layout_cjk_test）
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/parcel_map_test.png");
        std::fs::write(&path, png).unwrap();
    }

    /// 60 顶点圆形宗地（数据行 61 > A4 单块容量）。
    fn big_boundary() -> ParcelBoundary {
        let pts: Vec<(f64, f64)> = (0..60)
            .map(|i| {
                let a = i as f64 / 60.0 * std::f64::consts::TAU;
                (39595000.0 + 100.0 * a.cos(), 4127000.0 + 100.0 * a.sin())
            })
            .collect();
        ParcelBoundary {
            exterior: RealestateRing::new(pts, RingRole::Exterior),
            interiors: vec![],
        }
    }

    #[test]
    fn folded_coord_table_right_to_left() {
        let boundary = big_boundary();
        let points = cartography::generate_boundary_points(&boundary, "J");
        let lines = cartography::generate_boundary_lines(&boundary, &points);
        let table = CoordTable::build(&points, &lines, 31415.93);
        let chunks = table.block_rows();
        assert!(chunks.len() > 1, "61 数据行应触发折列: {chunks:?}");
        assert_eq!(chunks.iter().sum::<usize>(), table.rows.len());
        assert!(
            table.display_height() <= TABLE_MAX_H + 1e-6,
            "折列后展示高度应 ≤ 框内可用高"
        );
        let mut prims = Vec::new();
        table.emit(&mut prims);
        // 题行仅首列一次；表头每列重复；面积行仅末列一次
        let titles = prims
            .iter()
            .filter(|p| matches!(p, Prim::Text { text, .. } if text == "界址点坐标表"))
            .count();
        assert_eq!(titles, 1);
        let headers = prims
            .iter()
            .filter(|p| matches!(p, Prim::Text { text, .. } if text == "X坐标(m)"))
            .count();
        assert_eq!(headers, chunks.len());
        let areas = prims
            .iter()
            .filter(|p| matches!(p, Prim::Text { text, .. } if text == "宗地面积(平方米)"))
            .count();
        assert_eq!(areas, 1);
        // 右起列流：首列外框在次列右侧
        let rects: Vec<[f64; 4]> = prims
            .iter()
            .filter_map(|p| match p {
                Prim::Rect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert!(rects.len() == chunks.len() && rects[0][0] > rects[1][0]);
        // 面积值在末列（最左）：其 x 小于首列 x0
        let area_x = prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { text, x, .. } if text == "31415.93" => Some(*x),
                _ => None,
            })
            .expect("面积值文本应在图元中");
        assert!(area_x < rects[0][0], "面积行应在末列（最左）");
    }

    #[test]
    fn png_rotated_text_draws_vertical_ink() {
        let tb = TextBackend::system();
        let mut page = tiny_skia::Pixmap::new(200, 200).unwrap();
        page.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
        // 「8888」旋转 90°：真字体与点阵回退均有数字字形
        draw_rotated_text(&mut page, &tb, "8888", 100.0, 100.0, 20.0, 90.0);
        let (mut minx, mut miny, mut maxx, mut maxy) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..200 {
            for x in 0..200 {
                let p = page.pixel(x, y).unwrap();
                if p.red() < 100 {
                    minx = minx.min(x);
                    maxx = maxx.max(x);
                    miny = miny.min(y);
                    maxy = maxy.max(y);
                }
            }
        }
        assert!(maxx > minx && maxy > miny, "旋转文本应有墨迹");
        assert!(
            (maxy - miny) > (maxx - minx),
            "90° 旋转文本应垂直延展（高 {} > 宽 {}）",
            maxy - miny,
            maxx - minx
        );
    }
    #[test]
    fn sizhi_labels_anchor_outside_matching_side() {
        let boundary = test_boundary(); // 40×30 CCW 矩形：东 39595000-39595040，北 4127000-4127030
                                        // 东：主方位边=东界（中点 (39595040, 4127015)，外法线 (1,0)）
        let ((mx, my), (nx, ny), _) = sizhi_side(&boundary.exterior, (1.0, 0.0)).unwrap();
        assert!((mx - 39595040.0).abs() < 1e-6 && (my - 4127015.0).abs() < 1e-6);
        assert!(nx > 0.99 && ny.abs() < 1e-9, "东界外法线应朝东: {nx},{ny}");
        // 南/西/北同理
        let ((_, sy), (_, sny), _) = sizhi_side(&boundary.exterior, (0.0, -1.0)).unwrap();
        assert!(
            (sy - 4127000.0).abs() < 1e-6 && sny < -0.99,
            "南界中点且法线朝南"
        );
        let ((wx, _), (wnx, _), _) = sizhi_side(&boundary.exterior, (-1.0, 0.0)).unwrap();
        assert!(
            (wx - 39595000.0).abs() < 1e-6 && wnx < -0.99,
            "西界中点且法线朝西"
        );
        let ((_, ny2), (_, nny), _) = sizhi_side(&boundary.exterior, (0.0, 1.0)).unwrap();
        assert!(
            (ny2 - 4127030.0).abs() < 1e-6 && nny > 0.99,
            "北界中点且法线朝北"
        );
        // SVG 含分行四至注记
        let spec = ParcelMapSpec {
            sizhi_e: "东侧邻宗
GB00029"
                .to_string(),
            sizhi_s: "南侧邻宗".to_string(),
            ..full_spec()
        };
        let out = render_parcel_map_svg(&test_boundary(), &spec).unwrap();
        let svg = svg_of(&out);
        assert!(svg.contains("东侧邻宗") && svg.contains("GB00029"));
        assert!(svg.contains("南侧邻宗"));
        // 无四至（默认全空）：不含
        let out0 = render_parcel_map_svg(&test_boundary(), &full_spec()).unwrap();
        let svg0 = svg_of(&out0);
        assert!(!svg0.contains("邻宗"));
    }
    #[test]
    fn sizhi_wide_block_collides_with_edge_label_at_base() {
        // 回归（GB32 北边界实案）：宽四至文本块在基准位必与同边边长注记判交——
        // 驱动二维逃逸（法向抬升 × 切向滑移）的必要性断言
        let edge_mid = (39595481.05, 4127301.60);
        let n = (-0.062, 0.998);
        let mm_to_map = 0.7;
        // 37.10 边长注记 rect（近似）
        let edge_label = cartography::LabelRect {
            cx: 39595480.95,
            cy: 4127303.14,
            w: 4.03,
            h: 1.68,
            rot_rad: 0.062,
        };
        // N 四至块 t=0
        let off_n = (2.0 + 3.2) * mm_to_map;
        let sizhi = cartography::LabelRect {
            cx: edge_mid.0 + n.0 * off_n,
            cy: edge_mid.1 + n.1 * off_n,
            w: 38.4 * mm_to_map,
            h: 6.4 * mm_to_map,
            rot_rad: 0.0,
        };
        assert!(
            cartography::rects_overlap(&sizhi, &edge_label),
            "t=0 应判交"
        );
    }

    #[test]
    fn clip_segment_cases() {
        let rect = [0.0, 0.0, 100.0, 100.0];
        // 横穿：两端截断到边界
        let ((x0, _), (x1, _)) = clip_segment_to_rect((-10.0, 50.0), (110.0, 50.0), rect).unwrap();
        assert!((x0 - 0.0).abs() < 1e-9 && (x1 - 100.0).abs() < 1e-9);
        // 全在内
        let (a, b) = clip_segment_to_rect((10.0, 10.0), (20.0, 20.0), rect).unwrap();
        assert_eq!(a, (10.0, 10.0));
        assert_eq!(b, (20.0, 20.0));
        // 全在外（左侧平行）
        assert!(clip_segment_to_rect((-5.0, 10.0), (-5.0, 90.0), rect).is_none());
        // 全在外（对角不过框）
        assert!(clip_segment_to_rect((-10.0, 110.0), (-5.0, 120.0), rect).is_none());
        // 部分在内
        let (a, _) = clip_segment_to_rect((50.0, 50.0), (150.0, 50.0), rect).unwrap();
        assert_eq!(a, (50.0, 50.0));
    }

    #[test]
    fn roads_clip_and_name_along_line() {
        let boundary = test_boundary(); // 40×30 矩形，scale 300 → 地图 1m ≈ 3.33mm
                                        // 道路：纵贯地图框的东西向长线（穿过宗地南侧）
        let spec = ParcelMapSpec {
            roads: vec![RoadLine {
                path: vec![(39594900.0, 4126990.0), (39595120.0, 4126995.0)],
                name: "南大街".to_string(),
            }],
            ..full_spec()
        };
        let out = render_parcel_map_svg(&boundary, &spec).unwrap();
        let svg = svg_of(&out);
        assert!(svg.contains("南大街"), "路名应沿线绘出");
        // 可见段过短：路名让位仅绘线（宗地北侧 1m 处 2m 短线）
        let spec2 = ParcelMapSpec {
            roads: vec![RoadLine {
                path: vec![(39595000.0, 4127031.0), (39595002.0, 4127031.0)],
                name: "超短巷".to_string(),
            }],
            ..full_spec()
        };
        let out2 = render_parcel_map_svg(&boundary, &spec2).unwrap();
        let svg2 = svg_of(&out2);
        assert!(!svg2.contains("超短巷"), "可见段过短路名应让位");
        // 提取助手：LineString/MultiLineString + 路名候选键
        let gj: geojson::GeoJson = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[10,0]]},
             "properties":{"name":"南大街"}},
            {"type":"Feature","geometry":{"type":"MultiLineString","coordinates":[[[0,1],[5,1]],[[5,1],[9,2]]]},
             "properties":{"道路名称":"解放路"}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{}}
        ]}"#
        .parse()
        .unwrap();
        let roads = roads_from_collection(
            &geojson::FeatureCollection::try_from(gj).unwrap(),
            &["name", "道路名称"],
        );
        assert_eq!(roads.len(), 3);
        assert_eq!(roads[0].name, "南大街");
        assert_eq!(roads[1].name, "解放路");
        assert_eq!(roads[2].path.len(), 2);
    }
}
