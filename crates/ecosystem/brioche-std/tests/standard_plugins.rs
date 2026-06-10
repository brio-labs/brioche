//! Sprint 16 integration tests: standard plugin matrix.
//!
//! Invariants verified:
//! - I-Eco-ExtensionOverMod: Standard plugins are pure policy.
//! - I-Eco-OrderedCollections: All state uses ordered collections.
//! - I-Eco-Decision-Isolation: Plugins only mutate their own state.
//!
//! Refs: SPECS.md §Book IV Ch 1

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriochePlugin, ChatMessage, Effect, EngineInput,
    ExtensionStorage, PolicyDecision, Session, StreamEvent, ToolCallDescriptor, ToolOutcome,
    ToolResultDTO,
};
use brioche_governance_default::{
    LexicographicDecisionAggregator, SubRoutineCleanupGuard, ToolResultFormatter,
    ToolResultFormatterState,
};
use brioche_std::{
    AuditLogger, AuditLoggerState, CircuitBreaker, CircuitBreakerState, ContextOptimizer,
    ContextOptimizerState, GcPolicy, GcPolicyState, PendingTaskManager, PendingTaskState,
    TokenTracker, TokenTrackerState, ToolTimeoutPolicy,
};

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

#[test]
fn circuit_breaker_allows_below_threshold() {
    let breaker = CircuitBreaker::with_max_repetitions(3);
    let mut ext = ExtensionStorage::new();
    let history = vec![
        ChatMessage::ToolRequest {
            id: "t1".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
        ChatMessage::ToolRequest {
            id: "t2".into(),
            name: "calc".into(),
            arguments: "{\"x\":2}".into(),
        },
    ];

    let decision = match breaker.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(matches!(decision, PolicyDecision::Allow));
}

#[test]
fn circuit_breaker_blocks_at_threshold() {
    let breaker = CircuitBreaker::with_max_repetitions(2);
    let mut ext = ExtensionStorage::new();
    let history = vec![
        ChatMessage::ToolRequest {
            id: "t1".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
        ChatMessage::ToolRequest {
            id: "t2".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
        ChatMessage::ToolRequest {
            id: "t3".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
    ];

    let decision = match breaker.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(
        matches!(decision, PolicyDecision::Block { .. }),
        "circuit breaker should block when threshold exceeded"
    );

    let state = ext.get_or_insert_default::<CircuitBreakerState>();
    assert_eq!(state.loops_broken, 1);
}

#[test]
fn circuit_breaker_resets_on_different_signature() {
    let breaker = CircuitBreaker::with_max_repetitions(2);
    let mut ext = ExtensionStorage::new();
    let history = vec![
        ChatMessage::ToolRequest {
            id: "t1".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
        ChatMessage::ToolRequest {
            id: "t2".into(),
            name: "calc".into(),
            arguments: "{\"x\":2}".into(),
        },
        ChatMessage::ToolRequest {
            id: "t3".into(),
            name: "calc".into(),
            arguments: "{\"x\":1}".into(),
        },
    ];

    let decision = match breaker.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(matches!(decision, PolicyDecision::Allow));
}

// ---------------------------------------------------------------------------
// TokenTracker
// ---------------------------------------------------------------------------

#[test]
fn token_tracker_estimates_from_history() {
    let tracker = TokenTracker::new();
    let mut ext = ExtensionStorage::new();
    let history = vec![
        ChatMessage::User {
            content: "hello world".into(),
        },
        ChatMessage::Assistant {
            content: "hi there".into(),
            reasoning: None,
            tool_calls: Vec::new(),
        },
    ];

    let decision = match tracker.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(matches!(decision, PolicyDecision::Allow));

    let state = ext.get_or_insert_default::<TokenTrackerState>();
    // "hello world" = 11 chars => ceil(11/4) = 3 tokens (input)
    // "hi there" = 8 chars => ceil(8/4) = 2 tokens (output)
    assert!(state.total_input_tokens >= 3);
    assert!(state.total_output_tokens >= 2);
}

#[test]
fn token_tracker_increments_cycles() {
    let tracker = TokenTracker::new();
    let mut ext = ExtensionStorage::new();

    assert!(
        tracker.after_prediction(&mut ext).is_ok(),
        "after_prediction should succeed"
    );
    assert!(
        tracker.after_prediction(&mut ext).is_ok(),
        "after_prediction should succeed"
    );

    let state = ext.get_or_insert_default::<TokenTrackerState>();
    assert_eq!(state.prediction_cycles, 2);
}

// ---------------------------------------------------------------------------
// ContextOptimizer
// ---------------------------------------------------------------------------

#[test]
fn context_optimizer_triggers_at_threshold() {
    let optimizer = ContextOptimizer::with_threshold(10, 80);
    let mut ext = ExtensionStorage::new();
    let history: Vec<ChatMessage> = (0..9)
        .map(|i| ChatMessage::User {
            content: format!("msg{}", i),
        })
        .collect();

    // 9 messages >= 80% of 10 = 8, so it SHOULD trigger.
    let decision = match optimizer.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(
        matches!(
            decision,
            PolicyDecision::RequestEffect(Effect::TriggerSummarization)
        ),
        "should trigger summarization at 80% of 10 messages (threshold=8, history=9)"
    );

    let state = ext.get_or_insert_default::<ContextOptimizerState>();
    assert_eq!(state.summarizations_triggered, 1);
}

#[test]
fn context_optimizer_allows_below_threshold() {
    let optimizer = ContextOptimizer::with_threshold(100, 85);
    let mut ext = ExtensionStorage::new();
    let history: Vec<ChatMessage> = (0..10)
        .map(|i| ChatMessage::User {
            content: format!("msg{}", i),
        })
        .collect();

    let decision = match optimizer.before_prediction(&history, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "before_prediction failed: {}", err);
            return;
        }
    };
    assert!(matches!(decision, PolicyDecision::Allow));
}

// ---------------------------------------------------------------------------
// ToolTimeoutPolicy
// ---------------------------------------------------------------------------

#[test]
fn std_tool_timeout_policy_applies_default() {
    let policy = ToolTimeoutPolicy::with_default_timeout(15000);
    let mut ext = ExtensionStorage::new();
    let mut calls = vec![ToolCallDescriptor {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        arguments: "{}".into(),
        timeout_ms: None,
    }];

    assert!(
        policy.on_tool_calls(&mut calls, &mut ext).is_ok(),
        "on_tool_calls should succeed"
    );
    assert_eq!(calls[0].timeout_ms, Some(15000));
}

#[test]
fn std_tool_timeout_policy_bounds_max() {
    let policy = ToolTimeoutPolicy::with_bounds(10000, 20000);
    let mut ext = ExtensionStorage::new();
    let mut calls = vec![ToolCallDescriptor {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        arguments: "{}".into(),
        timeout_ms: Some(50000),
    }];

    assert!(
        policy.on_tool_calls(&mut calls, &mut ext).is_ok(),
        "on_tool_calls should succeed"
    );
    assert_eq!(calls[0].timeout_ms, Some(20000));
}

// ---------------------------------------------------------------------------
// ToolResultFormatter
// ---------------------------------------------------------------------------

#[test]
fn tool_result_formatter_truncates_oversized() {
    let formatter = ToolResultFormatter::with_max_result_bytes(10);
    let mut ext = ExtensionStorage::new();
    let mut results = vec![ToolResultDTO {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        outcome: ToolOutcome::Success("this is a very long result".into()),
    }];

    assert!(
        formatter.on_tool_result(&mut results, &mut ext).is_ok(),
        "on_tool_result should succeed"
    );

    let state = ext.get_or_insert_default::<ToolResultFormatterState>();
    assert_eq!(state.formatted_count, 1);

    if let ToolOutcome::Success(content) = &results[0].outcome {
        assert!(content.contains("truncated"));
        assert!(content.contains("original_len"));
    } else {
        assert_eq!(1, 0, "expected Success outcome after truncation");
    }
}

#[test]
fn tool_result_formatter_passes_small_results() {
    let formatter = ToolResultFormatter::with_max_result_bytes(100);
    let mut ext = ExtensionStorage::new();
    let mut results = vec![ToolResultDTO {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        outcome: ToolOutcome::Success("short".into()),
    }];

    assert!(
        formatter.on_tool_result(&mut results, &mut ext).is_ok(),
        "on_tool_result should succeed"
    );

    let state = ext.get_or_insert_default::<ToolResultFormatterState>();
    assert_eq!(state.formatted_count, 1);
}

// ---------------------------------------------------------------------------
// PendingTaskManager
// ---------------------------------------------------------------------------

#[test]
fn pending_task_manager_detects_pending_marker() {
    let manager = PendingTaskManager::with_default_check_after_ms(3000);
    let mut ext = ExtensionStorage::new();
    let mut results = vec![ToolResultDTO {
        tool_id: "task_1".into(),
        tool_name: "long_task".into(),
        outcome: ToolOutcome::Success("__PENDING__ handle=abc".into()),
    }];

    assert!(
        manager.on_tool_result(&mut results, &mut ext).is_ok(),
        "on_tool_result should succeed"
    );

    let state = ext.get_or_insert_default::<PendingTaskState>();
    assert!(state.pending.contains_key("task_1"));
    assert_eq!(state.pending["task_1"].check_after_ms, 3000);
}

#[test]
fn pending_task_manager_ignores_non_pending() {
    let manager = PendingTaskManager::with_default_check_after_ms(3000);
    let mut ext = ExtensionStorage::new();
    let mut results = vec![ToolResultDTO {
        tool_id: "task_1".into(),
        tool_name: "quick_task".into(),
        outcome: ToolOutcome::Success("done".into()),
    }];

    assert!(
        manager.on_tool_result(&mut results, &mut ext).is_ok(),
        "on_tool_result should succeed"
    );

    let state = ext.get_or_insert_default::<PendingTaskState>();
    assert!(state.pending.is_empty());
}

// ---------------------------------------------------------------------------
// GcPolicy
// ---------------------------------------------------------------------------

#[test]
fn gc_policy_counts_cycles() {
    let policy = GcPolicy::with_cycle_interval(5);
    let mut ext = ExtensionStorage::new();

    for _ in 0..5 {
        // Inject a non-idle snapshot so GC doesn't trigger early.
        ext.insert(brioche_core::SessionSnapshot {
            current_state: brioche_core::AgentStateTag::Predicting,
            state_stack_depth: 0,
        });
        assert!(
            policy.after_prediction(&mut ext).is_ok(),
            "after_prediction should succeed"
        );
    }

    let state = ext.get_or_insert_default::<GcPolicyState>();
    assert_eq!(state.cycles_since_gc, 5);
    assert_eq!(state.gcs_triggered, 0);
}

#[test]
fn gc_policy_respects_idle_flag() {
    let policy = GcPolicy::with_cycle_interval(2);
    let mut ext = ExtensionStorage::new();

    // First cycle: not idle, no trigger.
    assert!(
        policy.after_prediction(&mut ext).is_ok(),
        "after_prediction should succeed"
    );
    let state = ext.get_or_insert_default::<GcPolicyState>();
    assert_eq!(state.gcs_triggered, 0);

    // Second cycle: simulate idle by injecting SessionSnapshot.
    ext.insert(brioche_core::SessionSnapshot {
        current_state: brioche_core::AgentStateTag::Idle,
        state_stack_depth: 0,
    });
    assert!(
        policy.after_prediction(&mut ext).is_ok(),
        "after_prediction should succeed"
    );
    let state = ext.get_or_insert_default::<GcPolicyState>();
    assert_eq!(state.gcs_triggered, 1);
    assert_eq!(state.cycles_since_gc, 0);
}

// ---------------------------------------------------------------------------
// AuditLogger
// ---------------------------------------------------------------------------

#[test]
fn audit_logger_batches_entries() {
    let logger = AuditLogger::with_batch_size(2);
    let mut ext = ExtensionStorage::new();

    // First input: batch not full.
    let decision = match logger.on_input(&EngineInput::UserMessage("hello".into()), &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "on_input failed: {}", err);
            return;
        }
    };
    assert!(matches!(decision, PolicyDecision::Allow));

    // Second input: batch full, triggers SavePluginBlob.
    let decision = match logger.on_input(&EngineInput::UserMessage("world".into()), &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "on_input failed: {}", err);
            return;
        }
    };
    assert!(
        matches!(
            decision,
            PolicyDecision::RequestEffect(Effect::SavePluginBlob { .. })
        ),
        "audit logger should request SavePluginBlob when batch is full"
    );

    let state = ext.get_or_insert_default::<AuditLoggerState>();
    assert_eq!(state.total_logged, 2);
    assert!(state.pending.is_empty());
}

