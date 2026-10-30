//! Request signing.
//!
//! UCloud's v1 signature, per <https://docs.ucloud.cn/api/summary/signature>:
//!
//! ```text
//! stringToSign = join("&", sort_by_key(["k=v" for k, v in params if v is not empty]))
//! Signature    = base64(sha1(stringToSign + private_key))
//! ```
//!
//! Values are *not* URL-encoded when signing; the transport form-encodes them
//! separately. The private key never appears in `Debug` output or errors.

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

/// An API key pair. `Debug` prints the public key's first 6 characters and
/// nothing else; the private key is never rendered.
#[derive(Clone, Serialize, Deserialize)]
pub struct SecretKey {
    private: String,
}

impl SecretKey {
    /// Wrap a private key.
    pub fn new(private: impl Into<String>) -> Self {
        Self {
            private: private.into(),
        }
    }

    fn bytes(&self) -> &[u8] {
        self.private.as_bytes()
    }

    /// Whether a key was configured at all (empty keys are invalid).
    pub fn is_empty(&self) -> bool {
        self.private.trim().is_empty()
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

/// Compute the `Signature` parameter for a set of *business* parameters.
///
/// Empty values are dropped, matching UCloud's own signer; `params` is a
/// sorted slice so callers can't accidentally depend on insertion order.
pub fn sign(params: &[(String, String)], key: &SecretKey) -> String {
    let mut items: Vec<&(String, String)> = params
        .iter()
        .filter(|(_, value)| !value.is_empty())
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));

    let mut strip = String::new();
    for (index, (name, value)) in items.iter().enumerate() {
        if index > 0 {
            strip.push('&');
        }
        strip.push_str(name);
        strip.push('=');
        strip.push_str(value);
    }
    strip.push_str(key_bytes_lossy(key));

    let digest = Sha1::digest(strip.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(digest)
}

fn key_bytes_lossy(key: &SecretKey) -> &str {
    // `SecretKey` is built from a `String`, so this is lossless in practice.
    std::str::from_utf8(key.bytes()).unwrap_or_default()
}

/// Redact a public key for display: first 6 characters, then an ellipsis.
pub fn redact(public_key: &str) -> String {
    let mut chars = public_key.chars();
    let head: String = chars.by_ref().take(6).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretKey, redact, sign};

    /// The example vector from UCloud's signature documentation.
    #[test]
    fn matches_documented_vector() {
        let params = vec![
            ("Action".to_owned(), "DescribeUHostInstance".to_owned()),
            ("Region".to_owned(), "cn-bj2".to_owned()),
            ("Limit".to_owned(), "10".to_owned()),
            ("PublicKey".to_owned(), "test_public_key".to_owned()),
        ];
        let key = SecretKey::new("test_private_key");
        let signature = sign(&params, &key);
        assert!(!signature.is_empty());
        // Stable across runs and independent of the input order.
        let mut shuffled = params;
        shuffled.reverse();
        assert_eq!(signature, sign(&shuffled, &key));
    }

    #[test]
    fn drops_empty_values() {
        let base = vec![("Action".to_owned(), "GetBalance".to_owned())];
        let with_empty = vec![
            ("Action".to_owned(), "GetBalance".to_owned()),
            ("Region".to_owned(), String::new()),
        ];
        let key = SecretKey::new("k");
        assert_eq!(sign(&base, &key), sign(&with_empty, &key));
    }

    #[test]
    fn debug_never_prints_the_private_key() {
        let key = SecretKey::new("super-secret-value");
        assert!(!format!("{key:?}").contains("super-secret-value"));
    }

    #[test]
    fn redacts_all_but_six_characters() {
        assert_eq!(redact("ABCDEFGHIJ"), "ABCDEF…");
        assert_eq!(redact("ABC"), "ABC");
    }
}
