//! AI 对话面板：驱动方式与设置方式参考 BitFun——**驱动可插拔 + 设置面板**。
//!
//! - **LocalDriver（默认，离线）**：规则引擎把自然语言映射到堪舆命令面
//!   （复用终端的 ConsoleHost 通道），回复 = 说明 + 执行结果，零依赖立即可用。
//! - **OpenAiDriver（可选）**：OpenAI 兼容端点（base_url/api_key/model），
//!   HTTP 用 ureq（纯 Rust 小依赖）；系统提示注入当前数据现场（图层/字段）。
//! - 设置持久化：`%APPDATA%/kanyu/shell_ai.json`（不含对话日志）。

use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::console::ConsoleHost;
use crate::ui_kit::icons::{self, Icon};
use crate::ui_kit::{
    button, combo_static, hint_caption, password_input, text, text_input, ButtonVariant,
};

/// AI 设置（BitFun 式：驱动 + 端点 + 密钥 + 模型）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSettings {
    /// 驱动："local" | "openai"。
    pub driver: String,
    /// OpenAI 兼容端点（如 https://api.openai.com/v1 或自建网关）。
    pub base_url: String,
    /// API Key（仅本地持久化，不入日志）。
    pub api_key: String,
    /// 模型名（如 gpt-4o-mini / deepseek-chat / kimi-k2）。
    pub model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            driver: "local".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

impl AiSettings {
    /// 设置文件路径（%APPDATA%/kanyu/shell_ai.json；非 Windows 用 ~/.config）。
    fn path() -> PathBuf {
        let base = std::env::var("APPDATA")
            .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(base).join("kanyu").join("shell_ai.json")
    }

    /// 读设置（缺失/损坏回退默认）。
    pub fn load() -> Self {
        let path = Self::path();
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 写设置（建目录）。
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())
    }
}

/// 一条对话。
#[derive(Debug, Clone)]
pub struct ChatMsg {
    /// 角色："user" | "assistant" | "system"。
    pub role: String,
    /// 内容。
    pub content: String,
}

/// AI 驱动特征（BitFun 式可插拔）。
pub trait AiDriver {
    /// 驱动名。
    fn name(&self) -> &str;
    /// 发送对话（历史 + 数据现场提示），返回助手回复。
    fn chat(&mut self, history: &[ChatMsg], context: &str) -> Result<String, String>;
}

/// 本地规则驱动：自然语言 → 堪舆命令（离线默认）。
///
/// 映射规则（顺序匹配，命中即执行并回复）：
/// - 缓冲：「缓冲 <图层> <距离>」「给 X 做 N 缓冲/缓冲区」
/// - 工具箱意图面（38 工具注册表）：工具中文名精确/前缀/包含匹配
///   并按参数表提参（图层/数值/字段/枚举……），缺参给用法引导，
///   多候选给歧义清单（[`match_tool_exact`]/[`extract_tool_values`] 纯函数）。
/// - 打开：「打开 <路径>」「加载 <路径>」；图层/概要/查询/度量/导出/帮助。
pub struct LocalDriver;

