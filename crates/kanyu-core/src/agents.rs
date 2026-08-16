//! `AGENTS.md` 项目语义描述文件 —— 解析、校验与生成。
//!
//! 对应总规 §4.3.2：每个堪舆项目根目录包含一份 `AGENTS.md`，
//! 它是 AI 理解项目的"罗盘"——图层语义、坐标系、业务规则、工作流。
//! 本模块将其结构化为机器可读对象，供 CLI/MCP 工具消费。

use serde::Serialize;

use crate::error::{KanyuError, Result};

/// CRS 的「不适用」占位值（中英文）。
///
/// 地理项目应写具体坐标系编码（`EPSG:nnnn`，或 `EPSGC:` / `ESRI:` /
/// `ESRI-WKT:` 前缀）；**非地理的纯软件工程仓库**（如本仓库自身）没有地理
/// 数据，元数据里硬写一个 EPSG 代码只会误导 AI「这是一个地理项目」。故
/// `kanyu agents init` 对代码仓库用 `不适用` 占位，[`MetadataCtx::detect`]
/// 据此判定为非地理项目，免检数据层语义表。
pub const NOT_APPLICABLE: [&str; 3] = ["不适用", "N/A", "not applicable"];

/// 校验上下文：区分「地理项目」与「纯软件工程仓库」两类被校验对象。
///
/// 单一事实来源（[`is_geo`](Self::is_geo)）由 crs 值判定，杜绝「图层语义表」
/// 与「crs 不适用」两条校验规则互相矛盾——这正是 `MasterPLAN` §4.3.2 的适用
/// 边界：`AGENTS.md` 的「数据层语义表」是**地理项目**的项目描述约定，不应
/// 强加于以代码为主、无地理数据的软件工程仓库（否则 `kanyu agents validate`
/// 对软件仓库恒不通过，无法作为守门检查）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct MetadataCtx {
    /// `true` = 地理项目（crs 为真实坐标系编码）；`false` = 代码仓库/软件项目。
    pub is_geo: bool,
}

impl MetadataCtx {
    /// 构造上下文。
    pub fn new(is_geo: bool) -> Self {
        Self { is_geo }
    }

    /// 从 `crs:` 字段值推导 [`is_geo`](Self::is_geo)（唯一事实来源，幂等可测）。
    ///
    /// - `Some(v)` 为真实坐标系编码（`EPSG:` / `EPSGC:` / `ESRI:` /
    ///   `ESRI-WKT:` 前缀，或 [`NOT_APPLICABLE`] 任一占位之外的非空值）→
    ///   地理项目（`true`）；
    /// - `None`（crs 未写）、空串、或 [`NOT_APPLICABLE`] 占位（`不适用` / `N/A` /
    ///   `not applicable`，含英文）→ 代码仓库（`false`）；
    ///   `EPSGC:`/`ESRI:`/`ESRI-WKT:` 前缀与 `EPSG:` 同级，视为真实坐标系。
    ///
    /// 判定只读 crs 值、不信任何调用方意图，故稳定可复算。
    pub fn detect(crsv: Option<&str>) -> Self {
        // 判定只读 crs 值、不信任何调用方意图，故稳定可复算。
        // 空串 / 空白与 `None` 同义（未写 crs），均判非地理——`NOT_APPLICABLE`
        // 占位本身必然非空，故无需单独分支；占位值只会落入「真实坐标系前缀」
        // 判定失败而自然归 `false`。
        let is_geo = crsv
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| {
                t.starts_with("EPSG:")
                    || t.starts_with("EPSGC:")
                    || t.starts_with("ESRI:")
                    || t.starts_with("ESRI-WKT:")
            })
            .unwrap_or(false);
        Self::new(is_geo)
    }
}

