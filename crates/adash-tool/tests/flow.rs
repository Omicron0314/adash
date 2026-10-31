//! End-to-end tests of the agent's execution surface, in mock mode.
//!
//! These run the real binary: what the agent can and cannot do is a property of
//! the process boundary, so testing it in-process would prove nothing.

use serde_json::Value;
use std::io::Write as _;
use std::process::{Command, Stdio};

/// A throw-away HOME so each test gets its own state file.
struct Sandbox {
    home: std::path::PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let home = std::env::temp_dir().join(format!("adash-tool-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("sandbox home");
        Self { home }
    }

    fn state_path(&self) -> std::path::PathBuf {
        self.home.join(".local/share/adash/state.json")
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Value {
        let mut command = Command::new(env!("CARGO_BIN_EXE_adash-tool"));
        command
            .args(args)
            .env("UDASH_MOCK", "1")
            .env("HOME", &self.home)
            .env_remove("UDASH_MOCK_DIR")
            .env_remove("UCLOUD_PUBLIC_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn adash-tool");
        if let Some(text) = stdin {
            child
                .stdin
                .as_mut()
                .expect("stdin")
                .write_all(text.as_bytes())
                .expect("write stdin");
        }
        drop(child.stdin.take());
        let output = child.wait_with_output().expect("wait");
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("not JSON: {error}\nstdout: {stdout}"))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn ok(value: &Value) -> &Value {
    assert_eq!(value["ok"], true, "expected ok, got {value}");
    &value["data"]
}

fn error_code(value: &Value) -> &str {
    assert_eq!(value["ok"], false, "expected failure, got {value}");
    value["error"]["code"].as_str().unwrap_or("")
}

#[test]
fn inventory_summarizes_the_mock_account() {
    let sandbox = Sandbox::new("inventory");
    let output = sandbox.run(&["inventory"], None);
    let data = ok(&output);
    assert_eq!(data["mode"], "mock");
    assert_eq!(data["resources"].as_array().expect("resources").len(), 11);
    assert!(
        data["summary"]
            .as_str()
            .expect("summary")
            .contains("11 个资源")
    );
}

#[test]
fn unknown_command_is_a_usage_error() {
    let sandbox = Sandbox::new("usage");
    assert_eq!(
        error_code(&sandbox.run(&["deploy-everything"], None)),
        "usage"
    );
}

#[test]
fn mutating_without_an_approval_is_refused() {
    let sandbox = Sandbox::new("no-approval");
    let planned = sandbox.run(&["plan-optimize"], None);
    let plan_id = ok(&planned)["plan_id"]
        .as_str()
        .expect("plan id")
        .to_owned();

    let apply = sandbox.run(&["apply", &plan_id], None);
    assert_eq!(error_code(&apply), "approval");
}

#[test]
fn wrong_token_is_refused() {
    let sandbox = Sandbox::new("wrong-token");
    let planned = sandbox.run(&["plan-optimize"], None);
    let plan_id = ok(&planned)["plan_id"]
        .as_str()
        .expect("plan id")
        .to_owned();
    ok(&sandbox.run(&["approve", &plan_id], None));

    let apply = sandbox.run(&["apply", &plan_id, "--token", "apr-forged"], None);
    assert_eq!(error_code(&apply), "approval");
}

#[test]
fn a_confirmed_plan_applies_exactly_once() {
    let sandbox = Sandbox::new("apply-once");
    ok(&sandbox.run(&["reset"], None));
    let planned = sandbox.run(&["plan-optimize"], None);
    let data = ok(&planned);
    let plan_id = data["plan_id"].as_str().expect("plan id").to_owned();
    assert_eq!(data["monthly_delta"], "−¥104.00");

    let token = ok(&sandbox.run(&["approve", &plan_id], None))["token"]
        .as_str()
        .expect("token")
        .to_owned();
    let applied = sandbox.run(&["apply", &plan_id, "--token", &token], None);
    assert_eq!(ok(&applied)["mode"], "mock");

    // The approval is spent and the plan is gone: nothing to replay.
    let again = sandbox.run(&["apply", &plan_id, "--token", &token], None);
    assert!(!error_code(&again).is_empty());

    // The execution shows up in the project's flow history.
    // The execution lands on the bucket its resources belong to.
    let flows = sandbox.run(&["flows"], None);
    let projects: Vec<&str> = ok(&flows)["flows"]
        .as_array()
        .expect("flows")
        .iter()
        .filter_map(|flow| flow["project"].as_str())
        .collect();
    assert!(projects.contains(&"unassigned"), "flows: {projects:?}");

    let shop = sandbox.run(&["flow", "shop-api"], None);
    let runs = ok(&shop)["runs"].as_array().expect("runs");
    assert_eq!(
        runs.len(),
        1,
        "the demo deploy is still the only shop-api run"
    );
}

#[test]
fn a_tampered_plan_cannot_be_approved() {
    let sandbox = Sandbox::new("tamper");
    let planned = sandbox.run(&["plan-optimize"], None);
    let plan_id = ok(&planned)["plan_id"]
        .as_str()
        .expect("plan id")
        .to_owned();

    // Rewrite the stored plan so it claims to save more than it does.
    let path = sandbox.state_path();
    let text = std::fs::read_to_string(&path).expect("state file");
    let mut state: Value = serde_json::from_str(&text).expect("state json");
    state["plans"][0]["monthly_delta_cents"] = Value::from(-9_999_999);
    std::fs::write(&path, serde_json::to_string_pretty(&state).expect("json")).expect("write");

    let approved = sandbox.run(&["approve", &plan_id], None);
    assert_eq!(error_code(&approved), "plan");
}

#[test]
fn deploy_plan_carries_its_project_and_cost() {
    let sandbox = Sandbox::new("deploy");
    let input = r#"{"project":"blog","region":"cn-sh2","spec":"2C / 4GiB","count":1,"repo":"github.com/me/blog"}"#;
    let envelope = sandbox.run(&["plan-deploy"], Some(input));
    let data = ok(&envelope);
    let plan = &data["plan"];
    assert_eq!(plan["project"], "blog");
    // one host + one EIP
    assert_eq!(plan["actions"].as_array().expect("actions").len(), 2);
    assert_eq!(plan["monthly_delta_cents"], 18_640 + 2_400);
    assert_eq!(data["monthly_delta"], "+¥210.40");
}

#[test]
fn repo_plan_reads_manifests_and_never_returns_env_secrets() {
    let sandbox = Sandbox::new("repo-plan");
    let repo = sandbox.home.join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir");
    std::fs::write(
        repo.join("package.json"),
        r#"{"dependencies":{"next":"15","pg":"8","ioredis":"5"}}"#,
    )
    .expect("manifest");
    std::fs::write(
        repo.join(".env"),
        "DATABASE_URL=postgres://user:secret@example/db",
    )
    .expect("secret file");
    let input = serde_json::json!({
        "project": "repo-demo",
        "source": repo.to_str().expect("path"),
        "region": "cn-bj2"
    })
    .to_string();
    let planned = sandbox.run(&["plan-repo-deploy"], Some(&input));
    let data = ok(&planned);
    let actions = data["plan"]["actions"].as_array().expect("actions");
    assert!(actions.iter().any(|action| action["kind"] == "lighthost"));
    assert!(actions.iter().any(|action| action["kind"] == "eip"));
    assert!(actions.iter().any(|action| action["kind"] == "udb"));
    assert!(actions.iter().any(|action| action["kind"] == "redis"));
    assert_eq!(data["repo"]["languages"][0], "Node.js");
    assert!(!planned.to_string().contains("secret"));
    let state = std::fs::read_to_string(sandbox.state_path()).expect("state");
    assert!(!state.contains("secret"));
}