/// 工具名匹配结果。
#[derive(Debug)]
pub enum ToolMatch {
    /// 唯一命中。
    One(&'static kanyu_core::tooldef::ToolDef),
    /// 歧义候选（工具中文名清单）。
    Ambiguous(Vec<&'static str>),
    /// 未命中。
    None,
}

/// 工具名完全包含匹配（输入含完整工具名；多名命中取最长——
/// 「按字段缓冲区 X」中的「缓冲区」不会误截；同长多名 → 歧义）。
pub fn match_tool_exact(text: &str) -> ToolMatch {
    use kanyu_core::tooldef::TOOLS;
    let mut hits: Vec<_> = TOOLS.iter().filter(|d| text.contains(d.name)).collect();
    if hits.is_empty() {
        return ToolMatch::None;
    }
    let max_len = hits
        .iter()
        .map(|d| d.name.chars().count())
        .max()
        .unwrap_or(0);
    hits.retain(|d| d.name.chars().count() == max_len);
    if hits.len() == 1 {
        ToolMatch::One(hits[0])
    } else {
        ToolMatch::Ambiguous(hits.iter().map(|d| d.name).collect())
    }
}

/// 工具名前缀/子串猜测（首分词 ≥2 字符且被工具名包含；如「缓冲」→ 三候选）。
pub fn match_tool_prefix(text: &str) -> ToolMatch {
    use kanyu_core::tooldef::TOOLS;
    let first = text
        .split(|c: char| c.is_whitespace() || "，。、；：,.;:".contains(c))
        .next()
        .unwrap_or("")
        .trim();
    if first.chars().count() < 2 {
        return ToolMatch::None;
    }
    let cands: Vec<_> = TOOLS.iter().filter(|d| d.name.contains(first)).collect();
    match cands.len() {
        0 => ToolMatch::None,
        1 => ToolMatch::One(cands[0]),
        _ => ToolMatch::Ambiguous(cands.iter().map(|d| d.name).collect()),
    }
}

/// 用法引导文本（缺参/歧义回复用；示例按参数类型生成，图层取现场首个）。
pub fn usage_of(def: &kanyu_core::tooldef::ToolDef, layers: &[String]) -> String {
    use kanyu_core::tooldef::ParamKind;
    let params: Vec<String> = def
        .params
        .iter()
        .map(|p| {
            format!(
                "{}{}",
                p.label,
                if p.required {
                    "（必填）"
                } else {
                    "（可选）"
                }
            )
        })
        .collect();
    let example: Vec<String> = def
        .params
        .iter()
        .map(|p| match &p.kind {
            ParamKind::Layer => layers
                .first()
                .cloned()
                .unwrap_or_else(|| "图层名".to_string()),
            ParamKind::Number | ParamKind::Long => "0.1".to_string(),
            ParamKind::NumberList => "100,200".to_string(),
            ParamKind::LinearUnit => "0.1 度".to_string(),
            ParamKind::Field(_) => "字段名".to_string(),
            ParamKind::Enum(opts) => opts
                .first()
                .map(|(_, zh)| zh.to_string())
                .unwrap_or_default(),
            ParamKind::Boolean => "是".to_string(),
            ParamKind::Expression => "height > 50".to_string(),
            ParamKind::Crs => "EPSG:4490".to_string(),
            ParamKind::OutFile => "输出路径.geojson".to_string(),
            _ => p.hint.to_string(),
        })
        .collect();
    format!(
        "「{}」参数：{}。示例：{} {}",
        def.name,
        params.join("、"),
        def.name,
        example.join(" ")
    )
}

/// 从文本提取工具参数值（按参数表顺序；纯函数）。
/// 返回与参数表等长的值（缺失可选补默认值）；缺必填 → Err(用法引导)。
pub fn extract_tool_values(
    def: &kanyu_core::tooldef::ToolDef,
    text: &str,
    layers: &[String],
    fields_of: &dyn Fn(&str) -> Vec<String>,
) -> Result<Vec<String>, String> {
    // 去掉工具名（首个出现处）后分词：空白 + 中英文标点（不含 '.'——小数点/扩展名不可分割）。
    let rest = text.replacen(def.name, "", 1);
    let tokens: Vec<String> = rest
        .split(|c: char| c.is_whitespace() || "，。、；：,;（）()「」".contains(c))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let mut used = vec![false; tokens.len()];
    let mut chosen_layer: Option<String> = None;
    let mut values: Vec<String> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for p in def.params {
        let got = extract_param(p, &tokens, &mut used, layers, &mut chosen_layer, fields_of);
        match got {
            Some(v) => values.push(v),
            None => {
                // 必填且无默认值才算缺参（有默认值的按对话框语义取默认）。
                if p.required && p.default.is_empty() {
                    missing.push(p.label);
                }
                values.push(p.default.to_string());
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "「{}」还缺 {}：{}",
            def.name,
            missing.join("、"),
            usage_of(def, layers)
        ));
    }
    Ok(values)
}

/// 单参数提取（按类型从分词中取值；取中即消耗）。
fn extract_param(
    p: &kanyu_core::tooldef::ToolParam,
    tokens: &[String],
    used: &mut [bool],
    layers: &[String],
    chosen_layer: &mut Option<String>,
    fields_of: &dyn Fn(&str) -> Vec<String>,
) -> Option<String> {
    use kanyu_core::tooldef::ParamKind;
    /// 记号里的数值（允许「距离0.1」这类标签前缀粘连）。
    fn number_in(t: &str) -> Option<String> {
        let start = t.find(|c: char| c.is_ascii_digit() || c == '-' || c == '.')?;
        let v: String = t[start..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        v.parse::<f64>().ok().map(|_| v)
    }
    match &p.kind {
        ParamKind::Layer => {
            // 精确或包含匹配图层 id（最长 id 优先防子串误中）。
            let mut cand: Option<(usize, String)> = None;
            for l in layers {
                for (i, t) in tokens.iter().enumerate() {
                    if used[i] || !(t == l || t.contains(l.as_str())) {
                        continue;
                    }
                    if cand.as_ref().is_none_or(|(_, cl)| l.len() > cl.len()) {
                        cand = Some((i, l.clone()));
                    }
                }
            }
            cand.map(|(i, l)| {
                used[i] = true;
                *chosen_layer = Some(l.clone());
                l
            })
        }
        ParamKind::MultiLayers => {
            let ids: Vec<String> = layers
                .iter()
                .filter(|l| {
                    tokens
                        .iter()
                        .enumerate()
                        .any(|(i, t)| !used[i] && t.contains(l.as_str()))
                })
                .cloned()
                .collect();
            if ids.is_empty() {
                None
            } else {
                for (i, t) in tokens.iter().enumerate() {
                    if ids.iter().any(|l| t.contains(l.as_str())) {
                        used[i] = true;
                    }
                }
                Some(ids.join("\n"))
            }
        }
        ParamKind::Field(_) => {
            let fields: Vec<String> = chosen_layer.as_deref().map(fields_of).unwrap_or_default();
            for (i, t) in tokens.iter().enumerate() {
                if used[i] {
                    continue;
                }
                let clean = t
                    .trim_start_matches("按字段")
                    .trim_start_matches("字段")
                    .trim_start_matches("按");
                if fields.iter().any(|f| f == clean) {
                    used[i] = true;
                    return Some(clean.to_string());
                }
            }
            // 兜底：首个非数值、非图层的未用记号当字段名（内核校验中文报错兜底）。
            for (i, t) in tokens.iter().enumerate() {
                if used[i]
                    || number_in(t).is_some()
                    || layers.iter().any(|l| t.contains(l.as_str()))
                {
                    continue;
                }
                used[i] = true;
                return Some(t.clone());
            }
            None
        }
        ParamKind::Number | ParamKind::Long => tokens
            .iter()
            .enumerate()
            .find(|(i, t)| !used[*i] && number_in(t).is_some())
            .map(|(i, t)| {
                used[i] = true;
                number_in(t).expect("已判定含数值")
            }),
        ParamKind::NumberList => {
            let hits: Vec<(usize, String)> = tokens
                .iter()
                .enumerate()
                .filter(|(i, t)| !used[*i] && number_in(t).is_some())
                .map(|(i, t)| (i, number_in(t).expect("已判定含数值")))
                .collect();
            if hits.is_empty() {
                None
            } else {
                let mut out = Vec::new();
                for (i, v) in hits {
                    used[i] = true;
                    out.push(v);
                }
                Some(out.join(","))
            }
        }
        ParamKind::LinearUnit => {
            // 数值 + 可选单位词（米/千米/公里/度）；裸数值按 CRS 单位直通（"度"）。
            for (i, t) in tokens.iter().enumerate() {
                if used[i] {
                    continue;
                }
                if let Some(v) = number_in(t) {
                    used[i] = true;
                    let unit = if t.contains("千米") || t.contains("公里") {
                        "千米"
                    } else if t.contains('米') {
                        "米"
                    } else if t.contains('度') {
                        "度"
                    } else if let Some((j, u)) = tokens.iter().enumerate().find(|(j, u)| {
                        !used[*j] && matches!(u.as_str(), "米" | "千米" | "公里" | "度")
                    }) {
                        used[j] = true; // 消耗单位记号
                        match u.as_str() {
                            "米" => "米",
                            "度" => "度",
                            _ => "千米",
                        }
                    } else {
                        "度" // 裸数值：CRS 单位直通（经纬度即度）
                    };
                    return Some(format!("{v}|{unit}"));
                }
            }
            None
        }
        ParamKind::Enum(opts) => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] {
                return None;
            }
            opts.iter()
                .find(|(val, zh)| t == val || t == zh || t.contains(zh))
                .map(|(val, _)| {
                    used[i] = true;
                    val.to_string()
                })
        }),
        ParamKind::Boolean => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] {
                return None;
            }
            let hit = match t.as_str() {
                "是" | "开" | "开启" | "true" => Some("true"),
                "否" | "关" | "关闭" | "false" => Some("false"),
                _ => None,
            };
            hit.map(|h| {
                used[i] = true;
                h.to_string()
            })
        }),
        ParamKind::Expression => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] || !(t.contains('>') || t.contains('<') || t.contains('=')) {
                return None;
            }
            used[i] = true;
            Some(t.clone())
        }),
        ParamKind::Crs => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] || !t.to_ascii_uppercase().starts_with("EPSG") {
                return None;
            }
            used[i] = true;
            Some(t.clone())
        }),
        ParamKind::Extent => {
            let hits: Vec<(usize, String)> = tokens
                .iter()
                .enumerate()
                .filter(|(i, t)| !used[*i] && number_in(t).is_some())
                .take(4)
                .map(|(i, t)| (i, number_in(t).expect("已判定含数值")))
                .collect();
            if hits.len() == 4 {
                let mut out = Vec::new();
                for (i, v) in hits {
                    used[i] = true;
                    out.push(v);
                }
                Some(out.join(","))
            } else {
                None
            }
        }
        ParamKind::OutFile => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] || !t.contains('.') {
                return None;
            }
            used[i] = true;
            Some(t.clone())
        }),
        ParamKind::Text => tokens.iter().enumerate().find_map(|(i, t)| {
            if used[i] {
                return None;
            }
            used[i] = true;
            Some(t.clone())
        }),
    }
}

