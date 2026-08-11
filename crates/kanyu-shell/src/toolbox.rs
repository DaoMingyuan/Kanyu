//! 工具箱面板（QGIS Processing 工具箱范式）：声明式工具注册表 + 分类树 +
//! 通用参数对话框 + 统一执行入口。
//!
//! ## 架构约定
//!
//! - **单一事实来源**：[`TOOLS`] 注册表声明每个工具的 id/中文名/分类/一句说明/
//!   参数表；面板树与参数表单都由注册表驱动，加工具 = 表内加一行 + `run_tool`
//!   加一个分支。
//! - 分类按 QGIS 分组（中文）：矢量分析 / 矢量几何 / 矢量选择 / 数据管理 /
//!   统计度量（[`ToolCategory`]）。
//! - 语义映射：裁剪 = overlay intersection、差值 = difference 等——独立中文
//!   工具名映射到同一内核函数（分支内注明）。
//! - 执行入口 [`run_tool`] 不触碰 app 状态：图层访问经 `get_layer` 闭包注入，
//!   产出 [`ToolOutcome`]（新图层 / 终端报告 / 导出）由 app 结算——借用零冲突。
//! - 参数校验 [`validate`] 为纯函数，配单元测试。

use eframe::egui;
use geojson::FeatureCollection;
use kanyu_core::{analysis, crs, geoprocess, Layer};

use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{
    combo_static, error_caption, hint_caption, layer_picker, text_input, tree_row,
};

// ===== 注册表 =====

/// 参数类型。
pub enum ParamKind {
    /// 输入图层（下拉选图层 id）。
    Layer,
    /// 数值（f64 文本输入）。
    Number,
    /// 数值列表（逗号分隔，如多环距离）。
    NumberList,
    /// 自由文本（路径、CRS、范围四数、逗号清单等）。
    Text,
    /// 字段名：选项取自第 N 个 Layer 参数（params 下标）所选图层的字段。
    Field(usize),
    /// 枚举：（内核值， 中文标签）静态选项。
    Enum(&'static [(&'static str, &'static str)]),
    /// 属性表达式（如 `height > 50`）。
    Expression,
}

/// 工具参数声明。
pub struct ToolParam {
    /// 参数键（run_tool 取值用）。
    pub key: &'static str,
    /// 中文标签。
    pub label: &'static str,
    /// 类型。
    pub kind: ParamKind,
    /// 是否必填（可选参数如融合字段、权重字段、最小面积）。
    pub required: bool,
    /// 输入占位提示。
    pub hint: &'static str,
    /// 默认值。
    pub default: &'static str,
}

const fn param(
    key: &'static str,
    label: &'static str,
    kind: ParamKind,
    required: bool,
    hint: &'static str,
    default: &'static str,
) -> ToolParam {
    ToolParam {
        key,
        label,
        kind,
        required,
        hint,
        default,
    }
}

/// 工具分类（QGIS 分组，中文）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolCategory {
    /// 矢量分析。
    Analysis,
    /// 矢量几何。
    Geometry,
    /// 矢量选择。
    Selection,
    /// 数据管理。
    DataManagement,
    /// 统计度量。
    Statistics,
}

impl ToolCategory {
    /// 全部分类（显示顺序）。
    pub const ALL: [ToolCategory; 5] = [
        ToolCategory::Analysis,
        ToolCategory::Geometry,
        ToolCategory::Selection,
        ToolCategory::DataManagement,
        ToolCategory::Statistics,
    ];

    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            ToolCategory::Analysis => "矢量分析",
            ToolCategory::Geometry => "矢量几何",
            ToolCategory::Selection => "矢量选择",
            ToolCategory::DataManagement => "数据管理",
            ToolCategory::Statistics => "统计度量",
        }
    }

    /// 下标（折叠状态表）。
    fn index(self) -> usize {
        match self {
            ToolCategory::Analysis => 0,
            ToolCategory::Geometry => 1,
            ToolCategory::Selection => 2,
            ToolCategory::DataManagement => 3,
            ToolCategory::Statistics => 4,
        }
    }
}

/// 工具声明。
pub struct ToolDef {
    /// 工具 id（英文，结果图层命名前缀）。
    pub id: &'static str,
    /// 中文名。
    pub name: &'static str,
    /// 分类。
    pub category: ToolCategory,
    /// 一句中文说明。
    pub desc: &'static str,
    /// 参数表。
    pub params: &'static [ToolParam],
    /// true = 统计类（结果输出终端）；false = 产出新图层。
    pub report: bool,
}

const fn tool(
    id: &'static str,
    name: &'static str,
    category: ToolCategory,
    desc: &'static str,
    params: &'static [ToolParam],
    report: bool,
) -> ToolDef {
    ToolDef {
        id,
        name,
        category,
        desc,
        params,
        report,
    }
}

/// 空间谓词选项（内核值与 analysis::SpatialPredicate 解析对齐）。
const PREDICATES: &[(&str, &str)] = &[
    ("intersects", "相交"),
    ("contains", "包含"),
    ("within", "位于…内"),
];

