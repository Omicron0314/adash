//! UCloud OpenAPI client — the single place UCloud is talked to.
//!
//! Signing, transport, typed response models and the mock backend live here.
//! The GUI links this crate directly; the agent reaches it only through
//! `adash-tool`. The TypeScript extension must never implement signing.
//!
//! Planned modules (see docs/ARCHITECTURE.md):
//! `signature`, `transport`, `paths`, `models`, `credentials`, `mock`, `client`, `error`.

pub mod error;

pub use error::{Error, Result};