#[test]
fn audit_logger_sequences_entries() {
    let logger = AuditLogger::with_batch_size(10);
    let mut ext = ExtensionStorage::new();

    for i in 0..5 {
        let result = logger.on_input(&EngineInput::UserMessage(format!("msg{}", i)), &mut ext);
        assert!(result.is_ok(), "on_input should succeed for msg{}", i);
    }

    let state = ext.get_or_insert_default::<AuditLoggerState>();
    assert_eq!(state.pending.len(), 5);
    for (i, entry) in state.pending.iter().enumerate() {
        assert_eq!(entry.sequence as usize, i);
    }
}

// ---------------------------------------------------------------------------
// End-to-end: engine with standard plugins
// ---------------------------------------------------------------------------

#[test]
fn engine_with_all_std_plugins_runs_user_message() {
    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(CircuitBreaker::default()))
        .with_plugin(Box::new(TokenTracker::new()))
        .with_plugin(Box::new(ContextOptimizer::default()))
        .with_plugin(Box::new(ToolTimeoutPolicy::default()))
        .with_plugin(Box::new(ToolResultFormatter::default()))
        .with_plugin(Box::new(PendingTaskManager::default()))
        .with_plugin(Box::new(GcPolicy::default()))
        .with_plugin(Box::new(AuditLogger::default()))
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(matches!(session.state, AgentState::Predicting { .. }));
    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
}

#[test]
fn engine_after_prediction_hooks_fire_on_stream_done() {
    let mut engine = BriocheEngineBuilder::new()
        .with_plugin(Box::new(TokenTracker::new()))
        .with_plugin(Box::new(GcPolicy::default()))
        .with_decision_aggregator(Box::new(LexicographicDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(SubRoutineCleanupGuard::new()))
        .build();

    let mut session = Session::new("test");

    // Enter Predicting state.
    let _ = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));
    assert!(matches!(session.state, AgentState::Predicting { .. }));

    // Send stream done.
    let effects = engine.transition(&mut session, &EngineInput::LlmStream(StreamEvent::Done));

    // Should return to Idle and emit SystemIdle.
    assert!(matches!(session.state, AgentState::Idle));
    assert!(effects.iter().any(|e| matches!(e, Effect::SystemIdle)));

    // TokenTracker and GcPolicy should have incremented their counters.
    let tracker_state = session
        .extensions
        .get_or_insert_default::<TokenTrackerState>();
    assert_eq!(tracker_state.prediction_cycles, 1);

    let gc_state = session.extensions.get_or_insert_default::<GcPolicyState>();
    assert_eq!(gc_state.cycles_since_gc, 1);
}
