//! 命令实现。

use anyhow::{bail, Context, Result};
use kanyu_core::{agents, introspect, FormatRegistry, Layer};

use crate::cli::{
    AgentsCommand, AnalysisCommand, DataCommand, McpCommand, RenderCommand, SkillCommand, Transport,
};

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
            // kmz 是 kml 的 zip 容器变体（非独立格式条目）：按 kml 校验后分流。
            let caps = if format == "kmz" {
                registry.require("kml", "write")?
            } else {
                registry.require(format, "write")?
            };
            if *symbol_mapping && !caps.symbol.usable() {
                bail!("格式 '{format}' 不支持符号化保留（--symbol-mapping 不可用）");
            }
            let layer = Layer::load(stem_of(file), file)?;
            match caps.id {
                "geojson" => {
                    let text = Layer::to_geojson_string(&layer.collection());
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (geojson)", layer.len());
                }
                "csv" => {
                    let text = Layer::to_csv_string(&layer.collection())?;
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (csv)", layer.len());
                }
                "fgb" => {
                    let bytes = Layer::to_fgb_bytes(&layer.collection())?;
                    std::fs::write(out, bytes).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (fgb)", layer.len());
                }
                "geoparquet" => {
                    let bytes = Layer::to_geoparquet_bytes(&layer.collection())?;
                    std::fs::write(out, bytes).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (geoparquet)", layer.len());
                }
                "dxf" => {
                    let text = Layer::to_dxf_string(&layer.collection())?;
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (dxf)", layer.len());
                }
                "kml" if format == "kmz" => {
                    let bytes = Layer::to_kmz_bytes(&layer.collection())?;
                    std::fs::write(out, bytes).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (kmz)", layer.len());
                }
                "shp" => {
                    // shp 为三件套：out 去扩展名作 base。
                    let base = strip_shp_extension(out);
                    Layer::write_shp(&layer.collection(), base)?;
                    eprintln!("已导出 {} 个要素 → {base}.shp/.shx/.dbf (shp)", layer.len());
                }
                "kdb" => {
                    // 堪舆数据库：RecordBatch 直通（类型保真，不经 GeoJSON 中间层）。
                    std::fs::write(out, layer.to_kdb_bytes()?)
                        .with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (kdb 堪舆数据库)", layer.len());
                }
                "kml" => {
                    let text = Layer::to_kml_string(&layer.collection())?;
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (kml)", layer.len());
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
        DataCommand::Reproject {
            file,
            from,
            to,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::crs::reproject(&layer.collection(), from, to)?;
            write_geojson_result(&result, output.as_deref())?;
        }
    }
    Ok(())
}

/// shp 为三件套：路径去 `.shp` 扩展名（大小写不敏感）作为 base。
fn strip_shp_extension(out: &str) -> &str {
    if out.len() > 4 && out[out.len() - 4..].eq_ignore_ascii_case(".shp") {
        &out[..out.len() - 4]
    } else {
        out
    }
}

