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

- [x] `signature`：签名算法 + 排序/空值/脱敏测试
- [x] `credentials`：环境变量，`Debug` 脱敏（测试覆盖）
- [x] `transport`：超时、`RetCode` 错误映射、Mock 后端、CRLF/分帧
- [x] `paths`：接口名常量 + 文档链接
- [x] 首批接口：`DescribeUHostInstance`、`DescribeEIP`、`DescribeUDisk`、`DescribeUDBInstance`、
      `DescribeUMem`、`DescribeULB`、`GetBalance`、`ListBills`、`GetRegion`
- [x] `mock`：`UDASH_MOCK=1` 全部读接口可跑（`fixtures/mock`，9 份真实结构样例）
- [x] `client` 门面 + 解析测试
      证据：`cargo test -p ucloud-api` 8 passed；`adash-tool inventory` 输出 11 个资源
- [ ] 后续：钥匙串读取、US3/轻量主机接口、`GetUHostInstancePrice` 实时价格

验收：`UDASH_MOCK=1 cargo run -p adash-tool -- inventory | jq` 输出全部资源；
真实账户 `UDASH_LIVE=1` 只跑 read。

## M2 领域层与执行面（`adash-core` + `adash-tool`）

- [x] `inventory`：聚合、`adash:<slug>` 标签 → 项目归属、未分组兜底
- [x] `project` / `budget` / `billing`：预算、已花费、按项目/产品分摊
- [x] `plan`：生成、哈希封印、过期、一次性批准
- [x] `flow`：节点/连线/证据（EIP 绑定、ULB 后端、云盘挂载、推断）+ 分层布局 + 执行回放
- [x] `optimizer`：孤儿 EIP、未挂载云盘、停机仍计费（带金额）
- [x] `state`：`state.json`，0600；计划/批准/项目/执行记录
- [x] `executor`：GUI 与 CLI 共用的执行路径（Mock 记录执行）
- [x] `adash-tool` 命令面：read / plan / mutate 分级 + 集成测试
      证据：`cargo test --workspace` 45 passed；`tests/flow.rs` 覆盖无批准拒绝、错令牌拒绝、
      一次性执行、计划被篡改拒绝
- [ ] 后续：写操作 live 实现（创建/停机/释放）、监控接口喂给 optimizer

验收：`cargo test --workspace`；`apply` 在无令牌/令牌过期/计划被改三种情况下都拒绝执行。

## M3 pi 代理（`agent/` + `pi-rpc`）

- [x] `pi-rpc`：进程生命周期、分帧、请求 id、事件分类、extension UI 应答
- [x] `agent/extensions/ucloud`：9 个 `ucloud_*` 工具（7 read、3 plan、1 apply），全部走 `adash-tool`
      证据：`npx tsc --noEmit` 通过
- [x] `agent/prompts/system.md` + `agent/skills/{deploy-cheapest,cost-optimize,ucloud-regions,flow-map}`
- [x] 计划确认闭环：GUI 计划卡片 → 一次性批准 → `ucloud_apply_plan`；离线时本地记录
- [ ] 后续：脱敏输出测试落成单测、pi 会话目录固定到应用数据目录

验收：Mock 下跑通「一句话部署 → 计划卡片 → 确认 → 资源出现」与「找出 2 个孤儿 EIP 并生成计划」。

## M4 GUI（`adash-app`，M3 Expressive）

- [x] 应用外壳：自绘标题栏、分组侧栏（产品页 / 连接主机 / 设置）、Mock·Live 徽标、提示条
- [x] **对话（主页）**：消息流、流式文本与工具调用折叠、计划卡片（成本/风险/可逆）、输入区；
      右侧概览（余额 / 本月花费 / 预算进度 / 资源计数 / 待处理）
- [x] **云资源**：搜索 + 产品/地域/项目筛选、状态点、详情面板（复制 id、查看流程图、打开终端）、空态
- [x] **AI 流程**：分层 DAG 布局、正交连线、推断边虚线、执行回放高亮、「适应窗口」紧凑布局、空态
- [x] **连接主机（方案 B）**：从资源页打开的会话进入侧栏分组；会话页含终端/文件/监控三个面，
      展示真实主机事实与登录命令（SSH 传输在 M6）
- [x] 代理不可用时的本地应答（标注「离线模式」，不编数据、不执行变更）
- [ ] 后续：kit 的 `SearchField`/`MaterialSideSheet` 替换自绘等价物、主题联动、虚拟化长列表
- [ ] 截图进 `docs/screenshots/`（本机缺截图工具，见该目录 README）

验收：三个页面在 Mock 数据下都能操作；1280×800 与 1440×900 下无横向滚动；
`docs/MANUAL_TESTS.md` 的 UI 部分全过。

### 导航方案（已定：B）

catussh 的终端 / 文件 / 监控 / 身份 / 片段 / 端口转发全部保留，作为独立分组
「连接主机」放在三个产品页下方：打开过的主机会话排在最前，其余三项固定在后；
没有会话时只显示一行提示，不抢首屏。

## M6 SSH 传输（连接主机分组的能力）

- [ ] 引入 `russh`（或 `ssh2`）+ 终端仿真，会话页三个面接真实主机
- [ ] 身份（密码/私钥）进钥匙串；主机指纹首次确认、变更拒绝
- [ ] SFTP 文件浏览与编辑；端口转发
- [ ] 片段：多主机批量执行
- [ ] 传输层必须独立成 crate（`adash-ssh`），不得把连接逻辑写进 `adash-app`

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
