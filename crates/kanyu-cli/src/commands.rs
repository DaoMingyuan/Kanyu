//! 命令实现。

use anyhow::{bail, Context, Result};
use kanyu_core::{agents, introspect, FormatRegistry, Layer};

use crate::cli::{AgentsCommand, DataCommand, McpCommand, Transport};

/// `kanyu data ...`
pub fn data(cmd: &DataCommand, json: bool) -> Result<()> {
    match cmd {
        DataCommand::Info { file } => {
            let layer = Layer::load(stem_of(file), file)?;
            print_value(&layer.summary(), json, |s| {
                format!(
                    "图层:      {}\n格式:      {}\n要素数:    {}\n几何类型:  {}\n字段:      {}",
                    s.id,
                    s.format,
                    s.feature_count,
                    s.geometry_types.join(", "),
                    s.fields.join(", ")
                )
            });
        }
        DataCommand::Load { file, alias, crs } => {
            let id = alias.clone().unwrap_or_else(|| stem_of(file));
            let layer = Layer::load(&id, file)?;
            let summary = layer.summary();
            print_value(&summary, json, |s| {
                let crs_note = crs
                    .as_deref()
                    .map(|c| format!("，CRS 声明 {c}"))
                    .unwrap_or_default();
                format!(
                    "已加载图层 '{}'：{} 个要素（{}）{crs_note}",
                    s.id, s.feature_count, s.format
                )
            });
        }
        DataCommand::Query {
            file,
            filter,
            output: out,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = layer.query(filter)?;
            let text = Layer::to_geojson_string(&result);
            match out {
                Some(path) => {
                    std::fs::write(path, &text).with_context(|| format!("写入 {path} 失败"))?;
                    eprintln!("已写出 {} 个要素 → {path}", result.features.len());
                }
                None => println!("{text}"),
            }
        }
        DataCommand::Export {
            file,
            format,
            out,
            symbol_mapping,
        } => {
            let registry = FormatRegistry::builtin();
            let caps = registry.require(format, "write")?;
            if *symbol_mapping && !caps.symbol.usable() {
                bail!("格式 '{format}' 不支持符号化保留（--symbol-mapping 不可用）");
            }
            let layer = Layer::load(stem_of(file), file)?;
            match caps.id {
                "geojson" => {
                    let text = Layer::to_geojson_string(layer.collection());
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (geojson)", layer.len());
                }
                other => {
                    bail!(
                        "格式 '{other}' 的原生导出尚未启用（driver: {}）。\n\
                         桥接/插件驱动将在对应阶段就绪后开放，见 docs/MASTERPLAN.md 第五部分。",
                        caps.driver
                    )
                }
            }
        }
    }
    Ok(())
}

/// `kanyu introspect`
pub fn introspect_cmd(json: bool) -> Result<()> {
    let report = introspect::report();
    print_value(&report, json, |r| {
        let mut s = format!(
            "堪舆内核 v{} ({})\n{}\n\n模块:\n",
            r.version, r.codename, r.manifesto
        );
        for m in &r.modules {
            s.push_str(&format!("  {:<14} [{}] {}\n", m.name, m.status, m.role));
        }
        s.push_str("\nMCP 工具:\n");
        for t in &r.tools {
            s.push_str(&format!("  {:<28} {:<9} [{}]\n", t.name, t.group, t.status));
        }
        s.push_str(&format!(
            "\n格式矩阵: {} 种格式（详见 --json 或 docs/）",
            r.formats.len()
        ));
        s
    });
    Ok(())
}

/// `kanyu agents ...`
pub fn agents_cmd(cmd: &AgentsCommand, json: bool) -> Result<()> {
    match cmd {
        AgentsCommand::Init {
            project,
            name,
            crs,
            force,
        } => {
            let dir = std::path::Path::new(project);
            let name = name.clone().unwrap_or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("kanyu-project")
                    .to_string()
            });
            let path = dir.join("AGENTS.md");
            if path.exists() && !force {
                bail!("{} 已存在（使用 --force 覆盖）", path.display());
            }
            std::fs::create_dir_all(dir)?;
            std::fs::write(&path, agents::template(&name, crs))?;
            eprintln!("已生成 {}", path.display());
        }
        AgentsCommand::Validate { path } => {
            let doc = agents::load(path)?;
            let issues = doc.validate();
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "valid": issues.is_empty(), "issues": issues, "document": doc })
                );
            } else if issues.is_empty() {
                println!(
                    "AGENTS.md 校验通过：{} 个图层，{} 条业务规则",
                    doc.layers.len(),
                    doc.business_rules.len()
                );
            } else {
                for issue in &issues {
                    eprintln!("✗ {issue}");
                }
                bail!("AGENTS.md 校验未通过：{} 个问题", issues.len());
            }
        }
    }
    Ok(())
}

/// `kanyu mcp serve`
pub fn mcp_cmd(cmd: &McpCommand) -> Result<()> {
    match cmd {
        McpCommand::Serve { transport, port } => match transport {
            Transport::Stdio => {
                eprintln!(
                    "kanyu-mcp: MCP server 监听 stdio（initialize / tools/list / tools/call）"
                );
                kanyu_mcp::serve_stdio().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            }
            Transport::Sse => {
                bail!("SSE 传输（端口 {port}）将在 kanyu-mcp v0.2 提供（rmcp streamable HTTP）；当前请使用 --transport=stdio");
            }
        },
    }
    Ok(())
}

/// 按全局 --json 标志打印一个可序列化值。
fn print_value<T: serde::Serialize>(value: &T, json: bool, human: impl FnOnce(&T) -> String) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value).expect("序列化失败")
        );
    } else {
        println!("{}", human(value));
    }
}

/// 取文件名主干作为默认图层名。
fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string()
}
