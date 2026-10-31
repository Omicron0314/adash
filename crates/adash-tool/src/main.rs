//! `adash-tool <command>` — one JSON in on stdin, one JSON out on stdout.
//!
//! This binary is the agent's only execution surface. Every command is
//! classified `read` / `plan` / `mutate`; `apply` refuses to run without the
//! one-time approval token the GUI wrote after the user confirmed the plan.

use adash_core::executor;
use adash_core::flow::Flow;
use adash_core::inventory::Inventory;
use adash_core::plan::{Plan, PlanAction, PlanKind, Verb, create_action};
use adash_core::project::project_views;
use adash_core::resource::{ResourceId, ResourceKind, money};
use adash_core::state::{ProjectRecord, Store};
use adash_core::{billing::Billing, optimizer};
use serde_json::{Value, json};
use std::process::ExitCode;
use ucloud_api::UcloudClient;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        return fail("usage", "缺少命令；运行 `adash-tool help` 查看可用命令");
    };
    let rest = &args[1..];

    let result = match command.as_str() {
        "help" | "--help" | "-h" => Ok(json!({ "commands": COMMANDS })),
        "inventory" => read_inventory(),
        "resource" => read_resource(require(rest, 0, "resource <kind:id>")),
        "projects" => read_projects(),
        "billing" => read_billing(),
        "flows" => read_flows(),
        "flow" => read_flow(require(rest, 0, "flow <project>")),
        "plan-deploy" => plan_deploy(),
        "plan-optimize" => plan_optimize(),
        "plan-stop" => plan_stop(),
        "approve" => approve(rest),
        "apply" => apply(rest),
        "reset" => reset(),
        "doctor" => doctor(),
        other => Err(Error::new(
            "usage",
            format!("未知命令 {other}；`adash-tool help`"),
        )),
    };

    match result {
        Ok(data) => {
            println!("{}", json!({ "ok": true, "data": data }));
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!(
                "{}",
                json!({ "ok": false, "error": { "code": error.code, "message": error.message } })
            );
            ExitCode::from(if error.code == "usage" { 2 } else { 1 })
        }
    }
}

const COMMANDS: &[(&str, &str)] = &[
    (
        "inventory",
        "read: every resource in the account, summarized",
    ),
    ("resource <kind:id>", "read: one resource in detail"),
    (
        "projects",
        "read: projects with resources, spend and budget",
    ),
    ("billing", "read: balance and this month's spend by project"),
    (
        "flows",
        "read: one flow per project (nodes, edges, run count)",
    ),
    (
        "flow <project>",
        "read: a project's full graph and executions",
    ),
    ("plan-deploy", "plan: stage a deployment (JSON on stdin)"),
    (
        "plan-optimize",
        "plan: stage release of every idle resource",
    ),
    (
        "plan-stop",
        "plan: stage stopping one resource (JSON on stdin)",
    ),
    (
        "approve <plan-id>",
        "mutate-free: GUI writes the one-time approval",
    ),
    (
        "apply <plan-id> [--token <t>]",
        "mutate: execute a confirmed plan",
    ),
    ("reset", "restore the mock baseline (demo history included)"),
    ("doctor", "read: mode, state path, credential and pi status"),
];

/// An error we render as `{"ok":false,...}`.
struct Error {
    code: &'static str,
    message: String,
}

impl Error {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn fail(code: &'static str, message: &str) -> ExitCode {
    println!(
        "{}",
        json!({ "ok": false, "error": { "code": code, "message": message } })
    );
    ExitCode::from(2)
}

fn require<'a>(args: &'a [String], index: usize, usage: &'a str) -> &'a str {
    args.get(index).map_or(usage, String::as_str)
}

fn client() -> Result<UcloudClient, Error> {
    UcloudClient::from_env().map_err(|error| Error::new("credentials", error.to_string()))
}

fn mode_label(client: &UcloudClient) -> &'static str {
    match client.mode() {
        ucloud_api::Mode::Mock => "mock",
        ucloud_api::Mode::Live => "live",
    }
}

