# adash

> 桌面端 UCloud 控制台 + 对话式代理：一句话管云资源、云项目与预算，
> 还能像 n8n 一样看清「这个项目到底用了哪些服务」。

基于 [catussh](https://github.com/mizorewww/catussh) 的外壳骨架，Rust + [GPUI](https://github.com/longbridge/gpui-kit) +
[gpui-kit-material](https://github.com/mizorewww/gpui-kit-material)（Material 3 Expressive）。
代理基于 [pi](https://github.com/earendil-works/pi)（`pi --mode rpc`），所有云操作经 `adash-tool` 的 JSON CLI。

## 三个页面

| 对话（主页） | 云资源 | AI 流程 |
| --- | --- | --- |
| 代理对话 + 账户概览 | 已创建的全部资源 | 项目用了哪些服务、怎么连的 |
| 一句话 → 计划卡片 → 一键确认 → 结果 | 按产品/地域/项目筛选，点进详情 | n8n 风格节点图 + 执行回放 |

## 快速开始（Mock，不花钱）

```sh
git clone --recursive <repo>
cargo build -p adash-tool -p adash-app
UDASH_MOCK=1 cargo run -p adash-app
```

需要已安装并配置好模型的 `pi`。`adash-tool` 必须与 app 在同一个 `target/debug`，
或用 `UDASH_TOOL=/path/to/adash-tool` 指定。

真实账户：设置 `UCLOUD_PUBLIC_KEY` / `UCLOUD_PRIVATE_KEY` / `UCLOUD_PROJECT_ID`
（或写入系统钥匙串），不要设置 `UDASH_MOCK`。

## 文档

从 [AGENTS.md](AGENTS.md)（规则）开始，再看 [docs/](docs/README.md) 的索引。
