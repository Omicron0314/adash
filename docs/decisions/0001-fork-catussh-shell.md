# ADR 0001：外壳骨架沿用 catussh

- 状态：已接受
- 日期：2025-10-30

## 背景

需求是「在 catussh 的基础上做一个 agent dashboard」：同一套自绘标题栏、左侧导航、
多页面结构，但内容从 SSH 工具换成 UCloud 控制台。catussh 本身是 Flutter，
而组件库 `gpui-kit-material` 与视觉参考 `neko.html` 都是 GPUI / M3 Expressive 生态。

## 决定

复用 catussh 的**信息架构与视觉骨架**（自绘标题栏、220px 左侧导航、每页一个主任务、
快捷键与状态反馈的克制风格），但用 Rust + GPUI 重新实现，代码不跨语言复用。

- 不移植 catussh 的任何 Dart 代码，也不在仓库内保留 Flutter 工程。
- 布局与交互约定写进 [docs/UI.md](../UI.md)，视觉细节参考 `docs/design/neko.html`。
- 页面从 catussh 的 Home / Servers / … 映射为：对话 / 云资源 / AI 流程。

## 后果

- 好处：视觉与手感延续，决策成本低；技术栈与组件库一致，没有跨语言桥。
- 代价：catussh 的具体实现（终端、SFTP、监控采集）不能复用，本项目也不需要它们——它们被替换成
  UCloud 的资源模型与流程图，属于产品层面的替换而非损失。
- 如果将来要做「连上主机」，SSH 能力应当作为独立 crate 引入，而不是把 Flutter 代码搬回来。
