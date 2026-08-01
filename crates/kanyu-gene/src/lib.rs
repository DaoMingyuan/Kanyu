//! # kanyu-gene —— 堪舆的 WASM 基因系统宿主（总规 §4.5"以 WASM 为基因"）
//!
//! 基因是编译到 WebAssembly 组件模型的插件：经 [`wit/gene.wit`] 定义的
//! 强类型 ABI（`meta() -> string`、`run(string) -> result<string, string>`）
//! 与内核交互。宿主基于 wasmtime：
//!
//! - **沙箱**：组件无 WASI 导入（纯计算，无文件/网络/环境访问）；
//! - **资源配额**：`Config::consume_fuel(true)` + 每次执行前 `set_fuel`
//!   （默认 10 亿），fuel 耗尽即 trap——覆盖纯计算死循环；组件无 IO
//!   导入，不存在挂起等待，故本轮不设墙钟超时（注释即契约）；
//! - **错误**：加载/元数据/trap/配额/结果五类中文结构化错误。

use geojson::FeatureCollection;

mod wit_bindings {
    wasmtime::component::bindgen!({
        path: "wit/gene.wit",
        world: "gene",
    });
}

/// 单次执行的 fuel 配额（10 亿指令量级；过小会误伤正常分析，过大失去配额意义）。
const FUEL_LIMIT: u64 = 1_000_000_000;

/// 基因系统错误。
#[derive(Debug, thiserror::Error)]
pub enum GeneError {
    /// wasm 编译/实例化失败（含接口不匹配）。
    #[error("基因加载失败（{path}）：{reason}")]
    LoadFailed {
        /// 基因文件路径。
        path: String,
        /// 失败原因。
        reason: String,
    },
    /// meta() 调用失败或元数据 JSON 非法。
    #[error("基因元数据非法：{0}")]
    MetaInvalid(String),
    /// 执行期 trap（含基因返回的业务错误）。
    #[error("基因执行陷阱：{0}")]
    Trap(String),
    /// 超出 fuel 配额（疑似死循环或过重计算）。
    #[error("基因执行超出 fuel 配额：{0}")]
    Timeout(String),
    /// 返回值不是合法 FeatureCollection。
    #[error("基因结果非法：{0}")]
    ResultInvalid(String),
}

/// 基因元数据（guest `meta()` 返回的 JSON 形状）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeneMeta {
    /// 基因名。
    pub name: String,
    /// 版本号。
    pub version: String,
    /// 能力清单（如 ["analyzer"]）。
    pub capabilities: Vec<String>,
}

/// 一个已加载校验的基因。
pub struct Gene {
    component: wasmtime::component::Component,
    meta: GeneMeta,
}

impl Gene {
    /// 元数据。
    pub fn meta(&self) -> &GeneMeta {
        &self.meta
    }
}

/// 基因宿主（wasmtime 引擎，consume_fuel 开启）。
pub struct GeneHost {
    engine: wasmtime::Engine,
}

