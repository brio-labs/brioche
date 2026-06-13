//! Redb-backed storage engine implementing the `Persistence` trait.
//!
//! `RedbStorage` bridges the synchronous engine thread and the async
//! runtime via a shared `SessionStore`. The engine thread populates the
//! store with `SessionStoreEntry` (head DTO + full message history); the
//! async `save_session` effect reads from this store and flushes to Redb.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1–3, I-Persist-SaveSession, I-Persist-PluginBlob

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use brioche_core::ChatMessage;
use brioche_shell_runtime::{Persistence, ShellError};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use tokio::sync::RwLock;

use crate::dto::SessionHeadDTO;

// ---------------------------------------------------------------------------
// Schema (merged from schema.rs)
// ---------------------------------------------------------------------------

/// Session head table. Key = session ID, Value = MessagePack blob.
///
/// Overwritten at each `SaveSession` effect.
pub const SESSIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");

/// Message history table. Key = (session_id, message_index), Value = MessagePack blob.
///
/// Append-only: existing entries are never updated, only new indices inserted.
pub const MESSAGES_TABLE: TableDefinition<(&str, u32), &[u8]> = TableDefinition::new("messages");

/// Cold plugin blob table. Key = plugin_id, Value = raw binary blob.
///
/// Written asynchronously by `SavePluginBlob` without blocking the engine.
pub const BLOBS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("blobs");

// ---------------------------------------------------------------------------
// Error types (merged from error.rs)
// ---------------------------------------------------------------------------

/// Errors originating in the shell persistence layer.
///
/// All operations return `Result<T, PersistenceError>`; panics are
/// prohibited by clippy and philosophy.
///
/// Refs: I-Core-NoPanic
#[derive(Debug, thiserror::Error)]
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

// ---------------------------------------------------------------------------
// Save helpers (merged from save.rs)
// ---------------------------------------------------------------------------

use brioche_core::Session;

/// Size threshold above which a message or session head blob is Zstd-compressed.
///
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
pub const COMPRESSION_THRESHOLD: usize = 1024;

/// Compression flag prefix: `0x00` = uncompressed, `0x01` = zstd-compressed.
const FLAG_UNCOMPRESSED: u8 = 0x00;
const FLAG_COMPRESSED: u8 = 0x01;

/// Extract the delta slice of messages not yet persisted.
///
/// Returns messages from index `persisted_msg_count` onwards.
///
/// Complexity: O(1) slice creation; no allocation.
///
/// Refs: docs/SPECS.md §Book III-B Ch 3.1
pub fn extract_delta(session: &Session) -> &[ChatMessage] {
    &session.history[session.persisted_msg_count..]
}