/// 工具注册表（全部内核算法的中文工具面）。
pub const TOOLS: &[ToolDef] = &[
    // —— 矢量分析 ——
    tool(
        "buffer",
        "缓冲区",
        ToolCategory::Analysis,
        "按距离生成缓冲区（结果存为新图层；米制请先投影）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "distance",
                "距离",
                ParamKind::Number,
                true,
                "CRS 单位（投影后为米）",
                "",
            ),
        ],
        false,
    ),
    tool(
        "overlay_union",
        "联合",
        ToolCategory::Analysis,
        "两面图层并集（overlay union）",
        &[
            param("layer", "目标图层", ParamKind::Layer, true, "", ""),
            param("overlay", "叠加图层", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "overlay_intersection",
        "裁剪",
        ToolCategory::Analysis,
        "以叠加图层范围裁剪目标图层（overlay intersection）",
        &[
            param("layer", "目标图层", ParamKind::Layer, true, "", ""),
            param("overlay", "裁剪范围（面）", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "overlay_difference",
        "差值",
        ToolCategory::Analysis,
        "目标图层减去叠加图层（overlay difference）",
        &[
            param("layer", "目标图层", ParamKind::Layer, true, "", ""),
            param("overlay", "减去部分（面）", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "overlay_xor",
        "对称差",
        ToolCategory::Analysis,
        "两面图层对称差（overlay xor）",
        &[
            param("layer", "目标图层", ParamKind::Layer, true, "", ""),
            param("overlay", "叠加图层", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "sjoin",
        "空间连接",
        ToolCategory::Analysis,
        "按空间谓词合并两图层属性（左连接 + explode）",
        &[
            param("layer", "目标图层", ParamKind::Layer, true, "", ""),
            param("join", "连接图层", ParamKind::Layer, true, "", ""),
            param(
                "predicate",
                "谓词",
                ParamKind::Enum(PREDICATES),
                true,
                "",
                "相交",
            ),
        ],
        false,
    ),
    tool(
        "count_points_in_polygon",
        "面内点计数",
        ToolCategory::Analysis,
        "统计每个面要素内的点数（追加 NUMPOINTS 属性）",
        &[
            param("layer", "面图层", ParamKind::Layer, true, "", ""),
            param("points", "点图层", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "mean_coordinates",
        "平均坐标",
        ToolCategory::Analysis,
        "要素质心的（加权）平均坐标点",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "weight",
                "权重字段",
                ParamKind::Field(0),
                false,
                "可空 = 等权",
                "",
            ),
        ],
        false,
    ),
    tool(
        "distance_matrix",
        "距离矩阵",
        ToolCategory::Analysis,
        "两图层代表点逐对测地线距离（米，矩阵输出终端）",
        &[
            param("layer", "图层 A", ParamKind::Layer, true, "", ""),
            param("layer2", "图层 B", ParamKind::Layer, true, "", ""),
        ],
        true,
    ),
    tool(
        "nearest_neighbor",
        "最近邻分析",
        ToolCategory::Analysis,
        "点集最近邻指数（<1 聚集 / ≈1 随机 / >1 离散，输出终端）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        true,
    ),
    tool(
        "multi_ring_buffer",
        "多环缓冲区",
        ToolCategory::Analysis,
        "按距离列表生成多环缓冲（环带 RING/DISTANCE 属性）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "distances",
                "距离列表",
                ParamKind::NumberList,
                true,
                "逗号分隔，严格递增非负，如 100,200,300",
                "",
            ),
        ],
        false,
    ),
    tool(
        "variable_buffer",
        "按字段缓冲区",
        ToolCategory::Analysis,
        "缓冲距离取数值字段（逐要素变距）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("field", "距离字段", ParamKind::Field(0), true, "", ""),
            param(
                "segments",
                "圆弧分段数",
                ParamKind::Number,
                true,
                "≥1",
                "16",
            ),
        ],
        false,
    ),
    // —— 矢量几何 ——
    tool(
        "dissolve",
        "融合",
        ToolCategory::Geometry,
        "按字段分组做布尔并集（空 = 全组融合）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "field",
                "分组字段",
                ParamKind::Field(0),
                false,
                "可空 = 全部融合为一",
                "",
            ),
        ],
        false,
    ),
    tool(
        "centroid",
        "质心",
        ToolCategory::Geometry,
        "逐要素几何质心（点图层）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "convex_hull",
        "凸包",
        ToolCategory::Geometry,
        "全图层要素的总凸包（单面要素）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "simplify",
        "简化",
        ToolCategory::Geometry,
        "Douglas-Peucker 几何简化（容差为 CRS 单位）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("tolerance", "容差", ParamKind::Number, true, "如 0.001", ""),
        ],
        false,
    ),
    tool(
        "delete_holes",
        "删洞",
        ToolCategory::Geometry,
        "删除面要素内环（可设最小面积阈值）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "min_area",
                "最小面积",
                ParamKind::Number,
                false,
                "可空 = 删除全部洞",
                "",
            ),
        ],
        false,
    ),
    tool(
        "explode",
        "炸开多部件",
        ToolCategory::Geometry,
        "Multi* → 单部件逐要素（属性复制）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "boundary",
        "边界",
        ToolCategory::Geometry,
        "面转边界线 / 开放线转端点",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "bounding_boxes",
        "包络矩形",
        ToolCategory::Geometry,
        "逐要素外包矩形（面图层）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "points_along_lines",
        "沿线等距点",
        ToolCategory::Geometry,
        "沿折线按测地线间距生成里程点（DISTANCE 属性，米）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("distance", "间距（米）", ParamKind::Number, true, ">0", ""),
        ],
        false,
    ),
    tool(
        "concave_hull",
        "凹包",
        ToolCategory::Geometry,
        "整层要素的凹包（单面）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "concavity",
                "凹度",
                ParamKind::Number,
                true,
                ">0，越大越凹",
                "2.0",
            ),
        ],
        false,
    ),
    tool(
        "minimum_rotated_rect",
        "定向最小包络矩形",
        ToolCategory::Geometry,
        "逐要素最小旋转外接矩形（面图层）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "topology_check",
        "拓扑检查",
        ToolCategory::Geometry,
        "no_overlap 规则检查（报告输出终端）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        true,
    ),
    // —— 矢量选择 ——
    tool(
        "extract_by_attribute",
        "按属性提取",
        ToolCategory::Selection,
        "按属性表达式提取要素为新图层",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "expr",
                "表达式",
                ParamKind::Expression,
                true,
                "如 height > 50",
                "",
            ),
        ],
        false,
    ),
    tool(
        "extract_by_location",
        "按位置提取",
        ToolCategory::Selection,
        "按与掩膜图层的空间关系提取要素",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("mask", "掩膜图层", ParamKind::Layer, true, "", ""),
            param(
                "predicate",
                "谓词",
                ParamKind::Enum(PREDICATES),
                true,
                "",
                "相交",
            ),
        ],
        false,
    ),
    tool(
        "query",
        "属性查询",
        ToolCategory::Selection,
        "属性查询表达式过滤（结果存为新图层）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "expr",
                "表达式",
                ParamKind::Expression,
                true,
                "如 usage == residential",
                "",
            ),
        ],
        false,
    ),
    // —— 数据管理 ——
    tool(
        "merge",
        "合并矢量图层",
        ToolCategory::DataManagement,
        "两图层要素按序拼接为一层",
        &[
            param("layer", "图层 A", ParamKind::Layer, true, "", ""),
            param("layer2", "图层 B", ParamKind::Layer, true, "", ""),
        ],
        false,
    ),
    tool(
        "split_by_field",
        "分割矢量图层",
        ToolCategory::DataManagement,
        "按字段值拆分为多图层（每组一个，split_源图层_组值）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("field", "分组字段", ParamKind::Field(0), true, "", ""),
        ],
        false,
    ),
    tool(
        "add_geometry_attributes",
        "添加几何属性",
        ToolCategory::DataManagement,
        "追加几何量属性列（AREA_M2/PERIMETER_M/LENGTH_M，测地口径）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        false,
    ),
    tool(
        "create_grid",
        "创建网格",
        ToolCategory::DataManagement,
        "按范围与格距生成规则格网（ROW/COL 属性）",
        &[
            // 范围默认取当前数据范围（app 打开对话框时填入，可改）。
            param(
                "extent",
                "范围",
                ParamKind::Text,
                true,
                "minx,miny,maxx,maxy（默认已填当前数据范围）",
                "",
            ),
            param(
                "cell_size",
                "格距",
                ParamKind::Number,
                true,
                "CRS 单位，>0",
                "",
            ),
        ],
        false,
    ),
    tool(
        "reproject",
        "投影变换",
        ToolCategory::DataManagement,
        "EPSG 全库投影变换（结果存为新图层）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "from",
                "源 CRS",
                ParamKind::Text,
                true,
                "如 EPSG:4326",
                "EPSG:4326",
            ),
            // 目标 CRS 默认取工程坐标系（app 打开对话框时填入）。
            param(
                "to",
                "目标 CRS",
                ParamKind::Text,
                true,
                "默认 = 工程坐标系",
                "",
            ),
        ],
        false,
    ),
    tool(
        "export",
        "导出图层",
        ToolCategory::DataManagement,
        "导出为 geojson/csv/fgb/parquet/dxf/kml/kmz/shp（按扩展名）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("out", "输出路径", ParamKind::Text, true, "如 out.fgb", ""),
        ],
        false,
    ),
    // —— 统计度量 ——
    tool(
        "zonal_stats",
        "分区统计",
        ToolCategory::Statistics,
        "值图层按区归属统计数值字段（列写回分区图层）",
        &[
            param("zones", "分区（面）图层", ParamKind::Layer, true, "", ""),
            param("values", "值图层", ParamKind::Layer, true, "", ""),
            param("field", "数值字段", ParamKind::Field(1), true, "", ""),
            param(
                "stats",
                "统计项",
                ParamKind::Text,
                true,
                "count,sum,mean,min,max",
                "count,sum,mean,min,max",
            ),
        ],
        false,
    ),
    tool(
        "stats",
        "图层统计",
        ToolCategory::Statistics,
        "要素数/总长/总面积（测地线口径，报告输出终端）",
        &[param("layer", "输入图层", ParamKind::Layer, true, "", "")],
        true,
    ),
    tool(
        "field_stats",
        "字段统计",
        ToolCategory::Statistics,
        "数值字段基本统计（报告输出终端）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param("field", "数值字段", ParamKind::Field(0), true, "", ""),
        ],
        true,
    ),
    tool(
        "measure",
        "测地度量",
        ToolCategory::Statistics,
        "Karney 2013 测地线长度/面积（米/平方米，输出终端）",
        &[
            param("layer", "输入图层", ParamKind::Layer, true, "", ""),
            param(
                "kind",
                "类别",
                ParamKind::Enum(&[("length", "长度"), ("area", "面积")]),
                true,
                "",
                "长度",
            ),
        ],
        true,
    ),
];