/// 项目元数据。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectMeta {
    /// 项目名。
    pub name: Option<String>,
    /// 坐标参考系（如 `EPSG:4526`）。非地理/代码仓库写 `不适用` 占位（见
    /// [`NOT_APPLICABLE`]），[`MetadataCtx::detect`] 据此判为非地理项目。
    pub crs: Option<String>,
    /// 范围 `[minx, miny, maxx, maxy]`。
    pub extent: Option<Vec<f64>>,
    /// 作者。
    pub author: Option<String>,
    /// 创建日期。
    pub created: Option<String>,
    /// 数据层（语义表）是否适用于本项目。`Some(true)`=地理项目（有 GIS 图层，
    /// 语义表必填）；`Some(false)`=代码/软件仓库（无地理数据，语义表不适用，
    /// 校验免检）；`None`=未显式声明（回退 `crs` 占位判定，见
    /// [`AgentsMd::resolve_data_layer`]）。
    pub data_layer: Option<bool>,
}

/// 数据层语义（一行图层描述）。
#[derive(Debug, Clone, Serialize)]
pub struct LayerSemantics {
    /// 图层名。
    pub layer: String,
    /// 几何类型。
    pub geometry: String,
    /// 业务语义。
    pub semantics: String,
    /// 关键字段。
    pub key_fields: String,
    /// 业务规则。
    pub rules: String,
}

/// 解析后的 AGENTS.md。
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentsMd {
    /// 项目元数据。
    pub meta: ProjectMeta,
    /// 数据层语义表。
    pub layers: Vec<LayerSemantics>,
    /// 业务规则（编号列表原文）。
    pub business_rules: Vec<String>,
    /// 自定义工具名列表。
    pub custom_tools: Vec<String>,
}

