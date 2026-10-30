# UI 规范

视觉参考是 [design/neko.html](design/neko.html)（Rayon · Material 3 Expressive 的画廊外壳），
外壳骨架沿用 [catussh](https://github.com/mizorewww/catussh)（标题栏 + 左侧导航 + 页面内容），
组件一律来自 `gpui-kit-material`。三者冲突时的优先级：

1. `gpui-kit-material` 的 M3 token 与组件行为（行为正确优先于看起来像）
2. 本文件（adash 自己的布局与页面结构）
3. `neko.html`（具体的疏密、圆角、配色的感觉）

## 1. 设计基调

- **M3 Expressive**：大圆角、大留白、克制的颜色；只有「当前状态」和「要用户决策的东西」上色。
- **一屏一件事**：主页 = 对话 + 概览；资源页 = 清单；流程页 = 图。不要在每个页面塞满所有信息。
- **数据可信**：任何数字都要能追到来源（刷新时间、地域、计划 id）。没有数据的格子显示 `—`，不猜。
- **代理不抢镜**：代理的中间过程折叠成一行状态，用户需要时才展开。

## 2. Token 用法（禁止硬编码）

| 用途 | 写法 |
| --- | --- |
| 页面底色 | `color(ColorRole::SURFACE)` |
| 卡片 / 面板 | `color(ColorRole::SURFACE_CONTAINER_LOW)`、`SURFACE_CONTAINER`、`SURFACE_CONTAINER_HIGH` |
| 正文 / 次要文字 | `ON_SURFACE` / `ON_SURFACE_VARIANT` |
| 主操作 | `PRIMARY` + `ON_PRIMARY`；容器式按钮 `PRIMARY_CONTAINER` |
| 选中态（导航、筛选） | `SECONDARY_CONTAINER` + `ON_SECONDARY_CONTAINER` |
| 分隔线 | `OUTLINE_VARIANT` |
| 危险（删除/释放/超支） | `ERROR` + `ON_ERROR`；超支预警用 `ERROR_CONTAINER` |
| 运行中 / 异常 | `TERTIARY` / `ERROR`，配 `Badge` + 文案（不只靠颜色） |

形状与字号只用 `tp`/`shape` token：`rounded(rem(12.))` 起、卡片 `rem(16.)`、对话气泡 `rem(20.)`、
下拉与输入框 `rem(12.)`；字号用 `TEXT_BODY`、`TEXT_LABEL`、`TEXT_TITLE`、`TEXT_HEADLINE`，不写裸 px 字号。
间距用 4 的倍数（`rem(4/8/12/16/24/32.)`）。

## 3. 外壳（所有页面共用）

```
┌──────────────────────────────────────────────────────────────────────┐
│ TitleBar  adash · 当前账户/项目 · Mock|Live 徽标             — □ ×   │ 56px
├───────────┬──────────────────────────────────────────────────────────┤
│ 对话      │                                                          │
│ 云资源    │                     页面内容                             │
│ AI 流程   │                                                          │
│ ─────     │                                                          │
│ 连接主机  │   ← 打开过的会话；下面固定是 身份 / 片段 / 端口转发        │
│   web-1   │                                                          │
│ 身份      │                                                          │
│ 片段      │                                                          │
│ 端口转发  │                                                          │
│           │                                                          │
│ 设置      │                                                          │
└───────────┴──────────────────────────────────────────────────────────┘
```

- 左侧导航：宽 232px，`NavigationRail`/自绘等价物，选中项 `SECONDARY_CONTAINER` 胶囊；
  图标用 Material Symbols（`forum` / `dns` / `account_tree` / `settings`）。
- 分组顺序固定：三个产品页 → `连接主机`（已打开的会话在前，`身份 / 片段 / 端口转发` 固定在后）
  → 底部 `设置`。没有会话时，`连接主机` 显示一行提示，不占首屏。
- 窗口最小 1100×720；宽度 < 1180 时导航收成 72px 图标栏（`NavigationRail` 折叠形态）。
- 标题栏右侧永远显示 `Mock` 或 `Live` 徽标：这是防「以为在演示、其实在花钱」的最后一道提醒。
- 全局 `Snackbar` 在底部中央：只用于「已复制」「已刷新」「计划已取消」这类一次性反馈。

## 4. 对话（主页）

两栏，左 1.5 : 右 1，宽 < 1280 时上下堆叠（概览在上，对话在下）。

**左：代理对话**

- 顶部一行：项目选择（`ExposedDropdown`）+ 「新对话」`IconButton`。
- 消息流：用户消息右对齐 `PRIMARY_CONTAINER`；代理消息左对齐、无气泡背景，正文 + 折叠的工具调用。
- 工具调用折叠成一行：`▸ 查询 12 个资源 · cn-bj2、cn-sh2`，展开是等宽小字表格。
- 计划卡片（`MaterialCard`，`SURFACE_CONTAINER_HIGH`，圆角 20）：
  标题「部署计划」+ 资源清单表 + 月成本（大字）+ 风险行 + `取消` / `确认并执行`。
  含删除/释放时标题前加 `ERROR` 色警示图标，主按钮文案变「确认停机」。
- 输入区固定在底部：`TextField` 多行 + 发送 `IconButton`；上方一行快捷「许愿」建议 `SuggestionChip`。
- 流式输出时输入区禁用，右侧显示 `CircularProgress`。

**右：账户概览**

| 卡片 | 内容 | 组件 |
| --- | --- | --- |
| 余额 | 可用余额（大字）+ 上次刷新时间 | `MaterialCard` |
| 本月花费 | 金额 + 环比 + 迷你条形 | `MaterialCard` + 自绘条 |
| 预算 | 已用/预算 + `LinearProgress`，超 80% 警告色 | `MaterialCard` |
| 资源 | 按产品计数（主机 3 · EIP 2 · 云盘 5…），点击跳资源页并带上筛选 | `ListItem` × n |
| 待处理 | 闲置/孤儿资源条数，点击生成优化计划 | `MaterialCard` + `Badge` |

概览数据每 60s 或每次代理 read 后刷新；刷新时数字淡入（`motion` token），不整块闪白。

## 5. 云资源

- 顶部 `TopAppBar`：标题「云资源」+ 刷新 + 搜索 `SearchField` + 视图切换（表格/卡片）。
- 筛选行：产品 `FilterChip`（主机/EIP/云盘/UDB/Redis/US3/ULB）、地域下拉、项目下拉、状态下拉；
  筛选条件写进 URL 等价的 `Page::Resources` 状态，从概览点进来时可直接带条件。
- 主体：`ListItem` 行（虚拟化滚动）。每行：状态点 + 名称（粗）+ 产品/规格 + 地域 + 项目 chip + 月成本。
- 点行 → 右侧 `MaterialSideSheet` 详情：id（可复制）、状态、规格、内网/公网 IP、绑定关系、
  所属项目、创建时间、月成本；底部按钮：`查看流程图`、`打开 UCloud 控制台`、`停机`（mutate 走计划）。
- 空态：一张卡「这个账户还没有资源」，两个按钮：`让代理帮我建一个` / `查看示例流程`。

## 6. AI 流程（n8n 风格，只读）

```
┌ 项目：shop-api        [上次部署 12:04]  [▶ 回放]   [适应窗口] ┐
│                                                              │
│   ┌─────────┐        ┌──────────┐        ┌────────────┐      │
│   │ GitHub  │──────▶ │ UHost ×2 │──────▶ │ ULB        │      │
│   │ repo    │        │ cn-bj2   │        │ shop-api   │      │
│   └─────────┘        └────┬─────┘        └─────┬──────┘      │
│                            │                   │             │
│                       ┌────▼─────┐        ┌────▼──────┐      │
│                       │ UDB MySQL│        │ EIP 1.2.3 │      │
│                       └──────────┘        └───────────┘      │
└──────────────────────────────────────────────────────────────┘
```

- 画布：自绘 `widgets/flow_canvas.rs`（kit 没有图组件）。分层 DAG 布局：左→右，
  列 = 拓扑层级，同层竖直居中；缩放 0.5–2，拖动平移，双击节点居中。
- 节点（`MaterialCard`，圆角 16，宽 180）：
  - 图标（产品符号或外部服务符号）+ 名称 + 一行副标题（地域 / 规格 / 状态）。
  - 状态用左侧 3px 色条 + `Badge`：运行 `TERTIARY`、停止 `OUTLINE`、异常 `ERROR`。
  - 选中：`SECONDARY_CONTAINER` 描边 2px；点击打开下方详情条。
- 连线：正交折线（圆角 8 拐角），默认 `OUTLINE_VARIANT`；`▶ 回放` 时按时间顺序逐条高亮为 `PRIMARY`，
  并让对应节点脉冲一次（`motion::expressive`，尊重 reduced motion）。
- 顶部工具条：项目下拉、时间范围、`▶ 回放` / `⏸`、`适应窗口`、导出（SVG/PNG）。
- 空态：说明「这个项目还没有部署记录」，按钮 `让代理部署这个项目`（跳回对话页并预填）。

## 7. 组件映射（先查 kit，再写薄组合）

| 需求 | 用什么 |
| --- | --- |
| 窗口标题栏 | `TitleBar` + `CaptionLabels`（照 catussh 自绘） |
| 侧栏导航 | `NavigationRail`（折叠）/ `NavigationDrawer` |
| 页面标题栏 | `TopAppBar` |
| 卡片、列表行 | `MaterialCard`、`ListItem` |
| 搜索、筛选、项目选择 | `SearchField`、`FilterChip`/`AssistChip`、`ExposedDropdown` |
| 确认与详情 | `MaterialDialog`、`MaterialSideSheet`、`MaterialBottomSheet`（窄屏） |
| 进度、状态 | `LinearProgress`、`CircularProgress`、`Badge`、`LoadingIndicator` |
| 提示 | `Snackbar`、`MaterialTooltip` |
| 按钮 | `MaterialButton`（`filled`/`tonal`/`outlined`/`text`/`elevated`）、`IconButton`、`Fab` |
| 概览卡片、资源行、流程节点、连线层 | **本仓库** `crates/adash-app/src/widgets/`（薄组合） |

不允许：为「看起来更 M3」而 fork kit、手写带颜色常量的控件、复制 kit 的交互逻辑。

## 8. 文案

- 全部走 `crates/adash-app/src/i18n.rs`；中文优先，英文预留。
- 语气：短句、动词开头、不用感叹号。按钮说结果（`确认并执行`）而不是动作（`确定`）。
- 数字：金额用 `¥12.40`，大数用 `¥1,234`；地域用 UCloud 原名（`cn-bj2`）；时间用相对时间
  （`3 分钟前`），悬浮显示绝对时间。
- 危险操作文案必须写清后果与可逆性：「将主机 `web-1` 停机（可再启动，仍计费）」。
