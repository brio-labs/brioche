//! Redb storage implementation and persistence trait boundary.
//!
//! This module owns database construction and transaction orchestration. Helper
//! modules provide schema, serialization, loading, cache, and GC concerns.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1–3, I-Persist-SaveSession, I-Persist-PluginBlob

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use brioche_core::ChatMessage;
use brioche_shell_runtime::{Persistence, ShellError};
use redb::{Database, ReadableDatabase, ReadableTable};

use super::{
    BLOBS_TABLE, GcRunner, MESSAGES_TABLE, PersistenceError, SESSIONS_TABLE, SessionStore,
    SessionStoreEntry, deserialize_head, deserialize_message, maybe_compress, maybe_decompress,
    serialize_head, serialize_message,
};
use crate::dto::SessionHeadDTO;

/// Redb-backed persistent storage.
///
/// Implements the `Persistence` trait from `brioche-shell-runtime` so it
/// can be plugged into `DefaultEffectExecutor`.
///
/// Clone is cheap (all fields are `Arc`-wrapped or `COPY`).
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone)]
pub struct RedbStorage {
    db: Arc<Database>,
    session_store: SessionStore,
    gc_runner: Arc<GcRunner>,
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
            gc_runner: Arc::new(GcRunner::new()),
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

    /// Load all messages for a session in index order, synchronously.
    ///
    /// This is the blocking counterpart of [`Self::load_messages_for_session`],
    /// intended for callers that run outside the async runtime (e.g. the
    /// synchronous `SubRoutineHydrator::hydrate` callback on the engine thread).
    ///
    /// # Errors
    /// Returns `PersistenceError::Redb` on database errors.
    /// Returns `PersistenceError::Serialization` on MessagePack decode failure.
    /// Returns `PersistenceError::Compression` on Zstd decompression failure.
    ///
    /// Refs: I-Shell-Load-Batch
    pub fn load_messages_for_session_sync(
        &self,
        session_id: &str,
    ) -> Result<Vec<(u32, ChatMessage)>, PersistenceError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(MESSAGES_TABLE)?;
        let range = table.iter()?;
        let mut results = Vec::new();
        for item in range {
            let (k, v) = item?;
            let (key_sid, idx) = k.value();
            if key_sid != session_id {
                continue;
            }
            let decompressed = maybe_decompress(v.value())?;
            let msg = deserialize_message(&decompressed)?;
            results.push((idx, msg));
        }
        results.sort_by_key(|(idx, _)| *idx);
        Ok(results)
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

        let head = entry.head.clone();
        let sid = session_id.to_string();
        let db = Arc::clone(&self.db);
        let delta = entry.messages[entry.head.persisted_msg_count as usize..].to_vec();
        let start_index = entry.head.persisted_msg_count as usize;

        // Serialize and commit head + delta messages in a single Redb
        // transaction so the on-disk state is always consistent.
        let new_count = tokio::task::spawn_blocking(move || {
            let head_blob = serialize_head(&head)?;
            let head_compressed = maybe_compress(head_blob)?;

            let mut message_batch = Vec::with_capacity(delta.len());
            for (offset, msg) in delta.iter().enumerate() {
                let index = (start_index + offset) as u32;
                let blob = serialize_message(msg)?;
                let compressed = maybe_compress(blob)?;
                message_batch.push(((sid.clone(), index), compressed));
            }

            let write_txn = db.begin_write()?;
            {
                let mut sessions = write_txn.open_table(SESSIONS_TABLE)?;
                sessions.insert(sid.as_str(), head_compressed.as_slice())?;

                let mut messages = write_txn.open_table(MESSAGES_TABLE)?;
                for ((session_id, idx), compressed) in message_batch {
                    messages.insert((session_id.as_str(), idx), compressed.as_slice())?;
                }
            }
            write_txn.commit()?;
            Ok::<_, PersistenceError>(entry.messages.len())
        })
        .await
        .map_err(|e| ShellError::EffectExecution(e.to_string()))?
        .map_err(|e| ShellError::EffectExecution(e.to_string()))?;

        // Advance the watermark only after the atomic commit succeeds.
        let mut store = self.session_store.write().await;
        if let Some(entry) = store.get_mut(session_id) {
            entry.head.persisted_msg_count = new_count as u64;
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

    /// Run opportunistic GC for the given session.
    ///
    /// Uses the session's `compaction_index` as the compaction watermark:
    /// messages with index strictly less than that value are safe to
    /// remove because they have already been folded into the head DTO.
    ///
    /// Refs: I-Persist-GC-Interrupt, I-Persist-SaveSession
    async fn gc(&self, session_id: &str) -> Result<u64, ShellError> {
        let compaction_index = {
            let store = self.session_store.read().await;
            store
                .get(session_id)
                .map(|entry| entry.head.compaction_index)
        };
        let Some(index) = compaction_index else {
            return Ok(0);
        };
        self.gc_runner
            .run_gc(self, session_id, index)
            .await
            .map_err(|e| ShellError::EffectExecution(e.to_string()))
    }
}
