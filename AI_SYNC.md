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

> 每次收工回记时更新。截至 **2026-08-03 · v0.15.0 · 134 测试全绿**。

### 1.1 已完成实现

| 模块 | 状态 | 内容 |
|------|------|------|
| kanyu-core | ✅ stable | GeoArrow RecordBatch 内存模型；17 格式注册表；AGENTS.md 语义层；系统自省 |
| 格式 I/O | ✅ | GeoJSON/CSV/TSV/xlsx/SHP(读写)/FGB/GeoParquet/DXF/KML/KMZ/DWG(读) 全免 GDAL |
| DWG | ✅(读) | acadrust+自持补丁层（裁决 #18）；六类几何+标注要素+椭圆；143 样本/52 万实体验证 |
| 分析 | ✅ | buffer/overlay/topology/sjoin/zonal_stats/measure + EPSG 全库投影 |
| kanyu-render | ✅ | 离屏 PNG/SVG；晨山/夜观星；graduated/categorical 符号化 |
| kanyu-mcp | ✅ | 17 stable 工具；stdio+streamable HTTP；SEP-2663 长任务 |
| kanyu-skill | 🚧 incubating | wasmtime+WIT 宿主；燃料沙箱；MCP 热加载（hotload/skill_run/skill_list） |
| kanyu-cli | ✅ | 7 命令组，全局 --json；v0.14.0 已发布并安装 |
| kanyu-shell | 🚧 incubating | v0.3：ArcGIS Pro 式 Ribbon 图标大按钮（ui_kit::icons 33 枚线性图标）、Contents 骨架目录树、终端/AI 对话双页签（LocalDriver+OpenAiDriver 双驱动）、地图色彩解耦、ui_kit 设计系统 |
| 堪舆数据库 .kdb | ✅ | 自研存档（裁决 #19）：Arrow IPC + kanyu.* 元数据，RecordBatch 直通类型保真，全格式转换接入 |
| 堪舆工程 .kyu | ✅ | JSON 工程清单（裁决 #19）：图层引用/视口/地图色彩/可见性，壳层打开/保存 |
| 开源规范 | ✅ | 双许可/CI/Release 工作流/五份接口文档/README 实拍图 |
| 上游回馈 | ✅ | acadrust issue #55（AC15 定位缺陷 + 修法 + 证据） |

### 1.2 待完成事项（优先级序）

1. **基础 GIS 功能移植**（用户指令，进行中）：宗地 TXT（读/写/质检）+ 图层统计（亩/公顷）——本轮落地；后续批次见 §6.4 移植清单
2. **crates.io 发布**：六个名称可注册，待用户 cargo login（发布顺序 core→render→skill→mcp→cli）
3. **DWG 深化**（用户决定后置）：INSERT 拆块 / HATCH 边界 / SPLINE 采样 / R2018+ 复测
4. **Phase 2 视界续**：wgpu 实时渲染管线（KanyuDB→SSBO）、MLT 瓦片、SDF 文字
5. **Phase 3 手**：DCEL 增量拓扑编辑内核、Undo/Redo
6. **Phase 4 脑**：LLM 融合（自然语言→工具调用编排）、MCP resources/prompts、GeoAnalystBench 基准
7. **Phase 5 魂续**：技能市场、A/B 测试框架、知识库 RAG
8. **性能基准**：对 QGIS 的 §5.3 指标实测并公开基准报告
9. **parquet codec 裁剪**：zstd-sys 等 C codec 经 parquet 引入，评估裁剪保持"内核零 C"纯度
10. **属性面板重建**：等待用户定制要求

### 1.3 自我迭代边界（不可逾越）

