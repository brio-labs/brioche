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
//! Refs: SPECS.md §Book III-B

pub mod cache;
pub mod dto;
pub mod error;
pub mod gc;
pub mod load;
pub mod save;
pub mod schema;
pub mod storage;

pub use cache::SubRoutineCache;
pub use dto::{FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion};
pub use error::PersistenceError;
pub use gc::GcRunner;
pub use load::{LazySessionLoader, load_session_full, load_subroutine};
pub use save::{
    COMPRESSION_THRESHOLD, deserialize_head, deserialize_message, extract_delta, maybe_compress,
    maybe_decompress, serialize_head, serialize_message,
};
pub use storage::{RedbStorage, SessionStore, SessionStoreEntry, new_session_store};
