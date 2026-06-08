//! Sub-routine UI state manager — Book III-C §5.
//!
//! Tracks accordion states and isolated [`ContentRenderer`] instances
//! for each sub-routine. Deferred DOM open until `SubRoutineRestored`.
//!
//! ## Invariants upheld
//! - I-UI-NoDirectDOM: State is declarative; frontend decides rendering.
//! - I-UI-StreamBuffer: Each sub-routine has its own renderer.
//! - I-Eco-OrderedCollections: `BTreeMap` for deterministic ordering.
//!
//! Refs: SPECS.md §Book III-C Ch 5

use crate::ContentRenderer;
use brioche_core::SubRoutineHandle;
use std::collections::BTreeMap;

/// Accordion lifecycle states for a sub-routine in the UI.
///
/// The frontend drives transitions based on these states:
/// - `Idle`: accordion closed, renderer allocated but empty.
/// - `Loading`: user opened accordion, shell is fetching from persistence.
/// - `Loaded`: `SubRoutineRestored` received, renderer active.
/// - `Error`: load failed or sub-routine faulted.
/// - `Timeout`: `SubRoutineTimeoutPolicy` triggered.
///
/// Refs: I-UI-NoDirectDOM
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubRoutineAccordionState {
    /// Accordion closed; renderer exists but is empty.
    Idle,
    /// User requested open; shell fetching from cache / Redb.
    Loading,
    /// Kernel confirmed restoration; content may stream.
    Loaded,
    /// Persistence miss or plugin fault.
    Error,
    /// `SubRoutineTimeoutPolicy` fired.
    Timeout,
}

/// Per-sub-routine UI state.
///
/// Holds the accordion state and an isolated [`ContentRenderer`] for
/// streaming text accumulation.
///
/// Refs: I-UI-NoDirectDOM, I-UI-StreamBuffer
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubRoutineUiState {
    /// Current accordion lifecycle state.
    pub accordion: SubRoutineAccordionState,
    /// Isolated streaming renderer for this sub-routine.
    pub renderer: ContentRenderer,
}

impl SubRoutineUiState {
    /// Create a new UI state in the `Idle` accordion state.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn idle() -> Self {
        Self {
            accordion: SubRoutineAccordionState::Idle,
            renderer: ContentRenderer::new(),
        }
    }

    /// Create a new UI state in the `Loading` accordion state.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn loading() -> Self {
        Self {
            accordion: SubRoutineAccordionState::Loading,
            renderer: ContentRenderer::new(),
        }
    }
}

/// Manager for all sub-routine UI states.
///
/// `SubRoutineManager` is owned by the shell and updated in response
/// to kernel effects (`SubRoutineRestored`) and IPC commands
/// (`load_subroutine`).
///
/// Refs: I-UI-NoDirectDOM, I-Eco-OrderedCollections
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubRoutineManager {
    states: BTreeMap<SubRoutineHandle, SubRoutineUiState>,
}

