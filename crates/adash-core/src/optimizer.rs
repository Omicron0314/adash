//! Rules that turn the inventory into a saving plan.
//!
//! Only evidence the API actually gives us is used: an EIP with no binding, a
//! disk with no host, a stopped host still on the bill. CPU-based idle detection
//! needs the monitoring API and is tracked in docs/ROADMAP.md.

use crate::inventory::Inventory;
use crate::plan::{Plan, PlanAction, PlanKind, Verb};
use crate::resource::{Resource, ResourceId, ResourceKind, ResourceState, money};
use serde::{Deserialize, Serialize};

/// How urgent a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Pure waste, safe to remove.
    Waste,
    /// Might be needed; check before removing.
    Review,
}

/// One thing the account is paying for and not using.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Stable id for the UI list.
    pub id: String,
    /// Short title, e.g. 「空闲 EIP 106.75.40.88」.
    pub title: String,
    /// Why we think so.
    pub detail: String,
    /// The resource to act on.
    pub resource: Option<ResourceId>,
    /// Monthly saving in cents.
    pub monthly_savings_cents: i64,
    /// Urgency.
    pub severity: Severity,
    /// What applying the plan will do.
    pub verb: Verb,
}

impl Finding {
    /// `省 ¥24.00 / 月`.
    pub fn savings_text(&self) -> String {
        format!("省 {} / 月", money(self.monthly_savings_cents))
    }
}

/// Analyze an inventory, biggest savings first.
pub fn analyze(inventory: &Inventory) -> Vec<Finding> {
    let mut findings = Vec::new();

    for resource in inventory.of_kind(ResourceKind::Eip) {
        if resource.attached_to.is_empty() {
            findings.push(finding(
                resource,
                Severity::Waste,
                Verb::Release,
                format!(
                    "未绑定任何资源，仍按 {} 计费",
                    money(resource.monthly_cost_cents)
                ),
            ));
        }
    }

    for resource in inventory.of_kind(ResourceKind::Disk) {
        if resource.attached_to.is_empty() {
            findings.push(finding(
                resource,
                Severity::Waste,
                Verb::Release,
                format!("未挂载到任何主机，占用 {}GiB", resource.spec),
            ));
        }
    }

    for resource in inventory.of_kind(ResourceKind::UHost) {
        if resource.state == ResourceState::Stopped {
            findings.push(finding(
                resource,
                Severity::Review,
                Verb::Release,
                "已停机但仍在计费，确认无用后可释放".to_owned(),
            ));
        }
    }

    findings.sort_by_key(|finding| -finding.monthly_savings_cents);
    findings
}

fn finding(resource: &Resource, severity: Severity, verb: Verb, detail: String) -> Finding {
    Finding {
        id: format!("find-{}", resource.id.key()),
        title: format!("{} {}", resource.id.kind.label(), resource.name),
        detail,
        resource: Some(resource.id.clone()),
        monthly_savings_cents: resource.monthly_cost_cents,
        severity,
        verb,
    }
}

/// Turn findings into a confirmable plan.
pub fn plan(findings: &[Finding]) -> Plan {
    let actions: Vec<PlanAction> = findings
        .iter()
        .filter_map(|finding| {
            Some(PlanAction {
                verb: finding.verb,
                kind: finding.resource.as_ref()?.kind,
                spec: String::new(),
                region: String::new(),
                resource: finding.resource.clone(),
                monthly_cents: -finding.monthly_savings_cents,
            })
        })
        .collect();
    Plan::new(
        PlanKind::Optimize,
        format!("释放 {} 个闲置资源", actions.len()),
        actions,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ucloud_api::UcloudClient;

    #[test]
    fn mock_account_has_two_waste_findings() {
        let inventory = Inventory::load(&UcloudClient::mock());
        let findings = analyze(&inventory);
        assert_eq!(findings.len(), 2);
        let total: i64 = findings
            .iter()
            .map(|finding| finding.monthly_savings_cents)
            .sum();
        assert_eq!(total, 2400 + 8000);
    }

    #[test]
    fn findings_become_a_saving_plan() {
        let inventory = Inventory::load(&UcloudClient::mock());
        let findings = analyze(&inventory);
        let plan = plan(&findings);
        assert_eq!(plan.actions.len(), 2);
        assert_eq!(plan.monthly_delta_cents, -(2400 + 8000));
        assert!(!plan.reversible);
        assert_eq!(plan.kind, PlanKind::Optimize);
    }
}
