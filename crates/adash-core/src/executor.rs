//! Applying a confirmed plan.
//!
//! Shared by `adash-tool` and the app so both paths behave identically. In mock
//! mode the plan is recorded and applied to a local overlay so the resources
//! page and flow history can show the result. No cloud is touched. Live
//! execution lands with M2's write APIs and will call `ucloud-api` from here.

use crate::flow::{FlowRun, FlowStep};
use crate::inventory::Inventory;
use crate::plan::{PlanAction, Verb};
use crate::resource::{Resource, ResourceId, ResourceKind, ResourceState};
use crate::state::{Store, now_secs};

/// What an apply did.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Project the execution is attributed to.
    pub project: String,
    /// The recorded execution.
    pub run: FlowRun,
    /// True when the cloud was not touched (mock).
    pub mock: bool,
}

/// Apply a plan that already passed `Store::plan_for_apply` and whose approval
/// was consumed. Records the flow run and removes the plan.
pub fn apply_mock(
    store: &mut Store,
    inventory: &Inventory,
    plan_id: &str,
) -> Result<Outcome, String> {
    let plan = store
        .plan(plan_id)
        .ok_or_else(|| format!("计划 {plan_id} 不存在"))?
        .clone();

    let project = plan
        .project
        .clone()
        .or_else(|| {
            plan.actions
                .iter()
                .filter_map(|action| action.resource.as_ref())
                .filter_map(|id| inventory.get(id))
                .filter_map(|resource| resource.project.clone())
                .next()
        })
        .unwrap_or_else(|| "unassigned".to_owned());

    // Make confirmed Mock effects visible to every subsequent inventory read.
    for (index, action) in plan.actions.iter().enumerate() {
        match action.verb {
            Verb::Create => {
                store.data.mock.created.push(mock_created(
                    action,
                    plan.project.as_deref(),
                    &plan.id,
                    index,
                ));
            }
            Verb::Release => {
                if let Some(id) = &action.resource {
                    store.data.mock.released.insert(id.key());
                }
            }
            Verb::Stop | Verb::Start | Verb::Resize => {
                let Some(id) = action.resource.as_ref() else {
                    continue;
                };
                let Some(original) = inventory.get(id).cloned() else {
                    continue;
                };
                let mut updated = original;
                match action.verb {
                    Verb::Stop => updated.state = ResourceState::Stopped,
                    Verb::Start => updated.state = ResourceState::Running,
                    Verb::Resize => {
                        updated.spec = action.spec.clone();
                        updated.monthly_cost_cents = action.monthly_cents;
                    }
                    _ => {}
                }
                store.data.mock.overrides.insert(id.key(), updated);
            }
        }
    }

    let mut at = now_secs();
    let steps: Vec<FlowStep> = plan
        .actions
        .iter()
        .map(|action| {
            at += 1;
            FlowStep {
                node: action
                    .resource
                    .as_ref()
                    .map_or_else(|| "ext:repo".to_owned(), ResourceId::key),
                label: action.summary(),
                at,
            }
        })
        .collect();

    let run = FlowRun {
        id: format!("run-{}", uuid::Uuid::new_v4()),
        project: project.clone(),
        label: plan.kind.label().to_owned(),
        started_at: now_secs(),
        steps,
    };
    store.push_run(run.clone());
    store.take_plan(plan_id);
    Ok(Outcome {
        project,
        run,
        mock: true,
    })
}

fn mock_created(
    action: &PlanAction,
    project: Option<&str>,
    plan_id: &str,
    index: usize,
) -> Resource {
    let suffix: String = plan_id
        .trim_start_matches("plan-")
        .chars()
        .take(8)
        .collect();
    let project = project.unwrap_or("app");
    let id = ResourceId::new(
        action.kind,
        format!("{}-{}-{}", action.kind.as_str(), suffix, index + 1),
    );
    let mut attributes = std::collections::BTreeMap::new();
    attributes.insert("来源".to_owned(), "Mock 部署计划".to_owned());
    attributes.insert("可逆".to_owned(), "是".to_owned());
    Resource {
        id,
        name: format!("{project}-{}-{}", action.kind.as_str(), index + 1),
        region: action.region.clone(),
        zone: None,
        state: ResourceState::Running,
        project: Some(project.to_owned()),
        spec: action.spec.clone(),
        monthly_cost_cents: action.monthly_cents,
        public_ip: (action.kind == ResourceKind::Eip)
            .then(|| format!("198.51.100.{}", 10 + index % 200)),
        private_ip: None,
        attached_to: Vec::new(),
        attributes,
        created_at: Some(now_secs()),
    }
}

/// Demo history for the flow page: one deploy of `shop-api`.
///
/// Written by `adash-tool reset` and on first launch in mock mode, so the
/// replay button has something real to walk through.
type DemoRun = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