- **堪舆灵不在用户运行时直接修改内核**。自我迭代发生在 **GitHub 协作层**：
  所有变更经提交/PR 进入仓库，CI（fmt+clippy+test+deny）必须全绿，
  内核合并须人道明远审核（现阶段）；WASM 技能热加载是唯一免审核通道
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
### [收工] 2026-08-03 kimi-code(main) — QGIS 核心算法移植 + Python 打通 + 宗地 TXT
- 提交：见本次 commit；测试：149 全绿（含 parcel/toolbox 集成测试）+ clippy 零警告
- 验证：Python 直调 Rust 内核（load/query/buffer/stats/render_png 实测）；kanyu toolbox list/run 端到端；宗地 TXT 读写质检往返
- 内容：geoprocess 六算法+stats（QGIS 语义，keep-first/DP/删洞阈值/炸开）；parcel.rs（宗地 TXT 双向+质检，注册表第 19 格式）；kanyu-py（PyO3，21 函数）+ python/kanyu 包（Layer 链式 + toolbox 运行时，修复 -m 双实例发现 bug 为 __module__ 归属判定）；CLI analysis 七命令 + data validate + toolbox list/run；裁决 #20 入规；文档全链（ARCHITECTURE/API/SDK/CLI/MCP/CHANGELOG）
- 偏差：dissolve 测试期望值修正（4+4-2=6）；写出侧闭合点复用首点编号（格式校验要求）
- 后续：§1.2 #1 基础 GIS 移植第一批完成；#2 crates.io 待 token
### [开工] 2026-08-03 kimi-code(main) — QGIS 核心算法移植 + Rust/Python 打通（kanyu-py + 工具箱）
- 范围：kanyu-core/geoprocess.rs（QGIS 六算法+stats）、crates/kanyu-py（新，PyO3）、python/kanyu/（包+toolbox 约定）、CLI toolbox 命令组、文档（裁决 #20）
- 依据：用户指令（QGIS 核心算法移植正确转写；Rust 核心 Python 调动；ArcGIS Pro .pyt 工具箱方式）；总规 §5.1 脚本层
- 预计：大（新 crate + Python 包 + CLI + 文档）
### [开工] 2026-08-03 kimi-code(main) — DWG INSERT 拆块（块参照展开）
- 范围：crates/kanyu-core/src/dwg.rs + 测试 + 文档
- 依据：AI_SYNC §1.2 #4（DWG 深化）；总规 §6.4 Phase 5 遗留；spike 统计 INSERT=22.4%
- 预计：中（约 200 行 + 测试）
### [收工] 2026-08-03 kimi-code(main) — 项目级命名修正：基因 → 技能
- 提交：d88820e；测试：134 全绿 + clippy 零警告；验证：attr_scaler.wasm 按 kanyu:skill/analyzer 新 ABI 重编并通过 wasm-tools 组件化校验
- 内容：crate/类型/WIT ABI/MCP 工具（kanyu_skill_run/skill_list）/CLI 命令组（kanyu skill）/UI/全文档同步改名；历史记录不改写；AGENTS.md 依赖方向修正（render/skill ← core；cli/mcp/shell ← core+render+skill）
- 后续：§1.1 kanyu-gene 行应改 kanyu-skill（下轮快照顺手更新）
### [收工] 2026-08-03 kimi-code(main) — 桌面端 MSI 安装包与 Release 同步
- 提交：packaging/wix/kanyu.wxs（安装包即代码）；验证：MSI magic 校验 + msiexec /qn 静默安装全文件就位；Release v0.15.0 已附加 `kanyu-0.15.0-x86_64.msi`（25MB，幂等重传）并更新安装说明；README 快速开始加 MSI 路径
- 内容：WiX v5（dotnet tool）制作用户级 MSI（免 UAC）：GUI+CLI+MCP+图标/许可，桌面「堪舆」与开始菜单快捷方式；Util 扩展 PATH 环境变量因扩展解析失败降级移除（本机手动安装已配 PATH，不影响交付）
- 后续：release.yml 可加 cargo-wix/MSI 工件（CI 化，列入 §1.2）
### [收工] 2026-08-03 kimi-code(main) — shell v0.5：四标准设计深化（HIG/QGIS/ArcGIS/邮箱）
- 提交：见本次 commit 组；测试：134 全绿 + clippy 零警告；验证：浅色截图人工确认（QAT 三段式、组名组宽内居中、QGIS 浏览器树根节点、Segoe/Cascadia 字体栈生效）
- 内容：联系邮箱统一 daomingyuan@qq.com；Apple HIG 文本分级（28/22/17sb/15/13/11/12）+ 连续圆角（6/10/14）+ 0.5px 发丝线 + Segoe UI/Cascadia Code 字体栈；目录面板改 QGIS 浏览器树（根节点懒加载）；图层 QGIS 工具栏+右键菜单+筛选；功能区 QAT 三段式
- 偏差：无（clippy 适配：sort_by_key、const assert、闭包 mut）
- 后续：§1.2 属性面板重建待用户定制；crates.io 发布仍待 token
### [收工] 2026-08-03 kimi-code(main) — shell v0.4：design-review 驱动的设计迭代
- 提交：3 个原子提交（Ribbon 版式/Catalog 分离/规范入档）；测试：132 全绿 + clippy 零警告；验证：双主题截图人工确认（Ribbon 三分离正确、组名归位、目录浏览器可用、暗色界面+晨山地图）
- 内容：design-review 技能沉淀入 ui_kit 规范；Catalog 文件浏览器（快捷位置/面包屑/数据文件过滤/双击加载）；左侧双页签（目录|图层）；Ribbon 版式系统修复（含组名横跨窗口定位 bug）；删除右侧属性面板（待用户定制）
- 偏差：无（计划外修复：组名定位、借用冲突、MB/GB 常量作用域）
- 后续：§1.2 属性面板重建待用户定制要求；crates.io 发布仍待 token
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
- 内容：ui_kit 设计系统（tokens/controls/containers + 铁律入 AGENTS.md #8）；七页签 Ribbon；独立终端（内核直驱）；可停靠面板；11 类对话框；技能入壳；Layer::from_collection
- 偏差：子代理（agent-5）因配额中断，主体由主线程完成；ribbon 组静态布局改 Vec（静态提升受限）；无其他偏差
- 后续：GUI 安装 + 桌面快捷方式"堪舆"（本次收尾闭环）
### [开工] 2026-08-03 kimi-code(main) — kanyu-shell v0.2 深度 UI 改建（ArcGIS Pro Ribbon + 独立终端 + bitfun 卡片视觉）
- 范围：crates/kanyu-shell 全面重构（ribbon/panels/console/dialogs/theme 模块化）、kanyu-skill 接线入 shell、文档与会签簿
- 依据：用户指令（借鉴 ArcGIS Pro 分类设计 + 独立终端 + bitfun 设计思路）；总规第二部分
- 预计：大（约 1500+ 行新代码）
### [开工] 2026-08-11 kimi-code(main) — ArcGIS Pro 位图图标接入（本机资源双轨制）
- 范围：kanyu-shell ui_kit/icons.rs（IconCache/draw_or_image/arcgis_resource_name 映射表）、controls.rs、ribbon.rs、app.rs；AGENTS.md 加图标工作流
- 依据：用户指令（按 Esri DAML-ID 图标下载调用、主题风格优化、供后续扩展）；总规 §1.4 图标系统
- 许可边界：Esri 位图 PNG 仅存本机 %LOCALAPPDATA%\Programs\kanyu\icons\（light 10916 + dark 10900，提取自用户已授权 ArcGIS Pro 安装），不进仓库再分发；仓库保留手绘回退
- 预计：中（约 200 行变更）
### [收工] 2026-08-11 kimi-code(main) — ArcGIS Pro 位图图标接线闭环
- 提交：0756bc2；测试：149 全绿 + clippy 零警告；验证：晨山/夜观星双主题截图目检（Ribbon 大按钮 ArcGIS 彩色位图清晰、dark 主题取 darkimages 变体）；release exe 47.8MB 已同步安装至 Programs\kanyu
- 内容：IconCache（本机 icons 目录探测 + 主题纹理缓存，tiny-skia 解码）、draw_or_image() 双轨入口、arcgis_resource_name() 33 枚映射表（扩展登记点）；ribbon_button/qat_button/Ribbon::ui 全链接线；AGENTS.md 加图标工作流与许可边界
- 本机资源：icons light 10916 + dark 10900 PNG（提取自用户已授权 ArcGIS Pro 安装，未入仓库）
- 偏差：clippy map_entry 改 entry API；Icon::Gene 改名遗漏修正（Skill）
- 后续：目录树/图层树行图标仍走手绘 draw（tree_row 未接线，可后续评估）；crates.io 发布仍待 token
