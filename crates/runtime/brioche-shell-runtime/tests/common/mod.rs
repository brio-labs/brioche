#![allow(dead_code)]

use std::sync::Arc;

use brioche_core::{AgentState, BriocheEngineBuilder, ChatMessage, Session};
use brioche_governance_default::{LexicographicDecisionAggregator, SubRoutineCleanupGuard};
use brioche_shell_runtime::{
    BriocheShell, DefaultEffectExecutor, EchoToolExecutor, MockLlmClient, NoopPersistence,
    SessionCallback, ShellConfig,
};

pub fn build_minimal_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard))
        .build()
}

pub fn build_shell() -> BriocheShell {
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);
    BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor,
        None,
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
    pub state: AgentState,
    pub generation_id: Option<u64>,
    pub stack_depth: usize,
    pub history: Vec<ChatMessage>,
}

pub fn session_recorder() -> (SessionCallback, Arc<std::sync::Mutex<Vec<SessionView>>>) {
    let views = Arc::new(std::sync::Mutex::new(Vec::new()));
    let views_clone = Arc::clone(&views);
    let callback: SessionCallback = Box::new(move |session| {
        if let Ok(mut guard) = views_clone.lock() {
            guard.push(SessionView {
                state: session.state.clone(),
                generation_id: match session.state {
                    AgentState::Predicting { generation_id }
                    | AgentState::ExecutingTools { generation_id } => Some(generation_id),
                    _ => None,
                },
                stack_depth: session.state_stack.len(),
                history: session.history.clone(),
            });
        }
    });
    (callback, views)
}

pub fn recorded_views(views: &Arc<std::sync::Mutex<Vec<SessionView>>>) -> Vec<SessionView> {
    match views.lock() {
        Ok(guard) => guard.clone(),
        Err(_) => Vec::new(),
    }
}

pub fn is_predicting(state: &AgentState, generation_id: u64) -> bool {
    matches!(state, AgentState::Predicting { generation_id: g } if *g == generation_id)
}

pub fn is_executing_tools(state: &AgentState, generation_id: u64) -> bool {
    matches!(state, AgentState::ExecutingTools { generation_id: g } if *g == generation_id)
}

pub fn is_idle(state: &AgentState) -> bool {
    matches!(state, AgentState::Idle)
}

pub fn build_shell_with_recorder() -> (BriocheShell, Arc<std::sync::Mutex<Vec<SessionView>>>) {
    let (callback, views) = session_recorder();
    let executor =
        DefaultEffectExecutor::new(EchoToolExecutor, MockLlmClient::default(), NoopPersistence);
    let shell = BriocheShell::new(
        || (build_minimal_engine(), Session::new("test")),
        ShellConfig::default(),
        executor,
        Some(callback),
    );
    (shell, views)
}
