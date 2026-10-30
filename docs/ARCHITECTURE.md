# 架构

## 总览

```
┌──────────────────────────── adash-app (GPUI, M3 Expressive) ─────────────────────────────┐
│  对话（主页）                云资源                         AI 流程                        │
│  概览卡片 + 会话            资源清单 + 详情                n8n 风格节点图 + 执行回放      │
│        │                        │                              │                          │
│        └────────────── 共用 adash-core 领域类型 ────────────────┘                          │
└───────────┬──────────────────────────────────────────────┬─────────────────────────────────┘
            │ pi-rpc: JSONL over stdio                     │ 直接链接
            ▼                                              ▼
   pi --mode rpc（受限）                             ucloud-api（唯一客户端）
     ├ agent/extensions/ucloud  (ucloud_* 工具)            │  签名 / 签名 v2 / 传输 / 模型
     ├ agent/skills/*                                      │  Mock 后端（fixtures/mock）
     └ spawns: adash-tool <cmd>  ────── JSON ──────────────┘
```

三条数据路径：

1. **GUI 读**：`adash-app` → `ucloud-api`（read 接口）→ `adash-core` 归一化 → 三个页面。
2. **代理做**：`pi` → TS 扩展 → `adash-tool` → `ucloud-api`/`adash-core` → JSON 回传。
3. **流程图画**：`adash-core::flow` 里的项目—资源关系与执行记录 → `adash-app` 渲染。模型不画图。

## crates

### ucloud-api

唯一的 UCloud OpenAPI 客户端。无 UI、无代理逻辑。

| 模块 | 职责 |
| --- | --- |
| `signature` | 签名算法（v1 `Signature`、v2 若需要），纯函数，单独测试向量 |
| `transport` | HTTP、重试、超时、`RetCode != 0` 转 `Error::Api` |
| `paths` | 接口名常量 + 文档链接（`https://docs.ucloud.cn/api/...`） |
| `models` | 各产品响应的 `serde` 类型，字段名保持 UCloud 原样（PascalCase，用 `rename`） |
| `credentials` | 环境变量 → 系统钥匙串，私钥不进日志（`Debug` 手写脱敏） |
| `mock` | `UDASH_MOCK=1` 时从 `fixtures/mock` 读响应，行为与真实接口一致 |
| `client` | 对外门面：`inventory()`、`billing()`、`price()`、`regions()`… |

规则：新增接口 → 先补 `fixtures/mock` 的真实结构样例 → 加 `paths` 常量与模型 → 加 client 方法 →
写测试（签名向量 + 解析 + 脱敏）。

### adash-core

领域层，GUI 与代理共享。这里**不**发 HTTP。

| 模块 | 内容 |
| --- | --- |
| `resource` | `Resource`、`ResourceKind`、`ResourceState`：所有产品归一到一个词表 |
| `inventory` | 并发/串行地从 `ucloud-api` 拉全量资源，聚合、去重、标注项目归属 |
| `project` | 项目 = `adash:<slug>` 标签下的资源集合 + 备注 + 预算 |
| `flow` | `Flow { nodes, edges }` 与 `FlowRun`：节点是资源或服务，边是实测/声明的依赖 |
| `budget` | 预算、已花费、预测月底、超支告警 |
| `billing` | 账单明细归一化与按项目分摊 |
| `plan` | `Plan { actions, monthly_delta, risks, reversible }` + 哈希 + 过期时间 |
| `executor` | 执行已批准的计划（Mock 下记录执行并写入流程历史）；GUI 与 `adash-tool` 共用 |
| `optimizer` | 闲置/超配/孤儿资源规则，产出带金额的建议 |
| `state` | 本地持久化：项目、流程、计划、批准令牌（应用数据目录，0600） |

### adash-tool（代理的唯一执行面）

`adash-tool <command>`：stdin 一个 JSON，stdout 一个 JSON（`{"ok":true,"data":…}` /
`{"ok":false,"error":{…}}`）。命令按副作用分级：

| 等级 | 命令 | 约束 |
| --- | --- | --- |
| read | `inventory` `billing` `price` `regions` `flows` `flow-run` `budget` | 随时 |
| plan | `plan-deploy` `plan-optimize` `plan-stop` | 只写本地计划，不动云 |
| mutate | `apply <plan_id>` | 需计划未过期 + 哈希一致 + GUI 批准令牌 |

