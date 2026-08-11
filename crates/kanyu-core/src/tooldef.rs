//! 工具定义注册表（纯数据，零 UI 依赖）——QGIS Processing 式工具面的
//! **单一事实来源**，自 kanyu-shell 下沉：一处声明、三面投影
//! （shell 工具箱面板 / kanyu-py SDK / MCP 工具面（后续））。
//!
//! 语义映射：裁剪 = overlay intersection、差值 = difference 等——独立中文
//! 工具名映射到同一内核函数（执行分支见 [`crate::toolrun`]）。
//! 加工具 = 本表加一行 + `toolrun.rs` 加分支。

use serde::Serialize;

// ===== 注册表类型 =====

/// 参数类型（serde Serialize 供 SDK JSON 投影消费）。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
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
#[derive(Clone, Copy, Debug, Serialize)]
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
    /// 参数帮助（ArcGIS Pro 式焦点说明区文案）。
    pub help: &'static str,
}

const fn param(
    key: &'static str,
    label: &'static str,
    kind: ParamKind,
    required: bool,
    hint: &'static str,
    default: &'static str,
    help: &'static str,
) -> ToolParam {
    ToolParam {
        key,
        label,
        kind,
        required,
        hint,
        default,
        help,
    }
}

/// 工具分类（QGIS 分组，中文）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
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

    /// 下标（折叠状态表等 UI 侧定长索引用）。
    pub fn index(self) -> usize {
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
#[derive(Clone, Copy, Debug, Serialize)]
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要生成缓冲区的图层。",
            ),
            param(
                "distance",
                "距离",
                ParamKind::Number,
                true,
                "CRS 单位（投影后为米）",
                "",
                "缓冲半径。EPSG:4326 下单位为度，米制缓冲请先「投影变换」。",
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
            param(
                "layer",
                "目标图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "参与并集的第一个面图层。",
            ),
            param(
                "overlay",
                "叠加图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "参与并集的第二个面图层。",
            ),
        ],
        false,
    ),
    tool(
        "overlay_intersection",
        "裁剪",
        ToolCategory::Analysis,
        "以叠加图层范围裁剪目标图层（overlay intersection）",
        &[
            param(
                "layer",
                "目标图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "被裁剪的图层（保留其属性）。",
            ),
            param(
                "overlay",
                "裁剪范围（面）",
                ParamKind::Layer,
                true,
                "",
                "",
                "裁剪范围面图层；只保留落入其内的部分。",
            ),
        ],
        false,
    ),
    tool(
        "overlay_difference",
        "差值",
        ToolCategory::Analysis,
        "目标图层减去叠加图层（overlay difference）",
        &[
            param(
                "layer",
                "目标图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "被减的图层。",
            ),
            param(
                "overlay",
                "减去部分（面）",
                ParamKind::Layer,
                true,
                "",
                "",
                "要从目标中减去的范围。",
            ),
        ],
        false,
    ),
    tool(
        "overlay_xor",
        "对称差",
        ToolCategory::Analysis,
        "两面图层对称差（overlay xor）",
        &[
            param(
                "layer",
                "目标图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "参与对称差的第一个面图层。",
            ),
            param(
                "overlay",
                "叠加图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "参与对称差的第二个面图层。",
            ),
        ],
        false,
    ),
    tool(
        "sjoin",
        "空间连接",
        ToolCategory::Analysis,
        "按空间谓词合并两图层属性（左连接 + explode）",
        &[
            param(
                "layer",
                "目标图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "保留全部要素的一侧（左连接）。",
            ),
            param(
                "join",
                "连接图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "属性被并入的一侧（键冲突加 join_ 前缀）。",
            ),
            param(
                "predicate",
                "谓词",
                ParamKind::Enum(PREDICATES),
                true,
                "",
                "相交",
                "空间匹配条件：相交/包含/位于…内。",
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
            param(
                "layer",
                "面图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "被统计的分区面图层。",
            ),
            param(
                "points",
                "点图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "落入面内即计数（含边界）。",
            ),
        ],
        false,
    ),
    tool(
        "mean_coordinates",
        "平均坐标",
        ToolCategory::Analysis,
        "要素质心的（加权）平均坐标点",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "取其要素质心参与平均。",
            ),
            param(
                "weight",
                "权重字段",
                ParamKind::Field(0),
                false,
                "可空 = 等权",
                "",
                "数值字段作权重；留空则每个要素等权。",
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
            param(
                "layer",
                "图层 A",
                ParamKind::Layer,
                true,
                "",
                "",
                "矩阵行（点取坐标，非点取质心）。",
            ),
            param(
                "layer2",
                "图层 B",
                ParamKind::Layer,
                true,
                "",
                "",
                "矩阵列。完整矩阵以 JSON 输出到终端。",
            ),
        ],
        true,
    ),
    tool(
        "nearest_neighbor",
        "最近邻分析",
        ToolCategory::Analysis,
        "点集最近邻指数（<1 聚集 / ≈1 随机 / >1 离散，输出终端）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "点图层（非点取质心）；统计报告输出到终端。",
        )],
        true,
    ),
    tool(
        "multi_ring_buffer",
        "多环缓冲区",
        ToolCategory::Analysis,
        "按距离列表生成多环缓冲（环带 RING/DISTANCE 属性）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要生成环带的图层。",
            ),
            param(
                "distances",
                "距离列表",
                ParamKind::NumberList,
                true,
                "逗号分隔，严格递增非负，如 100,200,300",
                "",
                "各环外缘距离（CRS 单位），须严格递增且非负。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "逐要素按字段值缓冲。",
            ),
            param(
                "field",
                "距离字段",
                ParamKind::Field(0),
                true,
                "",
                "",
                "数值字段，值为缓冲半径（CRS 单位）。",
            ),
            param(
                "segments",
                "圆弧分段数",
                ParamKind::Number,
                true,
                "≥1",
                "16",
                "圆弧逼近分段数，越大越圆滑。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要融合的面图层。",
            ),
            param(
                "field",
                "分组字段",
                ParamKind::Field(0),
                false,
                "可空 = 全部融合为一",
                "",
                "同值要素并为一组；留空则整层并为一个要素。",
            ),
        ],
        false,
    ),
    tool(
        "centroid",
        "质心",
        ToolCategory::Geometry,
        "逐要素几何质心（点图层）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "逐要素求几何质心（可能落在凹面外）。",
        )],
        false,
    ),
    tool(
        "convex_hull",
        "凸包",
        ToolCategory::Geometry,
        "全图层要素的总凸包（单面要素）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "整层要素的最小凸多边形。",
        )],
        false,
    ),
    tool(
        "simplify",
        "简化",
        ToolCategory::Geometry,
        "Douglas-Peucker 几何简化（容差为 CRS 单位）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要简化折线/面的图层。",
            ),
            param(
                "tolerance",
                "容差",
                ParamKind::Number,
                true,
                "如 0.001",
                "",
                "道格拉斯-普克容差（CRS 单位）；退化要素剔除。",
            ),
        ],
        false,
    ),
    tool(
        "delete_holes",
        "删洞",
        ToolCategory::Geometry,
        "删除面要素内环（可设最小面积阈值）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "含洞面图层。",
            ),
            param(
                "min_area",
                "最小面积",
                ParamKind::Number,
                false,
                "可空 = 删除全部洞",
                "",
                "仅删面积小于阈值的洞；留空删除全部洞。",
            ),
        ],
        false,
    ),
    tool(
        "explode",
        "炸开多部件",
        ToolCategory::Geometry,
        "Multi* → 单部件逐要素（属性复制）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "MultiPoint/MultiLineString/MultiPolygon 拆为单部件要素。",
        )],
        false,
    ),
    tool(
        "boundary",
        "边界",
        ToolCategory::Geometry,
        "面转边界线 / 开放线转端点",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "面 → 边界环转线；开放线 → 首尾端点；闭合线与点跳过。",
        )],
        false,
    ),
    tool(
        "bounding_boxes",
        "包络矩形",
        ToolCategory::Geometry,
        "逐要素外包矩形（面图层）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "逐要素轴对齐外包矩形。",
        )],
        false,
    ),
    tool(
        "points_along_lines",
        "沿线等距点",
        ToolCategory::Geometry,
        "沿折线按测地线间距生成里程点（DISTANCE 属性，米）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "线图层。",
            ),
            param(
                "distance",
                "间距（米）",
                ParamKind::Number,
                true,
                ">0",
                "",
                "相邻点的测地线里程间隔（米）。",
            ),
        ],
        false,
    ),
    tool(
        "concave_hull",
        "凹包",
        ToolCategory::Geometry,
        "整层要素的凹包（单面）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "整层要素坐标集的凹包。",
            ),
            param(
                "concavity",
                "凹度",
                ParamKind::Number,
                true,
                ">0，越大越凹",
                "2.0",
                "凹度系数（>0）：越大结果越贴合点集。",
            ),
        ],
        false,
    ),
    tool(
        "minimum_rotated_rect",
        "定向最小包络矩形",
        ToolCategory::Geometry,
        "逐要素最小旋转外接矩形（面图层）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "逐要素最小面积旋转矩形。",
        )],
        false,
    ),
    tool(
        "topology_check",
        "拓扑检查",
        ToolCategory::Geometry,
        "no_overlap 规则检查（报告输出终端）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "检查面要素相互重叠（违规清单输出终端）。",
        )],
        true,
    ),
    // —— 矢量选择 ——
    tool(
        "extract_by_attribute",
        "按属性提取",
        ToolCategory::Selection,
        "按属性表达式提取要素为新图层",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "从中提取的图层。",
            ),
            param(
                "expr",
                "表达式",
                ParamKind::Expression,
                true,
                "如 height > 50",
                "",
                "形如 field op value；op 支持 =/!=/>/>=/</<=/contains。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "从中提取的图层。",
            ),
            param(
                "mask",
                "掩膜图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "与掩膜任一要素满足谓词即提取。",
            ),
            param(
                "predicate",
                "谓词",
                ParamKind::Enum(PREDICATES),
                true,
                "",
                "相交",
                "空间关系：相交/包含/位于…内。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要查询的图层。",
            ),
            param(
                "expr",
                "表达式",
                ParamKind::Expression,
                true,
                "如 usage == residential",
                "",
                "属性过滤表达式（Layer::query 语义）。",
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
            param(
                "layer",
                "图层 A",
                ParamKind::Layer,
                true,
                "",
                "",
                "要素排在前。",
            ),
            param(
                "layer2",
                "图层 B",
                ParamKind::Layer,
                true,
                "",
                "",
                "要素排在后；属性原样保留。",
            ),
        ],
        false,
    ),
    tool(
        "split_by_field",
        "分割矢量图层",
        ToolCategory::DataManagement,
        "按字段值拆分为多图层（每组一个，split_源图层_组值）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要拆分的图层。",
            ),
            param(
                "field",
                "分组字段",
                ParamKind::Field(0),
                true,
                "",
                "",
                "每个不同字段值产出一个新图层。",
            ),
        ],
        false,
    ),
    tool(
        "add_geometry_attributes",
        "添加几何属性",
        ToolCategory::DataManagement,
        "追加几何量属性列（AREA_M2/PERIMETER_M/LENGTH_M，测地口径）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "面加面积/周长（㎡/m），线加长度（m）。",
        )],
        false,
    ),
    tool(
        "create_grid",
        "创建网格",
        ToolCategory::DataManagement,
        "按范围与格距生成规则格网（ROW/COL 属性）",
        &[
            param(
                "extent",
                "范围",
                ParamKind::Text,
                true,
                "minx,miny,maxx,maxy（默认已填当前数据范围）",
                "",
                "格网覆盖范围四数（CRS 单位），默认取当前数据范围。",
            ),
            param(
                "cell_size",
                "格距",
                ParamKind::Number,
                true,
                "CRS 单位，>0",
                "",
                "单元格边长（CRS 单位）。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要变换的图层。",
            ),
            param(
                "from",
                "源 CRS",
                ParamKind::Text,
                true,
                "如 EPSG:4326",
                "EPSG:4326",
                "数据当前坐标系，如 EPSG:4326。",
            ),
            param(
                "to",
                "目标 CRS",
                ParamKind::Text,
                true,
                "默认 = 工程坐标系",
                "",
                "目标坐标系；默认取「设置 → 坐标系」的工程坐标系。",
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
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "要导出的图层。",
            ),
            param(
                "out",
                "输出路径",
                ParamKind::Text,
                true,
                "如 out.fgb",
                "",
                "格式按扩展名推断。",
            ),
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
            param(
                "zones",
                "分区（面）图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "统计结果列写回该图层。",
            ),
            param(
                "values",
                "值图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "按代表点归属分区。",
            ),
            param(
                "field",
                "数值字段",
                ParamKind::Field(1),
                true,
                "",
                "",
                "值图层中被统计的数值字段。",
            ),
            param(
                "stats",
                "统计项",
                ParamKind::Text,
                true,
                "count,sum,mean,min,max",
                "count,sum,mean,min,max",
                "逗号分隔统计项。",
            ),
        ],
        false,
    ),
    tool(
        "stats",
        "图层统计",
        ToolCategory::Statistics,
        "要素数/总长/总面积（测地线口径，报告输出终端）",
        &[param(
            "layer",
            "输入图层",
            ParamKind::Layer,
            true,
            "",
            "",
            "统计报告输出到终端。",
        )],
        true,
    ),
    tool(
        "field_stats",
        "字段统计",
        ToolCategory::Statistics,
        "数值字段基本统计（报告输出终端）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "字段所在图层。",
            ),
            param(
                "field",
                "数值字段",
                ParamKind::Field(0),
                true,
                "",
                "",
                "计数/和/均值/最值/总体标准差等输出到终端。",
            ),
        ],
        true,
    ),
    tool(
        "measure",
        "测地度量",
        ToolCategory::Statistics,
        "Karney 2013 测地线长度/面积（米/平方米，输出终端）",
        &[
            param(
                "layer",
                "输入图层",
                ParamKind::Layer,
                true,
                "",
                "",
                "输入应为经纬度数据（如 EPSG:4326）。",
            ),
            param(
                "kind",
                "类别",
                ParamKind::Enum(&[("length", "长度"), ("area", "面积")]),
                true,
                "",
                "长度",
                "测地线总长或总面积。",
            ),
        ],
        true,
    ),
];

/// 按 id 找工具。
pub fn find(id: &str) -> Option<&'static ToolDef> {
    TOOLS.iter().find(|t| t.id == id)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表完整性：id 唯一、分类齐备、必填参数类型合法、帮助文案非空。
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
                assert!(!p.help.is_empty(), "{}.{} 缺参数帮助", t.id, p.key);
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

    /// SDK 投影：ToolDef 序列化为 JSON（含枚举/字段锚点/帮助文案）。
    #[test]
    fn tooldef_serializes_to_json() {
        let buf = find("buffer").unwrap();
        let json = serde_json::to_string(buf).unwrap();
        assert!(json.contains("\"id\":\"buffer\""));
        assert!(json.contains("缓冲区"));
        let sj = find("sjoin").unwrap();
        let v = serde_json::to_value(sj).unwrap();
        // 枚举参数选项在 JSON 中可见。
        let text = v.to_string();
        assert!(text.contains("intersects"));
        assert_eq!(v["category"], serde_json::json!("Analysis"));
        // 字段锚点（Field(0)）序列化。
        let fs = serde_json::to_string(find("field_stats").unwrap()).unwrap();
        assert!(fs.contains("Field"));
    }
}
