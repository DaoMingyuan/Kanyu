//! Undo/Redo 框架：`EditCommand` trait + 双栈历史。
//!
//! v1 策略为**命令逆操作**（每命令自带 revert），而非 GeoArrow Delta 快照——
//! 逆操作在编辑命令上零额外内存；Delta 快照（全量/增量快照回放）是后续优化项
//! （见 crate 根 rustdoc 路线图 v2）。

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
        self.undo.push(cmd);
        if self.undo.len() > self.capacity {
            self.undo.remove(0); // 淘汰最旧
        }
        self.redo.clear();
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
}
