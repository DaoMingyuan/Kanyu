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
        stderr.contains("acadrust"),
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

#[test]
fn data_reproject_4326_to_3857_end_to_end() {
    let dir = std::env::temp_dir().join("kanyu_itest_reproject");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bj.geojson");
    std::fs::write(
        &path,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[116.3914,39.9072]},
             "properties":{"name":"北京"}}
        ]}"#,
    )
    .unwrap();
    let out_path = dir.join("bj3857.geojson");

    let out = kanyu()
        .args([
            "data",
            "reproject",
            path.to_str().unwrap(),
            "--from",
            "EPSG:4326",
            "--to",
            "EPSG:3857",
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
    let coords = written["features"][0]["geometry"]["coordinates"]
        .as_array()
        .unwrap();
    // 球面 Web 墨卡托（R=6378137）：x ≈ 1.2957e7，y ≈ 4.85e6，量级校验（±1000m）。
    let x = coords[0].as_f64().unwrap();
    let y = coords[1].as_f64().unwrap();
    assert!((x - 12_956_600.0).abs() < 1000.0, "x 量级错误: {x}");
    assert!((y - 4_852_500.0).abs() < 1000.0, "y 量级错误: {y}");
    // 属性不受影响。
    assert_eq!(written["features"][0]["properties"]["name"], "北京");
}

#[test]
fn analysis_zonal_end_to_end_with_stats_columns() {
    let dir = std::env::temp_dir().join("kanyu_itest_zonal");
    std::fs::create_dir_all(&dir).unwrap();
    let zones = dir.join("zones.geojson");
    std::fs::write(
        &zones,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[0,0],[0,4],[4,4],[4,0],[0,0]]]},"properties":{"name":"z1"}}
        ]}"#,
    )
    .unwrap();
    let values = dir.join("values.geojson");
    std::fs::write(
        &values,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
             "properties":{"height":10}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[2,2]},
             "properties":{"height":30}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[100,100]},
             "properties":{"height":99}}
        ]}"#,
    )
    .unwrap();
    let out_path = dir.join("zoned.geojson");

    let out = kanyu()
        .args([
            "analysis",
            "zonal",
            zones.to_str().unwrap(),
            values.to_str().unwrap(),
            "--field",
            "height",
            "--stats",
            "count,sum,mean",
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
    let props = &written["features"][0]["properties"];
    assert_eq!(props["height_count"], 2);
    assert_eq!(props["height_sum"], 40.0);
    assert_eq!(props["height_mean"], 20.0);
    // 区外值计入 unzoned_count。
    assert_eq!(written["unzoned_count"], 1);
}

#[test]
fn mcp_serve_http_accepts_initialize_over_tcp() {
    use std::io::{Read, Write};

    let port = 39177u16; // 随机高端口，避开常见冲突。
    let mut child = kanyu()
        .args([
            "mcp",
            "serve",
            "--transport",
            "http",
            "--port",
            &port.to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // 等待端口就绪（~5s 上限）。
    let mut ready = false;
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ready, "HTTP 服务端口未在 5s 内就绪");

    // 裸 TCP 手写最小 HTTP/1.1 POST initialize（不引入 reqwest，保持依赖精简）。
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"itest","version":"0"}}}"#;
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();

    let _ = child.kill();
    let _ = child.wait();

    // SSE 帧（text/event-stream）或纯 JSON 响应均可接受——
    // rmcp 默认 legacy session 模式下 initialize 走 SSE 通道。
    assert!(
        response.contains("serverInfo") && response.contains("kanyu-mcp"),
        "响应应含 serverInfo/kanyu-mcp：\n{response}"
    );
}

