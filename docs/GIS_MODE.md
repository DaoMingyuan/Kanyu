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

## 4. 当前状态（2026-08-18，第三十轮）

- **kanyu_crs reproject 计数回执（本轮新增）**：动态工具 reproject 带
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
