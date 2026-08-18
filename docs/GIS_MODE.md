# dsh GIS 模式（kanyu-gis preset）使用手册

> 本手册是 [AI_SYNC.md](../AI_SYNC.md) 会签簿「GIS 模式 / dsh 组件移植」系列回记的实现文档：
> 堪舆 GIS × DeepSeek Harness（DSH / Cordis）组件与 `kanyu-gis` agent preset 如何加载、
> 如何运行、与 kanyu 仓库如何联动。读者：接手本仓库的维护者与 AI 代理。
>
> **仓库内单一事实来源是 [`dsh/`](../dsh/) 目录**（`git ls-files dsh/`）；
> 组件安装位于 `~/.dsh/.agent-presets/kanyu-gis/`（用户私有安装区，DSH 升级不覆盖）。
> 两处的同步义务见末节「维护契约」。

## 1. 组件是什么

`kanyu-gis` 组件把堪舆七大 GIS 能力移植进 DSH 会话，分两个半区：

- **Host 半**（[`dsh/plugin/host.js`](../dsh/plugin/host.js)，宿主进程侧）：以 `kanyu` CLI
  为执行后端，向 Client 半提供 Package 私有 JSON RPC（`harness.handle`），并向 DSH 模型
  注册 8 个动态工具（`harness.registerTool`）——堪舆原壳层 LocalDriver/OpenAiDriver
  意图面在 Harness function-calling 代理循环中的整合形态：自然语言 → 工具调用由
  Harness 模型驱动，组件只暴露语义对齐内核注册表的工具面。
- **Client 半**（[`dsh/plugin/client.js`](../dsh/plugin/client.js)，浏览器侧）：DSH Web GUI
  「堪舆 GIS 工作台」——会话头部「🧭 堪舆GIS」按钮 + 全局浮层七页签
  （目录/数据/地图/坐标/处理/编辑/3D/关于）+ cordis 卡片，全部经 `host.call` 走 Host 半。

七大能力域与内核落点：

| 能力 | 组件工具 | kanyu 侧对应物 |
|------|----------|----------------|
| 地图面板 | `kanyu_render` | `kanyu render map`（晨山/夜观星，PNG/SVG） |
| GIS 数据目录读取 | `kanyu_catalog` / `kanyu_data` | `kanyu data info/query/validate` + `format.rs` 格式注册表 |
| 坐标框架 | `kanyu_crs` | `kanyu data reproject`（EPSG 全库）+ 常用坐标系速查表 |
| 工程目录 | Client 目录页签 | `catalog.list` RPC（递归扫描，扩展名矩阵对齐注册表） |
| 地理处理 | `kanyu_geoprocess` | `kanyu analysis` 13 工具（QGIS 语义，参数名逐旗标对拍 v0.22.0 实测） |
| 地理编辑 | `kanyu_edit` | 组件内 GeoJSON 在线编辑内核（6 算子 + undo/redo 双栈）；深度拓扑编辑由 `kanyu-edit` crate 承接 |
| 3D 地理 | `kanyu_scene3d` | 挤出体场景数据制备 + Client canvas 软件 3D 管线（对齐 scene3d.rs：yaw/pitch 拖拽旋转/背面剔除/质心纵深排序/两档明暗） |

另有 `kanyu_introspect`（系统自省，对齐 `kanyu introspect --json`）。工具参数与输出
直接引用内核注册表语义——「工具语义 == 内核语义」，无第二套参数表。

## 2. 如何加载与运行

**仓库内 preset 源 → 本机安装区**（同步脚本即权威机制）：

```bash
bash dsh/sync-preset.sh
```

脚本把 `dsh/presets/kanyu-gis/` 全量复制到 `~/.dsh/.agent-presets/kanyu-gis/`
（先清空防残留），随后用 `dsh/tools/verify_preset.mjs` 做旁路可加载性校验
（与 DSH 发现库同判定链：js-yaml `entryListSchema` 解析 + `readPresetMetadata`）。

> 注：早期版本文档曾写 `kanyu dsh --preset ... --workdir ...` 同步命令——
> kanyu CLI **没有** dsh 子命令（`crates/kanyu-cli/src` 无此定义），该命令线不可复现，
> 已于 2026-08-18 更正为上述 `sync-preset.sh`。

**新开 kanyu-gis 会话**（preset 落位后，2026-08-18 活体实证的两条途径）：

- **Web UI**：`dsh web` 启动后在新建会话处选择 preset「kanyu-gis」。
- **API**：`POST /api/agentPreset.list` 确认 roster 中 `kanyu-gis` 无 `broken` 标记，
  再 `POST /api/session.create`（payload 含 `cwd` + `agentPreset: "kanyu-gis"`）；
  会话技能目录经 `POST /api/skill.list` 应见 `kanyu-gis` 领域技能入目。

> 注：早期版本文档曾写 `kanyu dsh --preset ... --workdir ...` 同步命令——
> kanyu CLI **没有** dsh 子命令（`crates/kanyu-cli/src` 无此定义），该命令线不可复现，
> 已于 2026-08-18 更正为上述 `sync-preset.sh`。旧版另写的 `dsh run --preset` /
> `cordis_mount` 挂载命令线亦未经实证，以上述两条实证途径为准。

## 3. 与 kanyu 仓库的联动机制