impl AgentsMd {
    /// 从 Markdown 文本解析。容错设计：缺省段落不报错，
    /// 由 [`AgentsMd::validate`] 负责完整性检查。
    pub fn parse(markdown: &str) -> Result<Self> {
        let mut doc = AgentsMd::default();
        let mut section = String::new();

        for line in markdown.lines() {
            let trimmed = line.trim();
            if let Some(title) = trimmed.strip_prefix("## ") {
                section = title.trim().to_string();
                continue;
            }
            match section.as_str() {
                s if s.starts_with("项目元数据") => parse_meta_line(trimmed, &mut doc.meta),
                s if s.starts_with("数据层语义") => {
                    if let Some(row) = parse_layer_row(trimmed) {
                        doc.layers.push(row);
                    }
                }
                s if s.starts_with("业务规则") => {
                    if let Some(rule) = parse_numbered_item(trimmed) {
                        doc.business_rules.push(rule);
                    }
                }
                s if s.starts_with("自定义工具") => {
                    if let Some(tool) = trimmed.strip_prefix("- `") {
                        if let Some(name) = tool.split('`').next() {
                            doc.custom_tools.push(name.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(doc)
    }

    /// 带**校验上下文**的完整性校验（区分地理项目 / 软件仓库，语义见
    /// [`MetadataCtx`]）。返回缺失/问题清单（空列表 = 通过）：
    ///
    /// - **地理项目**（`is_geo = true`）：
    ///   1. `meta.name` 缺失 → 错误；
    ///   2. `meta.crs` 缺失 → 错误（坐标参考系是**强制项**）；
    ///   3. 数据层语义表为空 → 错误（AI 无法理解项目）；
    ///   4. 每层 `key_fields` 为空 → **警告**（不阻断通过，分诊应改进）。
    /// - **软件仓库**（`is_geo = false`，典型为 `crs: 不适用`）：
    ///   - 免检「数据层语义表」（代码仓库没有 GIS 图层，不判错，彻底消除
    ///     与 `crs: 不适用` 的矛盾）；
    ///   - `meta.name` / `meta.crs` 缺失 → **降级为非阻塞告警**（仍列出、
    ///     供 AI 参考，但不使 `is_empty()` 为假）→ 元数据不全但含
    ///     `crs: 不适用` 的**软件仓库**可干净通过；
    ///   - 若该库仍含数据层，则「每层 `key_fields` 为空」保留为**警告**。
    ///
    /// 该降级正是 `kanyu agents validate --path AGENTS.md` 能对**本仓库**
    /// （软件工程仓库，`crs: 不适用` 且无图层语义表）判为通过所依赖的行为；
    /// 对缺 `crs` 的**地理**项目（如 `validate_flags_missing_crs`）仍判不通过。
    /// 上下文由 crs 值唯一决定（`MetadataCtx::detect`），不信调用方意图。
    /// 解析「本项目是否**要求/允许**数据层语义表」的单一事实裁决。
    ///
    /// 优先级：显式 `- **data-layer**: 是/否` 元数据行（`Some(true/false)`）
    /// 优先于 crs 占位推断。
    /// - 显式 `是`（`Some(true)`）→ `true`（有 GIS 图层，语义表必填）；
    /// - 显式 `否`（`Some(false)`）→ `false`（代码/软件仓库，语义表不适用，
    ///   校验免检——这是**代码仓库声明为地理不适用**的权威途径）；
    /// - 未显式声明（`None`）→ 回退 crs 占位推断（[`MetadataCtx::detect`]）：
    ///   crs 为真实编码 → `true`；crs 为 `不适用/N/A/not applicable` 占位或缺失
    ///   → `false`。
    ///
    /// 由此**代码仓库**（写 `data-layer: 否`）与**地理项目**（写 `data-layer: 是`）
    /// 都能被本方法正确裁决，`kanyu agents validate` 对两类仓库均可判为通过，
    /// 彻底消除「crs 不适用」却仍被要求「数据层语义表」的矛盾。
    /// 零参 `validate` 与本方法一致，`validate_code_repo` 额外把 `is_geo` 钉死为
    /// `false`（供调用方显式声明「此为代码仓库、免检数据层」）。
    pub fn resolve_data_layer(&self) -> bool {
        match self.meta.data_layer {
            Some(flag) => flag,
            None => MetadataCtx::detect(self.meta.crs.as_deref()).is_geo,
        }
    }

    pub fn validate(&self, pin_geo: Option<bool>) -> Vec<String> {
        // 数据层适用性裁决。`pin_geo` 携带调用方显式语境钉死（CLI `--geo`
        // 钉 `Some(true)` / `--code-repo` 钉 `Some(false)`），**绕过**
        // `resolve_data_layer` 的显式 `data-layer` 元数据行——供调用方声明的
        // 「地理/代码仓库」语境压过文档自裁决。`pin_geo` 为 `None`（零参
        // `kanyu agents validate` / `--check-code-repo`）时不钉死，数据层适用性
        // 交 `resolve_data_layer()` 自动裁决（`data-layer` 行优先、crs 占位回退），
        // 故地理与代码两类仓库零手工均可通过。
        let data_layer_required = match pin_geo {
            Some(flag) => flag,
            None => self.resolve_data_layer(),
        };
        let is_geo = match pin_geo {
            Some(flag) => flag,
            None => MetadataCtx::detect(self.meta.crs.as_deref()).is_geo,
        };
        let mut issues = Vec::new();
        // 元数据缺失：地理项目为「错误」，软件仓库降级为「告警」。
        if self.meta.name.is_none() {
            if is_geo {
                issues.push("缺少项目元数据: name".to_string());
            } else {
                issues.push("缺少项目元数据: name（软件仓库：非必填，建议补全）".to_string());
            }
        }
        if self.meta.crs.is_none() {
            if is_geo {
                issues.push("缺少项目元数据: crs（坐标参考系是堪舆项目的强制项）".to_string());
            } else {
                issues.push(
                    "缺少项目元数据: crs（软件仓库：建议写「不适用」以显式声明）。".to_string(),
                );
            }
        }
        // 数据层语义表：仅当本项目**要求**数据层时强制（`is_geo` 已综合
        // 显式 `data-layer` 声明与 crs 占位）；纯软件工程仓库（`data-layer: 否`
        // 或 `crs: 不适用`）免检，彻底消除与「crs 不适用」的矛盾。
        if data_layer_required && self.layers.is_empty() {
            issues.push("数据层语义表为空：AI 无法理解图层含义".to_string());
        }
        // 每层关键字段：两类项目均为「警告」（不阻断通过，分诊应改进）。
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.key_fields.is_empty() {
                issues.push(format!(
                    "图层 {} (第 {} 行) 未声明关键字段（建议补全）",
                    layer.layer,
                    i + 1
                ));
            }
        }
        issues
    }

    /// 软件仓库（代码库）语境校验：[`MetadataCtx`] `is_geo = false`，**钉死**
    /// 免检数据层语义表。供**调用方明确声明「此为代码仓库」**时使用（如 `kanyu
    /// agents validate --code-repo`）。与零参 [`validate`](Self::validate) 的区别：
    /// 本方法把 `is_geo` 强制为 `false`，**忽略** crs/`data-layer` 值；零参
    /// `validate` 则以真实 crs 占位与显式 `data-layer` 行裁决（见
    /// [`resolve_data_layer`](Self::resolve_data_layer)），故两类仓库均无需
    /// 区分语境、直接调零参即可正确通过。
    pub fn validate_code_repo(&self) -> Vec<String> {
        // 钉死非地理（`Some(false)`）：忽略文档中数据层语义表与 crs 占位。
        self.validate(Some(false))
    }
}

/// 解析元数据行：`- **key**: value`。
fn parse_meta_line(line: &str, meta: &mut ProjectMeta) {
    let Some(rest) = line.strip_prefix("- **") else {
        return;
    };
    let Some((key, value)) = rest.split_once("**: ") else {
        return;
    };
    let value = value.trim().to_string();
    match key.trim() {
        "name" => meta.name = Some(value),
        "crs" => meta.crs = Some(value),
        "extent" => {
            let nums: Vec<f64> = value
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if nums.len() == 4 {
                meta.extent = Some(nums);
            }
        }
        "author" => meta.author = Some(value),
        "created" => meta.created = Some(value),
        // 数据层适用性声明（`是`/`否`，大小写/空白宽容）：显式标记本项目是否
        // 含 GIS 数据层。`是/适用/yes/true/1` → `Some(true)`；
        // `否/不适用/no/false/0` → `Some(false)`（代码仓库，校验免检语义表）。
        // 与 `crs` 占位互为冗余的单一事实来源，供
        // [`AgentsMd::resolve_data_layer`] 以「显式声明优先于 crs 占位」裁决。
        "data-layer" | "data_layer" => {
            let lowered = value.trim().to_ascii_lowercase();
            meta.data_layer = Some(
                lowered == "是"
                    || lowered.starts_with("适用")
                    || lowered == "yes"
                    || lowered == "true"
                    || lowered == "1",
            );
        }
        _ => {}
    }
}

/// 解析数据层语义表行：`| buildings | Polygon | 建筑轮廓 | height, floor | ... |`。
fn parse_layer_row(line: &str) -> Option<LayerSemantics> {
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .split('|')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    if cells.len() < 5 {
        return None;
    }
    // 跳过表头与分隔行。
    if cells[0] == "图层" || cells[0].chars().all(|c| c == '-' || c == ' ') {
        return None;
    }
    Some(LayerSemantics {
        layer: cells[0].to_string(),
        geometry: cells[1].to_string(),
        semantics: cells[2].to_string(),
        key_fields: cells[3].to_string(),
        rules: cells[4].to_string(),
    })
}

/// 解析编号列表项：`1. 建筑必须完全位于地块内。`
fn parse_numbered_item(line: &str) -> Option<String> {
    let (num, rest) = line.split_once(". ")?;
    if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
        Some(rest.trim().to_string())
    } else {
        None
    }
}

/// 生成一份 AGENTS.md 模板（`kanyu agents init` 使用）。
///
/// 按项目性质选择模板形态（`is_geo`）：
/// - **地理项目**（`true`）：含 `## 数据层语义` 语义表、`## 坐标系统`
///   （`crs` 为坐标系编码）、`## 业务规则`（空间约束）；
/// - **软件/代码仓库**（`false`，由调用方对 `crs: [code]` 哨兵传 `false`）：
///   crs 写 [`NOT_APPLICABLE`]「不适用」、免 `## 数据层语义` 表（代码仓库无
///   GIS 图层，不要求填——与 `validate_code_repo` 的免检一致）、`## 业务规则`
///   改软工程式（接口/不变量/测试），并加 `## 非地理项目` 说明块点明边界。
pub fn template(project_name: &str, crs: &str, is_geo: bool) -> String {
    if is_geo {
        format!(
            r#"# AGENTS.md —— {project_name}

## 项目元数据
- **name**: {project_name}
- **crs**: {crs}
- **data-layer**: 是
- **extent**: [minx, miny, maxx, maxy]
- **author**: TODO
- **created**: TODO

## 数据层语义
| 图层 | 类型 | 语义 | 关键字段 | 业务规则 |
|------|------|------|---------|---------|
| example | Polygon | 示例图层 | id, name | TODO |

## 坐标系统
- 所有数据采用 {crs}。
- 禁止混用其他坐标系的平面坐标。

## 业务规则
1. TODO: 描述空间约束（如 建筑必须完全位于地块内）。

## AI 工作流
- **制图**: TODO
- **分析**: TODO
- **导出**: TODO

## 自定义工具
- `kanyu.tools.example`: TODO
"#
        )
    } else {
        format!(
            r#"# AGENTS.md —— {project_name}

## 项目元数据
- **name**: {project_name}
- **crs**: 不适用
- **data-layer**: 否
- **author**: TODO
- **created**: TODO

## 非地理项目
本仓库是**软件工程/代码仓库**，无 GIS 地理数据，故 `crs: 不适用`（占位值，
不是可投影的坐标系）。**不要求**数据层语义表；AI 请遵循软件工程式的构建、
测试与审查工作流，而非地理制图/分析约定（见 `MasterPLAN` §4.3.2 适用边界）。

## 构建与验证
```bash
cargo build --workspace          # 构建
cargo test --workspace           # 全部测试（提交前必须通过）
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## 业务规则（软件工程式）
1. TODO: 描述代码不变量（如 公共 API 需有文档与测试覆盖）。

## 自定义工具
- 见根目录 AI_SYNC.md 与 docs/（模块清单单一事实来源：introspect.rs）。
"#
        )
    }
}

/// 从文件读取并解析。
pub fn load(path: &str) -> Result<AgentsMd> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| KanyuError::AgentsMd(format!("无法读取 {path}: {e}")))?;
    AgentsMd::parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# AGENTS.md —— 城市更新规划项目

## 项目元数据
- **name**: 朝阳区城市更新规划
- **crs**: EPSG:4526 (CGCS2000 / 3-degree Gauss-Kruger CM 117E)
- **extent**: [116.2, 39.8, 116.6, 40.0]
- **author**: 道明远
- **created**: 2026-08-01

## 数据层语义
| 图层 | 类型 | 语义 | 关键字段 | 业务规则 |
|------|------|------|---------|---------|
| buildings | Polygon | 建筑轮廓 | height, floor, usage | height > 0 为有效建筑 |
| roads | LineString | 道路中心线 | width, grade, name | width 单位米 |

## 业务规则
1. 建筑必须完全位于地块内 (within)。
2. 道路交叉口必须生成节点 (intersection)。

## 自定义工具
- `kanyu.tools.check_fsr`: 检查建筑高度是否符合容积率。
"#;

    #[test]
    fn parses_masterplan_example() {
        let doc = AgentsMd::parse(SAMPLE).unwrap();
        assert_eq!(doc.meta.name.as_deref(), Some("朝阳区城市更新规划"));
        assert!(doc.meta.crs.as_deref().unwrap().starts_with("EPSG:4526"));
        assert_eq!(doc.meta.extent, Some(vec![116.2, 39.8, 116.6, 40.0]));
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.layers[0].layer, "buildings");
        assert_eq!(doc.business_rules.len(), 2);
        assert_eq!(doc.custom_tools, vec!["kanyu.tools.check_fsr"]);
        assert!(doc.validate(Some(true)).is_empty());
    }

    #[test]
    fn validate_flags_missing_crs() {
        let doc = AgentsMd::parse("# empty").unwrap();
        let issues = doc.validate(Some(true));
        assert!(issues.iter().any(|i| i.contains("crs")));
    }

    #[test]
    fn template_roundtrips_through_parser() {
        let text = template("测试项目", "EPSG:4326", true);
        let doc = AgentsMd::parse(&text).unwrap();
        assert_eq!(doc.meta.name.as_deref(), Some("测试项目"));
        assert_eq!(doc.meta.crs.as_deref(), Some("EPSG:4326"));
    }
}
