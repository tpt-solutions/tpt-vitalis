//! Shared error type and `Result` alias used across all Vitalis crates.

use std::fmt;

/// The unified error type for the Vitalis survival stack.
///
/// Each variant maps to a broad failure class. Crates may embed richer
/// context (e.g. an underlying I/O or crypto error) via [`Error::Other`].
#[derive(Debug)]
pub enum Error {
    /// Serialization or deserialization of a persisted/shared type failed.
    Encode(String),
    /// A required value was missing or a contract was violated.
    Invalid(String),
    /// A resource acquisition or budget operation was rejected.
    Resource(String),
    /// A replication, migration, or copy-limit policy was violated.
    Replication(String),
    /// A cryptographic integrity or authentication check failed.
    Integrity(String),
    /// A capability was absent or a request was outside its scope.
    Capability(String),
    /// Transport / peer / network operation failed (or is unavailable).
    Transport(String),
    /// Catch-all for errors that don't fit the classes above.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Encode(m) => write!(f, "encode error: {m}"),
            Error::Invalid(m) => write!(f, "invalid: {m}"),
            Error::Resource(m) => write!(f, "resource error: {m}"),
            Error::Replication(m) => write!(f, "replication error: {m}"),
            Error::Integrity(m) => write!(f, "integrity error: {m}"),
            Error::Capability(m) => write!(f, "capability error: {m}"),
            Error::Transport(m) => write!(f, "transport error: {m}"),
            Error::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Other(Box::new(e))
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Encode(e.to_string())
    }
}

impl From<postcard::Error> for Error {
    fn from(e: postcard::Error) -> Self {
        Error::Encode(e.to_string())
    }
}

impl From<hex::FromHexError> for Error {
    fn from(e: hex::FromHexError) -> Self {
        Error::Invalid(e.to_string())
    }
}

/// Convenience `Result` alias used across the workspace.
pub type Result<T> = std::result::Result<T, Error>;