impl SubRoutineManager {
    /// Create an empty manager.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
        }
    }

    /// Get or insert a `Loading` state for the given handle.
    ///
    /// If the handle already exists, returns a mutable reference to
    /// the existing state. Otherwise inserts `Loading` and returns it.
    ///
    /// This is called when the `load_subroutine` IPC command fires.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-UI-NoDirectDOM, I-Shell-Load-Batch
    pub fn begin_load(&mut self, handle: SubRoutineHandle) -> &mut SubRoutineUiState {
        self.states
            .entry(handle)
            .or_insert_with(SubRoutineUiState::loading)
    }

    /// Mark a sub-routine as `Loaded`.
    ///
    /// Called when the kernel emits `Effect::SubRoutineRestored`.
    /// If the handle is unknown, inserts a fresh `Loaded` state.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn mark_loaded(&mut self, handle: SubRoutineHandle) -> &mut SubRoutineUiState {
        let state = self
            .states
            .entry(handle)
            .or_insert_with(|| SubRoutineUiState {
                accordion: SubRoutineAccordionState::Loaded,
                renderer: ContentRenderer::new(),
            });
        state.accordion = SubRoutineAccordionState::Loaded;
        state
    }

    /// Mark a sub-routine as `Error`.
    ///
    /// Called when `load_subroutine` fails or a plugin fault occurs.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn mark_error(&mut self, handle: &SubRoutineHandle) -> Option<&mut SubRoutineUiState> {
        let state = self.states.get_mut(handle)?;
        state.accordion = SubRoutineAccordionState::Error;
        Some(state)
    }

    /// Mark a sub-routine as `Timeout`.
    ///
    /// Called when `SubRoutineTimeoutPolicy` fires.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-UI-NoDirectDOM
    pub fn mark_timeout(&mut self, handle: &SubRoutineHandle) -> Option<&mut SubRoutineUiState> {
        let state = self.states.get_mut(handle)?;
        state.accordion = SubRoutineAccordionState::Timeout;
        Some(state)
    }

    /// Remove a sub-routine and its renderer.
    ///
    /// Called on accordion close or session cleanup.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn remove(&mut self, handle: &SubRoutineHandle) -> Option<SubRoutineUiState> {
        self.states.remove(handle)
    }

    /// Access a sub-routine's UI state.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn get(&self, handle: &SubRoutineHandle) -> Option<&SubRoutineUiState> {
        self.states.get(handle)
    }

    /// Mutable access to a sub-routine's UI state.
    ///
    /// Complexity: O(log n).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn get_mut(&mut self, handle: &SubRoutineHandle) -> Option<&mut SubRoutineUiState> {
        self.states.get_mut(handle)
    }

    /// Iterate over all sub-routines in deterministic order.
    ///
    /// Complexity: O(1) for iterator creation.
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn iter(&self) -> impl Iterator<Item = (&SubRoutineHandle, &SubRoutineUiState)> {
        self.states.iter()
    }

    /// Number of tracked sub-routines.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Returns `true` if no sub-routines are tracked.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Clear all sub-routines.
    ///
    /// Complexity: O(1) (drops the map).
    ///
    /// Refs: I-Eco-OrderedCollections
    pub fn clear(&mut self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn manager_begin_load_inserts_loading() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let handle = SubRoutineHandle::new("sub-1")?;
        let state = mgr.begin_load(handle.clone());
        assert_eq!(state.accordion, SubRoutineAccordionState::Loading);
        assert_eq!(mgr.len(), 1);
        Ok(())
    }

    #[test]
    fn manager_mark_loaded_transitions_state() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let handle = SubRoutineHandle::new("sub-1")?;
        mgr.begin_load(handle.clone());
        let state = mgr.mark_loaded(handle.clone());
        assert_eq!(state.accordion, SubRoutineAccordionState::Loaded);
        Ok(())
    }

    #[test]
    fn manager_mark_loaded_creates_unknown_handle() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let handle = SubRoutineHandle::new("sub-1")?;
        let state = mgr.mark_loaded(handle.clone());
        assert_eq!(state.accordion, SubRoutineAccordionState::Loaded);
        assert_eq!(mgr.len(), 1);
        Ok(())
    }

    #[test]
    fn manager_mark_error_and_timeout() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let handle = SubRoutineHandle::new("sub-1")?;
        mgr.begin_load(handle.clone());

        let state = mgr
            .mark_error(&handle)
            .unwrap_or_else(|| unreachable!("handle must exist"));
        assert_eq!(state.accordion, SubRoutineAccordionState::Error);

        let state = mgr
            .mark_timeout(&handle)
            .unwrap_or_else(|| unreachable!("handle must exist"));
        assert_eq!(state.accordion, SubRoutineAccordionState::Timeout);
        Ok(())
    }

    #[test]
    fn manager_remove_clears_entry() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let handle = SubRoutineHandle::new("sub-1")?;
        mgr.begin_load(handle.clone());
        let removed = mgr
            .remove(&handle)
            .unwrap_or_else(|| unreachable!("handle must exist"));
        assert_eq!(removed.accordion, SubRoutineAccordionState::Loading);
        assert!(mgr.is_empty());
        Ok(())
    }

    #[test]
    fn manager_isolated_renderers() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        let h1 = SubRoutineHandle::new("sub-1")?;
        let h2 = SubRoutineHandle::new("sub-2")?;

        mgr.begin_load(h1.clone());
        mgr.begin_load(h2.clone());

        // Each sub-routine has its own ContentRenderer.
        let r1 = &mut mgr
            .get_mut(&h1)
            .unwrap_or_else(|| unreachable!("h1 must exist"))
            .renderer;
        r1.buffer_mut().append("trace", "alpha");

        let r2 = &mgr
            .get(&h2)
            .unwrap_or_else(|| unreachable!("h2 must exist"))
            .renderer;
        assert!(r2.buffer().is_empty());
        Ok(())
    }

    #[test]
    fn manager_iter_deterministic() -> Result<(), Box<dyn Error>> {
        let mut mgr = SubRoutineManager::new();
        mgr.begin_load(SubRoutineHandle::new("z")?);
        mgr.begin_load(SubRoutineHandle::new("a")?);
        mgr.begin_load(SubRoutineHandle::new("m")?);

        let keys: Vec<String> = mgr.iter().map(|(h, _)| h.as_str().to_string()).collect();
        assert_eq!(keys, vec!["a", "m", "z"]);
        Ok(())
    }
}
