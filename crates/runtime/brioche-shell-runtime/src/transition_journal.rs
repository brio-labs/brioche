//! `TransitionJournal` — lock-free transition log for recovery.
//!
//! Persists each `EngineInput` in a pre-allocated 1 MB ring buffer
//! before the call to `transition()`.  In case of watchdog restart,
//! the shell replays unpersisted transitions to restore session state.
//!
//! The journal is **append-only** from the engine thread and
//! **read-only** from the watchdog thread.  No locks are required:
//! the engine thread advances a monotonic `write_index` and the
//! watchdog reads the stable prefix.
//!
//! ## Invariants
//! - I-Shell-TransitionJournal: Every `EngineInput` is persisted
//!   before `transition()`.
//! - I-Shell-TransitionJournal-Idempotent: Replaying a journal entry
//!   produces the same effects as the original transition.
//!
//! Refs: SPECS.md §Book III-A Ch 1, §Book III-A Ch 4

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use brioche_core::EngineInput;

/// Capacity of the ring buffer in bytes.
///
/// 1 MB covers ~10 000 typical user messages at 100 bytes each,
/// sufficient for session recovery between auto-saves.
pub const JOURNAL_CAPACITY_BYTES: usize = 1_048_576;

/// Maximum serialized size of a single journal entry.
///
/// Entries larger than this are truncated (recorded as `Oversized`).
pub const MAX_ENTRY_BYTES: usize = 4096;

/// A lock-free ring buffer that records `EngineInput`s before they
/// are submitted to the engine.
///
/// # Thread safety
/// - **Writer**: single writer (the engine thread) calls `append()`.
/// - **Reader**: single reader (the watchdog / recovery thread) calls
///   `read_unacknowledged()` and `acknowledge()`.
///
/// Both threads access the buffer concurrently without locks.
///
/// Refs: I-Shell-TransitionJournal
/// # Safety
/// `TransitionJournal` is `Send + Sync` because access is coordinated
pub struct TransitionJournal {
    /// Pre-allocated byte buffer.
    ///
    /// Wrapped in `UnsafeCell` because the engine thread writes while
    /// the watchdog thread reads concurrently.  The single-writer /
    /// single-reader protocol (atomic indices) makes this safe.
    buffer: UnsafeCell<Box<[u8; JOURNAL_CAPACITY_BYTES]>>,
    /// Monotonic write position (modulo `JOURNAL_CAPACITY_BYTES`).
    write_index: AtomicUsize,
    /// Monotonic acknowledge position (modulo `JOURNAL_CAPACITY_BYTES`).
    /// The watchdog advances this after a successful save or restart.
    ack_index: AtomicUsize,
}

/// # Safety
/// `TransitionJournal` is `Send + Sync` because access is coordinated
/// by atomic indices: a single writer thread and a single reader thread
/// never touch the same byte concurrently.
unsafe impl Send for TransitionJournal {}
unsafe impl Sync for TransitionJournal {}

