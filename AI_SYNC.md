# AI_SYNC.md —— 堪舆长久性联动机制

> **任何 AI 代理或开发者在堪舆仓库工作前，必须先读完本文件并登记；收工必须回记。**
> 本文件是所有迭代者（人类与 AI）的"会签簿"与"作战地图"。
> 遵循 [agents.md](https://agents.md) 规范；本文件是仓库级 AGENTS.md 的强制前置阅读。

---

## 0. 联动协议（强制）

### 0.1 开工前（Step 0，缺一不可）

1. **读本文件**：§1 状态快照（已完成/待完成）、§2 迭代会签簿（他人在做什么）。
2. **读总规**：`docs/MASTERPLAN.md` 第六部分（18+ 项裁决）与 §6.4 阶段清单——裁决即历史结论，不得推翻重来，除非新增裁决条目。
3. **读代码真相**：`kanyu introspect --json`（模块/工具/格式矩阵的单一事实来源）。
4. **登记开工**（见 §0.2），**然后才动手**。

### 0.2 开工登记（先于代码改动）

在 §2 会签簿**顶部**追加一条（≤6 行）：

```
### [开工] 2026-08-03 <迭代者标识> — <一句话意图>
- 范围：<预计触动的模块/文件>
- 依据：<总规条目 / Issue / 裁决编号>
- 预计：<体量估计（小/中/大）>
```

- **范围避让**：先读已有"开工"条目，与其重叠的范围须另选或等待；后登记者让行。
- 迭代者标识建议带身份，如 `kimi-code(agent-3)`、`claude-code`、`codex`、人类 GitHub ID。

### 0.3 收工回记（随最终提交）

同一位置追加（≤8 行）：

```
### [收工] 2026-08-03 <迭代者标识> — <一句话结果>
- 提交：<hash 列表>；测试：<数字>；验证：<fmt/clippy/冒烟>
- 偏差：<与原意图的差异及原因；无则写"无">
- 后续：<新产生的待办，已同步写入 §1.2>
```

并同步：① §1.1/§1.2 状态快照；② 涉及能力变化时更新 `crates/kanyu-core/src/introspect.rs`（单一事实来源）；③ CHANGELOG.md。

### 0.4 文件纪律

- **只增不改**：会签簿历史条目永不删改（纠错以新条目注明）；新条目永远加在会签簿**顶部**。
- **先拉后推**：改动本文件前 `git pull --rebase`，冲突时保留双方条目（按时间排序）。
- **单入口**：协议只写在本文件，AGENTS.md 与 CONTRIBUTING.md 指向这里，不复制条款。

---

## 1. 状态快照

> 每次收工回记时更新。截至 **2026-08-03 · v0.14.0+ · 130 测试全绿**。

### 1.1 已完成实现

| 模块 | 状态 | 内容 |
|------|------|------|
| kanyu-core | ✅ stable | GeoArrow RecordBatch 内存模型；17 格式注册表；AGENTS.md 语义层；系统自省 |
| 格式 I/O | ✅ | GeoJSON/CSV/TSV/xlsx/SHP(读写)/FGB/GeoParquet/DXF/KML/KMZ/DWG(读) 全免 GDAL |
| DWG | ✅(读) | acadrust+自持补丁层（裁决 #18）；六类几何+标注要素+椭圆；143 样本/52 万实体验证 |
| 分析 | ✅ | buffer/overlay/topology/sjoin/zonal_stats/measure + EPSG 全库投影 |
| kanyu-render | ✅ | 离屏 PNG/SVG；晨山/夜观星；graduated/categorical 符号化 |
| kanyu-mcp | ✅ | 17 stable 工具；stdio+streamable HTTP；SEP-2663 长任务 |
| kanyu-gene | 🚧 incubating | wasmtime+WIT 宿主；燃料沙箱；MCP 热加载（hotload/gene_run/gene_list） |
| kanyu-cli | ✅ | 7 命令组，全局 --json；v0.14.0 已发布并安装 |
| kanyu-shell | 🚧 incubating | v0.3：ArcGIS Pro 式 Ribbon 图标大按钮（ui_kit::icons 33 枚线性图标）、Contents 骨架目录树、终端/AI 对话双页签（LocalDriver+OpenAiDriver 双驱动）、地图色彩解耦、ui_kit 设计系统 |
| 堪舆数据库 .kdb | ✅ | 自研存档（裁决 #19）：Arrow IPC + kanyu.* 元数据，RecordBatch 直通类型保真，全格式转换接入 |
| 堪舆工程 .kyu | ✅ | JSON 工程清单（裁决 #19）：图层引用/视口/地图色彩/可见性，壳层打开/保存 |
| 开源规范 | ✅ | 双许可/CI/Release 工作流/五份接口文档/README 实拍图 |
| 上游回馈 | ✅ | acadrust issue #55（AC15 定位缺陷 + 修法 + 证据） |

### 1.2 待完成事项（优先级序）

1. **桌面 UI**（kanyu-shell MVP ✅ 2026-08-03 → 打包安装，桌面图标"堪舆"）——打包进行中
2. **凤鸟图标**：入库 assets/ + 快捷方式/GUI 窗口/GitHub 社会预览（待用户提供图片文件）
3. **crates.io 发布**：六个名称可注册，待用户 cargo login（发布顺序 core→render→gene→mcp→cli）
4. **DWG 深化**：INSERT 拆块（22.4% 实体）/ HATCH 边界 / SPLINE 采样 / R2018+ 样本复测
5. **Phase 2 视界续**：wgpu 实时渲染管线（GeoArrow→SSBO）、MLT 瓦片、SDF 文字
6. **Phase 3 手**：DCEL 增量拓扑编辑内核、Undo/Redo
7. **Phase 4 脑**：LLM 融合（自然语言→工具调用编排）、MCP resources/prompts、GeoAnalystBench 基准
8. **Phase 5 魂续**：基因市场、A/B 测试框架、知识库 RAG
9. **性能基准**：对 QGIS 的 §5.3 指标实测并公开基准报告
10. **parquet codec 裁剪**：zstd-sys 等 C codec 经 parquet 引入，评估裁剪保持"内核零 C"纯度

### 1.3 自我迭代边界（不可逾越）

- **堪舆灵不在用户运行时直接修改内核**。自我迭代发生在 **GitHub 协作层**：
  所有变更经提交/PR 进入仓库，CI（fmt+clippy+test+deny）必须全绿，
  内核合并须人道明远审核（现阶段）；WASM 基因热加载是唯一免审核通道
  （沙箱隔离，不改内核）。
- 任何 AI 不得删除/弱化本边界条款；修订只能以新裁决条目追加进总规 §6.1。

---

## 2. 迭代会签簿（新条目加在顶部）

### [收工] 2026-08-03 kimi-code(agent-5) — kanyu-shell 桌面 UI MVP 落地
- 提交：见本次 commit；测试：112 全绿（core 65/render 15/shell-view 8/mcp 6/gene 4/集成 14）；验证：fmt/clippy 零警告 + 晨山/夜观星/空状态三截图目检 + release 冒烟
- 偏差：eframe/egui 0.35 API 大变（SidePanel 并入 egui::Panel、App::update→App::ui）已适配；修复自身截图状态机 take() 误吞状态 bug（元组无条件求值致帧流断裂）；release 构建 1m42s（远快于预估），kanyu-shell.exe 24.1MB
- 后续：GUI 打包安装 + 桌面快捷方式「堪舆」（已同步 §1.2 #1）

### [开工] 2026-08-03 kimi-code(agent-5) — kanyu-shell 桌面 UI MVP 续作（agent-3 中途失联接管）
- 范围：crates/kanyu-shell（main.rs/app.rs 主体）、introspect、文档同步、截图验证、release 构建
- 依据：总规第二部分 + 裁决 #5（egui 方向）；用户指令（桌面端 UI 打包安装）；承接 agent-3 现场
  （render viewport 参数、shell Cargo.toml、view.rs 视图数学均已就绪）
- 预计：中（主体代码 + 验证；eframe/wgpu 依赖树已在 Cargo.lock 解析）

### [收工] 2026-08-03 kimi-code(main) — 联动机制文件建立
- 提交：见本次 commit；依据：用户指令（GitHub 长久联动机制 + 迭代边界入规）
- 内容：AI_SYNC.md 初版（协议/快照/边界/会签簿）；AGENTS.md 协议入口升级

### [开工] 2026-08-03 kimi-code(agent-3) — kanyu-shell 桌面 UI MVP
- 范围：crates/kanyu-shell（新）、kanyu-render（viewport 扩展）、assets/、introspect、文档
- 依据：总规第二部分 + 裁决 #5（egui 方向）；用户指令（桌面端 UI 打包安装）
- 预计：大（eframe/wgpu 依赖树，release 构建 15-25 分钟）

<!-- 新条目加在这行之上 -->
### [收工] 2026-08-03 kimi-code(main) — kanyu-shell v0.3 + KDB/KYU 双格式定版
- 提交：见本次 commit；测试：130 全绿 + clippy 零警告；验证：双主题截图人工确认（图标大按钮版式正确；**界面夜观星 + 地图固定晨山**解耦实证）；KDB 端到端（geojson→kdb→info/query）
- 内容：ui_kit::icons 33 枚线性图标 + ribbon_button/tab_strip/tree_row/password_input；Contents 骨架目录（弃卡片式）；底部双页签（终端|AI 对话）；ai.rs 双驱动（LocalDriver 意图引擎 + OpenAiDriver/ureq）；MapThemeMode 地图色彩解耦；KDB（Arrow IPC+kanyu.*）与 KYU（JSON 清单）入 core + 注册表第 18 格式 + 全格式转换；文档升级（裁决 #19、§3.5 格式节、ARCHITECTURE/API/README/CHANGELOG/CLI/MCP）
- 偏差：ribbon 静态组布局改 Vec；painter.arc 不存在改折线近似；ribbon_button 版式一次修正（按钮 min_size 撑满 64×52）；ureq 3 API 适配（header/send/read_to_string）
- 后续：§1.2 待办 #2 图标已闭环；GUI 安装与快捷方式沿用（本次同步更新 kanyu-shell.exe）
### [开工] 2026-08-03 kimi-code(main) — kanyu-shell v0.3：ArcGIS Pro 式深度升级
- 范围：ui_kit（icons/ribbon_button/tab_strip/tree_row/chat_bubble）、panels（骨架目录+双页签）、ribbon（图标按钮+组细分）、ai.rs（驱动+设置）、app（地图色彩解耦）、文档
- 依据：用户八点指令；总规 §1.4 图标系统/§2.1 UI 架构；计划文件 hawkman-scarlet-witch-miss-martian.md
- 预计：大（约 2000+ 行变更）
### [收工] 2026-08-03 kimi-code(main) — GUI 安装与桌面快捷方式闭环
- 内容：kanyu-shell.exe（GUI 子系统，PE 验证）安装至 Programs\kanyu；桌面 **堪舆.lnk**（GUI 直启 + 凤鸟图标）与 **堪舆终端.lnk**（wt + kanyu introspect）创建并校验（中文名正确）
- 偏差：PowerShell 5.1 无 BOM 按 ANSI 读 UTF-8 脚本致首轮快捷方式文件名乱码——清除后以带 BOM 脚本重建（教训入档：PS 脚本一律带 BOM 或全 ASCII）
- 后续：§1.2 待办 #1/#2 已闭环
### [收工] 2026-08-03 kimi-code(main) — kanyu-shell v0.2 深度 UI 改建落地
- 提交：见本次 commit；测试：119 全绿 + clippy 零警告；验证：双主题截图人工确认（Ribbon/终端/面板/对话框齐备）
- 内容：ui_kit 设计系统（tokens/controls/containers + 铁律入 AGENTS.md #8）；七页签 Ribbon；独立终端（内核直驱）；可停靠面板；11 类对话框；基因入壳；Layer::from_collection
- 偏差：子代理（agent-5）因配额中断，主体由主线程完成；ribbon 组静态布局改 Vec（静态提升受限）；无其他偏差
- 后续：GUI 安装 + 桌面快捷方式"堪舆"（本次收尾闭环）
### [开工] 2026-08-03 kimi-code(main) — kanyu-shell v0.2 深度 UI 改建（ArcGIS Pro Ribbon + 独立终端 + bitfun 卡片视觉）
- 范围：crates/kanyu-shell 全面重构（ribbon/panels/console/dialogs/theme 模块化）、kanyu-gene 接线入 shell、文档与会签簿
- 依据：用户指令（借鉴 ArcGIS Pro 分类设计 + 独立终端 + bitfun 设计思路）；总规第二部分
- 预计：大（约 1500+ 行新代码）