/// 按 id 找工具。
pub fn find(id: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.id == id)
}

// ===== 参数校验（纯函数）=====

/// 校验参数值（与注册表对齐）：必填非空、数值可解析、枚举标签合法、表达式非空。
/// 返回首个错误（中文）。
pub fn validate(def: &ToolDef, values: &[String]) -> Result<(), String> {
    if values.len() != def.params.len() {
        return Err(format!(
            "参数个数不符（{} 个值 / {} 个参数）",
            values.len(),
            def.params.len()
        ));
    }
    for (p, v) in def.params.iter().zip(values) {
        let v = v.trim();
        if p.required && v.is_empty() {
            return Err(format!("「{}」为必填参数", p.label));
        }
        if v.is_empty() {
            continue; // 可选参数留空 = 取默认语义
        }
        match &p.kind {
            ParamKind::Number => {
                if v.parse::<f64>().map(|f| !f.is_finite()).unwrap_or(true) {
                    return Err(format!("「{}」须为数值: {v}", p.label));
                }
            }
            ParamKind::NumberList => {
                parse_number_list(v).map_err(|e| format!("「{}」{e}", p.label))?;
            }
            ParamKind::Enum(options) if !options.iter().any(|(_, label)| *label == v) => {
                return Err(format!("「{}」取值非法: {v}", p.label));
            }
            _ => {}
        }
    }
    Ok(())
}

