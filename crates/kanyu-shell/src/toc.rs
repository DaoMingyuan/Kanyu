//! 目录树（TOC, Table of Contents）数据模型 —— ArcGIS Pro Contents 窗格范式。
//!
//! ## 设计约定
//!
//! - **身份**：`TocNode::Layer` 存图层 **id(String)** 而非 layers Vec 下标——
//!   layers Vec 增删会使下标漂移，id 才是稳定身份；查找时按 id 定位（图层数量级
//!   为十~百，线性查找足够）。
//! - **排序**：目录树**树顶图层绘制在最上层**；渲染合并时按"目录树自下而上"
//!   的顺序压入要素（自下而上 = 树底图层先绘制、树顶图层最后绘制压顶）。
//! - **有效可见性** = 图层自身 visible 且所有祖先组 visible；合并缓存
//!   （rebuild_merged）只合并有效可见图层。
//! - **组路径**：嵌套组以 "/" 连接（如 `"基底/参考"`），与 .kyu 工程的
//!   `ProjectLayer::group` 字段同一约定；组名内的 "/" 与 "\" 在创建/重命名时
//!   被清洗为全角"／"，保证路径可逆解析。
//! - 本模块全部为**纯函数**（不依赖 egui / Layer 本体），便于单元测试；
//!   图层自身可见性等外部状态经闭包参数注入。

/// 目录树节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocNode {
    /// 图层节点（值为图层 id）。
    Layer(String),
    /// 图层组。
    Group(GroupNode),
}

/// 图层组节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupNode {
    /// 组名（同级唯一，创建/重命名时保证）。
    pub name: String,
    /// 展开（目录树 UI 态）。
    pub expanded: bool,
    /// 组可见性（一键显隐全组；有效可见性还需各图层自身可见）。
    pub visible: bool,
    /// 子节点（树顶 = children 首部）。
    pub children: Vec<TocNode>,
}

impl TocNode {
    fn group(&self) -> Option<&GroupNode> {
        match self {
            TocNode::Group(g) => Some(g),
            TocNode::Layer(_) => None,
        }
    }
}

/// 移动方向（在父列表内）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveDir {
    /// 上移一位（朝树顶）。
    Up,
    /// 下移一位（朝树底）。
    Down,
    /// 移至父列表顶。
    Top,
    /// 移至父列表底。
    Bottom,
}

// ===== 路径工具 =====

/// 拆分组路径（忽略空段；"" → 无段 = 根）。
fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// 清洗组名：路径分隔符替换为全角"／"，去除首尾空白。
pub fn sanitize_group_name(name: &str) -> String {
    name.trim().replace(['/', '\\'], "／")
}

// ===== 查找 =====

/// 树中是否含指定图层。
pub fn contains_layer(toc: &[TocNode], id: &str) -> bool {
    toc.iter().any(|n| match n {
        TocNode::Layer(l) => l == id,
        TocNode::Group(g) => contains_layer(&g.children, id),
    })
}

/// 按路径取组（不可变）。path 为空串返回 None（根不是组）。
pub fn find_group<'a>(toc: &'a [TocNode], path: &str) -> Option<&'a GroupNode> {
    let segs = split_path(path);
    let mut level = toc;
    let mut found: Option<&GroupNode> = None;
    for seg in segs {
        found = level.iter().find_map(|n| match n {
            TocNode::Group(g) if g.name == seg => Some(g),
            _ => None,
        });
        level = &found?.children;
    }
    found
}

/// 按路径取组（可变）。递归下潜，避免多级可变借用的纠缠。
pub fn find_group_mut<'a>(toc: &'a mut [TocNode], path: &str) -> Option<&'a mut GroupNode> {
    let segs = split_path(path);
    let (first, rest) = segs.split_first()?;
    for n in toc.iter_mut() {
        if let TocNode::Group(g) = n {
            if g.name == *first {
                if rest.is_empty() {
                    return Some(g);
                }
                return find_group_mut(&mut g.children, &rest.join("/"));
            }
        }
    }
    None
}

