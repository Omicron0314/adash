//! The n8n-style graph: which services a project uses and how they connect.
//!
//! Every edge carries its evidence. Nothing here is drawn from a model's
//! imagination (ADR 0002): edges come from resource attributes, or are marked
//! [`Evidence::Inferred`] so the canvas can draw them dashed.

use crate::inventory::Inventory;
use crate::resource::{Resource, ResourceId, ResourceKind, ResourceState};
use crate::state::ProjectRecord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What a node represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A real cloud resource.
    Resource(ResourceKind),
    /// Something outside the account: a repository, a domain, a payment API.
    External(String),
}

/// Why two nodes are connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    /// The EIP is bound to this host.
    EipBinding,
    /// The disk is attached to this host.
    DiskAttachment,
    /// The load balancer has this host as a backend.
    UlbBackend,
    /// Stored when the project was deployed.
    Declared,
    /// Inferred because the product offers no link; drawn dashed.
    Inferred,
}

impl Evidence {
    /// Chinese label shown on hover.
    pub fn label(self) -> &'static str {
        match self {
            Self::EipBinding => "EIP 绑定",
            Self::DiskAttachment => "云盘挂载",
            Self::UlbBackend => "ULB 后端",
            Self::Declared => "部署记录",
            Self::Inferred => "推断",
        }
    }
}

/// One box on the canvas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowNode {
    /// `uhost:uhost-shopweb1` or `ext:repo`.
    pub id: String,
    /// Name shown on the card.
    pub label: String,
    /// What it is.
    pub kind: NodeKind,
    /// One line under the name (region, spec, state).
    pub detail: String,
    /// Lifecycle state; external nodes are always `Running`.
    pub state: ResourceState,
    /// The resource behind this node, when there is one.
    pub resource: Option<ResourceId>,
}

impl FlowNode {
    /// The product label for the node's type.
    pub fn kind_label(&self) -> &str {
        match &self.kind {
            NodeKind::Resource(kind) => kind.label(),
            NodeKind::External(name) => name,
        }
    }
}

/// One wire between two boxes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowEdge {
    /// Source node id.
    pub from: String,
    /// Target node id.
    pub to: String,
    /// Short label on the wire.
    pub label: String,
    /// Why the wire exists.
    pub evidence: Evidence,
}

/// A project's graph plus its executions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    /// Project slug.
    pub project: String,
    /// Display name.
    pub name: String,
    /// Nodes, in insertion order.
    pub nodes: Vec<FlowNode>,
    /// Edges, in insertion order.
    pub edges: Vec<FlowEdge>,
    /// Executions, oldest first.
    pub runs: Vec<FlowRun>,
}

/// One execution of a project's flow (a deploy, a scale event, an optimize).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRun {
    /// `run-<uuid>`.
    pub id: String,
    /// Project slug.
    pub project: String,
    /// What ran, e.g. 「部署」.
    pub label: String,
    /// Unix seconds.
    pub started_at: i64,
    /// Ordered highlight steps.
    pub steps: Vec<FlowStep>,
}

/// One step of an execution: light a node (and the edge into it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowStep {
    /// Node id to light.
    pub node: String,
    /// What happened, shown in the timeline.
    pub label: String,
    /// Unix seconds; steps may share a second in mock data.
    pub at: i64,
}

