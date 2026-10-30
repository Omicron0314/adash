//! Error type shared by every UCloud call path.

/// Anything that can go wrong talking to (or mocking) UCloud.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Transport-level failure: DNS, TLS, connect, timeout, non-2xx HTTP.
    #[error("transport: {0}")]
    Transport(String),
    /// UCloud answered with a non-zero `RetCode`.
    #[error("ucloud api error {code}: {message}")]
    Api {
        /// UCloud `RetCode`.
        code: i64,
        /// UCloud `Message`.
        message: String,
    },
    /// Response did not match the documented shape.
    #[error("decode: {0}")]
    Decode(String),
    /// Credentials missing or unreadable.
    #[error("credentials: {0}")]
    Credentials(String),
    /// Mock backend had no fixture for this call.
    #[error("mock: {0}")]
    Mock(String),
    /// The call needs a feature this demo does not implement yet.
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, Error>;
