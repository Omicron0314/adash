//! Plans: the only object that can change the cloud.
//!
//! A plan is what the user sees on the confirmation card, and what
//! `adash-tool apply` re-validates before touching anything: sealed (hash),
//! unexpired, and backed by a one-time approval the GUI wrote.

use crate::price;
use crate::resource::{ResourceId, ResourceKind, money};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// What a plan is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// Create the resources a project needs.
    Deploy,
    /// Release or downsize waste.
    Optimize,
    /// Stop (not delete) named resources.
    Stop,
    /// Start named resources.
    Start,
}

impl PlanKind {
    /// Chinese title for the plan card.
    pub fn label(self) -> &'static str {
        match self {
            Self::Deploy => "部署计划",
            Self::Optimize => "省钱计划",
            Self::Stop => "停机计划",
            Self::Start => "启动计划",
        }
    }
}

/// One action inside a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    /// Create a new resource.
    Create,
    /// Power off; reversible and still billed.
    Stop,
    /// Power on.
    Start,
    /// Delete; irreversible.
    Release,
    /// Change the规格 of an existing resource.
    Resize,
}

impl Verb {
    /// Whether undo is possible.
    pub fn reversible(self) -> bool {
        !matches!(self, Self::Release)
    }

    /// Chinese verb for the action row.
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "创建",
            Self::Stop => "停机",
            Self::Start => "启动",
            Self::Release => "释放",
            Self::Resize => "变更规格",
        }
    }
}

/// One line of the plan card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanAction {
    /// What happens.
    pub verb: Verb,
    /// Product.
    pub kind: ResourceKind,
    /// Spec for creates/resizes; empty otherwise.
    pub spec: String,
    /// Region.
    pub region: String,
    /// Existing resource for stop/start/release/resize.
    pub resource: Option<ResourceId>,
    /// Monthly cost this action adds (positive) or saves (negative), in cents.
    pub monthly_cents: i64,
}

impl PlanAction {
    /// `创建 云主机 2C / 4GiB @ cn-bj2`.
    pub fn summary(&self) -> String {
        let verb = self.verb.label();
        let kind = self.kind.label();
        match (&self.resource, self.spec.is_empty()) {
            (Some(resource), _) => format!("{verb} {kind} {} @ {}", resource.id, self.region),
            (None, false) => format!("{verb} {kind} {} @ {}", self.spec, self.region),
            (None, true) => format!("{verb} {kind} @ {}", self.region),
        }
    }
}

/// A confirmable change set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Stable id, e.g. `plan-<uuid>`.
    pub id: String,
    /// When it was created, unix seconds.
    pub created_at: i64,
    /// When it stops being applicable.
    pub expires_at: i64,
    /// What kind of plan this is.
    pub kind: PlanKind,
    /// One sentence the user reads first.
    pub summary: String,
    /// Project the plan belongs to, when it targets one.
    #[serde(default)]
    pub project: Option<String>,
    /// The actions, in execution order.
    pub actions: Vec<PlanAction>,
    /// Net monthly change in cents (negative saves money).
    pub monthly_delta_cents: i64,
    /// `false` for anything that deletes data.
    pub reversible: bool,
    /// Things the user must know before confirming.
    pub risks: Vec<String>,
    /// Seal over everything above; empty until [`Plan::seal`].
    pub hash: String,
}

/// Default time-to-live: 15 minutes.
pub const TTL_SECS: i64 = 15 * 60;

impl Plan {
    /// Build a plan from its actions, deriving costs and risks.
    pub fn new(kind: PlanKind, summary: impl Into<String>, actions: Vec<PlanAction>) -> Self {
        let monthly_delta_cents = actions.iter().map(|action| action.monthly_cents).sum();
        let reversible = actions.iter().all(|action| action.verb.reversible());
        let risks = risks_for(&actions);
        let now = crate::state::now_secs();
        Self {
            id: format!("plan-{}", uuid::Uuid::new_v4()),
            created_at: now,
            expires_at: now + TTL_SECS,
            kind,
            summary: summary.into(),
            project: None,
            actions,
            monthly_delta_cents,
            reversible,
            risks,
            hash: String::new(),
        }
    }