impl GeneHost {
    /// 构造宿主（consume_fuel 开启；引擎初始化失败极少见，包为中文错误）。
    pub fn new() -> Result<Self, GeneError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config).map_err(|e| GeneError::LoadFailed {
            path: String::new(),
            reason: format!("wasmtime 引擎初始化失败: {e}"),
        })?;
        Ok(Self { engine })
    }

    /// 加载并校验基因：编译 → 实例化 → 调 `meta()` 取元数据并校验。
    /// 无效 wasm、WIT 接口不匹配、元数据非法分别报中文结构化错误。
    pub fn load(&self, path: &str) -> Result<Gene, GeneError> {
        let component =
            wasmtime::component::Component::from_file(&self.engine, path).map_err(|e| {
                GeneError::LoadFailed {
                    path: path.to_string(),
                    reason: e.to_string(),
                }
            })?;
        let linker = wasmtime::component::Linker::new(&self.engine);
        let mut store = wasmtime::Store::new(&self.engine, ());
        store
            .set_fuel(FUEL_LIMIT)
            .map_err(|e| GeneError::Trap(format!("fuel 配置失败: {e}")))?;
        let bindings =
            wit_bindings::Gene::instantiate(&mut store, &component, &linker).map_err(|e| {
                GeneError::LoadFailed {
                    path: path.to_string(),
                    reason: format!("实例化失败（WIT 接口 kanyu:gene/analyzer 不匹配？）: {e}"),
                }
            })?;
        let meta_json = bindings
            .kanyu_gene_analyzer()
            .call_meta(&mut store)
            .map_err(|e| GeneError::MetaInvalid(format!("meta() 调用失败: {e}")))?;
        let meta: GeneMeta = serde_json::from_str(&meta_json)
            .map_err(|e| GeneError::MetaInvalid(format!("meta() 返回非合法 JSON: {e}")))?;
        if meta.name.is_empty() {
            return Err(GeneError::MetaInvalid("meta.name 不能为空".to_string()));
        }
        if meta.version.is_empty() {
            return Err(GeneError::MetaInvalid("meta.version 不能为空".to_string()));
        }
        Ok(Gene { component, meta })
    }

    /// 在沙箱中执行基因：FeatureCollection JSON 进/出；每次执行重置 fuel。
    /// trap（含 fuel 耗尽）/结果非法各为独立中文错误。
    pub fn run(
        &self,
        gene: &Gene,
        input: &FeatureCollection,
    ) -> Result<FeatureCollection, GeneError> {
        let input_json = geojson::GeoJson::from(input.clone()).to_string();
        let linker = wasmtime::component::Linker::new(&self.engine);
        let mut store = wasmtime::Store::new(&self.engine, ());
        store
            .set_fuel(FUEL_LIMIT)
            .map_err(|e| GeneError::Trap(format!("fuel 配置失败: {e}")))?;
        let bindings = wit_bindings::Gene::instantiate(&mut store, &gene.component, &linker)
            .map_err(|e| GeneError::Trap(format!("实例化失败: {e}")))?;
        let result = bindings
            .kanyu_gene_analyzer()
            .call_run(&mut store, &input_json)
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("fuel") {
                    GeneError::Timeout(format!(
                        "疑似死循环或过重计算（fuel 上限 {FUEL_LIMIT}）: {msg}"
                    ))
                } else {
                    GeneError::Trap(msg)
                }
            })?;
        // 基因主动返回的业务错误（result<string,string> 的 Err 臂）。
        let output_json = result
            .map_err(|guest_err| GeneError::Trap(format!("基因返回业务错误: {guest_err}")))?;
        let gj: geojson::GeoJson = output_json
            .parse()
            .map_err(|e| GeneError::ResultInvalid(format!("基因返回非 GeoJSON: {e}")))?;
        FeatureCollection::try_from(gj)
            .map_err(|e| GeneError::ResultInvalid(format!("基因返回非 FeatureCollection: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/attr_scaler.wasm");

    fn collection_from_str(text: &str) -> FeatureCollection {
        let gj: geojson::GeoJson = text.parse().unwrap();
        FeatureCollection::try_from(gj).unwrap()
    }

    #[test]
    fn load_valid_gene_parses_meta() {
        let host = GeneHost::new().unwrap();
        let gene = host.load(FIXTURE).unwrap();
        assert_eq!(gene.meta().name, "attr_scaler");
        assert_eq!(gene.meta().version, "0.1.0");
        assert_eq!(gene.meta().capabilities, vec!["analyzer"]);
    }

    #[test]
    fn run_doubles_height_and_preserves_geometry() {
        let host = GeneHost::new().unwrap();
        let gene = host.load(FIXTURE).unwrap();
        let input = collection_from_str(
            r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.39,39.90]},
                 "properties":{"name":"a","height":80}},
                {"type":"Feature","geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]},
                 "properties":{"name":"b","height":30}},
                {"type":"Feature","geometry":{"type":"Point","coordinates":[116.40,39.91]},
                 "properties":{"name":"c","height":55.5}}
            ]}"#,
        );
        let out = host.run(&gene, &input).unwrap();
        assert_eq!(out.features.len(), 3);
        for (i, expected) in [160.0, 60.0, 111.0].iter().enumerate() {
            let props = out.features[i].properties.as_ref().unwrap();
            assert_eq!(
                props["height"].as_f64(),
                Some(*expected),
                "要素 {i} 的 height 应翻倍"
            );
        }
        // 几何与名称不变。
        assert_eq!(
            input.features[1].geometry.as_ref().unwrap().value,
            out.features[1].geometry.as_ref().unwrap().value
        );
        assert_eq!(
            out.features[0].properties.as_ref().unwrap()["name"],
            serde_json::Value::String("a".to_string())
        );
    }

    #[test]
    fn load_garbage_wasm_gives_chinese_error() {
        let dir = std::env::temp_dir().join("kanyu_gene_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.wasm");
        std::fs::write(&path, b"this is not wasm at all").unwrap();
        let host = GeneHost::new().unwrap();
        let err = match host.load(path.to_str().unwrap()) {
            Ok(_) => panic!("垃圾字节应加载失败"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("基因加载失败"),
            "应为中文加载错误: {err}"
        );
    }

    #[test]
    fn load_interface_mismatch_gives_clear_error() {
        // 合法组件但缺少 kanyu:gene/analyzer 导出（wat 手捏空世界组件）。
        let wat = r#"(component)"#;
        let dir = std::env::temp_dir().join("kanyu_gene_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.wasm");
        std::fs::write(&path, wat::parse_str(wat).unwrap()).unwrap();
        let host = GeneHost::new().unwrap();
        let err = match host.load(path.to_str().unwrap()) {
            Ok(_) => panic!("无 analyzer 导出的组件应加载失败"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("基因加载失败") || msg.contains("接口"),
            "应指出接口不匹配: {msg}"
        );
    }
}