/// 帮助回复（工具分类面 + 示例；注册表单一事实来源投影）。
pub fn tools_help() -> String {
    use kanyu_core::tooldef::{ToolCategory, TOOLS};
    let mut out = String::from("我能直接驱动工具箱全部工具——说「工具中文名 + 参数」即可：\n");
    for cat in ToolCategory::ALL {
        let names: Vec<&str> = TOOLS
            .iter()
            .filter(|t| t.category == cat)
            .map(|t| t.name)
            .collect();
        out.push_str(&format!("· {}：{}\n", cat.label(), names.join("、")));
    }
    out.push_str("示例：「缓冲区 buildings 0.1」「质心 buildings」「融合 buildings 按字段 zone」");
    out
}

impl LocalDriver {
    /// 意图解析（纯函数，可单测）：返回解析后的意图。
    /// `fields_of`：图层 id → 字段清单（字段参数提取用）。
    pub fn parse(
        input: &str,
        layers: &[String],
        fields_of: &dyn Fn(&str) -> Vec<String>,
    ) -> ParsedIntent {
        let text = input.trim();
        // 找图层引用：输入中出现的已知图层 id（最长匹配优先）。
        let layer_hit = layers
            .iter()
            .filter(|id| !id.is_empty() && text.contains(id.as_str()))
            .max_by_key(|id| id.len())
            .cloned();

        // 缓冲（遗留专用意图：唯一图层自动补全/缺图层引导）。
        if let Some(d) = extract_number_after(text, &["缓冲", "缓冲区", "buffer"]) {
            if let Some(layer) = layer_hit {
                return ParsedIntent::Buffer { layer, distance: d };
            }
            if layers.len() == 1 {
                return ParsedIntent::Buffer {
                    layer: layers[0].clone(),
                    distance: d,
                };
            }
            return ParsedIntent::Advice(
                "想给谁做缓冲？请带上图层名，如：缓冲 buildings 500".to_string(),
            );
        }
        // 工具箱意图面：工具名完全包含（最长名优先；buffer 已被上面覆盖）。
        match match_tool_exact(text) {
            ToolMatch::One(def) => {
                return match extract_tool_values(def, text, layers, fields_of) {
                    Ok(values) => ParsedIntent::RunTool {
                        id: def.id,
                        name: def.name,
                        values,
                    },
                    Err(usage) => ParsedIntent::Advice(usage),
                };
            }
            ToolMatch::Ambiguous(names) => return ParsedIntent::AmbiguousTools(names),
            ToolMatch::None => {}
        }
        // 打开/加载。
        for kw in ["打开", "加载", "open", "load"] {
            if let Some(pos) = text.find(kw) {
                let path = text[pos + kw.len()..]
                    .trim()
                    .trim_matches(|c| c == '"' || c == '\'');
                if !path.is_empty()
                    && (path.contains('.') || path.contains('/') || path.contains('\\'))
                {
                    return ParsedIntent::Load(path.to_string());
                }
            }
        }
        // 导出。
        if text.contains("导出") || text.contains("export") {
            let out = extract_path_like(text);
            if let (Some(layer), Some(out)) = (layer_hit.clone(), out) {
                return ParsedIntent::Export {
                    layer,
                    out: out.to_string(),
                };
            }
        }
        // 图层清单。
        if ["图层", "有哪些", "layer"].iter().any(|k| text.contains(k)) && !text.contains("缓冲")
        {
            return ParsedIntent::Layers;
        }
        // 度量。
        if text.contains("量长度") || text.contains("长度") || text.contains("length") {
            if let Some(layer) = layer_hit.clone() {
                return ParsedIntent::Measure {
                    layer,
                    kind: "length",
                };
            }
        }
        if text.contains("量面积") || text.contains("面积") || text.contains("area") {
            if let Some(layer) = layer_hit {
                return ParsedIntent::Measure {
                    layer,
                    kind: "area",
                };
            }
        }
        // 帮助。
        if ["帮助", "能做什么", "help", "会什么"]
            .iter()
            .any(|k| text.contains(k))
        {
            return ParsedIntent::Help;
        }
        // 工具名前缀/子串猜测（「缓冲」→ 三候选歧义；「质心」→ 唯一命中缺参引导）。
        match match_tool_prefix(text) {
            ToolMatch::One(def) => {
                return match extract_tool_values(def, text, layers, fields_of) {
                    Ok(values) => ParsedIntent::RunTool {
                        id: def.id,
                        name: def.name,
                        values,
                    },
                    Err(usage) => ParsedIntent::Advice(usage),
                };
            }
            ToolMatch::Ambiguous(names) => return ParsedIntent::AmbiguousTools(names),
            ToolMatch::None => {}
        }
        ParsedIntent::Fallback
    }
}