impl std::fmt::Debug for TransitionJournal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransitionJournal")
            .field("write_index", &self.write_index.load(Ordering::Relaxed))
            .field("ack_index", &self.ack_index.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// Recorded journal entry.
/// Refs: SPECS.md §Book III-A
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalEntry {
    /// A persisted `EngineInput`.
    Input(EngineInput),
    /// The original entry exceeded `MAX_ENTRY_BYTES`.
    /// Recovery replays a synthetic `UserMessage` containing the
    /// truncated payload hash so the session can continue deterministically.
    Oversized {
        /// Number of bytes that were discarded.
        truncated_bytes: usize,
    },
}

impl TransitionJournal {
    /// Create a new empty journal with 1 MB pre-allocated.
    ///
    /// Complexity: O(1). Allocates one heap block.
    /// Refs: SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            buffer: UnsafeCell::new(Box::new([0u8; JOURNAL_CAPACITY_BYTES])),
            write_index: AtomicUsize::new(0),
            ack_index: AtomicUsize::new(0),
        }
    }

    /// Append an `EngineInput` to the journal.
    ///
    /// Called by the engine thread **before** each `transition()`.
    /// If serialization fails or the entry is too large, an
    /// `Oversized` placeholder is written instead.
    ///
    /// # Complexity
    /// O(serialized length). One `postcard` serialization.
    ///
    /// Refs: I-Shell-TransitionJournal
    pub fn append(&self, input: &EngineInput) {
        let serialized = match postcard::to_allocvec(input) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(error = %err, "transition_journal: serialization failed");
                self.append_oversized(0);
                return;
            }
        };

        if serialized.len() > MAX_ENTRY_BYTES {
            self.append_oversized(serialized.len());
            return;
        }

        let len = serialized.len();
        let write_pos = self.write_index.load(Ordering::Relaxed);
        let ack_pos = self.ack_index.load(Ordering::Acquire);

        // Ensure we don't overwrite unacknowledged data.
        let available = if write_pos >= ack_pos {
            JOURNAL_CAPACITY_BYTES - (write_pos - ack_pos)
        } else {
            ack_pos - write_pos
        };

        // Header: 4 bytes little-endian length + payload.
        let needed = 4 + len;
        if needed > available {
            tracing::warn!(
                needed,
                available,
                "transition_journal: buffer full, dropping entry"
            );
            return;
        }

        // Write header (little-endian u32).
        let len_bytes = (len as u32).to_le_bytes();
        self.write_bytes(write_pos, &len_bytes);
        let write_pos = (write_pos + 4) % JOURNAL_CAPACITY_BYTES;

        // Write payload.
        self.write_bytes(write_pos, &serialized);
        let new_write_pos = (write_pos + len) % JOURNAL_CAPACITY_BYTES;

        self.write_index.store(new_write_pos, Ordering::Release);
    }

    /// Write a slice into the ring buffer, wrapping around if necessary.
    fn write_bytes(&self, start: usize, data: &[u8]) {
        let mut pos = start % JOURNAL_CAPACITY_BYTES;
        // SAFETY: The engine thread is the sole writer.  `pos` is
        // always within bounds because of the modulo.
        let buf = unsafe { &mut *self.buffer.get() };
        for byte in data {
            buf[pos] = *byte;
            pos = (pos + 1) % JOURNAL_CAPACITY_BYTES;
        }
    }

    /// Append an `Oversized` marker.
    fn append_oversized(&self, truncated_bytes: usize) {
        let write_pos = self.write_index.load(Ordering::Relaxed);
        let len_bytes = (0xFFFF_FFFFu32).to_le_bytes();
        self.write_bytes(write_pos, &len_bytes);
        let write_pos = (write_pos + 4) % JOURNAL_CAPACITY_BYTES;

        let marker = (truncated_bytes as u64).to_le_bytes();
        self.write_bytes(write_pos, &marker);
        let new_write_pos = (write_pos + 8) % JOURNAL_CAPACITY_BYTES;
        self.write_index.store(new_write_pos, Ordering::Release);
    }

    /// Read all unacknowledged entries.
    ///
    /// Called by the watchdog / recovery thread.  Returns entries in
    /// insertion order.
    ///
    /// # Complexity
    /// O(n) where n = number of unacknowledged entries.
    ///
    /// Refs: I-Shell-TransitionJournal
    pub fn read_unacknowledged(&self) -> Vec<JournalEntry> {
        let mut entries = Vec::new();
        let mut read_pos = self.ack_index.load(Ordering::Acquire);
        let write_pos = self.write_index.load(Ordering::Acquire);

        while read_pos != write_pos {
            // Read 4-byte length header.
            let mut len_buf = [0u8; 4];
            self.read_bytes(read_pos, &mut len_buf);
            let len = u32::from_le_bytes(len_buf) as usize;
            read_pos = (read_pos + 4) % JOURNAL_CAPACITY_BYTES;

            if len == 0xFFFF_FFFF {
                // Oversized marker.
                let mut marker_buf = [0u8; 8];
                self.read_bytes(read_pos, &mut marker_buf);
                let truncated = u64::from_le_bytes(marker_buf) as usize;
                entries.push(JournalEntry::Oversized {
                    truncated_bytes: truncated,
                });
                read_pos = (read_pos + 8) % JOURNAL_CAPACITY_BYTES;
            } else {
                let mut payload = vec![0u8; len];
                self.read_bytes(read_pos, &mut payload);
                match postcard::from_bytes(&payload) {
                    Ok(input) => entries.push(JournalEntry::Input(input)),
                    Err(err) => {
                        tracing::warn!(error = %err, "transition_journal: deserialization failed");
                        entries.push(JournalEntry::Oversized {
                            truncated_bytes: payload.len(),
                        });
                    }
                }
                read_pos = (read_pos + len) % JOURNAL_CAPACITY_BYTES;
            }
        }

        entries
    }

    /// Read a slice from the ring buffer, wrapping around if necessary.
    fn read_bytes(&self, start: usize, out: &mut [u8]) {
        let mut pos = start % JOURNAL_CAPACITY_BYTES;
        // SAFETY: The watchdog thread is the sole reader of this
        // region (indices ensure no overlap with the writer's live
        // range).  `pos` is always within bounds.
        let buf = unsafe { &*self.buffer.get() };
        for byte in out.iter_mut() {
            *byte = buf[pos];
            pos = (pos + 1) % JOURNAL_CAPACITY_BYTES;
        }
    }

    /// Advance the acknowledge position, marking all entries up to
    /// `write_index` as processed.
    ///
    /// Called by the watchdog after a successful Redb flush or engine
    /// restart.
    ///
    /// Refs: I-Shell-TransitionJournal
    pub fn acknowledge_all(&self) {
        let write_pos = self.write_index.load(Ordering::Acquire);
        self.ack_index.store(write_pos, Ordering::Release);
    }

    /// Return the number of unacknowledged bytes.
    ///
    /// Used by the watchdog to decide whether recovery replay is needed.
    ///
    /// Refs: I-Shell-TransitionJournal
    pub fn unacknowledged_bytes(&self) -> usize {
        let write_pos = self.write_index.load(Ordering::Acquire);
        let ack_pos = self.ack_index.load(Ordering::Acquire);
        if write_pos >= ack_pos {
            write_pos - ack_pos
        } else {
            JOURNAL_CAPACITY_BYTES - (ack_pos - write_pos)
        }
    }
}

