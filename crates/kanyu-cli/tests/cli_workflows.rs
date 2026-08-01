//! 端到端集成测试：CLI 驱动内核完成真实工作流。

use std::process::Command;

fn kanyu() -> Command {
    Command::new(env!("CARGO_BIN_EXE_kanyu"))
}

/// 写入临时 GeoJSON 并返回路径。
fn write_sample(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("sample.geojson");
    std::fs::write(
        &path,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},
             "properties":{"name":"a","height":80.0}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
             "properties":{"name":"b","height":20.0}}
        ]}"#,
    )
    .unwrap();
    path
}

#[test]
fn data_info_reports_summary_as_json() {
    let dir = std::env::temp_dir().join("kanyu_itest_info");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);

    let out = kanyu()
        .args(["data", "info", sample.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["feature_count"], 2);
    assert_eq!(json["format"], "geojson");
}

#[test]
fn query_filters_and_export_writes_file() {
    let dir = std::env::temp_dir().join("kanyu_itest_query");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);
    let out_path = dir.join("high.geojson");

    let out = kanyu()
        .args([
            "data",
            "query",
            sample.to_str().unwrap(),
            "--filter",
            "height > 50",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    assert_eq!(written["features"].as_array().unwrap().len(), 1);
}

#[test]
fn introspect_lists_modules_and_tools() {
    let out = kanyu().args(["introspect", "--json"]).output().unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["modules"].as_array().unwrap().len() >= 3);
    let tool_names: Vec<&str> = json["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(tool_names.contains(&"kanyu_data_load"));
}

#[test]
fn agents_init_then_validate_passes() {
    let dir = std::env::temp_dir().join("kanyu_itest_agents");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let init = kanyu()
        .args([
            "agents",
            "init",
            "--project",
            dir.to_str().unwrap(),
            "--name",
            "集成测试项目",
            "--crs",
            "EPSG:4326",
        ])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let validate = kanyu()
        .args([
            "agents",
            "validate",
            "--path",
            dir.join("AGENTS.md").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        validate.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
}

#[test]
fn export_to_unsupported_driver_fails_with_structured_message() {
    let dir = std::env::temp_dir().join("kanyu_itest_export");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);

    let out = kanyu()
        .args([
            "data",
            "export",
            sample.to_str().unwrap(),
            "-f",
            "dwg",
            "--out",
            dir.join("x.dwg").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("libredwg"),
        "stderr 应指出驱动状态: {stderr}"
    );
}

#[test]
fn analysis_buffer_produces_polygon_with_props() {
    let dir = std::env::temp_dir().join("kanyu_itest_buffer");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);

    let out = kanyu()
        .args([
            "analysis",
            "buffer",
            sample.to_str().unwrap(),
            "--distance",
            "0.5",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let features = json["features"].as_array().unwrap();
    assert_eq!(features.len(), 2);
    // 缓冲结果为面几何（geo 返回 MultiPolygon 包装），属性随行。
    let geom_type = features[0]["geometry"]["type"].as_str().unwrap();
    assert!(
        geom_type == "Polygon" || geom_type == "MultiPolygon",
        "缓冲结果应为面几何，实际: {geom_type}"
    );
    assert_eq!(features[0]["properties"]["name"], "a");
}

#[test]
fn analysis_topology_json_reports_violation() {
    let dir = std::env::temp_dir().join("kanyu_itest_topology");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("overlap.geojson");
    std::fs::write(
        &path,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[0,0],[0,4],[4,4],[4,0],[0,0]]]},"properties":{"name":"a"}},
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[2,2],[2,6],[6,6],[6,2],[2,2]]]},"properties":{"name":"b"}}
        ]}"#,
    )
    .unwrap();

    let out = kanyu()
        .args([
            "analysis",
            "topology",
            path.to_str().unwrap(),
            "--rules",
            "no_overlap",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["rule"], "no_overlap");
    assert_eq!(json["feature_count"], 2);
    assert_eq!(json["violation_count"], 1);
    assert_eq!(json["violations"][0]["feature_a"], 0);
    assert_eq!(json["violations"][0]["feature_b"], 1);
}
