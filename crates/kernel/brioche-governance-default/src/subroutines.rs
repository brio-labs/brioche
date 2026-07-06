//! Sub-routine lifecycle — Book II §5.
//!
//! Reference implementations for sub-routine delegation and cleanup:
//! - `SubRoutineOrchestrator`: `SubRoutineHandler`
//! - `SubRoutineCleanupGuard`: `SubRoutineLifecycleGuard`
//!
//! Refs: I-Comp-Epoch-Subroutine, I-Gov-SubRoutineLifecycle-Guard

use std::collections::BTreeMap;

use brioche_core::{
    AgentState, BriocheExtensionType, ChatMessage, Effect, EngineInput, PluginResult, Session,
    SessionRegistry, SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SubRoutineOrchestrator
// ---------------------------------------------------------------------------

/// Sub-routine orchestrator.
///
/// Monitors the child `Session` after its native transition and handles
/// bubbling up when the child reaches `Idle` or `Failure`.
///
/// Refs: I-Comp-Epoch-Subroutine, I-Shell-Session-NoSend
pub struct SubRoutineOrchestrator;

impl SubRoutineOrchestrator {
    /// Creates a new instance.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubRoutineOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect terminal sub-routine states and bubble the result up to the parent.
fn detect_subroutine_termination(
    parent: &mut Session,
    child: &mut Session,
) -> Result<Option<Vec<Effect>>, brioche_core::BriocheError> {
    match &child.state {
        AgentState::Idle => {
            if let Some(last) = child.history.last() {
                parent.history.push(last.clone());
            }
            parent.pop_state()?;
            Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
        }
        AgentState::Failure => {
            parent.history.push(ChatMessage::System {
                content: "sub-routine failed".into(),
            });
            parent.pop_state()?;
            Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
        }
        _ => Ok(None),
    }
}

impl SubRoutineHandler for SubRoutineOrchestrator {
    type Session = Session;
    type EngineInput = EngineInput;
    type Effect = Effect;
    type PluginError = brioche_core::PluginError;

    fn handle_subroutine(
        &self,
        parent: &mut Session,
        child: &mut Session,
        _input: &EngineInput,
    ) -> PluginResult<Option<Vec<Effect>>> {
        let wrap = |e: brioche_core::BriocheError| brioche_core::PluginError::Soft {
            plugin_name: "subroutine_orchestrator".into(),
            message: e.to_string(),
        };
        detect_subroutine_termination(parent, child).map_err(wrap)
    }
}

// ---------------------------------------------------------------------------
// SubRoutineCleanupGuard
// ---------------------------------------------------------------------------

/// Outgoing sub-routine transition counters stored as governance state.
///
/// `SubRoutineCleanupGuard` uses this state to track how many times each
/// sub-routine handle has exited, without polluting the mechanism-only
/// `SessionRegistry`.
///
/// ## Snapshot strategy
/// No snapshot: `SubRoutineExitState` is reconstructed each transition
/// cycle. The exit counters are transient governance metadata; they are
/// not persisted across engine restarts.
///
/// # Invariants
/// - I-Eco-OrderedCollections: Uses `BTreeMap` for deterministic iteration.
/// - I-Gov-SubRoutineLifecycle-Guard: Governance owns per-handle lifecycle metadata.
///
/// # Complexity
/// O(log n) per handle lookup/insertion.
///
/// # Panics
/// Never panics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct SubRoutineExitState {
    /// Map sub-routine handle -> number of outgoing transitions observed.
    /// BTreeMap preserves deterministic iteration order.
    pub exit_counts: BTreeMap<SubRoutineHandle, u64>,
}

/// Sub-routine cleanup guard.
///
/// Cleans up the `SessionRegistry` on every outgoing transition from
/// the `SubRoutine` state, preventing the accumulation of orphaned sessions.
///
/// Refs: I-Gov-TraitAtomic
/// Refs: I-Gov-SubRoutineLifecycle-Guard
pub struct SubRoutineCleanupGuard;

impl SubRoutineCleanupGuard {
    /// Creates a new instance of the cleanup guard.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubRoutineCleanupGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl SubRoutineLifecycleGuard for SubRoutineCleanupGuard {
    type SubRoutineHandle = SubRoutineHandle;
    type Session = Session;
    type SessionRegistry = SessionRegistry;
    type Effect = Effect;
    type PluginError = brioche_core::PluginError;

    fn on_exit(
        &self,
        handle: SubRoutineHandle,
        parent: &mut Session,
        registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        let state = parent
            .extensions
            .get_or_insert_default::<SubRoutineExitState>();
        *state.exit_counts.entry(handle.clone()).or_insert(0) += 1;

        if registry.remove(&handle).is_some() {
            Ok(vec![Effect::SaveSession])
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use brioche_core::{
        AgentState, BriocheError, ChatMessage, Effect, EngineInput, PluginError, Session,
        SubRoutineHandle,
    };

    use super::*;

    fn to_plugin_err(e: BriocheError) -> PluginError {
        PluginError::Fatal {
            plugin_name: "subroutine_test".into(),
            message: e.to_string(),
        }
    }

    fn subroutine_parent() -> Result<Session, PluginError> {
        let mut parent = Session::new("parent");
        parent
            .push_state(AgentState::Predicting { generation_id: 1 })
            .map_err(to_plugin_err)?;
        parent
            .push_state(AgentState::SubRoutine(SubRoutineHandle::new("sub")))
            .map_err(to_plugin_err)?;
        Ok(parent)
    }

    #[test]
    fn orchestrator_bubbles_up_on_idle() -> Result<(), PluginError> {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent()?;
        let mut child = Session::new("child");
        child.history.push(ChatMessage::Assistant {
            content: "sub-result".into(),
            reasoning: None,
            tool_calls: Vec::new(),
        });
        child.state = AgentState::Idle;

        let effects = orchestrator
            .handle_subroutine(
                &mut parent,
                &mut child,
                &EngineInput::ToolCallsResult {
                    generation_id: 1,
                    results: Vec::new(),
                },
            )?
            .ok_or_else(|| PluginError::Fatal {
                plugin_name: "subroutine_test".into(),
                message: "expected bubble-up effects".into(),
            })?;

        assert_eq!(parent.history.len(), 1);
        let first = parent.history.first().ok_or_else(|| PluginError::Fatal {
            plugin_name: "subroutine_test".into(),
            message: "expected bubbled history entry".into(),
        })?;
        assert_eq!(
            first,
            &ChatMessage::Assistant {
                content: "sub-result".into(),
                reasoning: None,
                tool_calls: Vec::new(),
            }
        );
        assert!(matches!(
            parent.state,
            AgentState::Predicting { generation_id: 1 }
        ));
        assert!(effects.contains(&Effect::SaveSession));
        assert!(effects.contains(&Effect::CallLlmNetwork));
        Ok(())
    }

    #[test]
    fn orchestrator_bubbles_up_on_failure() -> Result<(), PluginError> {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent()?;
        let mut child = Session::new("child");
        child.state = AgentState::Failure;

        let effects = orchestrator
            .handle_subroutine(
                &mut parent,
                &mut child,
                &EngineInput::ToolCallsResult {
                    generation_id: 1,
                    results: Vec::new(),
                },
            )?
            .ok_or_else(|| PluginError::Fatal {
                plugin_name: "subroutine_test".into(),
                message: "expected bubble-up effects".into(),
            })?;

        assert_eq!(parent.history.len(), 1);
        let first = parent.history.first().ok_or_else(|| PluginError::Fatal {
            plugin_name: "subroutine_test".into(),
            message: "expected bubbled history entry".into(),
        })?;
        assert_eq!(
            first,
            &ChatMessage::System {
                content: "sub-routine failed".into(),
            }
        );
        assert!(matches!(
            parent.state,
            AgentState::Predicting { generation_id: 1 }
        ));
        assert!(effects.contains(&Effect::SaveSession));
        assert!(effects.contains(&Effect::CallLlmNetwork));
        Ok(())
    }

    #[test]
    fn cleanup_guard_tracks_exit_count_in_extension_state() -> Result<(), PluginError> {
        let guard = SubRoutineCleanupGuard::new();
        let handle = SubRoutineHandle::new("sub");
        let mut registry = SessionRegistry::new();
        registry.insert(handle.clone(), Session::new("child"));
        let mut parent = Session::new("parent");

        let effects = guard.on_exit(handle.clone(), &mut parent, &mut registry)?;

        assert!(!registry.contains(&handle));
        assert_eq!(effects, vec![Effect::SaveSession]);

        let state = parent
            .extensions
            .get_or_insert_default::<SubRoutineExitState>();
        assert_eq!(state.exit_counts.get(&handle), Some(&1));
        Ok(())
    }
}