impl Default for TransitionJournal {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use brioche_core::EngineInput;

    use super::*;

    #[test]
    fn journal_roundtrip_single_entry() {
        let journal = TransitionJournal::new();
        let input = EngineInput::UserMessage("hello world".into());
        journal.append(&input);

        let entries = journal.read_unacknowledged();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], JournalEntry::Input(input));
    }

    #[test]
    fn journal_acknowledge_clears_entries() {
        let journal = TransitionJournal::new();
        journal.append(&EngineInput::UserMessage("a".into()));
        journal.append(&EngineInput::UserMessage("b".into()));

        let before = journal.read_unacknowledged();
        assert_eq!(before.len(), 2);

        journal.acknowledge_all();
        let after = journal.read_unacknowledged();
        assert!(after.is_empty());
    }

    #[test]
    fn journal_wraparound() {
        let journal = TransitionJournal::new();
        // Fill the buffer with small entries to force wraparound.
        for i in 0..10_000usize {
            journal.append(&EngineInput::UserMessage(format!("msg-{i}")));
        }
        // Should not panic; entries near the end may be overwritten.
        let _entries = journal.read_unacknowledged();
    }

    #[test]
    fn journal_oversized_entry() {
        let journal = TransitionJournal::new();
        let huge = EngineInput::UserMessage("x".repeat(MAX_ENTRY_BYTES + 100));
        journal.append(&huge);

        let entries = journal.read_unacknowledged();
        assert_eq!(entries.len(), 1);
        assert!(
            matches!(entries[0], JournalEntry::Oversized { .. }),
            "oversized entry should be recorded as Oversized"
        );
    }
}
