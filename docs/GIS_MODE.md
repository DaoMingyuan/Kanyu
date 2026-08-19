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

## 4. 当前状态（2026-08-19，第一百轮）

- **底部会话/终端融合 + 地图白底（本轮）**：壳内底部条（`.kyg-bottombar`）
  三页签——「地图」满高 /「会话」压壳 52% 露出宿主会话区（功能完整性由
  宿主保证）/「终端」壳内受限控制台（`console.run` RPC：kanyu 子命令
  白名单 data/render/crs/tool/edit/agents/introspect/skill 直通，拒
  链式/重定向/替换符号，无任意代码执行安全基线）；顶栏「返回会话」钮
  删除，收起入底部条（kyg-reopen 浮动钮保留）。地图/编辑画布白底
  `#ffffff`（对齐 ArcGIS Pro 纯白画布与壳层 mapview.rs 约定），壳底色
  向 DSH 中性灰对齐（#16181d，topbar 中性化）。RPC 面 34→35。
  测试器 269/269（static 212/212），agent-browser 3080 实测：白底渲染/
  终端 data info 出图层信息/非白名单命令拒绝/会话压壳露 composer +
  截图目检。九十九轮双仓 CI（211567c / 72f5bf2）success。

- **分析工具右侧化（第九十九轮）**：地理处理按 ArcGIS Pro 工具箱范式迁入右侧
  停靠面板（`.kyg-right` 300px）：ribbon 「处理」页签撤下改「分析处理」
  组内「分析」开关钮（`gpOpen`），面板内含工具搜索框（`gpq` 名称/id
  子串过滤）+ 精选工具清单行（GP 徽标）+ 参数表单 + 工具箱全库注册表。
  测试器 266/266（static 209/209），agent-browser 3080 实测搜索过滤
  + buffer 选工具/填参/运行出 4 要素 + 截图目检。
  九十八轮双仓 CI（55da450 / 1e16d12）推送完成。

- **地图界面 GIS 化（第九十八轮）**：主题选择器删除（UI 固定晨山 light，
  `render.map` theme 契约保留恒传）；工具行单行化（符号化/.kyu 持久化
  收进「样式」折叠区 symOpen）；画布 `kyg-fill` flex 链铺满中央区；
  2D/3D 视图切换（`kyg-viewswitch` 分段控件，scene3d 页签 hidden 不占
  ribbon，ArcGIS Pro 视图范式）。测试器 264/264（static 207/207），
  agent-browser 3080 实测 2D 渲染/3D 场景加载/切换回环 + 截图目检。
  九十七轮双仓 CI（3f7ee87 / 494ca54）推送完成。

- **目录 ArcGIS Pro 化（第九十七轮）**：`catalog.list` 六分类重构（文件夹连接/
  数据库/矢量数据/服务链接/地图框/布局框），矢量数据 `vecGroups` 按格式
  分组可折叠（Shapefile/GeoJSON/GeoPackage…）；`.gdb` 目录整目录登记入
  数据库类不深入扫描，`gdbUnsupported` 守卫（data info/query/validate/
  preview/calc）明确报错 GDAL 指引不做伪支持；Dock 工程图层/数据图层
  分组折叠（grpHead）。测试器 262/262（static 205/205），agent-browser
  3080 实测六分类渲染 + GDB 条目指引 + Dock 折叠回环。
  九十六轮双仓 CI（4954c14 / 9827395）均 success。

- **Dock 工程图层可见性开关（第九十六轮）**：`style.setVisible` RPC 写回 .kyu
  `visible` 布尔，Dock 工程图层行复选框（stopPropagation 不抢接力点击）
  对齐壳层 toc.rs 复选框语义。测试器 257/257（static 200/200），
  agent-browser 3080 实测 demo.kyu 关/开回环 + 文件写回 + 徽标联动。
  九十五轮双仓 CI（c1929bd / 265f51b）均 success。

- **3D 高程夸张系数（第九十五轮）**：`drawScene3d` exag 形参 zScale 乘算（纯
  显示），3D 页签「夸张」下拉 ×0.5–×5 + 画布标注档位（ArcGIS Pro 垂直
  夸张语义）。测试器 253/253（static 196/196），agent-browser 3080
  实测 ×3 高耸 vs ×0.5 低矮对比成立。九十四轮双仓 CI（1f1219e /
  3f5adf1）均 success。

