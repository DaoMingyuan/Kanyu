//! kanyu-edit —— 堪舆编辑内核（MASTERPLAN Phase 3「手」的首个增量）。
//!
//! ## 定位与增量路线
//!
//! Phase 3 终态是 DCEL 增量拓扑编辑内核；本增量（v1）交付：
//! - [`history`]：Undo/Redo 框架（`EditCommand` trait + 双栈 + 容量淘汰）；
//! - [`ops`]：基础编辑命令（顶点移动 / 要素平移 / 删除 / 插入 / 属性更新），
//!   全部为纯函数式命令（apply/revert over `FeatureCollection`）。
//!
//! 后续增量（路线图，rustdoc 即契约）：
//! - v2：GeoArrow Delta 快照（大图层下逆操作回放的成本优化）；
//! - v3：壳层编辑模式（顶点拖拽手柄/绘制工具）接入；
//! - v4：DCEL 拓扑（共享边/结点的联动编辑）。

pub mod history;
pub mod ops;

pub use history::{EditCommand, History};
pub use ops::{
    validate_hole, AddHole, DeleteFeatures, GeomPath, InsertFeature, MoveFeature, MoveVertex,
    UpdateProperties,
};
