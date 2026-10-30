//! Pull every product into one [`Inventory`].
//!
//! A single failing product must not blank the dashboard: each call's error is
//! collected into [`Inventory::errors`] and the rest of the account still
//! renders. The GUI shows those errors as a dismissible notice; the agent gets
//! them in the tool result so it can explain the gap instead of inventing data.

use crate::price;
use crate::resource::{Resource, ResourceId, ResourceKind, ResourceState, money};
use std::collections::BTreeMap;
use ucloud_api::UcloudClient;

/// A point-in-time view of the account.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    /// Every resource we could read, in product order.
    pub resources: Vec<Resource>,
    /// Unix seconds of the last successful refresh.
    pub fetched_at: i64,
    /// One entry per product call that failed.
    pub errors: Vec<String>,
    /// Spendable balance in yuan, when the call succeeded.
    pub balance_yuan: Option<f64>,
}

impl Inventory {
    /// Read the whole account. Never fails; see [`Inventory::errors`].
    pub fn load(client: &UcloudClient) -> Self {
        let mut inventory = Self {
            fetched_at: crate::state::now_secs(),
            ..Self::default()
        };

        match client.balance() {
            Ok(balance) => inventory.balance_yuan = Some(balance.account.amount),
            Err(error) => inventory.errors.push(format!("余额：{error}")),
        }

        let mut push = |result: Result<Vec<Resource>, ucloud_api::Error>, label: &str| match result
        {
            Ok(resources) => inventory.resources.extend(resources),
            Err(error) => inventory.errors.push(format!("{label}：{error}")),
        };

        push(
            client
                .uhosts()
                .map(|list| list.hosts.iter().map(host_resource).collect()),
            "云主机",
        );
        push(
            client
                .eips()
                .map(|list| list.eips.iter().map(eip_resource).collect()),
            "弹性 IP",
        );
        push(
            client
                .udisks()
                .map(|list| list.disks.iter().map(disk_resource).collect()),
            "云硬盘",
        );
        push(
            client
                .udbs()
                .map(|list| list.instances.iter().map(udb_resource).collect()),
            "云数据库",
        );
        push(
            client
                .redis()
                .map(|list| list.instances.iter().map(redis_resource).collect()),
            "Redis",
        );
        push(
            client
                .ulbs()
                .map(|list| list.ulbs.iter().map(ulb_resource).collect()),
            "负载均衡",
        );

        inventory
    }

    /// Look one resource up by id.
    pub fn get(&self, id: &ResourceId) -> Option<&Resource> {
        self.resources.iter().find(|resource| &resource.id == id)
    }

    /// Resources of one product, in inventory order.
    pub fn of_kind(&self, kind: ResourceKind) -> impl Iterator<Item = &Resource> {
        self.resources
            .iter()
            .filter(move |resource| resource.id.kind == kind)
    }

    /// Counts per product, skipping empty products, in [`ResourceKind::ALL`] order.
    pub fn counts(&self) -> Vec<(ResourceKind, usize)> {
        ResourceKind::ALL
            .iter()
            .filter_map(|kind| {
                let count = self.of_kind(*kind).count();
                (count > 0).then_some((*kind, count))
            })
            .collect()
    }

    /// Project slugs seen in tags, sorted.
    pub fn projects(&self) -> Vec<String> {
        let mut slugs: Vec<String> = self
            .resources
            .iter()
            .filter_map(|resource| resource.project.clone())
            .collect();
        slugs.sort();
        slugs.dedup();
        slugs
    }

    /// Slugs for the flow page: real projects plus `unassigned` when untagged
    /// resources exist. Bills for those resources are real, so they need a page.
    pub fn flow_projects(&self) -> Vec<String> {
        let mut slugs = self.projects();
        if self
            .resources
            .iter()
            .any(|resource| resource.project.is_none())
        {
            slugs.push("unassigned".to_owned());
        }
        slugs
    }

    /// Region ids seen, sorted.
    pub fn regions(&self) -> Vec<String> {
        let mut regions: Vec<String> = self
            .resources
            .iter()
            .map(|resource| resource.region.clone())
            .collect();
        regions.sort();
        regions.dedup();
        regions
    }

    /// Resources of one project, unassigned included when `slug` is `None`.
    pub fn of_project(&self, slug: Option<&str>) -> Vec<&Resource> {
        self.resources
            .iter()
            .filter(|resource| resource.project.as_deref() == slug)
            .collect()
    }

    /// Sum of the monthly estimates, in cents.
    pub fn monthly_total_cents(&self) -> i64 {
        self.resources
            .iter()
            .map(|resource| resource.monthly_cost_cents)
            .sum()
    }