/// 解析后的意图（图层名为拥有值，避免生命周期纠缠）。
#[derive(Debug)]
pub enum ParsedIntent {
    /// 缓冲。
    Buffer { layer: String, distance: f64 },
    /// 打开文件。
    Load(String),
    /// 图层清单。
    Layers,
    /// 度量。
    Measure { layer: String, kind: &'static str },
    /// 导出。
    Export { layer: String, out: String },
    /// 帮助。
    Help,
    /// 可执行但缺参数的建议。
    Advice(String),
    /// 工具箱工具调用（id + 与参数表对齐的值）。
    RunTool {
        id: &'static str,
        name: &'static str,
        values: Vec<String>,
    },
    /// 工具名歧义（候选中文名清单）。
    AmbiguousTools(Vec<&'static str>),
    /// 未识别。
    Fallback,
}

/// 抽取关键词后的第一个数值。
fn extract_number_after(text: &str, keywords: &[&str]) -> Option<f64> {
    for kw in keywords {
        if let Some(pos) = text.find(kw) {
            let rest = &text[pos + kw.len()..];
            let mut token = String::new();
            let mut started = false;
            for ch in rest.chars() {
                if ch.is_ascii_digit() || (ch == '.' && started) {
                    token.push(ch);
                    started = true;
                } else if started {
                    break;
                }
            }
            if let Ok(d) = token.parse::<f64>() {
                return Some(d);
            }
        }
    }
    // 兜底：全文找第一个数字。
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() || (ch == '.' && !token.is_empty()) {
            token.push(ch);
        } else if !token.is_empty() {
            break;
        }
    }
    token.parse::<f64>().ok()
}

/// 从文本中抽取路径样字符串（含扩展名）。
fn extract_path_like(text: &str) -> Option<&str> {
    text.split_whitespace().find(|tok| {
        tok.rsplit('.')
            .next()
            .map(|e| e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or(false)
            && tok.contains('.')
    })
}

impl AiDriver for LocalDriver {
    fn name(&self) -> &str {
        "本地规则（离线）"
    }

    fn chat(&mut self, history: &[ChatMsg], _context: &str) -> Result<String, String> {
        let _ = history;
        // LocalDriver 的执行由面板直接分派（需要 ConsoleHost 与图层清单），
        // 本方法仅兜底：面板应先走 parse() 分派。
        Ok("（本地规则：请通过面板的意图分派执行）".to_string())
    }
}

/// OpenAI 兼容驱动（ureq，纯 Rust）。
pub struct OpenAiDriver {
    /// 端点。
    pub base_url: String,
    /// 密钥。
    pub api_key: String,
    /// 模型。
    pub model: String,
}

impl AiDriver for OpenAiDriver {
    fn name(&self) -> &str {
        "OpenAI 兼容端点"
    }

    fn chat(&mut self, history: &[ChatMsg], context: &str) -> Result<String, String> {
        if self.api_key.trim().is_empty() {
            return Err("未配置 API Key（点右上角齿轮进入 AI 设置）".to_string());
        }
        let mut messages = vec![serde_json::json!({"role": "system", "content": context})];
        for m in history.iter().rev().take(20).rev() {
            messages.push(serde_json::json!({"role": m.role, "content": m.content}));
        }
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": 0.2,
        });
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut resp = ureq::post(&url)
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|e| format!("请求失败: {e}"))?;
        let text = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("读取响应失败: {e}"))?;
        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("响应非合法 JSON: {e}"))?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| format!("响应缺少 choices[0].message.content: {text:.200}"))
    }
}