/// Serialize a `SessionHeadDTO` to MessagePack.
///
/// Complexity: O(serialization cost). Allocates one `Vec`.
/// Refs: docs/SPECS.md §Book III-A
pub fn serialize_head(dto: &SessionHeadDTO) -> Result<Vec<u8>, PersistenceError> {
    rmp_serde::to_vec(dto).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a `SessionHeadDTO` from a MessagePack blob.
/// Refs: docs/SPECS.md §Book III-A
pub fn deserialize_head(blob: &[u8]) -> Result<SessionHeadDTO, PersistenceError> {
    rmp_serde::from_slice(blob).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Serialize a single `ChatMessage` to MessagePack.
/// Refs: docs/SPECS.md §Book III-A
pub fn serialize_message(msg: &ChatMessage) -> Result<Vec<u8>, PersistenceError> {
    rmp_serde::to_vec(msg).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a single `ChatMessage` from a MessagePack blob.
/// Refs: docs/SPECS.md §Book III-A
pub fn deserialize_message(blob: &[u8]) -> Result<ChatMessage, PersistenceError> {
    rmp_serde::from_slice(blob).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Compress a payload with Zstd if it exceeds the threshold.
///
/// A one-byte flag prefix indicates whether compression was applied:
/// - `0x00` followed by raw data (uncompressed)
/// - `0x01` followed by zstd-compressed data
///
/// This makes decompression deterministic and branch-free.
///
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
pub fn maybe_compress(data: Vec<u8>) -> Result<Vec<u8>, PersistenceError> {
    if data.len() > COMPRESSION_THRESHOLD {
        let compressed = zstd::encode_all(data.as_slice(), 3)
            .map_err(|e| PersistenceError::Compression(e.to_string()))?;
        let mut result = Vec::with_capacity(1 + compressed.len());
        result.push(FLAG_COMPRESSED);
        result.extend_from_slice(&compressed);
        Ok(result)
    } else {
        let mut result = Vec::with_capacity(1 + data.len());
        result.push(FLAG_UNCOMPRESSED);
        result.extend_from_slice(&data);
        Ok(result)
    }
}

/// Decompress a payload based on the leading flag byte.
///
/// Falls back to returning the raw slice if no flag is recognized
/// (backward compatibility for legacy data written without flag).
///
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
pub fn maybe_decompress(data: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    match data.first() {
        Some(&FLAG_COMPRESSED) => {
            zstd::decode_all(&data[1..]).map_err(|e| PersistenceError::Compression(e.to_string()))
        }
        Some(&FLAG_UNCOMPRESSED) => Ok(data[1..].to_vec()),
        _ => Ok(data.to_vec()),
    }
}

/// Shared in-memory store bridging the engine thread and async persistence.
///
/// The engine thread (which owns the `!Send` `Session`) inserts entries
/// after each transition. The async `RedbStorage` reads from this map
/// when processing `SaveSession` effects.
///
/// Refs: I-Shell-Session-NoSend
pub type SessionStore = Arc<RwLock<BTreeMap<String, SessionStoreEntry>>>;

/// Create a new empty `SessionStore`.
/// Refs: docs/SPECS.md §Book III-A
pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(BTreeMap::new()))
}

/// A single entry in the `SessionStore`, holding both the head DTO and
/// the full message history needed for delta persistence.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct SessionStoreEntry {
    /// Flattened session head.
    pub head: SessionHeadDTO,
    /// Complete message history (including already-persisted messages).
    pub messages: Vec<ChatMessage>,
}

/// Redb-backed persistent storage.
///
/// Implements the `Persistence` trait from `brioche-shell-runtime` so it
/// can be plugged into `DefaultEffectExecutor`.
///
/// Clone is cheap (all fields are `Arc`-wrapped or `Copy`).
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone)]
pub struct RedbStorage {
    db: Arc<Database>,
    session_store: SessionStore,
}

