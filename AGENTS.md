# AGENTS.md — adash（Agent Dashboard for UCloud）

本文件是写给**在本仓库里干活的 AI 编码代理与人类贡献者**的项目规则。动手前完整读一遍；
涉及架构、页面或任务时再读 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)、
[docs/UI.md](docs/UI.md) 与 [docs/ROADMAP.md](docs/ROADMAP.md)。

## 1. 产品是什么

adash 是一个**桌面端云控制台 + 对话式代理**：用户用自己的 UCloud 账号登录，
既可以像控制台一样看清账户里的资源、项目和预算，也可以直接用一句话让代理去查、去建、去省。

界面只有一个窗口、三个页面（[catussh](https://github.com/mizorewww/catussh) 的外壳骨架，
M3 Expressive 视觉来自 [neko.html](docs/design/neko.html)）：

| 页面 | 是什么 | 关键词 |
| --- | --- | --- |
| **对话（主页）** | 以 pi 为底层的代理对话入口 + 账户整体概览卡片 | 一句话 → 计划卡片 → 确认 → 结果 |
| **云资源** | 用户**已经创建**的资源清单（主机、EIP、云盘、数据库、Redis、US3…） | 筛选/搜索/详情/跳转 |
| **AI 流程** | 参考 n8n 的只读流程图：**一个项目用了哪些服务**、怎么连的、这次部署/请求走了哪条路 | 节点、连线、执行回放 |

三个页面共用同一份领域数据：代理、概览卡片和流程图看到的资源必须是同一批。

产品原则，冲突时按顺序取舍：

1. **安全**：不经用户确认绝不产生费用、绝不删除/变更资源；凭证绝不进入 LLM 上下文。
2. **看得懂**：用户随时能从「代理刚才做了什么」追到「具体哪个项目、哪些资源、哪条连线」。
3. **最便宜**：默认选最低成本可行方案，并把价格依据摆在计划卡片里。

## 2. 技术栈与仓库布局

Rust workspace，GPUI 桌面应用，Material 3 Expressive 组件来自
[gpui-kit-material](https://github.com/mizorewww/gpui-kit-material)（固定 commit 的 submodule）。

| 路径 | 语言 | 职责 |
| --- | --- | --- |
| `crates/ucloud-api` | Rust | **唯一的** UCloud OpenAPI 客户端：签名、传输、类型化响应、Mock 后端 |
| `crates/adash-core` | Rust | 领域层：统一资源模型、项目、流程（Flow）、预算/花费、计划(Plan)、本地状态 |
| `crates/adash-tool` | Rust（bin） | 给 pi 扩展调用的 JSON CLI，代理**唯一的**执行面 |
| `crates/pi-rpc` | Rust | `pi --mode rpc` 子进程客户端（JSONL over stdio、事件流、extension UI） |
| `crates/adash-app` | Rust（bin） | GPUI 应用：外壳/路由、三个页面、M3 组件组合 |
| `agent/extensions/ucloud` | TypeScript | pi 扩展：注册 `ucloud_*` 工具，内部只调 `adash-tool` |
| `agent/skills/*` | Markdown | 资源盘点、成本优化、流程图生成等领域知识（按需加载） |
| `agent/prompts/system.md` | Markdown | 追加给 pi 的系统提示（角色、流程、禁区） |
| `fixtures/mock` | JSON | Mock 账户数据，demo、测试与截图共用 |
| `docs/` | Markdown | 架构、UI 规范、路线图、决策记录、人工测试清单 |
| `docs/design/neko.html` | HTML | 视觉参考（只读，**不要修改、不要当作依赖**） |
| `third_party/gpui-kit-material` | git submodule | M3 组件库（固定 commit，**不要在本仓库内修改它**） |

不可破坏的边界：

- **UCloud API 只在 `ucloud-api` 里实现一次。** GUI 直接链接它；代理通过 `adash-tool` 间接使用它。
  TS 扩展里**禁止**直接请求 UCloud API，也禁止实现签名逻辑。
- **GUI 不嵌入 Node。** 代理是 `pi --mode rpc` 子进程，GUI 只通过 `pi-rpc` 与之通信。
- **pi 以受限模式启动**：`--no-builtin-tools`（没有 bash/edit/write），只加载本仓库的扩展、skill 与提示，
  会话存在应用数据目录。代理能做的事 = `ucloud_*` 工具能做的事。
- **三个页面共享 `adash-core` 的类型**，不允许各页面自己解析 UCloud JSON。

## 3. 安全与花钱规则（硬性）

1. 每个 `adash-tool` 命令与每个 `ucloud_*` 工具都标副作用等级，并写在工具 `annotations` 与文档里：
   - `read`：查询（列资源、查价格、查账单、查监控、读流程图）。随时可调用。
   - `plan`：只生成计划（`ucloud_plan_*`），不改云上状态。随时可调用。
   - `mutate`：创建/删除/变更/付费。**只能通过 `ucloud_apply_plan(plan_id)` 执行一个已存在的计划**，
     且必须先经过 extension UI `confirm`，由 GUI 渲染计划卡片（资源清单、月成本、风险、可逆性）后用户点确认。
     没有其他 mutate 入口。
2. `adash-tool apply` 自身再校验：计划存在、未过期（默认 15 分钟）、哈希未变、
   带 GUI 写入的一次性批准令牌。代理无法伪造批准。
3. 删除/释放类操作在计划卡片里必须标红并二次确认；默认先「停机」而非「删除」。
4. 凭证（PublicKey/PrivateKey/ProjectId）只放系统钥匙串（`keyring`）或环境变量
   `UCLOUD_PUBLIC_KEY` / `UCLOUD_PRIVATE_KEY` / `UCLOUD_PROJECT_ID`，由 `ucloud-api` 自己读取。
   工具输出、日志、会话文件、错误信息里**不得出现私钥**；改动触及输出路径时要补脱敏测试。
5. 开发、测试、CI 默认 **Mock 模式**（`UDASH_MOCK=1`，读 `fixtures/mock`）。
   真实账户测试必须显式 `UDASH_LIVE=1` 且只允许 `read` 级接口；任何会产生费用的 live 测试禁止提交。
6. 所有由本应用创建的资源都打标签/备注 `adash:<project-slug>`，这是「项目视角」和流程图的依据。

## 4. UI 规则（GPUI + Material 3）

- 组件来自 `gpui-kit-material`（Kit 负责行为，Material 负责外观）。先查它的 gallery 与
  `third_party/gpui-kit-material/docs/ARCHITECTURE.md`，有现成组件就用，**不要自造通用控件**。
  缺组件时在本仓库写薄的组合组件放 `crates/adash-app/src/widgets/`，在 ROADMAP 记一笔并考虑回馈上游；
  不要 fork submodule。
- 上色只用语义角色：`color(ColorRole::SURFACE_CONTAINER)`；尺寸用 `rem(..)`；**禁止硬编码颜色值**。
- 布局、断点、动效与「哪些地方必须是 kit 组件」以 [docs/UI.md](docs/UI.md) 为准；
  视觉细节以 `docs/design/neko.html` 为准，与其冲突时先改 UI.md 再改代码。
- 应用启动固定为 `gpui_kit::application()` → `gpui_kit::init` → `gpui_kit_material::init` →
  `theme::init_component_theme` → `gpui_kit::open_window`。**render 里不改主题、不 notify 循环。**
- 用户可见文案集中在 `crates/adash-app/src/i18n.rs`（默认中文，预留英文）；组件里不散落字符串。
- 长列表虚拟化；只 notify 拥有该状态的最小 view；后台 I/O（API、pi 事件）走 `cx.background_spawn`，
  绝不阻塞 UI 线程。
- 代理中间过程默认折叠成一行状态（「正在比较 3 个地域的价格…」），可展开看工具调用明细。

## 5. 代理（pi）规则

- 工具命名 `ucloud_<verb>_<noun>`，参数用 TypeBox schema，返回 `content`（给模型的精简文本）+
  `structuredContent`/`details`（给 GUI 渲染的结构化 JSON）。
- 工具输出要短：列表默认摘要 + 条数，模型需要时再按 id 查详情；大输出截断并说明。
- 部署/省钱/画流程图的知识写进 `agent/skills/`，不要塞进系统提示。
- 「AI 流程」页面读的是**真实数据**：`adash-core` 的项目—资源关系 + 执行记录，不是模型画出来的图。
  代理只负责维护关系和触发执行，图由 GUI 用领域数据渲染。
- 修改工具契约时同步更新：`adash-core` 类型 → `adash-tool` → TS 扩展 → GUI 渲染 → 文档，同一个提交内完成。

## 6. 开发命令

```sh
git submodule update --init --recursive        # 首次克隆后
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
UDASH_MOCK=1 cargo run -p adash-app            # Mock 模式跑 GUI
UDASH_MOCK=1 cargo run -p adash-tool -- inventory < /dev/null
cd agent/extensions/ucloud && npm install && npm run typecheck
```

Linux 构建 GPUI 需要 `libxkbcommon wayland vulkan-icd-loader libx11 libxcb fontconfig freetype2`
（Arch：`paru -S --needed libxkbcommon wayland vulkan-icd-loader libxcb fontconfig freetype2`）。
工具链由 `rust-toolchain.toml` 固定，必须与 gpui-kit-material 一致。

## 7. 工作方式

- 按 [docs/ROADMAP.md](docs/ROADMAP.md) 的里程碑推进：开始前把状态改为进行中，完成后勾选，
  并写清验收证据（命令输出、截图路径）。
- 小步原子提交，提交信息说明「改了什么 + 为什么」；一个提交只做一件事；在 `main` 上工作，不 force-push。
- 合并前必须通过第 6 节的 fmt / clippy / test；UI 改动附截图到 `docs/screenshots/`。
- 不确定 UCloud 接口字段时以官方文档 <https://docs.ucloud.cn/api> 为准，注释里写接口名与文档链接；
  不要凭记忆臆造字段。新接口先在 `fixtures/mock` 补一份真实结构的样例响应。
- 不要升级 `third_party/gpui-kit-material` 的 commit，除非任务明确要求；升级要单独提交，
  并同步 `[patch]`、`rust-toolchain.toml` 与 `Cargo.lock`。
- `docs/design/neko.html` 是参考素材：不参与构建、不引入其中的字体/脚本、不修改。
