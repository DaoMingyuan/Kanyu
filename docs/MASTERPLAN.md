# 堪舆 (Kanyu) —— AI 原生地理空间操作系统

> **作者**：道明远  
> **版本**：v0.2.0（v0.1.0-alpha 经 2026-08-01 全网调研后修订，见第六部分裁决表）  
> **日期**：2026-08-01  
> **宣言**：以天地为盘，以数据为爻，以 AI 为神，重构地理空间之道。

---

## 目录

1. [设计哲学与视觉系统](#第一部分设计哲学与视觉系统)
2. [UI 架构层级](#第二部分ui-架构层级)
3. [数据与格式架构](#第三部分数据与格式架构)
4. [AI 自我迭代计划](#第四部分ai-自我迭代计划)
5. [技术实现路径](#第五部分技术实现路径)
6. [技术选型裁决与计划修订（v0.2.0）](#第六部分技术选型裁决与计划修订v020)
7. [附录：格式支持矩阵](#附录格式支持矩阵)

---

## 第一部分：设计哲学与视觉系统

### 1.1 设计原则

堪舆的设计语言源于东方美学中的"留白"与"气韵"，融合现代工具软件的精密感。核心原则：

| 原则 | 描述 |
|------|------|
| **气** (Qi) | 界面元素之间保持呼吸感，拒绝信息过载。密度控制在 0.6（每 100px² 不超过 6 个交互点）。 |
| **韵** (Yun) | 所有过渡动效遵循自然物理曲线（ease-out-cubic），时长 200ms–400ms，拒绝机械感。 |
| **清** (Qing) | 色彩饱和度低于 60%，主色调取自山水色谱——远黛青、雾白、墨黑。 |
| **变** (Bian) | 界面是活的。AI 根据用户当前任务自动重组面板布局，而非固定 Dock。 |

### 1.2 色彩系统

#### 亮色模式 (Light Mode) —— "晨山"

```
Background Primary:   #F7F5F2  (雾白，带 2% 暖黄，如晨雾中的宣纸)
Background Secondary: #FFFFFF  (纯白，用于卡片浮层)
Background Tertiary:  #EDEAE6  (浅灰，用于区分区块)

Surface Elevated:     #FFFFFF + shadow(0 4px 24px rgba(0,0,0,0.06))
Surface Pressed:      #E8E5E1

Text Primary:         #1A1A1A  (墨黑，对比度 18:1)
Text Secondary:       #5C5C5C  (深灰，对比度 8:1)
Text Tertiary:        #8A8A8A  (中灰，用于注释)
Text Disabled:        #BDBDBD

Accent Primary:       #2D6A5E  (远黛青，主品牌色)
Accent Secondary:     #C75B3A  (朱砂，用于警告/强调)
Accent Tertiary:      #D4A843  (琥珀，用于选中/高亮)

Border:               #E0DDD8  (极浅灰，1px)
Border Focus:         #2D6A5E  (远黛青，2px)

Map Canvas Background:#F0EDE8  (米白，降低屏幕眩光)
Grid Line:            #D9D5D0  (浅灰，虚线，0.5px)
```

#### 暗色模式 (Dark Mode) —— "夜观星"

```
Background Primary:   #121418  (墨夜，带 2% 蓝调，如深夜宣纸)
Background Secondary: #1A1D22  (深灰蓝，用于卡片)
Background Tertiary:  #23272E  (中灰蓝，用于区分区块)

Surface Elevated:     #1E2228 + shadow(0 8px 32px rgba(0,0,0,0.4))
Surface Pressed:      #2A2F36

Text Primary:         #E8E4DF  (月白，对比度 16:1)
Text Secondary:       #A0A5AB  (银灰，对比度 7:1)
Text Tertiary:        #6E737A  (暗灰，用于注释)
Text Disabled:        #4A4F55

Accent Primary:       #4DB8A8  (青玉，亮色主色的提亮版)
Accent Secondary:     #E07A5F  (珊瑚，朱砂的提亮版)
Accent Tertiary:      #E9C46A  (金珀，琥珀的提亮版)

Border:               #2A2F36  (深灰，1px)
Border Focus:         #4DB8A8  (青玉，2px)

Map Canvas Background:#0D0F12  (极暗，保护夜视)
Grid Line:            #2A2F36  (深灰，虚线，0.5px)
```

#### 语义色彩映射

| 语义 | 亮色 | 暗色 | 用途 |
|------|------|------|------|
| Success | `#2D6A5E` | `#4DB8A8` | 操作成功、拓扑正确 |
| Warning | `#B8860B` | `#E9C46A` | 坐标系警告、数据异常 |
| Error | `#C75B3A` | `#E07A5F` | 编辑冲突、渲染失败 |
| Info | `#4A7C9B` | `#7EB8DA` | AI 提示、系统通知 |
| Selection | `#D4A843` @ 20% | `#E9C46A` @ 25% | 要素选中填充 |
| Hover | `#2D6A5E` @ 8% | `#4DB8A8` @ 12% | 鼠标悬停背景 |

### 1.3 字体与排版

```
Display / 标题:    "Noto Serif SC", "Source Han Serif SC", serif
                   字重 700，用于品牌标识、大标题

UI / 界面:         "Inter", "Noto Sans SC", "PingFang SC", sans-serif
                   字重 400/500/600，用于所有 UI 文本

Mono / 等宽:       "JetBrains Mono", "Fira Code", monospace
                   字重 400，用于坐标、代码、属性值

Data / 数据:       "Inter", "Roboto Mono", monospace
                   字重 500，用于表格、统计数字
```

**字号层级**：

| Token | 大小 | 行高 | 字间距 | 用途 |
|-------|------|------|--------|------|
| `display-xl` | 32px | 40px | -0.02em | 启动页标题 |
| `display-lg` | 24px | 32px | -0.01em | 面板标题 |
| `heading` | 18px | 26px | 0 | 区块标题 |
| `body-lg` | 15px | 24px | 0 | 正文、属性值 |
| `body` | 13px | 20px | 0.01em | 标签、按钮 |
| `caption` | 11px | 16px | 0.02em | 注释、状态栏 |
| `data` | 12px | 16px | 0.04em | 坐标、ID |

### 1.4 图标系统

- **风格**：线性图标 (stroke 1.5px)，圆角端点 (round cap)，几何极简。
- **尺寸**：12px (紧凑)、16px (标准)、20px (工具栏)、24px (导航)、32px (空状态)。
- **动态**：图标支持微动效——悬停时线条从 1.5px 过渡到 2px，配合 4° 旋转（如刷新图标）。
- **语义图标**：
  - 山形图标 = 地形/高程
  - 水波纹 = 水系/流域
  - 罗盘 = 坐标/方向
  - 太极简化 = AI 智能模式

### 1.5 动效与交互

**全局缓动函数**：
```css
--ease-out-expo: cubic-bezier(0.16, 1, 0.3, 1);
--ease-in-out-sine: cubic-bezier(0.37, 0, 0.63, 1);
--ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);  /* 弹性，用于面板弹出 */
```

**关键动效**：

| 场景 | 时长 | 曲线 | 描述 |
|------|------|------|------|
| 面板展开 | 300ms | ease-out-expo | 高度从 0 展开，带 8px 延迟的透明度淡入 |
| 地图缩放 | 200ms | ease-in-out-sine | 非线性缩放，模拟惯性 |
| 要素选中 | 150ms | ease-spring | 选中框从中心向外弹性扩散 |
| 图层切换 | 400ms | ease-out-expo | 旧图层淡出 200ms，新图层淡入 200ms |
| AI 思考中 | 循环 2s | linear | 罗盘图标持续旋转，带呼吸透明度 |
| 数据加载 | 循环 1.5s | ease-in-out-sine | 进度条波浪动画 |

**减少动效 (Reduced Motion)**：所有动效时长压缩至 0ms 或 50ms，仅保留透明度变化。

---

## 第二部分：UI 架构层级

### 2.1 架构总览

堪舆的 UI 采用 **"壳-层-格" (Shell-Layer-Grid)** 三级架构：

```
┌─────────────────────────────────────────────────────────────┐
│  Shell Layer (应用壳)                                        │
│  ├─ TitleBar (自定义标题栏，含全局搜索、AI 状态、主题切换)    │
│  ├─ CommandPalette (命令面板，Cmd+K 唤起，AI 语义搜索)      │
│  ├─ NotificationCenter (通知中心，右下角堆叠)                │
│  └─ StatusBar (状态栏，底部，含坐标、比例尺、AI 就绪状态)     │
├─────────────────────────────────────────────────────────────┤
│  Layer System (层叠系统)                                     │
│  ├─ Base Layer: 地图画布 (GPU 渲染，占满剩余空间)             │
│  ├─ Overlay Layer: 临时浮层 (测量、标注、弹窗)               │
│  ├─ Panel Layer: 可停靠面板 (图层树、属性表、符号库)        │
│  ├─ Modal Layer: 模态对话框 (设置、导出、AI 对话)           │
│  └─ HUD Layer: 抬头显示 (指北针、比例尺、当前工具提示)      │
├─────────────────────────────────────────────────────────────┤
│  Grid System (栅格系统)                                      │
│  ├─ 12 列响应式栅格，面板宽度基于栅格倍数 (3/4/6/8/9)        │
│  ├─ 最小触控区域 44×44px (桌面端) / 56×56px (触屏)           │
│  └─ 间距系统: 4px 基值 (4/8/12/16/24/32/48/64)              │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Shell Layer (应用壳)

#### 2.2.1 TitleBar (标题栏)

高度 40px，无边框设计，集成于内容区上方。

```
┌────────────────────────────────────────────────────────────────┐
│  ◇ 堪舆      文件  编辑  视图  图层  分析  AI  帮助          🔍 │
│              ────────────────────────────────────────  🤖 ●  🌓 │
└────────────────────────────────────────────────────────────────┘
```

- **左侧**：品牌标识 (◇ 山形图标 + "堪舆")，点击返回首页/项目中心。
- **中部**：全局菜单 (文件/编辑/视图/图层/分析/AI/帮助)。
- **右侧**：
  - 🔍 全局搜索 (Cmd+K)：模糊匹配命令、图层、要素、AI 动作。
  - 🤖 AI 状态指示器：绿点 = 就绪，黄点 = 思考中，红点 = 错误。
  - 🌓 主题切换：点击切换亮/暗，跟随系统可配置。

#### 2.2.2 CommandPalette (命令面板)

全屏半透明遮罩 + 居中浮窗，类似 VS Code / Linear。

```
┌────────────────────────────────────────┐
│  > 输入命令或描述你的意图...            │
├────────────────────────────────────────┤
│  📁 最近打开                              │
│    项目A.kyu                    2h ago │
│    地形分析.kyu                 5h ago │
│  ──────────────────────────────────────│
│  🛠  工具                                │
│    缓冲区分析                    ⌘B    │
│    拓扑检查                      ⌘T    │
│  ──────────────────────────────────────│
│  🤖 AI 建议                              │
│    "根据当前选择生成 500m 缓冲区"         │
└────────────────────────────────────────┘
```

- **模式切换**：`>` 命令模式、`?` 帮助模式、`@` AI 对话模式、`#` 图层搜索。
- **AI 集成**：输入自然语言直接调用 MCP 工具，如 "提取所有建筑轮廓并导出 DWG"。

#### 2.2.3 StatusBar (状态栏)

高度 28px，背景 `Background Tertiary`，字体 `caption`。

```
┌────────────────────────────────────────────────────────────────┐
│  就绪  │  当前坐标: 116.3914°E, 39.9072°N  │  比例尺 1:5,000  │
│  内存: 1.2GB  │  渲染: 60fps  │  🤖 AI 引擎就绪  │  v0.1.0   │
└────────────────────────────────────────────────────────────────┘
```

- **左侧**：操作状态 + 进度条（若有后台任务）。
- **中部**：鼠标当前坐标（支持度分秒/十进制度/投影坐标切换）、当前比例尺。
- **右侧**：系统资源、AI 引擎状态、版本号。

### 2.3 Layer System (层叠系统)

#### 2.3.1 Base Layer —— 地图画布 (MapCanvas)

- **占据**：Shell 扣除 TitleBar 和 StatusBar 后的全部空间。
- **渲染**：GPU 原生 (bgfx/Vulkan)，非 Qt Widget 嵌入，确保帧率。
- **背景**：`Map Canvas Background` 色，而非纯白，降低视觉疲劳。
- **网格**：虚线经纬网/方里网，仅在缩放至特定级别显示，避免杂乱。
- **十字丝**：细线 (1px) + 中心点 (4px 圆)，颜色 `Accent Primary`。

#### 2.3.2 Panel Layer —— 可停靠面板 (Dockable Panels)

采用 **"磁吸边缘 + 浮动"** 双模式：

**边缘磁吸 (Docked)**：
- 左边缘：图层树 (LayerTree)、项目浏览器 (ProjectBrowser)
- 右边缘：属性表 (AttributeTable)、符号库 (SymbolLibrary)、AI 助手 (AIAssistant)
- 底部：时间轴 (Timeline，用于时态数据)、命令行 (Console)

**面板规范**：
- 默认宽度：320px (4 列栅格)
- 最小宽度：240px
- 最大宽度：480px
- 折叠态：仅显示 48px 图标栏
- 标题栏高度：36px，含拖拽手柄、折叠按钮、关闭按钮
- 内容区：圆角 8px，内边距 16px，滚动条自定义 (4px 宽，圆角)

**面板组件接入方式**：

每个面板是一个 **WASM 插件容器**，通过标准接口注册：

```rust
// 面板注册接口 (Rust trait)
pub trait KanyuPanel {
    fn id(&self) -> &str;                    // 唯一标识
    fn title(&self) -> String;              // 显示标题
    fn icon(&self) -> IconName;              // 16px 图标
    fn default_position(&self) -> PanelPosition; // Left/Right/Bottom/Float
    fn render(&mut self, ctx: &mut PanelCtx); // 渲染回调
    fn on_message(&mut self, msg: PanelMsg); // 接收系统消息
}

// 注册示例
kanyu::register_panel(Box::new(LayerTreePanel::new()));
```

#### 2.3.3 Overlay Layer —— 临时浮层

- **测量工具**：跟随鼠标的路径 + 实时距离/面积显示。
- **要素提示**：Hover 时显示要素属性卡片 (200ms 延迟，避免闪烁)。
- **上下文菜单**：右键点击地图/图层/要素时弹出，最大高度 400px，支持子菜单。

#### 2.3.4 Modal Layer —— 模态对话框

- **遮罩**：`Background Primary` @ 60% 透明度，带模糊 (backdrop-filter: blur(8px))。
- **对话框**：圆角 12px，阴影 `Surface Elevated`，最大宽度 640px。
- **类型**：
  - **确认框**：危险操作二次确认 (删除、覆盖)。
  - **表单框**：属性编辑、符号配置、导出设置。
  - **AI 对话框**：类 ChatGPT 界面，支持代码块、地图预览、文件拖拽。

#### 2.3.5 HUD Layer —— 抬头显示

- **指北针**：右上角，圆形，直径 48px，始终指向真北，支持点击快速旋转。
- **比例尺**：左下角，线段 + 文字，随缩放动态更新。
- **当前工具提示**：鼠标旁跟随，显示工具名称 + 快捷键 + 下一步操作提示。
- **AI 语音气泡**：AI 执行操作时，在相关区域显示简短说明 (3 秒后淡出)。

### 2.4 核心面板详解

#### 2.4.1 图层树 (LayerTreePanel)

```
┌─ 图层 ─────────────────────┐
│  [+] [🔍] [⚙]            │
│  ─────────────────────────│
│  ⬇ 📁 基础底图            │
│    ⬜ 影像底图             │
│    ⬜ 地形晕渲             │
│  ⬇ 📁 矢量数据            │
│    ☑ 建筑轮廓              │
│    ☑ 道路中心线            │
│    ☐ 水系                 │
│  ⬇ 📁 AI 生成层           │
│    ☑ 缓冲区分析结果        │
│  ─────────────────────────│
│  [+] 添加图层              │
└───────────────────────────┘
```

- **交互**：拖拽排序、右键菜单（缩放至图层、导出、属性、符号化）。
- **状态**：可见性 (☑/☐)、编辑状态 (✏️ 图标)、锁定 (🔒)、坐标系 (🌐)。
- **AI 集成**：AI 可直接在此面板插入新图层、调整顺序、修改可见性。

#### 2.4.2 属性表 (AttributeTable)

- **表格**：列头固定，行高 32px，斑马纹 (`Background Tertiary` 交替)。
- **编辑**：双击单元格进入编辑，支持批量编辑、字段计算器。
- **筛选**：列头下拉筛选，支持空间筛选（仅显示当前视图内要素）。
- **AI 集成**：AI 可直接查询 "找出所有高度大于 50m 的建筑" 并高亮表格行。

#### 2.4.3 AI 助手面板 (AIAssistantPanel)

```
┌─ AI 助手 ──────────────────┐
│  🤖 堪舆灵                   │
│  ─────────────────────────│
│  > 你好，道明远。当前项目    │
│    包含 3 个图层，共 12,450  │
│    个要素。我能为你做什么？  │
│  ─────────────────────────│
│  [📝 生成报告] [🗺 智能制图]│
│  [🔍 异常检测] [📐 批量编辑]│
│  ─────────────────────────│
│  > 提取所有道路交叉口...     │
│  > 将建筑图层导出为 DWG...   │
└───────────────────────────┘
```

- **会话历史**：支持多轮对话，上下文保留当前项目状态。
- **工具调用**：AI 调用 MCP 工具时，显示执行进度和结果预览。
- **代码生成**：生成的 WASM/Rust/Python 代码可直接预览、编译、热加载。

### 2.5 主题系统实现

**CSS 变量方案**：

```css
:root {
  /* 亮色默认 */
  --kanyu-bg-primary: #F7F5F2;
  --kanyu-bg-secondary: #FFFFFF;
  --kanyu-text-primary: #1A1A1A;
  --kanyu-accent: #2D6A5E;
  /* ... 全部语义 token */
}

[data-theme="dark"] {
  --kanyu-bg-primary: #121418;
  --kanyu-bg-secondary: #1A1D22;
  --kanyu-text-primary: #E8E4DF;
  --kanyu-accent: #4DB8A8;
  /* ... */
}
```

**切换机制**：
- 系统级：监听 `prefers-color-scheme`。
- 应用级：用户手动切换，写入本地存储，即时生效 (无刷新)。
- 画布级：地图渲染器同步切换配色方案（网格、选中框、标注颜色）。

---

## 第三部分：数据与格式架构

### 3.1 统一内存模型：堪舆数据库（KanyuDB，GeoArrow 兼容）

> **命名定版（裁决 #19，2026-08-03）**：堪舆的内存与存档模型统一命名为
> **堪舆数据库（KanyuDB，简称 KDB）**。内存形态为 GeoArrow RecordBatch
> 兼容布局（WKB 几何列 + `geoarrow.wkb` 扩展元数据 + 类型化属性列）；
> 存档形态为 `.kdb` 文件（Arrow IPC + `kanyu.*` 元数据，见 §3.5）。

堪舆的所有数据在内存中以 **堪舆数据库（GeoArrow RecordBatch 兼容）** 格式存在，实现零拷贝跨模块共享：

```
磁盘文件 → 解析器 → GeoArrow RecordBatch → 内核内存池
                                      ↓
                    ┌─────────────────┼─────────────────┐
                    ↓                 ↓                 ↓
              GPU 渲染器         AI 分析引擎         导出编码器
           (直接映射 SSBO)    (Arrow Flight)     (GeoArrow → 目标格式)
```

**核心优势**：
- **零序列化**：Python/Rust/C++ 模块共享同一块物理内存。
- **向量化**：SIMD 友好，批量操作比逐要素快 10–100 倍。
- **流式**：大文件无需全量加载，支持 RecordBatch 流式处理。

### 3.2 主流矢量格式支持矩阵

#### 3.2.1 导入 (Import)

| 格式 | 扩展名 | 驱动 | 状态 | 备注 |
|------|--------|------|------|------|
| **Shapefile** | .shp | GDAL/OGR | ✅ 完整 | 支持 .cpg 编码自动识别，修复常见损坏 |
| **GeoPackage** | .gpkg | GDAL/OGR | ✅ 完整 | 读写，支持 Spatialite 扩展 |
| **GeoJSON** | .geojson | GDAL/OGR | ✅ 完整 | 支持 RFC 7946，大文件流式解析 |
| **FlatGeobuf** | .fgb | native | ✅ 完整 | 首选内部交换格式，MMAP 零拷贝 |
| **GeoParquet** | .parquet | native | ✅ 完整 | 云原生，列式压缩 |
| **KML/KMZ** | .kml | GDAL/OGR | ✅ 完整 | 支持样式、网络链接 |
| **GML** | .gml | GDAL/OGR | ✅ 完整 | 支持 CityGML 子集 |
| **DWG** | .dwg | LibreDWG + ODA | ✅ 读写 | r13–r2018 读取，r13–r2000 写入原生 DWG；高版本通过 ODA 转换 |
| **DXF** | .dxf | GDAL/OGR + native | ✅ 完整 | 支持块、图层、标注样式映射 |
| **DGN** | .dgn | GDAL/OGR | ✅ 读取 | v7/v8 |
| **MapInfo** | .tab/.mif | GDAL/OGR | ✅ 完整 | |
| **PostGIS** | — | GDAL/OGR | ✅ 完整 | 读写，支持几何字段、空间索引 |
| **SpatiaLite** | .sqlite | GDAL/OGR | ✅ 完整 | |
| **WFS** | — | GDAL/OGR | ✅ 读取 | v1.1/v2.0 |
| **CSV/Excel** | .csv/.xlsx | native | ✅ 完整 | 自动坐标列识别 (lon/lat/x/y) |

#### 3.2.2 导出 (Export)

| 格式 | 状态 | 符号化保留 | 布局保留 | 备注 |
|------|------|-----------|---------|------|
| **Shapefile** | ✅ | 部分 | ❌ | 导出为 .sld 样式文件伴随 |
| **GeoPackage** | ✅ | 完整 | 部分 | 样式存入 `gpkg_contents` 扩展表 |
| **GeoJSON** | ✅ | 部分 | ❌ | 样式存入 `style` 属性 |
| **FlatGeobuf** | ✅ | 完整 | ❌ | 内部交换首选 |
| **GeoParquet** | ✅ | 完整 | ❌ | |
| **DWG** | ✅ | 完整 | ✅ | **核心能力**，见 3.3 节 |
| **DXF** | ✅ | 完整 | ✅ | 支持图层、块、线型、颜色映射 |
| **KML** | ✅ | 部分 | ❌ | 颜色、图标保留 |
| **PDF** | ✅ | 完整 | ✅ | 支持地图册、多页面 |
| **SVG** | ✅ | 完整 | ✅ | 矢量图形，支持交互热区 |
| **PostGIS** | ✅ | 完整 | ❌ | 样式存入元数据表 |

### 3.3 DWG/DXF 原生支持深度方案

DWG 是堪舆的**核心差异化能力**，采用三层策略确保完美兼容：

#### 3.3.1 读取架构

```
DWG 文件 → 格式嗅探 → 版本识别 → 分流处理
                              │
              ┌───────────────┼───────────────┐
              ↓               ↓               ↓
         r13–r2018      r2018–r2024      r2024+
         LibreDWG        ODA File Converter   RealDWG SDK
         (直接读取)      (后台转换 DXF)      (商业授权)
              │               │               │
              └───────────────┴───────────────┘
                              ↓
                    统一 DCEL 拓扑模型
                              ↓
                    GeoArrow RecordBatch
```

**LibreDWG 集成**：
- 直接链接 `libredwg.so`，读取 r13–r2018 所有实体类型。
- 支持解析：LINE, ARC, CIRCLE, POLYLINE, LWPOLYLINE, SPLINE, HATCH, INSERT (块), DIMENSION, MTEXT, ATTRIB。
- 图层、线型、颜色、线宽、块定义、外部参照 (XREF) 完整映射。

**ODA File Converter 集成**（高版本 DWG）：
- 后台静默调用 ODA 工具将高版本 DWG 转为 DXF。
- 用户无感知，进度在状态栏显示。

#### 3.3.2 写入架构

```
GeoArrow 数据 → 符号化引擎 → CAD 实体映射 → DWG/DXF 编码器
                              │
              ┌───────────────┼───────────────┐
              ↓               ↓               ↓
         几何映射         符号映射           布局映射
         (点→POINT)     (填充→HATCH)      (地图框→VIEWPORT)
         (线→LINE/LWPOLY) (线型→LTYPE)     (指北针→INSERT)
         (面→LWPOLYLINE) (颜色→ACI索引)    (比例尺→DIMENSION)
```

**符号 → CAD 映射表**：

| 堪舆符号 | CAD 实体 | 属性映射 |
|---------|---------|---------|
| 简单填充 (Solid) | HATCH, pattern = SOLID | 颜色 → ACI 索引 |
| 渐变填充 (Gradient) | HATCH, gradient | 起止颜色 → 渐变定义 |
| 虚线 (Dashed) | LWPOLYLINE + LTYPE | 线型比例、段长 |
| 标记符号 (Marker) | INSERT (块引用) | 块名、缩放、旋转 |
| 标注 (Label) | MTEXT / DIMENSION | 字体、字高、对齐方式 |
| 比例尺 (ScaleBar) | LINE + MTEXT | 线段长度 = 地图单位 |

**布局导出**：
- 堪舆的打印布局 (Layout) 直接映射为 CAD 的 Paper Space。
- 地图框 → VIEWPORT 实体，支持多比例尺视口。
- 图例 → 块引用 (INSERT) 阵列。
- 指北针、比例尺 → 标准块库。

#### 3.3.3 编辑互操作

- **双向编辑**：在堪舆中编辑的矢量数据可导出 DWG，在 AutoCAD 中修改后再导入，支持变更检测 (Change Detection)。
- **块编辑**：堪舆的符号库与 CAD 块库双向同步。
- **字段映射**：堪舆属性字段 ↔ CAD 扩展数据 (XData) / 属性 (Attribute)。

### 3.4 符号与样式系统

**符号定义格式** (JSON，AI 可直接读写)：

```json
{
  "symbol_id": "building_footprint",
  "name": "建筑轮廓",
  "type": "fill",
  "layers": [
    {
      "type": "fill",
      "paint": {
        "color": "#E8E4DF",
        "outline_color": "#5C5C5C",
        "outline_width": 0.5
      }
    },
    {
      "type": "marker",
      "filter": "height > 30",
      "paint": {
        "shape": "triangle",
        "size": 8,
        "color": "#C75B3A"
      }
    }
  ],
  "cad_mapping": {
    "fill": { "entity": "HATCH", "pattern": "SOLID" },
    "outline": { "entity": "LWPOLYLINE", "ltype": "CONTINUOUS" }
  }
}
```

### 3.5 堪舆数据库（.kdb）与堪舆工程（.kyu）文件格式

> 2026-08-03 定版（裁决 #19）。堪舆的自研格式双璧：**KDB 存数据，KYU 存工程**。

#### 3.5.1 堪舆数据库（.kdb，KanyuDB v1）

**设计**：KDB 文件 = **Arrow IPC 文件**，schema 元数据携带 `kanyu.*` 键：

| 键 | 值 |
|----|----|
| `kanyu:format` | 恒为 `"kdb"`（读取校验） |
| `kanyu:format_version` | 恒为 `"1"` |
| `kanyu:producer` | 如 `"kanyu-core 0.14.0"` |

- **类型保真**：直接序列化内存 RecordBatch（WKB 几何列 + `geoarrow.wkb` 扩展 +
  类型化属性列），读写不经 GeoJSON 中间层，Int64/Float64/Utf8/Boolean 原样往返。
- **开放互操作**：任何 Arrow 工具链（pyarrow、DuckDB、Polars）可直接读取——自研但不自闭。
- **转换**：CLI/MCP `export -f kdb` 与 `load .kdb` 已接入全格式转换矩阵
  （任意格式 ↔ kdb ↔ 任意格式）；v1 单批次。

**KDB v2（多图层容器，2026-08-28 落地）**：zip 容器（deflate 纯 Rust），
`manifest.json`（`kanyu:format_version="2"` + layers 清单）+ `layers/<名>.kdb`
（每层一个 v1 IPC，独立校验）——面向《不动产登记数据库标准》多表形态
（ZDJBXX/JZD/JZX… 单文件建库）。v1 完全兼容（魔数嗅探分流）；
`kanyu data kdb-pack` 多文件打包、`kanyu data info` 自动展开图层清单。

#### 3.5.2 堪舆工程（.kyu，KanyuProject v1）

**设计**：`.kyu` 为 JSON 工程清单：

```json
{
  "kanyu_project": 1,
  "name": "示例工程", "crs": "EPSG:4326",
  "viewport": [116.3, 39.8, 116.5, 40.0],
  "map_theme": "fixed_light",
  "layers": [
    {"id": "buildings", "source": "data/buildings.geojson", "visible": true, "style": null}
  ]
}
```

- 图层按**路径引用**外部数据源（不内嵌数据，数据源变动工程即跟随）；
- 保存界面状态：视口、地图色彩模式、图层可见性；
- 无来源的内存图层（分析产出）不入工程，保存时明确提示（先导出为数据文件）；
- 壳层「主页 → 打开工程…/保存工程」落地；版本升级校验（高版本拒绝并提示升级）。

---

## 第四部分：AI 自我迭代计划

### 4.1 元架构：堪舆灵 (Kanyu Spirit)

堪舆的 AI 不是外挂，而是系统的**元级意识层**，代号 **"堪舆灵"**。它同时是：
- **用户助手**：理解自然语言指令，执行空间操作。
- **系统医生**：监控系统健康，诊断性能瓶颈。
- **进化引擎**：生成代码、优化算法、扩展功能。

**架构分层** (受 GOLEM 元架构启发)：

```
┌─────────────────────────────────────────────┐
│  Meta-Level: 堪舆灵 (Kanyu Spirit)          │
│  ├─ 意图理解 (Intent Parser)                │
│  ├─ 空间推理 (Spatial Reasoner)              │
│  ├─ 代码生成 (Code Generator)                │
│  ├─ 性能诊断 (Performance Diagnostician)     │
│  └─ 伦理约束 (Ethical Guardrails)            │
├─────────────────────────────────────────────┤
│  Object-Level: 堪舆内核 (Kanyu Kernel)       │
│  ├─ 数据引擎、渲染引擎、编辑引擎             │
│  └─ 堪舆灵无法修改目标函数（安全约束）        │
├─────────────────────────────────────────────┤
│  Interface: MCP 神经接口                     │
│  └─ AI 与内核之间的唯一通信协议               │
└─────────────────────────────────────────────┘
```

### 4.2 MCP 神经接口规范

堪舆灵通过 **MCP (Model Context Protocol)** 与内核通信。所有内核能力暴露为工具：

#### 4.2.1 数据工具 (Data Tools)

```json
{
  "name": "kanyu.data.load",
  "description": "加载地理数据文件到内存",
  "input": {
    "path": "string",
    "format": "auto|shp|gpkg|fgb|dwg|dxf|...",
    "crs": "string?",
    "batch_size": "integer?"
  },
  "output": "GeoArrow RecordBatch Stream"
}
```

```json
{
  "name": "kanyu.data.query",
  "description": "对图层执行空间或属性查询",
  "input": {
    "layer_id": "string",
    "filter": "SQL-like expression | GeoJSON geometry | natural language",
    "spatial_predicate": "intersects|contains|within|touches|crosses"
  },
  "output": "GeoArrow RecordBatch"
}
```

```json
{
  "name": "kanyu.data.export",
  "description": "将图层导出为目标格式",
  "input": {
    "layer_id": "string",
    "path": "string",
    "format": "shp|gpkg|dwg|dxf|pdf|svg|...",
    "symbol_mapping": "boolean",
    "layout_id": "string?"
  }
}
```

#### 4.2.2 空间分析工具 (Analysis Tools)

```json
{
  "name": "kanyu.analysis.buffer",
  "description": "生成缓冲区",
  "input": { "layer_id": "string", "distance": "number", "segments": "integer?" }
}
```

```json
{
  "name": "kanyu.analysis.overlay",
  "description": "叠加分析 (Union/Intersect/Difference)",
  "input": { "target": "string", "overlay": "string", "operation": "string" }
}
```

```json
{
  "name": "kanyu.analysis.topology",
  "description": "拓扑规则检查与修复",
  "input": { "layer_id": "string", "rules": ["no_overlap", "no_gap", "must_be_covered_by"] }
}
```

#### 4.2.3 渲染工具 (Render Tools)

```json
{
  "name": "kanyu.render.symbolize",
  "description": "为图层应用符号化规则",
  "input": {
    "layer_id": "string",
    "style": "JSON symbol definition | natural language description"
  }
}
```

```json
{
  "name": "kanyu.render.camera",
  "description": "控制地图相机",
  "input": {
    "action": "fly_to|zoom_to_layer|set_rotation|set_tilt",
    "target": "GeoJSON Point | layer_id | bounding_box",
    "duration": "number"
  }
}
```

#### 4.2.4 系统工具 (System Tools)

```json
{
  "name": "kanyu.system.introspect",
  "description": "系统自省，返回当前架构、性能指标、源码树",
  "input": { "depth": "shallow|deep", "include_source": "boolean" }
}
```

```json
{
  "name": "kanyu.system.generate",
  "description": "生成优化代码或新功能模块",
  "input": {
    "task": "string",
    "target_language": "rust|cpp|wast|python",
    "target_module": "renderer|editor|analyzer|io",
    "test_cases": "string[]?"
  },
  "output": "source_code + wasm_binary + benchmark_report"
}
```

```json
{
  "name": "kanyu.system.hotload",
  "description": "热加载 WASM 插件",
  "input": { "wasm_path": "string", "sandbox": "strict|permissive" }
}
```

### 4.3 专属 CLI 框架：`kanyu`

堪舆灵不仅存在于 GUI，更有一个**原生的 Rust CLI**，作为系统的"脊髓"。

#### 4.3.1 命令结构

```bash
# 系统自省 —— AI 读取自身
kanyu introspect --format=json --include-source > system_schema.json

# 数据操作
kanyu data load ./buildings.shp --crs=EPSG:4326 --as=buildings
kanyu data query buildings --filter="height > 50" --output=high_rise.fgb
kanyu data export buildings --format=dwg --layout=layout_01 --out=./output.dwg

# 空间分析
kanyu analysis buffer roads --distance=500m --output=road_buffer
kanyu analysis topology buildings --rules=no_overlap

# AI 代码生成与热加载
kanyu codegen --prompt="优化百万级点云渲染管线，采用 GPU 实例化"               --target=rust               --module=renderer               --test=benchmark_pointcloud_1m
kanyu plugin build ./generated/plugin.rs --release
kanyu plugin load ./target/wasm32-unknown-unknown/release/plugin.wasm

# A/B 测试与回归
kanyu benchmark --plugin=plugin.wasm --baseline=current --metric=fps --duration=60s
kanyu regression test --suite=spatial_analysis --threshold=5%

# MCP 服务启动（供外部 AI 接入）
kanyu mcp serve --transport=stdio    # 本地 AI 助手
kanyu mcp serve --transport=sse --port=3000  # 远程 AI 代理

# 项目语义描述
kanyu agents init --project=./city_planning.kyu
kanyu agents validate              # 检查 AGENTS.md 完整性
```

#### 4.3.2 AGENTS.md 项目语义描述

每个堪舆项目根目录包含 `AGENTS.md`，它是 AI 理解项目的"罗盘"：

```markdown
# AGENTS.md —— 城市更新规划项目

## 项目元数据
- **name**: 朝阳区城市更新规划
- **crs**: EPSG:4526 (CGCS2000 / 3-degree Gauss-Kruger CM 117E)
- **extent**: [116.2, 39.8, 116.6, 40.0]
- **author**: 道明远
- **created**: 2026-08-01

## 数据层语义
| 图层 | 类型 | 语义 | 关键字段 | 业务规则 |
|------|------|------|---------|---------|
| buildings | Polygon | 建筑轮廓 | height, floor, usage | height > 0 为有效建筑 |
| roads | LineString | 道路中心线 | width, grade, name | width 单位米，grade 0-4 |
| parcels | Polygon | 用地地块 | land_use, area, owner | land_use 需符合国标分类 |
| water | Polygon | 水系 | type, name | type ∈ [river, lake, canal] |

## 坐标系统
- 所有数据采用 EPSG:4526。
- 高程采用 1985 国家高程基准，单位米。
- 禁止混用 WGS84 平面坐标。

## 业务规则
1. 建筑必须完全位于地块内 (within)。
2. 道路交叉口必须生成节点 (intersection)。
3. 水系两侧 15m 为生态红线，不可建设。
4. 建筑高度不得超过地块容积率对应上限。

## AI 工作流
- **制图**: 生成 1:5000 规划图，包含建筑、道路、水系三层。
- **分析**: 计算每个地块的容积率、建筑密度、绿地率。
- **导出**: 最终成果导出为 DWG (r2018) + PDF 双格式。

## 自定义工具
- `kanyu.tools.check_fsr`: 检查建筑高度是否符合容积率。
- `kanyu.tools.gen_section`: 沿道路中心线生成长剖面图。
```

AI 读取此文件后，无需人类解释即可理解：
- "height" 是建筑高度，单位米，必须大于 0。
- 导出 DWG 时知道要保留图层结构和符号映射。
- 执行分析时知道如何计算容积率。

### 4.4 自迭代闭环 (Self-Evolution Loop)

```
┌─────────────────────────────────────────────────────────────┐
│  Phase 1: 观察 (Observe)                                     │
│  ├─ 系统自省: kanyu introspect → 源码树 + 性能火焰图        │
│  ├─ 用户行为日志: 高频操作、卡顿点、错误堆栈                   │
│  └─ 基准测试: 自动运行 fps、内存、I/O 测试套件                │
├─────────────────────────────────────────────────────────────┤
│  Phase 2: 诊断 (Diagnose)                                    │
│  ├─ AI 分析瓶颈: "矢量渲染在 120 万要素时降至 18fps"         │
│  ├─ 根因定位: GPU 三角化阶段成为瓶颈，DrawCall 过多          │
│  └─ 方案生成: 提出 3 种优化策略 (实例化、LOD、瓦片化)          │
├─────────────────────────────────────────────────────────────┤
│  Phase 3: 编码 (Code)                                        │
│  ├─ AI 生成 Rust/C++ 代码: GPU 实例化渲染管线                 │
│  ├─ 编译为 WASM: wasmtime 编译，静态检查通过                 │
│  └─ 单元测试: AI 自动生成 50 组边界测试用例                    │
├─────────────────────────────────────────────────────────────┤
│  Phase 4: 验证 (Verify)                                      │
│  ├─ 沙箱测试: WASM 在隔离环境运行，确保无崩溃/泄漏            │
│  ├─ A/B 测试: 新旧算法并行，自动对比 fps、内存、正确性        │
│  └─ 回归测试: 全量测试套件通过，性能提升 > 阈值 (如 20%)       │
├─────────────────────────────────────────────────────────────┤
│  Phase 5: 部署 (Deploy)                                      │
│  ├─ 热加载: 通过 kanyu plugin load 替换运行中模块             │
│  ├─ 灰度发布: 10% 用户试用新算法，监控 24h                   │
│  └─ 合并: 通过审核后，AI 生成 PR，合并到主分支源码             │
├─────────────────────────────────────────────────────────────┤
│  Phase 6: 回溯 (Reflect)                                     │
│  ├─ 效果评估: 收集真实用户反馈，对比预期收益                   │
│  ├─ 知识沉淀: 将优化经验写入系统知识库 (RAG)                 │
│  └─ 模型微调: 基于本次迭代数据，微调代码生成模型               │
└─────────────────────────────────────────────────────────────┘
```

**安全约束 (不可逾越的边界)**：
- 堪舆灵**不能修改**自己的目标函数（伦理/安全约束）。
- 堪舆灵**不能绕过**沙箱直接操作文件系统（除通过 MCP 工具）。
- 所有代码生成必须经过人类审核才能合并到内核源码（WASM 热加载除外）。

### 4.5 WASM 技能系统：功能的可进化单元

堪舆的所有可扩展功能都是 **WASM 模块**，称为"技能" (Gene)。

#### 4.5.1 技能类型

| 类型 | 后缀 | 描述 | 示例 |
|------|------|------|------|
| `renderer` | `.ren.wasm` | 渲染器插件 | 自定义符号渲染、三维特效 |
| `analyzer` | `.ana.wasm` | 分析算法 | 自定义空间统计、机器学习推理 |
| `io` | `.io.wasm` | 数据格式驱动 | 专有格式解析器 |
| `panel` | `.ui.wasm` | UI 面板 | 行业专用工具面板 |
| `tool` | `.tool.wasm` | 地图工具 | 自定义测量、捕捉逻辑 |

#### 4.5.2 技能接口

```rust
// 标准技能接口
#[no_mangle]
pub extern "C" fn gene_init(ctx: &mut GeneCtx) -> GeneResult {
    // 注册能力
    ctx.register_capability(Capability::Render2D);
    ctx.register_capability(Capability::SpatialQuery);
    GeneResult::Ok
}

#[no_mangle]
pub extern "C" fn gene_render(ctx: &RenderCtx, batch: &GeoArrowBatch) {
    // 直接操作 GPU SSBO
    let ssbo = ctx.gpu_allocate(batch.geometry_size());
    ssbo.write(batch.geometry_bytes());
    ctx.draw_instanced(ssbo, batch.count());
}

#[no_mangle]
pub extern "C" fn gene_on_message(msg: &PanelMsg) -> Option<PanelAction> {
    // UI 面板响应消息
    match msg {
        PanelMsg::UserClick(x, y) => Some(PanelAction::QueryAt(x, y)),
        _ => None,
    }
}
```

#### 4.5.3 技能生命周期

```
AI 生成代码 → Rust/C++ 编译 → WASM 字节码
      ↓
堪舆灵沙箱验证 (wasmtime, 100ms 超时)
      ↓
  ┌───┴───┐
  ↓       ↓
通过    失败
  ↓       ↓
热加载   回滚 + 错误报告
  ↓
A/B 测试 (与旧版本并行)
  ↓
  ┌───┴───┐
  ↓       ↓
更优    更差
  ↓       ↓
激活    废弃
```

---

## 第五部分：技术实现路径

### 5.1 技术栈总览

| 层级 | 技术选型 | 语言 | 理由 |
|------|---------|------|------|
| **UI 壳层** | Qt6 Widgets (自定义样式) | C++20 | 矢量编辑交互的唯一成熟方案 |
| **渲染引擎** | bgfx + vg-renderer + Cesium Native | C++20 | 跨平台 GPU，成熟矢量渲染，专业三维 |
| **编辑引擎** | GPU SSBO + Compute Shader | C++20 | 实时几何编辑，零延迟 |
| **内核引擎** | geoarrow-rs + FlatGeobuf | Rust | 零拷贝内存，内存安全 |
| **空间分析** | Rust 并行 + GEOS (FFI) | Rust | 无锁并发，编译期安全 |
| **数据 I/O** | GDAL/OGR + LibreDWG | C++ | 格式兼容性最广 |
| **AI 接口** | MCP Server + Arrow Flight | Rust | 标准协议，高性能传输 |
| **CLI 框架** | 自研 `kanyu` | Rust | 类 Codex CLI，极速启动 |
| **插件系统** | WASM (wasmtime) | Rust/C++ | 沙箱安全，热重载 |
| **脚本层** | pybind11 + PyO3 | Python | 生态无敌，零拷贝绑定 |

### 5.2 五阶段实施计划

#### Phase 1：地基 —— 内核与数据 (Months 1–3)

**目标**：构建堪舆的数据心脏。

- [ ] 搭建 Rust 内核项目 (`kanyu-core`)，集成 `geoarrow-rs`。
- [ ] 实现 FlatGeobuf 读写器，支持 MMAP 零拷贝。
- [ ] 集成 GDAL/OGR 作为格式桥接层，支持 Shapefile/GeoPackage/GeoJSON。
- [ ] 集成 LibreDWG，实现 r13–r2018 DWG 读取。
- [ ] 设计并实现 GeoArrow 内存池管理器。
- [ ] 编写 `AGENTS.md` 解析器。
- [ ] **里程碑**：能命令行导入/导出所有主流格式，内存占用比 QGIS 低 50%。

#### Phase 2：视界 —— GPU 渲染 (Months 4–6)

**目标**：构建堪舆的眼睛。

- [ ] 搭建 bgfx 渲染管线，支持 Vulkan/Metal/D3D12。
- [ ] 实现 vg-renderer 集成，支持路径三角化、SDF 字体。
- [ ] 实现 GeoArrow → GPU SSBO 直接映射。
- [ ] 实现视窗裁剪 + LOD 瓦片系统。
- [ ] 集成 Cesium Native，实现 3D Tiles 加载。
- [ ] 实现亮/暗主题切换的地图渲染适配。
- [ ] **里程碑**：百万级矢量要素稳定 60fps，三维城市场景流畅。

#### Phase 3：手 —— 矢量编辑 (Months 7–9)

**目标**：构建堪舆的手。

- [ ] 搭建 Qt6 应用壳，实现自定义 TitleBar/StatusBar。
- [ ] 实现 GPU SSBO 编辑管线（顶点拖拽、拓扑捕捉）。
- [ ] 实现增量 DCEL 拓扑内核。
- [ ] 实现 Undo/Redo 系统（GeoArrow Delta 快照）。
- [ ] 实现图层树、属性表面板。
- [ ] 实现 DWG 符号 → 堪舆符号的双向映射。
- [ ] **里程碑**：能流畅编辑 10 万+ 要素的 DWG 文件，Undo 无延迟。

#### Phase 4：脑 —— AI 融合 (Months 10–12)

**目标**：赋予堪舆灵意识。

- [ ] 实现 MCP Server，暴露全部内核工具。
- [ ] 开发 `kanyu` CLI 框架（Rust）。
- [ ] 集成 LLM（本地 llama.cpp / 远程 API），实现自然语言理解。
- [ ] 实现 AI 代码生成 → WASM 编译 → 沙箱验证流水线。
- [ ] 实现 AI 助手面板（Qt Widget 嵌入）。
- [ ] 实现 AGENTS.md 自动生成与验证。
- [ ] **里程碑**：用户可用自然语言完成 "提取交叉口并导出 DWG" 全流程。

#### Phase 5：魂 —— 自迭代 (Months 13–18)

**目标**：堪舆开始自我进化。

- [ ] 实现系统自省 API（源码、性能、架构图输出）。
- [ ] 实现 A/B 测试框架（自动对比算法性能）。
- [ ] 实现 WASM 技能市场（Gene Marketplace），支持分享/下载插件。
- [ ] 训练领域微调模型（空间分析代码生成）。
- [ ] 实现知识库 RAG（积累优化经验）。
- [ ] **里程碑**：AI 自动发现并修复一个真实性能瓶颈，人类仅需审核。

### 5.3 性能基准

| 指标 | QGIS 基准 | 堪舆目标 | 提升倍数 |
|------|----------|---------|---------|
| 百万要素渲染 | 5–10 fps | 60 fps | 6–12x |
| 2GB 影像加载 | 30s | 3s | 10x |
| DWG (10MB) 导入 | 15s | 2s | 7.5x |
| 全国路网路径规划 | 分钟级 | 秒级 | 10x+ |
| 内存占用 (同等数据) | 100% | 50% | 2x |
| 启动时间 | 8s | 1s | 8x |

---

## 第六部分：技术选型裁决与计划修订（v0.2.0）

> 本部分为 **2026-08-01 全网调研**（crates.io / GitHub / NVD / 官方文档实时核实）后对
> 第一～五部分的修订。凡本部分与上文冲突之处，**以本部分为准**。上文保留作为设计
> 意图的历史记录。

### 6.1 裁决总表

| # | 原计划（v0.1.0-alpha） | 裁决 | 现行方案 | 理由（调研证据） |
|---|----------------------|------|---------|----------------|
| 1 | geoarrow-rs 内核 | **保留** | geoarrow-array 0.8 系 + arrow-rs | 0.8 系（2026-03）已支持 GeoArrow 0.2 规范，RecordBatch 流式为一等模型；风险是 0.x 高频升级，需在 workspace 根锁定版本 |
| 2 | WASM 插件（wasmtime） | **保留** | wasmtime 47 + WIT 组件模型 | 组件模型已稳定，fuel/epoch 可做资源配额；extism 更简单但类型表达力不足 |
| 3 | GDAL/OGR 进内核 | **替换** | 纯 Rust I/O 栈进内核；GDAL 降为可选插件 | GDAL 的 C 依赖地狱与 Windows 构建痛苦不可接受；flatgeobuf 6 / geoparquet 0.6 / shapefile 0.9 / kml 0.14 / dxf 0.6 均已成熟，geozero 0.15 可作统一抽象层；GML 是唯一无纯 Rust 答案的格式，划归 GDAL 旁挂插件 |
| 4 | bgfx + C++ 渲染 | **替换** | wgpu 30（纯 Rust） | bgfx 无一流 Rust 绑定、WebGPU 仅支持 Dawn Native；wgpu 月下载 320 万，egui/Zed/Bevy 同底座，且同一渲染内核可编译到 wasm 端 |
| 5 | Qt6 Widgets + C++ UI 壳 | **替换** | egui 0.35（+wgpu）为首选，slint/Tauri 备选 | 全 Rust 单一工具链，构建/分发成本骤降；AI-native 快速迭代不需要 C++ 工具链拖累；"矢量编辑交互的唯一成熟方案"这一前提在 2026 年已不成立 |
| 6 | GEOS (FFI) 空间分析 | **逐步淘汰** | geo 0.33 进内核；geos 留作可选插件 | geo 已内置布尔运算（i_overlay）、buffer/offset、完整 DE-9IM 谓词；GEOS 仍是 C++ 编译链且与 WASM 无缘 |
| 7 | LibreDWG 直接链接 | **降级** | LibreDWG 编译为 WASM，在 wasmtime 沙箱中**只读**运行 | ① GPLv3+ 与 MIT/Apache 双许可分发冲突；② 2026 年连续爆出多个 CVE（堆溢出/UAF），不可信输入必须沙箱化；③ 写仅 ≤r2004 可靠，R2010–R2018 写出有 CRC 错误 |
| 8 | DWG r13–r2018 原生写入 | **修正** | DWG 写 ≤r2004（LibreDWG 沙箱）；现代版本以 **DXF 导出**替代；ODA SDK 为商业可选插件 | 开源界对现代 DWG 写入无解；ODA File Converter 非会员仅限非商用且不可再分发，原计划"后台静默调用 ODA 转换"不可行 |
| 9 | MCP 工具名 `kanyu.data.load` | **修正** | `kanyu_data_load` | MCP 规范限制工具名为 `[a-zA-Z0-9_-]{1,64}`，不允许点号 |
| 10 | 自研/未定 MCP 实现 | **确定** | 官方 `rmcp` 3.x SDK | 官方背书、1800 万+ 下载；原生支持 streamable HTTP 与 SEP-1686 长任务（tasks/get\|result\|cancel）——耗时栅格/网络分析的一等载体 |
| 11 | SSE 远程传输 | **修正** | streamable HTTP | MCP 官方已用 streamable HTTP 取代旧 SSE |
| 12 | 许可未声明 | **确定** | **MIT OR Apache-2.0** 双许可 | 与 geo / geoarrow / arrow / rmcp 全兼容，Rust 生态默认 |
| 13 | proj 未明确 | **确定** | proj4rs（纯 Rust、WASM 兼容）进内核；`proj` C 绑定为可选 feature | proj4rs 月下载 30 万；PROJ 9 C 库不进默认构建 |
| 14 | 无基准评测 | **新增** | 首日接入 GeoAnalystBench 类基准 | GISclaw（arXiv 2603.26845）证明：schema 清晰度 + 领域知识注入 + 错误记忆决定 agent 成功率；单 agent ReAct 优于多 agent 管线 |
| 15 | GeoPackage/SpatiaLite/PostGIS/WFS 进内核 | **降级** | 可选 feature 插件（`sqlite-io`：rusqlite bundled；`net-io`：PostGIS/WFS 客户端） | rusqlite bundled 编译 C SQLite，违反内核零 C 依赖红线；服务类协议无文件语义，独立 feature 保持默认构建纯净 |
| 16 | 阶段顺序：Phase 2 渲染 → Phase 3 编辑 → Phase 4 AI/分析 | **调序** | Phase 1（地基）→ **分析内核**（geo crate：buffer/overlay/topology，CLI/MCP 先行）→ Phase 2 渲染 → Phase 3 编辑 → Phase 4 AI 融合 | CLI/MCP 是已交付的产品面，分析工具组可立即兑现总规 §4.2.2 的 MCP 承诺；UI 壳层（egui/wgpu）体量大、不阻塞内核能力沉淀。内核优先让 AI 代理用户尽早获得完整工作流 |
| 17 | MCP 工具 kanyu_render_symbolize 独立存在 | **合并** | 符号化并入 kanyu_render_map 的 style 参数（graduated/categorical 规则） | 避免两个工具做同一件事的冗余面；无 style 调用行为不变，向后兼容 |
| 18 | DWG 读取 = LibreDWG 编译 WASM 沙箱（裁决 #7 路线） | **改道** | **首选 acadrust 0.4（纯 Rust、MPL-2.0、R13–R2018 读写）原生进内核**（driver: acadrust，只读起步）；LibreDWG-wasm 降为备选（覆盖率不足时启用，GPL 制品独立可选分发） | 2026-08-03 调研：① @mlightcad/libredwg-web 为 emscripten 模块，wasmtime 跑不了；② LibreDWG→wasi-sdk 无人做过，autoconf 交叉编译中偏大工作量且 GPL 分发受限；③ acadrust 纯 Rust 内存安全（沙箱隔离的理由从"GPL+CVE"消失）、MPL-2.0 文件级弱 copyleft 与双许可兼容、已有 dwgdxf npm 包的 WASM 实证。风险：0.4.x 年轻、真实图纸覆盖率待实测——以 421 个真实 DWG 样本 spike 验证后再定稿 |
| 19 | 内存/存档模型沿用 GeoArrow 名称与外部格式 | **升级** | **命名定版为堪舆数据库（KanyuDB，KDB）**：内存形态 GeoArrow 兼容（WKB+`geoarrow.wkb`+类型化列）；新增自研存档格式 **.kdb**（Arrow IPC + `kanyu.*` 元数据，类型保真，任何 Arrow 工具链可读）与工程格式 **.kyu**（JSON 清单：图层引用+视口+地图色彩+可见性） | GeoArrow 是事实标准布局（继续兼容互操作），但产品级需要一个可命名的数据库与工程格式闭环：KDB 与内存模型同构零转换，KYU 让"工作现场"可存档可分享；转换经既有 export 矩阵，无新增依赖面 |
| 20 | 脚本层仅 pybind11/PyO3 远期设想 | **定版落地** | **kanyu-py（PyO3 扩展模块）+ ArcGIS .pyt 式 Python 工具箱**：GeoJSON 文本为跨语言契约；`kanyu toolbox list/run` 经 JSON over stdout 驱动用户 .py 工具箱 | 核心算法全部 Rust 实现保性能与正确性；Python 负责编排与行业工具编写（规划/测绘工具箱生态）；"属性面积图"等业务工具由 Python 快速迭代而内核稳定 |

### 6.2 竞品格局与差异化定位（调研摘要）

现有 GIS MCP 全是 Python/TS 薄壳：qgis_mcp（★1k，socket 控制 QGIS 桌面）、
gis-mcp（★174，92 个工具但 WKT 进出）、gdal-mcp、postgis-mcp 等。共性弱点：
几何以 WKT 字符串进出无类型化 IO、`execute_code` 任意代码执行是安全隐患、
无内省、无任务管理、无项目级上下文文件。

堪舆的可量化差异点：

1. **Rust 原生内核 + 一等公民 MCP**（无 GIL/GDAL 依赖地狱）。
2. **GeoArrow/GeoParquet 类型化 IO**，每个结果携带 CRS/单位/bbox 元数据。
3. **项目级 AGENTS.md 地理 profile**（图层语义/CRS/业务规则）——当前生态位空白，
   且与 Linux 基金会 Agentic AI Foundation 托管的 agents.md 标准完全同向。
4. **MCP tasks 长任务**（rmcp 原生 SEP-1686）——竞品类无人做到。
5. **确定性、可沙箱、可审计**——明确拒绝 `execute_code`。

### 6.3 修订后的技术栈总览（替换 §5.1）

| 层级 | 技术选型 | 语言 | 状态 |
|------|---------|------|------|
| **内核引擎** | geoarrow-array 0.8 + arrow-rs（v0.1 暂以 GeoJSON 为载体过渡） | Rust | 🚧 |
| **数据 I/O** | 原生：flatgeobuf 6 / geoparquet 0.6 / shapefile 0.9 / geojson / kml 0.14 / dxf 0.6；统一抽象 geozero 0.15 | Rust | 🚧 |
| **空间分析** | geo 0.33 + rstar 0.13 + robust + earcutr | Rust | 📋 |
| **投影** | proj4rs（可选 feature：proj-sys） | Rust | 📋 |
| **渲染引擎** | wgpu 30 + glyphon（SDF 文字）；MLT 列式瓦片原生支持 | Rust | 📋 |
| **UI 壳层** | egui 0.35（+wgpu）；备选 slint 1.17 / Tauri 2 | Rust | 📋 |
| **编辑引擎** | GPU SSBO + Compute Shader（wgpu compute） | Rust | 📋 |
| **AI 接口** | rmcp 3.x（stdio + streamable HTTP + MCP tasks） | Rust | ✅ v0.1 stdio |
| **CLI 框架** | clap 4 derive，名词优先子命令，全局 `--json` | Rust | ✅ |
| **插件系统** | wasmtime 47 + WIT 组件模型 | Rust | 📋 |
| **DWG 支持** | acadrust 原生读取（AC15 补丁层 + 编码层）+ dxf crate 原生读写；LibreDWG-wasm 备选 | Rust | ✅ 读取（六类几何） |
| **脚本层** | PyO3 绑定（Python SDK） | Python | 📋 |

### 6.4 修订后的五阶段实施计划（替换 §5.2）

#### Phase 1：地基 —— 内核与数据（Months 1–3）🚧 v0.1.0 已落地首块基石

- [x] Rust workspace 与内核骨架（kanyu-core / kanyu-cli / kanyu-mcp）。
- [x] 统一格式注册表与能力矩阵（18 种格式，代码即单一事实来源）。
- [x] AGENTS.md 解析器/校验器/模板生成器。
- [x] GeoJSON 原生加载与属性谓词查询。
- [x] MCP Server（rmcp，stdio）+ 6 个确定性工具 + 系统自省。
- [x] GeoArrow RecordBatch 内存模型替换 GeoJSON 载体（2026-08-02 完成：WKB 几何列 + 类型化属性列；Layer API 仅 collection() 改为返回拥有值，新增 batch() 零拷贝访问）。
- [x] **堪舆数据库 .kdb**（裁决 #19，2026-08-03：Arrow IPC + `kanyu.*` 元数据，RecordBatch 直通类型保真；读取/导出/全格式转换接入 CLI/MCP）。
- [x] **堪舆数据库 .kdb v2 多图层容器**（2026-08-28：zip 容器 manifest.json + layers/<名>.kdb 逐层 v1 IPC——面向不动产登记数据库标准多表形态单文件建库；v1 完全兼容（魔数嗅探），`kanyu data kdb-pack` 打包、`data info` 自动展开清单；真实数据三图层（CASS DXF/宗地 TXT/CASS .dat）建库-读取-渲染闭环验证）。
- [x] **堪舆工程 .kyu**（裁决 #19，2026-08-03：JSON 工程清单——图层引用/视口/地图色彩/可见性；壳层打开/保存落地）。
- [x] FlatGeobuf 原生读写（内部首选交换格式，列 schema 自动推断）。
- [x] GeoParquet 原生读写（云原生列式，WKB 几何编码 + geo 元数据）。
- [x] DXF 原生读写（CAD 互操作：POINT/LINE/LWPOLYLINE/CIRCLE/ARC 映射，图层→layer 属性）。
- [x] KML/KMZ 原生读写（Placemark 展平、ExtendedData 属性、含洞 Polygon；KMZ zip 容器 ✅）。
- [x] Shapefile 原生读写（Point/MultiPoint/Polyline/Polygon 含洞，dbase 属性类型化；写出单一几何类型三件套 + 字段名截断）。
- [x] CSV/TSV 坐标列自动识别（lon/lat/x/y/经度/纬度）与 CSV 导出闭环。
- [x] **里程碑（格式部分）**：主流矢量文件格式（shp/geojson/fgb/geoparquet/dxf/kml/csv）免 GDAL 读写 ✅（2026-08-02 达成；gpkg/spatialite/postgis/wfs 按裁决 #15 走可选 feature 路线，内存基准测试待 GeoArrow 迁移后统一进行）。

#### Phase 1.5：分析内核（裁决 #16 调序前置，v0.4.0 落地）

- [x] buffer 缓冲分析（geo crate Buffer，圆角分段可调，属性随行，2026-08-02）。
- [x] overlay 叠加分析（geo crate BooleanOps/i_overlay：union/intersection/difference/xor，2026-08-02）。
- [x] topology 拓扑检查（NoOverlap 面重叠检测，2026-08-02）。
- [x] 投影变换 reproject + 测地线度量 measure（proj4rs + geo geodesic，2026-08-02）。
- [x] sjoin 空间连接 + zonal_stats 分区统计（2026-08-02）。
- [x] MCP streamable HTTP 传输（远程 AI 代理接入，2026-08-02）。
- [x] MCP tasks 长任务（SEP-2663 协议级任务：白名单分析工具 `"task": true` 异步化，rmcp TaskManager；2026-08-02）。
- [x] **QGIS 核心算法移植**（geoprocess 模块：dissolve/simplify/centroid/convex_hull/delete_holes/explode/stats（亩/公顷），语义对齐 QGIS Processing，2026-08-03）。
- [x] **QGIS 核心算法移植第二/三批**（第二批 2026-08-11：boundary/bounding_boxes/merge/extract_by_attribute/extract_by_location/count_points_in_polygon/field_stats/mean_coordinates；第三批 2026-08-11：距离矩阵/最近邻分析/多环缓冲区/按字段缓冲区/分割矢量图层/添加几何属性/创建网格/沿线等距点/凹包/定向最小包络矩形；壳层工具箱同步扩编至 37 工具）。
- [x] **宗地 TXT 格式**（移植自堪舆工具箱 txt_feature.py：双段解析/校验/写出/质检；简单点表回退；注册表第 19 格式，2026-08-03）。
- [x] **不动产制图引擎与宗地图出图**（cartography：勘测定界图注记契约移植——边长中点法线零沿线偏移/界址点号角平分线朝外/旋转矩形 SAT 避让/least_bad 诚实标记；kanyu-render `parcelmap`：GB/T 42547-2023 图 L.3 版式宗地图（界址点符号/红界址线/J 点号/边长注记/界址点坐标表/分式注记/整百比例尺/「北」指北针/签注栏）SVG+PNG 双通道；`kanyu render parcel-map`；真实宗地 CASS DXF 渲染验证 18 注记 0 压盖，2026-08-28）。
- [x] **南方 CASS 联动**（cass 模块：CASS 坐标数据文件 .dat 读写（CASS 标准轴序 点号,编码,Y东,X北[,H]，编码列保留；注册表第 20 格式，CLI/MCP/壳层/Python/工具箱五处导出分派全接入）；宗地成果 CASS 兼容 DXF 导出（AC1024：ZD/JZX/JZD/ZJ 分层 + SOUTH 编码 XDATA 302001/302002），`kanyu render parcel-dxf`；真实 CASS DXF「读取→宗地图渲染→CASS DXF 再导出→再读取」闭环验证，2026-08-28）。
- [x] **宗海界址图出图**（kanyu-render `seamap`：GB/T 42547-2023 图 L.7 版式 A4 横——自适应经纬网图廓（度分秒注记）、宗海图斑/红界址线/点号边长注记（cartography 排版）、右侧界址点编号及坐标表（北纬|东经，度分秒 3 位小数，EPSG:4490 反算）、网格签注表；`kanyu render sea-boundary-map`；真实宗地代理渲染 DMS 坐标表与金样逐行一致，2026-08-28）。
- [x] **宗海位置图/平面布置图出图**（`SeaMapKind` 图种分派同管线：图 L.6/L.8 版式——经纬网图廓+图斑+界址点符号+签注表+下中比例尺，无坐标表注记；`kanyu render sea-location-map`/`sea-layout-map`（共享 SeaMapArgs 三变体）；真实代理渲染目检对齐金样 160/162，2026-08-28）。
- [x] **用岛范围图/设施布置图出图**（kanyu-render `islandmap`（复用 seamap 经纬网/坐标表体系）：图 L.9 版式——用岛代码行/经纬网/图斑/罗盘指北针（四向星形 N/E/W/S）/图例框/签注表九行/界址点编号及坐标表（DMS）+用岛面积行；图 L.10 版式——设施黄色图斑+编号+一览表（合计行）+图例；`kanyu render island-range-map`/`island-facility-map`（共享 IslandMapArgs）；真实代理渲染 DMS 坐标表与用岛面积（3483.61）同金样 163/164 逐值一致，2026-08-28）。
- [x] **kanyu-py + Python 工具箱**（裁决 #20：PyO3 扩展模块全量暴露内核；ArcGIS .pyt 式 Toolbox/Tool/Param 约定，`kanyu toolbox list/run` 驱动，2026-08-03）。

#### Phase 2：视界 —— GPU 渲染（Months 4–6）

- [x] 离屏渲染 render_map（tiny-skia PNG + SVG 双通道，晨山/夜观星主题，2026-08-02）。
- [x] 属性驱动符号化（graduated/categorical 样式规则，并入 render_map style 参数——裁决 #17，2026-08-02）。
- [x] egui 壳层 MVP（kanyu-shell：eframe/wgpu 原生窗口，TitleBar/图层面板/MapCanvas/StatusBar，晨山/夜观星双主题，视口缩放平移（render 显式视口参数），`--screenshot` 截图验证，2026-08-03）。
- [ ] wgpu 渲染管线（Vulkan/Metal/DX12/GL 一套代码；交互实时渲染，随 kanyu-shell）。
- [ ] GeoArrow → SSBO 直通映射；视窗裁剪 + LOD 瓦片。
- [ ] glyphon SDF 文字；亮/暗主题渲染适配（晨山/夜观星色彩系统）。
- [ ] MLT（MapLibre Tile）列式瓦片支持——与 GeoArrow 内存模型同构。
- [ ] **里程碑**：百万级矢量要素稳定 60fps。

#### Phase 3：手 —— 矢量编辑（Months 7–9）

- [x] egui 应用壳（磁吸面板/多地图视图/属性表查看与字段计算，v0.16–v0.18 分批落地，2026-08-11）。
- [ ] GPU SSBO 编辑管线（顶点拖拽、拓扑捕捉）。
- [ ] 增量 DCEL 拓扑内核；Undo/Redo（GeoArrow Delta 快照）。
- [ ] DXF 符号 ↔ 堪舆符号双向映射。
- [ ] **里程碑**：流畅编辑 10 万+ 要素 DXF 文件，Undo 无延迟。

#### Phase 4：脑 —— AI 融合（Months 10–12）

- [ ] MCP streamable HTTP 传输 + MCP tasks 长任务（栅格/网络分析）。
- [ ] MCP resources（`layer://`、`crs://EPSG/4326`）与 prompts（常用分析流）。
- [ ] 分析工具组落地：buffer / overlay / topology / sjoin / zonal_stats（geo crate）。
- [ ] LLM 接入（本地 llama.cpp / 远程 API）；AI 助手面板。
- [ ] GeoAnalystBench 类基准接入 CI，拒绝 execute_code 的安全叙事可证伪。
- [ ] **里程碑**：自然语言完成"提取交叉口并导出 DXF"全流程。

#### Phase 5：魂 —— 自迭代（Months 13–18）

> **迭代边界（2026-08-03 入规，见仓库根 AI_SYNC.md §1.3）**：堪舆灵不在用户运行时
> 直接修改内核；自我迭代发生在 **GitHub 协作层**——所有变更经提交/PR + CI + 审核
> 进入仓库，WASM 技能热加载是唯一免审核通道。全体迭代者（人类与 AI）经
> AI_SYNC.md 会签簿联动。

- [x] wasmtime + WIT 技能宿主（加载/校验/fuel 沙箱执行，2026-08-03）。
- [x] DWG 原生读取（acadrust，裁决 #18；421 样本 spike → 覆盖率报告 → 进内核）。**Spike 结论（2026-08-03，143 个真实 R2000 样本 / 52 万实体）**：acadrust 0.4.1 开箱 0%（AC15 objects 段定位推断缺陷：AuxHeader 在 Handles 之后时 size 为负）；手工按 ODA 约定以 [Classes_end, Handles_start) 定位后 **143/143 打开、521,750 实体 100% 读出、0 panic、123ms/文件**；可映射六类几何 48.0%，INSERT 22.4%、MTEXT/TEXT 25.9%、HATCH 3.6%；中文双乱码形态（GBK 未按 codepage 转码 + MIF \U+XXXX 未解码）已定位修法。**定稿：acadrust + 自持补丁层（locator workaround ~40 行 + 编码层后处理）进内核，read: Partial（六类几何，INSERT/HATCH/MTEXT 跳过+计数 📋），向上游提 issue/PR。**（2026-08-03 已落地 crates/kanyu-core/src/dwg.rs）
- [x] DWG 原生读取进内核（acadrust + 自持补丁层：AC15 locator workaround + GBK/MIF 编码层；六类几何 + TEXT/MTEXT 标注要素化 + ELLIPSE 近似；INSERT/HATCH/SPLINE 跳过+计数 📋；R2018+ 待样本复测，2026-08-03）。
- [ ] libredwg-wasm 技能（备选路线：覆盖率不足时启用，wasi-sdk + wasi-virt，GPL 制品独立分发）。
- [x] MCP 热加载接线（kanyu_system_hotload 实质化，2026-08-03）。
- [ ] AI 代码生成 → WASM 编译 → 沙箱验证 → A/B 测试流水线。
- [ ] 技能市场（Gene Marketplace）；知识库 RAG。
- [ ] **里程碑**：AI 自动发现并修复真实性能瓶颈，人类仅需审核。

### 6.5 v0.1.0 落地状态速查

| 总规条目 | 状态 | 位置 |
|---------|------|------|
| §3.2 格式矩阵 | ✅ 代码化（FormatRegistry，18 格式） | `crates/kanyu-core/src/format.rs` |
| §4.2 MCP 工具（数据组） | ✅ 3/4 组首工具落地（命名修正为下划线式） | `crates/kanyu-mcp/src/server.rs` |
| §4.2.2 分析工具组 | ✅ 3/3 首工具（buffer/overlay/topology，geo crate）+ measure（测地线度量） | `crates/kanyu-core/src/analysis.rs`、`crates/kanyu-core/src/crs.rs` |
| §4.2.3 渲染工具组 | ✅ 首个工具 render_map（离屏 PNG/SVG，晨山/夜观星主题） | `crates/kanyu-render/src/lib.rs` |
| §4.3.1 CLI | ✅ data/analysis/introspect/agents/mcp 五组；plugin/benchmark 📋 | `crates/kanyu-cli/` |
| §4.3.2 AGENTS.md | ✅ 解析/校验/模板 | `crates/kanyu-core/src/agents.rs` |
| §4.4 自省（Phase 1 观察） | ✅ `kanyu.system.introspect` | `crates/kanyu-core/src/introspect.rs` |
| §5.2 Phase 1 | 🚧 首块基石完成 | 见 §6.4 |

---

## 附录：格式支持矩阵

### A.1 矢量格式详细支持

| 格式 | 读 | 写 | 编辑 | 符号 | 布局 | 备注 |
|------|----|----|------|------|------|------|
| ESRI Shapefile | ✅ | ✅ | ✅ | ⚠️ | ❌ | 伴随 .sld 样式文件 |
| GeoPackage | ✅ | ✅ | ✅ | ✅ | ⚠️ | 样式存扩展表 |
| GeoJSON | ✅ | ✅ | ✅ | ⚠️ | ❌ | 样式存属性 |
| FlatGeobuf | ✅ | ✅ | ✅ | ✅ | ❌ | 内部首选格式 |
| GeoParquet | ✅ | ✅ | ✅ | ✅ | ❌ | 云原生 |
| DWG (r13–r2018) | ✅ | ✅ | ✅ | ✅ | ✅ | LibreDWG |
| DWG (r2018+) | ✅ | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ODA 转换 |
| DXF | ✅ | ✅ | ✅ | ✅ | ✅ | 完整块/图层支持 |
| DGN | ✅ | ⚠️ | ❌ | ❌ | ❌ | v7/v8 读取 |
| MapInfo TAB | ✅ | ✅ | ✅ | ⚠️ | ❌ | |
| PostGIS | ✅ | ✅ | ✅ | ✅ | ❌ | 样式存元数据 |
| SpatiaLite | ✅ | ✅ | ✅ | ✅ | ❌ | |
| WFS | ✅ | ❌ | ❌ | ❌ | ❌ | v1.1/v2.0 |
| KML/KMZ | ✅ | ✅ | ⚠️ | ⚠️ | ❌ | 网络链接支持 |
| GML | ✅ | ⚠️ | ❌ | ❌ | ❌ | CityGML 子集 |
| CSV/Excel | ✅ | ✅ | ✅ | ❌ | ❌ | 自动坐标识别 |
| PDF | ⚠️ | ✅ | ❌ | ✅ | ✅ | 导出为地图册 |
| SVG | ⚠️ | ✅ | ❌ | ✅ | ✅ | 矢量图形导出 |

### A.2 栅格/三维格式

| 格式 | 读 | 写 | 备注 |
|------|----|----|------|
| GeoTIFF / COG | ✅ | ✅ | 云优化 GeoTIFF |
| JPEG2000 | ✅ | ❌ | |
| PNG / JPEG | ✅ | ✅ | 带世界文件 |
| MrSID / ECW | ✅ | ❌ | 需插件 |
| HDF5 / NetCDF | ✅ | ⚠️ | 多维数据 |
| 3D Tiles | ✅ | ⚠️ | Cesium Native |
| glTF / glb | ✅ | ✅ | 三维模型 |
| LAS / LAZ | ✅ | ⚠️ | 点云，PDAL 集成 |
| IFC | ⚠️ | ❌ | BIM 导入 |

---

> **"天行健，君子以自强不息。"**  
> 堪舆不仅是一个 GIS 系统，更是一个活的地理空间生命体。它以 GeoArrow 为血液，以 GPU 为眼，以 AI 为魂，以 WASM 为技能，在道明远的指引下，不断自我迭代，直至成为地理空间领域的通用人工智能。

---
*文档版本: v0.2.0（2026-08-01 调研驱动修订，见第六部分） | 作者: 道明远 | 项目: 堪舆 (Kanyu)*
