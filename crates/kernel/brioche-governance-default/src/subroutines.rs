//! Sub-routine lifecycle — Book II §5.
//!
//! Reference implementations for sub-routine delegation and cleanup:
//! - `SubRoutineOrchestrator`: `SubRoutineHandler`
//! - `SubRoutineCleanupGuard`: `SubRoutineLifecycleGuard`
//!
//! Refs: I-Comp-Epoch-Subroutine, I-Gov-SubRoutineLifecycle-Guard

use brioche_core::{
    ActiveToolCall, AgentState, ChatMessage, Effect, EngineInput, PluginResult, Session,
    SessionRegistry, StreamEvent, SubRoutineHandle, SubRoutineHandler, SubRoutineLifecycleGuard,
    ToolResultDTO,
};

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
        _parent: &mut Session,
        registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        registry.increment_exit_count(&handle);

        if registry.remove(&handle).is_some() {
            Ok(vec![Effect::SaveSession])
        } else {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_core::{AgentState, ChatMessage, Effect, EngineInput, Session, SubRoutineHandle};

    fn subroutine_parent() -> Session {
        let mut parent = Session::new("parent");
        let _ = parent.push_state(AgentState::Predicting { generation_id: 1 });
        let _ = parent.push_state(AgentState::SubRoutine(SubRoutineHandle::new("sub")));
        parent
    }

    #[test]
    fn orchestrator_delegates_user_message() {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent();
        let mut child = Session::new("child");

        let result = orchestrator.handle_subroutine(
            &mut parent,
            &mut child,
            &EngineInput::UserMessage("hello".into()),
        );

        let effects = match result {
            Ok(Some(effects)) => effects,
            Ok(None) => Vec::new(),
            Err(_) => {
                assert!(false, "handle_subroutine should succeed");
                return;
            }
        };

        assert_eq!(child.history.len(), 1);
        assert!(matches!(child.history[0], ChatMessage::User { .. }));
        assert!(matches!(child.state, AgentState::Predicting { .. }));
        assert!(effects.contains(&Effect::CallLlmNetwork));
        assert!(effects.contains(&Effect::SaveSession));
    }

    #[test]
    fn orchestrator_accumulates_stream_tools() {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent();
        let mut child = Session::new("child");
        let _ = child.push_state(AgentState::Predicting { generation_id: 1 });

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
            match orchestrator.handle_subroutine(&mut parent, &mut child, event) {
                Ok(Some(e)) => effects.extend(e),
                Ok(None) => {}
                Err(_) => {
                    assert!(false, "handle_subroutine should succeed");
                    return;
                }
            }
        }

        assert!(matches!(child.state, AgentState::ExecutingTools { .. }));
        assert_eq!(child.active_tools.len(), 1);
        assert_eq!(child.active_tools[0].tool_id, "tc1");
        assert_eq!(child.active_tools[0].tool_name, "calc");
        assert_eq!(
            child.active_tools[0].timeout_ms,
            brioche_core::DEFAULT_TOOL_TIMEOUT_MS
        );
        assert!(effects.iter().any(|e| matches!(e, Effect::ExecuteTools(_))));
        assert!(effects.contains(&Effect::SaveSession));
    }

    #[test]
    fn orchestrator_bubbles_up_on_idle() {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent();
        let mut child = Session::new("child");
        child.history.push(ChatMessage::Assistant {
            content: "sub-result".into(),
            reasoning: None,
            tool_calls: Vec::new(),
        });
        child.state = AgentState::Idle;

        let result = orchestrator.handle_subroutine(
            &mut parent,
            &mut child,
            &EngineInput::ToolCallsResult {
                generation_id: 1,
                results: Vec::new(),
            },
        );

        let effects = match result {
            Ok(Some(effects)) => effects,
            Ok(None) => {
                assert!(false, "expected bubble-up effects");
                return;
            }
            Err(_) => {
                assert!(false, "handle_subroutine should succeed");
                return;
            }
        };

        assert_eq!(parent.history.len(), 1);
        assert_eq!(
            parent.history[0],
            ChatMessage::Assistant {
                content: "sub-result".into(),
                reasoning: None,
                tool_calls: Vec::new(),
            }
        );
        assert!(matches!(parent.state, AgentState::Predicting { generation_id: 1 }));
        assert!(effects.contains(&Effect::SaveSession));
        assert!(effects.contains(&Effect::CallLlmNetwork));
    }

    #[test]
    fn orchestrator_bubbles_up_on_failure() {
        let orchestrator = SubRoutineOrchestrator::new();
        let mut parent = subroutine_parent();
        let mut child = Session::new("child");
        child.state = AgentState::Failure;

        let result = orchestrator.handle_subroutine(
            &mut parent,
            &mut child,
            &EngineInput::ToolCallsResult {
                generation_id: 1,
                results: Vec::new(),
            },
        );

        let effects = match result {
            Ok(Some(effects)) => effects,
            Ok(None) => {
                assert!(false, "expected bubble-up effects");
                return;
            }
            Err(_) => {
                assert!(false, "handle_subroutine should succeed");
                return;
            }
        };

        assert_eq!(parent.history.len(), 1);
        assert_eq!(
            parent.history[0],
            ChatMessage::System {
                content: "sub-routine failed".into(),
            }
        );
        assert!(matches!(parent.state, AgentState::Predicting { generation_id: 1 }));
        assert!(effects.contains(&Effect::SaveSession));
        assert!(effects.contains(&Effect::CallLlmNetwork));
    }

    #[test]
    fn cleanup_guard_removes_child_session() {
        let guard = SubRoutineCleanupGuard::new();
        let handle = SubRoutineHandle::new("sub");
        let mut registry = SessionRegistry::new();
        registry.insert(handle.clone(), Session::new("child"));
        let mut parent = Session::new("parent");

        let result = guard.on_exit(handle, &mut parent, &mut registry);

        let effects = match result {
            Ok(effects) => effects,
            Err(_) => {
                assert!(false, "on_exit should succeed");
                return;
            }
        };

        assert!(!registry.contains(&SubRoutineHandle::new("sub")));
        assert_eq!(effects, vec![Effect::SaveSession]);
    }
}
