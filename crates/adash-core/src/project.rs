//! Projects: a slug, the resources tagged with it, and its budget.

use crate::budget::Budget;
use crate::inventory::Inventory;
use crate::resource::{Resource, ResourceId};
use crate::state::ProjectRecord;
use serde::{Deserialize, Serialize};

/// A project as the GUI and the agent see it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectView {
    /// Tag slug.
    pub slug: String,
    /// Display name (falls back to the slug).
    pub name: String,
    /// Source repository, when recorded.
    pub repo: Option<String>,
    /// Free-form note.
    pub note: String,
    /// Budget and spend.
    pub budget: Budget,
    /// Members, in inventory order.
    pub resources: Vec<ResourceId>,
}

impl ProjectView {
    /// Whether this project has anything running.
    pub fn running(&self, inventory: &Inventory) -> usize {
        self.resources
            .iter()
            .filter_map(|id| inventory.get(id))
            .filter(|resource| resource.state == crate::resource::ResourceState::Running)
            .count()
    }
}

/// Every project in the account, richest first, plus any recorded-but-empty one.
///
/// Spend comes from the bill; the estimate comes from [`crate::price`]. Both are
/// shown because they answer different questions ("what did I pay" vs "what am
/// I signed up for").
pub fn project_views(
    inventory: &Inventory,
    billing: &crate::billing::Billing,
    records: impl Iterator<Item = ProjectRecord>,
) -> Vec<ProjectView> {
    let records: Vec<ProjectRecord> = records.collect();
    let mut slugs = inventory.projects();
    for record in &records {
        if !slugs.contains(&record.slug) {
            slugs.push(record.slug.clone());
        }
    }
    slugs.sort();
    slugs.dedup();

    slugs
        .into_iter()
        .map(|slug| {
            let resources: Vec<&Resource> = inventory.of_project(Some(&slug));
            let estimate_cents: i64 = resources
                .iter()
                .map(|resource| resource.monthly_cost_cents)
                .sum();
            let record = records.iter().find(|record| record.slug == slug);
            ProjectView {
                budget: Budget::new(
                    record.map_or(0, |record| record.budget_cents),
                    billing.project_cents(&slug),
                    estimate_cents,
                ),
                name: record.map_or_else(|| slug.clone(), |record| record.name.clone()),
                repo: record.and_then(|record| record.repo.clone()),
                note: record.map_or_else(String::new, |record| record.note.clone()),
                resources: resources
                    .iter()
                    .map(|resource| resource.id.clone())
                    .collect(),
                slug,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::Billing;
    use ucloud_api::UcloudClient;

    #[test]
    fn projects_are_listed_with_resources_and_spend() {
        let client = UcloudClient::mock();
        let inventory = Inventory::load(&client);
        let billing = Billing::load(&client, &inventory);
        let views = project_views(&inventory, &billing, std::iter::empty());

        let shop = views
            .iter()
            .find(|view| view.slug == "shop-api")
            .expect("shop-api");
        assert_eq!(shop.resources.len(), 8);
        assert_eq!(shop.running(&inventory), 8);
        assert!(shop.budget.spent_cents > 0);
        assert!(shop.budget.estimate_cents > 0);
    }
}
