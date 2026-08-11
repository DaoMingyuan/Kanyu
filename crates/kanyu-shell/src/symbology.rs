//! 图层符号化模型（ArcGIS Pro 符号系统范式）：单色 / 唯一值分类 / 分级设色。
//!
//! - 模型可 serde 持久化（.kyu `ProjectLayer.style` 字段，JSON 层面与 core 解耦）；
//! - [`to_style_rule`] 把模型投影为 `kanyu_render::StyleRule`（纯函数，可测）；
//! - 色带内置三组（青玉/琥珀/蓝灰），双主题可用；
//! - 默认单色按几何类型取图例色（面=青、线=蓝灰、点=琥珀，与 Contents 一致）。

use kanyu_render::StyleRule;
use serde::{Deserialize, Serialize};

/// 图层符号化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum LayerSymbology {
    /// 单色。
    Single { color: [u8; 3] },
    /// 唯一值分类。
    Categorical {
        field: String,
        /// 类别值 → 颜色（按显示序）。
        colors: Vec<(String, [u8; 3])>,
        /// 「<其他>」颜色。
        other: [u8; 3],
    },
    /// 分级设色（断点 n 个 → n+1 类，色带均匀取样）。
    Graduated {
        field: String,
        /// 断点（严格升序）。
        breaks: Vec<f64>,
        ramp: Ramp,
    },
}

/// 色带（内置三组，色值出自 theme palette 语义族）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ramp {
    /// 青玉系（浅→深）。
    Jade,
    /// 琥珀系。
    Amber,
    /// 蓝灰系。
    Slate,
}

impl Ramp {
    /// 全部色带（显示名）。
    pub const ALL: [Ramp; 3] = [Ramp::Jade, Ramp::Amber, Ramp::Slate];

    /// 中文名。
    pub fn label(self) -> &'static str {
        match self {
            Ramp::Jade => "青玉",
            Ramp::Amber => "琥珀",
            Ramp::Slate => "蓝灰",
        }
    }

    /// 色带颜色（浅→深 5 色）。
    pub fn colors(self) -> [[u8; 3]; 5] {
        match self {
            Ramp::Jade => [
                [0xE8, 0xF4, 0xF0],
                [0xBF, 0xE0, 0xD8],
                [0x7F, 0xBF, 0xB2],
                [0x4D, 0x9A, 0x8C],
                [0x2D, 0x6A, 0x5E],
            ],
            Ramp::Amber => [
                [0xFB, 0xF3, 0xDC],
                [0xF3, 0xDF, 0xA0],
                [0xE9, 0xC4, 0x6A],
                [0xD9, 0xA2, 0x3C],
                [0xB0, 0x78, 0x18],
            ],
            Ramp::Slate => [
                [0xEA, 0xF1, 0xF6],
                [0xC6, 0xD9, 0xE6],
                [0x8F, 0xB3, 0xCC],
                [0x5E, 0x8F, 0xAD],
                [0x3A, 0x6B, 0x8C],
            ],
        }
    }

    /// 均匀取样 n 色（n ≥ 1）。
    pub fn sample(self, n: usize) -> Vec<[u8; 3]> {
        let cs = self.colors();
        let n = n.max(1);
        (0..n)
            .map(|i| {
                let pos = if n == 1 {
                    cs.len() - 1
                } else {
                    i * (cs.len() - 1) / (n - 1)
                };
                cs[pos]
            })
            .collect()
    }
}

/// 几何类型默认图例色（与 Contents 树色块同一语义：面=青、线=蓝灰、点=琥珀）。
pub fn default_single(geometry_types: &[String]) -> LayerSymbology {
    let has = |k: &str| geometry_types.iter().any(|t| t.contains(k));
    let color = if has("Polygon") {
        [0x2D, 0x6A, 0x5E]
    } else if has("LineString") {
        [0x4A, 0x7C, 0x9B]
    } else {
        [0xD4, 0xA8, 0x43]
    };
    LayerSymbology::Single { color }
}