impl RedbStorage {
    /// Open or create a Redb database at the given path.
    ///
    /// `session_store` must be shared with the code that populates it
    /// from the engine thread (typically via `update_session`).
    ///
    /// Complexity: O(1). File creation is deferred to first write.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(
        path: impl AsRef<Path>,
        session_store: SessionStore,
    ) -> Result<Self, PersistenceError> {
        let db = Database::create(path.as_ref())?;
        Ok(Self {
            db: Arc::new(db),
            session_store,
        })
    }

    /// Access the underlying Redb `Arc<Database>`.
    ///
    /// Used by crate-internal modules (`gc`, `load`) that need direct
    /// table access.
    ///
    /// Complexity: O(1). Clones an `Arc`.
    /// Refs: docs/SPECS.md §Book III-A
    pub(crate) fn db(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }

    /// Update the in-memory store with a new session entry.
    ///
    /// Call this from the engine thread (or an async bridge) after each
    /// `transition()` so that `save_session` has fresh data to flush.
    ///
    /// # Cancel safety
    /// This future holds an `RwLock` write guard across a single
    /// non-awaiting statement. Dropping it before await completion
    /// leaves the store unchanged.
    ///
    /// Complexity: O(log n) where n = number of tracked sessions.
    pub async fn update_session(&self, entry: SessionStoreEntry) {
        let mut store = self.session_store.write().await;
        store.insert(entry.head.id.clone(), entry);
    }

    /// Persist a session head DTO directly (bypassing the store).
    ///
    /// Used by tests and by `save_session` after reading from the store.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` task. Dropping it leaks the
    /// background write; the on-disk state is consistent because Redb
    /// commits inside the blocking task regardless of the awaited future.
    ///
    /// Complexity: O(serialization + I/O). Executed on `spawn_blocking`.
    pub async fn save_session_dto(&self, dto: &SessionHeadDTO) -> Result<(), PersistenceError> {
        let blob = serialize_head(dto)?;
        let compressed = maybe_compress(blob)?;
        let id = dto.id.clone();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(SESSIONS_TABLE)?;
                table.insert(id.as_str(), compressed.as_slice())?;
            }
            write_txn.commit()?;
            Ok::<_, PersistenceError>(())
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))??;

        Ok(())
    }

    /// Load a session head DTO from Redb.
    ///
    /// Returns `Ok(None)` if the session ID is not found.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` read task. Dropping it
    /// discards the result; the database is not modified.
    ///
    /// Complexity: O(I/O). Executed on `spawn_blocking`.
    pub async fn load_session(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionHeadDTO>, PersistenceError> {
        let id = session_id.to_string();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(SESSIONS_TABLE)?;
            let Some(guard) = table.get(id.as_str())? else {
                return Ok(None);
            };
            let bytes = guard.value();
            let decompressed = maybe_decompress(bytes)?;
            let dto = deserialize_head(&decompressed)?;
            Ok(Some(dto))
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))?
    }

    /// Persist a slice of messages starting at `start_index`.
    ///
    /// Each message is serialized to MessagePack and written to
    /// `MESSAGES_TABLE` with the composite key `(session_id, index)`.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` write task. Dropping it may
    /// leave the write running in the background; the on-disk state is
    /// consistent because Redb commits inside the blocking task.
    ///
    /// Complexity: O(m * (serialization + I/O)) where m = messages.len().
    pub async fn save_messages(
        &self,
        session_id: &str,
        messages: &[ChatMessage],
        start_index: usize,
    ) -> Result<(), PersistenceError> {
        let mut batch: Vec<((String, u32), Vec<u8>)> = Vec::with_capacity(messages.len());
        for (offset, msg) in messages.iter().enumerate() {
            let index = (start_index + offset) as u32;
            let blob = serialize_message(msg)?;
            let compressed = maybe_compress(blob)?;
            batch.push(((session_id.to_string(), index), compressed));
        }

        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(MESSAGES_TABLE)?;
                for ((sid, idx), compressed) in batch {
                    table.insert((sid.as_str(), idx), compressed.as_slice())?;
                }
            }
            write_txn.commit()?;
            Ok::<_, PersistenceError>(())
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))??;

        Ok(())
    }

    /// Load a single message by session ID and index.
    ///
    /// Returns `Ok(None)` if the key is not found.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` read task. Dropping it
    /// discards the result; the database is not modified.
    pub async fn load_message(
        &self,
        session_id: &str,
        index: u32,
    ) -> Result<Option<ChatMessage>, PersistenceError> {
        let sid = session_id.to_string();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(MESSAGES_TABLE)?;
            let Some(guard) = table.get((sid.as_str(), index))? else {
                return Ok(None);
            };
            let bytes = guard.value();
            let decompressed = maybe_decompress(bytes)?;
            let msg = deserialize_message(&decompressed)?;
            Ok(Some(msg))
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))?
    }

    /// Load all messages for a session in index order.
    ///
    /// Iterates the `MESSAGES_TABLE` range for the given session ID.
    /// Returns messages sorted by index.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` read task. Dropping it
    /// discards the result; the database is not modified.
    ///
    /// Refs: I-Shell-Load-Batch (preparation for Sprint 13).
    pub async fn load_messages_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<(u32, ChatMessage)>, PersistenceError> {
        let sid = session_id.to_string();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(MESSAGES_TABLE)?;
            let range = table.iter()?;
            let mut results = Vec::new();
            for item in range {
                let (k, v) = item?;
                let (key_sid, idx) = k.value();
                if key_sid != sid.as_str() {
                    continue;
                }
                let decompressed = maybe_decompress(v.value())?;
                let msg = deserialize_message(&decompressed)?;
                results.push((idx, msg));
            }
            // Deterministic ordering: sort by index.
            results.sort_by_key(|(idx, _)| *idx);
            Ok::<_, PersistenceError>(results)
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))?
    }

    /// Delete all messages for a session with index strictly less than
    /// `compaction_index`.
    ///
    /// Returns the number of removed message entries.
    ///
    /// # Cancel safety
    /// This future awaits a `spawn_blocking` write task. Dropping it may
    /// leave the deletion running in the background; the on-disk state is
    /// consistent because Redb commits inside the blocking task.
    ///
    /// Complexity: O(m) where m = messages scanned. One write transaction.
    ///
    /// Refs: I-Persist-GC-Interrupt
    pub async fn delete_messages_below(
        &self,
        session_id: &str,
        compaction_index: u32,
    ) -> Result<u64, PersistenceError> {
        let sid = session_id.to_string();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            let count = {
                let mut table = write_txn.open_table(MESSAGES_TABLE)?;
                let range = table.iter()?;
                let mut to_remove = Vec::new();

                for item in range {
                    let (k, _v) = item?;
                    let (key_sid, idx) = k.value();
                    if key_sid == sid.as_str() && idx < compaction_index {
                        to_remove.push((key_sid.to_string(), idx));
                    }
                }

                let count = to_remove.len() as u64;
                for (key_sid, idx) in to_remove {
                    table.remove(&(key_sid.as_str(), idx))?;
                }
                count
            };

            write_txn.commit()?;
            Ok::<_, PersistenceError>(count)
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))?
    }
}

