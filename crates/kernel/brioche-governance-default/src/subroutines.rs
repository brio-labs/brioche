//! Sub-routine lifecycle — Book II §5.
//!
//! Reference implementations for sub-routine delegation and cleanup:
//! - `SubRoutineOrchestrator`: `SubRoutineHandler`
//! - `SubRoutineCleanupGuard`: `SubRoutineLifecycleGuard`
//!
//! Refs: I-Comp-Epoch-Subroutine, I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    ActiveToolCall, AgentState, BriocheExtensionType, ChatMessage, Effect, EngineInput,
    PluginResult, Session, SessionRegistry, StreamEvent, SubRoutineHandle, SubRoutineHandler,
    SubRoutineLifecycleGuard, ToolResultDTO,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// SubRoutineOrchestrator
// ---------------------------------------------------------------------------

/// Sub-routine orchestrator.
///
/// Delegates inputs to the child `Session` and handles bubbling up when
/// the child reaches `Idle` or `Failure`.
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

/// Push a user message into the child's history and transition to Predicting.
fn delegate_user_message(
    child: &mut Session,
    content: &str,
) -> Result<Vec<Effect>, brioche_core::BriocheError> {
    child.history.push(ChatMessage::User {
        content: content.into(),
    });

    let generation = child
        .extensions
        .get_or_insert_default::<brioche_core::EpochState>()
        .current_generation;
    child.push_state(AgentState::Predicting {
        generation_id: generation,
    })?;

    Ok(vec![Effect::CallLlmNetwork, Effect::SaveSession])
}

/// Accumulate tool-call fragments from an `LlmStream` event.
fn accumulate_stream_tools(
    child: &mut Session,
    event: &StreamEvent,
) -> Result<Option<Vec<Effect>>, brioche_core::BriocheError> {
    if !matches!(child.state, AgentState::Predicting { .. }) {
        return Ok(None);
    }

    match event {
        StreamEvent::ToolCallStart { id, name, .. } => {
            let acc = child
                .extensions
                .get_or_insert_default::<brioche_core::StreamToolAccumulator>();
            acc.pending.insert(
                id.clone(),
                brioche_core::ToolCallDescriptor {
                    tool_id: id.clone(),
                    tool_name: name.clone(),
                    arguments: String::new(),
                    timeout_ms: None,
                },
            );
            Ok(None)
        }
        StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
            let acc = child
                .extensions
                .get_or_insert_default::<brioche_core::StreamToolAccumulator>();
            if let Some(desc) = acc.pending.get_mut(id) {
                desc.arguments.push_str(&String::from_utf8_lossy(chunk));
            }
            Ok(None)
        }
        StreamEvent::ToolCallDone { .. } => {
            let acc = child
                .extensions
                .get_or_insert_default::<brioche_core::StreamToolAccumulator>();
            let pending: Vec<_> = std::mem::take(&mut acc.pending).into_values().collect();
            if pending.is_empty() {
                return Ok(None);
            }
            let active: Vec<ActiveToolCall> = pending
                .into_iter()
                .map(|d| brioche_core::seal_single(d, brioche_core::DEFAULT_TOOL_TIMEOUT_MS))
                .collect();
            child.active_tools = active.clone();
            let generation = match child.state {
                AgentState::Predicting { generation_id } => generation_id,
                _ => 0,
            };
            child.push_state(AgentState::ExecutingTools {
                generation_id: generation,
            })?;
            Ok(Some(vec![
                Effect::ExecuteTools(active),
                Effect::SaveSession,
            ]))
        }
        _ => Ok(None),
    }
}

/// Convert tool results into history entries and transition child back to Predicting.
fn resolve_tool_results(
    child: &mut Session,
    generation_id: u64,
    results: &[ToolResultDTO],
) -> Result<(), brioche_core::BriocheError> {
    if !matches!(child.state, AgentState::ExecutingTools { .. }) {
        return Ok(());
    }
    child.pop_state()?;
    child.active_tools.clear();

    for result in results {
        child.history.push(ChatMessage::ToolResult {
            id: result.tool_id.clone(),
            content: brioche_core::tool_outcome_to_string(&result.outcome),
        });
    }

    child.push_state(AgentState::Predicting { generation_id })?;
    Ok(())
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
    fn handle_subroutine(
        &self,
        parent: &mut Session,
        child: &mut Session,
        input: &EngineInput,
    ) -> PluginResult<Option<Vec<Effect>>> {
        let wrap = |e: brioche_core::BriocheError| brioche_core::PluginError::Soft {
            plugin_name: "subroutine_orchestrator".into(),
            message: e.to_string(),
        };

        match input {
            EngineInput::UserMessage(content) => delegate_user_message(child, content)
                .map(Some)
                .map_err(wrap),

            EngineInput::LlmStream(event) => accumulate_stream_tools(child, event).map_err(wrap),

            EngineInput::ToolCallsResult {
                generation_id,
                results,
            } => {
                resolve_tool_results(child, *generation_id, results).map_err(wrap)?;
                detect_subroutine_termination(parent, child).map_err(wrap)
            }

            EngineInput::RestoreSubRoutine { .. } => Ok(None),
            _ => Ok(None),
        }
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
    fn orchestrator_delegates_user_message() -> Result<(), PluginError> {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent()?;
        let mut child = Session::new("child");

        let mut effects = Vec::new();
        if let Some(e) = orchestrator.handle_subroutine(
            &mut parent,
            &mut child,
            &EngineInput::UserMessage("hello".into()),
        )? {
            effects = e;
        }

        assert_eq!(child.history.len(), 1);
        assert!(matches!(child.history[0], ChatMessage::User { .. }));
        assert!(matches!(child.state, AgentState::Predicting { .. }));
        assert!(effects.contains(&Effect::CallLlmNetwork));
        assert!(effects.contains(&Effect::SaveSession));
        Ok(())
    }

    #[test]
    fn orchestrator_accumulates_stream_tools() -> Result<(), PluginError> {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent()?;
        let mut child = Session::new("child");
        child
            .push_state(AgentState::Predicting { generation_id: 1 })
            .map_err(to_plugin_err)?;

        let path = brioche_core::ExecutionPath::default();
        let events = vec![
            EngineInput::LlmStream(brioche_core::StreamEvent::ToolCallStart {
                path: path.clone(),
                id: "tc1".into(),
                name: "calc".into(),
            }),
            EngineInput::LlmStream(brioche_core::StreamEvent::ToolArgumentChunk {
                path: path.clone(),
                id: "tc1".into(),
                chunk: From::from(&b"{\"x\":1}"[..]),
            }),
            EngineInput::LlmStream(brioche_core::StreamEvent::ToolCallDone { path }),
        ];

        let mut effects = Vec::new();
        for event in &events {
            if let Some(e) = orchestrator.handle_subroutine(&mut parent, &mut child, event)? {
                effects.extend(e);
            }
        }

        assert!(matches!(child.state, AgentState::ExecutingTools { .. }));
        assert_eq!(child.active_tools.len(), 1);
        let active = child
            .active_tools
            .first()
            .ok_or_else(|| PluginError::Fatal {
                plugin_name: "subroutine_test".into(),
                message: "expected active tool".into(),
            })?;
        assert_eq!(active.tool_id, "tc1");
        assert_eq!(active.tool_name, "calc");
        assert_eq!(active.timeout_ms, brioche_core::DEFAULT_TOOL_TIMEOUT_MS);
        assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
        assert!(effects.contains(&Effect::SaveSession));
        Ok(())
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