1. **命令面**：组件 8 个工具全部**经 PATH 上的 `kanyu` CLI 执行**（`kanyu introspect` /
   `kanyu data ...` / `kanyu render map` / `kanyu analysis ...`），与 `kanyu-mcp` 服务
   同一落点；组件不依赖宿主 `node_modules`——DSH 升级、npx 缓存清理均不影响组件工具。
   找不到 `kanyu.exe` 时，`kanyu-mcp`（MCP stdio 入口，同批安装）兜底。
2. **语义面**：`tooldef` 参数类型、`format.rs` 格式矩阵、`crs` 全库是参数的单一事实来源。
3. **GIS 模式 preset**（[`dsh/presets/kanyu-gis/`](../dsh/presets/kanyu-gis/)）：
   `agent.cordis.yml` 代理平面组合（persona 身份层 + shell/fs/jobs/skill/goal 工具面 +
   plan-mode / compaction / delegation 三个 isolate 组 + skill-filesystem 领域技能注入；
   形态逐行沿用 local-hybrid 方言——模型路由、fs/沙箱、subagents 注册表皆属宿主平面，
   preset 不得声明）+ `preset.yml` 发现元数据 +
   `skills/kanyu-gis/SKILL.md` 领域技能（七域能力地图 + 四道门禁 + 仓库约定边界）。
4. **会签面**：组件迭代在 [AI_SYNC.md](../AI_SYNC.md) 会签簿登记；自我迭代只发生在
   Git 协作层（提交/PR + CI），运行时绝不自改内核（AI_SYNC §1.3）。

## 4. 当前状态（2026-08-18，第六十四轮）

- **feature-add 画布化（本轮，绘制点/线/面新要素进画布）**：双端 client.js
  编辑画布绘制模式扩三种——绘制点单击即成 feature-add Point，绘制线 ≥2 点 /
  绘制面 ≥3 点（自动闭合）攒点应用 feature-add LineString/Polygon；复用
  drawRef/drawOverlay/afterEdit 骨架。测试器 185/185（static 149/149），
  生产桥 feature-add Polygon 实测通过。注：六十三轮双仓 CI
  （65e2f61 / 31251f9）均 success。
- **挖洞/打断画布交互（第六十三轮，两算子进编辑画布）**：双端 client.js 顶点编辑
  画布新增「绘制挖洞 / 点选打断」模式——挖洞逐点攒环（覆盖层 ≥3 点预闭合
  预览，应用写 hole-add），打断单击落点即 line-split；目标要素=属性表选中
  行否则 #0；afterEdit 联动刷新与 vUp 同语义。测试器 183/183
  （static 147/147）。注：六十二轮双仓 CI（4846677 / a24a543）均 success。
- **顶点画布拓扑模式开关（第六十二轮，Map Topology 语义进画布）**：双端 client.js
  顶点编辑区新增拓扑模式复选框——开启后拖拽顶点松开写 topo-move（被拖顶点
  原坐标精确匹配，共享顶点一次同移），关闭保持 vertex-move 单点移动；两路
  均入 undo 栈一次撤销。测试器 181/181（static 145/145）。注：六十一轮
  双仓 CI（f2a7f8b / 2e3047a）均 success。
- **编辑页签算子清单同步（第六十一轮，新算子进工作台 UI）**：双端 client.js 编辑页签
  OPS 下拉 6→11 算子 + HINTS 逐算子参数示例（ringPath 分派/挖洞闭合/打断吸附/
  拓扑精确匹配语义注记）；容量提示 64→100 跟随内核对齐。测试器 179/179
  （static 143/143），生产桥 edit.ops 11 算子实测。注：六十轮双仓 CI
  （8192596 / 8e0b3bb）均 success。
- **共享顶点拓扑编辑移植（第六十轮，编辑算子盘点表收官）**：EDIT_OPS 10→11 新增
  topo-move（对齐 kanyu-edit move_shared_vertex，Map Topology 语义）——坐标
  精确相等一次移动全部共享顶点（含环闭合首末点多处出现），自逆坐标对换。
  测试器 177/177（static 141/141），3080 生产桥 3 处命中 + 撤销闭环实测通过。
  至此盘点表全部落地：6 原始算子 + 五件移植（feature-move/hole-add/
  attributes-replace/line-split/topo-move）+ vertex-move 双修复，面切割留内核侧。
- **线打断移植（第五十九轮，kanyu-edit split_line_at_point → 组件）**：EDIT_OPS 9→10
  新增 line-split——打断点投影最近线段（t 截断 + 1e-9 吸附顶点），首段就地
  改 + 次段插入（属性复制），逆操作 line-unsplit 合并回原样。**面切割评估
  结论**：split_polygon_by_line 依赖 geo Buffer/BooleanOps 差集 + 碎条剔除，
  无忠实 JS 等价物，组件不移植、留内核侧（未来经 WASM 技能或 CLI 出口接入）。
  测试器 173/173（static 137/137），3080 生产桥打断/撤销闭环实测通过。
  注：五十八轮双仓 CI（aa9a5f8 / 81152cc）均 success。
- **整行属性替换移植（第五十八轮，kanyu-edit UpdateProperties → 组件）**：EDIT_OPS
  8→9 新增 attributes-replace——properties 整体覆写（null 清空属性表），
  自逆算子（undo 恢复旧属性行含 null 态）。测试器 169/169（static 133/133），
  3080 生产桥替换/撤销闭环实测通过。注：五十七轮组件仓 CI（3c5be08）success。