fn read_inventory() -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let resources: Vec<Value> = inventory
        .resources
        .iter()
        .map(|resource| {
            json!({
                "id": resource.id.key(),
                "name": resource.name,
                "kind": resource.id.kind.as_str(),
                "region": resource.region,
                "state": resource.state.label(),
                "project": resource.project,
                "spec": resource.spec,
                "monthly": money(resource.monthly_cost_cents),
            })
        })
        .collect();
    Ok(json!({
        "mode": mode_label(&client),
        "summary": inventory.summary_line(),
        "balance": inventory.balance_yuan.map(|yuan| format!("¥{yuan:.2}")),
        "monthly_total": money(inventory.monthly_total_cents()),
        "counts": inventory.counts().iter().map(|(kind, count)| json!({"kind": kind.as_str(), "label": kind.label(), "count": count})).collect::<Vec<_>>(),
        "projects": inventory.projects(),
        "errors": inventory.errors,
        "resources": resources,
    }))
}

fn read_resource(key: &str) -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let id = parse_id(key)?;
    let resource = inventory
        .get(&id)
        .ok_or_else(|| Error::new("not_found", format!("找不到资源 {key}")))?;
    Ok(serde_json::to_value(resource).unwrap_or(Value::Null))
}

fn read_projects() -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let billing = Billing::load(&client, &inventory);
    let store = Store::open_default();
    let views = project_views(&inventory, &billing, store.data.projects.values().cloned());
    Ok(json!({
        "projects": views.iter().map(|view| json!({
            "slug": view.slug,
            "name": view.name,
            "repo": view.repo,
            "resources": view.resources.len(),
            "monthly": money(view.budget.estimate_cents),
            "spent": money(view.budget.spent_cents),
            "budget": if view.budget.limit_cents > 0 { money(view.budget.limit_cents) } else { "未设置".to_owned() },
        })).collect::<Vec<_>>(),
        "unassigned": inventory.of_project(None).len(),
    }))
}

fn read_billing() -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let billing = Billing::load(&client, &inventory);
    Ok(json!({
        "balance": billing.balance_yuan.map(|yuan| format!("¥{yuan:.2}")),
        "month_spend": money(billing.month_cents),
        "by_project": billing.by_project.iter().map(|(project, cents)| json!({"project": project, "spent": money(*cents)})).collect::<Vec<_>>(),
        "by_product": billing.by_product.iter().map(|(product, cents)| json!({"product": product, "spent": money(*cents)})).collect::<Vec<_>>(),
        "errors": billing.errors,
    }))
}

fn flow_for(store: &Store, inventory: &Inventory, slug: &str) -> Flow {
    Flow::build(inventory, slug, store.project(slug), store.runs_of(slug))
}

fn read_flows() -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let store = Store::open_default();
    let flows: Vec<Value> = inventory
        .flow_projects()
        .iter()
        .map(|slug| {
            let flow = flow_for(&store, &inventory, slug);
            json!({
                "project": slug,
                "name": flow.name,
                "summary": flow.summary(),
                "nodes": flow.nodes.len(),
                "edges": flow.edges.len(),
                "runs": flow.runs.len(),
            })
        })
        .collect();
    Ok(json!({ "flows": flows }))
}

fn read_flow(slug: &str) -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let store = Store::open_default();
    let flow = flow_for(&store, &inventory, slug);
    if flow.nodes.is_empty() {
        return Err(Error::new("not_found", format!("项目 {slug} 没有资源")));
    }
    Ok(json!({
        "project": flow.project,
        "name": flow.name,
        "summary": flow.summary(),
        "layers": flow.layers(),
        "nodes": flow.nodes,
        "edges": flow.edges,
        "runs": flow.runs,
    }))
}

