# 从代码仓库生成最省钱上云计划

1. 先调用 `ucloud_repo_inspect`，source 用用户给的 GitHub URL 或本机目录。
   它只返回依赖清单、框架、服务线索和证据；**不要**声称读过源码正文、`.env` 或密钥。
2. 对照线索判断最小可行组合：
   - 单台轻量主机足够时不要加负载均衡；只有明确需要多实例/高可用才加 ULB。
   - 有 SQL/ORM 才建议 UDB；有 Redis/队列/缓存才建议 Redis；有 S3 SDK 才建议 US3。
   - 已有 `ucloud_inventory` 资源优先复用，不为每个项目重复建数据库。
3. 调用 `ucloud_plan_repo_deploy`，传 `project`、`source`、`region`，以及你选择的 `services`：
   `[{kind, spec, count}]`。不传时工具会选单台轻量主机 + 公网 IP 和清单要求的服务。
4. 检查返回的月成本、风险和可逆性。**只生成计划**，不要在回答里声称已经创建资源；
   用户确认后 `ucloud_apply_plan` 才进入执行路径。
5. 当前执行器只记录 Mock 部署轨迹；如果用户要求真实部署，明确说明 Live 写接口尚未接入，
   不要编造创建成功。