- **挖洞算子移植（第五十七轮，kanyu-edit AddHole → 组件）**：EDIT_OPS 7→8 新增
  hole-add 面内挖洞——ring 未闭合自动闭合 + holeValidate 校验语义完整移植
  （点环关系射线法 + 边界相接判负），part 单面恒 0/多面子面下标，逆操作
  hole-remove 弹出末环。测试器 166/166（static 130/130），3080 生产桥
  挖洞/撤销闭环实测通过。注：五十六轮双仓 CI（293707a / 71292ec）均 success。
- **编辑算子对照盘点补齐（第五十六轮，EDIT_OPS ↔ kanyu-edit 全量比对）**：新增
  feature-move 整要素平移算子（对齐 MoveFeature {index,dx,dy}，递归平移
  任意维度坐标、保留 Z/M、负量逆操作）；vertex-move 两个 bug 级修复——
  ringPath 缺省按几何类型分派（面[0]/多面与多线[0,0]/线与点[]，旧版恒
  [0] 对线/点错误下钻）+ 仅覆写 x/y 保留 Z/M（旧版丢弃高程）；undo 容量
  64→100 对齐 kanyu-edit History 默认。EDIT_OPS 6→7（RPC 仍 31），
  kanyu_edit 工具描述同步补齐分派语义。测试器 162/162（static 126/126），
  3080 生产桥 feature-move 平移/撤销闭环实测通过。
- **目录 .kyu 工程图层接力（第五十五轮，目录→地图联动闭环）**：新增 style.list
  RPC（30→31，.kyu 图层全列 + source 相对工程目录绝对化 + styleMode
  摘要）；双端目录页签 .kyu 条目点击展开图层清单（主色色块对齐壳层
  Contents），图层行点击载入当前图层 + store.sym/kyu/layerId 接力地图
  页签回填符号化表单与写入区。测试器 156/156（static 120/120）。
  3080 生产桥实测通过。SKILL.md v1.5。端点复测仍全部离线。
- **3D 场景符号化着色（第五十四轮）**：`scene3d.data` 新增 symbology 入参——
  逐要素派生 hex 色（categorical 接管 colorField + catColors 映射；
  graduated stops 末档命中；缺字段不着色）；kanyu_scene3d 工具同参；
  双端 3D 页签符号化行（复用 buildSymbology）+ 模型色三级回退 +
  HUD 标注。测试器 152/152（static 117/117）。3080 生产桥实测通过。
  SKILL.md v1.4。端点复测仍全部离线。
- **模型侧符号化同能力（第五十三轮）**：`kanyu_render` 动态工具 schema 新增
  `symbology` 入参（LayerSymbology 编辑模型三模式文档化），地图与 layout
  双分支同款 symToRule 投影（显式 style 优先）——模型侧与面板侧同一
  编辑模型语义，AI 可直接产出 .kyu 持久化格式样式。RPC 仍 30 项。
  测试器 145/145（static 110/110）。3080 生产桥复测通过。SKILL.md v1.3。
  端点复测仍全部离线。
- **图层符号化编辑移植（第五十二轮，壳层图层属性页对齐）**：Host 半
  LayerSymbology→StyleRule 投影（symToRule + 三色带 + F64_MIN 首档）；
  render.map 新增 symbology 入参（回执 styleApplied）；新增 style.get /
  style.set RPC（28→30，.kyu 图层样式读写，写拒绝带 writeHint 指引）；
  双端地图页签符号化区升级三模式编辑模型 + 工程样式读写行（读取回填/
  写入工程）。测试器 141/141（static 108/108）。3080 生产桥三测通过。
  SKILL.md 同步 v1.2。端点复测仍全部离线。
- **SKILL.md 能力面一致性精修（第五十一轮，AI 能力整合优化）**：GIS 模式领域
  技能 `dsh/presets/kanyu-gis/skills/kanyu-gis/SKILL.md` 实况对齐——
  面板侧 RPC 清单 26→28（补 render.layout / catalog.readImage）、
  验证面计数 123→134（static 96→104）并补四项验证列举、metadata
  version 1.0→1.1。verify_preset 可加载 + static 104/104 全绿 +
  sync-preset 回灌完成。纯文档一致性改动。端点复测仍全部离线。
- **目录地图框点击预览（第五十轮，目录五分类可点闭环）**：host 半新增
  `catalog.readImage` RPC（27→28，渲染产物 PNG → base64，越界防护仅限
  dsh/output 产物目录内 .png）；双端目录页签地图框条目点击 → PNG 内嵌
  预览。测试器 134/134（static 104/104）。RPC 28 项。端点复测仍全部离线。
- **布局预览 UI（第四十八轮）**：host 半新增
  `render.layout` RPC（26→27）——layoutPreview 支持 path 直传与
  `kyu + title` 工程模式（布局规格 page/dpi/legend/scalebar/north 取自
  .kyu 清单，数据取首个可见图层、source 相对工程目录解析）；catalog.list
  布局框条目回传 kyu 工程路径；双端目录页签布局框点击 → SVG 内嵌预览
  + 关闭按钮。测试器 129/129（static 100/100）。RPC 27 项。端点复测仍全部离线。
- **verify_preset 校验扩展（第四十九轮）**：行内插件包存在性对照宿主
  检出 + 自带技能 SKILL.md frontmatter 校验（sync-preset.sh 通道覆盖）。
  正/负向实测通过，static 回归 100/100。RPC 仍 27 项。端点复测仍全部离线。