#[async_trait]
impl Persistence for RedbStorage {
    /// Flush the session head and message delta to Redb.
    ///
    /// Reads the current entry from the shared `SessionStore`, serializes
    /// the head DTO to MessagePack, extracts the message delta, and writes
    /// both to Redb in separate table transactions.
    ///
    /// After a successful flush, `persisted_msg_count` in the store is
    /// updated so the next save only writes new messages.
    ///
    /// Refs: I-Persist-SaveSession, I-Persist-AppendOnly
    async fn save_session(&self, session_id: &str) -> Result<(), ShellError> {
        let entry = {
            let store = self.session_store.read().await;
            store.get(session_id).cloned().ok_or_else(|| {
                ShellError::EffectExecution(format!("session {} not found in store", session_id))
            })?
        };

        // 1. Persist the head DTO.
        self.save_session_dto(&entry.head)
            .await
            .map_err(|e| ShellError::EffectExecution(e.to_string()))?;

        // 2. Persist delta messages.
        let delta = &entry.messages[entry.head.persisted_msg_count..];
        if !delta.is_empty() {
            self.save_messages(session_id, delta, entry.head.persisted_msg_count)
                .await
                .map_err(|e| ShellError::EffectExecution(e.to_string()))?;
        }

        // 3. Advance the watermark in the store.
        let mut store = self.session_store.write().await;
        if let Some(entry) = store.get_mut(session_id) {
            entry.head.persisted_msg_count = entry.messages.len();
        }

        Ok(())
    }

