# 实际运行截图

以下图片来自 Rust/GPUI 应用在 niri/Wayland 上的实际窗口。
使用 `UDASH_MOCK=1 ADASH_OFFLINE=1`，应用状态置于独立临时 `XDG_DATA_HOME`。
没有执行云资源变更。裁去 compositor 阴影后按逻辑像素保存，未重绘界面。

| 页面 | 截图 | 窗口 |
| --- | --- | --- |
| 对话与账户概览 | [overview.png](overview.png) | 1440×900 |
| 深色主题 | [overview-dark.png](overview-dark.png) | 1440×900 |
| 资源卡片与详情 | [resources.png](resources.png) | 1440×900 |
| 服务图与检查器 | [flows.png](flows.png) | 1440×900 |
| 省钱计划（较窄窗口） | [overview-1280.png](overview-1280.png) | 1280×800 |
| 主机与添加表单 | [hosts.png](hosts.png) | 1440×900 |
| 身份 | [identities.png](identities.png) | 1440×900 |
| 片段 | [snippets.png](snippets.png) | 1440×900 |
| 端口转发 | [port-forward.png](port-forward.png) | 1440×900 |
| 设置 · 通用 / 终端 / 连接 / 关于 | [settings-0.png](settings-0.png) … [settings-3.png](settings-3.png) | 1440×900 |
| 设置 · 关于（真实 SDK 状态） | [settings-3.png](settings-3.png) | 1440×900 |
| 主机会话 | [session.png](session.png) | 1440×900 |

复现（需要已构建应用、niri、wl-paste 与 ImageMagick）：

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

默认使用本地应答（`ADASH_OFFLINE=1`）；`--online` 启动嵌入的 pi SDK，用于核对模型与工具状态。

脚本仅调整、关闭自己启动的窗口，并恢复先前焦点；不会覆盖用户的 HOME 或应用状态。

已逐张检查布局、圆角输入、卡片边距、资源价格换行与图中节点可见性。
这些截图是视觉验收证据，不能替代完整的鼠标、键盘、滚动及真实账户交互测试。
SSH/SFTP/PTY/监控/转发由 `adash-ssh` 的本地临时 sshd 集成测试验证，而不是截图。