/// 取参数值（按 key；调用前须已过 validate）。
fn value_of(def: &ToolDef, values: &[String], key: &str) -> String {
    def.params
        .iter()
        .zip(values)
        .find(|(p, _)| p.key == key)
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default()
}

/// 枚举参数的中文标签 → 内核值。
fn enum_value(def: &ToolDef, values: &[String], key: &str) -> String {
    let label = value_of(def, values, key);
    def.params
        .iter()
        .find(|p| p.key == key)
        .and_then(|p| match &p.kind {
            ParamKind::Enum(options) => options
                .iter()
                .find(|(_, l)| *l == label)
                .map(|(v, _)| v.to_string()),
            _ => None,
        })
        .unwrap_or(label)
}

// ===== 纯解析/校验辅助（注册表参数 → 内核入参）=====

/// 解析数值列表（逗号分隔）：非空、全部有限、非负、严格递增（多环缓冲语义）。
pub fn parse_number_list(raw: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        let v: f64 = part
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("须为逗号分隔的数值列表，遇到: '{part}'"))?;
        if v < 0.0 {
            return Err(format!("距离须非负: {v}"));
        }
        if let Some(&prev) = out.last() {
            if v <= prev {
                return Err(format!("距离列表须严格递增: {prev} 之后出现 {v}"));
            }
        }
        out.push(v);
    }
    if out.is_empty() {
        return Err("距离列表不能为空".to_string());
    }
    Ok(out)
}

/// 解析范围四数 "minx,miny,maxx,maxy"（创建网格）。
pub fn parse_extent(raw: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = raw.split(',').map(|s| s.trim()).collect();
    if parts.len() != 4 {
        return Err(format!("范围须为 minx,miny,maxx,maxy 四个数值: '{raw}'"));
    }
    let mut out = [0.0; 4];
    for (i, p) in parts.iter().enumerate() {
        out[i] = p
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("范围第 {} 个数须为数值: '{p}'", i + 1))?;
    }
    if out[0] >= out[2] || out[1] >= out[3] {
        return Err(format!("范围须满足 min<max: '{raw}'"));
    }
    Ok(out)
}

/// 解析"须为正数"的数值参数（凹度/间距/格距等）。
pub fn parse_positive(raw: &str, label: &str) -> Result<f64, String> {
    let v: f64 = raw
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{label}须为数值: {raw}"))?;
    if v <= 0.0 {
        return Err(format!("{label}须为正数: {v}"));
    }
    Ok(v)
}

// ===== 执行 =====

/// 工具产出（app 结算）。
#[derive(Debug)]
pub enum ToolOutcome {
    /// 新图层（要素集合 + 命名前缀 + 中文动词）。
    NewLayer {
        collection: FeatureCollection,
        base: String,
        verb: String,
    },
    /// 多产出新图层（如分割矢量图层：逐组一个，base 已含组值）。
    NewLayers {
        layers: Vec<(String, FeatureCollection)>,
        verb: String,
    },
    /// 终端报告（统计/检查类）。
    Report(String),
    /// 导出（交给 app::op_export，格式按扩展名）。
    Export { layer: String, out: String },
}