    /// Persist a cold plugin blob asynchronously.
    ///
    /// Writes the raw blob to `BLOBS_TABLE` keyed by `plugin_id`.
    /// The operation is executed on `spawn_blocking` so the engine thread
    /// is never blocked.
    ///
    /// Refs: I-Persist-PluginBlob
    async fn save_plugin_blob(&self, plugin_id: &str, data: Vec<u8>) -> Result<(), ShellError> {
        let pid = plugin_id.to_string();
        let db = Arc::clone(&self.db);

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(BLOBS_TABLE)?;
                table.insert(pid.as_str(), data.as_slice())?;
            }
            write_txn.commit()?;
            Ok::<_, PersistenceError>(())
        })
        .await
        .map_err(|e| ShellError::EffectExecution(e.to_string()))?
        .map_err(|e| ShellError::EffectExecution(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// GC runner (merged from gc.rs)
// ---------------------------------------------------------------------------

use tokio_util::sync::CancellationToken;

/// Runner for opportunistic garbage collection.
///
/// Removes stale messages from `MESSAGES_TABLE` that fall below the
/// compaction watermark. Interruptible via `CancellationToken` so
/// new user input never waits on I/O locks.
///
/// Refs: docs/SPECS.md §Book III-B Ch 5, I-Persist-GC-Interrupt
#[derive(Clone, Debug)]
pub struct GcRunner {
    cancel: CancellationToken,
}

impl Default for GcRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl GcRunner {
    /// Create a new GC runner with a fresh cancellation token.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            cancel: CancellationToken::new(),
        }
    }

    /// Request cancellation of any in-progress GC operation.
    ///
    /// This is safe to call from any thread; it is non-blocking.
    /// The next iteration of the GC scan will observe the cancellation
    /// and break out of the loop, committing whatever deletions have
    /// already been staged.
    ///
    /// Refs: I-Persist-GC-Interrupt
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Access the underlying cancellation token.
    ///
    /// Used when integrating with `tokio::select!` in the shell.
    ///
    /// Refs: I-Persist-GC-Interrupt
    pub fn token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Run GC on a single session: remove all messages with index
    /// strictly less than `compaction_index`.
    ///
    /// The algorithm:
    /// 1. Opens a write transaction.
    /// 2. Scans `MESSAGES_TABLE`.
    /// 3. Collects keys matching `session_id` with `idx < compaction_index`.
    /// 4. Checks the cancellation token between each scanned entry.
    /// 5. Removes collected keys.
    /// 6. Commits the transaction (partial deletions are safe).
    ///
    /// Returns the count of removed message entries.
    ///
    /// Complexity: O(m) where m = messages in table. One transaction.
    ///
    /// # Errors
    /// Returns `PersistenceError::Redb` on database errors.
    /// Returns `PersistenceError::Compression` if the background task panics.
    ///
    /// Refs: I-Persist-GC-Interrupt
    pub async fn run_gc(
        &self,
        storage: &RedbStorage,
        session_id: &str,
        compaction_index: u32,
    ) -> Result<u64, PersistenceError> {
        let db = storage.db();
        let sid = session_id.to_string();
        let cancel = self.cancel.clone();

        tokio::task::spawn_blocking(move || {
            let write_txn = db.begin_write()?;
            let count = {
                let mut table = write_txn.open_table(MESSAGES_TABLE)?;
                let range = table.iter()?;
                let mut to_remove = Vec::new();

                for item in range {
                    // Check cancellation between each scanned entry.
                    // This guarantees the I/O lock is released quickly
                    // when new user input arrives.
                    if cancel.is_cancelled() {
                        break;
                    }
                    let (k, _v) = item?;
                    let (key_sid, idx) = k.value();
                    if key_sid == sid.as_str() && idx < compaction_index {
                        to_remove.push((key_sid.to_string(), idx));
                    }
                }

                let count = to_remove.len() as u64;
                for (key_sid, idx) in to_remove {
                    table.remove(&(key_sid.as_str(), idx))?;
                }
                count
            };

            // Commit even if partially done — partial GC is safe
            // because messages below the compaction index are by
            // definition no longer needed for replay.
            write_txn.commit()?;
            Ok::<_, PersistenceError>(count)
        })
        .await
        .map_err(|e| PersistenceError::Compression(e.to_string()))?
    }
}

// ---------------------------------------------------------------------------
// Load helpers (merged from load.rs)
// ---------------------------------------------------------------------------

