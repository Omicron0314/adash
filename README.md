# adash

> 桌面端 UCloud 控制台 + 对话式代理：一句话管云资源、云项目与预算，
> 还能像 n8n 一样看清「这个项目到底用了哪些服务」。

基于 [catussh](https://github.com/mizorewww/catussh) 的外壳骨架，Rust + [GPUI](https://github.com/longbridge/gpui-kit) +
[gpui-kit-material](https://github.com/mizorewww/gpui-kit-material)（Material 3 Expressive）。
代理复用 [pi 的 SDK](https://github.com/earendil-works/pi)，以 `createAgentSession` 嵌入应用
（`agent/runtime`）；所有云操作经 `adash-tool` 的 JSON CLI。

## 三个页面

| 对话（主页） | 云资源 | AI 流程 |
| --- | --- | --- |
| 代理对话 + 账户概览 | 已创建的全部资源 | 项目用了哪些服务、怎么连的 |
| 一句话 → 计划卡片 → 一键确认 → 结果 | 按产品/地域/项目筛选，点进详情 | n8n 风格节点图 + 执行回放 |

导航另有「主机」及身份、片段、端口转发：SSH 传输（终端、SFTP、监控、两种端口转发）
已由独立的 `adash-ssh` 实现，首次连接需核对主机指纹，密码/口令只进系统钥匙串。
主机可编辑并重选身份，终端支持滚动历史、中文 IME、复制粘贴和尺寸同步；SFTP 可编辑文本、
管理目录与文件，并显示可取消的上传/下载进度。多条转发规则可按主机分别运行。

## 界面预览

按 [neko.html](docs/design/neko.html) 的图标导航、圆角面板和色板重做，以下为 Mock 模式下的实际 GPUI 截图。

![对话与账户概览](docs/screenshots/overview.png)

[云资源](docs/screenshots/resources.png) · [AI 流程](docs/screenshots/flows.png) ·
[1280×800 计划卡片](docs/screenshots/overview-1280.png) · [真实 SSH 终端](docs/screenshots/ssh-terminal.png) ·
[SFTP 文件](docs/screenshots/ssh-files.png) · [截图复现方法](docs/screenshots/README.md)

## 快速开始（Mock，不花钱）

```sh
git clone --recursive <repo>
python3 scripts/build-desktop.py
```

然后在系统启动器搜索 **adash** 即可打开。每次修改后重新运行该命令，构建应用和工具并更新启动入口。
生成的 `target/desktop/`（以 Cargo 实际输出目录为准）包含程序、工具、SDK 运行时与依赖，
可整体移动后运行其中的 `launch-adash`。桌面入口在安装时按当前输出位置生成；
移动程序目录后可重新运行构建脚本指定 `--output` 更新入口。`--no-install` 只生成程序目录。
入口默认 Mock，可用 `UDASH_MOCK=0` 显式选择真实账户读取。

首次打开后进入 **设置 → AI**，填写服务的 Endpoint（Base URL）、API key 和模型 ID，
选择 Chat Completions、Responses 或 Anthropic Messages 协议，然后保存并连接。
API key 保存在系统钥匙串，配置文件只记录密钥引用；更换 Endpoint 需重新填写密钥。
上下文长度和最大输出长度也可设置。

运行需要 Node.js 及当前平台的 GPUI 系统依赖；无需 pi CLI 或个人 pi 配置。
应用仅使用用户在设置中保存的 AI 服务，不从 `~/.pi`、`ADASH_MODEL` 自动选择模型。
SDK 不可用、未配置 AI 或模型报错时，仍能用本地领域数据回答资源与账单问题。
Mock 数据内置于二进制，运行时无需原始 checkout 或 fixtures 目录。
Mock 账户里有一台「shop-api」项目（2 台主机 + MySQL + Redis + ULB + EIP + 云盘）、
一个孤儿 EIP 和一块未挂载云盘，`adash-tool reset` 可恢复基线并写入一条可回放的部署记录。

真实账户读取：设置 `UCLOUD_PUBLIC_KEY` / `UCLOUD_PRIVATE_KEY` / `UCLOUD_PROJECT_ID`，
不要设置 `UDASH_MOCK`。UCloud 钥匙串读取及 Live 写操作尚未实现；Mock 计划执行目前记录流程历史，
不会实际创建或删除库存资源。

## 验证

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
ADASH_SSH_INTEGRATION=1 cargo test -p adash-ssh --test localhost
npm --prefix agent/runtime test
```

SSH 集成测试使用临时密钥与回环 sshd（需要 `sshd`、`ssh-keygen`；SFTP 用内置 `internal-sftp`）；
SDK 测试使用回环模型端点与真实 Rust CLI，不调用付费模型或云写接口。
原生窗口可用 `python3 scripts/ssh-smoke.py --capture /tmp/adash-ssh.png` 检查，退出后清理临时服务与数据。

## 路径与开发验证

程序按运行位置寻找资源；也支持 `<prefix>/bin` 与 `<prefix>/share/adash` 布局。
`ADASH_RESOURCES` 可指定资源目录，`ADASH_TOOL` 可指定工具路径，`ADASH_NODE` 可指定 Node.js，
`ADASH_DATA_DIR` 可指定应用数据目录。未覆盖时使用系统用户数据目录。密钥不通过这些环境变量传递。
`adash-tool doctor` 报告实际路径与 SDK 资源状态，不执行 `pi --version`。

截图与开场提问钩子只编译进 `ui-test-hooks` 功能；正常构建不处理它们。
验证脚本会显式构建测试版并读取 Cargo 返回的真实二进制路径，支持自定义 `CARGO_TARGET_DIR`。
截图可自动使用 niri，也可通过 `--capture-command` 指定其他桌面的命令（`{pid}`、`{output}`），
程序本身不依赖 niri。SSH 验证通过 PATH 寻找工具，支持 `ADASH_TEST_SSHD`、
`ADASH_TEST_SFTP_SERVER` 等覆盖项。

## 文档

从 [AGENTS.md](AGENTS.md)（规则）开始，再看 [docs/](docs/README.md) 的索引。
