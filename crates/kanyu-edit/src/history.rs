//! Undo/Redo 框架：`EditCommand` trait + 双栈历史。
//!
//! v1 策略为**命令逆操作**（每命令自带 revert）；v2 起另有
//! [`crate::delta`] Delta 快照通道（大批量编辑的前后两态切片），两通道在
//! 本模块统一 undo/redo 语义（同一双栈、同一容量淘汰）。事务支持：
//! [`History::begin_transaction`] / [`History::commit_transaction`] 把多命令
//! 合并为一条 undo 记录（原子提交，中途失败逆序回滚）。

use geojson::FeatureCollection;
use kanyu_core::KanyuError;

/// 编辑命令：对要素集合的一次可逆修改。
pub trait EditCommand {
    /// 应用（push 时与 redo 时调用）。
    fn apply(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError>;
    /// 逆回（undo 时调用）。
    fn revert(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError>;
    /// 中文摘要（历史面板/状态栏提示用）。
    fn describe(&self) -> String;
}

/// 编辑历史（undo/redo 双栈 + 容量上限，溢出淘汰最旧）。
pub struct History {
    undo: Vec<Box<dyn EditCommand>>,
    redo: Vec<Box<dyn EditCommand>>,
    /// 容量上限（undo 栈最大深度）。
    capacity: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(100)
    }
}

impl History {
    /// 指定容量构造。
    pub fn new(capacity: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// 压入命令（立即 apply；清空 redo 栈；溢出淘汰最旧）。
    pub fn push(
        &mut self,
        cmd: Box<dyn EditCommand>,
        collection: &mut FeatureCollection,
    ) -> Result<(), KanyuError> {
        cmd.apply(collection)?;
        self.push_applied(cmd);
        Ok(())
    }

    /// 压入**已应用**的命令（清空 redo；溢出淘汰最旧）。
    fn push_applied(&mut self, cmd: Box<dyn EditCommand>) {
        self.undo.push(cmd);
        if self.undo.len() > self.capacity {
            self.undo.remove(0); // 淘汰最旧
        }
        self.redo.clear();
    }

    /// Delta 快照通道：大批量编辑（增/删/改三态切片）整体压入为一条记录
    /// （立即 apply；与命令通道共用 undo/redo 语义与容量淘汰）。
    pub fn push_delta(
        &mut self,
        delta: crate::delta::DeltaSet,
        collection: &mut FeatureCollection,
    ) -> Result<(), KanyuError> {
        self.push(Box::new(delta), collection)
    }

    /// 开启事务（多命令合并为一条 undo 记录，标签为事务摘要）。
    pub fn begin_transaction(&self, label: impl Into<String>) -> Transaction {
        Transaction::new(label)
    }

    /// 提交事务：**原子语义**——逐命令应用，第 k 条失败则逆序回滚前 k-1 条
    /// 且历史不产生记录；全部成功则合并为一条 undo 记录（容量淘汰同样生效）。
    /// 空事务不产生记录。
    pub fn commit_transaction(
        &mut self,
        tx: Transaction,
        collection: &mut FeatureCollection,
    ) -> Result<(), KanyuError> {
        if tx.cmds.is_empty() {
            return Ok(());
        }
        for (i, cmd) in tx.cmds.iter().enumerate() {
            if let Err(e) = cmd.apply(collection) {
                // 逆序回滚已应用命令（逆操作自身不应失败；失败亦继续尽力回滚）。
                for prev in tx.cmds[..i].iter().rev() {
                    let _ = prev.revert(collection);
                }
                return Err(e);
            }
        }
        self.push_applied(Box::new(tx));
        Ok(())
    }

    /// 撤销一步；无可撤销返回中文错误。
    pub fn undo(&mut self, collection: &mut FeatureCollection) -> Result<String, KanyuError> {
        let Some(cmd) = self.undo.pop() else {
            return Err(KanyuError::Other("没有可撤销的编辑".to_string()));
        };
        cmd.revert(collection)?;
        let desc = cmd.describe();
        self.redo.push(cmd);
        Ok(desc)
    }

    /// 重做一步；无可重做返回中文错误。
    pub fn redo(&mut self, collection: &mut FeatureCollection) -> Result<String, KanyuError> {
        let Some(cmd) = self.redo.pop() else {
            return Err(KanyuError::Other("没有可重做的编辑".to_string()));
        };
        cmd.apply(collection)?;
        let desc = cmd.describe();
        self.undo.push(cmd);
        Ok(desc)
    }

    /// 是否可撤销。
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    /// 是否可重做。
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// undo 栈深度。
    pub fn len(&self) -> usize {
        self.undo.len()
    }
    /// 是否为空历史。
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }
    /// 清空（redo 语义同步失效）。
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// 编辑事务（[`History::begin_transaction`] 构造）：多命令合并为一条
/// undo 记录；apply 按入栈序、revert 逆序（整体语义 = 单条记录）。
pub struct Transaction {
    /// 事务摘要（历史面板提示用）。
    label: String,
    /// 命令清单（入栈序）。
    cmds: Vec<Box<dyn EditCommand>>,
}

impl Transaction {
    /// 以摘要构造空事务。
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            cmds: Vec::new(),
        }
    }