    /// Attribute the plan to a project (used for the flow history entry).
    pub fn with_project(mut self, project: impl Into<String>) -> Self {
        self.project = Some(project.into());
        self
    }

    /// Recompute and store the seal.
    pub fn seal(&mut self) {
        self.hash = self.compute_hash();
    }

    /// Whether the seal matches the current contents.
    pub fn verify(&self) -> bool {
        !self.hash.is_empty() && self.hash == self.compute_hash()
    }

    /// Whether this plan can still be applied.
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    /// Monthly cost change, as a signed string (`+¥186.40` / `−¥48.00`).
    pub fn monthly_delta_text(&self) -> String {
        signed_money(self.monthly_delta_cents)
    }

    fn compute_hash(&self) -> String {
        // Hash the meaning, not the serialization quirks: rebuild a canonical
        // JSON tuple of every field except the seal itself.
        let canonical = serde_json::json!({
            "id": self.id,
            "created_at": self.created_at,
            "expires_at": self.expires_at,
            "kind": self.kind,
            "summary": self.summary,
            "project": self.project,
            "actions": self.actions,
            "monthly_delta_cents": self.monthly_delta_cents,
            "reversible": self.reversible,
            "risks": self.risks,
        });
        let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
        let digest = Sha256::digest(&bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// `−¥48.00` when saving, `+¥186.40` when spending.
pub fn signed_money(cents: i64) -> String {
    if cents == 0 {
        return "¥0.00".to_owned();
    }
    if cents < 0 {
        format!("−{}", money(-cents))
    } else {
        format!("+{}", money(cents))
    }
}

fn risks_for(actions: &[PlanAction]) -> Vec<String> {
    let mut risks = Vec::new();
    if actions.iter().any(|action| action.verb == Verb::Release) {
        risks.push("将删除资源，数据不可恢复".to_owned());
    }
    if actions.iter().any(|action| action.verb == Verb::Stop) {
        risks.push("停机后仍会计费（公网 IP 与云盘保留）".to_owned());
    }
    if actions
        .iter()
        .any(|action| action.verb == Verb::Create && action.kind == ResourceKind::UHost)
    {
        risks.push("创建主机需要镜像与 SSH 初始密码".to_owned());
    }
    risks
}

/// Cost of creating `count` resources, for the agent's plan building.
pub fn create_action(kind: ResourceKind, spec: &str, region: &str, count: i64) -> Vec<PlanAction> {
    (0..count.max(1))
        .map(|_| PlanAction {
            verb: Verb::Create,
            kind,
            spec: spec.to_owned(),
            region: region.to_owned(),
            resource: None,
            monthly_cents: price::monthly_cents(kind, spec),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealing_detects_tampering() {
        let mut plan = Plan::new(
            PlanKind::Deploy,
            "部署 shop-api",
            create_action(ResourceKind::UHost, "2C / 4GiB", "cn-bj2", 2),
        );
        assert!(!plan.verify());
        plan.seal();
        assert!(plan.verify());
        plan.actions[0].spec = "8C / 16GiB".into();
        assert!(!plan.verify());
    }

    #[test]
    fn release_makes_a_plan_irreversible() {
        let actions = vec![PlanAction {
            verb: Verb::Release,
            kind: ResourceKind::Eip,
            spec: String::new(),
            region: "cn-bj2".into(),
            resource: Some(ResourceId::new(ResourceKind::Eip, "eip-orphan1")),
            monthly_cents: -2400,
        }];
        let plan = Plan::new(PlanKind::Optimize, "释放空闲 EIP", actions);
        assert!(!plan.reversible);
        assert_eq!(plan.monthly_delta_cents, -2400);
        assert_eq!(plan.monthly_delta_text(), "−¥24.00");
        assert!(plan.risks.iter().any(|risk| risk.contains("不可恢复")));
    }
}
