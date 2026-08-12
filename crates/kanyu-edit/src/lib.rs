//! kanyu-edit —— 堪舆编辑内核（MASTERPLAN Phase 3「手」）。
//!
//! ## 定位与增量路线
//!
//! Phase 3 终态是 DCEL 增量拓扑编辑内核；已交付：
//! - [`history`]：Undo/Redo 框架（`EditCommand` trait + 双栈 + 容量淘汰）
//!   v2 增强：Delta 通道（`History::push_delta`）与事务
//!   （`begin_transaction`/`commit_transaction`，原子提交）；
//! - [`ops`]：基础编辑命令（顶点移动 / 要素平移 / 删除 / 插入 / 属性更新），
//!   全部为纯函数式命令（apply/revert over `FeatureCollection`）；
//! - [`delta`]（v2）：Delta 快照（`FeatureDelta` 增/删/改三态 + `DeltaSet`
//!   事务切片）——大批量编辑比逐命令逆操作更省内存与耗时（O(1) 集合级回放），
//!   与命令通道并存、统一 undo/redo 语义；GeoArrow RecordBatch 级 Delta
//!   为留接口路线（见 delta 模块头）；
//! - [`dcel`]（v3）：DCEL 增量拓扑内核（三表五指针、孔洞虚面约定、欧拉自检、
//!   面对角线分裂；v2 续篇：外面/孔虚面 stub 环遍历（顶点角度序转向）、
//!   merge_faces 单边合并逆操作（墓碑式删除保下标稳定））。
//!
//! 后续增量（路线图，rustdoc 即契约）：
//! - v4：DCEL 与壳层编辑模式接线 + 增量操作扩展（顶点移动联动等）。

pub mod dcel;
pub mod delta;
pub mod history;
pub mod ops;

pub use dcel::{Dcel, FaceKind, MergeResult, SplitResult, OUTER_FACE};
pub use delta::{DeltaSet, FeatureDelta, DELTA_RECOMMEND_THRESHOLD};
pub use history::{EditCommand, History, Transaction};
pub use ops::{
    validate_hole, AddHole, DeleteFeatures, GeomPath, InsertFeature, MoveFeature, MoveVertex,
    UpdateProperties,
};