/// 图层所在组路径；根级图层返回 `Some("")`；未找到返回 `None`。
pub fn group_path_of(toc: &[TocNode], id: &str) -> Option<String> {
    fn rec(nodes: &[TocNode], id: &str, prefix: &str) -> Option<String> {
        for n in nodes {
            match n {
                TocNode::Layer(l) if l == id => return Some(prefix.to_string()),
                TocNode::Group(g) => {
                    let child_prefix = if prefix.is_empty() {
                        g.name.clone()
                    } else {
                        format!("{prefix}/{}", g.name)
                    };
                    if let Some(p) = rec(&g.children, id, &child_prefix) {
                        return Some(p);
                    }
                }
                _ => {}
            }
        }
        None
    }
    rec(toc, id, "")
}

/// 全部组路径（先序，供「移至分组」子菜单列举）。
pub fn group_paths(toc: &[TocNode]) -> Vec<String> {
    let mut out = Vec::new();
    fn rec(nodes: &[TocNode], prefix: &str, out: &mut Vec<String>) {
        for n in nodes {
            if let TocNode::Group(g) = n {
                let path = if prefix.is_empty() {
                    g.name.clone()
                } else {
                    format!("{prefix}/{}", g.name)
                };
                out.push(path.clone());
                rec(&g.children, &path, out);
            }
        }
    }
    rec(toc, "", &mut out);
    out
}

/// 组内图层总数（递归，供「组名 (N 项)」）。
pub fn layer_count(nodes: &[TocNode]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            TocNode::Layer(_) => 1,
            TocNode::Group(g) => layer_count(&g.children),
        })
        .sum()
}

/// 组内全部图层 id（树序，递归）。
pub fn group_layer_ids(nodes: &[TocNode]) -> Vec<String> {
    let mut out = Vec::new();
    for n in nodes {
        match n {
            TocNode::Layer(l) => out.push(l.clone()),
            TocNode::Group(g) => out.extend(group_layer_ids(&g.children)),
        }
    }
    out
}

// ===== 插入 / 移除 =====

/// 新图层插入树顶（ArcGIS 约定：新图层在最上方）。幂等：已存在则忽略。
pub fn insert_layer_top(toc: &mut Vec<TocNode>, id: &str) {
    if !contains_layer(toc, id) {
        toc.insert(0, TocNode::Layer(id.to_string()));
    }
}

/// 移除图层节点（递归）。返回是否移除成功。
pub fn remove_layer(toc: &mut Vec<TocNode>, id: &str) -> bool {
    if let Some(pos) = toc
        .iter()
        .position(|n| matches!(n, TocNode::Layer(l) if l == id))
    {
        toc.remove(pos);
        return true;
    }
    for n in toc.iter_mut() {
        if let TocNode::Group(g) = n {
            if remove_layer(&mut g.children, id) {
                return true;
            }
        }
    }
    false
}

/// 确保组路径存在（逐级创建缺失组，新组默认展开+可见）。
pub fn ensure_group_path(toc: &mut Vec<TocNode>, path: &str) {
    let mut level = toc;
    for seg in split_path(path) {
        let exists = level
            .iter()
            .any(|n| matches!(n, TocNode::Group(g) if g.name == seg));
        if !exists {
            level.push(TocNode::Group(GroupNode {
                name: seg.to_string(),
                expanded: true,
                visible: true,
                children: Vec::new(),
            }));
        }
        let idx = level
            .iter()
            .position(|n| matches!(n, TocNode::Group(g) if g.name == seg))
            .expect("刚插入的组必存在");
        level = match &mut level[idx] {
            TocNode::Group(g) => &mut g.children,
            TocNode::Layer(_) => unreachable!(),
        };
    }
}