- **布局排版出口（第四十六轮）**：主仓新增
  `kanyu render layout`（render crate `layout` 排版器首个 CLI 出口——
  A4 横/竖 + 标题/图例/比例尺/指北针内嵌地图渲染，`--page/--dpi/
  --no-legend/--no-scalebar/--no-north/--theme/--style[-file]` 全参数面）；
  组件侧 `kanyu_render` 动态工具新增 `layout` 分支（renderLayout 助手 +
  ensureOutDir 防护 + 样式文件直通 + 「排版完成」回执）。测试器
  125/125（static 97/97）。RPC 仍 26 项（布局预览 UI 页签为下轮候选）。
  端点复测仍全部离线。
- **中文路径根因复核（第四十五轮）**：推翻上轮「shell 桥 GBK 乱码」初判——
  乱码源是 curl.exe 命令行参数 GBK 化（测试方法学伪影），组件桥 UTF-8
  解码本就正确，生产中文路径全链路实证无恙（组件零改动）；测试器 +1
  桥 UTF-8 正文回归锁。测试器 123/123（static 96/96）。RPC 仍 26 项。
  端点复测仍全部离线。
- **字段计算器 UI 面板（第四十四轮）**：host 半 data.calc RPC（25→26）+
  双端编辑页签 ƒx 区（目标字段/表达式 + 前 5 行预览 + 应用落盘联动广播，
  对齐壳层 attrtable preview_calc 语义）。测试器 122/122（static 95/95）。
  3080 桥实测 calc 通过（「中文路径乱码」初判已于第四十五轮推翻——
  实为 curl.exe 参数 GBK 化伪影，桥 UTF-8 解码本就正确）。端点复测仍全部离线。
- **字段计算器出口（第四十三轮）**：主仓新增 `kanyu data calc`（attrcalc
  内核首个 CLI 出口；--target/--expr/--output，表达式支持算术/比较/逻辑/
  函数与 $area/$length/$x/$y）；组件侧 kanyu_data 新增 action=calc
  （ensureOutDir 防护 + 落盘确认回执，与 query 分支同契约）。测试器
  119/119（static 93/93）。RPC 仍 25 项。端点复测仍全部离线。
- **双半盘点 + RPC 面对称锁（第四十二轮）**：动态/静态两半 RPC 方法集
  18 = 18 零独有、页签 8/8 相同；差异仅四处设计意图（cordis 卡片/preset
  门控/样式注入/slot 数）已入 dsh/README 差异白名单表；测试器新增两半
  对称断言（强于既有单向 ⊆ 锁）。测试器 115/115（static 92/92）。
  RPC 仍 25 项。端点复测仍全部离线。
- **3D 页签联动重载（第四十一轮）**：双端 Tab3d 已加载场景时跟随 store.path
  切换/rev 递增自动重载（未加载不自动制备），TabMap 同范式——store.rev
  版本号机制第二受益面。测试器 114/114（static 91/91）。RPC 仍 25 项。
  端点复测仍全部离线。
- **地图页签联动重渲染（第四十轮）**：store 加 rev 内容版本号，编辑四入口
  （apply2/undoRedo/applyAttr/vUp）成功一律 rev++ 广播——同路径内容变更
  （撤销/原地编辑）地图页签亦可感知；TabMap 已渲染过时跟随图层切换/rev
  递增自动重渲（未渲染过不自动出图）。测试器 112/112（static 89/89）。
  RPC 仍 25 项。端点复测仍全部离线。
- **目录页签 freshness 自动重扫（第三十九轮）**：双端目录页签监听
  store.path——当前图层变为清单外新文件（查询/编辑/服务拉取产出）时自动
  重扫一次（knownRef 防重复），计数与清单不再滞留到手工扫描。产出类操作
  四面广播后最后一个不跟随的面已闭环。测试器 110/110（static 87/87）。
  RPC 仍 25 项。端点复测仍全部离线。
- **查询结果联动属性表（第三十八轮）**：双端数据页签 runQuery 命中并设为
  当前图层后自动 data.preview 载入结果集属性表（命中即览，免二次点击；
  预览不可达降级仅留计数回执）。测试器 108/108（static 85/85）。
  RPC 仍 25 项。端点复测仍全部离线。
- **kanyu_scene3d 高度范围回执（第三十七轮）**：scene3d.data RPC 增量返回
  heightRange（缺高度字段归一 10 后累积 min/max）；动态工具回执附
  「高度范围 min~max」+ 工作台 3D 页签接力指引。8 个动态工具回执面全部
  过一轮。测试器 106/106（static 83/83）。RPC 仍 25 项。
  端点复测仍全部离线。
- **kanyu_catalog WMS 参数面（第三十六轮）**：WMS 底图分支 bbox/width/height
  直通（可据 data.info extent 给真实范围，此前恒全球）+ urlOnly 离线契约
  路径 + 回执宽高写实；服务链接三分支模型侧面全部补齐。测试器 103/103
  （static 80/80）。RPC 仍 25 项。端点复测仍全部离线。
- **编辑页签联动刷新（第三十五轮）**：双端 Client 编辑页签 apply2/undoRedo
  成功后——属性表作废待重载、顶点画布重载几何、非原地产出改用 r.output
  为当前路径并 props.notify() 广播（对齐顶点编辑 vUp 语义）；此前通用
  表单应用/撤销/重做后两区滞留旧数据。测试器 101/101（static 78/78）。
  RPC 仍 25 项。端点复测仍全部离线。
