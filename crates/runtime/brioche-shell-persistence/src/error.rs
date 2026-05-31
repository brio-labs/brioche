//! Persistence error types.
//!
//! Refs: I-Core-NoPanic

use thiserror::Error;

/// Errors originating in the shell persistence layer.
///
/// All operations return `Result<T, PersistenceError>`; panics are
/// prohibited by clippy and philosophy.
///
/// Refs: I-Core-NoPanic
#[derive(Debug, Error)]
pub enum PersistenceError {
    /// Underlying Redb database error.
    #[error("redb error: {0}")]
    Redb(#[from] redb::Error),

    /// MessagePack or JSON serialization failure.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Session identifier not found in the in-memory store.
    #[error("session not found: {0}")]
    SessionNotFound(String),

    /// Zstd compression or decompression failure.
    #[error("compression error: {0}")]
    Compression(String),

    /// GC task was interrupted (e.g. by cancellation token).
    #[error("gc interrupted: {0}")]
    GcInterrupted(String),
}

impl From<redb::TransactionError> for PersistenceError {
    fn from(err: redb::TransactionError) -> Self {
        Self::Redb(err.into())
    }
}

impl From<redb::TableError> for PersistenceError {
    fn from(err: redb::TableError) -> Self {
        Self::Redb(err.into())
    }
}

impl From<redb::CommitError> for PersistenceError {
    fn from(err: redb::CommitError) -> Self {
        Self::Redb(err.into())
    }
}

impl From<redb::StorageError> for PersistenceError {
    fn from(err: redb::StorageError) -> Self {
        Self::Redb(err.into())
    }
}

impl From<redb::DatabaseError> for PersistenceError {
    fn from(err: redb::DatabaseError) -> Self {
        Self::Redb(err.into())
    }
}