/// Load a complete session (head + all messages) from Redb.
///
/// This is the inverse of the delta save protocol: it loads the head
/// DTO from `SESSIONS_TABLE` and all messages from `MESSAGES_TABLE`,
/// reconstructing the full session state needed for replay.
///
/// Returns `Ok(None)` if the session ID is not found.
///
/// # Cancel safety
/// This future awaits other async loaders. Dropping it discards the
/// result; cache and database remain unchanged.
///
/// Complexity: O(I/O + deserialization). Executed on `spawn_blocking`.
///
/// # Errors
/// Returns `PersistenceError::Redb` on database errors.
/// Returns `PersistenceError::Serialization` on MessagePack decode failure.
/// Returns `PersistenceError::Compression` on Zstd decompression failure
/// or if the background task panics.
///
/// Refs: I-Shell-Load-Batch
pub async fn load_session_full(
    storage: &RedbStorage,
    session_id: &str,
) -> Result<Option<(SessionHeadDTO, Vec<ChatMessage>)>, PersistenceError> {
    let head = storage.load_session(session_id).await?;
    let head = match head {
        Some(h) => h,
        None => return Ok(None),
    };

    let messages = storage.load_messages_for_session(session_id).await?;
    let msgs: Vec<ChatMessage> = messages.into_iter().map(|(_, msg)| msg).collect();

    Ok(Some((head, msgs)))
}

/// Load a sub-routine with cache fallback (L1 → L2 → Redb).
///
/// Checks the `SubRoutineCache` first, then falls back to Redb.
/// If loaded from Redb, the DTO is inserted into L2.
///
/// # Cancel safety
/// This future awaits `storage.load_session`. Dropping it discards the
/// result; cache and database remain unchanged.
///
/// Complexity: O(1) for L1/L2 hit; O(I/O) for Redb fallback.
///
/// Refs: I-Persist-Cache
pub async fn load_subroutine(
    storage: &RedbStorage,
    cache: &mut SubRoutineCache,
    handle: &str,
) -> Result<Option<SessionHeadDTO>, PersistenceError> {
    if let Some(dto) = cache.get(handle) {
        return Ok(Some(dto.clone()));
    }

    let dto = storage.load_session(handle).await?;
    if let Some(ref d) = dto {
        cache.insert(handle.to_string(), d.clone());
    }

    Ok(dto)
}

/// Lazy session loader that pre-fetches depth-1 children.
///
/// On construction, loads the root session head and all immediate
/// sub-routine children into the L2 LRU cache.
///
/// Refs: I-Shell-Load-Batch
pub struct LazySessionLoader<'a> {
    storage: &'a RedbStorage,
}

impl<'a> LazySessionLoader<'a> {
    /// Create a new lazy loader backed by the given storage.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(storage: &'a RedbStorage) -> Self {
        Self { storage }
    }

    /// Load a session and pre-fill its depth-1 sub-routines into L2.
    ///
    /// Returns `Ok(None)` if the session ID is not found in Redb.
    ///
    /// For every `FlattenedAgentState::SubRoutine(child_id)` found in
    /// the current state or state stack, the child head is loaded from
    /// Redb and inserted into `SubRoutineCache.l2_lru` (unless already
    /// present in L1 or L2).
    ///
    /// # Cancel safety
    /// This future awaits other async loaders. Dropping it discards the
    /// result; cache and database remain unchanged.
    ///
    /// Complexity: O(I/O + n * I/O) where n = number of sub-routine
    /// references in the state stack.
    ///
    /// Refs: I-Shell-Load-Batch
    pub async fn load(
        &self,
        session_id: &str,
        cache: &mut SubRoutineCache,
    ) -> Result<Option<(SessionHeadDTO, Vec<ChatMessage>)>, PersistenceError> {
        let result = load_session_full(self.storage, session_id).await?;
        let (head, messages) = match result {
            Some(r) => r,
            None => return Ok(None),
        };

        // Pre-fill depth-1 children into L2 cache.
        //
        // Rationale: when a user loads a session, the most likely next
        // interaction is opening a sub-routine accordion in the UI.
        // By pre-fetching depth-1 children now (while we already hold
        // the Redb read context), we amortize the I/O cost and avoid
        // a synchronous stall when the UI later requests the child.
        // Deeper levels are loaded on-demand via `load_subroutine`.
        for state in std::iter::once(&head.state).chain(head.state_stack.iter()) {
            if let crate::dto::FlattenedAgentState::SubRoutine(child_id) = state
                && !cache.contains(child_id)
                && let Some(child) = self.storage.load_session(child_id).await?
            {
                cache.insert(child_id.clone(), child);
            }
        }

        Ok(Some((head, messages)))
    }
}