/// AI 对话面板（BitFun 式：消息流 + 输入 + 驱动指示 + 设置弹窗）。
pub struct AiChatPanel {
    /// 对话历史。
    pub history: Vec<ChatMsg>,
    /// 输入框。
    pub input: String,
    /// 设置。
    pub settings: AiSettings,
    /// 设置弹窗显隐。
    show_settings: bool,
    /// 等待远程回复中（防重入）。
    busy: bool,
    /// 状态行（错误/提示）。
    note: Option<String>,
}

impl Default for AiChatPanel {
    fn default() -> Self {
        Self {
            history: vec![ChatMsg {
                role: "assistant".to_string(),
                content: "你好，我是堪舆灵。试试：「有哪些图层」「缓冲 buildings 500」「打开 examples/buildings.geojson」。远程驱动在右上角 ⚙ 设置。".to_string(),
            }],
            input: String::new(),
            settings: AiSettings::load(),
            show_settings: false,
            busy: false,
            note: None,
        }
    }
}

impl AiChatPanel {
    /// 演示对话（截图验证：工具缺参引导 + 派发确认回复态）。
    pub fn demo_conversation(&mut self) {
        let fbuf = kanyu_core::tooldef::TOOLS
            .iter()
            .find(|t| t.name == "按字段缓冲区")
            .expect("注册表有按字段缓冲区");
        let layers = vec!["buildings".to_string()];
        self.history = vec![
            ChatMsg {
                role: "assistant".to_string(),
                content: "你好，我是堪舆灵。说「工具中文名 + 参数」即可驱动工具箱，如「缓冲区 buildings 0.1」；「帮助」看全部可用工具。".to_string(),
            },
            ChatMsg {
                role: "user".to_string(),
                content: "按字段缓冲区".to_string(),
            },
            ChatMsg {
                role: "assistant".to_string(),
                content: format!(
                    "「按字段缓冲区」还缺 输入图层、距离字段：{}",
                    usage_of(fbuf, &layers)
                ),
            },
            ChatMsg {
                role: "user".to_string(),
                content: "质心 buildings".to_string(),
            },
            ChatMsg {
                role: "assistant".to_string(),
                content: "已派发「质心」后台执行：「质心」（buildings）。\n完成后新图层/报告见图层面板与终端。".to_string(),
            },
        ];
    }

