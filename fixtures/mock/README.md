# Mock 数据

`UDASH_MOCK=1` 时 `ucloud-api` 从这里读响应，文件名 = UCloud `Action`（如
`DescribeUHostInstance.json`），内容与真实响应同结构（含 `Action` / `RetCode` / `Message`）。

Mock 账户要讲得通：

- 一个项目 `shop-api`：UHost ×2（cn-bj2）、UDB MySQL ×1、Redis ×1、EIP ×2（一绑一闲）、
  ULB ×1（后端是那两台 UHost）、US3 桶 ×1。
- 浪费项：孤儿 EIP ×2、未挂载云盘 ×1、长期空闲 UHost ×1 —— 供货架页与优化演示。
- 另一个项目 `blog`：轻量应用主机 ×1 + US3 桶 ×1，用于流程页切换演示。

写操作在 Mock 下只修改本目录（或本地状态目录）里的 JSON，不动云。
