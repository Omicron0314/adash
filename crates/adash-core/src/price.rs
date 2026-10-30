//! Monthly cost estimates.
//!
//! The demo estimates from a static table instead of calling
//! `GetUHostInstancePrice` on every render: one network round trip per resource
//! is slow and, in mock mode, meaningless. `fixtures/mock` costs and this table
//! agree; replacing it with live pricing is tracked in docs/ROADMAP.md (M1
//! follow-up) and touches nothing else.

use crate::resource::ResourceKind;

/// A point in the demo price table.
struct Row {
    /// Prefix of the spec string this row prices.
    spec_contains: &'static str,
    /// Monthly cost in cents.
    cents: i64,
}

/// Yuan→cents helper for the table below.
const fn yuan(value: i64) -> i64 {
    value * 100
}

/// `uhost` rows: per-vCPU linear pricing (¥93.20 per vCPU-month).
const UHOST_PER_CPU: i64 = 9320;
/// What the estimate assumes per vCPU: 2 GiB.
const GIB_PER_CPU: i64 = 2;

/// Static estimates, by product. Ordered most specific first.
const ROWS: &[(ResourceKind, &[Row])] = &[
    (
        ResourceKind::Eip,
        &[Row {
            spec_contains: "Mbps",
            cents: yuan(24),
        }],
    ),
    (
        ResourceKind::Disk,
        &[Row {
            spec_contains: "GiB",
            cents: 40, // 0.4 yuan per GiB-month, expressed per GiB below
        }],
    ),
    (
        ResourceKind::Udb,
        &[Row {
            spec_contains: "",
            cents: yuan(98),
        }],
    ),
    (
        ResourceKind::Redis,
        &[Row {
            spec_contains: "",
            cents: yuan(77),
        }],
    ),
    (
        ResourceKind::Ulb,
        &[Row {
            spec_contains: "",
            cents: yuan(18),
        }],
    ),
    (
        ResourceKind::LightHost,
        &[Row {
            spec_contains: "",
            cents: yuan(30),
        }],
    ),
    (
        ResourceKind::Bucket,
        &[Row {
            spec_contains: "",
            cents: yuan(5),
        }],
    ),
];

/// Estimate a resource's monthly cost in cents.
///
/// `spec` is the same one-line string the GUI shows (`2C / 4GiB`,
/// `100GiB`, `5Mbps`). Unparseable specs fall back to a per-product default so
/// a new product never renders as free.
pub fn monthly_cents(kind: ResourceKind, spec: &str) -> i64 {
    match kind {
        ResourceKind::UHost => {
            let cpus = leading_number(spec).unwrap_or(2).max(1);
            cpus * UHOST_PER_CPU
        }
        ResourceKind::Disk => {
            let gib = leading_number(spec)
                .or_else(|| between(spec, "GiB", ""))
                .unwrap_or(100)
                .max(1);
            gib * 40
        }
        _ => ROWS
            .iter()
            .find(|(row_kind, _)| *row_kind == kind)
            .and_then(|(_, rows)| {
                rows.iter()
                    .find(|row| row.spec_contains.is_empty() || spec.contains(row.spec_contains))
            })
            .map_or(yuan(10), |row| row.cents),
    }
}

/// The first integer in `text`.
fn leading_number(text: &str) -> Option<i64> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn between(text: &str, start: &str, end: &str) -> Option<i64> {
    let after = text.split(start).nth(1)?;
    let value = match end.is_empty() {
        true => after,
        false => after.split(end).next()?,
    };
    leading_number(value)
}

/// Memory a host spec implies, used when the API does not report it.
pub fn memory_gib(cpus: i64) -> i64 {
    (cpus * GIB_PER_CPU).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_hosts_by_vcpu() {
        assert_eq!(monthly_cents(ResourceKind::UHost, "2C / 4GiB"), 18640);
        assert_eq!(monthly_cents(ResourceKind::UHost, "8C / 16GiB"), 74560);
    }

    #[test]
    fn prices_disks_by_size() {
        assert_eq!(monthly_cents(ResourceKind::Disk, "100GiB"), 4000);
        assert_eq!(monthly_cents(ResourceKind::Disk, "200GiB"), 8000);
    }

    #[test]
    fn unknown_spec_still_has_a_price() {
        assert!(monthly_cents(ResourceKind::Bucket, "") > 0);
        assert!(monthly_cents(ResourceKind::UHost, "") > 0);
    }
}
