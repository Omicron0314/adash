# UCloud 接口约定

所有 UCloud 请求只在 `crates/ucloud-api` 里发生。官方文档：<https://docs.ucloud.cn/api>。
**不要凭记忆写字段**：新接口先在 `fixtures/mock` 放一份真实结构的响应，再写模型。

## 1. 通用规则

- 协议：`https://api.ucloud.cn/?Action=<Action>&<公共参数>&<业务参数>`，`GET`/`POST` 均可，统一用 `POST` 表单。
- 公共参数：`Action`、`PublicKey`、`Signature`、`ProjectId`、`Region`（若接口需要）。
- 响应：`{"Action":"…","RetCode":0,"Message":"","<业务字段>":…}`；`RetCode != 0` 一律映射为
  `Error::Api { code, message }`，不吞错。
- 字段名保持 UCloud 原样（`PascalCase`），Rust 侧用 `#[serde(rename = "...")]`，避免自造命名。
- 分页：`Offset` + `Limit`，响应里带 `TotalCount`；客户端负责翻页并设上限（防止一次拉爆）。

## 2. 签名

`signature.rs` 负责，纯函数 + 测试向量：

```
stringToSign = 按 key 升序拼接 所有非空参数 形如 "k=v"，用 "&" 连接（v 不 urlencode）
Signature    = base64( sha1( stringToSign + PrivateKey ) )
```

对外只暴露 `sign(params, private_key) -> String`，`PrivateKey` 用包装类型，`Debug` 打印 `<redacted>`。

## 3. 首批接口清单

| 用途 | Action | 备注 |
| --- | --- | --- |
| 账户余额 | `GetBalance` | 主页概览「余额」 |
| 账单明细 | `ListBills` | 按项目分摊、本月花费 |
| 地域列表 | `GetRegion` | 筛选下拉 |
| 云主机 | `DescribeUHostInstance` | 含 `IPSet`、`DiskSet`、`Tag` |
| 轻量主机 | `DescribeULightHostInstance` | 轻量应用主机 |
| 弹性 IP | `DescribeEIP` | 绑定关系决定流程图的边 |
| 云硬盘 | `DescribeUDisk` | 挂载到哪台主机 |
| 云数据库 | `DescribeUDBInstance` | MySQL 等 |
| Redis | `DescribeUDBInstance`（`ClassType=URedis`） | 与 UDB 同一 Action |
| 负载均衡 | `DescribeULB` | `BackendSet` → 边 |
| 对象存储 | `DescribeBucket` / US3 列表 | 桶与域名 |
| 价格 | `GetUHostInstancePrice` 等 | 计划卡片的成本依据 |
| 监控（可选） | `GetUHostInstanceMonitor` | 资源页迷你趋势 |

> 上表的 Action 名在实现时逐条对照官方文档核对，并在 `paths.rs` 里写注释链接；
> 若文档与本文不一致，以官方文档为准并更新本文。

## 4. Mock 约定

- `fixtures/mock/<action>.json`：真实结构样例（脱敏后的真实响应或按文档构造）。
- `UDASH_MOCK=1` 时 `mock::respond(action, params)` 返回对应文件；未覆盖的 action 返回 `Error::Mock`。
- Mock 账户要讲得通：一个「shop-api」项目（UHost ×2 + UDB + Redis + EIP + ULB + US3），
  外加 2 个孤儿 EIP、1 块未挂载云盘、1 台长期空闲主机 —— 供优化建议与流程演示使用。
- `adash-tool reset` 恢复 Mock 基线（写操作在 Mock 下只改本地 JSON）。

## 5. 脱敏

- 输出给模型/UI 的任何结构里，`PublicKey` 只显示前 6 位 + `…`，`PrivateKey` 永不出现。
- 错误信息、日志、会话文件同样规则；改动涉及输出的路径时补一条测试：
  断言渲染后的字符串不含私钥内容。