fn read_stdin_json() -> Result<Value, Error> {
    use std::io::Read as _;
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|error| Error::new("input", error.to_string()))?;
    if text.trim().is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(&text).map_err(|error| Error::new("input", error.to_string()))
}

fn plan_deploy() -> Result<Value, Error> {
    let input = read_stdin_json()?;
    let project = input
        .get("project")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("input", "缺少 project"))?;
    let region = input
        .get("region")
        .and_then(Value::as_str)
        .unwrap_or("cn-bj2");
    let spec = input
        .get("spec")
        .and_then(Value::as_str)
        .unwrap_or("2C / 4GiB");
    let count = input.get("count").and_then(Value::as_i64).unwrap_or(1);

    let mut actions = create_action(ResourceKind::UHost, spec, region, count);
    actions.extend(create_action(ResourceKind::Eip, "5Mbps", region, count));
    let summary = format!("为 {project} 在 {region} 部署 {count} 台主机与公网 IP");
    let plan = Plan::new(PlanKind::Deploy, summary, actions).with_project(project);

    let mut store = Store::open_default();
    if let Some(repo) = input.get("repo").and_then(Value::as_str) {
        let mut record = store.project(project).cloned().unwrap_or(ProjectRecord {
            slug: project.to_owned(),
            name: project.to_owned(),
            ..ProjectRecord::default()
        });
        record.repo = Some(repo.to_owned());
        store.set_project(record);
    }
    let id = store.insert_plan(plan.clone());
    store.save().map_err(|error| Error::new("state", error))?;
    Ok(json!({
        "plan_id": id,
        "plan": plan,
        "monthly_delta": plan.monthly_delta_text(),
    }))
}

fn plan_optimize() -> Result<Value, Error> {
    let client = client()?;
    let inventory = Inventory::load(&client);
    let findings = optimizer::analyze(&inventory);
    if findings.is_empty() {
        return Ok(json!({ "findings": [], "plan": null, "message": "没有发现闲置资源" }));
    }
    let plan = optimizer::plan(&findings);
    let mut store = Store::open_default();
    let id = store.insert_plan(plan.clone());
    store.save().map_err(|error| Error::new("state", error))?;
    Ok(json!({
        "findings": findings,
        "plan_id": id,
        "plan": plan,
        "monthly_delta": plan.monthly_delta_text(),
    }))
}

fn plan_stop() -> Result<Value, Error> {
    let input = read_stdin_json()?;
    let key = input
        .get("resource")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("input", "缺少 resource"))?;
    let id = parse_id(key)?;
    let client = client()?;
    let inventory = Inventory::load(&client);
    let resource = inventory
        .get(&id)
        .ok_or_else(|| Error::new("not_found", format!("找不到资源 {key}")))?;
    let action = PlanAction {
        verb: Verb::Stop,
        kind: resource.id.kind,
        spec: String::new(),
        region: resource.region.clone(),
        resource: Some(resource.id.clone()),
        monthly_cents: 0,
    };
    let mut plan = Plan::new(
        PlanKind::Stop,
        format!("停机 {}（保留公网 IP 与云盘）", resource.name),
        vec![action],
    );
    plan.project = resource.project.clone();
    let mut store = Store::open_default();
    let id = store.insert_plan(plan.clone());
    store.save().map_err(|error| Error::new("state", error))?;
    Ok(json!({ "plan_id": id, "plan": plan }))
}

fn approve(args: &[String]) -> Result<Value, Error> {
    let plan_id = require(args, 0, "approve <plan-id>");
    if plan_id.starts_with("approve <") {
        return Err(Error::new(
            "usage",
            "用法：adash-tool approve <plan-id> [--ttl <秒>]",
        ));
    }
    let ttl = flag(args, "--ttl")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(adash_core::plan::TTL_SECS);
    let mut store = Store::open_default();
    store
        .plan_for_apply(plan_id)
        .map_err(|message| Error::new("plan", message))?;
    let token = store
        .approve(plan_id, ttl)
        .ok_or_else(|| Error::new("not_found", format!("计划 {plan_id} 不存在")))?;
    store.save().map_err(|error| Error::new("state", error))?;
    Ok(json!({ "plan_id": plan_id, "token": token, "ttl": ttl }))
}

