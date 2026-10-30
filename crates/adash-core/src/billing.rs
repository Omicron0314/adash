//! Bills and balance, joined with the inventory so spend has a project name.

use crate::inventory::Inventory;
use crate::resource::ResourceId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ucloud_api::UcloudClient;

/// This month's spend, by project and by product.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Billing {
    /// Spendable balance in yuan.
    pub balance_yuan: Option<f64>,
    /// Spend in cents for the entries the API returned.
    pub month_cents: i64,
    /// One entry per project slug (`未分组` for untagged spend).
    pub by_project: Vec<(String, i64)>,
    /// One entry per product name.
    pub by_product: Vec<(String, i64)>,
    /// Why the numbers may be incomplete.
    pub errors: Vec<String>,
}

impl Billing {
    /// Read the balance and the current bill, attributing each entry to the
    /// project of the resource it charges.
    pub fn load(client: &UcloudClient, inventory: &Inventory) -> Self {
        let mut billing = Self {
            balance_yuan: inventory.balance_yuan,
            ..Self::default()
        };

        let bills = match client.bills() {
            Ok(list) => list.bills,
            Err(error) => {
                billing.errors.push(format!("账单：{error}"));
                return billing;
            }
        };

        let mut projects: BTreeMap<String, i64> = BTreeMap::new();
        let mut products: BTreeMap<String, i64> = BTreeMap::new();
        for bill in bills {
            let cents = (bill.amount * 100.0).round() as i64;
            billing.month_cents += cents;

            let kind = inventory
                .resources
                .iter()
                .find(|resource| resource.id.id == bill.resource_id)
                .map(|resource| resource.id.kind);
            let resource_id = kind.map(|kind| ResourceId::new(kind, &bill.resource_id));
            let project = resource_id
                .as_ref()
                .and_then(|id| inventory.get(id))
                .and_then(|resource| resource.project.clone())
                .unwrap_or_else(|| "未分组".to_owned());

            *projects.entry(project).or_default() += cents;
            *products.entry(bill.product).or_default() += cents;
        }

        let mut by_project: Vec<(String, i64)> = projects.into_iter().collect();
        by_project.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut by_product: Vec<(String, i64)> = products.into_iter().collect();
        by_product.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        billing.by_project = by_project;
        billing.by_product = by_product;
        billing
    }

    /// Spend of one project, in cents.
    pub fn project_cents(&self, slug: &str) -> i64 {
        self.by_project
            .iter()
            .find(|(project, _)| project == slug)
            .map_or(0, |(_, cents)| *cents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_bills_are_attributed_to_projects() {
        let client = UcloudClient::mock();
        let inventory = Inventory::load(&client);
        let billing = Billing::load(&client, &inventory);

        assert!(billing.errors.is_empty(), "{:?}", billing.errors);
        assert_eq!(billing.month_cents, 98_350);
        assert_eq!(
            billing.project_cents("shop-api"),
            18_640 * 2 + 9_800 + 7_650 + 2_400 * 2
        );
        // The orphan EIP has no project tag but is still charged.
        assert_eq!(billing.project_cents("未分组"), 4_800);
    }
}
