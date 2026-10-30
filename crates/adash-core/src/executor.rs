//! Applying a confirmed plan.
//!
//! Shared by `adash-tool` and the app so both paths behave identically. In mock
//! mode nothing is created: the execution is recorded so the flow page can
//! replay it, and the result says so. Live execution lands with M2's write
//! APIs and will call `ucloud-api` from here.

use crate::flow::{FlowRun, FlowStep};
use crate::inventory::Inventory;
use crate::resource::ResourceId;
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

/// Demo history for the flow page: one deploy of `shop-api`.
///
/// Written by `adash-tool reset` and on first launch in mock mode, so the
/// replay button has something real to walk through.
pub fn seed_demo(store: &mut Store) {
    let now = now_secs();
    store.set_project(crate::state::ProjectRecord {
        slug: "shop-api".into(),
        name: "Shop API".into(),
        repo: Some("github.com/me/shop-api".into()),
        note: "演示项目：2 台主机 + MySQL + Redis，前面是 ULB".into(),
        budget_cents: 60_000,
    });
    let steps: [(&str, &str); 8] = [
        ("ext:repo", "拉取 github.com/me/shop-api"),
        ("uhost:uhost-shopweb1", "创建 shop-api-web-1"),
        ("uhost:uhost-shopweb2", "创建 shop-api-web-2"),
        ("udb:udb-shopdb1", "初始化 shop-api-db"),
        ("redis:umem-shopcache1", "初始化 shop-api-cache"),
        ("ulb:ulb-shopapi1", "挂载负载均衡后端"),
        ("eip:eip-shopweb1", "绑定公网 IP"),
        ("eip:eip-shopweb2", "绑定公网 IP"),
    ];
    store.push_run(FlowRun {
        id: format!("run-demo-{}", uuid::Uuid::new_v4()),
        project: "shop-api".into(),
        label: "部署".into(),
        started_at: now - 3600,
        steps: steps
            .into_iter()
            .enumerate()
            .map(|(index, (node, label))| FlowStep {
                node: node.to_owned(),
                label: label.to_owned(),
                at: now - 3600 + index as i64,
            })
            .collect(),
    });
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
    }

    #[test]
    fn seeded_demo_has_one_replayable_run() {
        let mut store = store("seed");
        seed_demo(&mut store);
        let runs = store.runs_of("shop-api");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].steps.len(), 8);
        assert_eq!(
            store.project("shop-api").expect("record").budget_cents,
            60_000
        );
    }
}
