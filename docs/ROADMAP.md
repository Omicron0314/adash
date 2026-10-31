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
- [x] `agent-sdk` 的 LF 分帧与生命周期函数 + 单测（原 `pi-rpc` 已移除）

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

## M3 pi 代理（`agent/runtime` + `agent-sdk`）

- [x] SDK 宿主：`createAgentSession`、启动等待、流式事件、错误、取消与会话清理；不调用 pi CLI。
- [x] `agent/extensions/ucloud`：10 个 `ucloud_*` 工具（6 read、3 plan、1 apply），全部走 `adash-tool`。
- [x] `agent/prompts/system.md` + `agent/skills/{deploy-cheapest,cost-optimize,ucloud-regions,flow-map}`。
- [x] 计划确认：GUI 一次性批准 → `ucloud_apply_plan`；仅收到 UI 应答而没有令牌仍被拒绝。
- [x] 会话写入应用数据目录；标准扩展、上下文文件、提示模板和工具发现关闭。
- [x] 本地 SDK 集成测试：回环模型服务 + 真实 SDK/扩展/Rust CLI，验证启动、白名单、拒绝/批准/取消、
      流式输出、模型错误、含多层斜线的模型 ID、进程正常退出，以及凭证不进入请求/事件/会话。
      证据：`npm --prefix agent/runtime test`；不调用付费模型或真实云写接口。

验收：Mock 下「一句话生成计划 → 确认 → 执行记录」；Mock 当前不会实际增删库存资源。

## M4 GUI（`adash-app`，M3 Expressive）

- [x] 按 neko.html 实际模板重做：96px 图标 rail、28px 主面板、白卡内容内边距、独立右栏与精确浅色色板。
      证据：`docs/screenshots/{overview,resources,flows,ssh-terminal,settings-0}.png`；
      `overview-1280.png` 覆盖 1280×800 下的省钱计划；fmt/clippy、Rust 测试与 TS typecheck 通过。

- [x] 应用外壳：32px 自绘标题栏、96px 图标导航（对话/云资源/主机/AI 流程/身份/片段/端口转发/设置）、
      Mock·Live 徽标；不再显示「连接主机」分组标题
- [x] **对话（主页）**：消息流、流式文本与工具调用折叠、计划卡片（成本/风险/可逆）、输入区；
      右侧概览（余额 / 本月花费 / 预算进度 / 资源计数 / 待处理）
- [x] **云资源**：搜索 + 产品/地域/项目筛选、状态点、详情面板（复制 id、查看流程图、打开终端）、空态
- [x] **AI 流程**：分层 DAG 布局、正交连线、推断边虚线、执行回放高亮、「适应窗口」紧凑布局、空态
- [x] 主机页（方案 B）：保存/删除主机（连接/批量选择）、身份、片段、端口转发；
      从资源页打开会创建并连接同一台主机
- [x] 代理不可用时的本地应答（标注「离线模式」，不编数据、不执行变更）
- [ ] 后续：kit 的 `SearchField`/`MaterialSideSheet` 替换自绘等价物、主题联动、虚拟化长列表
- [x] 截图进 `docs/screenshots/`；`scripts/capture-ui.py` 可在 niri 下复现，仅操作自身窗口。
- [ ] 完整的鼠标/键盘/滚动交互回归；截图核对不等于完整人工验收。

验收：三个页面在 Mock 数据下都能操作；1280×800 与 1440×900 下无横向滚动；
`docs/MANUAL_TESTS.md` 的 UI 部分全过。

### 导航方案（已定：B）

catussh 的终端 / 文件 / 监控 / 身份 / 片段 / 端口转发全部保留，作为「主机」页与
身份 / 片段 / 端口转发三个入口；导航不再显示分组标题，打开过的会话仍出现在侧栏。

## M6 SSH 传输（主机页与 catussh 能力）

- [x] 独立 `adash-ssh`：指纹先于认证、agent/密码/私钥（钥匙串）、PTY + vt100、SFTP
      列表/读写/上传/下载/重命名/删除、`/proc` 监控、本地与远程端口转发。
- [x] 主机/身份/片段/端口转发页面；片段可在当前主机或选中的多台主机上并行执行。
- [x] 集成测试：临时非特权 sshd，验证全部传输路径、指纹拒绝与两种转发方向。
      证据：`ADASH_SSH_INTEGRATION=1 cargo test -p adash-ssh --test localhost` → 1 passed。
- [x] 原生终端：同行多色与空格/中文网格、ANSI/256 色/truecolor、光标/粗体、按窗口同步 PTY 尺寸、
      滚轮历史、常用控制/功能/导航键、复制当前屏幕、bracketed paste 与中文 IME。
      GPUI 测试验证预编辑不发往远端，提交一次，失焦或弹窗时不发送。
- [x] 文件 UI：路径跳转/上一级、虚拟化目录列表、新建/重命名/删除、文本编辑、修改丢弃确认、
      上传/下载进度与取消；上传/下载拒绝覆盖已有目标，保存替换失败保留并恢复原文件。
- [x] 主机编辑与身份重选、保存失败回滚、稳定的异步会话索引、重连、关闭时的数据保护；
      每条转发规则绑定主机，可并行运行多条，按规则单独停止。
- [x] 文件/会话/指纹确认使用组件库 M3 模态框：Escape 关闭、焦点限制在弹窗内、关闭后返回原焦点；
      证据：`docs/screenshots/ssh-dialog.png`。
- [x] 最终验收：`cargo fmt --all --check`、Clippy（workspace/all-targets/-D warnings）、
      `cargo test --workspace`（65 项）、`ADASH_SSH_INTEGRATION=1 cargo test -p adash-ssh --test localhost`（1 项）、SDK 测试（13 项）、
      原生窗口截图（主页、资源、流程、主机、身份、片段、转发、设置、真实 SSH 终端/文件/监控/指纹/模态框）。
- [x] SDK 内置代理（`agent/runtime` + `agent-sdk`），GUI 不再连接外部 pi CLI；10 个云工具白名单。
- [x] catussh 设置移植（通用/终端/连接/关于），浅色/深色主题、终端字体与字号、超时、监控间隔、指纹清除。
- [x] 应用 SVG 图标、桌面入口与 hicolor 安装；每次构建更新启动器。
- [x] 主页单层 M3 圆角输入框（无左侧图标、无提示文字、Enter 发送），所有表单输入统一圆角。

## M5 Demo 打磨

- [ ] `docs/DEMO.md` 剧本（5 分钟版 + 2 分钟版）
- [ ] `fixtures/mock` 数据讲得通（一个像样的项目：web ×2 + MySQL + Redis + EIP + ULB + US3）
- [ ] 冷启动 < 2s、首屏骨架屏、代理连接失败有可读错误
- [x] 开发用 `.desktop` 入口、图标与构建安装脚本；已通过 `gtk-launch adash` 验证。
      每次修改后运行 `python3 scripts/build-desktop.py` 更新启动器。
- [ ] 独立分发包（AppImage）；当前启动器引用本地 checkout 的构建产物。

## 之后（不在 demo 范围）

- 写操作可视化编排（流程页可编辑，真正变成 n8n）
- 多云（阿里云/腾讯云）适配器
- 团队与权限、审计日志导出
- 把本地 `flow_canvas` 与需要的组件回馈给 gpui-kit-material 上游
