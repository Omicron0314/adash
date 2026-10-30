//! UCloud OpenAPI client — the single place UCloud is talked to.
//!
//! Signing, transport, typed response models and the mock backend live here.
//! The GUI links this crate directly; the agent reaches it only through
//! `adash-tool`. The TypeScript extension must never implement signing.
//!
//! Layering: [`paths`] names the actions, [`signature`] signs them,
//! [`transport`] performs them (live or fixture), [`models`] types the answers,
//! and [`client`] is the facade other crates use.

pub mod client;
pub mod credentials;
pub mod error;
pub mod models;
pub mod paths;
pub mod signature;
pub mod transport;

pub use client::{Mode, UcloudClient};
pub use credentials::Credentials;
pub use error::{Error, Result};
pub use models::*;
pub use signature::{SecretKey, redact};