// ---------------------------------------------------------------------------
// Sub-routine cache (merged from cache.rs)
// ---------------------------------------------------------------------------

use std::num::NonZeroUsize;

use lru::LruCache;

/// Two-level cache for sub-routine session heads.
///
/// - **L1 Visible**: sub-routines currently open in the UI (accordions).
///   These DTOs are never evicted by LRU policy.
/// - **L2 LRU**: recently used sub-routines managed by LRU eviction.
///
/// `BTreeMap` is used for L1 to uphold deterministic ordering.
///
/// Refs: docs/SPECS.md §Book III-B Ch 2.1
pub struct SubRoutineCache {
    /// UI-visible sub-routines (never evicted).
    l1_visible: BTreeMap<String, SessionHeadDTO>,
    /// Recently used sub-routines (LRU eviction).
    l2_lru: LruCache<String, SessionHeadDTO>,
}

impl SubRoutineCache {
    /// Create a new cache with the given L2 capacity.
    ///
    /// L1 capacity is unbounded (managed explicitly by UI open/close).
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new(l2_capacity: NonZeroUsize) -> Self {
        Self {
            l1_visible: BTreeMap::new(),
            l2_lru: LruCache::new(l2_capacity),
        }
    }

    /// Look up a sub-routine by ID.
    ///
    /// Checks L1 first, then L2. L2 lookups do **not** promote the entry
    /// (use `promote_to_l1` for explicit promotion).
    ///
    /// Complexity: O(log n) for L1, O(1) for L2.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn get(&self, id: &str) -> Option<&SessionHeadDTO> {
        self.l1_visible.get(id).or_else(|| self.l2_lru.peek(id))
    }

    /// Move a sub-routine from L2 to L1 (UI opened the accordion).
    ///
    /// Returns the previously held DTO if the ID was already in L1.
    ///
    /// Complexity: O(log n) for L1 insertion + O(1) for L2 removal.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn promote_to_l1(&mut self, id: String) -> Option<SessionHeadDTO> {
        if let Some(dto) = self.l2_lru.pop(&id) {
            self.l1_visible.insert(id, dto)
        } else {
            None
        }
    }

    /// Move a sub-routine from L1 to L2 (UI closed the accordion).
    ///
    /// Returns the DTO if it was not in L1 (already evicted or never present).
    ///
    /// Complexity: O(log n) for L1 removal + O(1) for L2 insertion.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn demote_to_l2(&mut self, id: String) {
        if let Some(dto) = self.l1_visible.remove(&id) {
            self.l2_lru.put(id, dto);
        }
    }

    /// Insert a sub-routine directly into L2.
    ///
    /// Used when loading from Redb on demand.
    ///
    /// Complexity: O(1).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn insert(&mut self, id: String, dto: SessionHeadDTO) {
        self.l2_lru.put(id, dto);
    }

    /// Returns `true` if the ID is present in either tier.
    ///
    /// Complexity: O(log n) for L1 + O(1) for L2.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn contains(&self, id: &str) -> bool {
        self.l1_visible.contains_key(id) || self.l2_lru.contains(id)
    }

    /// Remove a sub-routine from both tiers.
    ///
    /// Returns the removed DTO if present.
    ///
    /// Complexity: O(log n) for L1 + O(1) for L2.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn remove(&mut self, id: &str) -> Option<SessionHeadDTO> {
        self.l1_visible.remove(id).or_else(|| self.l2_lru.pop(id))
    }

    /// Number of entries currently in L1.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn l1_len(&self) -> usize {
        self.l1_visible.len()
    }

    /// Number of entries currently in L2.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn l2_len(&self) -> usize {
        self.l2_lru.len()
    }
}
