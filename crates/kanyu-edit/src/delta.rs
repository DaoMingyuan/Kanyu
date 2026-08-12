//! Delta 快照（v2）：要素级增/删/改三态统一的事务快照。
//!
//! 与 v1 命令逆操作的关系（rustdoc 即契约）：
//! - 命令通道（[`crate::history::EditCommand`]）逐命令携带逆操作，适合
//!   交互式小编辑（顶点拖拽/单要素修改），零额外内存；
//! - Delta 通道（本模块）把一次大编辑（批量删除/批量更新/粘贴上千要素）
//!   整体存为**前后两态切片**：undo 是 O(1) 次集合级回放，而非 N 次命令
//!   调用逐条回放；也避免命令通道「10k 次 push 超容量被淘汰、历史丢失」
//!   的问题。影响要素数 > [`DELTA_RECOMMEND_THRESHOLD`]（256）的编辑建议
//!   走 Delta 通道（`History::push_delta`）。
//!
//! ## index 语义（单一参照系约定）
//!
//! - 修改（before/after 均有）与删除（after=None）的 `index` 为**编辑前**
//!   集合下标；
//! - 插入（before=None）的 `index` 以**全部删除应用完成后**的集合为参照。
//!
//! 应用序：修改就地替换 → 删除按下标降序 → 插入按下标升序；
//! 回滚序恰为其逆（插入降序移除 → 删除升序插回 → 修改还原）。
//!
//! ## GeoArrow Delta 路线（留接口，当前不实现）
//!
//! `to_arrow_delta()`（RecordBatch 级列块前后切片）待壳层直驱
//! `Layer::batch()` 行序编辑时兑现：当前壳层编辑工作面为 FeatureCollection，
//! 本模块的要素级 Delta 已与之对齐；RecordBatch 级 Delta 需维护行序↔要素
//! 下标映射，属 Phase 3 后续增量。

use geojson::{Feature, FeatureCollection};
use kanyu_core::KanyuError;

use crate::history::EditCommand;

/// 建议走 Delta 通道的要素规模阈值（大编辑判定，rustdoc 即契约）：
/// 影响要素数超过 256 的批量编辑应构造 [`DeltaSet`] 而非逐命令 push。
pub const DELTA_RECOMMEND_THRESHOLD: usize = 256;

fn err(msg: impl Into<String>) -> KanyuError {
    KanyuError::Other(msg.into())
}

/// 要素 Delta（增/删/改三态统一）：
/// - `before=None, after=Some` → 新增；
/// - `before=Some, after=None` → 删除；
/// - 均 `Some` → 修改（before 供回滚，须为调用方捕获的原值）。
#[derive(Debug, Clone)]
pub struct FeatureDelta {
    /// 要素下标（修改/删除为编辑前下标；插入以删除完成后集合为参照——
    /// 见模块头「index 语义」）。
    pub index: usize,
    /// 编辑前要素（新增为 None）。
    pub before: Option<Feature>,
    /// 编辑后要素（删除为 None）。
    pub after: Option<Feature>,
}

impl FeatureDelta {
    /// 新增要素（index 以删除完成后集合为参照）。
    pub fn insert(index: usize, feature: Feature) -> Self {
        Self {
            index,
            before: None,
            after: Some(feature),
        }
    }

    /// 删除要素（feature 为调用方捕获的原值，index 为编辑前下标）。
    pub fn delete(index: usize, feature: Feature) -> Self {
        Self {
            index,
            before: Some(feature),
            after: None,
        }
    }

    /// 修改要素（index 为编辑前下标；before 供回滚）。
    pub fn modify(index: usize, before: Feature, after: Feature) -> Self {
        Self {
            index,
            before: Some(before),
            after: Some(after),
        }
    }
}

/// Delta 集合：一次编辑事务的全部要素切片（三态可混合，序约定见模块头）。
#[derive(Debug, Clone)]
pub struct DeltaSet {
    /// 中文摘要（历史面板/状态栏提示用）。
    label: String,
    /// 要素切片清单。
    deltas: Vec<FeatureDelta>,
}