#[test]
fn mcp_stdio_task_lifecycle_end_to_end() {
    use std::io::{BufRead, BufReader, Write};

    let dir = std::env::temp_dir().join("kanyu_itest_task");
    std::fs::create_dir_all(&dir).unwrap();
    let zones = dir.join("zones.geojson");
    std::fs::write(
        &zones,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[0,0],[0,4],[4,4],[4,0],[0,0]]]},"properties":{"name":"z1"}}
        ]}"#,
    )
    .unwrap();
    let values = dir.join("values.geojson");
    std::fs::write(
        &values,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]},
             "properties":{"height":10}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[2,2]},
             "properties":{"height":30}}
        ]}"#,
    )
    .unwrap();

    let mut child = kanyu()
        .args(["mcp", "serve"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    fn rpc(stdin: &mut impl Write, reader: &mut impl BufRead, line: &str) -> serde_json::Value {
        writeln!(stdin, "{line}").unwrap();
        stdin.flush().unwrap();
        let mut buf = String::new();
        reader.read_line(&mut buf).unwrap();
        serde_json::from_str(&buf).unwrap_or_else(|e| panic!("响应非 JSON 行: {e}: {buf}"))
    }

    // initialize（声明 tasks 扩展能力）。
    let init = rpc(
        &mut stdin,
        &mut reader,
        r##"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{"extensions":{"io.modelcontextprotocol/tasks":{}}},"clientInfo":{"name":"itest","version":"0"}}}"##,
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "kanyu-mcp");
    // 服务端声明 tasks 扩展。
    assert!(
        init["result"]["capabilities"]["extensions"]["io.modelcontextprotocol/tasks"].is_object(),
        "服务端应声明 tasks 扩展: {init}"
    );
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .unwrap();
    stdin.flush().unwrap();

    // tools/call 带 task:true → resultType:"task" + taskId。
    let call = rpc(
        &mut stdin,
        &mut reader,
        &format!(
            r##"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"kanyu_analysis_zonal_stats","arguments":{{"zones":"{}","values":"{}","field":"height","stats":["count","mean"],"task":true}}}}}}"##,
            zones.to_str().unwrap().replace('\\', "\\\\"),
            values.to_str().unwrap().replace('\\', "\\\\")
        ),
    );
    assert_eq!(
        call["result"]["resultType"], "task",
        "应返回任务句柄: {call}"
    );
    let task_id = call["result"]["taskId"].as_str().unwrap().to_string();

    // tasks/get 轮询到 completed，结果与同步调用一致。
    let mut done = None;
    for _ in 0..50 {
        let got = rpc(
            &mut stdin,
            &mut reader,
            &format!(
                r#"{{"jsonrpc":"2.0","id":3,"method":"tasks/get","params":{{"taskId":"{task_id}"}}}}"#
            ),
        );
        if got["result"]["status"] == "completed" {
            done = Some(got);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();

    let done = done.expect("任务应在 5s 内完成");
    assert_eq!(
        done["result"]["result"]["structuredContent"]["collection"]["features"][0]["properties"]
            ["height_count"],
        2
    );
    assert_eq!(
        done["result"]["result"]["structuredContent"]["collection"]["features"][0]["properties"]
            ["height_mean"],
        20.0
    );
}

#[test]
fn render_map_writes_valid_png() {
    let dir = std::env::temp_dir().join("kanyu_itest_render");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);
    let out_path = dir.join("map.png");

    let out = kanyu()
        .args([
            "render",
            "map",
            sample.to_str().unwrap(),
            "--out",
            out_path.to_str().unwrap(),
            "--width",
            "400",
            "--height",
            "300",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let bytes = std::fs::read(&out_path).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "PNG 魔数");
    assert!(bytes.len() > 500, "PNG 应包含实际像素数据");

    // SVG 路径。
    let svg_path = dir.join("map.svg");
    let out = kanyu()
        .args([
            "render",
            "map",
            sample.to_str().unwrap(),
            "--out",
            svg_path.to_str().unwrap(),
            "--theme",
            "dark",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let svg = std::fs::read_to_string(&svg_path).unwrap();
    assert!(svg.contains("viewBox=\"0 0 800 600\""));
    assert!(svg.contains("#0D0F12"), "夜观星画布色: {svg}");

    // 非法扩展名报错。
    let bad = kanyu()
        .args(["render", "map", sample.to_str().unwrap(), "--out", "x.bmp"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains(".png/.svg"));
}

#[test]
fn render_map_with_graduated_style_end_to_end() {
    let dir = std::env::temp_dir().join("kanyu_itest_style");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("polys.geojson");
    std::fs::write(
        &path,
        r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[0,0],[0,2],[2,2],[2,0],[0,0]]]},"properties":{"height":10}},
            {"type":"Feature","geometry":{"type":"Polygon","coordinates":[
                [[3,0],[3,2],[5,2],[5,0],[3,0]]]},"properties":{"height":120}}
        ]}"#,
    )
    .unwrap();
    let out_path = dir.join("styled.svg");

    let out = kanyu()
        .args([
            "render",
            "map",
            path.to_str().unwrap(),
            "--out",
            out_path.to_str().unwrap(),
            "--style",
            r##"{"type":"graduated","field":"height","stops":[[0,"#2D6A5E"],[50,"#D4A843"],[100,"#C75B3A"]]}"##,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = std::fs::read_to_string(&out_path).unwrap();
    assert!(svg.contains("#2D6A5E"), "低档色: {svg}");
    assert!(svg.contains("#C75B3A"), "高档色: {svg}");

    // 非法 style 报错。
    let bad = kanyu()
        .args([
            "render",
            "map",
            path.to_str().unwrap(),
            "--out",
            dir.join("x.svg").to_str().unwrap(),
            "--style",
            r##"{"type":"graduated","field":"height","stops":[[50,"#2D6A5E"],[50,"#D4A843"]]}"##,
        ])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("严格升序"),
        "stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    // --style 与 --style-file 互斥。
    let conflict = kanyu()
        .args([
            "render",
            "map",
            path.to_str().unwrap(),
            "--out",
            dir.join("y.svg").to_str().unwrap(),
            "--style",
            "{}",
            "--style-file",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stderr).contains("二选一"));
}

#[test]
fn gene_info_and_run_end_to_end() {
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../kanyu-skill/testdata/attr_scaler.wasm"
    );
    let dir = std::env::temp_dir().join("kanyu_itest_gene");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = write_sample(&dir);

    // gene info --json。
    let out = kanyu()
        .args(["skill", "info", fixture, "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(meta["name"], "attr_scaler");
    assert_eq!(meta["version"], "0.1.0");
    assert_eq!(meta["capabilities"][0], "analyzer");

    // gene run：height 翻倍、要素数不变。
    let out = kanyu()
        .args(["skill", "run", fixture, sample.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let features = result["features"].as_array().unwrap();
    assert_eq!(features.len(), 2);
    assert_eq!(features[0]["properties"]["height"], 160.0);
    assert_eq!(features[1]["properties"]["height"], 40.0);
    assert_eq!(features[0]["properties"]["name"], "a");

    // 垃圾 wasm 中文错误。
    let bad_path = dir.join("bad.wasm");
    std::fs::write(&bad_path, b"not wasm").unwrap();
    let bad = kanyu()
        .args(["skill", "info", bad_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("技能加载失败"));
}

#[test]
fn parcel_txt_validate_and_roundtrip() {
    let dir = std::env::temp_dir().join("kanyu_itest_parcel");
    std::fs::create_dir_all(&dir).unwrap();
    let sample = "[属性描述]\n格式版本号=1.0\n数据产生单位=测试\n数据产生日期=2026-08-03\n坐标系=CGCS2000\n几度分带=3\n投影类型=高斯克吕格\n计量单位=米\n带号=39\n精度=0.0001\n转换参数=\n[地块坐标]\n5,100.0,ZD001,测试地块,面,H50,住宅,,@\nJ1,1,4100000.0,39580000.0\nJ2,1,4100000.0,39580010.0\nJ3,1,4100010.0,39580010.0\nJ4,1,4100010.0,39580000.0\nJ1,1,4100000.0,39580000.0\n";
    let path = dir.join("a.txt");
    std::fs::write(&path, sample).unwrap();

    // 质检通过（含警告不影响退出码）。
    let v = kanyu()
        .args(["data", "validate", path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        v.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&v.stderr)
    );

    // 读取为面图层并导出回 txt（往返后可再解析）。
    let info = kanyu()
        .args(["data", "info", path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(json["feature_count"], 1);
    assert_eq!(json["format"], "txt");
    let out = dir.join("out.txt");
    let e = kanyu()
        .args([
            "data",
            "export",
            path.to_str().unwrap(),
            "-f",
            "txt",
            "--out",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        e.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&e.stderr)
    );
    let v2 = kanyu()
        .args(["data", "validate", out.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        v2.status.success(),
        "往返后质检应通过: {}",
        String::from_utf8_lossy(&v2.stderr)
    );
}

#[test]
fn toolbox_list_and_run_via_python() {
    // Python 不在 PATH 时跳过（CI 无 Python 环境的兜底）。
    if std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("python 不可用，跳过工具箱集成测试");
        return;
    }
    // 测试 cwd 为 crate 目录：显式指定仓库 python/ 包路径。
    let python_home = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../python")
        .canonicalize()
        .unwrap();
    let out = kanyu()
        .args(["toolbox", "list", "examples/planning_tools.py", "--json"])
        .env("KANYU_PYTHON", &python_home)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let tools = json["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "buffer500"));

    let run = kanyu()
        .args([
            "toolbox",
            "run",
            "examples/planning_tools.py",
            "highrise_report",
            "--param",
            "input=examples/buildings.geojson",
            "--param",
            "threshold=50",
        ])
        .env("KANYU_PYTHON", &python_home)
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["result"]["count"], 2);
}