- **identify 浮层「定位至此」（第九十四轮）**：`data.identify` 回执加 `centroid`
  几何范围中心，浮层「定位」钮一键 zf=2 + pan 倍率反解居中命中要素
  （ArcGIS Pro「缩放至所选」语义）。测试器 251/251（static 194/194），
  agent-browser 3080 实测要素居中偏差 +2/−1 px、比例尺 1:8,025→
  1:4,013。九十三轮双仓 CI（e5ee605 / 90465a6）均 success。

- **地图画布量测（第九十三轮）**：测距/测面双模式，画布单击攒点 + SVG 覆盖层
  实时折线/多边形，haversine/等距圆柱 shoelace 累算（4326），双击冻结、
  清除重来；量测中单击不触发 identify（onMapClick 分派）。测试器
  248/248（static 191/191），agent-browser 3080 实测测距 1.02 km /
  测面 1.36 km² 与理论值一致。九十二轮双仓 CI（e37e26f / d98e014）均
  success。

- **状态栏鼠标坐标实时跟踪（第九十二轮）**：画布 mousemove 节流（≥60ms）反算
  地图坐标 → `store.mapCursor` → 状态栏「坐标: x, y」实时显示，
  mouseleave/出图区清空（ArcGIS Pro/QGIS 状态栏坐标标配）。测试器
  246/246（static 189/189），agent-browser 3080 实测中心坐标
  116.39999,39.91001 与 mouseout 清空。九十一轮双仓 CI（d801b27 /
  3a80e61）均 success。

- **地图画布要素点选查询（第九十一轮）**：`data.identify` RPC 纯 fs 空间点选
  （面射线法含洞排除/线点段距/点最近距，tol 地图单位），画布单击经
  像素分数 + `store.mapExtent` 反算地图坐标命中要素，弹属性浮层并
  联动状态栏「选中要素 #N」；拖拽超 4px 抑制 click。测试器 244/244
  （static 187/187），agent-browser 3080 实测命中示例大厦A。九十轮
  主仓 CI（83b5385）success。

- **壳层 WMS GetMap 轴序对齐 1.3.0 规范（第九十轮）**：
  `services::build_getmap_url` EPSG:4326 bbox 改发 `纬度,经度` 规范轴
  序，与组件侧 axisSwap 语义对齐；严格 1.3.0 服务器（terrestris）可
  用，GeoServer 等宽限服务器同兼容。cargo test/clippy/doc 全绿。
  八十九轮双仓 CI（0492697 / efce90f）均 success。

- **工程下拉接目录自定义扫描目录（第八十九轮）**：目录页签 scan 发布
  `store.scanDir`，顶栏工程下拉随 scanDir 重扫 .kyu。测试器 240/240
  （static 183/183），agent-browser 实测清空/恢复联动通过。八十八轮
  主仓 CI（e654cfb）success。

- **滚轮缩放指针为锚（第八十八轮）**：缩放保持光标下内容点不动（pan 倍率
  差补偿），GIS 标准缩放手感。测试器 238/238（static 181/181），
  agent-browser 实测通过。八十七轮双仓 CI（60ed589 / 38131a7）均
  success。

- **地图画布缩放/平移 + 状态栏比例尺重算（第八十七轮）**：滚轮 1.2× 步进
  （0.5–16×，非 passive 监听）+ 拖拽平移 + 双击复位；倍率经
  `store.mapZoom` 发布，状态栏比例尺 ÷ 倍率实时重算并显缩放档。
  测试器 236/236（static 179/179），agent-browser 实测通过。八十六
  轮双仓 CI（6454074 / 8759701）均 success。

- **GIS 界面重排 + 堪舆手绘风图标（第八十六轮）**：ribbon 按 GIS 操作语义分
  五组（数据管理/地图视图/分析处理/编辑/系统，组框+组标签），页签改
  纵向「图标+文字」；`kanyuIcon()` 手绘风 SVG（stroke=currentColor
  随态变色，对齐壳层 ui_kit 手绘图标语义）替全部 emoji，罗盘入顶栏/
  头部/重开钮；会话功能保留。测试器 234/234（static 177/177），
  agent-browser 复验通过。八十五轮双仓 CI 均 success。

