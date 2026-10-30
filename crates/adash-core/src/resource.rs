//! One vocabulary for every cloud object, whatever product it came from.

use serde::{Deserialize, Serialize};

/// Stable identifier of a resource inside one account.
///
/// UCloud ids are strings and are unique per product; the kind is kept next to
/// it so `uhost-xxx` and an EIP that happen to share an id never collide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    /// Product family, e.g. `uhost`.
    pub kind: String,
    /// UCloud resource id, e.g. `uhost-abc123`.
    pub id: String,
}

/// Product family of a resource. Adding a variant means adding it to
/// `inventory` (crates/adash-core), `adash-tool` and the GUI filter chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    /// Cloud host (UHost).
    UHost,
    /// Lightweight application host.
    ULightHost,
    /// Elastic IP.
    Eip,
    /// Block storage / cloud disk.
    Disk,
    /// Managed MySQL.
    Udb,
    /// Managed Redis.
    Redis,
    /// US3 object storage bucket.
    Bucket,
    /// Load balancer.
    Ulb,
}

/// Lifecycle state as the console shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    /// Running / available.
    Running,
    /// Stopped but still billed (disks, IPs).
    Stopped,
    /// Being created, started or stopped.
    Pending,
    /// Failed and needs attention.
    Error,
    /// Released or terminated; kept only for history.
    Released,
}

/// A single cloud object with the fields the dashboard and agent both need.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Product + id.
    pub id: ResourceId,
    /// Human name (UCloud `Name`), may be empty.
    pub name: String,
    /// Region id, e.g. `cn-bj2`.
    pub region: String,
    /// Availability zone, when the product has one.
    pub zone: Option<String>,
    /// Lifecycle state.
    pub state: ResourceState,
    /// Project this resource belongs to, when tagged `adash:<slug>`.
    pub project: Option<String>,
    /// Monthly cost in cents, when known (billing estimate).
    pub monthly_cost_cents: Option<i64>,
    /// Creation time, RFC 3339.
    pub created_at: Option<String>,
}
