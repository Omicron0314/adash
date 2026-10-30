//! Every UCloud API action this demo calls, with its documentation link.
//!
//! Field names come from the docs, not from memory: when a shape looks wrong,
//! check the link, then fix the mock fixture first (see docs/API.md).

/// API endpoint. `UDASH_API_BASE` overrides it for tests and proxies.
pub const DEFAULT_ENDPOINT: &str = "https://api.ucloud.cn";

/// Actions we call. The value is the exact `Action` query parameter.
pub struct Action;

impl Action {
    /// Account balance. <https://docs.ucloud.cn/api/account-api/get_balance>
    pub const GET_BALANCE: &'static str = "GetBalance";
    /// Region list. <https://docs.ucloud.cn/api/summary/regionlist>
    pub const GET_REGION: &'static str = "GetRegion";
    /// Cloud hosts. <https://docs.ucloud.cn/api/uhost-api/describe_uhost_instance>
    pub const DESCRIBE_UHOST: &'static str = "DescribeUHostInstance";
    /// Elastic IPs. <https://docs.ucloud.cn/api/unet-api/describe_eip>
    pub const DESCRIBE_EIP: &'static str = "DescribeEIP";
    /// Cloud disks. <https://docs.ucloud.cn/api/udisk-api/describe_udisk>
    pub const DESCRIBE_UDISK: &'static str = "DescribeUDisk";
    /// Managed databases (UDB). <https://docs.ucloud.cn/api/udb-api/describe_udb_instance>
    pub const DESCRIBE_UDB: &'static str = "DescribeUDBInstance";
    /// Managed Redis (UMem). <https://docs.ucloud.cn/api/umem-api/describe_umem>
    pub const DESCRIBE_UMEM: &'static str = "DescribeUMem";
    /// Load balancers. <https://docs.ucloud.cn/api/ulb-api/describe_ulb>
    pub const DESCRIBE_ULB: &'static str = "DescribeULB";
    /// Bill entries. <https://docs.ucloud.cn/api/ubill-api/list_bills>
    pub const LIST_BILLS: &'static str = "ListBills";
}

/// Every action the mock backend can answer, in inventory order.
pub const ALL_ACTIONS: [&str; 9] = [
    Action::GET_BALANCE,
    Action::GET_REGION,
    Action::DESCRIBE_UHOST,
    Action::DESCRIBE_EIP,
    Action::DESCRIBE_UDISK,
    Action::DESCRIBE_UDB,
    Action::DESCRIBE_UMEM,
    Action::DESCRIBE_ULB,
    Action::LIST_BILLS,
];
