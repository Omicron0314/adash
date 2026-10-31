# 架构

## 总览

```
┌──────────────────────────── adash-app (GPUI, M3 Expressive) ─────────────────────────────┐
│  对话（主页）                云资源                         AI 流程                        │
│  概览卡片 + 会话            资源清单 + 详情                n8n 风格节点图 + 执行回放      │
│        │                        │                              │                          │
│        └────────────── 共用 adash-core 领域类型 ────────────────┘                          │
└───────────┬──────────────────────────────────────────────┬─────────────────────────────────┘
            │ agent-sdk: JSONL over stdio                  │ 直接链接
            ▼                                              ▼
   node agent/runtime（pi SDK createAgentSession）    ucloud-api（唯一客户端）
     ├ agent/extensions/ucloud  (ucloud_* 工具)            │  签名 / 传输 / 模型
     ├ agent/skills/*                                      │  Mock 后端（fixtures/mock）
     └ spawns: adash-tool <cmd>  ────── JSON ──────────────┘

   adash-ssh（独立 crate）← adash-app：ssh2 传输、PTY、SFTP、监控、转发
```

四条数据路径：

1. **GUI 读**：`adash-app` → `ucloud-api`（read 接口）→ `adash-core` 归一化 → 页面。
2. **代理做**：SDK 宿主 → TS 扩展 → `adash-tool` → `ucloud-api`/`adash-core` → JSON 回传。
3. **流程图画**：`adash-core::flow` 里的项目—资源关系与执行记录 → `adash-app` 渲染。模型不画图。
4. **主机连接**：`adash-app` → `adash-ssh` 工作线程（SSH/SFTP/PTY/转发）→ 事件回到 UI 定时泵，
   `ssh2::Session` 从不跨线程共享。

## crates

### ucloud-api

唯一的 UCloud OpenAPI 客户端。无 UI、无代理逻辑。

| 模块 | 职责 |
| --- | --- |
| `signature` | 签名算法（v1 `Signature`、v2 若需要），纯函数，单独测试向量 |
| `transport` | HTTP、重试、超时、`RetCode != 0` 转 `Error::Api` |
| `paths` | 接口名常量 + 文档链接（`https://docs.ucloud.cn/api/...`） |
| `models` | 各产品响应的 `serde` 类型，字段名保持 UCloud 原样（PascalCase，用 `rename`） |
| `credentials` | 环境变量或账户页保存的 profile；私钥只进钥匙串，`Debug` 脱敏 |
| `mock` | `UDASH_MOCK=1` 时读编译内置的样例；`UDASH_MOCK_DIR` 可覆盖，行为与真实接口一致 |
| `client` | 对外门面：`inventory()`、`billing()`、`price()`、`regions()`… |

规则：新增接口 → 先补 `fixtures/mock` 的真实结构样例并加入内置表 → 加 `paths` 常量与模型 → 加 client 方法 →
写测试（签名向量 + 解析 + 脱敏）。

### adash-repo

只读仓库扫描器：本机目录或公开 GitHub tarball；递归深度、文件数、单文件/总量都有限制，
只识别清单文件名并跳过依赖目录、`.git`、符号链接和 `.env`。输出语言、框架、服务线索与证据，
不输出源码正文。GUI 的主页提示与 `adash-tool repo-inspect` 共用这套规则。

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

### agent-sdk 与 agent/runtime

`agent/runtime` 是应用自带的 Node 宿主，用 pi SDK 的 `createAgentSession` 创建会话：
`DefaultResourceLoader` 关闭标准发现（`noExtensions`/`noSkills`/`noContextFiles`/`noPromptTemplates`），只加入本仓库的
`ucloud_*` 扩展、skills 与追加系统提示；工具白名单固定为 12 个 `ucloud_*`，没有 shell/edit/write。
宿主接收 `start` / `prompt` / `abort` / `set_model` / `ui_response` / `dispose`，输出 GUI 所需的 SDK 事件投影。
文本更新仅传本次 delta，避免反复传送完整消息与 provider 元数据；模型错误显式进入错误事件。
模型服务由用户在「账户」页填写：Endpoint、API 协议、模型 ID 与 token 限制存入应用 `ai.json`，
API key 存在系统钥匙串，JSON 仅保存引用。保存失败保留原配置，更换 Endpoint 必须提供新密钥。
SDK 使用内存凭据和 `modelsPath: null`，注册用户指定的服务与模型；不读取个人 pi 模型/认证配置。
密钥经私有 stdin 交给宿主，不写入 argv/env/模型元数据；服务错误回显的密钥在事件和会话持久化前脱敏。
扩展的 `ctx.ui.confirm` 以 `ui_request` 转发到 GUI，用户在计划卡片确认后回传 `ui_response`；
GUI 写入一次性批准，`adash-tool apply` 再校验计划哈希、过期与批准，执行仍在 Rust。

`crates/agent-sdk` 只负责进程生命周期、LF 分帧与事件分类；模型、会话、工具与流式输出都属于 SDK。

### 云账户配置

`ucloud-api::CloudProfile` 保存 PublicKey、ProjectId、Region、Endpoint 和启用状态到 `cloud.json`，
PrivateKey 只存系统钥匙串。GUI 与 `adash-tool` 共用该 profile，因此 Live 连接时代理工具读取同一账户。
保存前执行一次只读余额请求；失败不保存。`UDASH_MOCK` 显式覆盖，未设置时已启用 profile 自动 Live，
否则 Mock。账户页可停用（保留密钥）或移除（删除密钥）。