- **底图 WMS 入画布背景（第八十五轮）**：地图页签「底图 WMS」勾选行——内核
  新增透明背景渲染（`--background none`），`loadBasemap` 按图层范围拉
  GetMap 同尺寸底图垫底；`axisSwap` 修复严格 1.3.0 服务器（EPSG:4326
  纬度/经度规范轴序）空白图问题；导出含底图时 canvas 合成。agent-
  browser 实测真实 OSM 街道底图 + 要素叠加出图。测试器 232/232
  （static 175/175），cargo test --workspace 全绿。八十四轮双仓 CI
  （8d0d21f / c07a2a2）均 success。
- **顶栏工程选择接 .kyu（第八十四轮）**：顶栏新增工程下拉（catalog.list 扫
  .kyu → style.list 载入 → store.kyuProject 发布）；图层坞渲染工程图层
  组，点击 = 当前图层 + 样式/工程路径/图层 id 接力（同目录页签
  pickKyuLayer 语义）。agent-browser 实测 demo.kyu → 「工程: 组件目录
  夹具」图层组 → 点击 buildings → 状态栏当前图层切换。测试器 229/229
  （static 172/172）。八十三轮双仓 CI（a65b11f / 9bdcf7a）均 success。
- **状态栏接真实数据（第八十三轮）**：状态栏由静态占位升级为数据驱动——渲染
  成功后经 `data.info` 取要素计数/范围，`approxScale` 推算近似比例尺
  （范围宽 × 图像像素宽，经纬度按中心纬度换算），坐标系按格式推断
  （GeoJSON = EPSG:4326），`store.mapInfo` 上栏；编辑页签框选顶点集与
  属性表选中行实时上栏（`store.selVerts`/`selFeature`）。agent-browser
  实测「要素: 4 · 坐标系: EPSG:4326 · 比例尺≈1:8,025 · 已选顶点: 1」。
  测试器 227/227（static 170/170）。八十二轮双仓 CI（93b7785 / 21d4f31）
  均 success。
- **地图页签画布化（第八十二轮）**：TabMap 对齐参考形态——`stageRef` 量中央区
  宽度自适应出图（480–1600），`firstRef` 入场自动出图（带当前图层进页签
  即渲染），预览包入 `kyg-map-stage` 舞台容器，新增「导出地图图片」PNG
  下载按钮。agent-browser 实测舞台大图出图 + 导出在列。测试器 225/225
  （static 168/168）。八十一轮双仓 CI（389a1f6 / ce6c897）均 success。
- **GIS 模式全屏工作台（第八十一轮，参考用户提供的「地理工作台」截图形态）**：
  工作台由浮层改为全屏接管会话中央列（`useCenterRect` ResizeObserver 同步
  centerCol 矩形，position:fixed 落于 shell.overlay 层内，侧栏保持原生）。
  布局：顶栏 + 页签 ribbon + 左侧图层坞（catalog.list 数据类，点击设当前
  图层）+ 中央页签区 + 底部状态栏；「返回会话」留 `kyg-reopen` 悬浮重开钮。
  agent-browser 实测闭环：切入自动接管 → 返回会话 → 重开召回 → 切出收起。
  测试器 223/223（static 166/166，全屏布局契约键锁双端）。八十轮双仓
  CI（d257fbc / 558be49）均 success。
- **修「切 GIS 模式界面无变化」（第八十轮，用户实测报告）**：静态半
  `pkg/client.js` 工作台此前仅在会话头部按钮点击后展开，而首页/新会话
  视图不渲染会话头部槽位——切 kanyu-gis preset 后页面零变化（boot 清单
  与 client.js 投递其实正常）。修复：preset 转换边联动——`prevGis` ref
  切入 kanyu-gis 自动展开工作台、切出自动收起（手动关闭不反复弹出）；
  动态半 `plugin/client.js` 补同款 UX（`autoOpened` 激活即展开一次）。
  agent-browser 实测：标准模式 ↔ kanyu-gis 往返，面板 0 ↔ 22 个 kyg-*
  元素（8 页签）联动。测试器 221/221（static 164/164，联动契约键锁双端）。
- **CRS 检索命中双按钮（第七十九轮）**：双端 TabCrs 检索结果行加「源」/「目标」
  按钮分设 CRS（替代整行点击只设目标——源 CRS 此前无法从检索回填）。
  测试器 219/219（static 162/162，crsKeys 扩键锁双端）。七十八轮双仓
  CI（cea10af / db37bd5）均 success。
