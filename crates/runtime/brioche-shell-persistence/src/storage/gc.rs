//! Opportunistic Redb message garbage collection.
//!
//! GC is kept separate from the storage trait implementation because it owns a
//! cancellation token and a distinct compaction-watermark scan contract.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 5, I-Persist-GC-Interrupt

use std::sync::Arc;

use redb::ReadableTable;
use tokio_util::sync::CancellationToken;

use super::{MESSAGES_TABLE, PersistenceError, RedbStorage};

/// Runner for opportunistic garbage collection.
///
/// Removes stale messages from `MESSAGES_TABLE` that fall below the
/// compaction watermark. Interruptible via `CancellationToken` so
/// new user input never waits on I/O locks.
///
/// Refs: docs/SPECS.md §Book III-B Ch 5, I-Persist-GC-Interrupt
///
/// The token is stored behind a mutex so cancellation can be requested
/// through a shared reference and a fresh token is installed for the
/// next run.
#[derive(Clone, Debug)]
pub struct GcRunner {
    cancel: Arc<std::sync::Mutex<CancellationToken>>,
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
            cancel: Arc::new(std::sync::Mutex::new(CancellationToken::new())),
        }
    }

    /// Request cancellation of any in-progress GC operation.
    ///
    /// This is safe to call from any thread; it is non-blocking.
    /// The next iteration of the GC scan will observe the cancellation
    /// and break out of the loop, committing whatever deletions have
    /// already been staged. A fresh token is installed so future runs
    /// are not pre-cancelled.
    ///
    /// Refs: I-Persist-GC-Interrupt
    ///
    /// # Complexity
    /// O(1). One mutex lock and one `CancellationToken` replacement.
    ///
    /// # Panics
    /// Never panics. Poisoned mutexes are handled gracefully.
    pub fn cancel(&self) {
        let mut guard = match self.cancel.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let old = std::mem::replace(&mut *guard, CancellationToken::new());
        old.cancel();
    }

    /// Access a clone of the current cancellation token.
    ///
    /// Used when integrating with `tokio::select!` in the shell.
    ///
    /// Refs: I-Persist-GC-Interrupt
    ///
    /// # Complexity
    /// O(1). One mutex lock and one `CancellationToken` clone.
    ///
    /// # Panics
    /// Never panics. Poisoned mutexes are handled gracefully.
    pub fn token(&self) -> CancellationToken {
        match self.cancel.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
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
        let cancel = self.token();

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