    /// One-line summary for the agent and the overview card.
    pub fn summary_line(&self) -> String {
        let counts = self
            .counts()
            .into_iter()
            .map(|(kind, count)| format!("{} {}", kind.label(), count))
            .collect::<Vec<_>>()
            .join(" · ");
        format!(
            "{} 个资源（{}），月成本约 {}",
            self.resources.len(),
            counts,
            money(self.monthly_total_cents())
        )
    }
}

/// The `adash:<slug>` tag, if present.
fn project_tag(tags: &str) -> Option<String> {
    tags.split(',')
        .map(str::trim)
        .find_map(|tag| tag.strip_prefix("adash:"))
        .filter(|slug| !slug.is_empty())
        .map(str::to_owned)
}

fn host_resource(host: &ucloud_api::UHost) -> Resource {
    let mut attributes = BTreeMap::new();
    attributes.insert("计费".to_owned(), host.charge_type.clone());
    attributes.insert("机型".to_owned(), host.machine_type.clone());
    let public_ip = host
        .ips
        .iter()
        .find(|ip| ip.kind != "Internode")
        .map(|ip| ip.ip.clone());
    let private_ip = host
        .ips
        .iter()
        .find(|ip| ip.kind == "Internode")
        .map(|ip| ip.ip.clone());
    if let Some(ip) = &public_ip {
        attributes.insert("公网 IP".to_owned(), ip.clone());
    }
    if let Some(ip) = &private_ip {
        attributes.insert("内网 IP".to_owned(), ip.clone());
    }
    let memory_gib = (host.memory_mib / 1024).max(1);
    let spec = format!("{}C / {memory_gib}GiB", host.cpu.max(1));
    Resource {
        id: ResourceId::new(ResourceKind::UHost, &host.id),
        name: display_name(&host.name, &host.id),
        region: host.region.clone(),
        zone: Some(host.zone.clone()).filter(|zone| !zone.is_empty()),
        state: ResourceState::from_ucloud(&host.state),
        project: project_tag(&host.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::UHost, &spec),
        spec,
        public_ip,
        private_ip,
        attached_to: Vec::new(),
        attributes,
        created_at: Some(host.create_time).filter(|time| *time > 0),
    }
}

fn eip_resource(eip: &ucloud_api::Eip) -> Resource {
    let mut attributes = BTreeMap::new();
    attributes.insert("带宽".to_owned(), format!("{}Mbps", eip.bandwidth));
    attributes.insert("计费".to_owned(), eip.pay_mode.clone());
    let attached_to = eip
        .resource
        .as_ref()
        .filter(|resource| !resource.id.is_empty())
        .map(|resource| {
            attributes.insert("绑定到".to_owned(), resource.id.clone());
            vec![ResourceId::new(
                kind_from_ucloud(&resource.kind),
                &resource.id,
            )]
        })
        .unwrap_or_default();
    Resource {
        id: ResourceId::new(ResourceKind::Eip, &eip.id),
        name: display_name(&eip.name, &eip.ip),
        region: eip.region.clone(),
        zone: None,
        state: ResourceState::from_ucloud(&eip.state),
        project: project_tag(&eip.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::Eip, "5Mbps"),
        spec: format!("{}Mbps", eip.bandwidth),
        public_ip: Some(eip.ip.clone()).filter(|ip| !ip.is_empty()),
        private_ip: None,
        attached_to,
        attributes,
        created_at: Some(eip.create_time).filter(|time| *time > 0),
    }
}

fn disk_resource(disk: &ucloud_api::UDisk) -> Resource {
    let mut attributes = BTreeMap::new();
    attributes.insert("状态".to_owned(), disk.status.clone());
    let attached_to = if disk.uhost_id.is_empty() {
        Vec::new()
    } else {
        vec![ResourceId::new(ResourceKind::UHost, &disk.uhost_id)]
    };
    let spec = format!("{}GiB", disk.size_gib);
    Resource {
        id: ResourceId::new(ResourceKind::Disk, &disk.id),
        name: display_name(&disk.name, &disk.id),
        region: disk.region.clone(),
        zone: Some(disk.zone.clone()).filter(|zone| !zone.is_empty()),
        state: ResourceState::from_ucloud(&disk.status),
        project: project_tag(&disk.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::Disk, &spec),
        spec,
        public_ip: None,
        private_ip: None,
        attached_to,
        attributes,
        created_at: Some(disk.create_time).filter(|time| *time > 0),
    }
}

fn udb_resource(udb: &ucloud_api::Udb) -> Resource {
    let mut attributes = BTreeMap::new();
    attributes.insert("类型".to_owned(), udb.db_type.to_uppercase());
    attributes.insert("计费".to_owned(), udb.charge_type.clone());
    let spec = format!("{} · {}GiB", udb.db_type.to_uppercase(), udb.disk_gib);
    Resource {
        id: ResourceId::new(ResourceKind::Udb, &udb.id),
        name: display_name(&udb.name, &udb.id),
        region: udb.region.clone(),
        zone: Some(udb.zone.clone()).filter(|zone| !zone.is_empty()),
        state: ResourceState::from_ucloud(&udb.state),
        project: project_tag(&udb.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::Udb, &spec),
        spec,
        public_ip: None,
        private_ip: None,
        attached_to: Vec::new(),
        attributes,
        created_at: Some(udb.create_time).filter(|time| *time > 0),
    }
}

