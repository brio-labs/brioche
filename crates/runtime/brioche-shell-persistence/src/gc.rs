//! Opportunistic garbage collection for the persistence layer.
//!
//! Removes stale messages from `MESSAGES_TABLE` that fall below the
//! compaction watermark. Interruptible via `CancellationToken` so
//! new user input never waits on I/O locks.
//!
//! Refs: SPECS.md §Book III-B Ch 5, I-Persist-GC-Interrupt

use crate::{error::PersistenceError, schema::MESSAGES_TABLE, storage::RedbStorage};
use redb::ReadableTable;
use tokio_util::sync::CancellationToken;

/// Runner for opportunistic garbage collection.
///
/// Holds a `CancellationToken` that can be triggered by the shell
/// when new user input arrives, causing the GC scan to abort early
/// and release the Redb write transaction.
///
/// # Usage
/// ```ignore
/// let gc = GcRunner::new();
/// let handle = tokio::spawn({
///     let gc = gc.clone();
///     async move {
///         gc.run_gc(&storage, "session-1", 100).await
///     }
/// });
/// // User sends message:
/// gc.cancel();
/// ```
///
/// Refs: I-Persist-GC-Interrupt
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