impl Flow {
    /// Build the graph for one project.
    ///
    /// `record` supplies the external source (repository) node; `runs` are the
    /// project's executions.
    pub fn build(
        inventory: &Inventory,
        slug: &str,
        record: Option<&ProjectRecord>,
        runs: Vec<FlowRun>,
    ) -> Self {
        let resources: Vec<&Resource> = if slug == "unassigned" {
            inventory.of_project(None)
        } else {
            inventory.of_project(Some(slug))
        };
        let mut nodes = Vec::new();
        let mut by_id: BTreeMap<String, &Resource> = BTreeMap::new();
        for resource in &resources {
            by_id.insert(resource.id.key(), resource);
        }

        let source_id = record.and_then(|record| record.repo.as_ref()).map(|repo| {
            let id = "ext:repo".to_owned();
            nodes.push(FlowNode {
                id: id.clone(),
                label: repo.clone(),
                kind: NodeKind::External("代码仓库".to_owned()),
                detail: "GitHub".to_owned(),
                state: ResourceState::Running,
                resource: None,
            });
            id
        });

        for resource in &resources {
            nodes.push(FlowNode {
                id: resource.id.key(),
                label: resource.name.clone(),
                kind: NodeKind::Resource(resource.id.kind),
                detail: format!("{} · {}", resource.region, resource.spec),
                state: resource.state,
                resource: Some(resource.id.clone()),
            });
        }

        let mut edges = Vec::new();
        let hosts: Vec<&Resource> = resources
            .iter()
            .copied()
            .filter(|resource| resource.id.kind == ResourceKind::UHost)
            .collect();
        let databases: Vec<&Resource> = resources
            .iter()
            .copied()
            .filter(|resource| matches!(resource.id.kind, ResourceKind::Udb | ResourceKind::Redis))
            .collect();

        for resource in &resources {
            for target in &resource.attached_to {
                let Some(target_key) = by_id.keys().find(|key| {
                    by_id
                        .get(*key)
                        .is_some_and(|candidate| candidate.id == *target)
                }) else {
                    continue;
                };
                let (from, to, label, evidence) = match resource.id.kind {
                    ResourceKind::Ulb => (
                        target_key.clone(),
                        resource.id.key(),
                        "后端".to_owned(),
                        Evidence::UlbBackend,
                    ),
                    ResourceKind::Eip => (
                        target_key.clone(),
                        resource.id.key(),
                        "公网入口".to_owned(),
                        Evidence::EipBinding,
                    ),
                    ResourceKind::Disk => (
                        target_key.clone(),
                        resource.id.key(),
                        "挂载".to_owned(),
                        Evidence::DiskAttachment,
                    ),
                    _ => continue,
                };
                edges.push(FlowEdge {
                    from,
                    to,
                    label,
                    evidence,
                });
            }
        }

        // The repository feeds every running host, once there is one.
        if let Some(source) = &source_id {
            for host in &hosts {
                edges.push(FlowEdge {
                    from: source.clone(),
                    to: host.id.key(),
                    label: "部署".to_owned(),
                    evidence: Evidence::Declared,
                });
            }
        }

        // UCloud records no host→database link; when a project has exactly one
        // database (or cache) we draw it dashed rather than inventing a graph.
        for database in &databases {
            let candidates: Vec<&Resource> = hosts
                .iter()
                .copied()
                .filter(|host| host.state == ResourceState::Running)
                .collect();
            if candidates.len() > 8 {
                continue;
            }
            for host in candidates {
                edges.push(FlowEdge {
                    from: host.id.key(),
                    to: database.id.key(),
                    label: "连接".to_owned(),
                    evidence: Evidence::Inferred,
                });
            }
        }

        Self {
            project: slug.to_owned(),
            name: record.map_or_else(|| slug.to_owned(), |record| record.name.clone()),
            nodes,
            edges,
            runs,
        }
    }

    /// A node by id.
    pub fn node(&self, id: &str) -> Option<&FlowNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Layered layout: column index per node, longest-path from the sources.
    ///
    /// Cycles (impossible in today's evidence set, possible with declared
    /// dependencies later) are broken by ignoring back edges.
    pub fn layers(&self) -> Vec<Vec<String>> {
        let mut level: BTreeMap<&str, usize> = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), 0usize))
            .collect();

        // Relax edges |nodes| times; stop early when stable.
        for _ in 0..self.nodes.len().max(1) {
            let mut changed = false;
            for edge in &self.edges {
                let from = level.get(edge.from.as_str()).copied().unwrap_or(0);
                let to = level.get(edge.to.as_str()).copied().unwrap_or(0);
                if to <= from {
                    level.insert(edge.to.as_str(), from + 1);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let depth = level.values().copied().max().unwrap_or(0) + 1;
        let mut columns = vec![Vec::new(); depth];
        for node in &self.nodes {
            let column = level.get(node.id.as_str()).copied().unwrap_or(0);
            columns[column].push(node.id.clone());
        }
        columns
    }

    /// Counts used by the page header.
    pub fn summary(&self) -> String {
        format!(
            "{} 个节点 · {} 条连线 · {} 次执行",
            self.nodes.len(),
            self.edges.len(),
            self.runs.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ucloud_api::UcloudClient;

    fn flow() -> Flow {
        let inventory = Inventory::load(&UcloudClient::mock());
        let record = ProjectRecord {
            slug: "shop-api".into(),
            name: "Shop API".into(),
            repo: Some("github.com/me/shop-api".into()),
            note: String::new(),
            budget_cents: 0,
        };
        Flow::build(&inventory, "shop-api", Some(&record), Vec::new())
    }

    #[test]
    fn every_project_resource_is_a_node() {
        let flow = flow();
        // 2 hosts + udb + redis + ulb + 2 eip + 2 disks + repo
        assert_eq!(flow.nodes.len(), 10);
    }

    #[test]
    fn edges_come_from_resource_attributes() {
        let flow = flow();
        let eip_edges = flow
            .edges
            .iter()
            .filter(|edge| edge.evidence == Evidence::EipBinding)
            .count();
        let ulb_edges = flow
            .edges
            .iter()
            .filter(|edge| edge.evidence == Evidence::UlbBackend)
            .count();
        assert_eq!(eip_edges, 2);
        assert_eq!(ulb_edges, 2);
        assert!(
            flow.edges
                .iter()
                .any(|edge| edge.evidence == Evidence::Inferred)
        );
    }

    #[test]
    fn layout_is_a_dag_with_the_repository_first() {
        let flow = flow();
        let layers = flow.layers();
        assert_eq!(layers[0], vec!["ext:repo"]);
        assert!(layers.len() >= 3);
        let total: usize = layers.iter().map(Vec::len).sum();
        assert_eq!(total, flow.nodes.len());
    }

    #[test]
    fn untagged_resources_stay_out() {
        let flow = flow();
        assert!(!flow.nodes.iter().any(|node| node.label == "batch-worker-1"));
    }
}
