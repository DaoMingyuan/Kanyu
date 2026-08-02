//! 堪舆样板分析技能：把每个要素的 `height` 属性乘以 2。
//!
//! 构建（生成 ../../attr_scaler.wasm，宿主测试的 fixture）：
//!   cargo build --target wasm32-unknown-unknown --release \
//!     --manifest-path crates/kanyu-skill/testdata/attr_scaler/Cargo.toml
//!   # wit-bindgen 产出核心模块，需组件化（wasmtime Component 只接受组件）：
//!   wasm-tools component new \
//!     crates/kanyu-skill/testdata/attr_scaler/target/wasm32-unknown-unknown/release/attr_scaler.wasm \
//!     -o crates/kanyu-skill/testdata/attr_scaler.wasm

wit_bindgen::generate!({
    world: "skill",
    path: "../../wit",
});

struct AttrScaler;

impl exports::kanyu::skill::analyzer::Guest for AttrScaler {
    fn meta() -> String {
        r#"{"name":"attr_scaler","version":"0.1.0","capabilities":["analyzer"]}"#.to_string()
    }

    fn run(input: String) -> Result<String, String> {
        let mut root: serde_json::Value =
            serde_json::from_str(&input).map_err(|e| format!("输入非合法 JSON: {e}"))?;
        let features = root
            .get_mut("features")
            .and_then(|f| f.as_array_mut())
            .ok_or_else(|| "输入缺少 features 数组".to_string())?;
        for feature in features.iter_mut() {
            if let Some(height) = feature
                .get_mut("properties")
                .and_then(|p| p.as_object_mut())
                .and_then(|p| p.get_mut("height"))
                .and_then(|h| h.as_f64())
            {
                let new_value = height * 2.0;
                if let Some(props) = feature
                    .get_mut("properties")
                    .and_then(|p| p.as_object_mut())
                {
                    props.insert(
                        "height".to_string(),
                        serde_json::Value::from(new_value),
                    );
                }
            }
        }
        serde_json::to_string(&root).map_err(|e| format!("输出序列化失败: {e}"))
    }
}

// export! 由上面的 generate! 就地生成（macros generating macros）。
export!(AttrScaler);
