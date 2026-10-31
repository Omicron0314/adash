# 实际运行截图

以下图片来自 Rust/GPUI 应用在 niri/Wayland 上的实际窗口。
使用 `UDASH_MOCK=1 ADASH_OFFLINE=1`，应用状态置于独立临时 `XDG_DATA_HOME`。
没有执行云资源变更。裁去 compositor 阴影后按逻辑像素保存，未重绘界面。

| 页面 | 截图 | 窗口 |
| --- | --- | --- |
| 对话与账户概览 | [overview.png](overview.png) | 1440×900 |
| 小窗口标题栏 | [titlebar-800.png](titlebar-800.png) · [titlebar-320.png](titlebar-320.png) | 800×600 / 320×32（仅标题栏） |
| 最大化后的标题栏与窗口 | [titlebar-maximized.png](titlebar-maximized.png) | 实际最大化窗口，未缩放 |
| 深色主题 | [overview-dark.png](overview-dark.png) | 1440×900 |
| Markdown 助手回复 | [chat-markdown.png](chat-markdown.png) | 1440×900 |
| Markdown 深色主题 / 宽表格 | [chat-markdown-dark-1280.png](chat-markdown-dark-1280.png) · [chat-markdown-wide-1280.png](chat-markdown-wide-1280.png) | 1280×800 |
| 资源卡片与详情 | [resources.png](resources.png) | 1440×900 |
| 服务图与检查器 | [flows.png](flows.png) | 1440×900 |
| 省钱计划（较窄窗口） | [overview-1280.png](overview-1280.png) | 1280×800 |
| 主机与添加表单 | [hosts.png](hosts.png) | 1440×900 |
| 身份 | [identities.png](identities.png) | 1440×900 |
| 片段 | [snippets.png](snippets.png) | 1440×900 |
| 端口转发 | [port-forward.png](port-forward.png) | 1440×900 |
| 设置 · 通用 / 终端 / 连接 / 关于 | [settings-0.png](settings-0.png) … [settings-3.png](settings-3.png) | 1440×900 |
| 设置 · 关于（真实 SDK 状态） | [settings-3.png](settings-3.png) | 1440×900 |
| 实际 SSH 终端 / SFTP / 监控 | [ssh-terminal.png](ssh-terminal.png) · [ssh-files.png](ssh-files.png) · [ssh-monitor.png](ssh-monitor.png) | 1440×900 |
| 首次连接与完整指纹 | [ssh-first-connection.png](ssh-first-connection.png) | 1440×900 |
| 远程目录确认（M3 模态框） | [ssh-dialog.png](ssh-dialog.png) | 1440×900 |
| SDK 模型启动失败后的本地应答 | [agent-fallback.png](agent-fallback.png) | 1440×900 |

复现（需要已构建应用、niri 与 ImageMagick）：

```sh
cargo build -p adash-app
python3 scripts/capture-ui.py home docs/screenshots/overview.png
python3 scripts/capture-ui.py resources docs/screenshots/resources.png
python3 scripts/capture-ui.py flows docs/screenshots/flows.png
python3 scripts/capture-ui.py home docs/screenshots/overview-1280.png \
  --width 1280 --height 800 --ask '有浪费的钱吗？'
ADASH_START_SETTINGS=1 python3 scripts/capture-ui.py settings docs/screenshots/settings-1.png
ADASH_START_SETTINGS=3 python3 scripts/capture-ui.py settings docs/screenshots/settings-3.png --online
ADASH_DEMO_APPEARANCE=dark python3 scripts/capture-ui.py home docs/screenshots/overview-dark.png
```

默认使用本地应答（`ADASH_OFFLINE=1`）；`--online` 启动嵌入的 pi SDK，并清除继承的离线标记。
SDK 失败截图使用不存在的模型 ID，不调用付费模型。

真实 SSH 截图使用临时密钥、临时目录与仅监听回环的 sshd：

```sh
python3 scripts/ssh-smoke.py --capture docs/screenshots/ssh-terminal.png
python3 scripts/ssh-smoke.py --tab files --capture docs/screenshots/ssh-files.png
python3 scripts/ssh-smoke.py --tab monitor --capture docs/screenshots/ssh-monitor.png
python3 scripts/ssh-smoke.py --untrusted --capture docs/screenshots/ssh-first-connection.png
python3 scripts/ssh-smoke.py --dialog --capture docs/screenshots/ssh-dialog.png
ADASH_MODEL=missing-provider/nested/model python3 scripts/capture-ui.py home \
  docs/screenshots/agent-fallback.png --online --ask '我有哪些资源？'
```

终端工作目录与 history 文件均在临时目录内；截图后清理自己启动的 app 与 sshd。
截图明确保存到临时路径，避免写入用户的截图库。

脚本仅调整、关闭自己启动的窗口，并恢复先前焦点；不会覆盖用户的 HOME 或应用状态。
即使调用者设置了 `ADASH_DATA_DIR`，截图也强制使用自己的临时数据目录。

标题栏回归另使用临时原生测试入口，读取 Kit 记录的按钮边界，确认 320px 窗口内按钮全部位于可见客户区及 32px 标题栏中。
注入实际 GPUI 鼠标按下/抬起事件，验证最大化、还原、关闭窗口及进程正常退出；最大化截图来自这一流程。
最小化是否出现由平台能力决定：本机 niri 报告 `minimize=false`，其余系统的实际最小化/恢复操作仍需对应桌面验收。

Markdown 对话截图使用临时预览入口，复用实际 `Adash` 对话视图，把固定示例正文按每 80ms、14 个字符追加。
验证标题、强调、列表、链接、行内代码、代码块、表格和引用，以及深色表头的可读性与宽表格的内部裁切。
示例消息仅用于排版验证；账户概览仍来自 Mock 数据，没有调用模型或云写接口。

已逐张检查布局、圆角输入、卡片边距、资源价格换行与图中节点可见性。
这些截图是视觉验收证据，不能替代完整的鼠标、键盘、滚动及真实账户交互测试。
SSH/SFTP/PTY/监控/转发由 `adash-ssh` 的本地临时 sshd 集成测试验证，而不是截图。