/// 把图层移动/插入到指定组（None = 根）的**顶部**；节点原位置摘除。
/// 用于「移至分组」「移出分组」与工程恢复（.kyu 的 group 字段）。
pub fn insert_layer_into(toc: &mut Vec<TocNode>, path: Option<&str>, id: &str) {
    remove_layer(toc, id);
    match path {
        Some(p) if !split_path(p).is_empty() => {
            ensure_group_path(toc, p);
            let g = find_group_mut(toc, p).expect("ensure_group_path 后组必存在");
            g.children.insert(0, TocNode::Layer(id.to_string()));
        }
        _ => toc.insert(0, TocNode::Layer(id.to_string())),
    }
}

// ===== 可见性 =====

/// 有效可见图层 id，按**绘制顺序**（目录树自下而上：树底先绘制、树顶压顶）。
/// `own_visible(id)` 注入图层自身可见性；祖先组任一无可见则整支不可见。
pub fn visible_draw_order(toc: &[TocNode], own_visible: impl Fn(&str) -> bool) -> Vec<String> {
    fn rec(
        nodes: &[TocNode],
        ancestors_visible: bool,
        own: &dyn Fn(&str) -> bool,
        out: &mut Vec<String>,
    ) {
        for n in nodes {
            match n {
                TocNode::Layer(l) => {
                    if ancestors_visible && own(l) {
                        out.push(l.clone());
                    }
                }
                TocNode::Group(g) => rec(&g.children, ancestors_visible && g.visible, own, out),
            }
        }
    }
    let mut top_down = Vec::new();
    rec(toc, true, &own_visible, &mut top_down);
    top_down.reverse();
    top_down
}

/// 组复选框状态：组自身可见且**全部后代图层有效可见**（全显）。
pub fn group_all_on(toc: &[TocNode], path: &str, own_visible: impl Fn(&str) -> bool) -> bool {
    let Some(g) = find_group(toc, path) else {
        return false;
    };
    if !g.visible {
        return false;
    }
    fn rec(nodes: &[TocNode], own: &dyn Fn(&str) -> bool) -> bool {
        nodes.iter().all(|n| match n {
            TocNode::Layer(l) => own(l),
            TocNode::Group(g) => g.visible && rec(&g.children, own),
        })
    }
    rec(&g.children, &own_visible)
}

/// 组一键显隐（ArcGIS 语义：全显→点击全隐；否则点击全显）。
/// - `on = true`：组可见，且全部后代图层置为自身可见（全显）；
/// - `on = false`：仅组自身置不可见（全隐；子孙自身标记保留，再全显时不动记忆之外的态）。
///
/// 图层自身标记经 `set_layer(id, visible)` 回写。
pub fn set_group_all(
    toc: &mut [TocNode],
    path: &str,
    on: bool,
    mut set_layer: impl FnMut(&str, bool),
) {
    let Some(g) = find_group_mut(toc, path) else {
        return;
    };
    g.visible = on;
    if on {
        // 全显：子孙图层全部置自身可见（经闭包回写外部图层状态）。
        let ids = group_layer_ids(&g.children);
        for id in ids {
            set_layer(&id, true);
        }
        // 后代子组也全部置可见（否则有效可见性仍被压制）。
        fn show_groups(nodes: &mut [TocNode]) {
            for n in nodes.iter_mut() {
                if let TocNode::Group(g) = n {
                    g.visible = true;
                    show_groups(&mut g.children);
                }
            }
        }
        show_groups(&mut g.children);
    }
}

/// 全部组可见性（空白区菜单「全部显示/全部隐藏」的组侧；图层侧由调用方逐图层置）。
pub fn set_all_groups_visible(toc: &mut [TocNode], visible: bool) {
    for n in toc.iter_mut() {
        if let TocNode::Group(g) = n {
            g.visible = visible;
            set_all_groups_visible(&mut g.children, visible);
        }
    }
}

// ===== 展开 / 折叠 =====

/// 全部组展开/折叠（图层骨架子节点状态由调用方另置）。
pub fn set_all_expanded(toc: &mut [TocNode], expanded: bool) {
    for n in toc.iter_mut() {
        if let TocNode::Group(g) = n {
            g.expanded = expanded;
            set_all_expanded(&mut g.children, expanded);
        }
    }
}

