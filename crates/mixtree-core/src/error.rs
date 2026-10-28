//! Error types for the MixTree core.
//!
//! All fallible operations return [`Result`], a thin alias over [`CoreError`].
//! Errors are deliberately structured (not stringly-typed) so that the Tauri
//! command layer can translate them into meaningful messages for the UI.

use thiserror::Error;

/// The unified error type for every fallible operation in `mixtree-core`.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A database operation failed.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// A filesystem operation failed.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Audio metadata could not be read or parsed.
    #[error("metadata error for {path}: {message}")]
    Metadata {
        /// The file that could not be read.
        path: String,
        /// The underlying error message.
        message: String,
    },

    /// A requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// The caller supplied invalid input.
    #[error("invalid input: {0}")]
    Invalid(String),

    /// (De)serialization failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, CoreError>;
