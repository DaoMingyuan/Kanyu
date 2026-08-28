//! 命令实现。

use anyhow::{bail, Context, Result};
use kanyu_core::{agents, introspect, FormatRegistry, Layer};

use crate::cli::{
    AgentsCommand, AnalysisCommand, CrsCommand, DataCommand, McpCommand, RenderCommand,
    SkillCommand, ToolCommand, ToolboxCommand, Transport,
};

/// `kanyu tool ...`（core::tooldef 注册表 + toolrun::run_tool 统一执行入口；
/// 与壳层工具箱面板/MCP 工具面同一单一事实来源）。
pub fn tool(cmd: &ToolCommand, json: bool) -> Result<()> {
    match cmd {
        ToolCommand::List => {
            let tools = kanyu_core::tooldef::TOOLS;
            print_value(&tools, json, |list| {
                list.iter()
                    .map(|t| {
                        format!(
                            "{:<28} {}（{}）—— {}",
                            t.id,
                            t.name,
                            t.category.label(),
                            t.desc
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        ToolCommand::Run { id, params, output } => {
            let def = kanyu_core::tooldef::find(id)
                .ok_or_else(|| anyhow::anyhow!("未知工具: {id}（kanyu tool list 查看注册表）"))?;
            // --param k=v → 键值表；按 def.params 序组装 values（缺省取参数默认值）。
            let mut kv = std::collections::HashMap::new();
            for p in params {
                let (k, v) = p
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("参数须为 k=v 形式: '{p}'"))?;
                kv.insert(k.trim().to_string(), v.to_string());
            }
            let values: Vec<String> = def
                .params
                .iter()
                .map(|p| {
                    let v = kv
                        .get(p.key)
                        .cloned()
                        .unwrap_or_else(|| p.default.to_string());
                    // 多图层参数：CLI 侧允许逗号分隔，规范化为内核的换行分隔承载。
                    if matches!(p.kind, kanyu_core::tooldef::ParamKind::MultiLayers) {
                        v.replace(',', "\n")
                    } else {
                        v
                    }
                })
                .collect();
            // Layer 类参数值 = 数据文件路径：执行前预加载进图层表
            // （run_tool 的 get_layer 为 Fn 只读闭包，同名路径只载一次）。
            let mut cache: std::collections::HashMap<String, geojson::FeatureCollection> =
                std::collections::HashMap::new();
            for (p, v) in def.params.iter().zip(&values) {
                let paths: Vec<String> = match p.kind {
                    kanyu_core::tooldef::ParamKind::Layer => {
                        if v.trim().is_empty() {
                            vec![]
                        } else {
                            vec![v.trim().to_string()]
                        }
                    }
                    kanyu_core::tooldef::ParamKind::MultiLayers => {
                        kanyu_core::toolrun::parse_multi_layers(v)
                    }
                    _ => vec![],
                };
                for path in paths {
                    if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(path) {
                        let layer = Layer::load(stem_of(e.key()), e.key())
                            .with_context(|| format!("加载图层 '{}' 失败", e.key()))?;
                        e.insert(layer.collection());
                    }
                }
            }
            let outcome =
                kanyu_core::toolrun::run_tool(id, &values, |path| cache.get(path).cloned())
                    .map_err(|e| anyhow::anyhow!(e))?;
            use kanyu_core::toolrun::ToolOutcome;
            match outcome {
                ToolOutcome::Report(text) => {
                    if json {
                        println!("{}", serde_json::json!({ "tool": id, "report": text }));
                    } else {
                        println!("{text}");
                    }
                }
                ToolOutcome::NewLayer { collection, .. } => {
                    write_geojson_result(&collection, output.as_deref())?;
                }
                ToolOutcome::NewLayers { layers, .. } => {
                    let dir = output.as_deref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "工具 {id} 产出 {} 个图层，须以 --output 指定输出目录",
                            layers.len()
                        )
                    })?;
                    std::fs::create_dir_all(dir).with_context(|| format!("创建 {dir} 失败"))?;
                    for (base, collection) in &layers {
                        let path = format!("{dir}/{base}.geojson");
                        std::fs::write(&path, Layer::to_geojson_string(collection))
                            .with_context(|| format!("写入 {path} 失败"))?;
                        eprintln!("已写出 {} 个要素 → {path}", collection.features.len());
                    }
                }
            }
        }
    }
    Ok(())
}

/// `kanyu crs ...`（EPSG 全库检索/检视，直连 kanyu_core::crs 单一事实来源）。
pub fn crs(cmd: &CrsCommand, json: bool) -> Result<()> {
    match cmd {
        CrsCommand::Search { query, limit } => {
            let results = kanyu_core::crs::search_crs(query.as_deref().unwrap_or(""), *limit);
            print_value(&results, json, |list| {
                if list.is_empty() {
                    return format!(
                        "无匹配条目（检索词 '{}'；内置 EPSG 库代码域 2000..=32766）",
                        query.as_deref().unwrap_or("")
                    );
                }
                list.iter()
                    .map(|c| {
                        format!(
                            "EPSG:{:<6} {}（{}，{}）",
                            c.code,
                            c.name,
                            crs_kind_cn(c.kind),
                            c.unit
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        CrsCommand::Info { code } => {
            let info = kanyu_core::crs::crs_info(*code).ok_or_else(|| {
                anyhow::anyhow!("EPSG:{code} 不在内置 EPSG 数据库（代码域 2000..=32766）")
            })?;
            let proj4 = kanyu_core::crs::crs_proj4_def(*code).unwrap_or("");
            print_value(&info, json, |c| {
                format!(
                    "EPSG:{}  {}\n类型:    {}\n单位:    {}\nproj4:   {}",
                    c.code,
                    c.name,
                    crs_kind_cn(c.kind),
                    c.unit,
                    proj4
                )
            });
        }
    }
    Ok(())
}

/// CRS 类型中文标签（人机输出用；JSON 输出走 serde 原名）。
fn crs_kind_cn(kind: kanyu_core::crs::CrsKind) -> &'static str {
    use kanyu_core::crs::CrsKind;
    match kind {
        CrsKind::Geographic => "地理坐标系",
        CrsKind::Projected => "投影坐标系",
        CrsKind::Other => "其他",
    }
}

/// `kanyu data ...`
pub fn data(cmd: &DataCommand, json: bool) -> Result<()> {
    match cmd {
        DataCommand::Info { file } => {
            // kdb v2 多图层容器：展开图层清单（v1/其他格式走原路径）。
            let is_kdb_v2 = std::path::Path::new(file)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("kdb"))
                && std::fs::read(file)
                    .map(|b| kanyu_core::kdb::is_kdb_v2(&b))
                    .unwrap_or(false);
            if is_kdb_v2 {
                let layers = Layer::load_kdb_layers(file)?;
                let summaries: Vec<_> = layers.iter().map(|l| l.summary()).collect();
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "id": stem_of(file),
                            "format": "kdb",
                            "format_version": "2",
                            "layer_count": summaries.len(),
                            "layers": summaries,
                        }))?
                    );
                } else {
                    println!("图层:      {}", stem_of(file));
                    println!("格式:      kdb（v2 多图层容器，{} 图层）", summaries.len());
                    for s in &summaries {
                        let extent = s.extent.map_or("（空）".to_string(), |e| {
                            format!("[{:.6}, {:.6}] → [{:.6}, {:.6}]", e[0], e[1], e[2], e[3])
                        });
                        println!(
                            "  ── {}：{} 要素（{}），范围 {}，字段: {}",
                            s.id,
                            s.feature_count,
                            s.geometry_types.join(", "),
                            extent,
                            s.fields.join(", ")
                        );
                    }
                }
                return Ok(());
            }
            let layer = Layer::load(stem_of(file), file)?;
            print_value(&layer.summary(), json, |s| {
                let extent = s.extent.map_or("（空）".to_string(), |e| {
                    format!("[{:.6}, {:.6}] → [{:.6}, {:.6}]", e[0], e[1], e[2], e[3])
                });
                format!(
                    "图层:      {}\n格式:      {}\n要素数:    {}\n几何类型:  {}\n范围:      {}\n字段:      {}",
                    s.id,
                    s.format,
                    s.feature_count,
                    s.geometry_types.join(", "),
                    extent,
                    s.fields.join(", ")
                )
            });
        }
        DataCommand::Validate { file } => {
            let registry = FormatRegistry::builtin();
            let caps = registry
                .detect(file)
                .ok_or_else(|| anyhow::anyhow!("无法识别的格式: {file}"))?;
            if caps.id != "txt" {
                bail!(
                    "质检当前仅支持宗地 TXT（.txt）；'{file}' 为 {} 格式",
                    caps.id
                );
            }
            let text =
                std::fs::read_to_string(file).with_context(|| format!("读取 {file} 失败"))?;
            let issues = kanyu_core::parcel::validate_parcel_txt(&text);
            if json {
                println!("{}", serde_json::to_string_pretty(&issues)?);
            } else if issues.is_empty() {
                println!("质检通过：{file}");
            } else {
                let errors = issues.iter().filter(|i| i.level == "错误").count();
                let warnings = issues.len() - errors;
                for issue in &issues {
                    let loc = if issue.line > 0 {
                        format!("第 {} 行", issue.line)
                    } else {
                        "文档级".to_string()
                    };
                    eprintln!("[{}] {}: {}", issue.level, loc, issue.message);
                }
                if errors > 0 {
                    bail!("质检未通过：{errors} 错误 / {warnings} 警告");
                }
                println!("质检通过（{warnings} 条警告）：{file}");
            }
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
                "txt" => {
                    // 宗地 TXT：面要素 → 界址点坐标文本（X北Y东测绘惯例）。
                    let text = kanyu_core::parcel::collection_to_parcel_txt(
                        &layer.collection(),
                        4,
                        "EPSG:4326",
                    )?;
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (txt 宗地界址点)", layer.len());
                }
                "dat" => {
                    // CASS 坐标数据文件：点要素 → 点号,编码,Y东,X北,H（CASS 标准轴序）。
                    let text = Layer::to_cass_dat_string(&layer.collection(), 3)?;
                    std::fs::write(out, text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!("已导出 {} 个要素 → {out} (dat CASS 坐标)", layer.len());
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
        DataCommand::Calc {
            file,
            target,
            expr,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::attrcalc::calc_field(&layer.collection(), target, expr)?;
            write_geojson_result(&result, output.as_deref())?;
        }
        DataCommand::KdbPack { files, out } => {
            if files.is_empty() {
                bail!("kdb-pack 至少需要一个输入文件");
            }
            let mut layers: Vec<kanyu_core::kdb::KdbLayer> = Vec::new();
            for f in files {
                let stem = stem_of(f);
                if layers.iter().any(|l| l.name == stem) {
                    bail!("图层名重复（{stem}）：请重命名输入文件之一");
                }
                let layer = Layer::load(stem.clone(), f)?;
                layers.push(kanyu_core::kdb::KdbLayer {
                    name: stem,
                    batch: layer.batch().clone(),
                });
            }
            let bytes = kanyu_core::kdb::layers_to_kdb(&layers)?;
            std::fs::write(out, &bytes).with_context(|| format!("写入 {out} 失败"))?;
            eprintln!(
                "已打包 {} 图层（{}）→ {out} (kdb v2 多图层容器)",
                layers.len(),
                layers
                    .iter()
                    .map(|l| l.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
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
        AnalysisCommand::Dissolve {
            file,
            field,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::dissolve(&layer.collection(), field.as_deref())?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Simplify {
            file,
            tolerance,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::simplify(&layer.collection(), *tolerance)?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Centroid { file, output } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::centroid(&layer.collection())?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Convexhull { file, output } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::convex_hull(&layer.collection())?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Deleteholes {
            file,
            min_area,
            output,
        } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::delete_holes(&layer.collection(), *min_area)?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Explode { file, output } => {
            let layer = Layer::load(stem_of(file), file)?;
            let result = kanyu_core::geoprocess::explode(&layer.collection())?;
            write_geojson_result(&result, output.as_deref())?;
        }
        AnalysisCommand::Stats { file } => {
            let layer = Layer::load(stem_of(file), file)?;
            let report = kanyu_core::geoprocess::stats(&layer.collection())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("图层统计（{}）:", stem_of(file));
                println!(
                    "  要素: {}（点 {} / 线 {} / 面 {} / 其他 {}）",
                    report.feature_count,
                    report.points,
                    report.lines,
                    report.polygons,
                    report.other
                );
                println!("  总长度: {:.3} km", report.total_length_km);
                println!(
                    "  总面积: {:.2} ㎡（{:.4} 公顷 / {:.4} 亩 / {:.6} km²）",
                    report.total_area_m2,
                    report.total_area_hectare,
                    report.total_area_mu,
                    report.total_area_km2
                );
                println!("  总周长: {:.1} m", report.total_perimeter_m);
            }
        }
        AnalysisCommand::Bench { size } => {
            analysis_bench(*size, std::path::Path::new("target/bench"), json)?;
        }
    }
    Ok(())
}

/// 单项基准结果。
#[derive(serde::Serialize)]
struct BenchRow {
    /// 基准项目。
    item: &'static str,
    /// 输入要素数（overlay 为单侧图层要素数）。
    features: usize,
    /// 三次耗时（毫秒）。
    runs_ms: [f64; 3],
    /// 中位耗时（毫秒）。
    median_ms: f64,
    /// 吞吐（要素/秒，按中位耗时折算）。
    features_per_sec: f64,
}

/// 单项执行 3 次取中位数（返回升序耗时，中位 = `[1]`）；闭包返回产出要素数
/// （仅防优化吞掉计算）。
fn bench3(mut f: impl FnMut() -> usize) -> [f64; 3] {
    let mut runs = [0.0; 3];
    for run in runs.iter_mut() {
        let start = std::time::Instant::now();
        let produced = f();
        *run = start.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(produced);
    }
    runs.sort_by(f64::total_cmp);
    runs
}

/// `kanyu analysis bench`：确定性场景 + Instant 计时，每项 3 次取中位数。
/// dir 为场景文件落盘目录（CLI 传 target/bench；测试传临时目录）。
fn analysis_bench(size: usize, dir: &std::path::Path, json: bool) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("创建 {} 失败", dir.display()))?;

    // 场景（种子 42 固定；overlay 规模 √size 配比，sjoin 连接侧固定 16 格——
    // 两者均为 O(n·m) 朴素实现，避免平方项失控）。
    let mixed = kanyu_core::bench::mixed(size, 42);
    let ov_n = ((size as f64).sqrt().round() as usize).max(4);
    let (ov_a, ov_b) = kanyu_core::bench::overlay_pair(ov_n, 42);
    let (join_grid, _) = kanyu_core::bench::overlay_pair(16, 7);

    // 场景落盘（加载解析项的输入；大文件不入仓库）。
    let mixed_path = dir.join(format!("mixed_{size}.geojson"));
    std::fs::write(&mixed_path, Layer::to_geojson_string(&mixed))
        .with_context(|| format!("写入 {} 失败", mixed_path.display()))?;
    std::fs::write(
        dir.join(format!("overlay_a_{ov_n}.geojson")),
        Layer::to_geojson_string(&ov_a),
    )?;
    std::fs::write(
        dir.join(format!("overlay_b_{ov_n}.geojson")),
        Layer::to_geojson_string(&ov_b),
    )?;

    // 单项执行 3 次取中位数。
    let mut rows: Vec<BenchRow> = Vec::new();
    let mut push = |item: &'static str, features: usize, runs: [f64; 3]| {
        let median = runs[1];
        rows.push(BenchRow {
            item,
            features,
            runs_ms: runs,
            median_ms: median,
            features_per_sec: if median > 0.0 {
                features as f64 / (median / 1000.0)
            } else {
                f64::INFINITY
            },
        });
    };

    // 加载解析（GeoJSON 文本 → Layer）。
    let path_str = mixed_path.to_string_lossy().to_string();
    let runs = bench3(|| Layer::load("mixed".to_string(), &path_str).unwrap().len());
    push("加载解析", size, runs);
    // buffer（CRS 单位 0.01° ≈ 1km，segments=8 与 CLI 默认一致）。
    let runs = bench3(|| {
        kanyu_core::analysis::buffer(&mixed, 0.01, 8)
            .unwrap()
            .features
            .len()
    });
    push("buffer", size, runs);
    // overlay union（√size × √size 面格对）。
    let runs = bench3(|| {
        kanyu_core::analysis::overlay(&ov_a, &ov_b, kanyu_core::analysis::OverlayOp::Union)
            .unwrap()
            .features
            .len()
    });
    push("overlay_union", ov_n, runs);
    // sjoin（混合图层 × 16 格网面，intersects）。
    let runs = bench3(|| {
        kanyu_core::analysis::sjoin(
            &mixed,
            &join_grid,
            kanyu_core::analysis::SpatialPredicate::Intersects,
        )
        .unwrap()
        .features
        .len()
    });
    push("sjoin", size, runs);
    // render_png（800×600 晨山；CPU 离屏管线）。
    let opts = kanyu_render::RenderOptions {
        width: 800,
        height: 600,
        ..Default::default()
    };
    let runs = bench3(|| kanyu_render::render_png(&mixed, &opts).unwrap().len());
    push("render_png", size, runs);

    if json {
        let payload = serde_json::json!({
            "size": size,
            "overlay_pairs": ov_n,
            "sjoin_join_features": 16,
            "results": rows,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("性能基准（规模 {size} 要素，overlay 单侧 {ov_n} 格，每项 3 次取中位数）:");
        println!(
            "{:<14}{:>12}{:>14}{:>18}",
            "项目", "要素数", "中位耗时", "吞吐(要素/秒)"
        );
        for r in &rows {
            println!(
                "{:<14}{:>12}{:>12.1}ms{:>18.0}",
                r.item, r.features, r.median_ms, r.features_per_sec
            );
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
            background,
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
                background: background.clone(),
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
        RenderCommand::Layout {
            file,
            out,
            title,
            page,
            dpi,
            no_legend,
            no_scalebar,
            no_north,
            theme,
            style,
            style_file,
        } => {
            // 样式规则解析与 render map 同路径。
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
            use kanyu_render::layout::{LayoutFrame, LayoutSpec, PageSize};
            let page_size = match page.as_str() {
                "a4l" => PageSize::A4Landscape,
                "a4p" => PageSize::A4Portrait,
                other => bail!("纸张仅支持 a4l（A4 横）/a4p（A4 纵）（实际: '{other}'）"),
            };
            let spec = LayoutSpec {
                page: page_size,
                dpi: *dpi,
                title: title.clone().unwrap_or_default(),
                show_legend: !no_legend,
                show_scalebar: !no_scalebar,
                show_north: !no_north,
            };
            let layer = Layer::load(stem_of(file), file)?;
            let collection = layer.collection();
            let frame = LayoutFrame::compute(&spec);
            // 图例行：样式规则分类直通（graduated ≤ 阈值 / categorical 类别值）；
            // 无样式留空（排版器对空图例不绘，对齐壳层空集语义）。
            let legend = layout_legend_rows(style_rule.as_ref());
            let opts = kanyu_render::RenderOptions {
                width: frame.map[2].round().max(1.0) as u32,
                height: frame.map[3].round().max(1.0) as u32,
                theme: theme.parse()?,
                style: style_rule,
                ..Default::default()
            };
            // 比例尺：视口东西跨度 × 赤道近似米/度（示意级，与壳层 layoutview 同式）。
            let scale = if spec.show_scalebar {
                kanyu_render::collection_extent(&collection)
                    .ok()
                    .flatten()
                    .map(|b| {
                        let span_m = (b[2] - b[0]).abs() * 111320.0;
                        let (label, bar_px, _bar_m) =
                            kanyu_render::layout::nice_scale(span_m, frame.map[2], spec.dpi);
                        (label, bar_px)
                    })
            } else {
                None
            };
            let scale_ref = scale.as_ref().map(|(l, p)| (l.as_str(), *p));
            let ext = out
                .rsplit('.')
                .next()
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            match ext.as_str() {
                "png" => {
                    let map_png = kanyu_render::render_png(&collection, &opts)?;
                    let bytes = kanyu_render::layout::render_layout_png(
                        &spec, &map_png, &legend, scale_ref,
                    )?;
                    std::fs::write(out, &bytes).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!(
                        "已排版 {} 个要素 → {out} (layout png, {}x{}px, {}dpi)",
                        layer.len(),
                        spec.page.pixels(spec.dpi).0,
                        spec.page.pixels(spec.dpi).1,
                        spec.dpi
                    );
                }
                "svg" => {
                    let map_svg = kanyu_render::render_svg(&collection, &opts)?;
                    let text = kanyu_render::layout::render_layout_svg(
                        &spec, &map_svg, &legend, scale_ref,
                    );
                    std::fs::write(out, &text).with_context(|| format!("写入 {out} 失败"))?;
                    eprintln!(
                        "已排版 {} 个要素 → {out} (layout svg, {}x{}px, {}dpi)",
                        layer.len(),
                        spec.page.pixels(spec.dpi).0,
                        spec.page.pixels(spec.dpi).1,
                        spec.dpi
                    );
                }
                other => {
                    bail!("输出格式按扩展名判定，仅支持 .png/.svg（实际: '.{other}'）")
                }
            }
        }
        RenderCommand::ParcelMap(args) => {
            parcel_map_render(args, kanyu_render::parcelmap::ParcelMapKind::UseRight)?;
        }
        RenderCommand::ParcelOwnershipMap(args) => {
            parcel_map_render(args, kanyu_render::parcelmap::ParcelMapKind::Ownership)?;
        }
        RenderCommand::ParcelSketchMap(args) => {
            parcel_map_render(args, kanyu_render::parcelmap::ParcelMapKind::Sketch)?;
        }
        RenderCommand::SeaBoundaryMap(args) => {
            sea_map_render(args, kanyu_render::seamap::SeaMapKind::BoundaryMap)?;
        }
        RenderCommand::SeaLocationMap(args) => {
            sea_map_render(args, kanyu_render::seamap::SeaMapKind::LocationMap)?;
        }
        RenderCommand::SeaLayoutMap(args) => {
            sea_map_render(args, kanyu_render::seamap::SeaMapKind::LayoutMap)?;
        }
        RenderCommand::IslandRangeMap(args) => {
            island_map_render(args, kanyu_render::islandmap::IslandMapKind::RangeMap)?;
        }
        RenderCommand::IslandFacilityMap(args) => {
            island_map_render(args, kanyu_render::islandmap::IslandMapKind::FacilityMap)?;
        }
        RenderCommand::ParcelDxf {
            file,
            out,
            parcel_code,
            land_use,
            owner,
            scale,
            no_xdata,
            index,
        } => {
            use kanyu_core::cartography::{generate_boundary_lines, generate_boundary_points};
            let (boundary, props) = parcel_boundary_from_file(file, *index)?;
            let points = generate_boundary_points(&boundary, "J");
            let lines = generate_boundary_lines(&boundary, &points);
            let spec = kanyu_core::cass::CassDxfSpec {
                scale: *scale,
                parcel_code: parcel_code
                    .clone()
                    .or_else(|| prop_str(&props, &["parcel_id", "ZDDM", "zddm"]))
                    .unwrap_or_default(),
                land_use: land_use
                    .clone()
                    .or_else(|| prop_str(&props, &["parcel_use", "YT"]))
                    .unwrap_or_default(),
                owner: owner
                    .clone()
                    .or_else(|| prop_str(&props, &["owner", "QLRMC", "parcel_name"]))
                    .unwrap_or_default(),
                xdata: !no_xdata,
            };
            let text = kanyu_core::cass::parcel_to_cass_dxf(&boundary, &points, &lines, &spec)?;
            std::fs::write(out, &text).with_context(|| format!("写入 {out} 失败"))?;
            eprintln!(
                "已导出 CASS 兼容 DXF → {out}（界址点 {}、界址线 {}，SOUTH XDATA {}）",
                points.len(),
                lines.len(),
                if *no_xdata { "关" } else { "开" }
            );
        }
    }
    Ok(())
}

/// 宗地图件出图共用实现（使用权宗地图 L.3 / 所有权宗地图 L.4）。
fn parcel_map_render(
    args: &crate::cli::ParcelMapArgs,
    kind: kanyu_render::parcelmap::ParcelMapKind,
) -> Result<()> {
    use kanyu_render::parcelmap::{
        render_parcel_map_png, render_parcel_map_svg, ParcelMapData, ParcelMapSpec,
    };
    let (boundary, props) = parcel_boundary_from_file(&args.file, args.index)?;
    let spec = ParcelMapSpec {
        kind,
        parcel_code: args
            .parcel_code
            .clone()
            .or_else(|| prop_str(&props, &["parcel_id", "ZDDM", "zddm"]))
            .unwrap_or_default(),
        owner: args
            .owner
            .clone()
            .or_else(|| prop_str(&props, &["owner", "QLRMC", "parcel_name"]))
            .unwrap_or_default(),
        map_sheet: args
            .map_sheet
            .clone()
            .or_else(|| prop_str(&props, &["map_sheet", "TFH"]))
            .unwrap_or_default(),
        area_sqm: args.area.or_else(|| prop_f64(&props, &["area", "ZDMJ"])),
        land_use: args
            .land_use
            .clone()
            .or_else(|| prop_str(&props, &["parcel_use", "YT"]))
            .unwrap_or_default(),
        unit_name: args.unit_name.clone(),
        survey_note: args.survey_note.clone(),
        drawer: args.drawer.clone(),
        reviewer: args.reviewer.clone(),
        draw_date: args.draw_date.clone(),
        review_date: args.review_date.clone(),
        // 四至注记：旗标优先，缺省取属性 ZDSZD/S/X/B（不动产登记数据库标准四至字段）
        sizhi_e: if args.sizhi_e.is_empty() {
            prop_str(&props, &["ZDSZD", "zdszd"]).unwrap_or_default()
        } else {
            args.sizhi_e.clone()
        },
        sizhi_s: if args.sizhi_s.is_empty() {
            prop_str(&props, &["ZDSZN", "zdszn"]).unwrap_or_default()
        } else {
            args.sizhi_s.clone()
        },
        sizhi_w: if args.sizhi_w.is_empty() {
            prop_str(&props, &["ZDSZX", "zdszx"]).unwrap_or_default()
        } else {
            args.sizhi_w.clone()
        },
        sizhi_n: if args.sizhi_n.is_empty() {
            prop_str(&props, &["ZDSZB", "zdszb"]).unwrap_or_default()
        } else {
            args.sizhi_n.clone()
        },
        roads: match &args.roads {
            Some(path) => {
                let road_layer = Layer::load(stem_of(path), path)?;
                kanyu_render::parcelmap::roads_from_collection(
                    &road_layer.collection(),
                    &["name", "NAME", "road_name", "道路名称", "DLMC", "dlmc"],
                )
            }
            None => Vec::new(),
        },
        cadastral_district: if args.cadastral_district.is_empty() {
            prop_str(&props, &["DJQDM", "djqdm"]).unwrap_or_default()
        } else {
            args.cadastral_district.clone()
        },
        collective_owner: args.collective_owner.clone(),
        ownership_survey: args.ownership_survey.clone(),
        realty_mapping: args.realty_mapping.clone(),
        measurer: args.measurer.clone(),
        measure_date: args.measure_date.clone(),
        checker: args.checker.clone(),
        check_date: args.check_date.clone(),
        scale: args.scale,
        dpi: args.dpi,
        ..Default::default()
    };
    let ext = args
        .out
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let output = match ext.as_str() {
        "png" => render_parcel_map_png(&boundary, &spec)?,
        "svg" => render_parcel_map_svg(&boundary, &spec)?,
        other => {
            bail!("输出格式按扩展名判定，仅支持 .png/.svg（实际: '.{other}'）")
        }
    };
    let overlaps = output
        .diagnostics
        .iter()
        .filter(|d| d.contains("overlap=true"))
        .count();
    match &output.data {
        ParcelMapData::Svg(text) => {
            std::fs::write(&args.out, text).with_context(|| format!("写入 {} 失败", args.out))?;
        }
        ParcelMapData::Png(bytes) => {
            std::fs::write(&args.out, bytes).with_context(|| format!("写入 {} 失败", args.out))?;
        }
    }
    let kind_name = match kind {
        kanyu_render::parcelmap::ParcelMapKind::UseRight => "宗地图",
        kanyu_render::parcelmap::ParcelMapKind::Ownership => "所有权宗地图",
        kanyu_render::parcelmap::ParcelMapKind::Sketch => "宗地草图",
    };
    eprintln!(
        "已出{kind_name} → {}（1:{}，注记 {} 条，残余压盖 {} 条）",
        args.out,
        output.scale,
        output.diagnostics.len(),
        overlaps
    );
    Ok(())
}

/// 宗海图件出图共用实现（宗海界址图 L.7 / 宗海位置图 L.6 / 宗海平面布置图 L.8）。
fn sea_map_render(
    args: &crate::cli::SeaMapArgs,
    kind: kanyu_render::seamap::SeaMapKind,
) -> Result<()> {
    use kanyu_render::parcelmap::ParcelMapData;
    use kanyu_render::seamap::{
        render_sea_boundary_map_png, render_sea_boundary_map_svg, SeaBoundaryMapSpec,
    };
    let (boundary, props) = parcel_boundary_from_file(&args.file, args.index)?;
    // 纯数字代码（如 4527）规范化为 EPSG:xxxx
    let source_epsg = if args.source_epsg.chars().all(|c| c.is_ascii_digit()) {
        format!("EPSG:{}", args.source_epsg)
    } else {
        args.source_epsg.clone()
    };
    let spec = SeaBoundaryMapSpec {
        kind,
        project_name: args
            .project_name
            .clone()
            .or_else(|| prop_str(&props, &["project_name", "XMMC", "xmmc"]))
            .unwrap_or_default(),
        sea_code: args
            .sea_code
            .clone()
            .or_else(|| prop_str(&props, &["sea_code", "ZHDM", "zhdm"]))
            .unwrap_or_default(),
        source_epsg,
        survey_unit: args.survey_unit.clone(),
        surveyor: args.surveyor.clone(),
        drawer: args.drawer.clone(),
        draw_date: args.draw_date.clone(),
        inspector: args.inspector.clone(),
        reviewer: args.reviewer.clone(),
        scale: args.scale,
        dpi: args.dpi,
    };
    let ext = args
        .out
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let output = match ext.as_str() {
        "png" => render_sea_boundary_map_png(&boundary, &spec)?,
        "svg" => render_sea_boundary_map_svg(&boundary, &spec)?,
        other => {
            bail!("输出格式按扩展名判定，仅支持 .png/.svg（实际: '.{other}'）")
        }
    };
    let overlaps = output
        .diagnostics
        .iter()
        .filter(|d| d.contains("overlap=true"))
        .count();
    match &output.data {
        ParcelMapData::Svg(text) => {
            std::fs::write(&args.out, text).with_context(|| format!("写入 {} 失败", args.out))?;
        }
        ParcelMapData::Png(bytes) => {
            std::fs::write(&args.out, bytes).with_context(|| format!("写入 {} 失败", args.out))?;
        }
    }
    eprintln!(
        "已出{} → {}（1:{}，注记 {} 条，残余压盖 {} 条）",
        kind.title_suffix(),
        args.out,
        output.scale,
        output.diagnostics.len(),
        overlaps
    );
    Ok(())
}

/// 用岛图件出图共用实现（用岛范围图 L.9 / 建筑物和设施布置图 L.10）。
fn island_map_render(
    args: &crate::cli::IslandMapArgs,
    kind: kanyu_render::islandmap::IslandMapKind,
) -> Result<()> {
    use kanyu_render::islandmap::{render_island_map_png, render_island_map_svg, IslandMapSpec};
    use kanyu_render::parcelmap::ParcelMapData;
    let (boundary, props) = parcel_boundary_from_file(&args.file, args.index)?;
    // 纯数字代码（如 4527）规范化为 EPSG:xxxx
    let source_epsg = if args.source_epsg.chars().all(|c| c.is_ascii_digit()) {
        format!("EPSG:{}", args.source_epsg)
    } else {
        args.source_epsg.clone()
    };
    // 设施提取（仅 L.10 加载设施文件；L.9 忽略 --facilities，渲染器亦忽略 facilities）
    let facilities = match (&args.facilities, kind) {
        (Some(path), kanyu_render::islandmap::IslandMapKind::FacilityMap) => {
            let layer = Layer::load(stem_of(path), path)?;
            facilities_from_collection(&layer.collection())
        }
        _ => Vec::new(),
    };
    let spec = IslandMapSpec {
        kind,
        island_code: args
            .island_code
            .clone()
            .or_else(|| prop_str(&props, &["island_code", "YDDM", "sea_code", "ZHDM"]))
            .unwrap_or_default(),
        source_epsg,
        survey_unit: args.survey_unit.clone(),
        surveyor: args.surveyor.clone(),
        drawer: args.drawer.clone(),
        reviewer: args.reviewer.clone(),
        draw_date: args.draw_date.clone(),
        area_sqm: args.area.or_else(|| prop_f64(&props, &["area", "ZDMJ"])),
        facilities,
        scale: args.scale,
        dpi: args.dpi,
    };
    let ext = args
        .out
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let output = match ext.as_str() {
        "png" => render_island_map_png(&boundary, &spec)?,
        "svg" => render_island_map_svg(&boundary, &spec)?,
        other => {
            bail!("输出格式按扩展名判定，仅支持 .png/.svg（实际: '.{other}'）")
        }
    };
    match &output.data {
        ParcelMapData::Svg(text) => {
            std::fs::write(&args.out, text).with_context(|| format!("写入 {} 失败", args.out))?;
        }
        ParcelMapData::Png(bytes) => {
            std::fs::write(&args.out, bytes).with_context(|| format!("写入 {} 失败", args.out))?;
        }
    }
    eprintln!(
        "已出{} → {}（1:{}{}）",
        kind.title(),
        args.out,
        output.scale,
        match kind {
            kanyu_render::islandmap::IslandMapKind::FacilityMap =>
                format!("，设施 {} 项", spec.facilities.len()),
            kanyu_render::islandmap::IslandMapKind::RangeMap => String::new(),
        }
    );
    Ok(())
}

/// 设施面要素提取（薄壳委托 [`kanyu_render::islandmap::facilities_from_collection`]，
/// CLI/MCP 同一事实来源）。
fn facilities_from_collection(
    collection: &geojson::FeatureCollection,
) -> Vec<kanyu_render::islandmap::IslandFacility> {
    kanyu_render::islandmap::facilities_from_collection(collection)
}

/// 宗地面要素选取与权属边界提取（parcel-map/parcel-dxf 共用）：
/// 薄壳委托 [`kanyu_core::cartography::boundary_from_collection`]
/// （多面要素缺省取面积最大者；`--index` 按文档序第 N 个，0 起）。
#[allow(clippy::type_complexity)]
fn parcel_boundary_from_file(
    file: &str,
    index: Option<usize>,
) -> Result<(
    kanyu_core::cartography::ParcelBoundary,
    serde_json::Map<String, serde_json::Value>,
)> {
    let layer = Layer::load(stem_of(file), file)?;
    Ok(kanyu_core::cartography::boundary_from_collection(
        &layer.collection(),
        index,
    )?)
}

/// 要素属性字符串拾取（薄壳委托 [`kanyu_core::cartography::feature_prop_str`]）。
fn prop_str(props: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    kanyu_core::cartography::feature_prop_str(props, keys)
}

/// 要素属性数值拾取（薄壳委托 [`kanyu_core::cartography::feature_prop_f64`]）。
fn prop_f64(props: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    kanyu_core::cartography::feature_prop_f64(props, keys)
}

/// 布局图例行：样式规则分类直通（无样式为空——排版器空图例不绘）。
fn layout_legend_rows(
    style: Option<&kanyu_render::StyleRule>,
) -> Vec<kanyu_render::layout::LegendRow> {
    fn rgb(s: &str) -> [u8; 3] {
        let h = s.trim().trim_start_matches('#');
        let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
        [p(0), p(2), p(4)]
    }
    match style {
        Some(kanyu_render::StyleRule::Graduated { stops, .. }) => stops
            .iter()
            .map(|(th, c)| kanyu_render::layout::LegendRow {
                color: rgb(c),
                label: format!("≤ {th}"),
            })
            .collect(),
        Some(kanyu_render::StyleRule::Categorical { colors, .. }) => {
            let mut rows: Vec<_> = colors
                .iter()
                .map(|(k, c)| kanyu_render::layout::LegendRow {
                    color: rgb(c),
                    label: k.clone(),
                })
                .collect();
            rows.sort_by(|a, b| a.label.cmp(&b.label));
            rows
        }
        None => Vec::new(),
    }
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
            geo,
            code_repo,
        } => {
            // `--geo` 与 `--code-repo` 互斥；两者都缺时缺省按地理项目（true）。
            if *geo && *code_repo {
                bail!("--geo 与 --code-repo 不可同时指定");
            }
            let is_geo = if *code_repo { false } else { *geo };
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
            std::fs::write(&path, agents::template(&name, crs, is_geo))?;
            eprintln!(
                "已生成 {}（模板：{}）",
                path.display(),
                if is_geo { "geo" } else { "code-repo" }
            );
        }
        AgentsCommand::Validate {
            path,
            check_code_repo,
            geo,
        } => {
            // 与 `agents init` 一致：`--geo` 与 `--check-code-repo` 互斥，两者都
            // 缺时零参自动裁决（`resolve_data_layer` 按 data-layer 元数据行 →
            // crs 占位回退），地理与代码两类仓库均可一次通过。
            if *geo && *check_code_repo {
                bail!("--geo 与 --check-code-repo 不可同时指定");
            }
            let doc = agents::load(path)?;
            let pin = if *check_code_repo {
                Some(false)
            } else if *geo {
                Some(true)
            } else {
                None
            };
            let issues = doc.validate(pin);
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

/// `kanyu toolbox ...`（ArcGIS .pyt 式 Python 工具箱）。
pub fn toolbox(cmd: &ToolboxCommand, json: bool) -> Result<()> {
    let python_home = resolve_python_home()?;
    let (file, argv) = match cmd {
        ToolboxCommand::List { file } => (file.clone(), vec!["list".to_string(), file.clone()]),
        ToolboxCommand::Run { file, tool, params } => {
            // --param k=v → params-json。
            let mut map = serde_json::Map::new();
            for kv in params {
                let Some((k, v)) = kv.split_once('=') else {
                    bail!("参数须为 k=v 形式: {kv}");
                };
                // 数值/布尔自动类型化（与 CSV 口径一致）。
                let value = v
                    .parse::<f64>()
                    .map(serde_json::Value::from)
                    .or_else(|_| v.parse::<bool>().map(serde_json::Value::from))
                    .unwrap_or_else(|_| serde_json::Value::String(v.to_string()));
                map.insert(k.to_string(), value);
            }
            let params_json = serde_json::to_string(&serde_json::Value::Object(map))?;
            (
                file.clone(),
                vec![
                    "run".to_string(),
                    file.clone(),
                    tool.clone(),
                    "--params-json".to_string(),
                    params_json,
                ],
            )
        }
    };
    let mut args = vec!["-m".to_string(), "kanyu.toolbox".to_string()];
    args.extend(argv);
    let output = std::process::Command::new("python")
        .args(&args)
        .env("PYTHONPATH", &python_home)
        .output()
        .with_context(|| "无法启动 python（需要 Python 3.9+ 在 PATH 上）")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        // 运行时统一错误出口为 JSON（{"ok":false,"error":...}），原样透出。
        if !stdout.trim().is_empty() {
            eprintln!("{stdout}");
        }
        if !stderr.trim().is_empty() {
            eprintln!("{stderr}");
        }
        bail!("工具箱执行失败（退出码 {:?}）", output.status.code());
    }
    if json {
        // JSON 原样透传（已是运行时 JSON）。
        println!("{stdout}");
    } else {
        // 人读：list 美化；run 透传。
        match cmd {
            ToolboxCommand::List { .. } => {
                let value: serde_json::Value = serde_json::from_str(stdout.trim())
                    .with_context(|| format!("工具箱返回非 JSON: {stdout}"))?;
                let tools = value["tools"].as_array().cloned().unwrap_or_default();
                if tools.is_empty() {
                    println!("工具箱无工具（检查 Toolbox/Tool 子类约定，见 docs/SDK.md）");
                }
                for t in tools {
                    println!(
                        "  {} — {}（{}）",
                        t["name"].as_str().unwrap_or("?"),
                        t["label"].as_str().unwrap_or(""),
                        t["toolbox"].as_str().unwrap_or("")
                    );
                    if let Some(params) = t["params"].as_array() {
                        for p in params {
                            println!(
                                "      --param {}=<{}>{}",
                                p["name"].as_str().unwrap_or("?"),
                                p["label"].as_str().unwrap_or(""),
                                if p["optional"].as_bool().unwrap_or(false) {
                                    "（可选）"
                                } else {
                                    ""
                                }
                            );
                        }
                    }
                }
            }
            ToolboxCommand::Run { .. } => print!("{stdout}"),
        }
    }
    let _ = file;
    Ok(())
}

/// 解析 Python 包根目录（含 kanyu 包的目录）：
/// 环境变量 KANYU_PYTHON > exe 同级 python/ > exe 上级 ../python > 当前目录 python/。
fn resolve_python_home() -> Result<String> {
    if let Ok(env_path) = std::env::var("KANYU_PYTHON") {
        return Ok(env_path);
    }
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("python"));
            if let Some(up) = dir.parent() {
                candidates.push(up.join("python"));
            }
        }
    }
    candidates.push(std::path::PathBuf::from("python"));
    for candidate in &candidates {
        if candidate.join("kanyu").join("toolbox.py").exists() {
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    bail!(
        "未找到 kanyu Python 包（kanyu/toolbox.py）。请设置 KANYU_PYTHON 环境变量指向仓库 python/ 目录"
    )
}

/// 取文件名主干作为默认图层名。
fn stem_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("layer")
        .to_string()
}

#[cfg(test)]
mod tests {
    /// CLI bench 冒烟（小档，落临时目录）：五项全部执行、场景文件落盘。
    #[test]
    fn bench_small_tier_smoke() {
        let dir = std::env::temp_dir().join("kanyu_cli_bench_smoke");
        super::analysis_bench(100, &dir, true).unwrap();
        assert!(dir.join("mixed_100.geojson").exists());
        assert!(dir.join("overlay_a_10.geojson").exists());
        assert!(dir.join("overlay_b_10.geojson").exists());
    }
}