- **kanyu_catalog 服务链接回执指引（第三十四轮）**：discover 回执附拉取
  用法指引、fetch 回执附产出接力提示（对齐 query/geoprocess/edit 范式）；
  工具面补 xml/data 离线直通参数（RPC 早有的调试路径），服务链接分支
  首次可离线动态实测。测试器 99/99（static 76/76）。RPC 仍 25 项。
  端点复测仍全部离线。
- **kanyu_edit 撤销栈回执（第三十三轮）**：动态工具成功回执附「撤销栈
  N 步 / 重做栈 M 步（可经 edit.undo/edit.redo RPC 或工作台编辑页签
  回滚）」——`editApply` 本已返回 history 栈深，此前被文本面丢弃；
  模型侧可据此提示可回滚步数（只读算子 feature-count 不附）。测试器
  96/96（static 73/73）。RPC 仍 25 项。端点复测仍全部离线。
- **data.info 范围摘要（第三十二轮）**：主仓 LayerSummary 新增 extent
  （[minx,miny,maxx,maxy]，WKB 解码累积，空为 None）——CLI 文本/JSON、
  MCP data_load、组件 data.info RPC 三面直通同获益；CRS 不报（内核不
  追踪坐标系，不诚实报告不如不报）。cargo test/clippy 全绿，本机 CLI
  已更新。测试器 94/94（static 71/71）。RPC 仍 25 项。
  端点复测仍全部离线。
- **kanyu_geoprocess 产出回执（第三十一轮）**：动态工具双分支（精选
  白名单 + 注册表全库）成功回执附「产出: N 要素 → path」写出清单
  （writesSummary 解析 stderr 共用契约，与客户端 tbRun 同源），此前
  白名单回执无路径无计数、注册表分支带 output 时回执无产出信息。
  测试器 93/93（static 71/71）。RPC 仍 25 项。端点复测仍全部离线。
- **kanyu_crs reproject 计数回执（第三十轮）**：动态工具 reproject 带
  output 成功时返回「投影变换完成：from → to，N 要素 → 已写出: path」
  （此前仅「已输出: path」无计数），要素数与客户端 runReproject 同源
  解析 stderr。测试器 90/90（static 70/70）。RPC 仍 25 项。
  端点复测仍全部离线。
- **kanyu_data query 落盘回执（第二十九轮）**：动态工具 query 带 output
  成功时返回「查询完成：命中 N 要素 → 已写出: path」确认文本（此前
  stdout 空、模型侧无回执），命中数与客户端 runQuery 同源解析 stderr；
  描述注明产出接力用法。测试器 88/88（static 69/69）。RPC 仍 25 项。
  端点复测仍全部离线。
- **工具箱产图层联动（第二十八轮）**：双端 ToolboxPanel 产图层工具
  （report=false 且无 OutFile 参数）输出缺省落 dsh/output/（split_by_field
  多产出视作目录），成功解析 stderr 写出清单 → 首产出设为当前图层全页签
  联动；报告类直出原文；host toolboxRun 补 ensureOutDir（三条 --output
  路径全部保底）。测试器 86/86（static 68/68）；3080 桥实测 buffer 落盘
  + 计数正确。RPC 仍 25 项。端点复测仍全部离线。
- **投影变换联动（第二十七轮）**：双端坐标页签「投影变换」改专属
  runReproject——crs.reproject 带 output 落盘 dsh/output/，stderr 解析
  计数展示「源 → 目标：变换 N 要素」，落盘成功即设为当前图层（store.path
  广播，各页签联动）；host crsReproject 补 ensureOutDir（reproject
  --output 同款不建父目录防护）。测试器 82/82（static 65/65）；3080 桥
  实测 4326→4547 落盘 + 计数正确。RPC 仍 25 项。端点复测仍全部离线。
- **数据页签查询联动（第二十六轮）**：双端「查询」改专属 runQuery——
  data.query 带 output 落盘 dsh/output/，stderr 解析命中数 + data.preview
  取总数展示「命中 N/M 要素」，落盘成功即设为当前图层（store.path 广播，
  各页签联动）；host dataQuery 补 ensureOutDir（--output 不建父目录防护）。
  测试器 78/78（static 62/62）；3080 桥实测落盘 + 计数正确。RPC 仍 25 项。
  端点复测仍全部离线。
- **3D 分类着色（第二十五轮）**：scene3d.data 加 colorField（逐要素 cat +
  categories 去重清单 ≤12 类）；双端 catColor 哈希 HSL 稳定取色、棱柱按
  类别着色 + 类别图例行；kanyu_scene3d 加 colorField。测试器 74/74
  （static 59/59）；3080 桥实测两类正确。RPC 仍 25 项。端点复测仍全部离线。
- **kanyu_geoprocess 注册表分支（第二十四轮）**：模型侧动态工具双轨分流——
  白名单外 id 走 toolbox.run 注册表分支（37 工具全库，input 映射 layer、
  params 键值透传），处理域模型面覆盖全库。测试器 72/72（static 57/57）。
  RPC 仍 25 项。端点复测仍全部离线。