pub fn seed_demo(store: &mut Store) {
    let now = now_secs();
    for (slug, name, repo, note, budget_cents) in [
        (
            "shop-api",
            "Shop API",
            "github.com/demo/shop-api",
            "生产电商 API：2 台 Web + MySQL + Redis + ULB，流量入口按双可用区展示",
            70_000,
        ),
        (
            "data-pipeline",
            "Data Pipeline",
            "github.com/demo/data-pipeline",
            "夜间批处理：8C worker + 分析库 + 队列；保留一台停机 worker 观察成本",
            150_000,
        ),
        (
            "portfolio",
            "Portfolio",
            "github.com/demo/portfolio",
            "香港个人作品集：单台小规格主机 + 静态资源盘",
            15_000,
        ),
    ] {
        store.set_project(crate::state::ProjectRecord {
            slug: slug.into(),
            name: name.into(),
            repo: Some(repo.into()),
            note: note.into(),
            budget_cents,
        });
    }

    let runs: [DemoRun; 3] = [
        (
            "shop-api",
            "部署",
            &[
                ("ext:repo", "拉取 github.com/demo/shop-api"),
                ("uhost:uhost-shopweb1", "创建 shop-api-web-1"),
                ("uhost:uhost-shopweb2", "创建 shop-api-web-2"),
                ("udb:udb-shopdb1", "初始化 shop-api-db"),
                ("redis:umem-shopcache1", "初始化 shop-api-cache"),
                ("ulb:ulb-shopapi1", "挂载负载均衡后端"),
                ("eip:eip-shopweb1", "绑定公网 IP 1"),
                ("eip:eip-shopweb2", "绑定公网 IP 2"),
                ("disk:udisk-shopdata1", "挂载 100GiB 数据盘"),
                ("disk:udisk-shopbackup1", "挂载 200GiB 备份盘"),
            ],
        ),
        (
            "data-pipeline",
            "部署",
            &[
                ("ext:repo", "拉取 github.com/demo/data-pipeline"),
                ("uhost:uhost-batch1", "创建 8C batch-worker-1"),
                ("uhost:uhost-batch2", "创建 4C batch-worker-2"),
                ("udb:udb-analytics1", "初始化 batch-analytics-db"),
                ("redis:umem-batchqueue1", "初始化 batch-job-queue"),
                ("disk:udisk-batchscratch", "挂载 500GiB scratch 盘"),
            ],
        ),
        (
            "portfolio",
            "部署",
            &[
                ("ext:repo", "拉取 github.com/demo/portfolio"),
                ("uhost:uhost-portfolio1", "创建香港 portfolio-web-1"),
                ("eip:eip-portfolio1", "绑定公网 IP"),
                ("disk:udisk-portfolio1", "挂载 50GiB 资源盘"),
            ],
        ),
    ];
    for (project, label, steps) in runs {
        store.push_run(FlowRun {
            id: format!("run-demo-{}", uuid::Uuid::new_v4()),
            project: project.into(),
            label: label.into(),
            started_at: now - 3600,
            steps: steps
                .iter()
                .enumerate()
                .map(|(index, (node, label))| FlowStep {
                    node: (*node).into(),
                    label: (*label).into(),
                    at: now - 3600 + index as i64,
                })
                .collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Plan, PlanKind, create_action};
    use crate::resource::ResourceKind;
    use ucloud_api::UcloudClient;

    fn store(name: &str) -> Store {
        let path =
            std::env::temp_dir().join(format!("adash-exec-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Store::open(path)
    }

    #[test]
    fn applying_records_a_run_and_drops_the_plan() {
        let mut store = store("apply");
        let inventory = Inventory::load(&UcloudClient::mock());
        let plan = Plan::new(
            PlanKind::Deploy,
            "部署 blog",
            create_action(ResourceKind::UHost, "2C / 4GiB", "cn-bj2", 1),
        )
        .with_project("blog");
        let id = store.insert_plan(plan);

        let outcome = apply_mock(&mut store, &inventory, &id).expect("apply");
        assert_eq!(outcome.project, "blog");
        assert_eq!(outcome.run.steps.len(), 1);
        assert!(store.plan(&id).is_none());
        assert_eq!(store.runs_of("blog").len(), 1);
        let after = Inventory::load_with_store(&UcloudClient::mock(), &store);
        assert_eq!(after.resources.len(), 21);
        assert!(
            after
                .get(&ResourceId::new(
                    ResourceKind::UHost,
                    &store.data.mock.created[0].id.id
                ))
                .is_some()
        );
        assert_eq!(store.data.mock.created[0].project.as_deref(), Some("blog"));
    }

    #[test]
    fn mock_release_changes_the_inventory_and_clears_findings() {
        let mut store = store("release");
        let client = UcloudClient::mock();
        let inventory = Inventory::load(&client);
        let findings = crate::optimizer::analyze(&inventory);
        assert_eq!(findings.len(), 4);
        let plan = crate::optimizer::plan(&findings);
        let id = store.insert_plan(plan);
        apply_mock(&mut store, &inventory, &id).expect("apply");
        let after = Inventory::load_with_store(&client, &store);
        assert_eq!(after.resources.len(), 16);
        assert!(crate::optimizer::analyze(&after).is_empty());
        assert_eq!(after.monthly_total_cents(), 249_240 - 50_080);
    }

    #[test]
    fn seeded_demo_has_replayable_runs_for_each_project() {
        let mut store = store("seed");
        seed_demo(&mut store);
        for (project, steps, budget) in [
            ("shop-api", 10, 70_000),
            ("data-pipeline", 6, 150_000),
            ("portfolio", 4, 15_000),
        ] {
            let runs = store.runs_of(project);
            assert_eq!(runs.len(), 1, "{project}");
            assert_eq!(runs[0].steps.len(), steps, "{project}");
            assert_eq!(
                store.project(project).expect("record").budget_cents,
                budget,
                "{project}"
            );
        }
    }
}
