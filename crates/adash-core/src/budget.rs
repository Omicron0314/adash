//! Budgets: how much of the month's allowance a project has used.

use serde::{Deserialize, Serialize};

/// Where a project stands against its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    /// No budget was set.
    Unset,
    /// Below 80%.
    Healthy,
    /// 80–100%.
    Near,
    /// Over budget.
    Over,
}

/// A project's budget and spend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Budget {
    /// Monthly allowance in cents; `0` means unset.
    pub limit_cents: i64,
    /// Spent so far this month, in cents.
    pub spent_cents: i64,
    /// The resources' monthly estimate, in cents.
    pub estimate_cents: i64,
}

impl Budget {
    /// Build from a limit, spend and estimate.
    pub fn new(limit_cents: i64, spent_cents: i64, estimate_cents: i64) -> Self {
        Self {
            limit_cents,
            spent_cents,
            estimate_cents,
        }
    }

    /// Used fraction of the limit, clamped to `0.0..=1.5`.
    pub fn ratio(&self) -> f32 {
        if self.limit_cents <= 0 {
            return 0.0;
        }
        (self.spent_cents as f32 / self.limit_cents as f32).clamp(0.0, 1.5)
    }

    /// Status band for the progress card.
    pub fn status(&self) -> BudgetStatus {
        if self.limit_cents <= 0 {
            return BudgetStatus::Unset;
        }
        match self.ratio() {
            ratio if ratio >= 1.0 => BudgetStatus::Over,
            ratio if ratio >= 0.8 => BudgetStatus::Near,
            _ => BudgetStatus::Healthy,
        }
    }

    /// Remaining allowance in cents (negative when over).
    pub fn remaining_cents(&self) -> i64 {
        self.limit_cents - self.spent_cents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_the_bands() {
        assert_eq!(Budget::new(0, 0, 0).status(), BudgetStatus::Unset);
        assert_eq!(
            Budget::new(10_000, 1_000, 0).status(),
            BudgetStatus::Healthy
        );
        assert_eq!(Budget::new(10_000, 8_500, 0).status(), BudgetStatus::Near);
        assert_eq!(Budget::new(10_000, 12_000, 0).status(), BudgetStatus::Over);
    }

    #[test]
    fn ratio_is_clamped() {
        assert!((Budget::new(10_000, 25_000, 0).ratio() - 1.5).abs() < f32::EPSILON);
    }
}
