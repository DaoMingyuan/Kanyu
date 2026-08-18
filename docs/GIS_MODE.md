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
| 地理编辑 | `kanyu_edit` | 组件内 GeoJSON 在线编辑内核（6 算子）；深度拓扑编辑由 `kanyu-edit` crate 承接 |
| 3D 地理 | `kanyu_scene3d` | 挤出体场景数据制备 + Client canvas 等距投影绘制 |

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

**新开 kanyu-gis 会话**（preset 落位后）：

```powershell
dsh run --preset kanyu-gis -w E:\BaiduSyncdisk\堪舆GIS
```

**从任意会话挂载体验**：

```powershell
cordis_mount "C:\Users\Administrator\.dsh\.agent-presets\kanyu-gis" kanyu-gis
```

挂载后刷新会话，`kanyu_*` 工具随 preflight 可用。

## 3. 与 kanyu 仓库的联动机制

1. **命令面**：组件 8 个工具全部**经 PATH 上的 `kanyu` CLI 执行**（`kanyu introspect` /
   `kanyu data ...` / `kanyu render map` / `kanyu analysis ...`），与 `kanyu-mcp` 服务
   同一落点；组件不依赖宿主 `node_modules`——DSH 升级、npx 缓存清理均不影响组件工具。
   找不到 `kanyu.exe` 时，`kanyu-mcp`（MCP stdio 入口，同批安装）兜底。
2. **语义面**：`tooldef` 参数类型、`format.rs` 格式矩阵、`crs` 全库是参数的单一事实来源。
3. **GIS 模式 preset**（[`dsh/presets/kanyu-gis/`](../dsh/presets/kanyu-gis/)）：
   `agent.cordis.yml` 组合（模型三级路由 model-local → model-deepseek → model-local-8b、
   文件/进程服务、子代理路由、工具面、system-prompt 追加段）+ `preset.yml` 发现元数据 +
   `skills/kanyu-gis/SKILL.md` 领域技能（七域能力地图 + 四道门禁 + 仓库约定边界）。
4. **会签面**：组件迭代在 [AI_SYNC.md](../AI_SYNC.md) 会签簿登记；自我迭代只发生在
   Git 协作层（提交/PR + CI），运行时绝不自改内核（AI_SYNC §1.3）。

## 4. 当前状态（2026-08-18）

- **仓库侧**：`dsh/` 组件源首次完整入库（plugin 双半 + preset + 技能 + 示例 + 工具）；
  `host.js` 的 kanyu CLI 命令面与 v0.22.0 实测逐旗标对拍一致；
  `verify_preset.mjs --preset-dir dsh/presets` 退出码 0；
  `kanyu agents validate --code-repo` 退出码 0。
- **安装侧**：`bash dsh/sync-preset.sh` 已把修正后的 preset 同步至
  `~/.dsh/.agent-presets/kanyu-gis/` 并通过旁路校验（含 agent.cordis.yml 的
  `fallback` 顶层键 YAML 非法修复——收回 model-local 行内）。
- **crates 侧**：本轮零改动（组件层作业，四道门禁不适用）。
- **远端侧**：见 AI_SYNC.md 会签簿 2026-08-18 回记（主仓库推送与独立组件仓建仓结果）。

## 5. 维护契约

- 改 `dsh/**` → 重跑 `bash dsh/sync-preset.sh`（同步 + 旁路校验一体）；
  改 crates 能力面 → 先四道门禁（build→test→clippy→fmt），再同步组件文档。
- 单一事实来源：能力表以本手册 §1、`dsh/README.md` 与 `introspect.rs` 三者一致为准；
  新增组件工具时三处同时更新，并在会签簿登记。
- 许可边界：组件安装目录仅含本仓库 `dsh/` 源文件原文，不引入任何第三方二进制、
  位图或闭源脚本（对照 AGENTS.md「无冗余文件」铁律）。
