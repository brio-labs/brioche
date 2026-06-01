//! Lazy session loading and sub-routine rehydration.
//!
//! This module provides helpers for restoring persisted session state
//! from Redb without blocking the engine thread.
//!
//! Refs: SPECS.md §Book III-B Ch 4, I-Shell-Load-Batch

use crate::{RedbStorage, SessionHeadDTO, SubRoutineCache, error::PersistenceError};
use brioche_core::ChatMessage;

/// Load a complete session (head + all messages) from Redb.
///
/// This is the inverse of the delta save protocol: it loads the head
/// DTO from `SESSIONS_TABLE` and all messages from `MESSAGES_TABLE`,
/// reconstructing the full session state needed for replay.
///
/// Returns `Ok(None)` if the session ID is not found.
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
            if let crate::FlattenedAgentState::SubRoutine(child_id) = state
                && !cache.contains(child_id)
                && let Some(child) = self.storage.load_session(child_id).await?
            {
                cache.insert(child_id.clone(), child);
            }
        }

        Ok(Some((head, messages)))
    }
}