fn redis_resource(redis: &ucloud_api::UMem) -> Resource {
    let mut attributes = BTreeMap::new();
    attributes.insert("计费".to_owned(), redis.charge_type.clone());
    let spec = format!("{}GiB", (redis.size_mib / 1024).max(1));
    Resource {
        id: ResourceId::new(ResourceKind::Redis, &redis.id),
        name: display_name(&redis.name, &redis.id),
        region: redis.region.clone(),
        zone: Some(redis.zone.clone()).filter(|zone| !zone.is_empty()),
        state: ResourceState::from_ucloud(&redis.state),
        project: project_tag(&redis.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::Redis, &spec),
        spec,
        public_ip: None,
        private_ip: None,
        attached_to: Vec::new(),
        attributes,
        created_at: Some(redis.create_time).filter(|time| *time > 0),
    }
}

fn ulb_resource(ulb: &ucloud_api::Ulb) -> Resource {
    let mut attributes = BTreeMap::new();
    let mut attached_to = Vec::new();
    for vserver in &ulb.vservers {
        attributes.insert(
            format!("监听 {}/{}", vserver.protocol, vserver.port),
            format!("{} 个后端", vserver.backends.len()),
        );
        for backend in &vserver.backends {
            attached_to.push(ResourceId::new(
                kind_from_ucloud(&backend.kind),
                &backend.id,
            ));
        }
    }
    attached_to.sort();
    attached_to.dedup();
    Resource {
        id: ResourceId::new(ResourceKind::Ulb, &ulb.id),
        name: display_name(&ulb.name, &ulb.id),
        region: ulb.region.clone(),
        zone: None,
        state: ResourceState::Running,
        project: project_tag(&ulb.tags),
        monthly_cost_cents: price::monthly_cents(ResourceKind::Ulb, ""),
        spec: format!("{} 个监听", ulb.vservers.len()),
        public_ip: None,
        private_ip: None,
        attached_to,
        attributes,
        created_at: Some(ulb.create_time).filter(|time| *time > 0),
    }
}

fn kind_from_ucloud(raw: &str) -> ResourceKind {
    match raw.trim().to_ascii_lowercase().as_str() {
        "eip" => ResourceKind::Eip,
        "udisk" | "disk" => ResourceKind::Disk,
        "udb" => ResourceKind::Udb,
        "umem" | "redis" => ResourceKind::Redis,
        "ulb" => ResourceKind::Ulb,
        "ufile" | "us3" | "bucket" => ResourceKind::Bucket,
        _ => ResourceKind::UHost,
    }
}

fn display_name(name: &str, fallback: &str) -> String {
    if name.trim().is_empty() {
        fallback.to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_inventory() -> Inventory {
        Inventory::load(&UcloudClient::mock())
    }

    #[test]
    fn mock_account_loads_without_errors() {
        let inventory = mock_inventory();
        assert!(inventory.errors.is_empty(), "{:?}", inventory.errors);
        assert_eq!(inventory.resources.len(), 11);
        assert_eq!(inventory.balance_yuan, Some(1280.55));
    }

    #[test]
    fn tags_become_projects() {
        let inventory = mock_inventory();
        assert_eq!(inventory.projects(), vec!["batch", "shop-api"]);
        assert_eq!(inventory.of_project(Some("shop-api")).len(), 8);
    }

    #[test]
    fn eip_and_disk_keep_their_attachments() {
        let inventory = mock_inventory();
        let eip = inventory
            .of_kind(ResourceKind::Eip)
            .find(|resource| resource.id.id == "eip-shopweb1")
            .expect("fixture eip");
        assert_eq!(
            eip.attached_to,
            vec![ResourceId::new(ResourceKind::UHost, "uhost-shopweb1")]
        );

        let orphan = inventory
            .of_kind(ResourceKind::Eip)
            .find(|resource| resource.id.id == "eip-orphan1")
            .expect("fixture orphan");
        assert!(orphan.attached_to.is_empty());
        assert_eq!(orphan.state, ResourceState::Available);
    }

    #[test]
    fn monthly_total_is_positive_and_matches_price_table() {
        let inventory = mock_inventory();
        // 2×2C hosts + 8C host + disk(100+200GiB) + udb + redis + ulb + 3 eip
        assert_eq!(
            inventory.monthly_total_cents(),
            18640 * 2 + 74560 + 12000 + 9800 + 7700 + 1800 + 7200
        );
    }
}