/// 统一执行入口。图层访问经 `get_layer(id)` 注入（返回图层要素集合的克隆）。
pub fn run_tool(
    id: &str,
    values: &[String],
    get_layer: impl Fn(&str) -> Option<FeatureCollection>,
) -> Result<ToolOutcome, String> {
    let def = find(id).ok_or_else(|| format!("未知工具: {id}"))?;
    validate(def, values)?;
    // 取图层（FeatureCollection 克隆——内核函数均为只读消费）。
    let layer = |key: &str| -> Result<FeatureCollection, String> {
        let lid = value_of(def, values, key);
        get_layer(&lid).ok_or_else(|| format!("图层不存在: {lid}"))
    };
    let new_layer = |collection: FeatureCollection, base: String| {
        Ok(ToolOutcome::NewLayer {
            collection,
            base,
            verb: def.name.to_string(),
        })
    };
    let src = value_of(def, values, "layer");
    match id {
        "buffer" => {
            let d: f64 = value_of(def, values, "distance")
                .parse()
                .map_err(|_| "距离须为数值".to_string())?;
            let c = analysis::buffer(&layer("layer")?, d, 16).map_err(|e| e.to_string())?;
            new_layer(c, format!("buf_{src}"))
        }
        // 裁剪/差值/对称差/联合 → 同一内核 overlay（op 不同）。
        "overlay_union" | "overlay_intersection" | "overlay_difference" | "overlay_xor" => {
            let op = match id {
                "overlay_union" => analysis::OverlayOp::Union,
                "overlay_intersection" => analysis::OverlayOp::Intersection,
                "overlay_difference" => analysis::OverlayOp::Difference,
                _ => analysis::OverlayOp::Xor,
            };
            let c = analysis::overlay(&layer("layer")?, &layer("overlay")?, op)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("ov_{src}"))
        }
        "sjoin" => {
            let pred: analysis::SpatialPredicate = enum_value(def, values, "predicate")
                .parse()
                .map_err(|e: kanyu_core::KanyuError| e.to_string())?;
            let c = analysis::sjoin(&layer("layer")?, &layer("join")?, pred)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("sj_{src}"))
        }
        "count_points_in_polygon" => {
            let c = geoprocess::count_points_in_polygon(&layer("layer")?, &layer("points")?)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("cnt_{src}"))
        }
        "mean_coordinates" => {
            let w = value_of(def, values, "weight");
            let c = geoprocess::mean_coordinates(
                &layer("layer")?,
                if w.is_empty() { None } else { Some(w.as_str()) },
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("mean_{src}"))
        }
        "distance_matrix" => {
            let m = geoprocess::distance_matrix(&layer("layer")?, &layer("layer2")?)
                .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "距离矩阵 {} × {}（米）:\n{}",
                value_of(def, values, "layer"),
                value_of(def, values, "layer2"),
                serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?
            )))
        }
        "nearest_neighbor" => {
            let r = geoprocess::nearest_neighbor(&layer("layer")?).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "最近邻分析 {src}:\n{}",
                serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
            )))
        }
        "multi_ring_buffer" => {
            let distances = parse_number_list(&value_of(def, values, "distances"))?;
            let c = geoprocess::multi_ring_buffer(&layer("layer")?, &distances)
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("rings_{src}"))
        }
        "variable_buffer" => {
            let segments = parse_positive(&value_of(def, values, "segments"), "圆弧分段数")? as u32;
            let c = geoprocess::variable_buffer(
                &layer("layer")?,
                &value_of(def, values, "field"),
                segments,
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("vbuf_{src}"))
        }
        "dissolve" => {
            let f = value_of(def, values, "field");
            let c = geoprocess::dissolve(
                &layer("layer")?,
                if f.is_empty() { None } else { Some(f.as_str()) },
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("dis_{src}"))
        }
        "centroid" => {
            let c = geoprocess::centroid(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("cen_{src}"))
        }
        "convex_hull" => {
            let c = geoprocess::convex_hull(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("hull_{src}"))
        }
        "simplify" => {
            let t: f64 = value_of(def, values, "tolerance")
                .parse()
                .map_err(|_| "容差须为数值".to_string())?;
            let c = geoprocess::simplify(&layer("layer")?, t).map_err(|e| e.to_string())?;
            new_layer(c, format!("sim_{src}"))
        }
        "delete_holes" => {
            let raw = value_of(def, values, "min_area");
            let min = if raw.is_empty() {
                None
            } else {
                Some(
                    raw.parse::<f64>()
                        .map_err(|_| "最小面积须为数值".to_string())?,
                )
            };
            let c = geoprocess::delete_holes(&layer("layer")?, min).map_err(|e| e.to_string())?;
            new_layer(c, format!("dh_{src}"))
        }
        "explode" => {
            let c = geoprocess::explode(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("exp_{src}"))
        }
        "boundary" => {
            let c = geoprocess::boundary(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("bnd_{src}"))
        }
        "bounding_boxes" => {
            let c = geoprocess::bounding_boxes(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("bbox_{src}"))
        }
        "points_along_lines" => {
            let d = parse_positive(&value_of(def, values, "distance"), "间距")?;
            let c =
                geoprocess::points_along_lines(&layer("layer")?, d).map_err(|e| e.to_string())?;
            new_layer(c, format!("pal_{src}"))
        }
        "concave_hull" => {
            let k = parse_positive(&value_of(def, values, "concavity"), "凹度")?;
            let c = geoprocess::concave_hull(&layer("layer")?, k).map_err(|e| e.to_string())?;
            new_layer(c, format!("chull_{src}"))
        }
        "minimum_rotated_rect" => {
            let c =
                geoprocess::minimum_rotated_rect(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("mrr_{src}"))
        }
        "topology_check" => {
            let report =
                analysis::topology_check(&layer("layer")?, &[analysis::TopologyRule::NoOverlap])
                    .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "拓扑检查 {src}:\n{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            )))
        }
        "extract_by_attribute" => {
            let c =
                geoprocess::extract_by_attribute(&layer("layer")?, &value_of(def, values, "expr"))
                    .map_err(|e| e.to_string())?;
            new_layer(c, format!("xat_{src}"))
        }
        "extract_by_location" => {
            let c = geoprocess::extract_by_location(
                &layer("layer")?,
                &layer("mask")?,
                &enum_value(def, values, "predicate"),
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("xloc_{src}"))
        }
        "query" => {
            // Layer::query 为方法：以临时 Layer 包装集合调用。
            let tmp = Layer::from_collection("q".to_string(), layer("layer")?);
            let c = tmp
                .query(&value_of(def, values, "expr"))
                .map_err(|e| e.to_string())?;
            new_layer(c, format!("q_{src}"))
        }
        "merge" => {
            let (a, b) = (layer("layer")?, layer("layer2")?);
            let c = geoprocess::merge(&[&a, &b]).map_err(|e| e.to_string())?;
            new_layer(c, format!("mrg_{src}"))
        }
        "split_by_field" => {
            let groups =
                geoprocess::split_by_field(&layer("layer")?, &value_of(def, values, "field"))
                    .map_err(|e| e.to_string())?;
            // 多产出：每组一个新图层（split_源图层_组值；空串组用「空值」）。
            let layers = groups
                .into_iter()
                .map(|(g, c)| {
                    let g = if g.is_empty() {
                        "空值".to_string()
                    } else {
                        g
                    };
                    (format!("split_{src}_{g}"), c)
                })
                .collect();
            Ok(ToolOutcome::NewLayers {
                layers,
                verb: def.name.to_string(),
            })
        }
        "add_geometry_attributes" => {
            let c =
                geoprocess::add_geometry_attributes(&layer("layer")?).map_err(|e| e.to_string())?;
            new_layer(c, format!("gattr_{src}"))
        }
        "create_grid" => {
            let extent = parse_extent(&value_of(def, values, "extent"))?;
            let cell = parse_positive(&value_of(def, values, "cell_size"), "格距")?;
            let c = geoprocess::create_grid(extent, cell).map_err(|e| e.to_string())?;
            new_layer(c, "grid".to_string())
        }
        "reproject" => {
            let c = crs::reproject(
                &layer("layer")?,
                &value_of(def, values, "from"),
                &value_of(def, values, "to"),
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("rp_{src}"))
        }
        "export" => Ok(ToolOutcome::Export {
            layer: src,
            out: value_of(def, values, "out"),
        }),
        "zonal_stats" => {
            let stats: Result<Vec<_>, _> = value_of(def, values, "stats")
                .split(',')
                .map(|s| s.trim().parse::<analysis::ZonalStat>())
                .collect();
            let stats = stats.map_err(|e: kanyu_core::KanyuError| e.to_string())?;
            let c = analysis::zonal_stats(
                &layer("zones")?,
                &layer("values")?,
                &value_of(def, values, "field"),
                &stats,
            )
            .map_err(|e| e.to_string())?;
            new_layer(c, format!("zs_{}", value_of(def, values, "zones")))
        }
        "stats" => {
            let s = geoprocess::stats(&layer("layer")?).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "图层统计 {src}:\n{}",
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
            )))
        }
        "field_stats" => {
            let s = geoprocess::field_stats(&layer("layer")?, &value_of(def, values, "field"))
                .map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "字段统计 {src}.{}:\n{}",
                value_of(def, values, "field"),
                serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?
            )))
        }
        "measure" => {
            let kind: crs::MeasureKind = enum_value(def, values, "kind")
                .parse()
                .map_err(|e: kanyu_core::KanyuError| e.to_string())?;
            let report = crs::measure(&layer("layer")?, kind).map_err(|e| e.to_string())?;
            Ok(ToolOutcome::Report(format!(
                "测地度量 {src}:\n{}",
                serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
            )))
        }
        other => Err(format!("工具未实现: {other}")),
    }
}

