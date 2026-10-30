//! Credential loading.
//!
//! Only two sources, both outside the model's reach: environment variables and
//! (later) the system keyring. `Credentials` never renders the private key.

use crate::error::{Error, Result};
use crate::signature::SecretKey;

/// A resolved API identity.
#[derive(Clone)]
pub struct Credentials {
    /// Public key; safe to show in a truncated form.
    pub public_key: String,
    /// Private key; never rendered.
    pub private_key: SecretKey,
    /// Default project id, when the account has one.
    pub project_id: Option<String>,
    /// Default region, when configured.
    pub region: Option<String>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("public_key", &crate::signature::redact(&self.public_key))
            .field("private_key", &self.private_key)
            .field("project_id", &self.project_id)
            .field("region", &self.region)
            .finish()
    }
}

impl Credentials {
    /// Read `UCLOUD_PUBLIC_KEY` / `UCLOUD_PRIVATE_KEY` (plus optional
    /// `UCLOUD_PROJECT_ID` and `UCLOUD_REGION`).
    ///
    /// Keyring support lands with M2; env vars are the supported demo path and
    /// the one the docs and tests use.
    pub fn from_env() -> Result<Self> {
        let public_key = non_empty("UCLOUD_PUBLIC_KEY")
            .ok_or_else(|| Error::Credentials("UCLOUD_PUBLIC_KEY is not set".into()))?;
        let private = non_empty("UCLOUD_PRIVATE_KEY")
            .ok_or_else(|| Error::Credentials("UCLOUD_PRIVATE_KEY is not set".into()))?;
        let private_key = SecretKey::new(private);
        if private_key.is_empty() {
            return Err(Error::Credentials("UCLOUD_PRIVATE_KEY is empty".into()));
        }
        Ok(Self {
            public_key,
            private_key,
            project_id: non_empty("UCLOUD_PROJECT_ID"),
            region: non_empty("UCLOUD_REGION"),
        })
    }

    /// The public key, redacted for display.
    pub fn public_key_redacted(&self) -> String {
        crate::signature::redact(&self.public_key)
    }
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_both_keys() {
        let credentials = Credentials {
            public_key: "ABCDEFGHIJKLMNOP".into(),
            private_key: SecretKey::new("private-material"),
            project_id: Some("org-demo".into()),
            region: Some("cn-bj2".into()),
        };
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains("private-material"));
        assert!(!rendered.contains("ABCDEFGHIJKLMNOP"));
        assert!(rendered.contains("ABCDEF…"));
    }
}