- **几何量算 WASM 技能（第七十八轮，技能沙箱第七算子）**：新 guest crate
  `dsh/skills/measure_geom/`（param `_measure: area/length` 逐要素写
  `_area`/`_length`，shoelace 外环减内环 / 欧氏长度零依赖实现，类型不
  匹配透传）+ `dsh/skills/measure_geom.wasm` 入仓；双端技能分析区加量算
  行（area/length 下拉 + skillRelay 接力）。测试器 219/219
  （static 162/162）。七十七轮双仓 CI（79f67f2 / cc932a3）均 success。
- **目录面板条目过滤（第七十七轮）**：双端 TabCatalog 加过滤框（五分类清单按名
  子串过滤、大小写不敏感，分类头命中/总数，过滤中强制展开）。测试器
  216/216（static 162/162，catKeys 扩键锁双端）。七十六轮双仓 CI
  （507c802 / dcd036b）均 success。
- **几何简化 WASM 技能（第七十六轮，技能沙箱第六算子）**：新 guest crate
  `dsh/skills/simplify_geom/`（param `_tolerance` RDP 容差抽稀线/面顶点，
  geo Simplify，属性继承 + `_tolerance`/`_verts`；点系透传、退化跳过）+
  `dsh/skills/simplify_geom.wasm` 入仓；双端技能分析区加简化行
  （skillRelay 接力）。测试器 216/216（static 162/162）。七十五轮双仓 CI
  （771582b / d299259）均 success。
- **裁剪 clip 算子（第七十五轮）**：`overlay_ops` guest 扩 `_op: clip`（ArcGIS Clip
  语义：基准面整体 ∩ 叠加整体一次性交集、不两两配对、叠加属性不入产出）；
  双端叠加算子下拉加「裁剪 clip（叠加层作模子）」。测试器 214/214
  （static 162/162）。七十四轮双仓 CI（996aa0a / 40cb8ad）均 success。
- **统计聚合 WASM 技能（第七十四轮，技能沙箱第五算子）**：新 guest crate
  `dsh/skills/stat_summary/`（param `_stat` 必填数值字段 + `_field` 可选
  分组字段，纯属性聚合输出 geometry:null 表语义要素，带
  `_count/_skipped/_sum/_min/_max/_avg`）+ `dsh/skills/stat_summary.wasm`
  入仓。调试发现并修复宿主侧隐蔽行为：混合类型列经 GeoArrow 类型化列
  中转被强制为字符串列，guest 兼容解析数值字符串（"10"→10），真正
  非数值跳过计 `_skipped`。host.js `kanyu_skill` 清单登记；双端技能分析区
  加统计行（skillRelay 接力）。测试器 213/213（static 162/162）。
  七十三轮双仓 CI（38bad5a / 36ced05）均 success。
- **3D 视角书签持久化（第七十三轮）**：双端 TabScene3d 书签改 localStorage 按
  图层路径键控（`kanyu-3d-views:<path>`，跨会话留存 + 逐条删除）。
  测试器 211/211（static 162/162）。七十二轮双仓 CI（80181b6 / f1a6c3c）
  均 success。
- **3D 视角书签 + PNG 导出（第七十二轮）**：双端 TabScene3d 新增视角书签
  （存 yaw/pitch 具名恢复 + 复位）与 PNG 导出（画布 toDataURL 浏览器
  下载）；TabAbout/README/agent.cordis.yml 工具计数 8→9 漂移修正。
  测试器 209/209（static 160/160）。七十一轮双仓 CI（fbb10ad / f863d95）
  均 success。
- **融合 WASM 技能（第七十一轮，技能沙箱第四算子）**：新 guest crate
  `dsh/skills/dissolve_field/`（geo union 组内折叠——按 `_field` 分组，
  相邻并单部 / 相离附 `_part`，留分组字段 + `_count`）+
  `dsh/skills/dissolve_field.wasm` 入仓；双端技能分析区加融合行
  （param `_field` 注入，skillRelay 接力）。测试器 207/207
  （static 158/158）。七十轮双仓 CI（d375093 / 9188788）均 success。