/// [u8;3] → `#RRGGBB`。
pub fn hex_of(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// `#RRGGBB`/`RRGGBB` 解析（属性页文本框校验用）。
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// 模型 → render StyleRule 投影（纯函数）。
///
/// - Single → Categorical 全 default（字段置空，一切要素取该色）；
/// - Categorical → 值映射 + other 兜底；
/// - Graduated → stops 严格升序：首档阈值取 f64::MIN（保证低于首断点
///   的要素也吃 ramp 首色，而非主题默认色——分级设色全域着色）。
pub fn to_style_rule(sym: &LayerSymbology) -> StyleRule {
    match sym {
        LayerSymbology::Single { color } => StyleRule::Categorical {
            field: String::new(),
            colors: std::collections::HashMap::new(),
            default: Some(hex_of(*color)),
        },
        LayerSymbology::Categorical {
            field,
            colors,
            other,
        } => StyleRule::Categorical {
            field: field.clone(),
            colors: colors
                .iter()
                .map(|(v, c)| (v.clone(), hex_of(*c)))
                .collect(),
            default: Some(hex_of(*other)),
        },
        LayerSymbology::Graduated {
            field,
            breaks,
            ramp,
        } => {
            let colors = ramp.sample(breaks.len() + 1);
            let mut stops: Vec<(f64, String)> = Vec::with_capacity(colors.len());
            stops.push((f64::MIN, hex_of(colors[0])));
            for (i, b) in breaks.iter().enumerate() {
                stops.push((*b, hex_of(colors[i + 1])));
            }
            StyleRule::Graduated {
                field: field.clone(),
                stops,
            }
        }
    }
}

/// 主色（3D 场景棱柱取色等"一层一色"场合）：
/// Single 取该色；Categorical 取首个类别色（无则 other）；Graduated 取色带最深。
pub fn primary_color(sym: &LayerSymbology) -> [u8; 3] {
    match sym {
        LayerSymbology::Single { color } => *color,
        LayerSymbology::Categorical { colors, other, .. } => {
            colors.first().map(|(_, c)| *c).unwrap_or(*other)
        }
        LayerSymbology::Graduated { ramp, .. } => *ramp.colors().last().unwrap(),
    }
}

/// Contents 展开的分类行（色块 + 标注；几何类型名由调用方给 single 用）。
pub fn class_rows(sym: &LayerSymbology, single_label: &str) -> Vec<([u8; 3], String)> {
    match sym {
        LayerSymbology::Single { color } => vec![(*color, single_label.to_string())],
        LayerSymbology::Categorical { colors, other, .. } => {
            let mut rows: Vec<([u8; 3], String)> =
                colors.iter().map(|(v, c)| (*c, v.clone())).collect();
            rows.push((*other, "<其他>".to_string()));
            rows
        }
        LayerSymbology::Graduated { breaks, ramp, .. } => {
            let colors = ramp.sample(breaks.len() + 1);
            let mut rows = Vec::with_capacity(colors.len());
            for (i, c) in colors.iter().enumerate() {
                let label = if breaks.is_empty() {
                    "全部".to_string()
                } else if i == 0 {
                    format!("≤ {}", fmt_num(breaks[0]))
                } else if i == colors.len() - 1 {
                    format!("> {}", fmt_num(breaks[breaks.len() - 1]))
                } else {
                    format!("{} – {}", fmt_num(breaks[i - 1]), fmt_num(breaks[i]))
                };
                rows.push((*c, label));
            }
            rows
        }
    }
}

/// 数值友好显示（断点标注）。
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// 定性色板（唯一值分类自动配色；内容数据色，非界面装饰色）。
pub const QUALITATIVE: [[u8; 3]; 8] = [
    [0x2D, 0x6A, 0x5E], // 远黛青
    [0xD4, 0xA8, 0x43], // 琥珀
    [0x4A, 0x7C, 0x9B], // 蓝灰
    [0xB1, 0x4E, 0x32], // 朱砂
    [0x7E, 0x6B, 0xAA], // 黛紫
    [0x8C, 0x6D, 0x46], // 赭石
    [0x4D, 0x9A, 0x8C], // 浅青
    [0xC9, 0x8A, 0x2D], // 金珀
];

/// 唯一值分类自动生成（取字段去重值 ≤12 个，定性色板循环；其余归 <其他>）。
pub fn auto_categorical(field: &str, collection: &geojson::FeatureCollection) -> LayerSymbology {
    let mut values: Vec<String> = Vec::new();
    for f in &collection.features {
        if let Some(v) = f.properties.as_ref().and_then(|p| p.get(field)) {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => continue,
                other => other.to_string(),
            };
            if !values.contains(&s) {
                values.push(s);
            }
            if values.len() >= 12 {
                break;
            }
        }
    }
    LayerSymbology::Categorical {
        field: field.to_string(),
        colors: values
            .into_iter()
            .enumerate()
            .map(|(i, v)| (v, QUALITATIVE[i % QUALITATIVE.len()]))
            .collect(),
        other: [0x88, 0x88, 0x88],
    }
}

