//! Two-level sub-routine cache (L1 Visible / L2 LRU).
//!
//! The shell manipulates only flattened DTOs; the kernel holds live
//! `Session` instances via `SessionRegistry`. `SubRoutineCache` bridges
//! the two by keeping recently-used sub-routine heads in memory.
//!
//! Refs: SPECS.md §Book III-B Ch 2, I-Persist-Cache

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use lru::LruCache;

use crate::dto::SessionHeadDTO;

/// Two-level cache for sub-routine session heads.
///
/// - **L1 Visible**: sub-routines currently open in the UI (accordions).
///   These DTOs are never evicted by LRU policy.
/// - **L2 LRU**: recently used sub-routines managed by LRU eviction.
///
/// `BTreeMap` is used for L1 to uphold deterministic ordering.
///
/// Refs: SPECS.md §Book III-B Ch 2.1
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
    pub fn get(&self, id: &str) -> Option<&SessionHeadDTO> {
        self.l1_visible.get(id).or_else(|| self.l2_lru.peek(id))
    }

    /// Move a sub-routine from L2 to L1 (UI opened the accordion).
    ///
    /// Returns the previously held DTO if the ID was already in L1.
    ///
    /// Complexity: O(log n) for L1 insertion + O(1) for L2 removal.
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
    pub fn insert(&mut self, id: String, dto: SessionHeadDTO) {
        self.l2_lru.put(id, dto);
    }

    /// Returns `true` if the ID is present in either tier.
    ///
    /// Complexity: O(log n) for L1 + O(1) for L2.
    pub fn contains(&self, id: &str) -> bool {
        self.l1_visible.contains_key(id) || self.l2_lru.contains(id)
    }

    /// Remove a sub-routine from both tiers.
    ///
    /// Returns the removed DTO if present.
    ///
    /// Complexity: O(log n) for L1 + O(1) for L2.
    pub fn remove(&mut self, id: &str) -> Option<SessionHeadDTO> {
        self.l1_visible.remove(id).or_else(|| self.l2_lru.pop(id))
    }

    /// Number of entries currently in L1.
    pub fn l1_len(&self) -> usize {
        self.l1_visible.len()
    }

    /// Number of entries currently in L2.
    pub fn l2_len(&self) -> usize {
        self.l2_lru.len()
    }
}
