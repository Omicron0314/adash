//! One vocabulary for every cloud object, whatever product it came from.
//!
//! The GUI, the agent and the flow graph all read these types; nothing else
//! parses UCloud JSON (see docs/ARCHITECTURE.md).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Product family of a resource.
///
/// Adding a variant means adding it to `inventory`, `adash-tool`, the GUI
/// filter chips and [`ResourceKind::label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    /// Cloud host (UHost).
    #[serde(rename = "uhost")]
    UHost,
    /// Lightweight application host.
    #[serde(rename = "lighthost")]
    LightHost,
    /// Elastic IP.
    #[serde(rename = "eip")]
    Eip,
    /// Block storage / cloud disk.
    #[serde(rename = "disk")]
    Disk,
    /// Managed MySQL (UDB).
    #[serde(rename = "udb")]
    Udb,
    /// Managed Redis (UMem).
    #[serde(rename = "redis")]
    Redis,
    /// Load balancer (ULB).
    #[serde(rename = "ulb")]
    Ulb,
    /// US3 object storage bucket.
    #[serde(rename = "bucket")]
    Bucket,
}

impl ResourceKind {
    /// Every kind, in filter-chip order.
    pub const ALL: [Self; 8] = [
        Self::UHost,
        Self::LightHost,
        Self::Eip,
        Self::Disk,
        Self::Udb,
        Self::Redis,
        Self::Ulb,
        Self::Bucket,
    ];

    /// Stable id used in ids, JSON and URLs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UHost => "uhost",
            Self::LightHost => "lighthost",
            Self::Eip => "eip",
            Self::Disk => "disk",
            Self::Udb => "udb",
            Self::Redis => "redis",
            Self::Ulb => "ulb",
            Self::Bucket => "bucket",
        }
    }

    /// Chinese label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::UHost => "云主机",
            Self::LightHost => "轻量主机",
            Self::Eip => "弹性 IP",
            Self::Disk => "云硬盘",
            Self::Udb => "云数据库",
            Self::Redis => "Redis",
            Self::Ulb => "负载均衡",
            Self::Bucket => "对象存储",
        }
    }
}

/// Stable identifier of a resource inside one account.
///
/// UCloud ids are strings and unique per product; the kind is kept next to the
/// id so an EIP that happens to share an id with a host never collides.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    /// Product family.
    pub kind: ResourceKind,
    /// UCloud resource id, e.g. `uhost-abc123`.
    pub id: String,
}

impl ResourceId {
    /// Build an id from its parts.
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }

    /// `uhost:uhost-abc123`, for graph ids and logs.
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }
}

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key())
    }
}

/// Lifecycle state as the console shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    /// Running, or in use (a bound EIP, an attached disk).
    Running,
    /// Powered off but still billed.
    Stopped,
    /// Allocated and billable, but not attached to anything (free EIP,
    /// unattached disk). This is what the optimizer looks for.
    Available,
    /// Being created, started or stopped.
    Pending,
    /// Failed and needs attention.
    Error,
    /// Released or terminated; kept only for history.
    Released,
}

impl ResourceState {
    /// Map a UCloud state string (case-insensitive) onto our vocabulary.
    pub fn from_ucloud(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "running" | "used" | "normal" | "inuse" => Self::Running,
            "available" | "free" | "unbind" | "unattached" => Self::Available,
            "stopped" | "shutdown" => Self::Stopped,
            "" => Self::Pending,
            value if value.contains("fail") => Self::Error,
            "terminated" | "deleted" | "released" => Self::Released,
            _ => Self::Pending,
        }
    }

    /// Short Chinese label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "运行中",
            Self::Stopped => "已停止",
            Self::Available => "未使用",
            Self::Pending => "处理中",
            Self::Error => "异常",
            Self::Released => "已释放",
        }
    }
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
    /// Project this resource belongs to, from the `adash:<slug>` tag.
    pub project: Option<String>,
    /// One-line规格, e.g. `2C / 4GiB`, `100GiB`, `5Mbps`.
    pub spec: String,
    /// Monthly cost estimate in cents, from [`crate::price`].
    pub monthly_cost_cents: i64,
    /// Public address, when the product has one.
    pub public_ip: Option<String>,
    /// Private address, when the product has one.
    pub private_ip: Option<String>,
    /// Resources this one is attached to (EIP → host, disk → host, host → ULB).
    pub attached_to: Vec<ResourceId>,
    /// Free-form details shown in the side sheet, in insertion order.
    pub attributes: BTreeMap<String, String>,
    /// Creation time, unix seconds.
    pub created_at: Option<i64>,
}

impl Resource {
    /// The `project` display name, or 「未分组」.
    pub fn project_or_unassigned(&self) -> &str {
        self.project.as_deref().unwrap_or("未分组")
    }

    /// Whether the monthly cost is a real bill or a demo estimate.
    pub fn monthly_yuan(&self) -> f64 {
        self.monthly_cost_cents as f64 / 100.0
    }
}

/// Format cents as `¥12.40`.
pub fn money(cents: i64) -> String {
    format!("¥{:.2}", cents as f64 / 100.0)
}
