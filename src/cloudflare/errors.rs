use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum CloudflareError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api {
        status: u16,
        message: String,
    },

    #[error("configuration version changed: expected {expected}, got {actual}")]
    VersionChanged {
        expected: i64,
        actual: i64,
    },

    #[error("configuration SHA-256 mismatch")]
    Sha256Mismatch,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("tunnel not found: {0}")]
    TunnelNotFound(String),

    #[error("ingress rule not found: {0}")]
    IngressNotFound(String),

    #[error("ambiguous match: multiple ingress rules match hostname {0}")]
    AmbiguousMatch(String),

    #[error("{0}")]
    Other(String),
}
