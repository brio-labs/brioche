//! SubRoutineOrchestrator — implémentation `SubRoutineHandler` (Book II §5.4).
//!
//! Gère la délégation et la résolution des sous-routines via `SessionRegistry`.
//!
//! # Limitation connue
//! Le trait `SubRoutineHandler` reçoit le `child` mutable mais pas le moteur.
//! La délégation complète de `transition(child, input)` nécessite une
//! évolution de l'interface kernel/shell (Sprint 7+). Pour le sprint 6,
//! cette implémentation gère les cas simples (pass-through) et la
//! détection de terminaison `Idle`/`Failure`.
//!
//! Refs: I-Comp-Epoch-Subroutine

use brioche_core::{
    ActiveToolCall, AgentState, ChatMessage, Effect, EngineInput, PluginResult, Session,
    SubRoutineHandler,
};

/// Orchestrateur de sous-routines.
///
/// Délègue les inputs au `Session` enfant et gère la remontée quand
/// l'enfant atteint `Idle` ou `Failure`.
pub struct SubRoutineOrchestrator;

impl SubRoutineOrchestrator {
    /// Crée une nouvelle instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SubRoutineOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl SubRoutineHandler for SubRoutineOrchestrator {
    fn handle_subroutine(
        &self,
        parent: &mut Session,
        child: &mut Session,
        input: &EngineInput,
    ) -> PluginResult<Option<Vec<Effect>>> {
        match input {
            EngineInput::UserMessage(content) => {
                child.history.push(ChatMessage::User {
                    content: content.clone(),
                });

                // Transition mécanique Idle -> Predicting sur l'enfant.
                let generation = child
                    .extensions
                    .get_or_insert_default::<brioche_core::EpochState>()
                    .current_generation;
                let _ = child.push_state(AgentState::Predicting {
                    generation_id: generation,
                });

                Ok(Some(vec![Effect::CallLlmNetwork, Effect::SaveSession]))
            }

            EngineInput::LlmStream(event) => {
                // Hot-path : délégation directe du stream vers l'enfant.
                // Le kernel n'applique pas `transition()` sur l'enfant ici ;
                // l'orchestrateur simule l'accumulation d'outils si besoin.
                if matches!(child.state, AgentState::Predicting { .. }) {
                    use brioche_core::StreamEvent;
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
                        }
                        StreamEvent::ToolArgumentChunk { id, chunk, .. } => {
                            let acc = child
                                .extensions
                                .get_or_insert_default::<brioche_core::StreamToolAccumulator>();
                            if let Some(desc) = acc.pending.get_mut(id) {
                                desc.arguments.push_str(&String::from_utf8_lossy(chunk));
                            }
                        }
                        StreamEvent::ToolCallDone { .. } => {
                            let acc = child
                                .extensions
                                .get_or_insert_default::<brioche_core::StreamToolAccumulator>();
                            let pending: Vec<_> =
                                std::mem::take(&mut acc.pending).into_values().collect();
                            if !pending.is_empty() {
                                let active: Vec<ActiveToolCall> = pending
                                    .into_iter()
                                    .map(|d| brioche_core::seal(vec![d]).remove(0))
                                    .collect();
                                child.active_tools = active.clone();
                                let generation = match child.state {
                                    AgentState::Predicting { generation_id } => generation_id,
                                    _ => 0,
                                };
                                let _ = child.push_state(AgentState::ExecutingTools {
                                    generation_id: generation,
                                });
                                return Ok(Some(vec![
                                    Effect::ExecuteTools(active),
                                    Effect::SaveSession,
                                ]));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }

            EngineInput::ToolCallsResult {
                generation_id,
                results,
            } => {
                if matches!(child.state, AgentState::ExecutingTools { .. }) {
                    let _ = child.pop_state();
                    child.active_tools.clear();

                    for result in results {
                        let content = match &result.outcome {
                            brioche_core::ToolOutcome::Success(s)
                            | brioche_core::ToolOutcome::BusinessError(s)
                            | brioche_core::ToolOutcome::SystemError(s) => s.clone(),
                            brioche_core::ToolOutcome::TimeoutWithPartialData {
                                partial_output,
                            } => partial_output.clone().unwrap_or_default(),
                        };
                        child.history.push(ChatMessage::ToolResult {
                            id: result.tool_id.clone(),
                            content,
                        });
                    }

                    let _ = child.push_state(AgentState::Predicting {
                        generation_id: *generation_id,
                    });
                }

                // Détection de terminaison de sous-routine.
                match &child.state {
                    AgentState::Idle => {
                        // Extrait le dernier message de l'enfant et l'injecte dans le parent.
                        if let Some(last) = child.history.last() {
                            parent.history.push(last.clone());
                        }
                        parent.pop_state().ok();
                        Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
                    }
                    AgentState::Failure => {
                        parent.history.push(ChatMessage::System {
                            content: "sub-routine failed".into(),
                        });
                        parent.pop_state().ok();
                        Ok(Some(vec![Effect::SaveSession, Effect::CallLlmNetwork]))
                    }
                    _ => Ok(None),
                }
            }

            EngineInput::RestoreSubRoutine { .. } => {
                // Ne devrait pas arriver sur une sous-routine déjà active.
                Ok(None)
            }
        }
    }
}