/// `kanyu analysis ...`
pub fn analysis(cmd: &AnalysisCommand, json: bool) -> Result<()> {
    match cmd {
        AnalysisCommand::Buffer {
            file,
            distance,
            segments,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::analysis::buffer(&layer.collection(), *distance, *segments)?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Overlay {
            target,
            overlay,
            operation,
            output,
        } => {
            let target_layer = Layer::load(stem_of(target), target)?;
            let overlay_layer = Layer::load(stem_of(overlay), overlay)?;
            let op: kanyu_core::analysis::OverlayOp = operation.parse()?;
            let result = kanyu_core::analysis::overlay(
                &target_layer.collection(),
                &overlay_layer.collection(),
                op,
            )?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Topology { file, rules } => {
            let layer = Layer::load(stem_of(file), file)?;
            let rules: Vec<kanyu_core::analysis::TopologyRule> = rules
                .split(',')
                .map(|s| s.trim().parse())
                .collect::<std::result::Result<_, _>>()?;
            let report = kanyu_core::analysis::topology_check(&layer.collection(), &rules)?;
            print_value(&report, json, |r| {
                if r.violation_count == 0 {
                    format!(
                        "拓扑检查通过：{} 个要素，规则 {}，无违规",
                        r.feature_count, r.rule
                    )
                } else {
                    let mut s = format!(
                        "拓扑检查发现 {} 条违规（规则 {}，{} 个要素）:",
                        r.violation_count, r.rule, r.feature_count
                    );
                    for v in &r.violations {
                        s.push_str(&format!(
                            "\n  要素 {} × 要素 {}：{}",
                            v.feature_a, v.feature_b, v.note
                        ));
                    }
                    s
                }
            });
        }
        AnalysisCommand::Measure { file, kind } => {
            let layer = Layer::load(stem_of(file), file)?;
            let kind: kanyu_core::crs::MeasureKind = kind.parse()?;
            let report = kanyu_core::crs::measure(&layer.collection(), kind)?;
            print_value(&report, json, |r| {
                let kind_zh = if r["kind"] == "length" {
                    "长度"
                } else {
                    "面积"
                };
                format!(
                    "测地线{kind_zh}总计: {:.3} {}（{} 个要素；--json 见逐要素明细）",
                    r["total"].as_f64().unwrap_or_default(),
                    r["unit"].as_str().unwrap_or_default(),
                    r["per_feature"].as_array().map(Vec::len).unwrap_or(0)
                )
            });
        }
        AnalysisCommand::Sjoin {
            target,
            join,
            predicate,
            output,
        } => {
            let target_layer = Layer::load(stem_of(target), target)?;
            let join_layer = Layer::load(stem_of(join), join)?;
            let predicate: kanyu_core::analysis::SpatialPredicate = predicate.parse()?;
            let result = kanyu_core::analysis::sjoin(
                &target_layer.collection(),
                &join_layer.collection(),
                predicate,
            )?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Zonal {
            zones,
            values,
            field,
            stats,
            output,
        } => {
            let zones_layer = Layer::load(stem_of(zones), zones)?;
            let values_layer = Layer::load(stem_of(values), values)?;
            let stats: Vec<kanyu_core::analysis::ZonalStat> = stats
                .split(',')
                .map(|s| s.trim().parse())
                .collect::<std::result::Result<_, _>>()?;
            let result = kanyu_core::analysis::zonal_stats(
                &zones_layer.collection(),
                &values_layer.collection(),
                field,
                &stats,
            )?;
            write_geojson_result(&result, output.as_deref())?;
        }
    }
    Ok(())
}

/// buffer/overlay 结果写出：有 --output 写文件（提示走 stderr），否则打 stdout。
fn write_geojson_result(result: &geojson::FeatureCollection, output: Option<&str>) -> Result<()> {
    let text = Layer::to_geojson_string(result);
    match output {
        Some(path) => {
            std::fs::write(path, &text).with_context(|| format!("写入 {path} 失败"))?;
            eprintln!("已写出 {} 个要素 → {path}", result.features.len());
        }
        None => println!("{text}"),
    }
    Ok(())
}

/// `kanyu render ...`
pub fn render(cmd: &RenderCommand) -> Result<()> {
    match cmd {
        RenderCommand::Map {
            file,
            out,
            width,
            height,
            theme,
            style,
            style_file,
        } => {
            let style_rule: Option<kanyu_render::StyleRule> = match (style, style_file) {
                (Some(_), Some(_)) => {
                    bail!("--style 与 --style-file 二选一，不可同时指定")
                }
                (Some(s), None) => Some(
                    serde_json::from_str(s)
                        .map_err(|e| anyhow::anyhow!("样式规则 JSON 解析失败: {e}"))?,
                ),
                (None, Some(p)) => {
                    let text = std::fs::read_to_string(p)
                        .with_context(|| format!("读取样式文件 {p} 失败"))?;
                    Some(
                        serde_json::from_str(&text)
                            .map_err(|e| anyhow::anyhow!("样式规则 JSON 解析失败（{p}）: {e}"))?,
                    )
                }
                (None, None) => None,
            };
            let opts = kanyu_render::RenderOptions {
                width: *width,
                height: *height,
                theme: theme.parse()?,
                style: style_rule,
                ..Default::default()
            };
            let layer = Layer::load(stem_of(file), file)?;
            let ext = out
                .rsplit('.')
                .next()
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            match ext.as_str() {
                "png" => {
                    let bytes = kanyu_render::render_png(&layer.collection(), &opts)?;
                    std::fs::write(out, &bytes).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!(
                        "已渲染 {} 个要素 → {out} (png, {}x{}, {})",
                        layer.len(),
                        opts.width,
                        opts.height,
                        opts.theme.name()
                    );
                }
                "svg" => {
                    let text = kanyu_render::render_svg(&layer.collection(), &opts)?;
                    std::fs::write(out, &text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!(
                        "已渲染 {} 个要素 → {out} (svg, {}x{}, {})",
                        layer.len(),
                        opts.width,
                        opts.height,
                        opts.theme.name()
                    );
                }
                other => {
                    bail!("输出格式按扩展名判定，仅支持 .png/.svg（实际: '.{other}'）")
                }
            }
        }
    }
    Ok(())
}

/// `kanyu skill ...`
pub fn skill(cmd: &SkillCommand, json: bool) -> Result<()> {
    match cmd {
        SkillCommand::Info { plugin } => {
            let host = kanyu_skill::SkillHost::new()?;
            let skill = host.load(plugin)?;
            print_value(skill.meta(), json, |m| {
                format!(
                    "技能:    {}\n版本:    {}\n能力:    {}",
                    m.name,
                    m.version,
                    m.capabilities.join(", ")
                )
            });
        }
        SkillCommand::Run {
            plugin,
            file,
            output,
        } => {
            let host = kanyu_skill::SkillHost::new()?;
            let skill = host.load(plugin)?;
            let layer = Layer::load(stem_of(file), file)?;
            let result = host.run(&skill, &layer.collection())?;
            write_geojson_result(&result, output.as_deref())?;
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
                let payload = serde_json::json!({ "valid": issues.is_empty(), "issues": issues, "document": doc });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&payload).expect("序列化失败")
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
            Transport::Http => {
                kanyu_mcp::serve_http(*port).map_err(|e| anyhow::anyhow!(e.to_string()))?;
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