impl DeltaSet {
    /// 以摘要构造空集合。
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            deltas: Vec::new(),
        }
    }

    /// 追加一条要素切片。
    pub fn push(&mut self, delta: FeatureDelta) -> &mut Self {
        self.deltas.push(delta);
        self
    }

    /// 影响要素数（供 [`DELTA_RECOMMEND_THRESHOLD`] 判定与摘要展示）。
    pub fn len(&self) -> usize {
        self.deltas.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }

    /// 应用（修改就地 → 删除降序 → 插入升序；越界报中文错误）。
    pub fn apply(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        // 修改：就地替换（前态下标）。
        for d in &self.deltas {
            if let (Some(_), Some(after)) = (&d.before, &d.after) {
                let slot = collection
                    .features
                    .get_mut(d.index)
                    .ok_or_else(|| err(format!("Delta 修改下标越界: {}（修改）", d.index)))?;
                *slot = after.clone();
            }
        }
        // 删除：按下标降序（前态下标，互不移位）。
        let mut deletes: Vec<&FeatureDelta> = self
            .deltas
            .iter()
            .filter(|d| d.before.is_some() && d.after.is_none())
            .collect();
        deletes.sort_by_key(|d| std::cmp::Reverse(d.index));
        for d in deletes {
            if d.index >= collection.features.len() {
                return Err(err(format!("Delta 删除下标越界: {}（删除）", d.index)));
            }
            collection.features.remove(d.index);
        }
        // 插入：按下标升序（以删除完成后集合为参照）。
        let mut inserts: Vec<&FeatureDelta> = self
            .deltas
            .iter()
            .filter(|d| d.before.is_none() && d.after.is_some())
            .collect();
        inserts.sort_by_key(|d| d.index);
        for d in inserts {
            let after = d.after.as_ref().expect("插入必有 after");
            if d.index > collection.features.len() {
                return Err(err(format!("Delta 插入下标越界: {}（插入）", d.index)));
            }
            collection.features.insert(d.index, after.clone());
        }
        Ok(())
    }

    /// 回滚（插入降序移除 → 删除升序插回 → 修改还原；apply 的严格逆）。
    pub fn revert(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        // 移除插入：按下标降序。
        let mut inserts: Vec<&FeatureDelta> = self
            .deltas
            .iter()
            .filter(|d| d.before.is_none() && d.after.is_some())
            .collect();
        inserts.sort_by_key(|d| std::cmp::Reverse(d.index));
        for d in inserts {
            if d.index >= collection.features.len() {
                return Err(err(format!("Delta 回滚下标越界: {}（移除插入）", d.index)));
            }
            collection.features.remove(d.index);
        }
        // 插回删除：按下标升序。
        let mut deletes: Vec<&FeatureDelta> = self
            .deltas
            .iter()
            .filter(|d| d.before.is_some() && d.after.is_none())
            .collect();
        deletes.sort_by_key(|d| d.index);
        for d in deletes {
            let before = d.before.as_ref().expect("删除必有 before");
            if d.index > collection.features.len() {
                return Err(err(format!("Delta 回滚下标越界: {}（插回删除）", d.index)));
            }
            collection.features.insert(d.index, before.clone());
        }
        // 还原修改：就地写回 before。
        for d in &self.deltas {
            if let (Some(before), Some(_)) = (&d.before, &d.after) {
                let slot = collection
                    .features
                    .get_mut(d.index)
                    .ok_or_else(|| err(format!("Delta 回滚下标越界: {}（还原修改）", d.index)))?;
                *slot = before.clone();
            }
        }
        Ok(())
    }
}

impl EditCommand for DeltaSet {
    fn apply(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        DeltaSet::apply(self, collection)
    }

    fn revert(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        DeltaSet::revert(self, collection)
    }

    fn describe(&self) -> String {
        format!("{}（{} 要素）", self.label, self.deltas.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, Value as Json};

    /// 带 name 属性的空几何要素（便于身份断言）。
    fn named(name: &str) -> Feature {
        let mut props = Map::new();
        props.insert("name".to_string(), Json::String(name.to_string()));
        Feature {
            bbox: None,
            geometry: None,
            id: None,
            properties: Some(props),
            foreign_members: None,
        }
    }

    fn coll_of(names: &[&str]) -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: names.iter().map(|n| named(n)).collect(),
            foreign_members: None,
        }
    }

    fn names(c: &FeatureCollection) -> Vec<String> {
        c.features
            .iter()
            .map(|f| {
                f.properties.as_ref().unwrap()["name"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn delta_tri_state_apply_and_revert() {
        // 前态 [A,B,C,D]：修改 0、删除 1 与 3、插入 1（X）→ [A2,X,C]。
        let mut c = coll_of(&["A", "B", "C", "D"]);
        let mut ds = DeltaSet::new("混合编辑");
        ds.push(FeatureDelta::modify(0, named("A"), named("A2")))
            .push(FeatureDelta::delete(1, named("B")))
            .push(FeatureDelta::delete(3, named("D")))
            .push(FeatureDelta::insert(1, named("X")));
        assert_eq!(ds.len(), 4);
        ds.apply(&mut c).unwrap();
        assert_eq!(names(&c), vec!["A2", "X", "C"]);
        ds.revert(&mut c).unwrap();
        assert_eq!(names(&c), vec!["A", "B", "C", "D"], "回滚应逐字节还原");
        // 再应用可重放（redo 语义）。
        ds.apply(&mut c).unwrap();
        assert_eq!(names(&c), vec!["A2", "X", "C"]);
    }

    #[test]
    fn delta_pure_delete_batch() {
        // 大批量删除（Delta 通道的主场景）：删 1/3/4，保留 0/2。
        let mut c = coll_of(&["A", "B", "C", "D", "E"]);
        let mut ds = DeltaSet::new("批量删除");
        for (i, n) in [(1, "B"), (3, "D"), (4, "E")] {
            ds.push(FeatureDelta::delete(i, named(n)));
        }
        ds.apply(&mut c).unwrap();
        assert_eq!(names(&c), vec!["A", "C"]);
        ds.revert(&mut c).unwrap();
        assert_eq!(names(&c), vec!["A", "B", "C", "D", "E"]);
    }

    #[test]
    fn delta_out_of_bounds_is_chinese_error() {
        let mut c = coll_of(&["A"]);
        let mut ds = DeltaSet::new("越界");
        ds.push(FeatureDelta::delete(5, named("B")));
        let e = ds.apply(&mut c).unwrap_err();
        assert!(e.to_string().contains("越界"), "{e}");
        assert_eq!(c.features.len(), 1, "失败不得产生半态（删除未发生）");
        let mut ds = DeltaSet::new("越界");
        ds.push(FeatureDelta::insert(9, named("X")));
        assert!(ds.apply(&mut c).unwrap_err().to_string().contains("越界"));
    }

    #[test]
    fn delta_describe_carries_label_and_count() {
        use crate::history::EditCommand;
        let mut ds = DeltaSet::new("批量删除");
        ds.push(FeatureDelta::delete(0, named("A")));
        assert_eq!(EditCommand::describe(&ds), "批量删除（1 要素）");
    }
}
