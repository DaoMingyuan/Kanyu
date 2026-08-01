//! `AGENTS.md` 项目语义描述文件 —— 解析、校验与生成。
//!
//! 对应总规 §4.3.2：每个堪舆项目根目录包含一份 `AGENTS.md`，
//! 它是 AI 理解项目的"罗盘"——图层语义、坐标系、业务规则、工作流。
//! 本模块将其结构化为机器可读对象，供 CLI/MCP 工具消费。

use serde::Serialize;

use crate::error::{KanyuError, Result};

/// 项目元数据。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectMeta {
    /// 项目名。
    pub name: Option<String>,
    /// 坐标参考系（如 `EPSG:4526`）。
    pub crs: Option<String>,
    /// 范围 `[minx, miny, maxx, maxy]`。
    pub extent: Option<Vec<f64>>,
    /// 作者。
    pub author: Option<String>,
    /// 创建日期。
    pub created: Option<String>,
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

    /// 完整性校验：返回缺失/问题清单（空列表 = 通过）。
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.meta.name.is_none() {
            issues.push("缺少项目元数据: name".to_string());
        }
        if self.meta.crs.is_none() {
            issues.push("缺少项目元数据: crs（坐标参考系是堪舆项目的强制项）".to_string());
        }
        if self.layers.is_empty() {
            issues.push("数据层语义表为空：AI 无法理解图层含义".to_string());
        }
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.key_fields.is_empty() {
                issues.push(format!(
                    "图层 {} (第 {} 行) 未声明关键字段",
                    layer.layer,
                    i + 1
                ));
            }
        }
        issues
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
pub fn template(project_name: &str, crs: &str) -> String {
    format!(
        r#"# AGENTS.md —— {project_name}

## 项目元数据
- **name**: {project_name}
- **crs**: {crs}
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
        assert!(doc.validate().is_empty());
    }

    #[test]
    fn validate_flags_missing_crs() {
        let doc = AgentsMd::parse("# empty").unwrap();
        let issues = doc.validate();
        assert!(issues.iter().any(|i| i.contains("crs")));
    }

    #[test]
    fn template_roundtrips_through_parser() {
        let text = template("测试项目", "EPSG:4326");
        let doc = AgentsMd::parse(&text).unwrap();
        assert_eq!(doc.meta.name.as_deref(), Some("测试项目"));
        assert_eq!(doc.meta.crs.as_deref(), Some("EPSG:4326"));
    }
}
