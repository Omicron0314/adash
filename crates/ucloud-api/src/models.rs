//! Typed responses for the actions in [`crate::paths::Action`].
//!
//! UCloud field names are kept verbatim via `rename`, and every optional field
//! is `#[serde(default)]` so a new server-side field never breaks the app.
//! Shapes follow the documented responses; the mock fixtures are the executable
//! statement of what we expect (see docs/API.md).

use serde::Deserialize;

/// `GetRegion` → <https://docs.ucloud.cn/api/summary/regionlist>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RegionList {
    /// All regions visible to the account.
    #[serde(rename = "Regions", default)]
    pub regions: Vec<Region>,
}

/// One region.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Region {
    /// Region id, e.g. `cn-bj2`.
    #[serde(rename = "RegionId", default)]
    pub id: String,
    /// Human name, e.g. `北京二`.
    #[serde(rename = "Region", default)]
    pub name: String,
}

/// `GetBalance` → <https://docs.ucloud.cn/api/account-api/get_balance>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Balance {
    /// Amounts, in yuan.
    #[serde(rename = "AccountInfo", default)]
    pub account: AccountInfo,
}

/// Account amounts; `Amount` is the spendable balance.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AccountInfo {
    /// Available balance in yuan.
    #[serde(rename = "Amount", default)]
    pub amount: f64,
    /// Free-trial amount in yuan.
    #[serde(rename = "AmountFree", default)]
    pub amount_free: f64,
    /// Coupon amount in yuan.
    #[serde(rename = "AmountCoupon", default)]
    pub amount_coupon: f64,
}

/// `DescribeUHostInstance` → <https://docs.ucloud.cn/api/uhost-api/describe_uhost_instance>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UHostList {
    /// Hosts.
    #[serde(rename = "UHostSet", default)]
    pub hosts: Vec<UHost>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// A cloud host.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UHost {
    /// `uhost-xxxx`.
    #[serde(rename = "UHostId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// `Running` / `Stopped` / `Pending` / `Install Fail` …
    #[serde(rename = "State", default)]
    pub state: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Availability zone.
    #[serde(rename = "Zone", default)]
    pub zone: String,
    /// vCPUs.
    #[serde(rename = "CPU", default)]
    pub cpu: i64,
    /// Memory in MiB.
    #[serde(rename = "Memory", default)]
    pub memory_mib: i64,
    /// Machine type, e.g. `N`, `C`, `O`.
    #[serde(rename = "UHostType", default)]
    pub machine_type: String,
    /// Charge type: `Month`, `Year`, `Dynamic`.
    #[serde(rename = "ChargeType", default)]
    pub charge_type: String,
    /// IPs, inner and public.
    #[serde(rename = "IPSet", default)]
    pub ips: Vec<HostIp>,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
}

/// One IP of a host.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct HostIp {
    /// The address.
    #[serde(rename = "IP", default)]
    pub ip: String,
    /// `Internode` (private) or `Bgp`/`International` (public).
    #[serde(rename = "Type", default)]
    pub kind: String,
}

/// `DescribeEIP` → <https://docs.ucloud.cn/api/unet-api/describe_eip>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EipList {
    /// Elastic IPs.
    #[serde(rename = "EIPSet", default)]
    pub eips: Vec<Eip>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// An elastic IP.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Eip {
    /// `eip-xxxx`.
    #[serde(rename = "EIPId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// The address.
    #[serde(rename = "IP", default)]
    pub ip: String,
    /// `free` / `used` / `unbind` …
    #[serde(rename = "State", default)]
    pub state: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Bandwidth in Mbps.
    #[serde(rename = "Bandwidth", default)]
    pub bandwidth: i64,
    /// `Dynamic` / `Month` / `Year`.
    #[serde(rename = "PayMode", default)]
    pub pay_mode: String,
    /// The resource it is bound to; empty when unbound.
    #[serde(rename = "Resource", default)]
    pub resource: Option<EipResource>,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
}

/// What an EIP is bound to.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EipResource {
    /// Resource id, e.g. `uhost-xxxx`.
    #[serde(rename = "ResourceId", default)]
    pub id: String,
    /// Resource type, e.g. `uhost`.
    #[serde(rename = "ResourceType", default)]
    pub kind: String,
}

/// `DescribeUDisk` → <https://docs.ucloud.cn/api/udisk-api/describe_udisk>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UDiskList {
    /// Disks.
    #[serde(rename = "DataSet", default)]
    pub disks: Vec<UDisk>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// A block-storage disk.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UDisk {
    /// `udisk-xxxx`.
    #[serde(rename = "UDiskId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// Size in GiB.
    #[serde(rename = "Size", default)]
    pub size_gib: i64,
    /// `Available` / `InUse` / `Failed`.
    #[serde(rename = "Status", default)]
    pub status: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Zone.
    #[serde(rename = "Zone", default)]
    pub zone: String,
    /// Attached host, empty when unattached.
    #[serde(rename = "UHostId", default)]
    pub uhost_id: String,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
}

