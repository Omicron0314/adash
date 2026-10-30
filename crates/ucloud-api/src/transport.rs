//! Transport: signed form requests against the live API, or fixture reads.
//!
//! The mock backend answers with the exact same JSON the live API returns, so
//! everything above this module is one code path.

use crate::credentials::Credentials;
use crate::error::{Error, Result};
use crate::paths::DEFAULT_ENDPOINT;
use crate::signature::sign;
use std::path::PathBuf;
use std::time::Duration;

/// Where responses come from.
pub enum Backend {
    /// Real UCloud.
    Live {
        /// API base, `https://api.ucloud.cn` unless overridden.
        endpoint: String,
        /// HTTP client with a bounded timeout.
        http: reqwest::blocking::Client,
        /// Whose account we are reading.
        credentials: Credentials,
    },
    /// Fixtures from `fixtures/mock`; no network, no money.
    Mock {
        /// Directory holding `<Action>.json` files.
        dir: PathBuf,
    },
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Live {
                endpoint,
                credentials,
                ..
            } => f
                .debug_struct("Live")
                .field("endpoint", endpoint)
                .field("credentials", credentials)
                .finish_non_exhaustive(),
            Self::Mock { dir } => f.debug_struct("Mock").field("dir", dir).finish(),
        }
    }
}

impl Backend {
    /// Build the live backend from the environment.
    pub fn live_from_env() -> Result<Self> {
        let credentials = Credentials::from_env()?;
        let endpoint =
            std::env::var("UDASH_API_BASE").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_owned());
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .user_agent(concat!("adash/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| Error::Transport(error.to_string()))?;
        Ok(Self::Live {
            endpoint,
            http,
            credentials,
        })
    }

    /// Build the fixture backend from `UDASH_MOCK_DIR` (or the in-repo default).
    pub fn mock() -> Self {
        Self::Mock { dir: mock_dir() }
    }

    /// Whether this backend spends real money if a mutate slips through.
    pub fn is_mock(&self) -> bool {
        matches!(self, Self::Mock { .. })
    }

    /// Perform one action. `params` are *business* parameters only.
    pub fn call(&self, action: &str, params: &[(String, String)]) -> Result<serde_json::Value> {
        match self {
            Self::Mock { dir } => mock_response(dir, action),
            Self::Live {
                endpoint,
                http,
                credentials,
            } => live_call(http, endpoint, credentials, action, params),
        }
    }
}

/// True when `UDASH_MOCK` is set to anything but `0`/`false`.
pub fn mock_enabled() -> bool {
    match std::env::var("UDASH_MOCK") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false"
        ),
        Err(_) => false,
    }
}

/// Fixture directory: `UDASH_MOCK_DIR`, else `<repo>/fixtures/mock`, else the
/// per-user data dir.
pub fn mock_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("UDASH_MOCK_DIR") {
        return PathBuf::from(dir);
    }
    let in_repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mock")
        .canonicalize();
    if let Ok(dir) = in_repo {
        return dir;
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("adash/fixtures")
}

fn mock_response(dir: &std::path::Path, action: &str) -> Result<serde_json::Value> {
    let path = dir.join(format!("{action}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|error| Error::Mock(format!("{}: {error}", path.display())))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| Error::Decode(error.to_string()))?;
    check_ret_code(&value)?;
    Ok(value)
}

fn live_call(
    http: &reqwest::blocking::Client,
    endpoint: &str,
    credentials: &Credentials,
    action: &str,
    params: &[(String, String)],
) -> Result<serde_json::Value> {
    let mut signed: Vec<(String, String)> = params.to_vec();
    signed.push(("Action".to_owned(), action.to_owned()));
    signed.push(("PublicKey".to_owned(), credentials.public_key.clone()));
    if let Some(project) = &credentials.project_id {
        signed.push(("ProjectId".to_owned(), project.clone()));
    }
    let signature = sign(&signed, &credentials.private_key);
    signed.push(("Signature".to_owned(), signature));

    let response = http
        .post(endpoint)
        .form(&signed)
        .send()
        .map_err(|error| Error::Transport(error.to_string()))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(Error::Transport(format!(
            "HTTP {} for {action}: {}",
            status.as_u16(),
            truncate(&body, 200)
        )));
    }

    let value: serde_json::Value = response
        .json()
        .map_err(|error| Error::Decode(error.to_string()))?;
    check_ret_code(&value)?;
    Ok(value)
}

fn check_ret_code(value: &serde_json::Value) -> Result<()> {
    let code = value
        .get("RetCode")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    if code == 0 {
        return Ok(());
    }
    let message = value
        .get("Message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown error");
    Err(Error::Api {
        code,
        message: message.to_owned(),
    })
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &text[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_backend_reads_fixtures() {
        let backend = Backend::mock();
        let response = backend
            .call(crate::paths::Action::GET_BALANCE, &[])
            .expect("fixture");
        assert_eq!(response["RetCode"], 0);
    }

    #[test]
    fn mock_backend_reports_missing_fixture() {
        let backend = Backend::mock();
        let error = backend.call("DescribeNothing", &[]).expect_err("must fail");
        assert!(matches!(error, Error::Mock(_)));
    }

    #[test]
    fn non_zero_ret_code_becomes_api_error() {
        let value = serde_json::json!({"RetCode": 8037, "Message": "signature failed"});
        let error = check_ret_code(&value).expect_err("must fail");
        assert!(matches!(error, Error::Api { code: 8037, .. }));
    }
}