    /// 入栈一条命令。
    pub fn push(&mut self, cmd: Box<dyn EditCommand>) -> &mut Self {
        self.cmds.push(cmd);
        self
    }

    /// 命令数。
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    /// 是否为空事务。
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

impl EditCommand for Transaction {
    fn apply(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        // redo 路径：按入栈序重放。
        for cmd in &self.cmds {
            cmd.apply(collection)?;
        }
        Ok(())
    }

    fn revert(&self, collection: &mut FeatureCollection) -> Result<(), KanyuError> {
        // undo 路径：逆序回滚。
        for cmd in self.cmds.iter().rev() {
            cmd.revert(collection)?;
        }
        Ok(())
    }

    fn describe(&self) -> String {
        format!("{}（{} 个命令）", self.label, self.cmds.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geojson::Feature;

    /// 测试命令：追加一个空要素（逆操作 = 弹出）。
    struct PushFeature;
    impl EditCommand for PushFeature {
        fn apply(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
            c.features.push(Feature {
                bbox: None,
                geometry: None,
                id: None,
                properties: None,
                foreign_members: None,
            });
            Ok(())
        }
        fn revert(&self, c: &mut FeatureCollection) -> Result<(), KanyuError> {
            c.features.pop();
            Ok(())
        }
        fn describe(&self) -> String {
            "追加要素".to_string()
        }
    }

    fn coll() -> FeatureCollection {
        FeatureCollection {
            bbox: None,
            features: Vec::new(),
            foreign_members: None,
        }
    }

    #[test]
    fn push_undo_redo_cycle() {
        let mut h = History::default();
        let mut c = coll();
        h.push(Box::new(PushFeature), &mut c).unwrap();
        h.push(Box::new(PushFeature), &mut c).unwrap();
        assert_eq!(c.features.len(), 2);
        assert!(h.can_undo() && !h.can_redo());
        let d = h.undo(&mut c).unwrap();
        assert_eq!(d, "追加要素");
        assert_eq!(c.features.len(), 1);
        assert!(h.can_redo());
        h.redo(&mut c).unwrap();
        assert_eq!(c.features.len(), 2);
        // push 清空 redo。
        h.undo(&mut c).unwrap();
        h.push(Box::new(PushFeature), &mut c).unwrap();
        assert!(!h.can_redo());
        // 空栈错误。
        let mut h2 = History::default();
        assert!(h2.undo(&mut c).unwrap_err().to_string().contains("可撤销"));
        assert!(h2.redo(&mut c).unwrap_err().to_string().contains("可重做"));
    }

    #[test]
    fn capacity_evicts_oldest() {
        let mut h = History::new(3);
        let mut c = coll();
        for _ in 0..5 {
            h.push(Box::new(PushFeature), &mut c).unwrap();
        }
        assert_eq!(h.len(), 3, "溢出淘汰最旧");
        let mut c2 = coll();
        while h.can_undo() {
            h.undo(&mut c2).unwrap();
        }
        // 只能逆回 3 步（容量 3）。
        assert_eq!(h.len(), 0);
        h.clear();
        assert!(h.is_empty());
    }

    /// 测试命令：恒失败（事务原子性测试用）。
    struct AlwaysFail;
    impl EditCommand for AlwaysFail {
        fn apply(&self, _c: &mut FeatureCollection) -> Result<(), KanyuError> {
            Err(KanyuError::Other("恒失败".to_string()))
        }
        fn revert(&self, _c: &mut FeatureCollection) -> Result<(), KanyuError> {
            Ok(())
        }
        fn describe(&self) -> String {
            "恒失败".to_string()
        }
    }

    #[test]
    fn transaction_merges_into_single_undo_record() {
        let mut h = History::default();
        let mut c = coll();
        let mut tx = h.begin_transaction("粘贴三要素");
        tx.push(Box::new(PushFeature));
        tx.push(Box::new(PushFeature));
        tx.push(Box::new(PushFeature));
        assert_eq!(tx.len(), 3);
        h.commit_transaction(tx, &mut c).unwrap();
        assert_eq!(c.features.len(), 3);
        assert_eq!(h.len(), 1, "三命令合并为一条 undo 记录");
        let d = h.undo(&mut c).unwrap();
        assert_eq!(d, "粘贴三要素（3 个命令）");
        assert_eq!(c.features.len(), 0, "一次 undo 回滚整个事务");
        h.redo(&mut c).unwrap();
        assert_eq!(c.features.len(), 3, "redo 按入栈序重放");
        // 空事务不产生记录。
        let empty = h.begin_transaction("空");
        h.commit_transaction(empty, &mut c).unwrap();
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn transaction_failure_rolls_back_atomically() {
        let mut h = History::default();
        let mut c = coll();
        // 先有既有状态：一个命令与两个既有要素。
        h.push(Box::new(PushFeature), &mut c).unwrap();
        h.push(Box::new(PushFeature), &mut c).unwrap();
        let before_len = c.features.len();
        let before_history = h.len();
        let mut tx = h.begin_transaction("批量删除");
        tx.push(Box::new(PushFeature));
        tx.push(Box::new(PushFeature));
        tx.push(Box::new(AlwaysFail));
        let e = h.commit_transaction(tx, &mut c).unwrap_err();
        assert!(e.to_string().contains("恒失败"), "{e}");
        assert_eq!(c.features.len(), before_len, "中途失败须回滚已应用命令");
        assert_eq!(h.len(), before_history, "失败事务不产生历史记录");
        assert!(!h.can_redo());
    }

    #[test]
    fn delta_channel_mixes_with_commands_in_undo_order() {
        use crate::delta::{DeltaSet, FeatureDelta};
        let mut h = History::default();
        let mut c = coll();
        // 命令通道追加 1 个；Delta 通道删除 1 个；undo 序应后入先出。
        h.push(Box::new(PushFeature), &mut c).unwrap();
        let mut ds = DeltaSet::new("批量删除");
        ds.push(FeatureDelta::delete(0, c.features[0].clone()));
        h.push_delta(ds, &mut c).unwrap();
        assert_eq!(c.features.len(), 0);
        assert_eq!(h.len(), 2);
        let d = h.undo(&mut c).unwrap();
        assert_eq!(d, "批量删除（1 要素）", "Delta 记录先撤销");
        assert_eq!(c.features.len(), 1);
        h.undo(&mut c).unwrap();
        assert_eq!(c.features.len(), 0);
        // redo 序对称。
        h.redo(&mut c).unwrap();
        h.redo(&mut c).unwrap();
        assert_eq!(c.features.len(), 0);
    }

    #[test]
    fn capacity_eviction_applies_to_delta_records() {
        use crate::delta::{DeltaSet, FeatureDelta};
        let mut h = History::new(2);
        let mut c = coll();
        for _ in 0..3 {
            h.push(Box::new(PushFeature), &mut c).unwrap();
            let mut ds = DeltaSet::new("改");
            let last = c.features.len() - 1;
            ds.push(FeatureDelta::modify(
                last,
                c.features[last].clone(),
                c.features[last].clone(),
            ));
            h.push_delta(ds, &mut c).unwrap();
        }
        assert_eq!(h.len(), 2, "容量 2：命令与 Delta 混合淘汰最旧");
    }
}