// ===== 参数对话框 =====

/// 参数对话框状态（工具 + 当前值 + 校验错误）。
pub struct ToolRunState {
    /// 工具定义。
    pub tool: &'static ToolDef,
    /// 当前参数值（与 tool.params 对齐）。
    pub values: Vec<String>,
    /// 校验/执行错误（error_caption 红字）。
    pub err: Option<String>,
}

impl ToolRunState {
    /// 以默认值初始化。
    pub fn new(tool: &'static ToolDef) -> Self {
        Self {
            tool,
            values: tool.params.iter().map(|p| p.default.to_string()).collect(),
            err: None,
        }
    }
}

/// 通用参数表单（按注册表参数表生成；图层下拉/字段联动/枚举中文选项）。
/// `fields_of(layer_id)` 注入图层字段清单查询（Field 参数选项来源）。
pub fn run_form(
    ui: &mut egui::Ui,
    st: &mut ToolRunState,
    layer_ids: &[String],
    fields_of: &dyn Fn(&str) -> Vec<String>,
) {
    hint_caption(ui, st.tool.desc);
    ui.add_space(4.0);
    let layer_ids = layer_ids.to_vec();
    for i in 0..st.tool.params.len() {
        let p = &st.tool.params[i];
        match &p.kind {
            ParamKind::Layer => {
                layer_picker(ui, p.label, &mut st.values[i], &layer_ids, true);
            }
            ParamKind::Field(layer_idx) => {
                // 字段选项随其图层参数的当前选择联动。
                let layer_id = st.values.get(*layer_idx).cloned().unwrap_or_default();
                let fields = fields_of(&layer_id);
                if fields.is_empty() {
                    text_input(ui, p.label, &mut st.values[i], p.hint, true);
                } else {
                    crate::ui_kit::combo(ui, p.label, &mut st.values[i], &fields, true);
                }
            }
            ParamKind::Enum(options) => {
                let labels: Vec<&str> = options.iter().map(|(_, l)| *l).collect();
                combo_static(ui, p.label, &mut st.values[i], &labels, true);
            }
            ParamKind::Number | ParamKind::NumberList | ParamKind::Text | ParamKind::Expression => {
                text_input(ui, p.label, &mut st.values[i], p.hint, true);
            }
        }
    }
    if let Some(e) = &st.err {
        error_caption(ui, e);
    }
}

