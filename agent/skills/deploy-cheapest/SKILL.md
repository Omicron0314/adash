# 最便宜地部署一个仓库

1. `ucloud_inventory` 看现有资源：已有数据库/Redis 能复用就不要新建。
2. 有代码仓库时先 `ucloud_repo_inspect`，按清单推断最小服务组合；不要臆造依赖。
3. 用 `ucloud_plan_repo_deploy`（或纯主机场景的 `ucloud_plan_deploy`）生成计划；
   能同机放的服务放一起，数据库优先复用或选最小规格，单实例不默认加 ULB。
4. 计划里写清月成本、可逆性与风险，然后停下等用户确认；不要声称已创建资源。