- **技能画布交互（第七十轮，WASM 技能入编辑页签对话框）**：双端 client.js 编辑
  页签新增「技能分析」区——缓冲区（距离输入走 param `_distance` 注入）/
  叠加分析（算子下拉 + 第二图层路径走 param `_op` + input2 注入）；公共
  `skillRelay` 产图层接力（产出设为当前图层 + 版本号广播 + 几何重载）。
  测试器 205/205（static 158/158）。六十九轮双仓 CI（70aceb8 / 748be79）
  均 success。
- **叠加分析 WASM 技能（第六十九轮，技能沙箱第三算子）**：新 guest crate
  `dsh/skills/overlay_ops/`（geo 0.33 BooleanOps——intersect 两两配对
  基准属性继承 / union 合并整体 / difference 基准减叠加，仅面要素）+
  `dsh/skills/overlay_ops.wasm` 入仓；第二图层经 host.js `skillRun` 新增
  `input2` 通道注入（逐要素标 `_role="overlay"`，与 cut/param 并轨），
  `skill.run` RPC 与 `kanyu_skill` 参数面同步。测试器 203/203
  （static 156/156）。六十八轮双仓 CI（3b2e10b / 17c17ae）均 success。
- **缓冲区 WASM 技能（第六十八轮，技能沙箱第二算子）**：新 guest crate
  `dsh/skills/buffer_zones/`（geo 0.33 Buffer round join——点/线/面按
  距离膨胀为面，属性继承 + `_distance` 回写、多部附 `_part`）+
  `dsh/skills/buffer_zones.wasm` 入仓；缓冲距离经 `_role` 注入约定传递——
  host.js `skillRun` 增 `param` 通道（参数键值注入 `_role="param"` 参数
  要素，与 cutLine 切割线注入并轨），`skill.run` RPC 与 `kanyu_skill`
  模型工具参数面同步加 `param`。测试器 199/199（static 156/156）。
  六十七轮双仓 CI（37f25d9 / fb324eb）均 success。
- **kanyu_skill 模型工具（第六十七轮，面切割入 AI 工具面）**：动态工具 8→9 新增
  kanyu_skill（skill/input/output/cutLine 直挂 skillRun，回执附产出清单 +
  接力提示）。测试器 196/196（static 156/156）。CI 修复：skill.run 三断言
  补 STATIC_ONLY 门控（六十六轮组件仓 CI e3f052e 红于 CLI 依赖未门控，
  已修复随本轮复跑）；六十六轮主仓 CI（6828cf1）success。
- **面切割 WASM 技能通道（第六十六轮，内核 BooleanOps 能力经技能沙箱进组件）**：
  新 guest crate `dsh/skills/split_polygons/`（geo 0.33 Buffer+BooleanOps 差集
  劈分，属性继承 + `_part` 序号）+ `dsh/skills/split_polygons.wasm` 入仓；
  host.js RPC 31→32 新增 `skill.run`（kanyu skill run CLI 出口 + cutLine 注入
  `_role="cut"`）；pkg 适配器注入 skillDir 同源定位；双端编辑画布「面切割」
  模式（cutPoly 攒切割线，产出接力当前图层）。测试器 195/195
  （static 156/156，CLI 依赖断言门控后计数），生产桥实测通过（横贯劈分 2 部 +
  未横贯面保留）。注：六十五轮双仓 CI（e63f193 / bb53068）均 success。
- **顶点框选批量移动（第六十五轮，vertices-move 原子批量算子）**：EDIT_OPS 11→12
  新增 vertices-move（先全量校验再统一写入，单条 undo 整体回滚，保留 Z/M）；
  双端 client.js 编辑画布新增「框选」开关——拖橡皮筋多选顶点（单击清空），
  选择集 ≥2 时拖拽任一选中顶点整组批量移动，drawEdit2d opts 叠加橡皮筋
  虚线框 + 选中高亮 + 批量联动预览。测试器 190/190（static 154/154），
  生产桥 vertices-move 实测通过（3 顶点批量移动，单条 undo 回滚）。注：
  六十四轮双仓 CI（74d5322 / b65771e）均 success。
- **feature-add 画布化（第六十四轮，绘制点/线/面新要素进画布）**：双端 client.js
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
  日志「kanyu-gis 静态插件已激活：9 个 kanyu_* 工具注册进工具注册表」。
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