// ===== 面板（分类树 + 筛选）=====

/// 工具箱面板状态。
#[derive(Default)]
pub struct ToolboxPanel {
    /// 筛选框（按中文名/说明过滤）。
    filter: String,
    /// 分类折叠态（默认全展开）。
    collapsed: [bool; 5],
}

impl ToolboxPanel {
    /// 面板 UI；返回待运行的工具 id（双击工具行或右键「运行」）。
    pub fn ui(&mut self, ui: &mut egui::Ui, cache: &mut icons::IconCache) -> Option<&'static str> {
        let mut run = None;
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .desired_width(f32::INFINITY)
                .hint_text("筛选工具…"),
        );
        ui.separator();
        let filter = self.filter.trim().to_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for cat in ToolCategory::ALL {
                let tools: Vec<&ToolDef> = TOOLS
                    .iter()
                    .filter(|t| t.category == cat)
                    .filter(|t| {
                        filter.is_empty()
                            || t.name.to_lowercase().contains(&filter)
                            || t.desc.to_lowercase().contains(&filter)
                    })
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
                for t in tools {
                    let (resp, _) =
                        tree_row(ui, cache, 1, Some(Icon::Play), t.name, None, |_ui| {});
                    let tip = if t.report {
                        format!("{}\n结果输出终端；双击运行", t.desc)
                    } else {
                        format!("{}\n双击运行", t.desc)
                    };
                    let resp = resp.on_hover_text(tip);
                    if resp.double_clicked() {
                        run = Some(t.id);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("运行").clicked() {
                            run = Some(t.id);
                            ui.close();
                        }
                    });
                }
            }
        });
        run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表完整性：id 唯一、分类齐备、必填参数类型合法。
    #[test]
    fn registry_is_consistent() {
        let mut ids = std::collections::HashSet::new();
        for t in TOOLS {
            assert!(ids.insert(t.id), "工具 id 重复: {}", t.id);
            assert!(
                !t.name.is_empty() && !t.desc.is_empty(),
                "{} 缺名称/说明",
                t.id
            );
            // 图层参数必存在（创建网格为无输入图层的生成器，豁免）。
            assert!(
                t.id == "create_grid"
                    || t.params.iter().any(|p| matches!(p.kind, ParamKind::Layer)),
                "{} 无输入图层参数",
                t.id
            );
            for p in t.params {
                if let ParamKind::Field(idx) = p.kind {
                    assert!(
                        matches!(t.params[idx].kind, ParamKind::Layer),
                        "{}.{} 的 Field 锚点不是图层参数",
                        t.id,
                        p.key
                    );
                }
                if let ParamKind::Enum(options) = p.kind {
                    assert!(!options.is_empty(), "{}.{} 枚举为空", t.id, p.key);
                    if !p.default.is_empty() {
                        assert!(
                            options.iter().any(|(_, l)| *l == p.default),
                            "{}.{} 默认值不在枚举标签内",
                            t.id,
                            p.key
                        );
                    }
                }
            }
        }
    }

    /// 工具总数与分类计数（防手滑删工具）。
    #[test]
    fn registry_size() {
        assert_eq!(TOOLS.len(), 37);
        let count = |c: ToolCategory| TOOLS.iter().filter(|t| t.category == c).count();
        assert_eq!(count(ToolCategory::Analysis), 12);
        assert_eq!(count(ToolCategory::Geometry), 12);
        assert_eq!(count(ToolCategory::Selection), 3);
        assert_eq!(count(ToolCategory::DataManagement), 6);
        assert_eq!(count(ToolCategory::Statistics), 4);
    }

    /// 校验：必填留空报错。
    #[test]
    fn validate_required() {
        let buf = find("buffer").unwrap();
        let err = validate(buf, &[String::new(), "100".into()]).unwrap_err();
        assert!(err.contains("必填"));
        assert!(validate(buf, &["buildings".into(), "100".into()]).is_ok());
    }

    /// 校验：数值参数拒绝非数值；可选数值留空放行。
    #[test]
    fn validate_number() {
        let buf = find("buffer").unwrap();
        assert!(validate(buf, &["a".into(), "abc".into()]).is_err());
        let dh = find("delete_holes").unwrap();
        assert!(validate(dh, &["a".into(), String::new()]).is_ok());
        assert!(validate(dh, &["a".into(), "12.5".into()]).is_ok());
        assert!(validate(dh, &["a".into(), "xyz".into()]).is_err());
    }

    /// 校验：枚举只收中文标签；枚举值映射回内核值。
    #[test]
    fn validate_enum_and_mapping() {
        let sj = find("sjoin").unwrap();
        assert!(validate(sj, &["a".into(), "b".into(), "相交".into()]).is_ok());
        assert!(validate(sj, &["a".into(), "b".into(), "intersects".into()]).is_err());
        assert_eq!(
            enum_value(sj, &["a".into(), "b".into(), "相交".into()], "predicate"),
            "intersects"
        );
    }

    /// 校验：参数个数不符报错。
    #[test]
    fn validate_arity() {
        let cen = find("centroid").unwrap();
        assert!(validate(cen, &[]).is_err());
        assert!(validate(cen, &["a".into()]).is_ok());
    }

    /// 执行：未知工具与缺失图层的中文错误。
    #[test]
    fn run_tool_errors() {
        assert!(run_tool("nope", &[], |_| None)
            .unwrap_err()
            .contains("未知工具"));
        let cen = find("centroid").unwrap();
        let err = run_tool("centroid", &["ghost".into()], |_| None).unwrap_err();
        assert!(err.contains("图层不存在"), "{err}");
        let _ = cen;
    }

    /// 执行：report 类工具走终端报告分支（以空集合驱动 stats）。
    #[test]
    fn run_tool_report_branch() {
        let empty = FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        let out = run_tool("stats", &["x".into()], |_| Some(empty.clone())).unwrap();
        match out {
            ToolOutcome::Report(text) => assert!(text.contains("图层统计"), "{text}"),
            _ => panic!("stats 应为终端报告"),
        }
    }

    /// 执行：新图层分支命名前缀（以空集合驱动 centroid）。
    #[test]
    fn run_tool_new_layer_branch() {
        let empty = FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        let out = run_tool("centroid", &["buildings".into()], |_| Some(empty.clone())).unwrap();
        match out {
            ToolOutcome::NewLayer { base, verb, .. } => {
                assert_eq!(base, "cen_buildings");
                assert_eq!(verb, "质心");
            }
            _ => panic!("centroid 应为新图层"),
        }
    }

    /// 纯解析：数值列表（非空/非负/严格递增）。
    #[test]
    fn number_list_rules() {
        assert_eq!(
            parse_number_list("100, 200,300").unwrap(),
            vec![100.0, 200.0, 300.0]
        );
        assert!(parse_number_list("").is_err()); // 空
        assert!(parse_number_list("100,abc").is_err()); // 非数值
        assert!(parse_number_list("100,-5").is_err()); // 负数
        assert!(parse_number_list("100,100").is_err()); // 非严格递增
        assert!(parse_number_list("200,100").is_err());
        // validate 集成：多环缓冲区的距离列表走同一规则。
        let mrb = find("multi_ring_buffer").unwrap();
        assert!(validate(mrb, &["a".into(), "1,2,3".into()]).is_ok());
        assert!(validate(mrb, &["a".into(), "3,2".into()]).is_err());
    }

    /// 纯解析：范围四数与正数参数。
    #[test]
    fn extent_and_positive_rules() {
        assert_eq!(parse_extent("0,0, 10, 20").unwrap(), [0.0, 0.0, 10.0, 20.0]);
        assert!(parse_extent("0,0,10").is_err()); // 个数不足
        assert!(parse_extent("0,0,10,x").is_err()); // 非数值
        assert!(parse_extent("10,0,0,20").is_err()); // min>=max
        assert_eq!(parse_positive("2.0", "凹度").unwrap(), 2.0);
        assert!(parse_positive("0", "凹度").is_err());
        assert!(parse_positive("-1", "格距").is_err());
        assert!(parse_positive("abc", "间距").is_err());
        // run_tool 集成：凹度非正在执行期拦截。
        let empty = FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        let err = run_tool("concave_hull", &["a".into(), "0".into()], |_| {
            Some(empty.clone())
        })
        .unwrap_err();
        assert!(err.contains("凹度"), "{err}");
    }

    /// 执行：分割矢量图层走多产出分支；空集合 → 0 组。
    #[test]
    fn run_tool_split_multi_output() {
        let empty = FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        };
        let out = run_tool("split_by_field", &["a".into(), "usage".into()], |_| {
            Some(empty.clone())
        })
        .unwrap();
        match out {
            ToolOutcome::NewLayers { layers, verb } => {
                assert!(layers.is_empty());
                assert_eq!(verb, "分割矢量图层");
            }
            _ => panic!("split_by_field 应为多产出"),
        }
    }
}
