//! SubRoutineOrchestrator — `SubRoutineHandler` implementation (Book II §5.4).
//!
//! Manages delegation and resolution of sub-routines via `SessionRegistry`.
//!
//! # Refactoring
//! Each `EngineInput` variant is handled by a dedicated pure helper
//! (`delegate_user_message`, `accumulate_stream_tools`, `resolve_tool_results`,
//! `detect_subroutine_termination`) to separate orchestration from calculation.
//!
//! Refs: I-Comp-Epoch-Subroutine, I-Comp-Pure-Logic

use brioche_core::{
    ActiveToolCall, AgentState, ChatMessage, Effect, EngineInput, PluginResult, Session,
    StreamEvent, SubRoutineHandler, ToolResultDTO,
};

/// Sub-routine orchestrator.
///
/// Delegates inputs to the child `Session` and handles bubbling up when
/// the child reaches `Idle` or `Failure`.
pub struct SubRoutineOrchestrator;

impl SubRoutineOrchestrator {
    /// Creates a new instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubRoutineOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Pure helpers — each corresponds to one `EngineInput` variant.
// ---------------------------------------------------------------------------

/// Push a user message into the child's history and transition to Predicting.
fn delegate_user_message(
    child: &mut Session,
    content: &str,
) -> Result<Vec<Effect>, brioche_core::BriocheError> {
    child.push_history(ChatMessage::User {
        content: content.into(),
    });

    let generation = child
        .extensions_mut()
        .with_or_insert_default::<brioche_core::EpochState, _>(|state| state.current_generation);
    child.push_state(AgentState::Predicting {
        generation_id: generation,
    })?;

    Ok(vec![Effect::CallLlmNetwork, Effect::SaveSession])
}

/// Accumulate tool-call fragments from an `LlmStream` event.
///
/// Returns `Some(effects)` when a complete tool call set is ready.
fn accumulate_stream_tools(
    child: &mut Session,
    event: &StreamEvent,
) -> Result<Option<Vec<Effect>>, brioche_core::BriocheError> {
    if !matches!(child.state(), AgentState::Predicting { .. }) {
        return Ok(None);
    }

    match event {
        StreamEvent::ToolCallStart { id, name, .. } => {
            child
                .extensions_mut()
                .with_or_insert_default::<brioche_core::StreamToolAccumulator, _>(|acc| {
                    acc.pending.insert(
                        id.clone(),
                        brioche_core::ToolCallDescriptor {
                            tool_id: id.clone(),
                            tool_name: name.clone(),
                            arguments: String::new(),
                            timeout_ms: None,
                        },
                    );
                });
            Ok(None)
        }
        StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
            child
                .extensions_mut()
                .with_or_insert_default::<brioche_core::StreamToolAccumulator, _>(|acc| {
                    if let Some(desc) = acc.pending.get_mut(id) {
                        desc.arguments.push_str(&String::from_utf8_lossy(chunk));
                    }
                });
            Ok(None)
        }
        StreamEvent::ToolCallDone { .. } => {
            let pending: Vec<_> = child
                .extensions_mut()
                .with_or_insert_default::<brioche_core::StreamToolAccumulator, _>(|acc| {
                    std::mem::take(&mut acc.pending).into_values().collect()
                });
            if pending.is_empty() {
                return Ok(None);
            }
            let active: Vec<ActiveToolCall> = pending
                .into_iter()
                .map(|d| brioche_core::seal(vec![d], 0).remove(0))
                .collect();
            child.set_active_tools(active.clone());
            let generation = if let AgentState::Predicting { generation_id } = child.state() {
                *generation_id
            } else {
                0
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
    if !matches!(child.state(), AgentState::ExecutingTools { .. }) {
        return Ok(());
    }
    child.pop_state()?;
    child.clear_active_tools();

    for result in results {
        child.push_history(ChatMessage::ToolResult {
            id: result.tool_id.clone(),
            tool_name: result.tool_name.clone(),
            outcome: result.outcome.clone(),
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
    match child.state() {
        AgentState::Idle => {
            if let Some(last) = child.history().last() {
                parent.push_history(last.clone());
            }
            parent.pop_state()?;
            Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
        }
        AgentState::Failure => {
            parent.push_history(ChatMessage::System {
                content: "sub-routine failed".into(),
            });
            parent.pop_state()?;
            Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
        }
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Trait implementation — thin orchestration layer.
// ---------------------------------------------------------------------------

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

            EngineInput::RestoreSubRoutine { .. } => {
                // Should not happen on an already active sub-routine.
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