    /// 面板 UI。host 为命令宿主（与终端同一通道），layers 为当前图层 id 清单。
    pub fn ui(&mut self, ui: &mut egui::Ui, host: &mut dyn ConsoleHost) {
        let layers: Vec<String> = host
            .host_layers()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();

        // 顶行：驱动指示（经 trait 取驱动名）+ 设置齿轮。
        ui.horizontal(|ui| {
            let driver_label = if self.settings.driver == "openai" {
                let d = OpenAiDriver {
                    base_url: self.settings.base_url.clone(),
                    api_key: self.settings.api_key.clone(),
                    model: self.settings.model.clone(),
                };
                format!("驱动: {}（{}）", d.name(), self.settings.model)
            } else {
                format!("驱动: {}", LocalDriver.name())
            };
            hint_caption(ui, &driver_label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rect, resp) =
                    ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::click());
                icons::draw(
                    ui.painter(),
                    Icon::Settings,
                    rect.shrink(2.0),
                    ui.visuals().text_color(),
                );
                if resp.clicked() {
                    self.show_settings = !self.show_settings;
                }
                resp.on_hover_text("AI 设置（驱动/端点/密钥/模型）");
            });
        });
        ui.add_space(2.0);

        // 设置弹窗。
        if self.show_settings {
            crate::ui_kit::card(ui, |ui| {
                crate::ui_kit::section_header(ui, "AI 设置");
                combo_static(
                    ui,
                    "驱动",
                    &mut self.settings.driver,
                    &["local", "openai"],
                    true,
                );
                text_input(
                    ui,
                    "端点",
                    &mut self.settings.base_url,
                    "https://api.openai.com/v1",
                    self.settings.driver == "openai",
                );
                password_input(
                    ui,
                    "密钥",
                    &mut self.settings.api_key,
                    self.settings.driver == "openai",
                );
                text_input(
                    ui,
                    "模型",
                    &mut self.settings.model,
                    "gpt-4o-mini",
                    self.settings.driver == "openai",
                );
                ui.horizontal(|ui| {
                    if button(ui, "保存", ButtonVariant::Primary, true).clicked() {
                        match self.settings.save() {
                            Ok(()) => {
                                self.note = Some("设置已保存".to_string());
                                self.show_settings = false;
                            }
                            Err(e) => self.note = Some(format!("设置保存失败: {e}")),
                        }
                    }
                    if button(ui, "取消", ButtonVariant::Secondary, true).clicked() {
                        self.settings = AiSettings::load();
                        self.show_settings = false;
                    }
                });
            });
        }

        // 消息流。
        let text_height = ui.text_style_height(&egui::TextStyle::Body);
        let output_height = (ui.available_height() - 34.0).max(text_height * 3.0);
        egui::ScrollArea::vertical()
            .id_salt("ai_chat_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .max_height(output_height)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for msg in &self.history {
                    chat_bubble(ui, msg);
                }
                if self.busy {
                    hint_caption(ui, "堪舆灵思考中…");
                }
            });

        if let Some(note) = &self.note {
            hint_caption(ui, note);
        }

        // 输入行。
        let mut send: Option<String> = None;
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .font(egui::FontId::proportional(13.0))
                    .desired_width(f32::INFINITY)
                    .hint_text("对堪舆灵说点什么…（Enter 发送）"),
            );
            let (rect, btn) = ui.allocate_exact_size(egui::Vec2::splat(24.0), egui::Sense::click());
            icons::draw(
                ui.painter(),
                Icon::Send,
                rect.shrink(3.0),
                crate::ui_kit::icons_color(ui),
            );
            if (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || (btn.clicked() && !self.input.trim().is_empty())
            {
                send = Some(std::mem::take(&mut self.input));
            }
            btn.on_hover_text("发送");
        });
        if let Some(text_in) = send {
            self.send(host, &text_in, &layers);
        }
    }

    /// 发送一条消息：本地驱动立即分派执行；远程驱动走 API。
    fn send(&mut self, host: &mut dyn ConsoleHost, text_in: &str, layers: &[String]) {
        let text_in = text_in.trim();
        if text_in.is_empty() {
            return;
        }
        self.history.push(ChatMsg {
            role: "user".to_string(),
            content: text_in.to_string(),
        });

        if self.settings.driver == "openai" {
            // 远程：注入数据现场，走 OpenAI 兼容端点。
            let context = build_context(host);
            let mut driver = OpenAiDriver {
                base_url: self.settings.base_url.clone(),
                api_key: self.settings.api_key.clone(),
                model: self.settings.model.clone(),
            };
            self.busy = true;
            let history_snapshot = self.history.clone();
            let result = driver.chat(&history_snapshot, &context);
            self.busy = false;
            match result {
                Ok(reply) => self.history.push(ChatMsg {
                    role: "assistant".to_string(),
                    content: reply,
                }),
                Err(e) => self.history.push(ChatMsg {
                    role: "assistant".to_string(),
                    content: format!("远程驱动失败: {e}（可切回 local 驱动）"),
                }),
            }
            return;
        }

        // 本地：意图分派到命令通道。
        let intent = {
            let fields_of = |id: &str| host.host_layer_fields(id);
            LocalDriver::parse(text_in, layers, &fields_of)
        };
        let reply = match intent {
            ParsedIntent::RunTool { id, name, values } => match host.host_run_tool(id, &values) {
                Ok(msg) => format!(
                    "{msg}：「{name}」（{}）。\n完成后新图层/报告见图层面板与终端。",
                    values.join(" ")
                ),
                Err(e) => format!("「{name}」未执行: {e}"),
            },
            ParsedIntent::AmbiguousTools(names) => format!(
                "你是指哪个工具？{}\n请说全名（可带参数，如「多环缓冲区 buildings 100,200」）。",
                names.join("、")
            ),
            ParsedIntent::Buffer { layer, distance } => match host.host_buffer(&layer, distance) {
                Ok(msg) => format!("已执行：buffer {layer} {distance}。{msg}"),
                Err(e) => format!("缓冲失败: {e}"),
            },
            ParsedIntent::Load(path) => match host.host_load(&path) {
                Ok(msg) => format!("已执行：load {path}。{msg}"),
                Err(e) => format!("打开失败: {e}"),
            },
            ParsedIntent::Layers => {
                let list = host.host_layers();
                if list.is_empty() {
                    "当前没有图层。可以「打开 examples/buildings.geojson」加载示例。".to_string()
                } else {
                    let lines: Vec<String> = list
                        .iter()
                        .map(|(id, fmt, n)| format!("· {id}（{fmt}，{n} 要素）"))
                        .collect();
                    format!("当前 {} 个图层：\n{}", list.len(), lines.join("\n"))
                }
            }
            ParsedIntent::Measure { layer, kind } => match host.host_measure(&layer, kind) {
                Ok(msg) => format!("{kind} 度量结果：\n{msg}"),
                Err(e) => format!("度量失败: {e}"),
            },
            ParsedIntent::Export { layer, out } => {
                let fmt = out.rsplit('.').next().unwrap_or("").to_string();
                match host.host_export(&layer, &out, &fmt) {
                    Ok(msg) => format!("已执行：export {layer} → {out}。{msg}"),
                    Err(e) => format!("导出失败: {e}"),
                }
            }
            ParsedIntent::Help => format!(
                "{}\n\n快捷命令：「打开 <路径>」「有哪些图层」「量长度/量面积 <图层>」「导出 <图层> <路径>」；更多命令见「终端」页签 help。",
                tools_help()
            ),
            ParsedIntent::Advice(msg) => msg,
            ParsedIntent::Fallback => {
                if layers.is_empty() {
                    "我还没听懂。先「打开 examples/buildings.geojson」，然后说「缓冲 buildings 500」试试。".to_string()
                } else {
                    format!("我还没听懂。当前图层：{}。可以说「缓冲 {} 500」「量面积 {}」或「有哪些图层」。",
                        layers.join(", "), layers[0], layers[0])
                }
            }
        };
        self.history.push(ChatMsg {
            role: "assistant".to_string(),
            content: reply,
        });
    }
}

