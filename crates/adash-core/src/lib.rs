//! adash domain layer: what the dashboard and the agent both reason about.
//!
//! Planned modules (see docs/ARCHITECTURE.md): `resource` (typed inventory that
//! unifies every UCloud product), `project` (project = a group of resources),
//! `flow` (AI flow graph + executions, the n8n-style view), `budget`,
//! `billing`, `plan` (confirmable change set), `state` (local persistence).

pub mod resource;

pub use resource::{Resource, ResourceId, ResourceKind, ResourceState};
