//! Redb-backed storage engine implementing the `Persistence` trait.
//!
//! `RedbStorage` bridges the synchronous engine thread and the async runtime via
//! a shared `SessionStore`. The storage modules are split by persistence
//! protocol concern: schema/errors, deterministic codec helpers, Redb
//! transactions, loading, GC, and sub-routine cache policy. This follows
//! `docs/PHILOSOPHY.md` module cohesion rules without one-file-per-type shards.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1–3, I-Persist-SaveSession, I-Persist-PluginBlob

mod cache;
mod codec;
mod error;
mod gc;
mod load;
mod redb;
mod schema;

pub use cache::SubRoutineCache;
pub use codec::{
    COMPRESSION_THRESHOLD, SessionStore, SessionStoreEntry, deserialize_head, deserialize_message,
    extract_delta, maybe_compress, maybe_decompress, new_session_store, serialize_head,
    serialize_message,
};
pub use error::PersistenceError;
pub use gc::GcRunner;
pub use load::{LazySessionLoader, load_session_full, load_subroutine};
pub use redb::RedbStorage;
pub use schema::{BLOBS_TABLE, MESSAGES_TABLE, SESSIONS_TABLE};
