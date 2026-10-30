# 文档索引

先读 [../AGENTS.md](../AGENTS.md)（规则与边界），再按要动的东西挑一份：

| 文档 | 什么时候读 | 状态 |
| --- | --- | --- |
| [ARCHITECTURE.md](ARCHITECTURE.md) | 改 crate 边界、数据流、代理执行面、流程图数据来源 | 已写 |
| [UI.md](UI.md) | 改任何界面：页面结构、组件选型、颜色/尺寸/动效、文案 | 已写 |
| [ROADMAP.md](ROADMAP.md) | 接任务、看里程碑与验收标准 | 已写 |
| [API.md](API.md) | 接新的 UCloud 接口：签名、接口清单、Mock 约定 | 已写 |
| [DEMO.md](DEMO.md) | 准备演示、录屏、对外讲故事 | 待 M5 |
| [MANUAL_TESTS.md](MANUAL_TESTS.md) | 发版前人工回归（真实账户、打包、图形） | 待 M5 |
| [decisions/](decisions/) | 想知道某个选择为什么这么定 | ADR 0001–0002 |
| [design/neko.html](design/neko.html) | 找视觉参考（只读素材，不参与构建） | 冻结 |
| [screenshots/](screenshots/) | 放每页截图，PR 里引用 | 空 |

## 路径规划（写到哪）

```
AGENTS.md                    规则：边界、安全、UI、开发命令、工作方式
README.md                    对外介绍 + 快速开始
docs/
  README.md                  本索引
  ARCHITECTURE.md            crate 职责、数据流、三个页面的数据从哪来
  UI.md                      设计语言、页面布局、组件映射、动效与文案
  API.md                     UCloud 接口清单、签名、Mock 与脱敏
  ROADMAP.md                 M0–M6 里程碑与验收
  MANUAL_TESTS.md            人工测试清单
  DEMO.md                    演示剧本
  decisions/0001-*.md        ADR：沿用 catussh 外壳
  decisions/0002-*.md        ADR：AI 流程页只读、数据来自领域层
  design/neko.html           视觉参考（只读）
  screenshots/               主页/资源/主机/设置/流程截图
crates/
  ucloud-api/                唯一 UCloud 客户端（签名/传输/模型/Mock）
  adash-core/                领域层（资源/项目/流程/预算/计划/状态）
  adash-tool/                代理唯一执行面（JSON CLI）
  agent-sdk/                 内置 pi SDK 宿主的 JSONL 边界
  adash-ssh/                 SSH/SFTP/PTY/监控/端口转发
  adash-app/                 GPUI 应用（app/ views/ widgets/ agent/ i18n）
agent/
  prompts/system.md          追加系统提示
  runtime/                   pi SDK 宿主（createAgentSession）
  extensions/ucloud/         pi 扩展：ucloud_* 工具
  skills/                    按需加载的领域知识
fixtures/mock/               Mock 账户数据（demo/测试/截图共用）
third_party/gpui-kit-material  submodule（固定 commit，不改）
```

## 写文档的规矩

- 一处结论只写一遍：规则进 `AGENTS.md`，理由进 `decisions/`，细节进对应专题文档，其余地方链接过去。
- 文档与代码同一个提交更新；改了接口契约、页面结构或安全策略却没改文档，视为未完成。
- 面向贡献者就用中文；代码注释、标识符、提交信息用英文。
