# 更新日志 (Changelog)

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与[语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 新增

- **QGIS 核心算法移植**（新模块 `kanyu-core/src/geoprocess.rs`，语义对齐
  QGIS Processing）：dissolve（按字段分组并集，keep-first 属性）、simplify
  （Douglas-Peucker，退化剔除）、centroid、convex_hull、delete_holes
  （阈值可选）、explode（多部件炸开）、stats（图层统计：测地线口径 +
  亩/公顷/平方千米）。CLI `kanyu analysis dissolve/simplify/centroid/
  convexhull/deleteholes/explode/stats` 与 MCP 七个对应工具同步落地。
- **宗地 TXT 格式**（移植自堪舆工具箱 txt_feature.py，新模块 `parcel.rs`）：
  [属性描述]+[地块坐标] 双段解析（全角段标兼容）、X北Y东测绘惯例映射、
  圈号外环/洞、完整校验（首尾闭合/点数一致/面积非零）；简单点表
  （name X Y [Z]）回退解析；写出（闭合点复用首点编号）；质检
  `kanyu data validate` / MCP `kanyu_data_validate`（表头必备项、中文逗号、
  空格、结构规则；警告不阻塞）。注册表第 19 种格式（txt，读写）。
- **kanyu-py**：PyO3 扩展模块 `kanyu`（Rust 内核 → Python 全量调用：
  load/query/buffer/overlay/topology/sjoin/zonal_stats/dissolve/simplify/
  centroid/convex_hull/delete_holes/explode/stats/measure/reproject/
  render_png/render_svg/export；GeoJSON 文本契约 + Layer 链式封装）。
- **Python 工具箱**（ArcGIS Pro .pyt 式样，`python/kanyu/toolbox.py`）：
  Toolbox/Tool/Param 约定；CLI `kanyu toolbox list/run <file.py>` 驱动
  （JSON over stdout，参数自动类型化）；示例 `examples/planning_tools.py`。
- **CLI**：`kanyu data validate`（宗地 TXT 质检）。

### 变更

- **项目级命名修正：基因（gene）→ 技能（skill）**。crate `kanyu-gene` →
  `kanyu-skill`；类型 `GeneHost/Gene/GeneMeta/GeneError` →
  `SkillHost/Skill/SkillMeta/SkillError`；WIT ABI `kanyu:gene/analyzer` →
  `kanyu:skill/analyzer`（样板技能 attr_scaler.wasm 已按新 ABI 重编）；
  MCP 工具 `kanyu_gene_run/gene_list` → `kanyu_skill_run/skill_list`；
  CLI 命令组 `kanyu gene` → `kanyu skill`；UI 页签/按钮与全部文档同步。
  历史版本记录（CHANGELOG/会签簿旧条目）保持原样不改写。

## [0.15.0] - 2026-08-03

**桌面壳层落地：ArcGIS Pro 式深度 UI + 堪舆数据库/工程双格式 + AI 对话面板。**

### 新增

- **kanyu-shell v0.5**（四标准设计深化：Apple HIG + QGIS + ArcGIS Pro + design-review）：
  - **Apple HIG 字体与间距**：文本分级锚定 HIG Type Scale 桌面适配（28/22/17sb/15/
    13/11/12）；圆角连续语义（控件 6/卡片 10/大浮层 14）；0.5px 发丝线分隔；
    字体栈 Segoe UI（SF 近亲）+ Cascadia Code + 雅黑回退。
  - **QGIS 式停靠面板**：目录面板改 QGIS 浏览器树（主目录/桌面/文档/下载/
    项目目录/磁盘根节点，子目录懒加载展开）；图层面板加 QGIS 工具栏
    （缩放/移除/展开全部/折叠全部/筛选框）与右键上下文菜单
    （缩放至图层/导出图层…/移除图层）。
  - **ArcGIS Pro 式功能区三段式**：QAT 快速访问工具栏（品牌标 + 保存工程 +
    撤销/重做占位 + 主题切换）+ 页签行 + 命令组行；组名在组宽内居中（ArcGIS 版式）。
  - **联系邮箱**：全项目统一为 daomingyuan@qq.com。

### 既有新增条目

- **kanyu-shell v0.4**（design-review 技能驱动的设计迭代）：
  - **目录面板（Catalog）**：ArcGIS Pro 目录窗格——快捷位置（桌面/文档/下载/
    项目目录/磁盘根）、面包屑、目录优先条目（仅数据文件含 .kdb/.kyu），
    双击进入目录/打开数据为图层；与图层面板职责分离（目录找数据、图层管现场）。
  - **左侧停靠区双页签**：目录 | 图层（各自保活）。
  - **Ribbon 版式修复**：图标/标题/简介三分离（简介仅悬停浮现卡），
    68×56 大按钮版式；组间节奏系统化（按钮 2px/组分隔 8+线+8/组名贴组左对齐/
    右端留白）；修复组名横跨整个窗口的定位错误。
  - **设计审查规范入档**：design-review 技能沉淀进 `ui_kit/mod.rs`
    （层级/间距/文本/色彩/交互/三分离/AI slop 黑名单）与 AGENTS.md #8。

### 变更

- **删除右侧属性面板**（v0.3 引入）：后续按新定制要求重建；
  技能清单输出通道保留在「技能 → 技能清单」（终端）。

### 既有新增条目

- **堪舆数据库（KanyuDB，`.kdb`）**：自研存档格式（裁决 #19）——Arrow IPC 容器 +
  `kanyu.*` schema 元数据（format/version/producer），几何列 `geoarrow.wkb` 扩展；
  与内存 RecordBatch 同构直通，**类型保真**（不经 GeoJSON 中间层），任何 Arrow
  工具链（pyarrow/DuckDB/Polars）可读。`Layer::to_kdb_bytes` / `Layer::from_batch` /
  `kdb::batch_to_kdb` / `kdb::kdb_to_batch`；注册表第 18 种格式（read/write/edit/symbol
  Full）；CLI/MCP `export -f kdb` 与 `load .kdb` 接入全格式转换矩阵。
- **堪舆工程（`.kyu`）**：JSON 工程清单（裁决 #19）——项目元数据/图层路径引用/
  可见性/视口/地图色彩模式；版本校验（高版本拒绝并提示升级）；壳层
  「主页 → 打开工程…/保存工程」落地；无来源内存图层不入工程并明示。
  `kanyu_core::project::{KanyuProject, ProjectLayer}`。
- **kanyu-shell v0.3**（ArcGIS Pro 式深度升级）：
  - **Ribbon 图标大按钮**：总规 §1.4 线性图标系统（`ui_kit::icons`，33 枚
    stroke 1.5px 几何极简图标，egui painter 直绘）+「图标+文字+功能介绍卡」
    组合按钮（`ui_kit::ribbon_button`，悬停介绍卡）；命令组再细分。
  - **Contents 骨架目录**（弃卡片式）：根→图层节点（展开箭头+几何图例色块+
    行尾可见性/缩放/移除图标按钮+选中联动）→几何/字段/格式子节点（`ui_kit::tree_row`）。
  - **底部双页签停靠区**：终端 | AI 对话 同级别切换（`ui_kit::tab_strip`）。
  - **AI 对话面板**（BitFun 式驱动与设置）：`ai.rs`——可插拔 `AiDriver`：
    LocalDriver（离线规则引擎：自然语言→缓冲/打开/图层/度量/导出/帮助，
    意图解析纯函数可测）+ OpenAiDriver（OpenAI 兼容端点，ureq/rustls；
    数据现场系统提示注入）；设置弹窗（驱动/端点/密钥/模型）持久化
    `%APPDATA%/kanyu/shell_ai.json`。
  - **地图色彩解耦**：`MapThemeMode`（固定晨山默认/固定夜观星/跟随界面），
    界面主题切换不再影响地图与制图输出，状态栏显示当前模式，
    随 .kyu 工程序列化。

### 既有条目（v0.2 周期）

- **kanyu-shell**（新 crate）：egui 桌面 UI MVP（总规 §2.1 壳-层-格）——
  eframe/wgpu 原生窗口（标题"堪舆 Kanyu"，1280×800 初始 / 800×500 最小，
  窗口图标 assets/logo-256.png 编译期嵌入）；TitleBar（40px：品牌+当前
  文件名 / 打开数据（rfd，11 格式过滤器）/ 主题切换 / 面板折叠）+
  图层面板（SidePanel 260px：可见性勾选、格式、要素数、几何类型、字段
  清单）+ MapCanvas（`render_png` 显式视口 → egui 纹理，状态变化重渲；
  滚轮光标锚点缩放 + 左键拖拽平移，`view.rs` 纯函数视图数学驱动
  `RenderOptions.viewport`）+ StatusBar（28px：鼠标数据坐标、可见要素
  总数、版本号）；晨山/夜观星双主题（§1.2 色板 → egui Visuals，渲染
  主题联动）；拖文件入窗打开；打开失败中文错误模态框；空状态中央引导；
  系统 CJK 字体运行时注入（egui 默认字体不含中文）。
  截图验证模式：`kanyu-shell --screenshot <out.png> [--load <file>]
  [--theme dark] [--delay <秒>]`——egui `ViewportCommand::Screenshot` →
  `Event::Screenshot` 原生管线截取真实窗口内容（含 TitleBar/面板/画布/
  状态栏）落盘后退出。
- **kanyu-render**：`RenderOptions.viewport: Option<[f64; 4]>` 显式视口
  （`[minx, miny, maxx, maxy]` 数据坐标；给出时跳过集合 bbox 自动适配，
  交互壳层的缩放/平移即经此每帧传入）；`collection_extent` 公开
  （壳层设定初始视口）；非有限/倒置视口报中文错误。
- **kanyu-shell v0.2 深度 UI 改建**（借鉴 ArcGIS Pro 分类设计与 bitfun 卡片视觉）：
  - **`ui_kit` 设计系统**：tokens（间距/圆角/控件高/七级文本）+ controls
    （KButton 四变体/KIconButton/KTextInput/KCombo/KCheckbox）+ containers
    （KCard/KSectionHeader/KDialogShell/KBadge）；铁律入 AGENTS.md：先查后用、
    无则按类新建、样式不出库。
  - **Ribbon 功能区**（ArcGIS Pro 分类方式）：主页/数据/分析/制图/视图/技能/帮助
    七页签 + 命令组，全部操作可达。
  - **独立终端**（ArcGIS Python 窗口理念）：命令直达内核（load/layers/info/
    query/buffer/measure/topology/reproject/export/fit/theme/clear/help），
    与界面共享数据现场——终端产出的图层即刻入图层树；历史导航 ↑↓。
  - **可停靠面板**：Contents 图层树（可见性/缩放/移除/选中联动）、
    属性/技能右面板、状态栏（坐标/视口宽/要素数/版本）。
  - **对话框**（地理处理窗格模式）：查询/导出/投影/缓冲/叠加/连接/分区统计/
    度量/渲染设置/地图导出/运行技能，全部 kit 组件组合。
  - **技能入壳**：WASM 技能热加载/清单/在图层上运行（kanyu-skill 接入 UI）。
- **kanyu-core**：`Layer::from_collection`（分析/查询/技能产出登记为内存图层，
  format 记为 "memory"）。

## [0.14.0] - 2026-08-03

**DWG 覆盖率二轮提升：标注要素化 + 椭圆近似（再吃 ~26% 实体）。**

### 新增

- **kanyu-core**：DWG 覆盖率二轮提升——TEXT/MTEXT 标注要素化 + ELLIPSE 近似：
  - TEXT/MTEXT → 标注要素（插入点 Point；`feature_kind: "annotation"`
    属性供消费者过滤、几何图层语义不被污染（文档即契约）；`text`
    经 decode_dwg_string + `clean_mtext` 最小清洗（`\P`→换行、`~`/`\~`→
    空格、`\\`→`\`、`{...}` 分组去括号保内容、`\f..\H..\W..\A..\C..\Q..\T..\X`
    样式参数码丢弃、`\S上/下;` 堆叠保留）；`height`/`rotation`（弧度→度）；
    空文本计 degenerate。fixture 标注命中数与 spike text 计数精确一致
    （sample_r2000：645/645，a16_test：2194/2194）；
  - ELLIPSE → 64 段参数方程近似（`ellipse_to_positions` pure fn：
    P(t)=C+R(α)·(a·cos t, b·sin t)，acadrust 弧度制；全角→闭合 Polygon、
    部分弧→LineString；ratio≤0/轴长≤0 计 degenerate）；
  - skipped_by_type 现仅含 INSERT/HATCH/SPLINE/DIMENSION 系/其余，
    `foreign_members["kanyu:dwg"]` 结构不变；format.rs dwg note 更新。

## [0.13.0] - 2026-08-03

**DWG 原生读取：acadrust + 自持补丁层（143 真实样本 / 52 万实体验证）。**

### 新增

- **kanyu-core**：DWG 原生读取进内核（新模块 `dwg`，acadrust 0.4 +
  自持补丁层，spike 定稿路线）——
  - **AC15 定位 workaround**：acadrust 0.4.1 的 objects 定位在
    "AuxHeader 位于 Handles 之后"的合法 R2000 布局下为负导致静默空文档
    （spike：143/143 AC1015 真实样本全中）；本层按 ODA 约定以
    `[Classes_end, Handles_start)` 推断 objects 段，直接驱动 acadrust
    底层 pub API（handle_reader/object_reader/DwgDocumentBuilder），
    AC15 系（R13/R14/R2000）直接走本层，其他版本走原生
    `DwgReader::read`（空文档回退）；
  - **编码层**：`decode_dwg_string` 修复两种乱码形态——GBK 字节
    Latin-1 展开（按 codepage 转码，GBK 兜底）与 MIF `\U+XXXX` 未解码；
  - 实体映射（z 丢弃）：POINT/LINE/LWPOLYLINE/POLYLINE（2D/3D，闭合→
    Polygon）/CIRCLE（64 段面）/ARC（64 段线，弧度）；ELLIPSE/SPLINE 与
    INSERT/HATCH/MTEXT/TEXT/DIMENSION 系跳过+按类型计数（标注层 📋）；
    要素带解码后 `layer` 属性；退化几何（dxf 同口径）单独计数；
  - `DwgStats`（version/skipped_by_type/degenerate）写入
    `foreign_members["kanyu:dwg"]`（与 buffer skipped 上报同模式）。
  format.rs dwg 条目 driver libredwg-wasm→acadrust（read 保持 Full、
  note 写明实体级 Partial 语义）。fixture 为用户自有
  `atlas__A001__1.dwg`（231KB，R2000 真实图纸，仅测试用途）。
- **kanyu-core / CLI / MCP**：KMZ 支持（kml 的 zip 容器，zip crate
  default-features=false + deflate 纯 Rust 后端）——`.kmz` 内存解包
  （doc.kml 优先、否则首个 .kml 条目，多 KML 条目取首个注释即契约），
  zip 损坏/无 .kml 条目中文结构化错误；`Layer::to_kmz_bytes`
  （doc.kml 单条目 deflate）。CLI/MCP export `-f kmz` 分流
  （kmz 为 kml 容器变体非独立格式条目，按 format 值区分 kml/kmz）。
  format.rs kml 条目 note：KMZ 📋→✅。
- **kanyu-core**：xlsx 读取（calamine 0.36，纯 Rust）——首个 worksheet
  表头行 + 数据行；行记录→Feature 逻辑泛化为 `rows_to_collection`
  （CellValue 中间表示：原生 Number/Bool 保真，文本按 CSV 同款规则数值化），
  CSV 与 xlsx 两路共用零复制；空表/空工作簿/无坐标列中文错误。
  范围：只读（写出 📋）；format.rs csv 条目 note 更新。
  测试 fixture 由 rust_xlsxwriter（dev-dependency）可复现生成。
- **kanyu-core / CLI / MCP**：Shapefile 写出（关闭 write: Partial 能力缺口，
  format.rs write: Partial→Full）——`Layer::write_shp(collection, base)`
  写 base.shp/.shx/.dbf 三件套；单一几何类型校验（GeometryCollection
  展平参与判定；混合几何中文错误提示先 data query 拆分，注释即契约）；
  Point/MultiPoint/LineString/MultiLineString（→Polyline）/Polygon/
  MultiPolygon（外环+洞，`with_rings` 自动闭合整向）；属性 dbase 字段名
  10 字节截断（按字符边界不断 UTF-8，冲突加 `_N` 序号）、
  String→Character(254 截断)、整数→Numeric(18,0)、浮点→Numeric(18,6)、
  Bool→Logical，空值跳过；导出侧 out 去扩展名作 base。

## [0.12.0] - 2026-08-03

**AI 代理远程热加载技能：kanyu_system_hotload 实质化 + skill_run/skill_list。**

### 新增

- **kanyu-mcp**：MCP 技能热加载接线（`kanyu_system_hotload` 从 planned
  变为真实工具，AI 代理可远程加载并执行 WASM 技能）——
  KanyuServer 持有技能注册表（内存态 `Arc<Mutex<GeneRegistryState>>`，
  Clone 共享，与 TaskManager 同模式；重启即丢）：
  - `kanyu_system_hotload(wasm_path)`：编译校验 + 实例化 + 元数据校验
    （hotload 即"验证"职责，**校验失败绝不注册**），返回 skill_id/meta，
    重名覆盖并返回 `replaced: true`；
  - `kanyu_skill_run(skill_id, path)`：已注册技能在数据文件上沙箱执行
    （FeatureCollection 进/出），未知 skill_id 中文错误提示先 hotload；
    加入任务化白名单（`task: true` 可异步执行，与分析工具同待遇）；
  - `kanyu_skill_list()`：注册表快照（skill_id/version/capabilities）。
  introspect：`kanyu_system_hotload` planned→stable；新增 gene 组
  （`kanyu_skill_run`/`kanyu_skill_list` stable）。v0.1 技能调用锁内串行化
  （注释即契约；按名细粒度锁 📋）。

## [0.11.0] - 2026-08-03

**Phase 5 魂启幕：WASM 技能系统宿主（wasmtime + WIT 组件模型，fuel 沙箱）。**

### 新增

- **kanyu-skill / CLI**：Phase 5「魂」启幕——WASM 技能系统宿主（总规 §4.5
  "以 WASM 为技能"落地，新 crate `kanyu-skill`）：wasmtime 47 组件模型
  + WIT 强类型 ABI（`wit/skill.wit`：`meta() -> string`、
  `run(string) -> result<string, string>`，FeatureCollection JSON 进/出）；
  沙箱无 WASI 导入（纯计算）+ fuel 配额（10 亿/次执行，耗尽即 trap；
  无 IO 挂起故不设墙钟超时，注释即契约）。`SkillHost::load`（编译校验 +
  实例化 + meta() 元数据校验）与 `run`（每次执行重置 fuel），
  LoadFailed/MetaInvalid/Trap/Timeout/ResultInvalid 五类中文结构化错误。
  样板分析技能 `attr_scaler`（真 Rust guest：wit-bindgen 0.60
  `generate!`/`export!`，height ×2；wasm32-unknown-unknown 核心模块 +
  `wasm-tools component new` 组件化，fixture 提交于 testdata/）。
  CLI 新命令组 `kanyu gene info/run`。
  introspect：kanyu-skill 模块 planned→incubating。
  **MSRV 1.88 → 1.94**（wasmtime 47 要求）。
  MCP 热加载接线（kanyu_system_hotload 实质化）与 libredwg-wasm 技能 📋。

## [0.10.0] - 2026-08-02

**属性驱动符号化：graduated 分级设色 / categorical 分类符号（总规 §3.4 落地）。**

### 新增

- **kanyu-render / CLI / MCP**：属性驱动符号化（总规 §3.4 符号系统第一块，
  裁决 #17：并入 `kanyu_render_map` 的 `style` 参数而非独立工具）——
  `StyleRule` 两型（JSON `type` 判别）：`graduated`（数值字段分档，
  `stops: [[阈值, "#RRGGBB"], …]` 严格升序，取最后满足 值≥阈值 的档，
  恰等阈值取该档、低于首档走默认）与 `categorical`（字符串字段类别映射，
  无匹配取 `default`）。字段缺失/类型不符的要素走主题默认样式（不产生
  脏样式）；样式决策仍集中于 `style_for`/`effective_style` 一处；
  命中色按几何类型派生（面=该色 20% 透明填充+同色描边、线=该色描边、
  点=该色填充）。空 stops/非升序/坏 hex 报中文错误并指出出错项。
  CLI `render map --style/--style-file`（互斥中文报错）；MCP `style`
  参数 JSON 原样透传。introspect：移除 `kanyu_render_symbolize`
  （`kanyu_render_camera` 保持 planned）。无 style 调用行为完全不变。

## [0.9.0] - 2026-08-02

**Phase 2 视界启幕：离屏地图渲染 render_map（PNG/SVG 双通道），AI 代理能"看见"数据。**

### 新增

- **kanyu-render / CLI / MCP**：Phase 2「视界」启幕——离屏地图渲染
  `kanyu_render_map`（新 crate `kanyu-render`，依赖方向更新为
  core ← render ← cli/mcp）：SVG 纯字符串零依赖生成（同时兑现注册表
  svg 导出 write:Full 承诺）、PNG 经 tiny-skia 纯 Rust CPU 光栅化
  （CI 无 GPU 依赖；wgpu 实时管线留给交互壳层 kanyu-shell）。
  色彩取自总规 §1.2：晨山（米白 #F0EDE8/墨黑/远黛青 #2D6A5E）与
  夜观星（极暗 #0D0F12/月白/青玉 #4DB8A8）双主题，点/线/面样式
  集中于 `style_for` 单一事实来源；bbox 等比自适应（y 翻转、居中、
  padding，单点/空集合退化安全）。CLI 新命令组 `kanyu render map`
  （格式按扩展名判定）；MCP `kanyu_render_map`——PNG 时 content 携带
  base64 image/png + 摘要文本，SVG 时携带 SVG 源码，structuredContent
  恒带 feature_count/bbox/尺寸/主题/格式摘要（AI 代理可直接"看见"数据）。
  introspect：kanyu-render 模块 planned→incubating，新增
  `kanyu_render_map`（render 组 stable）。

## [0.8.0] - 2026-08-02

**长任务一等特性：SEP-2663 协议级 MCP tasks，分析工具异步化。Phase 1.5 收官。**

### 新增

- **kanyu-mcp**：长任务能力（总规"MCP tasks 为一等特性"承诺落地）——
  调研确认 rmcp 3.1 服务端已完整实现 **SEP-2663**
  （`io.modelcontextprotocol/tasks`，即原 SEP-1686 的现行编号），
  走**协议级路线**：`ServerCapabilities` 声明 tasks 扩展；白名单分析工具
  （buffer/overlay/sjoin/zonal_stats/topology）的 `tools/call` arguments 带
  `"task": true` 时任务化执行（rmcp TaskManager spawn，阻塞内核调用进
  blocking 线程池），立即返回 `resultType:"task"` 任务句柄；客户端经协议方法
  `tasks/get`（轮询）/ `tasks/cancel`（协作取消）/ `tasks/update` 驱动
  生命周期。任务结果保留 10 分钟（TaskManager TTL 惰性清扫，内存态、
  重启即丢）。5 个分析工具的内核调用提取为同步函数供同步/任务两路径共享；
  stdio 与 HTTP 两传输同权。未知 taskId / 非白名单工具返回结构化/中文错误。

## [0.7.0] - 2026-08-02

**远程 AI 代理接入：MCP streamable HTTP 传输。**

### 新增

- **kanyu-mcp / CLI**：MCP streamable HTTP 传输（裁决 #11：官方 streamable
  HTTP 已取代旧 SSE；裁决 #16 第二承诺）——`kanyu mcp serve --transport http
  --port <port>` 绑定 `127.0.0.1`，endpoint `/mcp`（POST=JSON-RPC、
  GET=SSE 流、DELETE=会话终止；`Mcp-Session-Id` 头 + LocalSessionManager
  内存会话存储；service factory 模式每会话一个无状态 KanyuServer 实例）。
  ⚠️ 无鉴权/TLS（📋），远程暴露需自行加反代与鉴权（eprintln 与文档双警示）；
  MCP tasks（SEP-1686 长任务）📋。

### 变更（Breaking）

- **CLI**：`kanyu mcp serve --transport` 的取值 `sse` 改为 `http`
  （旧 `sse` 值不再接受，clap 直接报无效值；此前 `sse` 仅返回
  "待 v0.2" 错误提示，无实际行为损失）。

## [0.6.0] - 2026-08-02

**矢量分析面补全：空间连接 sjoin 与分区统计 zonal_stats。**

### 新增

- **kanyu-core / CLI / MCP**：空间连接与分区统计（补全矢量分析面）——
  - `analysis::sjoin`：空间连接（`SpatialPredicate::Intersects/Contains/Within`；
    **左连接 + 匹配展开**：保留全部 target 要素、无匹配 join 侧缺省、
    一对多各输出一条；属性合并、键冲突加 `join_` 前缀并附 `join_index`
    溯源；O(n·m)，rstar 加速 📋）。
  - `analysis::zonal_stats`：分区统计（`ZonalStat::Count/Sum/Mean/Min/Max`；
    values 按质心/代表点归属 zones 面要素、一值多区取首个匹配、区外值
    计入 `foreign_members.unzoned_count`；统计列命名 `{field}_{stat}`；
    count 同样要求 field 存在且数值）。
  - CLI：`kanyu analysis sjoin/zonal`；MCP：`kanyu_analysis_sjoin`、
    `kanyu_analysis_zonal_stats`（introspect 新增两项 stable）。

## [0.5.0] - 2026-08-02

**米制分析链路打通：投影变换（proj4rs + EPSG 数据库）与测地线度量。**

### 新增

- **kanyu-core / CLI / MCP**：坐标投影工具 + 测地线度量（新模块 `crs`，
  解决 EPSG:4326 数据无法做米制分析的痛点）——
  - `crs::reproject`：投影变换（proj4rs + crs-definitions 内置 EPSG 数据库；
    `"EPSG:xxxx"`/proj4 定义串/`WGS84` 均接受；逐坐标递归转换全几何类型，
    经纬度自动衔接度/弧度，z 不变；失败坐标报中文错误并指出要素序号）。
  - `crs::measure`：测地线度量（geo crate Karney 2013；`MeasureKind::Length`
    线长与面外环周长（米）、`MeasureKind::Area` 面面积（平方米，含洞扣除，
    `Orient` 归一化绕向防 ESRI 顺时针数据算出补集面积）；输出
    total + 逐要素明细 JSON；Point/无几何为 0）。
  - CLI：`kanyu data reproject`、`kanyu analysis measure`；
    MCP：`kanyu_data_reproject`、`kanyu_analysis_measure`
    （introspect 新增两项 stable）。
  - buffer/measure 文档中的"需先投影"警示句统一指向 reproject 工具。

## [0.4.0] - 2026-08-02

**分析内核落地：总规 §4.2.2 的 MCP 分析工具组兑现（裁决 #16 调序前置）。**

### 新增

- **kanyu-core / CLI / MCP**：空间分析工具组（geo crate，总规 §4.2.2 落地，
  裁决 #16 分析内核优先于 UI 壳层）——
  - `analysis::buffer`：缓冲区分析（圆角连接/端帽，segments 控制每象限
    分段数；属性随行；不可转换要素跳过并计入 `foreign_members.skipped`）。
  - `analysis::overlay`：叠加分析（BooleanOps/i_overlay：
    union/intersection/difference/xor；仅面要素，非面报错指出序号；
    target×overlay 逐要素对布尔、未做跨对融合；属性合并、键冲突加
    `overlay_` 前缀；Difference 为连续差、仅带 target 属性）。
  - `analysis::topology_check`：拓扑检查（NoOverlap：面要素两两交集
    面积 > 1e-10 判违规；`TopologyReport` 结构化报告；O(n²) 朴素实现，
    rstar 加速 📋）。
  - CLI 新命令组 `kanyu analysis buffer/overlay/topology`；
    MCP 新工具 `kanyu_analysis_buffer/overlay/topology`（introspect
    三项状态 planned→stable）。
  - 单位警示：distance/面积均为数据 CRS 单位，EPSG:4326 下是度；
    米制分析需先投影（proj4rs 📋）。

## [0.3.0] - 2026-08-02

**GeoArrow 成为内核血液：`Layer` 内存模型迁移至 RecordBatch 列式零拷贝。**

### 新增

- **kanyu-core**：`Layer::batch()` 零拷贝访问底层 GeoArrow RecordBatch
  （WKB 几何列 `geometry` 携带 geoarrow.wkb 扩展元数据 + 类型化属性列
  Int64/Float64/Boolean/Utf8，arrow 58 / geoarrow-schema 0.8）。
- **kanyu-core**：`summary()`/`query()` 改为直接在 RecordBatch 列上求值
  （几何类型读 WKB 头部类型码；谓词逐行求值后经 arrow take 取子集），
  谓词语义与 GeoJSON 载体时代逐比特一致。

### 变更（Breaking）

- **kanyu-core**：`Layer` 内部载体从 `geojson::FeatureCollection` 迁移为
  GeoArrow `RecordBatch`（总规"以 GeoArrow 为血液"落地）。格式解析器在边界
  统一 `collection_to_batch` 入列，导出时 `batch_to_collection` 转回——
  全部格式 I/O 代码零改动，对外行为不变（36 项既有测试全绿）。
  **Breaking**：`Layer::collection()` 签名由
  `fn collection(&self) -> &geojson::FeatureCollection`（只读借用）改为
  `fn collection(&self) -> geojson::FeatureCollection`（按需转换的拥有值）；
  调用方传引用处需加 `&`（CLI/MCP 已适配）。

## [0.2.0] - 2026-08-02

**Phase 1 里程碑：主流矢量文件格式免 GDAL 读写全部达成。**

### 新增

- **kanyu-core / CLI / MCP**：KML 原生读写（kml crate，免 GDAL）——
  Document/Folder 嵌套展平取全部 Placemark，Point/LineString/LinearRing/
  Polygon（含内环洞）/MultiGeometry（同类合并 Multi*、异类 GeometryCollection）
  映射；name/description/ExtendedData（Data/SimpleData/SchemaData）写为同名属性
  （值按 CSV 规则数值化）。写出全六类型（Multi*→MultiGeometry），属性除
  name/description 外入 ExtendedData/SimpleData。KMZ（zip 容器）返回待集成
  结构化错误。`Layer::to_kml_string`、CLI/MCP export `-f kml` 可用。
  至此 Phase 1 主流矢量文件格式（shp/geojson/fgb/geoparquet/dxf/kml/csv）
  免 GDAL 读写全部达成；gpkg/spatialite/postgis/wfs 按总规裁决 #15
  走可选 feature 插件路线。
- **kanyu-core / CLI / MCP**：DXF 原生读写（dxf crate，免 GDAL）——
  POINT/LINE/LWPOLYLINE/POLYLINE 映射（闭合折线→Polygon），CIRCLE/ARC 按 64 分段
  折线近似，其余实体跳过不报错；要素带 `layer`（图层名）与非 ByLayer 时的
  `color_index`（ACI）属性。写出为 R2000 LWPOLYLINE（Polygon 仅外环、洞舍弃、
  属性/XDATA 📋、z 丢弃），统一落图层 "0"。`Layer::to_dxf_string`、
  CLI/MCP export `-f dxf` 可用。
- **kanyu-core / CLI / MCP**：GeoParquet 原生读写（geoparquet crate，免 GDAL）——
  几何列按 GeoParquet 1.x 规范 WKB 编码（列名 `geometry`，自研小端 2D WKB
  编解码，六类基础几何 + GeometryCollection 可往返），geo 元数据/geometry_types/
  bbox 由 geoparquet crate 编码器生成；属性列 schema 推断规则与 FGB 一致，
  Arrow 列类型原生映射 JSON 类型（不支持的列类型读侧报中文错误，拒绝静默丢列）。
  `Layer::to_geoparquet_bytes`、CLI/MCP export `-f geoparquet` 可用。
- **kanyu-core / CLI / MCP**：FlatGeobuf 原生读写（flatgeobuf crate，免 GDAL）——
  读取逐要素转换（几何经 geozero `ToJson`、属性经自研 PropertyProcessor），
  FGB 列类型原生映射为 JSON 类型；
  写出列 schema 自动推断（String→String、整数→Long、浮点→Double、Bool→Bool，
  混合类型列退化为 String），单一几何类型按声明写出、混合几何按 Unknown 异构声明，
  Hilbert 空间索引 + EPSG:4326 CRS 声明。`Layer::to_fgb_bytes`、
  CLI/MCP export `-f fgb` 可用。
- **kanyu-core**：Shapefile 原生读取（shapefile crate，免 GDAL）——Point/MultiPoint/
  Polyline/Polygon（含 M/Z 变体与带洞多边形），dbase 属性类型化为 JSON
  （Numeric/Float→Number、Character→String、Logical→Bool、Date/DateTime/Memo→String、
  空值跳过）；缺少 .dbf 侧边文件时返回中文结构化错误。写出待 Phase 1 后续。
- **kanyu-core**：CSV/TSV 原生加载，坐标列自动识别（lon/lat、longitude/latitude、
  x/y、经度/纬度），其余列自动作为属性（数值单元格转为 JSON 数值）。
  xlsx 暂返回结构化错误提示（待 calamine 集成）。
- **kanyu-core / CLI / MCP**：CSV 导出（`x,y` 坐标列 + 属性字段并集），
  加载→查询→导出 CSV 闭环。

### 修正

- `kanyu introspect` 工具清单改为真实 MCP 表面名（`kanyu_data_load` 等），
  并补登 agents 组两个已实现工具——自省必须描述现实。
- MCP `ServerInfo` 上报为 `kanyu-mcp` + 内核版本（此前为 SDK 默认值）。
- DWG 能力矩阵诚实化：write/edit 降为 Partial，driver 改为 `libredwg-wasm`，
  note 对齐总规 §6.1 裁决（沙箱只读、写 ≤r2004、现代版本以 DXF 替代）。
- `agents validate --json` 输出改为 pretty JSON，与其他 `--json` 命令一致。
- workspace MSRV 声明修正为 1.88（与 rmcp 3.x 要求一致）。
- 文档：新增 ARCHITECTURE/API/SDK/MCP/CLI 五份接口文档；
  README 架构图补 kanyu-shell 模块。

## [0.1.0] - 2026-08-01

首个公开版本：堪舆的地基。

### 新增

- **kanyu-core**：统一格式注册表（17 种格式的读/写/编辑/符号/布局能力矩阵）、
  图层模型（GeoJSON 原生加载、`LayerSummary` 概要、属性谓词查询
  `field op value`）、`AGENTS.md` 语义文件解析/校验/模板生成、系统自省报告。
- **kanyu-cli**：`kanyu data info/load/query/export`、`kanyu introspect`、
  `kanyu agents init/validate`、`kanyu mcp serve`，全局 `--json` 输出。
- **kanyu-mcp**：基于官方 `rmcp` 3.x 的 stdio MCP Server，6 个确定性工具
  （`kanyu_data_load/query/export`、`kanyu_system_introspect`、
  `kanyu_agents_init/validate`），结构化 JSON 输出。
- **文档**：总规、架构、API、SDK、MCP、CLI 全套文档；开源社区文件齐备。
- 双许可 MIT OR Apache-2.0。

### 已知限制

- 原生 I/O 仅 GeoJSON；FlatGeobuf/GeoParquet/Shapefile/DXF 在 Phase 1 后续迭代。
- 分析/渲染/编辑工具组（buffer/overlay/topology/render）为规划状态。
- MCP 仅 stdio 传输；streamable HTTP 与 MCP tasks 长任务待 v0.2。

[Unreleased]: https://github.com/DaoMingyuan/Kanyu/compare/v0.15.0...HEAD
[0.15.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.15.0
[0.14.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.14.0
[0.13.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.13.0
[0.12.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.12.0
[0.11.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.11.0
[0.10.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.10.0
[0.9.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.9.0
[0.8.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.8.0
[0.7.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.7.0
[0.6.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.6.0
[0.5.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.5.0
[0.4.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.4.0
[0.3.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.3.0
[0.2.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.2.0
[0.1.0]: https://github.com/DaoMingyuan/Kanyu/releases/tag/v0.1.0
