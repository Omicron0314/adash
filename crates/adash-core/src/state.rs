//! Local persistence: projects, plans, approvals and flow runs.
//!
//! One JSON file in the app data directory, `0600`. Plans and one-time
//! approvals live here, which is what makes `adash-tool apply` safe: the agent
//! cannot mint an approval, it can only spend the one the GUI wrote.

use crate::flow::FlowRun;
use crate::plan::Plan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Unix seconds.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

/// What we know about a project beyond its resources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectRecord {
    /// Slug used in the `adash:<slug>` tag.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// Source repository, e.g. `github.com/me/shop-api`.
    pub repo: Option<String>,
    /// Free-form note shown in the flow header.
    pub note: String,
    /// Monthly budget in cents; `0` means "no budget set".
    pub budget_cents: i64,
}

/// A one-time approval written by the GUI after the user confirms a plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    /// Opaque token; the agent passes it back to `adash-tool apply`.
    pub token: String,
    /// When the user clicked confirm.
    pub issued_at: i64,
    /// Approvals expire even if the plan does not.
    pub expires_at: i64,
}

/// Everything the app remembers between launches.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoreData {
    /// Plans, newest last.
    pub plans: Vec<Plan>,
    /// plan id → approval, single use.
    pub approvals: BTreeMap<String, Approval>,
    /// slug → project record.
    pub projects: BTreeMap<String, ProjectRecord>,
    /// Flow executions, newest last.
    pub runs: Vec<FlowRun>,
}

/// A loaded [`StoreData`] plus where it came from.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    /// In-memory copy; every mutation calls [`Store::save`].
    pub data: StoreData,
}

