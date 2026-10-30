# 任务规划 / Roadmap

状态标记：`[ ]` 未开始 · `[~]` 进行中 · `[x]` 完成（附验收证据）。
开始任务时先改成 `[~]`；完成后勾选并写一行证据（命令输出 / 截图路径）。

## M0 仓库与工程骨架

- [x] workspace + 五个 crate + 固定工具链（`rust-toolchain.toml` = gpui-kit-material）
- [x] `third_party/gpui-kit-material` submodule 固定在 `45de656`
- [x] `[patch]` 与 submodule 一致（GPUI / Base / Fast + Kit）
- [x] 应用骨架：标题栏 + 左侧导航 + 三个页面占位
      证据：`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全通过
- [x] `AGENTS.md`、`docs/`（ARCHITECTURE / UI / API / ROADMAP / decisions）
- [x] `pi-rpc` 分帧函数 + 单测

## M1 UCloud API 客户端（`ucloud-api`）

- [ ] `signature`：签名算法 + 官方测试向量
- [ ] `credentials`：环境变量 / 钥匙串，`Debug` 脱敏
- [ ] `transport`：超时、重试、`RetCode` 错误映射（`Error::Api`）
- [ ] `paths`：接口名常量 + 文档链接
- [ ] 首批接口：`DescribeUHostInstance`、`DescribeEIP`、`DescribeUDisk`、`DescribeUDBInstance`、
      `DescribeRedis`、`DescribeUFile`/US3 列表、`DescribeULB`、`GetBalance`、`ListBills`、`GetRegion`
- [ ] `mock`：`UDASH_MOCK=1` 全部读接口可跑（`fixtures/mock`）
- [ ] `client` 门面 + 每条接口的解析测试

验收：`UDASH_MOCK=1 cargo run -p adash-tool -- inventory | jq` 输出全部资源；
真实账户 `UDASH_LIVE=1` 只跑 read。

## M2 领域层与执行面（`adash-core` + `adash-tool`）

- [ ] `inventory`：聚合、去重、`adash:<slug>` 标签 → 项目归属
- [ ] `project` / `budget` / `billing`：预算、已花费、预测月底
- [ ] `plan`：生成、哈希、过期、令牌校验
- [ ] `flow`：从资源属性与部署记录推导 `Flow { nodes, edges }` 与 `FlowRun`
- [ ] `optimizer`：闲置 / 超配 / 孤儿资源规则（带金额）
- [ ] `state`：应用数据目录持久化，0600
- [ ] `adash-tool` 命令面：read / plan / mutate 全部分级 + 集成测试（`crates/adash-tool/tests/flow.rs`）

验收：`cargo test --workspace`；`apply` 在无令牌/令牌过期/计划被改三种情况下都拒绝执行。

## M3 pi 代理（`agent/` + `pi-rpc`）

- [ ] `pi-rpc`：进程生命周期、请求/响应关联、事件流、extension UI `confirm`
- [ ] `agent/extensions/ucloud`：`ucloud_*` 工具，全部走 `adash-tool`，`npm run typecheck` 通过
- [ ] `agent/prompts/system.md` + `agent/skills/{deploy-cheapest,cost-optimize,ucloud-regions,flow-map}`
- [ ] 计划确认闭环：GUI 渲染计划卡片 → 一次性令牌 → `ucloud_apply_plan`
- [ ] 代理输出脱敏测试（私钥、完整 AK/SK 不出现在工具输出与会话文件）

验收：Mock 下跑通「一句话部署 → 计划卡片 → 确认 → 资源出现」与「找出 2 个孤儿 EIP 并生成计划」。

## M4 GUI（`adash-app`，M3 Expressive）

- [ ] 应用外壳：`NavigationRail` 折叠、Mock/Live 徽标、`Snackbar`、全局错误条
- [ ] **对话（主页）**：消息流、流式渲染、工具调用折叠、计划卡片、输入区；
      右侧概览卡片（余额 / 本月花费 / 预算进度 / 资源计数 / 待处理）
- [ ] **云资源**：`SearchField` + `FilterChip` 过滤（产品/地域/项目/状态）、虚拟化清单、
      详情 `MaterialSideSheet`、空态
- [ ] **AI 流程**：`widgets/flow_canvas.rs`（分层 DAG 布局、平移缩放、正交连线）、
      节点卡片与状态、执行回放高亮、项目/时间范围切换、空态跳转对话
- [ ] 主题联动：M3 动态种子色跟随系统、明暗切换、100–200% 字号不破版
- [ ] 截图进 `docs/screenshots/`

验收：三个页面在 Mock 数据下都能操作；1280×800 与 1440×900 下无横向滚动；
`docs/MANUAL_TESTS.md` 的 UI 部分全过。

## M5 Demo 打磨

- [ ] `docs/DEMO.md` 剧本（5 分钟版 + 2 分钟版）
- [ ] `fixtures/mock` 数据讲得通（一个像样的项目：web ×2 + MySQL + Redis + EIP + ULB + US3）
- [ ] 冷启动 < 2s、首屏骨架屏、代理连接失败有可读错误
- [ ] 打包（AppImage / `.desktop`）、图标、README 截图

## 之后（不在 demo 范围）

- 写操作可视化编排（流程页可编辑，真正变成 n8n）
- 多云（阿里云/腾讯云）适配器
- 团队与权限、审计日志导出
- 把本地 `flow_canvas` 与需要的组件回馈给 gpui-kit-material 上游
