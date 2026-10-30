//! adash domain layer: what the dashboard and the agent both reason about.
//!
//! Nothing here performs HTTP; [`inventory`] and [`billing`] read through
//! `ucloud-api`, everything else is pure data and rules. The GUI links this
//! crate directly, the agent reaches it through `adash-tool`.

pub mod billing;
pub mod budget;
pub mod executor;
pub mod flow;
pub mod inventory;
pub mod optimizer;
pub mod plan;
pub mod price;
pub mod project;
pub mod resource;
pub mod state;

pub use billing::Billing;
pub use budget::{Budget, BudgetStatus};
pub use flow::{Evidence, Flow, FlowEdge, FlowNode, FlowRun, FlowStep, NodeKind};
pub use inventory::Inventory;
pub use optimizer::Finding;
pub use plan::{Plan, PlanAction, PlanKind, Verb};
pub use project::{ProjectView, project_views};
pub use resource::{Resource, ResourceId, ResourceKind, ResourceState, money};
pub use state::{ProjectRecord, Store};