/// 指定组及其全部后代组展开/折叠（组菜单「展开全部/折叠全部」，作用域 = 该组子树）。
pub fn set_group_expanded(toc: &mut [TocNode], path: &str, expanded: bool) {
    if let Some(g) = find_group_mut(toc, path) {
        g.expanded = expanded;
        set_all_expanded(&mut g.children, expanded);
    }
}

/// 切换组展开态。
pub fn toggle_group_expand(toc: &mut [TocNode], path: &str) {
    if let Some(g) = find_group_mut(toc, path) {
        g.expanded = !g.expanded;
    }
}

// ===== 移动 =====

/// 在父列表内移动图层节点（上移/下移/顶层/底层）。返回是否有位移。
pub fn move_layer(toc: &mut Vec<TocNode>, id: &str, dir: MoveDir) -> bool {
    // 先在本级找；找不到则下潜各组。
    if let Some(pos) = toc
        .iter()
        .position(|n| matches!(n, TocNode::Layer(l) if l == id))
    {
        let last = toc.len() - 1;
        let target = match dir {
            MoveDir::Up => pos.saturating_sub(1),
            MoveDir::Down => (pos + 1).min(last),
            MoveDir::Top => 0,
            MoveDir::Bottom => last,
        };
        if target != pos {
            let node = toc.remove(pos);
            toc.insert(target, node);
            return true;
        }
        return false;
    }
    for n in toc.iter_mut() {
        if let TocNode::Group(g) = n {
            if move_layer(&mut g.children, id, dir) {
                return true;
            }
        }
    }
    false
}

// ===== 组管理 =====