- **处理页签工具箱全库表单（第二十三轮）**：双客户端新增 ToolboxPanel——
  toolbox.list 拉 tooldef 37 工具注册表，五分类 optgroup 分组 + ParamKind
  驱动动态表单（Enum 下拉/Boolean 复选/Layer 预填当前图层/LinearUnit·
  MultiLayers·Extent 格式提示），运行走 toolbox.run；13 精选快捷面并存。
  测试器 70/70（static 57/57，toolbox.list 静态断言改双态）。RPC 仍 25 项。
  端点复测仍全部离线。
- **工具箱注册表全库接内核 tooldef（第二十二轮）**：主仓 kanyu-cli 新增
  `kanyu tool list/run` 顶层子命令（37 工具，core::tooldef 单一事实来源；
  docs/CLI.md §4C）；组件新增 `toolbox.list`/`toolbox.run` RPC（25 项），
  CLI 过旧降级为中文升级指引；GP_TOOLS 13 白名单精选面保留。测试器 68/68
  （static 55/55）。双端处理页签全库表单下轮跟进。端点复测仍全部离线。
- **CRS 全库检索接内核 EPSG 库（第二十一轮）**：主仓 kanyu-cli 新增
  `kanyu crs search/info` 顶层子命令（直连 core::crs 单一事实来源，EPSG
  7507 条；docs/CLI.md §4B），本机 CLI 已更新；组件新增 `crs.search` RPC
  （23 项）——经 CLI 检索全库，CLI 过旧回退预设兜底并标注 degraded；
  `kanyu_crs` 工具加 search 分支；双客户端坐标页签加 EPSG 检索框（结果
  点击设为目标 CRS）。测试器 65/65（static 54/54）；cargo test 全绿。
  端点复测仍全部离线。
