//! # Brioche Shell Persistence — Book III-B
//!
//! Persistence layer for shell-side state. Handles serialization,
//! disk I/O, and hydration of `ExtensionStorage` snapshots.
//!
//! ## Public interface
//! - [`RedbStorage`]: Disk-backed storage engine implementing `Persistence`.
//! - [`SessionHeadDTO`]: Versioned session head for MessagePack serialization.
//! - [`SubRoutineCache`]: Two-level cache (L1 visible / L2 LRU) for sub-routine DTOs.
//! - [`SessionStore`] / [`SessionStoreEntry`]: Shared in-memory bridge between
//!   the engine thread and async persistence.
//!
//! ## Invariants upheld
//! - I-Persist-SaveSession: Atomic writes via Redb transactions.
//! - I-Persist-AppendOnly: `MESSAGES_TABLE` is append-only.
//! - I-Persist-PluginBlob: Cold blobs written without engine blocking.
//! - I-Persist-Cache: `SubRoutineCache` with L1 visible / L2 LRU tiers.
//!
//! Refs: docs/SPECS.md §Book III-B

pub mod dto;
pub mod extensions;
pub mod profiles;
pub mod settings;
pub mod skills;
pub mod storage;

pub use dto::{FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion};
pub use extensions::*;
pub use profiles::*;
pub use settings::*;
pub use skills::*;
pub use storage::{
    COMPRESSION_THRESHOLD, GcRunner, PersistenceError, RedbStorage, SessionStore,
    SessionStoreEntry, SubRoutineCache, deserialize_head, deserialize_message, extract_delta,
    load_subroutine, maybe_compress, maybe_decompress, new_session_store, serialize_head,
    serialize_message,
};

/// Hydrates a sub-routine session from a persisted MessagePack head blob.
///
/// This is the persistence-side implementation of the core
/// `SubRoutineHydrator` trait. It decodes the blob with `deserialize_head`
/// and reconstructs the session with an empty message history.
///
/// # Complexity
/// O(deserialization cost + session reconstruction).
///
/// # Errors
/// Returns `BriocheError::Serialization` if the blob cannot be decoded.
///
/// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
#[derive(Clone, Copy, Debug, Default)]
pub struct PersistenceSubRoutineHydrator;

impl brioche_core::SubRoutineHydrator for PersistenceSubRoutineHydrator {
    fn hydrate(
        &self,
        head_blob: &[u8],
    ) -> Result<brioche_core::Session, brioche_core::BriocheError> {
        let dto = crate::deserialize_head(head_blob)
            .map_err(|err| brioche_core::BriocheError::Serialization(err.to_string()))?;
        Ok(dto.to_session(Vec::new()))
    }
}