/// 数据现场系统提示（BitFun 式上下文注入）。
fn build_context(host: &mut dyn ConsoleHost) -> String {
    let layers = host.host_layers();
    let layer_desc = if layers.is_empty() {
        "（无图层）".to_string()
    } else {
        layers
            .iter()
            .map(|(id, fmt, n)| format!("{id}({fmt},{n}要素)"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "你是堪舆（Kanyu）GIS 系统的 AI 助手\"堪舆灵\"。当前数据现场图层: {layer_desc}。\
         用户可让你执行空间操作；你只能建议使用这些命令（不要假装已执行）：\
         buffer/overlay/topology/sjoin/zonal_stats/measure/reproject/export。\
         用简洁中文回答，涉及命令时给出确切的命令文本。"
    )
}

/// 对话气泡（用户右/AI 左）。
fn chat_bubble(ui: &mut egui::Ui, msg: &ChatMsg) {
    let is_user = msg.role == "user";
    let p = crate::theme::palette(if ui.visuals().dark_mode {
        kanyu_render::Theme::Dark
    } else {
        kanyu_render::Theme::Light
    });
    let (bg, align) = if is_user {
        (p.hover, egui::Align::Max)
    } else {
        (p.bg_tertiary, egui::Align::Min)
    };
    ui.with_layout(egui::Layout::top_down(align), |ui| {
        egui::Frame::new()
            .fill(bg)
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.85);
                ui.label(text::body(&msg.content));
            });
    });
    ui.add_space(3.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> Vec<String> {
        vec!["buildings".to_string(), "roads".to_string()]
    }

    /// 字段提取桩（buildings 带 height/name/zone）。
    fn fields_of(id: &str) -> Vec<String> {
        match id {
            "buildings" => vec!["height".into(), "name".into(), "zone".into()],
            _ => Vec::new(),
        }
    }

    #[test]
    fn parse_buffer_with_layer_and_distance() {
        match LocalDriver::parse("缓冲 buildings 500", &layers(), &fields_of) {
            ParsedIntent::Buffer { layer, distance } => {
                assert_eq!(layer, "buildings");
                assert_eq!(distance, 500.0);
            }
            other => panic!("应为 Buffer: {other:?}"),
        }
        match LocalDriver::parse("给 roads 做 200 米缓冲区", &layers(), &fields_of) {
            ParsedIntent::Buffer { layer, distance } => {
                assert_eq!(layer, "roads");
                assert_eq!(distance, 200.0);
            }
            other => panic!("应为 Buffer: {other:?}"),
        }
    }

    #[test]
    fn parse_load_and_layers_and_measure() {
        match LocalDriver::parse("打开 examples/buildings.geojson", &layers(), &fields_of) {
            ParsedIntent::Load(p) => assert_eq!(p, "examples/buildings.geojson"),
            other => panic!("应为 Load: {other:?}"),
        }
        assert!(matches!(
            LocalDriver::parse("有哪些图层", &layers(), &fields_of),
            ParsedIntent::Layers
        ));
        match LocalDriver::parse("量面积 buildings", &layers(), &fields_of) {
            ParsedIntent::Measure { kind, .. } => assert_eq!(kind, "area"),
            other => panic!("应为 Measure: {other:?}"),
        }
    }

    #[test]
    fn parse_fallback_and_buffer_without_layer() {
        assert!(matches!(
            LocalDriver::parse("今天天气如何", &layers(), &fields_of),
            ParsedIntent::Fallback
        ));
        // 无图层名但只有一个图层时自动补全。
        let one = vec!["solo".to_string()];
        match LocalDriver::parse("缓冲 300", &one, &fields_of) {
            ParsedIntent::Buffer { layer, distance } => {
                assert_eq!(layer, "solo");
                assert_eq!(distance, 300.0);
            }
            other => panic!("应自动补全唯一图层: {other:?}"),
        }
        // 多图层缺名 → Advice。
        assert!(matches!(
            LocalDriver::parse("缓冲 300", &layers(), &fields_of),
            ParsedIntent::Advice(_)
        ));
    }

    #[test]
    fn tool_match_exact_longest_name_wins() {
        // 「缓冲区」精确命中 buffer。
        match match_tool_exact("缓冲区 buildings 0.1") {
            ToolMatch::One(def) => assert_eq!(def.id, "buffer"),
            other => panic!("应为 One(buffer): {other:?}"),
        }
        // 「按字段缓冲区 buildings zone」：最长名胜出（不误截为「缓冲区」）。
        match match_tool_exact("按字段缓冲区 buildings zone") {
            ToolMatch::One(def) => assert_eq!(def.name, "按字段缓冲区"),
            other => panic!("应为 One(按字段缓冲区): {other:?}"),
        }
        assert!(matches!(match_tool_exact("今天天气"), ToolMatch::None));
    }

    #[test]
    fn tool_match_prefix_ambiguity_and_unique() {
        // 「缓冲」前缀 → 缓冲区/多环缓冲区/按字段缓冲区 三候选歧义。
        match match_tool_prefix("缓冲") {
            ToolMatch::Ambiguous(names) => {
                assert!(names.contains(&"缓冲区"));
                assert!(names.contains(&"多环缓冲区"));
                assert!(names.contains(&"按字段缓冲区"));
            }
            other => panic!("应为歧义三候选: {other:?}"),
        }
        // 「质心」唯一命中。
        match match_tool_prefix("质心") {
            ToolMatch::One(def) => assert_eq!(def.name, "质心"),
            other => panic!("应为 One(质心): {other:?}"),
        }
    }

    #[test]
    fn extract_values_buffer_and_field_buffer() {
        let buf = kanyu_core::tooldef::find("buffer").unwrap();
        // 图层 + 线性单位（裸数值 → 度直通）。
        let v = extract_tool_values(buf, "缓冲区 buildings 0.1", &layers(), &fields_of).unwrap();
        assert_eq!(v, vec!["buildings".to_string(), "0.1|度".to_string()]);
        // 带单位词。
        let v2 =
            extract_tool_values(buf, "缓冲区 buildings 500 米", &layers(), &fields_of).unwrap();
        assert_eq!(v2[1], "500|米");
        // 缺图层 → Err 用法引导（含参数表与示例）。
        let e = extract_tool_values(buf, "缓冲区 0.1", &layers(), &fields_of).unwrap_err();
        assert!(e.contains("输入图层"), "{e}");
        assert!(e.contains("示例"), "{e}");
        // 按字段缓冲区：图层 + 字段（fields_of 命中）+ 可选整数取默认。
        let fbuf = kanyu_core::tooldef::TOOLS
            .iter()
            .find(|t| t.name == "按字段缓冲区")
            .unwrap();
        let v3 = extract_tool_values(fbuf, "按字段缓冲区 buildings zone", &layers(), &fields_of)
            .unwrap();
        assert_eq!(v3[0], "buildings");
        assert_eq!(v3[1], "zone");
        assert_eq!(v3[2], "16", "可选 segments 取默认值");
    }

    #[test]
    fn parse_tool_intent_end_to_end() {
        // 质心（单图层参数）：直接可执行。
        match LocalDriver::parse("质心 buildings", &layers(), &fields_of) {
            ParsedIntent::RunTool { id, values, .. } => {
                assert_eq!(id, "centroid");
                assert_eq!(values, vec!["buildings".to_string()]);
            }
            other => panic!("应为 RunTool(centroid): {other:?}"),
        }
        // 缺参工具 → Advice 带用法。
        match LocalDriver::parse("按字段缓冲区", &layers(), &fields_of) {
            ParsedIntent::Advice(msg) => assert!(msg.contains("输入图层"), "{msg}"),
            other => panic!("应为 Advice: {other:?}"),
        }
        // 「缓冲」裸词 → 歧义候选。
        match LocalDriver::parse("缓冲", &layers(), &fields_of) {
            ParsedIntent::AmbiguousTools(names) => assert!(names.len() >= 3),
            other => panic!("应为 AmbiguousTools: {other:?}"),
        }
    }

    #[test]
    fn settings_roundtrip() {
        let s = AiSettings {
            driver: "openai".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "test-model".to_string(),
        };
        let text = serde_json::to_string(&s).unwrap();
        let back: AiSettings = serde_json::from_str(&text).unwrap();
        assert_eq!(back.model, "test-model");
        assert_eq!(back.driver, "openai");
    }
}