/// 分级设色自动生成（数值字段 min–max 均分 4 断点 / 5 类，青玉色带；
/// 无数值字段时退化为 0–1 四断点）。
pub fn auto_graduated(field: &str, collection: &geojson::FeatureCollection) -> LayerSymbology {
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for f in &collection.features {
        if let Some(v) = f
            .properties
            .as_ref()
            .and_then(|p| p.get(field))
            .and_then(|v| v.as_f64())
        {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if !min.is_finite() || !max.is_finite() || min >= max {
        min = 0.0;
        max = 1.0;
    }
    let step = (max - min) / 5.0;
    let breaks = (1..5).map(|i| min + step * i as f64).collect();
    LayerSymbology::Graduated {
        field: field.to_string(),
        breaks,
        ramp: Ramp::Jade,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex_of([0x2D, 0x6A, 0x5E]), "#2D6A5E");
        assert_eq!(parse_hex("#2D6A5E"), Some([0x2D, 0x6A, 0x5E]));
        assert_eq!(parse_hex("d4a843"), Some([0xD4, 0xA8, 0x43]));
        assert_eq!(parse_hex("#FFF"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
    }

    #[test]
    fn single_projects_to_default_color() {
        let rule = to_style_rule(&LayerSymbology::Single {
            color: [0x2D, 0x6A, 0x5E],
        });
        match rule {
            StyleRule::Categorical {
                default, colors, ..
            } => {
                assert_eq!(default.as_deref(), Some("#2D6A5E"));
                assert!(colors.is_empty());
            }
            _ => panic!("Single 应投影为 Categorical"),
        }
    }

    #[test]
    fn categorical_hit_and_other_fallback() {
        let sym = LayerSymbology::Categorical {
            field: "usage".into(),
            colors: vec![("办公".into(), [0x2D, 0x6A, 0x5E])],
            other: [0x88, 0x88, 0x88],
        };
        match to_style_rule(&sym) {
            StyleRule::Categorical {
                field,
                colors,
                default,
            } => {
                assert_eq!(field, "usage");
                assert_eq!(colors.get("办公").map(String::as_str), Some("#2D6A5E"));
                assert_eq!(default.as_deref(), Some("#888888")); // other 回退
            }
            _ => panic!(),
        }
    }

    #[test]
    fn graduated_stops_cover_all_ranges() {
        let sym = LayerSymbology::Graduated {
            field: "h".into(),
            breaks: vec![10.0, 50.0],
            ramp: Ramp::Jade,
        };
        match to_style_rule(&sym) {
            StyleRule::Graduated { stops, .. } => {
                assert_eq!(stops.len(), 3); // n 断点 → n+1 档
                assert_eq!(stops[0].0, f64::MIN); // 全域着色
                assert!(stops[0].1 != stops[2].1);
                // 严格升序。
                assert!(stops[0].1 != stops[1].1 && stops[1].0 < stops[2].0);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn class_rows_labels() {
        let single = default_single(&["Polygon".to_string()]);
        let rows = class_rows(&single, "面");
        assert_eq!(rows, vec![([0x2D, 0x6A, 0x5E], "面".to_string())]);
        let grad = LayerSymbology::Graduated {
            field: "h".into(),
            breaks: vec![10.0, 50.0],
            ramp: Ramp::Amber,
        };
        let labels: Vec<String> = class_rows(&grad, "").into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, vec!["≤ 10", "10 – 50", "> 50"]);
        let cat = LayerSymbology::Categorical {
            field: "u".into(),
            colors: vec![("办公".into(), [1, 2, 3])],
            other: [9, 9, 9],
        };
        let labels: Vec<String> = class_rows(&cat, "").into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, vec!["办公", "<其他>"]);
    }

    #[test]
    fn ramp_sampling() {
        let c2 = Ramp::Jade.sample(2);
        assert_eq!(c2[0], Ramp::Jade.colors()[0]);
        assert_eq!(c2[1], Ramp::Jade.colors()[4]);
        assert_eq!(Ramp::Slate.sample(1).len(), 1);
    }

    #[test]
    fn serde_roundtrip_for_kyu() {
        let sym = LayerSymbology::Graduated {
            field: "h".into(),
            breaks: vec![1.0, 2.0],
            ramp: Ramp::Slate,
        };
        let v = serde_json::to_value(&sym).unwrap();
        let back: LayerSymbology = serde_json::from_value(v).unwrap();
        assert_eq!(back, sym);
    }

    #[test]
    fn auto_generators() {
        let c: geojson::FeatureCollection = serde_json::from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":null,"properties":{"usage":"办公","h":10.0}},
                {"type":"Feature","geometry":null,"properties":{"usage":"住宅","h":90.0}},
                {"type":"Feature","geometry":null,"properties":{"usage":"办公","h":50.0}}
            ]}"#,
        )
        .unwrap();
        match auto_categorical("usage", &c) {
            LayerSymbology::Categorical { colors, .. } => {
                assert_eq!(colors.len(), 2); // 去重
                assert_eq!(colors[0].0, "办公");
            }
            _ => panic!(),
        }
        match auto_graduated("h", &c) {
            LayerSymbology::Graduated { breaks, .. } => {
                assert_eq!(breaks.len(), 4);
                assert!(breaks.windows(2).all(|w| w[0] < w[1])); // 严格升序
                assert!(breaks[0] > 10.0 && breaks[3] < 90.0);
            }
            _ => panic!(),
        }
    }
}
