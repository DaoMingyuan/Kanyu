//! # kanyu-skill —— 堪舆的 WASM 技能系统宿主（总规 §4.5"以 WASM 为技能"）
//!
//! 技能是编译到 WebAssembly 组件模型的插件：经 `wit/skill.wit` 定义的
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
        path: "wit/skill.wit",
        world: "skill",
    });
}

/// 单次执行的 fuel 配额（10 亿指令量级；过小会误伤正常分析，过大失去配额意义）。
const FUEL_LIMIT: u64 = 1_000_000_000;

/// 技能系统错误。
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// wasm 编译/实例化失败（含接口不匹配）。
    #[error("技能加载失败（{path}）：{reason}")]
    LoadFailed {
        /// 技能文件路径。
        path: String,
        /// 失败原因。
        reason: String,
    },
    /// meta() 调用失败或元数据 JSON 非法。
    #[error("技能元数据非法：{0}")]
    MetaInvalid(String),
    /// 执行期 trap（含技能返回的业务错误）。
    #[error("技能执行陷阱：{0}")]
    Trap(String),
    /// 超出 fuel 配额（疑似死循环或过重计算）。
    #[error("技能执行超出 fuel 配额：{0}")]
    Timeout(String),
    /// 返回值不是合法 FeatureCollection。
    #[error("技能结果非法：{0}")]
    ResultInvalid(String),
}

/// 技能元数据（guest `meta()` 返回的 JSON 形状）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillMeta {
    /// 技能名。
    pub name: String,
    /// 版本号。
    pub version: String,
    /// 能力清单（如 ["analyzer"]）。
    pub capabilities: Vec<String>,
}

/// 一个已加载校验的技能。
pub struct Skill {
    component: wasmtime::component::Component,
    meta: SkillMeta,
}

impl Skill {
    /// 元数据。
    pub fn meta(&self) -> &SkillMeta {
        &self.meta
    }
}

/// 技能宿主（wasmtime 引擎，consume_fuel 开启）。
pub struct SkillHost {
    engine: wasmtime::Engine,
}

impl SkillHost {
    /// 构造宿主（consume_fuel 开启；引擎初始化失败极少见，包为中文错误）。
    pub fn new() -> Result<Self, SkillError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config).map_err(|e| SkillError::LoadFailed {
            path: String::new(),
            reason: format!("wasmtime 引擎初始化失败: {e}"),
        })?;
        Ok(Self { engine })
    }

    /// 加载并校验技能：编译 → 实例化 → 调 `meta()` 取元数据并校验。
    /// 无效 wasm、WIT 接口不匹配、元数据非法分别报中文结构化错误。
    pub fn load(&self, path: &str) -> Result<Skill, SkillError> {
        let component =
            wasmtime::component::Component::from_file(&self.engine, path).map_err(|e| {
                SkillError::LoadFailed {
                    path: path.to_string(),
                    reason: e.to_string(),
                }
            })?;
        let linker = wasmtime::component::Linker::new(&self.engine);
        let mut store = wasmtime::Store::new(&self.engine, ());
        store
            .set_fuel(FUEL_LIMIT)
            .map_err(|e| SkillError::Trap(format!("fuel 配置失败: {e}")))?;
        let bindings =
            wit_bindings::Skill::instantiate(&mut store, &component, &linker).map_err(|e| {
                SkillError::LoadFailed {
                    path: path.to_string(),
                    reason: format!("实例化失败（WIT 接口 kanyu:skill/analyzer 不匹配？）: {e}"),
                }
            })?;
        let meta_json = bindings
            .kanyu_skill_analyzer()
            .call_meta(&mut store)
            .map_err(|e| SkillError::MetaInvalid(format!("meta() 调用失败: {e}")))?;
        let meta: SkillMeta = serde_json::from_str(&meta_json)
            .map_err(|e| SkillError::MetaInvalid(format!("meta() 返回非合法 JSON: {e}")))?;
        if meta.name.is_empty() {
            return Err(SkillError::MetaInvalid("meta.name 不能为空".to_string()));
        }
        if meta.version.is_empty() {
            return Err(SkillError::MetaInvalid("meta.version 不能为空".to_string()));
        }
        Ok(Skill { component, meta })
    }

    /// 在沙箱中执行技能：FeatureCollection JSON 进/出；每次执行重置 fuel。
    /// trap（含 fuel 耗尽）/结果非法各为独立中文错误。
    pub fn run(
        &self,
        skill: &Skill,
        input: &FeatureCollection,
    ) -> Result<FeatureCollection, SkillError> {
        let input_json = geojson::GeoJson::from(input.clone()).to_string();
        let linker = wasmtime::component::Linker::new(&self.engine);
        let mut store = wasmtime::Store::new(&self.engine, ());
        store
            .set_fuel(FUEL_LIMIT)
            .map_err(|e| SkillError::Trap(format!("fuel 配置失败: {e}")))?;
        let bindings = wit_bindings::Skill::instantiate(&mut store, &skill.component, &linker)
            .map_err(|e| SkillError::Trap(format!("实例化失败: {e}")))?;
        let result = bindings
            .kanyu_skill_analyzer()
            .call_run(&mut store, &input_json)
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("fuel") {
                    SkillError::Timeout(format!(
                        "疑似死循环或过重计算（fuel 上限 {FUEL_LIMIT}）: {msg}"
                    ))
                } else {
                    SkillError::Trap(msg)
                }
            })?;
        // 技能主动返回的业务错误（result<string,string> 的 Err 臂）。
        let output_json = result
            .map_err(|guest_err| SkillError::Trap(format!("技能返回业务错误: {guest_err}")))?;
        let gj: geojson::GeoJson = output_json
            .parse()
            .map_err(|e| SkillError::ResultInvalid(format!("技能返回非 GeoJSON: {e}")))?;
        FeatureCollection::try_from(gj)
            .map_err(|e| SkillError::ResultInvalid(format!("技能返回非 FeatureCollection: {e}")))
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
        let host = SkillHost::new().unwrap();
        let skill = host.load(FIXTURE).unwrap();
        assert_eq!(skill.meta().name, "attr_scaler");
        assert_eq!(skill.meta().version, "0.1.0");
        assert_eq!(skill.meta().capabilities, vec!["analyzer"]);
    }

    #[test]
    fn run_doubles_height_and_preserves_geometry() {
        let host = SkillHost::new().unwrap();
        let skill = host.load(FIXTURE).unwrap();
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
        let out = host.run(&skill, &input).unwrap();
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
        let dir = std::env::temp_dir().join("kanyu_skill_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("garbage.wasm");
        std::fs::write(&path, b"this is not wasm at all").unwrap();
        let host = SkillHost::new().unwrap();
        let err = match host.load(path.to_str().unwrap()) {
            Ok(_) => panic!("垃圾字节应加载失败"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("技能加载失败"),
            "应为中文加载错误: {err}"
        );
    }

    #[test]
    fn load_interface_mismatch_gives_clear_error() {
        // 合法组件但缺少 kanyu:skill/analyzer 导出（wat 手捏空世界组件）。
        let wat = r#"(component)"#;
        let dir = std::env::temp_dir().join("kanyu_skill_mismatch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.wasm");
        std::fs::write(&path, wat::parse_str(wat).unwrap()).unwrap();
        let host = SkillHost::new().unwrap();
        let err = match host.load(path.to_str().unwrap()) {
            Ok(_) => panic!("无 analyzer 导出的组件应加载失败"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("技能加载失败") || msg.contains("接口"),
            "应指出接口不匹配: {msg}"
        );
    }
}