impl Store {
    /// `<data dir>/adash/state.json`.
    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("adash/state.json")
    }

    /// Open the default store, or an empty one when the file is missing.
    pub fn open_default() -> Self {
        Self::open(Self::default_path())
    }

    /// Open a specific file.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, data }
    }

    /// Where this store is written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Persist to disk, creating the directory and tightening permissions.
    ///
    /// Failures are reported, not panicked on: losing local state must never
    /// take the window down.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            return Err(format!("{}: {error}", parent.display()));
        }
        let text = serde_json::to_string_pretty(&self.data).map_err(|error| error.to_string())?;
        std::fs::write(&self.path, text)
            .map_err(|error| format!("{}: {error}", self.path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Store a sealed plan and return its id.
    pub fn insert_plan(&mut self, mut plan: Plan) -> String {
        plan.seal();
        let id = plan.id.clone();
        self.data.plans.retain(|existing| existing.id != id);
        self.data.plans.push(plan);
        id
    }

    /// Look a plan up.
    pub fn plan(&self, id: &str) -> Option<&Plan> {
        self.data.plans.iter().find(|plan| plan.id == id)
    }

    /// Plan, but only if it is still valid: sealed, unmodified, unexpired.
    pub fn plan_for_apply(&self, id: &str) -> Result<&Plan, String> {
        let plan = self.plan(id).ok_or_else(|| format!("计划 {id} 不存在"))?;
        if !plan.verify() {
            return Err("计划内容与签名不一致，已拒绝执行".to_owned());
        }
        if plan.is_expired(now_secs()) {
            return Err("计划已过期，请重新生成".to_owned());
        }
        Ok(plan)
    }

    /// Write the one-time approval the GUI got from the user.
    pub fn approve(&mut self, plan_id: &str, ttl_secs: i64) -> Option<String> {
        self.plan(plan_id)?;
        let now = now_secs();
        let token = format!("apr-{}", uuid::Uuid::new_v4());
        self.data.approvals.insert(
            plan_id.to_owned(),
            Approval {
                token: token.clone(),
                issued_at: now,
                expires_at: now + ttl_secs,
            },
        );
        Some(token)
    }

    /// Consume an approval; `true` when it existed and had not expired.
    pub fn consume_approval(&mut self, plan_id: &str, token: &str) -> bool {
        let Some(approval) = self.data.approvals.remove(plan_id) else {
            return false;
        };
        approval.token == token && approval.expires_at >= now_secs()
    }

    /// Consume an approval without checking its token.
    ///
    /// Used by `apply` when the app answered the extension's confirm directly:
    /// the record only exists because the user clicked confirm in the card.
    pub fn consume_any_approval(&mut self, plan_id: &str) -> bool {
        let Some(approval) = self.data.approvals.remove(plan_id) else {
            return false;
        };
        approval.expires_at >= now_secs()
    }

    /// Remove and return a plan (used after it is applied).
    pub fn take_plan(&mut self, id: &str) -> Option<Plan> {
        let index = self.data.plans.iter().position(|plan| plan.id == id)?;
        Some(self.data.plans.remove(index))
    }

    /// The project record, if any.
    pub fn project(&self, slug: &str) -> Option<&ProjectRecord> {
        self.data.projects.get(slug)
    }

    /// Insert or replace a project record.
    pub fn set_project(&mut self, record: ProjectRecord) {
        self.data.projects.insert(record.slug.clone(), record);
    }

    /// Append a flow run, keeping the newest 50 per project.
    pub fn push_run(&mut self, run: FlowRun) {
        let project = run.project.clone();
        self.data.runs.push(run);
        let mut seen = 0;
        let mut keep = Vec::new();
        for run in self.data.runs.drain(..).rev() {
            if run.project == project {
                seen += 1;
                if seen > 50 {
                    continue;
                }
            }
            keep.push(run);
        }
        keep.reverse();
        self.data.runs = keep;
    }

    /// Runs of one project, oldest first.
    pub fn runs_of(&self, project: &str) -> Vec<FlowRun> {
        self.data
            .runs
            .iter()
            .filter(|run| run.project == project)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{PlanAction, PlanKind, Verb};
    use crate::resource::ResourceKind;

    fn plan(id: &str) -> Plan {
        Plan {
            id: id.to_owned(),
            created_at: now_secs(),
            expires_at: now_secs() + 900,
            kind: PlanKind::Deploy,
            summary: "deck".into(),
            project: None,
            actions: vec![PlanAction {
                verb: Verb::Create,
                kind: ResourceKind::UHost,
                spec: "2C / 4GiB".into(),
                region: "cn-bj2".into(),
                resource: None,
                monthly_cents: 18640,
            }],
            monthly_delta_cents: 18640,
            reversible: true,
            risks: Vec::new(),
            hash: String::new(),
        }
    }

    fn temp_store(name: &str) -> Store {
        let path =
            std::env::temp_dir().join(format!("adash-test-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Store::open(path)
    }

    #[test]
    fn plans_are_sealed_and_verify() {
        let mut store = temp_store("seal");
        let id = store.insert_plan(plan("p1"));
        assert!(store.plan(&id).expect("plan").verify());

        // Tampering breaks the seal.
        store.data.plans[0].monthly_delta_cents = 1;
        assert!(store.plan_for_apply(&id).is_err());
    }

    #[test]
    fn approvals_are_single_use() {
        let mut store = temp_store("approve");
        let id = store.insert_plan(plan("p2"));
        let token = store.approve(&id, 900).expect("approval");
        assert!(store.consume_approval(&id, &token));
        assert!(!store.consume_approval(&id, &token));
    }

    #[test]
    fn expired_approval_is_rejected() {
        let mut store = temp_store("expired");
        let id = store.insert_plan(plan("p3"));
        let token = store.approve(&id, -1).expect("approval");
        assert!(!store.consume_approval(&id, &token));
    }

    #[test]
    fn state_round_trips_through_disk() {
        let mut store = temp_store("roundtrip");
        store.set_project(ProjectRecord {
            slug: "shop-api".into(),
            name: "Shop API".into(),
            repo: Some("github.com/me/shop-api".into()),
            note: String::new(),
            budget_cents: 90_000,
        });
        store.save().expect("save");

        let reopened = Store::open(store.path().to_path_buf());
        assert_eq!(
            reopened.project("shop-api").expect("project").budget_cents,
            90_000
        );
        let _ = std::fs::remove_file(store.path());
    }
}