/// 同级唯一组名：`base`、`base 2`、`base 3`…
fn unique_child_name(nodes: &[TocNode], base: &str) -> String {
    let taken: Vec<&str> = nodes
        .iter()
        .filter_map(|n| n.group().map(|g| g.name.as_str()))
        .collect();
    if !taken.contains(&base) {
        return base.to_string();
    }
    for i in 2.. {
        let candidate = format!("{base} {i}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!()
}

/// 新建组（自动唯一名「新建图层组」）。parent = None 为根级。返回新组路径。
pub fn new_group(toc: &mut Vec<TocNode>, parent: Option<&str>) -> String {
    let children = match parent {
        Some(p) => {
            ensure_group_path(toc, p);
            &mut find_group_mut(toc, p)
                .expect("ensure_group_path 后组必存在")
                .children
        }
        None => &mut *toc,
    };
    let name = unique_child_name(children, "新建图层组");
    children.push(TocNode::Group(GroupNode {
        name: name.clone(),
        expanded: true,
        visible: true,
        children: Vec::new(),
    }));
    match parent {
        Some(p) if !split_path(p).is_empty() => format!("{p}/{name}"),
        _ => name,
    }
}

/// 以指定名新建组（对话框采集名）。同级重名/空名报错。返回新组路径。
pub fn new_group_named(
    toc: &mut Vec<TocNode>,
    parent: Option<&str>,
    name: &str,
) -> Result<String, String> {
    let name = sanitize_group_name(name);
    if name.is_empty() {
        return Err("组名不能为空".to_string());
    }
    let children = match parent {
        Some(p) => {
            ensure_group_path(toc, p);
            &mut find_group_mut(toc, p)
                .expect("ensure_group_path 后组必存在")
                .children
        }
        None => &mut *toc,
    };
    if children
        .iter()
        .any(|n| matches!(n, TocNode::Group(g) if g.name == name))
    {
        return Err(format!("同级已存在组「{name}」"));
    }
    children.push(TocNode::Group(GroupNode {
        name: name.clone(),
        expanded: true,
        visible: true,
        children: Vec::new(),
    }));
    Ok(match parent {
        Some(p) if !split_path(p).is_empty() => format!("{p}/{name}"),
        _ => name,
    })
}

/// 重命名组。空名/同级重名报错。返回新路径。
pub fn rename_group(toc: &mut [TocNode], path: &str, new_name: &str) -> Result<String, String> {
    let new_name = sanitize_group_name(new_name);
    if new_name.is_empty() {
        return Err("组名不能为空".to_string());
    }
    let segs = split_path(path);
    let Some((last, parent_segs)) = segs.split_last() else {
        return Err("路径为空".to_string());
    };
    let parent_path = parent_segs.join("/");
    // 同级重名检查。
    let siblings: &[TocNode] = match find_group_mut(toc, &parent_path) {
        Some(g) => &g.children,
        None => toc,
    };
    if new_name != *last
        && siblings
            .iter()
            .any(|n| matches!(n, TocNode::Group(g) if g.name == new_name))
    {
        return Err(format!("同级已存在组「{new_name}」"));
    }
    let g = find_group_mut(toc, path).ok_or_else(|| format!("组不存在: {path}"))?;
    g.name = new_name.clone();
    Ok(if parent_path.is_empty() {
        new_name
    } else {
        format!("{parent_path}/{new_name}")
    })
}

/// 取消分组：子项上移到组所在位置（保持顺序），组壳删除。返回是否成功。
pub fn ungroup(toc: &mut Vec<TocNode>, path: &str) -> bool {
    // 先在本级找组；找到则原位展开子项。
    let segs = split_path(path);
    let Some((last, parent_segs)) = segs.split_last() else {
        return false;
    };
    let children: &mut Vec<TocNode> = match find_group_mut(toc, &parent_segs.join("/")) {
        Some(g) => &mut g.children,
        None => toc,
    };
    if let Some(pos) = children
        .iter()
        .position(|n| matches!(n, TocNode::Group(g) if g.name == *last))
    {
        if let TocNode::Group(g) = children.remove(pos) {
            for (i, child) in g.children.into_iter().enumerate() {
                children.insert(pos + i, child);
            }
            return true;
        }
    }
    false
}

/// 移除组（含全部子孙节点）。返回组内全部图层 id（调用方据此移除图层本体）。
pub fn remove_group(toc: &mut Vec<TocNode>, path: &str) -> Option<Vec<String>> {
    let segs = split_path(path);
    let (last, parent_segs) = segs.split_last()?;
    let children: &mut Vec<TocNode> = match find_group_mut(toc, &parent_segs.join("/")) {
        Some(g) => &mut g.children,
        None => toc,
    };
    let pos = children
        .iter()
        .position(|n| matches!(n, TocNode::Group(g) if g.name == *last))?;
    match children.remove(pos) {
        TocNode::Group(g) => Some(group_layer_ids(&g.children)),
        TocNode::Layer(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造：组A{a1, 组B{b1}}, 顶部图层 top（树顶在前）。
    fn sample() -> Vec<TocNode> {
        vec![
            TocNode::Group(GroupNode {
                name: "组A".to_string(),
                expanded: true,
                visible: true,
                children: vec![
                    TocNode::Layer("a1".to_string()),
                    TocNode::Group(GroupNode {
                        name: "组B".to_string(),
                        expanded: true,
                        visible: true,
                        children: vec![TocNode::Layer("b1".to_string())],
                    }),
                ],
            }),
            TocNode::Layer("top".to_string()),
        ]
    }

    #[test]
    fn insert_top_and_dedupe() {
        let mut toc = Vec::new();
        insert_layer_top(&mut toc, "x");
        insert_layer_top(&mut toc, "y");
        insert_layer_top(&mut toc, "x"); // 幂等
        assert_eq!(
            toc,
            vec![
                TocNode::Layer("y".to_string()),
                TocNode::Layer("x".to_string())
            ]
        );
    }

    #[test]
    fn remove_layer_recursive() {
        let mut toc = sample();
        assert!(remove_layer(&mut toc, "b1"));
        assert!(!remove_layer(&mut toc, "b1")); // 已删
        assert!(!contains_layer(&toc, "b1"));
        assert!(contains_layer(&toc, "a1"));
    }

    #[test]
    fn group_path_of_positions() {
        let toc = sample();
        assert_eq!(group_path_of(&toc, "top"), Some(String::new())); // 根级
        assert_eq!(group_path_of(&toc, "a1"), Some("组A".to_string()));
        assert_eq!(group_path_of(&toc, "b1"), Some("组A/组B".to_string()));
        assert_eq!(group_path_of(&toc, "ghost"), None);
    }

    #[test]
    fn ensure_group_path_creates_nested() {
        let mut toc = Vec::new();
        ensure_group_path(&mut toc, "甲/乙/丙");
        assert!(find_group(&toc, "甲/乙/丙").is_some());
        ensure_group_path(&mut toc, "甲/乙"); // 幂等不重复
        assert_eq!(group_paths(&toc), vec!["甲", "甲/乙", "甲/乙/丙"]);
    }

    #[test]
    fn insert_layer_into_group_top() {
        let mut toc = sample();
        insert_layer_into(&mut toc, Some("组A/组B"), "top");
        assert_eq!(group_path_of(&toc, "top"), Some("组A/组B".to_string()));
        // 组顶 = children 首部。
        let b = find_group(&toc, "组A/组B").unwrap();
        assert_eq!(b.children[0], TocNode::Layer("top".to_string()));
        // 移出到根。
        insert_layer_into(&mut toc, None, "top");
        assert_eq!(group_path_of(&toc, "top"), Some(String::new()));
        assert_eq!(toc[0], TocNode::Layer("top".to_string()));
    }

    #[test]
    fn visible_draw_order_bottom_up_and_ancestor_gate() {
        let mut toc = sample();
        let own = |_: &str| true;
        // 树序（顶→底）：组A(a1, 组B(b1)), top → 绘制序（底→顶）：top, b1, a1。
        assert_eq!(visible_draw_order(&toc, own), vec!["top", "b1", "a1"]);
        // 组A 不可见 → a1/b1 被压制，top 仍在。
        find_group_mut(&mut toc, "组A").unwrap().visible = false;
        assert_eq!(visible_draw_order(&toc, own), vec!["top"]);
        // 图层自身不可见。
        find_group_mut(&mut toc, "组A").unwrap().visible = true;
        let own2 = |id: &str| id != "b1";
        assert_eq!(visible_draw_order(&toc, own2), vec!["top", "a1"]);
    }

    #[test]
    fn group_all_on_and_set_group_all() {
        let mut toc = sample();
        let all = |_: &str| true;
        assert!(group_all_on(&toc, "组A", all));
        // 隐藏一个后代 → 非全显。
        let own = |id: &str| id != "b1";
        assert!(!group_all_on(&toc, "组A", own));
        // 全显：子孙图层与子组全部置可见。
        let mut hidden: Vec<String> = Vec::new();
        set_group_all(&mut toc, "组A", true, |id, v| {
            if !v {
                hidden.push(id.to_string());
            }
        });
        assert!(group_all_on(&toc, "组A", all));
        // 全隐：仅组自身不可见（子孙标记不动）。
        set_group_all(&mut toc, "组A", false, |_, _| {});
        assert!(!find_group(&toc, "组A").unwrap().visible);
        assert!(find_group(&toc, "组A/组B").unwrap().visible);
    }

    #[test]
    fn move_layer_within_parent() {
        let mut toc = vec![
            TocNode::Layer("a".to_string()),
            TocNode::Layer("b".to_string()),
            TocNode::Layer("c".to_string()),
        ];
        assert!(move_layer(&mut toc, "c", MoveDir::Top));
        assert_eq!(toc[0], TocNode::Layer("c".to_string()));
        assert!(move_layer(&mut toc, "c", MoveDir::Down));
        assert_eq!(toc[1], TocNode::Layer("c".to_string()));
        assert!(move_layer(&mut toc, "c", MoveDir::Bottom));
        assert_eq!(toc[2], TocNode::Layer("c".to_string()));
        assert!(!move_layer(&mut toc, "c", MoveDir::Bottom)); // 已在底
        assert!(move_layer(&mut toc, "c", MoveDir::Up));
        assert_eq!(toc[1], TocNode::Layer("c".to_string()));
        // 组内移动不越界：a1 已是组A 内顶位，上移无位移。
        let mut t2 = sample();
        assert!(!move_layer(&mut t2, "a1", MoveDir::Up));
        assert!(move_layer(&mut t2, "a1", MoveDir::Bottom)); // 移到组A 内底（组B 之后）
        let a = find_group(&t2, "组A").unwrap();
        assert_eq!(a.children[1], TocNode::Layer("a1".to_string()));
    }

    #[test]
    fn new_group_unique_and_nested() {
        let mut toc = Vec::new();
        let p1 = new_group(&mut toc, None);
        let p2 = new_group(&mut toc, None);
        assert_eq!(p1, "新建图层组");
        assert_eq!(p2, "新建图层组 2");
        let sub = new_group(&mut toc, Some(&p1));
        assert_eq!(sub, "新建图层组/新建图层组"); // 父子不同级，可同名
        assert!(find_group(&toc, &sub).is_some());
    }

    #[test]
    fn new_group_named_validation() {
        let mut toc = Vec::new();
        assert!(new_group_named(&mut toc, None, "  ").is_err());
        assert_eq!(new_group_named(&mut toc, None, "基底").unwrap(), "基底");
        assert!(new_group_named(&mut toc, None, "基底").is_err()); // 重名
                                                                   // 路径分隔符清洗。
        assert_eq!(new_group_named(&mut toc, None, "a/b").unwrap(), "a／b");
    }

    #[test]
    fn rename_group_rules() {
        let mut toc = sample();
        assert_eq!(rename_group(&mut toc, "组A/组B", "乙").unwrap(), "组A/乙");
        assert!(find_group(&toc, "组A/乙").is_some());
        assert!(rename_group(&mut toc, "组A", "").is_err());
        let mut t2 = vec![
            TocNode::Group(GroupNode {
                name: "甲".into(),
                expanded: true,
                visible: true,
                children: vec![],
            }),
            TocNode::Group(GroupNode {
                name: "乙".into(),
                expanded: true,
                visible: true,
                children: vec![],
            }),
        ];
        assert!(rename_group(&mut t2, "甲", "乙").is_err()); // 同级重名
    }

    #[test]
    fn ungroup_lifts_children() {
        let mut toc = sample();
        assert!(ungroup(&mut toc, "组A"));
        // a1 与 组B 上移到根，组A 消失；顺序保持。
        assert_eq!(toc[0], TocNode::Layer("a1".to_string()));
        assert!(matches!(&toc[1], TocNode::Group(g) if g.name == "组B"));
        assert_eq!(group_path_of(&toc, "b1"), Some("组B".to_string()));
        assert!(!ungroup(&mut toc, "组A"));
    }

    #[test]
    fn remove_group_collects_layer_ids() {
        let mut toc = sample();
        let ids = remove_group(&mut toc, "组A").unwrap();
        assert_eq!(ids, vec!["a1".to_string(), "b1".to_string()]);
        assert_eq!(toc.len(), 1);
        assert!(remove_group(&mut toc, "组A").is_none());
    }

    #[test]
    fn expand_and_visibility_sweeps() {
        let mut toc = sample();
        set_all_expanded(&mut toc, false);
        assert!(!find_group(&toc, "组A").unwrap().expanded);
        assert!(!find_group(&toc, "组A/组B").unwrap().expanded);
        set_group_expanded(&mut toc, "组A", true);
        assert!(find_group(&toc, "组A").unwrap().expanded);
        assert!(find_group(&toc, "组A/组B").unwrap().expanded);
        toggle_group_expand(&mut toc, "组A/组B");
        assert!(!find_group(&toc, "组A/组B").unwrap().expanded);
        set_all_groups_visible(&mut toc, false);
        assert!(!find_group(&toc, "组A/组B").unwrap().visible);
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize_group_name("  a/b\\c "), "a／b／c");
    }
}