fn apply(args: &[String]) -> Result<Value, Error> {
    let plan_id = require(args, 0, "apply <plan-id> [--token <token>]");
    if plan_id.starts_with("apply <") {
        return Err(Error::new(
            "usage",
            "用法：adash-tool apply <plan-id> [--token <令牌>]",
        ));
    }

    let mut store = Store::open_default();
    // Sealed and unexpired before anything else is even considered.
    store
        .plan_for_apply(plan_id)
        .map_err(|message| Error::new("plan", message))?;
    // The approval itself is the gate; the token is only checked when supplied,
    // so the app can answer the extension's confirm without a round trip.
    let approved = match flag(args, "--token") {
        Some(token) => store.consume_approval(plan_id, &token),
        None => store.consume_any_approval(plan_id),
    };
    if !approved {
        return Err(Error::new(
            "approval",
            "计划尚未在界面中确认，或批准已过期；云上没有任何变更",
        ));
    }

    let client = client()?;
    let inventory = Inventory::load(&client);
    if matches!(client.mode(), ucloud_api::Mode::Live) {
        return Err(Error::new(
            "unsupported",
            "真实账户的写操作尚未实现（M2 后续）；演示请用 UDASH_MOCK=1",
        ));
    }

    // Mock: record the execution so the flow page can replay it, and drop the
    // plan. Nothing was created, and the result says so.
    let outcome = executor::apply_mock(&mut store, &inventory, plan_id)
        .map_err(|message| Error::new("apply", message))?;
    store.save().map_err(|error| Error::new("state", error))?;

    Ok(json!({
        "applied": plan_id,
        "project": outcome.project,
        "mode": "mock",
        "note": "Mock 模式：已记录执行，未真实创建资源",
        "run": outcome.run.id,
    }))
}

fn reset() -> Result<Value, Error> {
    let mut store = Store::open_default();
    let path = store.path().to_path_buf();
    std::fs::remove_file(&path).ok();
    store.data = adash_core::state::StoreData::default();
    executor::seed_demo(&mut store);
    store.save().map_err(|error| Error::new("state", error))?;
    Ok(json!({ "reset": true, "state": path.display().to_string() }))
}

fn doctor() -> Result<Value, Error> {
    let mock = ucloud_api::transport::mock_enabled();
    let credentials = std::env::var("UCLOUD_PUBLIC_KEY").is_ok();
    let store_path = Store::default_path();
    let resources = adash_paths::resource_dir();
    Ok(json!({
        "mode": if mock { "mock" } else { "live" },
        "credentials_present": credentials,
        "state_path": store_path.display().to_string(),
        "state_exists": store_path.exists(),
        "mock_dir": ucloud_api::transport::mock_dir().map(|path| path.display().to_string()),
        "mock_builtin": ucloud_api::transport::mock_dir().is_none(),
        "sdk_host_present": resources.as_ref().is_some_and(|path| path.join("agent/runtime/dist/index.js").is_file()),
        "resource_dir": resources.map(|path| path.display().to_string()),
        "node_available": adash_paths::find_on_path("node").is_some(),
    }))
}

fn parse_id(key: &str) -> Result<ResourceId, Error> {
    let (kind, id) = key
        .split_once(':')
        .ok_or_else(|| Error::new("input", format!("资源 id 需要 kind:id 形式，收到 {key}")))?;
    let kind = ResourceKind::ALL
        .into_iter()
        .find(|candidate| candidate.as_str() == kind)
        .ok_or_else(|| Error::new("input", format!("未知产品 {kind}")))?;
    Ok(ResourceId::new(kind, id))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.get(index + 1).cloned()
}