### adash-paths 与程序目录

`adash-paths` 统一用户数据目录、资源目录和辅助工具发现。按当前可执行文件的位置支持自包含目录和
`bin` / `share/adash` 布局；开发构建按实际运行位置寻找 checkout。没有编译机器的源码绝对路径。
`adash-tool` 从应用同目录或 PATH 查找，显式环境覆盖可用于自定义安装。
构建脚本将程序、工具、SDK/扩展及 Node 依赖复制到可移动目录；启动脚本按自身位置执行程序。
Mock JSON 编译进客户端，独立运行无需开发目录。截图启动钩子仅在 `ui-test-hooks` 功能下编译。

### 仓库扫描与部署计划

`adash-repo` 只读取受支持的清单文件名（package.json、Cargo.toml、requirements.txt、go.mod、Dockerfile、compose 等），
限制文件数量/大小/递归深度并跳过 `.git`、依赖目录、符号链接和 `.env`。本机路径只读；公开 GitHub URL
下载 tarball 后同样只解析清单。返回语言、框架、服务线索和证据，不返回文件正文。

`ucloud_repo_inspect` 将摘要交给模型；`ucloud_plan_repo_deploy` 用同一扫描结果生成计划，
默认选单台轻量主机 + 公网 IP + 清单要求的 UDB/Redis/US3，只有模型明确选择时才加 ULB。
计划仍走同一套批准/哈希/过期校验。当前执行器仅在 Mock 模式记录流程，真实创建 API 尚未接入。

### adash-ssh

独立传输层：主机指纹（首次确认、变更拒绝，先于认证）、agent/密码/私钥认证（密钥环）、
PTY + `vt100` 终端、SFTP 列表/读写/上传/下载/重命名/删除、`/proc` 监控采样、本地与远程端口转发。
GUI 只通过请求/应答通道交互，已关闭会话保留索引槽位，异步文件选择结果不会误用另一台主机。
终端的网格绘制和原生输入法分别在 `terminal_view` / `terminal_input`；每个输入实体只持有本会话的发送端，
输入法预编辑不发送到 PTY，提交时检查焦点与启用状态。文件列表使用 Kit 的 `uniform_list`。
文本保存优先原子替换；服务器不支持时保留备份，失败则恢复或报告备份路径。传输分块处理并支持取消。
集成测试 `crates/adash-ssh/tests/localhost.rs` 会启动一个临时的、
仅监听回环的非特权 `sshd`，验证上述全部路径（`ADASH_SSH_INTEGRATION=1`）。

### adash-app

| 模块 | 职责 |
| --- | --- |
| `app` | 外壳、路由、共享状态（产品页、主机页、设置） |
| `views/home` | 代理对话 + 账户概览卡片（余额、本月花费、资源计数、预算进度） |
| `views/resources` | 资源清单：筛选、搜索、详情抽屉、复制 id / 打开控制台 |
| `views/flows` | 流程图：节点画布、连线、执行回放、节点详情 |
| `widgets` | kit 缺的薄组合件（概览卡片、资源行、流程节点、连线层） |
| `agent` | SDK host 生命周期、消息流、计划确认；host 不可用时退化为本地应答 |
| `ai` | 用户 AI 配置校验、钥匙串与配置事务保存、表单状态 |
| `ssh` | 主机/身份/片段/转发状态、连接与终端生命周期、批处理 |
| `palette` | neko 浅色/深色语义色板，供主题切换使用 |
| `views_sessions` | 主机、身份、片段、端口转发与会话终端/监控 |
| `views_account` | 账户页：UCloud 云账户、AI 服务、Live/Mock 切换 |
| `views_files` | SFTP 目录、传输和文本编辑器 |
| `terminal_view` / `terminal_input` | VT 行样式、控制键编码、原生 IME 与按会话绑定的输入 |
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

Node/SDK 宿主不可用、模型没配好、或 `adash-tool` 不可用时，应用不会变成空壳：
`agent.rs` 会退化成**本地应答**——用同一份 `inventory` / `billing` / `flow` 数据回答
「有哪些资源」「花了多少」「这个项目用了哪些服务」「有浪费吗」，并且每条消息都标注「离线模式」。
它不会编造数据，也不会执行任何变更；离线时确认计划只记录执行，并明确说明没有创建资源。

SDK 已启动但**模型不可用**（没配 API key、provider 报错）时同样退化：这一轮改为本地应答，
并在折叠说明里写清原因（`模型不可用：…`），不会留下空白的助手气泡。

## 威胁模型（demo 级）

| 风险 | 处理 |
| --- | --- |
| 代理误删/误建 | 只能 apply 已批准计划；删除默认降级为停机；令牌一次性、带有效期 |
| 私钥泄漏到模型上下文 | 凭证只在 `ucloud-api` 读取；`adash-tool` 输出脱敏，测试覆盖 |
| 计划被篡改 | 计划哈希 + 过期时间 + 令牌三方校验 |
| 代理越权执行 shell | SDK 显式云工具白名单，关闭标准扩展、上下文与提示模板发现 |
| Mock 数据误连真实账户 | 默认 Mock；只有启用已保存的云账户或显式 `UDASH_MOCK=0` 才 Live；标题栏显示当前模式 |

## 明确不做（demo 范围）

- 不做多用户/团队权限、不做计费代付、不做资源编排引擎（流程页只读）。
- 不在 GUI 里内嵌 Web 控制台；需要时用系统浏览器打开 UCloud 控制台对应链接。
