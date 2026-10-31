# 黑客松演示剧本（5 分钟）

> 数据全部来自编译内置的 Mock；不会调用真实 UCloud 写接口，不会产生费用。
> 需要展示 AI 对话/仓库分析时，先在“账户”页配置 AI 服务；SSH 可选。

重置演示数据（保留 AI/UCloud 账户配置）：

```sh
UDASH_MOCK=1 target/desktop/adash-tool reset
```

## 0. 开场（20s）

打开 adash。标题栏 `MOCK`，主页右侧账户概览：

- 20 个资源，余额 ¥3280.55
- 本月花费 ¥2515.40
- 资源月估算 ¥2492.40
- 3 个项目：Shop API、Data Pipeline、Portfolio
- 预算 ¥2363.40 / ¥2350.00（超 ¥13.40）
- 4 条待处理，可省 ¥500.80 / 月

主页快捷入口下方有一行“部署仓库”提示：直接发 GitHub 地址或本机路径。

## 1. 盘点资源（45s）

点“云资源”：

- 切换项目：Shop API（9 个资源）、Data Pipeline（5）、Portfolio（3）
- 切换地域：北京二、上海二、香港
- 点 `batch-worker-2 (stopped)`：展示“已停止但仍计费”
- 点 `old-demo-eip` 或 `hk-hackathon-eip`：展示未绑定 EIP
- 点 `temp-import-disk`：展示未挂载的 200GiB 云盘

讲解：资源不是写死的展示数据，而是 `ucloud-api` 的 Mock 响应经 `adash-core` 归一化后的同一份领域数据。

## 2. 看懂项目（60s）

点“AI 流程”，默认是 Shop API：

- 10 个节点、12 条连线、1 次部署执行
- 实线：EIP 绑定、云盘挂载、ULB 后端
- 虚线：推断关系
- 点“回放”：按部署顺序高亮 GitHub → 主机 → 数据库/缓存 → ULB/EIP/云盘

再切换 Data Pipeline（6 节点）和 Portfolio（4 节点）；未分组的孤儿 EIP/云盘在 `unassigned` 流程里。

## 3. 省钱计划（60s）

回到“对话”，点击“寻找省钱机会”或输入：

```text
有浪费的钱吗？
```

预期 4 条：

- 2 个未绑定 EIP：¥24 + ¥24 / 月
- 1 块未挂载 200GiB 云盘：¥80 / 月
- 1 台停机仍计费的 4C worker：¥372.80 / 月

合计 **¥500.80 / 月**。计划卡片点“确认并执行”（Mock）：

- 资源页从 20 个降到 16 个，待处理清零
- 资源月估算从 ¥2492.40 降到 ¥1991.60
- “本月花费”是已发生账单，不随 Mock 释放回退
- 流程历史出现一次优化执行

## 4. 从仓库生成最省钱计划（90s）

在“账户”确认 AI 已配置。回到主页，直接发送：

```text
请分析 /path/to/adash/fixtures/demo/shop-api，生成最省钱的上云计划
# 把 /path/to/adash 换成本仓库的绝对路径；也可以直接拖入本地仓库路径
```

也可以换成公开 GitHub 地址（需要网络）：

```text
请分析 https://github.com/chalk/chalk，生成最省钱的上云计划
```

演示重点：

- AI 先调用 `ucloud_repo_inspect`，只读取 `package.json`、Dockerfile、compose，不读取 `.env`/源码/密钥
- 推断 Node.js + Express + PostgreSQL + Redis + S3
- 调用 `ucloud_plan_repo_deploy`，默认 **1 台轻量主机 + 1 个 EIP + UDB + Redis + US3**，不默认加 ULB
- 计划卡片显示月成本、风险、可逆性；用户确认前不执行

Mock 确认后会显示新建的 5 个资源（资源页/项目计数/流程执行记录）；真实创建 API 尚未接入，
Live 模式下确认会被明确拒绝，不会假装部署成功。

## 5. 账户与边界（45s）

点“账户”：

- UCloud 云账户：PublicKey/ProjectId/地域/Endpoint，PrivateKey 只进系统钥匙串；
  保存前做只读余额校验，支持切换 Mock/移除
- AI 服务：Endpoint、协议、模型、API key；密钥只进钥匙串，换 Endpoint 必须重填
- 演示 Mock 时无需真实凭据；真实账户只做读，写操作仍待接入

最后强调：

1. 所有变更必须经过计划卡片确认
2. 计划有哈希、过期时间、一次性批准
3. 仓库分析只给清单摘要，不把源码或密钥送进模型
4. Mock 与 Live 在标题栏和账户页都可见

## 6. Shell / 身份 / 片段 / 端口转发（可选，60s）

赛前或现场先启动服务，再打开 adash：

```sh
python3 scripts/demo-ssh-service.py
```

它会启动仅监听 `127.0.0.1` 的临时 sshd，并写入：

- 主机：**Demo Local SSH**
- 身份：**Demo Local Key**（演示私钥文件，不使用真实账号）
- 片段：运行时间、工作目录、磁盘、健康检查
- 端口转发：**Demo HTTP** → 演示静态站点

点“主机 → Demo Local SSH → 连接”：

- 终端显示演示 shell，工作目录是 `~/.local/share/adash/demo-ssh/work`
- “文件”可浏览/编辑 README、日志和配置示例
- “监控”显示本机 `/proc`/`df` 实际采样
- “身份”“片段”“端口转发”页都有内容，可执行片段和启动转发
- 终端里可直接试 `ls -la`、`cat logs/app.log`

停止服务：

```sh
python3 scripts/demo-ssh-service.py --stop
```

注意：这是本机用户权限的回环演示服务，不是安全沙箱；不要把它当作生产 SSH 隔离方案。

## 备用 2 分钟版

主页概览（20 资源）→ AI 流程 Shop API 回放 → 寻找省钱机会（¥500.80）→ 确认 Mock 释放 →
主页发送本地 `fixtures/demo/shop-api` → 计划卡片 → 账户页边界说明。

## 出问题时的兜底

- AI 未配置：仍可演示资源、流程图、优化计划、账户页；仓库部署用 CLI 说明计划生成，
  或在赛前配置好 AI。
- 网络不可用：用本机 `fixtures/demo/shop-api`，不要用 GitHub 地址。
- SSH：运行 `python3 scripts/demo-ssh-service.py`，会启动仅监听 127.0.0.1 的临时 sshd，
  自动写入“Demo Local SSH”主机、Demo 私钥身份、4 个片段和 Demo HTTP 端口转发规则。
  回到“主机”点 Demo Local SSH → 连接，即可展示终端、SFTP、监控、身份、片段和转发。
  停止：`python3 scripts/demo-ssh-service.py --stop`。服务使用独立 demo 工作目录，
  不会修改系统 sshd/账号；它仍是本机用户的 shell，不是安全隔离沙箱。