- **目录五分类补全（第二十轮）**：`catalog.list` 加 `mapItems`（output/*.png
  渲染产物 = 地图框对应物）+ `layoutItems`（解析 .kyu 工程 v2 layouts 节 =
  布局框对应物，壳层 project.rs 单一事实来源），五分类计数全部真实回填；
  双客户端改 catRows 分行描述符（产物行只读、数据行可点选）；demo.kyu 夹具
  加 layouts 节。测试器 63/63（static 52/52）；3080 桥实测布局框入列正确。
  端点复测仍全部离线。
- **顶点编辑画布（第十九轮）**：新增 `edit.geometry` RPC（22 项，原样几何
  不抽稀——scene3d.data 抽稀预算不可用）；双客户端编辑页签加顶点编辑画布
  （enumVertices 三级 ringPath 定位 + 拖拽松开写 vertex-move + 重载几何，
  非原地输出自动设为当前图层）。测试器 62/62（static 51/51）；3080 桥实测
  4/4 要素原样几何 + bbox 正确。端点复测仍全部离线。
- **属性单元格编辑（第十八轮）**：双客户端编辑页签加「加载属性表 → 点选行
  → 写入单元格」闭环（复用 data.preview + attribute-set，无新增 RPC）。
  **workspace-write 模式入档**：DSH fs 服务生产侧工作区外读放行、写拒绝
  （3080 实测），writeHint 统一给中文可操作指引；工作区内编辑闭环桥实测
  通过（写入 → preview 复查 → undo 栈 +1）。测试器 59/59（static 48/48）。
  端点复测仍全部离线。
- **WMS GetMap 底图（第十七轮）**：新增 `services.wms` RPC（21 项）——
  `buildGetmapUrl` 移植壳层 services.rs v2（WMS 1.3.0 + EPSG:4326 + bbox
  六位小数），联机拉 PNG base64 内联预览，`urlOnly` 离线契约路径；
  `kanyu_catalog` 加 `kind=wms` 分支；双客户端服务链接分类加底图预览行。
  测试器 56/56（static 45/45）；3080 桥实测地址构造逐字符正确。
  端点复测仍全部离线。
- **WFS GetFeature 拉取（第十六轮）**：新增 `services.fetch` RPC（20 项）——
  `buildGetFeatureUrl`/`joinQuery` 移植壳层 services.rs（GeoJSON 输出优先、
  10s 超时、离线 `data` 路径），响应校验 FeatureCollection 根，缺省落
  `output/wfs_<图层>.geojson`；`kanyu_catalog` 加 `url+layer` 拉取分支；
  双客户端服务链接图层行加「拉取」按钮（成功即设为当前图层）。测试器
  55/55（static 44/44）；3080 桥实测离线拉取落盘正确。端点复测仍全部离线。
- **服务链接 WFS 发现（第十五轮）**：新增 `services.discover` RPC（19 项）——
  `parseCapabilities` 移植壳层 services.rs 最小提取纯函数（不引 XML 库），
  URL 路径 10s 超时 + 离线 `xml` 解析路径；`kanyu_catalog` 加 `url` 分支；
  双客户端目录页签服务链接分类加发现表单。测试器 54/54（static 43/43）；
  3080 桥实测夹具解析正确。端点复测仍全部离线。
- **目录五分类（第十四轮）**：`catalog.list` 响应扩展 `categories` 固定五分类
  元组 + `dataItems`/`dbItems` 分离（.kdb/.kyu 入数据库类），对齐壳层
  catalog.rs 工程目录范式；双客户端目录页签改分类区渲染（计数徽标 +
  展开/收起，默认仅本机数据展开，空分类空态提示）；新增 demo.kyu 夹具。
  测试器 51/51（static 40/40）；3080 桥实测分类计数与 kyu 归类正确。
  端点复测仍全部离线。
- **属性表预览（第十三轮）**：新增 `data.preview` RPC（纯 fs 读面，不经
  CLI），RPC 表 17→18；`kanyu_data` 动态工具加 `preview` action；双客户端
  数据页签加「属性表」按钮 + 表格渲染（sticky 表头滚动容器）。测试器
  48/48（static 37/37）；3080 桥实测 buildings.geojson 5 字段 4 行、
  limit 截断正确。端点复测仍全部离线。
- **地图面板符号化（第十二轮）**：`render.map`/`kanyu_render` 支持 `style`
  （StyleRule graduated/categorical，经 `--style-file` 传递——pwsh 下 JSON
  内嵌引号不能走命令行，实测排障入档）；双客户端地图页签加符号化控件。
  测试器 46/46（static 35/35）；3080 桥端到端 graduated PNG 目检通过。
  `sync-local.sh` 加固（残留清理 + 新鲜度校验）。端点复测仍全部离线。
- **kanyu-mcp 桥接入 GIS 模式（第十一轮）**：preset 组合新增 `mcp-kanyu` 行
  （dsh-mcp-client，stdio `kanyu mcp serve`），内核 17 stable 工具以
  `mcp__kanyu__*` 限定名进会话模型工具面，与组件 8 动态工具互补。实证：
  roster 无 broken、session.create 成功、实例日志 9 处 kanyu-mcp stdio 启动
  零错误（模型侧入目待首局对话实测终验）。本地三模型端点复测仍全部离线。
- **本地同步契约落地（用户报告修复）**：运行中的 `dsh web` 实例不热加载——
  组合树/boot 图启动时一次成型，插件更新后旧实例症状为 boot 图无 kanyu 条目、
  bundle 404、health 落 SPA 兜底页；重启实例即修复。新增 `dsh/sync-local.sh`
  一键本地同步（preset 回灌 + 校验 + 插件重装），**每次 dsh/ 更新后必跑**，
  并重启 `dsh web` 实例（§5 维护契约已改写）。
- **3D 真管线对接（本轮新增）**：双客户端 `drawScene3d` 对齐内核 `scene3d.rs`
  软件管线——投影链（线性映射 → yaw 旋转 → sin(pitch) 压缩 → 高度抬升）、
  背面剔除、质心纵深排序、侧面两档明暗、高度归一化 0.25、纯白底；Tab3d 加
  拖拽旋转（yaw/pitch 钳制 30°–45°，内核交互契约同式）。测试器 42/42 全绿
  （新增 2 项 3D 契约断言），web profile 重装冒烟通过（health 200 + bundle
  含新管线）。本地三模型端点复测仍全部离线，首局对话实测继续顺延。
- **组件仓 CI 落地（第九轮）**：`tools/test_plugin.mjs` 新增 `--static` 零依赖
  CI 模式（跳过全部 kanyu CLI 依赖断言，RPC 桥实测改用纯本地 `crs.presets`）+
  主仓/组件仓双布局自检；新增 `.github/workflows/component-test.yml`（同步进
  组件仓仓根 `.github/workflows/`，push/PR 触发）。本机三验：主仓 static 31/31、
  全量 40/40 回归不破、模拟组件仓根布局 static 31/31。本地三模型端点复测仍全部
  离线，首局对话实测继续顺延。
- **领域技能 SKILL.md 对齐组件现状（第八轮）**：`skills/kanyu-gis/SKILL.md` 新增
  「DSH 组件形态」章节——双半与双安装形态（plugin/ 动态 + pkg/ 静态）、8 个
  `kanyu_*` 动态工具清单、17 项 RPC 全清单（含编辑 undo/redo/history）、工作台
  联动与编辑双栈、组件验证面（test_plugin.mjs 40 断言 / verify_preset.mjs /
  sync-preset.sh）。本地三模型端点（11434/1031614/15724）当日实测全部离线，
  kanyu-gis 会话首局对话实测顺延，待端点在线后执行。
- **编辑能力深化（第七轮）**：组件编辑内核对齐 kanyu-edit 命令逆操作双栈范式——
  `applyMutation` 单一变更入口正/逆共用，变更算子入 undo 栈（容量 64、新变更清 redo），
  新增 `edit.undo`/`edit.redo`/`edit.history` RPC（RPC 表 14→17），双客户端编辑页签
  加撤销/重做按钮；测试器 40/40 全绿（含 undo/redo 闭环 5 断言）。
- **面板联动加载（第五轮，用户指令落地）**：`dsh/pkg/` 升级为 dsh.client 双面包——
  `pkg/client.js` 静态客户端 bundle 常驻 web 前端 boot 图，「🧭 堪舆GIS」会话头部按钮 +
  七页签工作台浮层（目录/数据/地图/坐标/处理/编辑/3D/关于）经会话快照 `agentPreset`
  字段门控：**新建/切换到 kanyu-gis preset 会话时面板自动出现，切回其他模式即隐藏**。
  Host 半配套新增 `webServer` 前缀路由 `/kanyu-gis/call` RPC 桥（静态形态无 host.call，
  此为官方等价物）。实测验证链：boot 图条目 + bundle 200 + ping/catalog.list 中文路径
  端到端全通。实测教训：带 `exports` 的客户端包必须导出 `./package.json`，否则
  `require.resolve` 被封装拦截、客户端扫描静默跳过。
- **仓库侧**：`dsh/` 组件源完整入库；`host.js` CLI 命令面与 v0.22.0 实测逐旗标对拍一致；
  `verify_preset.mjs --preset-dir dsh/presets` exit 0；`kanyu agents validate --code-repo` exit 0。
- **GIS 模式 preset 活体挂载验证（第四轮，web profile 实证闭环）**：
  `dsh web` 实例上经 API 实测——`agentPreset.list` 起初判 kanyu-gis **broken**
  （"row 1 names no plugin"：初版组合误按宿主平面写了 model 路由行/file-operations/
  process 服务行/memory/system-prompt 特殊行与不存在的 dsh-tool-read 等包名）；
  按 local-hybrid 方言重写 `agent.cordis.yml` 后 roster 复验 broken 清除，
  `session.create(agentPreset=kanyu-gis)` 成功，`skill.list` 初见空目录——
  根因为 SKILL.md frontmatter 双引号标量内 `\B` 非法 YAML 转义致技能文件被静默
  丢弃，修为正斜杠路径后**技能入目实证通过**。
  实测教训两条入档：① 代理平面组合每行必须是带 `name` 的插件行（或 cordis:group
  组行），模型路由/fs/沙箱/subagents 注册表属宿主平面，preset 不可声明；
  ② SKILL.md frontmatter 是严格 YAML——双引号标量内 Windows 反斜杠路径为非法
  转义，一律用正斜杠或单引号。`verify_preset.mjs` 已补「行必须有 name」同款判定
  （对齐 invariant.js `entryListProblem`），此后仓库侧旁路校验可提前拦截此类 broken。
- **本地测试**：`dsh/tools/test_plugin.mjs` 组件测试器落地——node:vm 等价
  沙箱（shell→真实子进程跑 kanyu CLI、fs→node:fs、harness→RPC/工具表收集），
  **40/40 断言全绿**（17 RPC + 8 动态工具注册 + 七大能力逐项实证 + Client 半
  语法/结构静态校验 + 编辑 undo/redo 双栈闭环 + pkg 静态双面包契约组：
  exports/dsh.client 声明、工厂 id 契约、preset 门控、方言禁项、两半 RPC
  漂移锁、index.js RPC 桥实测 ping 200）；临时产物自清理，`dsh/output/` 已入 .gitignore。
- **DSH 活体冒烟**：`dsh --profile headless` 在仓库根执行真实任务
  「`kanyu agents validate --code-repo` 并引用输出」——会话代理实测执行并正确引用
  「AGENTS.md 校验通过：0 个图层，0 条业务规则」，DSH × kanyu CLI 链路活体验证通过。
- **边界（实测记录）**：headless profile 经 `--dump-config` 确认**不含** agent-presets
  roster 与 cordis 动态包 runner——preset 挂载与组件动态包（cordis_define/cordis_run）
  只能在 web profile（GUI 会话）进行；本机三个模型端点（11434/1031614/15724）当时离线，
  headless 走部署默认路由；cmd.exe 传中文绝对路径会被代码页截断（宿主 shell 为 pwsh
  无此问题，测试器走 Git Bash）。
- **安装侧**：preset 经 `bash dsh/sync-preset.sh` 同步至
  `~/.dsh/.agent-presets/kanyu-gis/`；**组件静态插件已装进本机 DSH web profile**——
  `dsh/pkg/` 适配器（读取 `dsh/plugin/host.js` 单一事实源，harness façade →
  `ctx.tools.register`，参数表折算标准 JSON Schema）经
  `dsh plugin --profile web add file:.../dsh/pkg` + `cordis.patch.yml` insert 行
  （`config.hostSource` 显式指向 host.js 绝对路径）安装，**web profile 启动实测激活**：
  日志「kanyu-gis 静态插件已激活：8 个 kanyu_* 工具注册进工具注册表」。
  实测教训三条入档：① Cordis 普通插件必须 `inject` 声明服务（无 inject 时 ctx.get
  取 undefined 静默停用）；② pnpm file: 安装为副本非活链，改 host.js/pkg 后须
  remove+add 重装刷新；③ `import.meta.url` 在 profile node_modules 副本下相对
  路径不可行，须 config.hostSource 显式路径。
- **crates 侧**：零改动（组件层作业）。
- **远端侧**：主仓 Kanyu + 独立仓 DaoMingyuan/kanyu-gis 双仓均已推送（见会签簿回记）。

## 5. 维护契约

- 改 `dsh/**` → 重跑 `bash dsh/sync-local.sh`（一键本地同步：preset 回灌 +
  旁路校验 + web profile 静态插件重装；用户指令「每次更新完成，本地要同步更新」
  的落地脚本）；若 `dsh web` 实例在跑，**须重启实例**——组合树与客户端 boot 图
  在启动时一次成型，不热加载（过期实例症状：boot 图无 kanyu 条目、bundle 404、
  /kanyu-gis/health 落 SPA 兜底页）。
  改 crates 能力面 → 先四道门禁（build→test→clippy→fmt），再同步组件文档。
- 单一事实来源：能力表以本手册 §1、`dsh/README.md` 与 `introspect.rs` 三者一致为准；
  新增组件工具时三处同时更新，并在会签簿登记。
- 许可边界：组件安装目录仅含本仓库 `dsh/` 源文件原文，不引入任何第三方二进制、
  位图或闭源脚本（对照 AGENTS.md「无冗余文件」铁律）。