/// `DescribeUDBInstance` → <https://docs.ucloud.cn/api/udb-api/describe_udb_instance>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UdbList {
    /// Instances.
    #[serde(rename = "DataSet", default)]
    pub instances: Vec<Udb>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// A managed database instance.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Udb {
    /// `udb-xxxx`.
    #[serde(rename = "DBId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// `Running` / `Stopped` …
    #[serde(rename = "State", default)]
    pub state: String,
    /// `mysql` / `postgresql` / `sqlserver` …
    #[serde(rename = "DBType", default)]
    pub db_type: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Zone.
    #[serde(rename = "Zone", default)]
    pub zone: String,
    /// Disk size in GiB.
    #[serde(rename = "DiskSpace", default)]
    pub disk_gib: i64,
    /// Charge type.
    #[serde(rename = "ChargeType", default)]
    pub charge_type: String,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
}

/// `DescribeUMem` → <https://docs.ucloud.cn/api/umem-api/describe_umem>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UMemList {
    /// Redis instances.
    #[serde(rename = "DataSet", default)]
    pub instances: Vec<UMem>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// A managed Redis instance.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UMem {
    /// `umem-xxxx`.
    #[serde(rename = "InstanceId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// `Running` / `Stopped` …
    #[serde(rename = "State", default)]
    pub state: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Zone.
    #[serde(rename = "Zone", default)]
    pub zone: String,
    /// Capacity in MiB.
    #[serde(rename = "Size", default)]
    pub size_mib: i64,
    /// Charge type.
    #[serde(rename = "ChargeType", default)]
    pub charge_type: String,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
}

/// `DescribeULB` → <https://docs.ucloud.cn/api/ulb-api/describe_ulb>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UlbList {
    /// Load balancers.
    #[serde(rename = "DataSet", default)]
    pub ulbs: Vec<Ulb>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// A load balancer.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Ulb {
    /// `ulb-xxxx`.
    #[serde(rename = "ULBId", default)]
    pub id: String,
    /// Display name.
    #[serde(rename = "Name", default)]
    pub name: String,
    /// Region id.
    #[serde(rename = "Region", default)]
    pub region: String,
    /// Virtual servers and their backends.
    #[serde(rename = "VServerSet", default)]
    pub vservers: Vec<VServer>,
    /// User tags.
    #[serde(rename = "Tag", default)]
    pub tags: String,
    /// Creation time, unix seconds.
    #[serde(rename = "CreateTime", default)]
    pub create_time: i64,
}

/// One virtual server of a ULB.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VServer {
    /// Virtual server id.
    #[serde(rename = "VServerId", default)]
    pub id: String,
    /// Protocol, e.g. `HTTP`.
    #[serde(rename = "Protocol", default)]
    pub protocol: String,
    /// Listening port.
    #[serde(rename = "FrontendPort", default)]
    pub port: i64,
    /// Backend hosts.
    #[serde(rename = "BackendSet", default)]
    pub backends: Vec<Backend>,
}

/// A backend attached to a virtual server.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Backend {
    /// Resource id of the backend (`uhost-xxxx`).
    #[serde(rename = "ResourceId", default)]
    pub id: String,
    /// `UHost` / `UDB` …
    #[serde(rename = "ResourceType", default)]
    pub kind: String,
    /// Backend port.
    #[serde(rename = "Port", default)]
    pub port: i64,
}

/// `ListBills` → <https://docs.ucloud.cn/api/ubill-api/list_bills>
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BillList {
    /// Bill entries.
    #[serde(rename = "Bills", default)]
    pub bills: Vec<Bill>,
    /// Total across pages.
    #[serde(rename = "TotalCount", default)]
    pub total: i64,
}

/// One bill entry.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Bill {
    /// Amount in yuan (negative for refunds).
    #[serde(rename = "Amount", default)]
    pub amount: f64,
    /// Resource id this entry belongs to.
    #[serde(rename = "ResourceId", default)]
    pub resource_id: String,
    /// Product name, e.g. `UHost`.
    #[serde(rename = "Product", default)]
    pub product: String,
    /// Billing day, `YYYY-MM-DD`.
    #[serde(rename = "Date", default)]
    pub date: String,
    /// Project id the entry is charged to.
    #[serde(rename = "ProjectId", default)]
    pub project_id: String,
}
