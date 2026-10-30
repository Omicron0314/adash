# 最便宜地部署一个仓库

1. `ucloud_inventory` 看现有资源：已有数据库/Redis 能复用就不要新建。
2. `ucloud_price` 比较目标地域的轻量应用主机与云主机，按「月成本 / 能否停机省钱」排序。
3. 生成 `ucloud_plan_deploy`：能同机放的服务放一起，数据库优先复用或选最小规格。
4. 计划里写清 `monthly_cost`、`reversible`、`risks`，然后停下等用户确认。