`adash-tool` 自己读凭证、自己做脱敏；它的输出就是模型的输入，因此**默认短**：
列表给摘要与计数，详情按 id 单查。

### pi-rpc

`pi --mode rpc` 子进程客户端：严格 LF 分帧 JSONL、命令/响应关联、事件流、
extension UI 子协议（`confirm` 卡片）。GUI 侧只暴露「发一条用户消息」「订阅事件」「应答确认」。

### adash-app

| 模块 | 职责 |
| --- | --- |
| `app` | 外壳、路由、共享状态（`Page::{Home, Resources, Flows}`） |
| `views/home` | 代理对话 + 账户概览卡片（余额、本月花费、资源计数、预算进度） |
| `views/resources` | 资源清单：筛选、搜索、详情抽屉、复制 id / 打开控制台 |
| `views/flows` | 流程图：节点画布、连线、执行回放、节点详情 |
| `widgets` | kit 缺的薄组合件（概览卡片、资源行、流程节点、连线层） |
| `agent` | pi 进程生命周期、消息流、计划卡片、工具调用折叠；pi 不可用时退化为本地应答 |
| `views_sessions` | 「连接主机」：主机事实 + 终端/文件/监控三个面（SSH 传输 M6） |
| `i18n` | 全部用户可见文案 |

## 三个页面的数据来源

- **主页概览**：`adash-core::billing`（余额/花费/预算）+ `inventory`（计数与异常）。
  与对话面板并排；代理的每次 read/plan 都会刷新同一份数据，因此对话和卡片永远一致。
- **云资源**：`inventory` 全量 → 按 `ResourceKind` / `region` / `project` / `state` 过滤。
  只显示**已创建**的资源；产品目录、价格页不属于这里。
- **AI 流程**：`adash-core::flow`。
  - 节点 = 项目下的资源 + 外部端点（GitHub、US3、域名、CDN）。
  - 边 = 来自三类证据：UCloud 资源属性（EIP 绑定、UDB 挂载、ULB 后端）、
    部署记录（`plan` 里声明的依赖）、主机上实测的连通性（可选）。
  - `FlowRun` = 一次部署/优化执行的时间线，回放时高亮走过的边。
  - 图的布局在 GUI 里算（分层 DAG），存盘只存拓扑，不存像素。

## 部署流程（「许愿」的标准路径）

```
用户：「把这个仓库部署上线，越便宜越好」
  → 代理 read：regions / price / inventory（摘要在对话里折叠成一行）
  → 代理 plan：plan-deploy → adash-core::Plan（资源、月成本、风险、是否可逆）
  → GUI 渲染计划卡片（成本、地域、可逆性、标红项）→ 用户点「确认」
  → GUI 写一次性批准令牌 → 代理调用 ucloud_apply_plan(plan_id)
  → adash-tool apply 校验 → 执行 → 验证 → 打标签 adash:<slug>
  → inventory 刷新 → 主页概览与「AI 流程」同时出现新节点
```

## 代理不可用时的退化

没有装 pi、模型没配好、或 `adash-tool` 不在 `target/debug` 时，应用不会变成空壳：
`agent.rs` 会退化成**本地应答**——用同一份 `inventory` / `billing` / `flow` 数据回答
「有哪些资源」「花了多少」「这个项目用了哪些服务」「有浪费吗」，并且每条消息都标注「离线模式」。
它不会编造数据，也不会执行任何变更；离线时确认计划只记录执行，并明确说明没有创建资源。

## 威胁模型（demo 级）

| 风险 | 处理 |
| --- | --- |
| 代理误删/误建 | 只能 apply 已批准计划；删除默认降级为停机；令牌一次性、带有效期 |
| 私钥泄漏到模型上下文 | 凭证只在 `ucloud-api` 读取；`adash-tool` 输出脱敏，测试覆盖 |
| 计划被篡改 | 计划哈希 + 过期时间 + 令牌三方校验 |
| 代理越权执行 shell | `pi --no-builtin-tools`，只加载本仓库扩展 |
| Mock 数据误连真实账户 | 必须显式清掉 `UDASH_MOCK`；启动时在标题栏显示当前模式 |

## 明确不做（demo 范围）

- 不做多用户/团队权限、不做计费代付、不做资源编排引擎（流程页只读）。
- 不在 GUI 里内嵌 Web 控制台；需要时用系统浏览器打开 UCloud 控制台对应链接。
