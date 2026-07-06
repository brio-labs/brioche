//! Sprint 8 integration tests: governance profiles and remaining plugins.
//!
//! Invariants verified:
//! - I-Gov-Profile-Agnostic: profiles bootstrap an engine in under 5 lines.
//! - I-Gov-SubRoutineLifecycle-Guard: DepthGuard limits nesting.
//! - I-Gov-Quarantine-Isolate: QuarantineManager emits RebuildRoutes on fatal.
//! - I-Gov-Timeout-Bound: ToolTimeoutPolicy bounds tool timeouts.
//! - I-Gov-Tiered-Rollback: TieredUndoFrameGuard respects critical types.

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriocheExtensionType, BriochePlugin, CycleRollbackPolicy,
    DecisionAggregator, Effect, EngineInput, ErrorCode, ExtensionStorage, PluginError,
    PolicyDecision, Session, StreamEvent, SubRoutineHandle, ToolCallDescriptor,
};
use brioche_governance_default::{
    AdaptiveUndoFrameGuard, BriocheEngineBuilderExt, GovernanceCompatibilityMatrix,
    GovernanceProfile, QuarantineManager, QuarantineState, TelemetryPlugin, TieredUndoFrameGuard,
    ToolTimeoutPolicy, TreeDecisionAggregator,
};

// ---------------------------------------------------------------------------
// Profile bootstrap tests
// ---------------------------------------------------------------------------

#[test]
fn standard_profile_runs_user_message() {
    let mut engine = BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .build();

    let mut session = Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert!(matches!(session.state, AgentState::Predicting { .. }));
    assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
}

// ---------------------------------------------------------------------------
// DepthGuard
// ---------------------------------------------------------------------------

#[test]
fn depth_guard_blocks_at_limit() {
    let mut engine = BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Standard)
        .build();

    let mut session = Session::new("test");
    // Artificially inflate stack depth to exceed limit.
    // Standard profile max_depth = 10. SubRoutine state adds +1.
    for _ in 0..9 {
        session.state_stack.push(AgentState::Idle);
    }
    session.state = AgentState::SubRoutine(SubRoutineHandle::new("sub"));

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("deep".into()));

    // Should be blocked due to depth limit (stack_depth 9 + SubRoutine + 1 = 11 > 10).
    assert!(
        effects.iter().any(
            |e| matches!(e, Effect::Error { code, .. } if *code == ErrorCode::StateInconsistency)
        ),
        "depth guard should block when limit exceeded"
    );
}

// ---------------------------------------------------------------------------
// QuarantineManager
// ---------------------------------------------------------------------------

#[test]
fn quarantine_manager_rebuilds_on_fatal() {
    // Unit-test the quarantine manager directly since the kernel's
    // on_error hook routing is not yet invoked in Sprint 8 engine.
    let manager = QuarantineManager::new();
    let mut ext = ExtensionStorage::new();

    let error = PluginError::Fatal {
        plugin_name: "bad_plugin".into(),
        message: "simulated fatal".into(),
    };

    let decision = match manager.on_error(&error, &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "on_error failed: {}", err);
            return;
        }
    };

    assert!(
        matches!(
            decision,
            PolicyDecision::RequestEffect(Effect::RebuildRoutes)
        ),
        "quarantine manager should request RebuildRoutes on fatal error"
    );

    let state = ext.get_or_insert_default::<QuarantineState>();
    assert!(state.quarantined.contains("bad_plugin"));
}

// ---------------------------------------------------------------------------
// ToolTimeoutPolicy
// ---------------------------------------------------------------------------

#[test]
fn tool_timeout_policy_applies_default() {
    let policy = ToolTimeoutPolicy::with_default_timeout(15000);
    let mut ext = ExtensionStorage::new();
    let mut calls = vec![ToolCallDescriptor {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        arguments: "{}".into(),
        timeout_ms: None,
    }];

    assert!(policy.on_tool_calls(&mut calls, &mut ext).is_ok());
    assert_eq!(calls[0].timeout_ms, Some(15000));
}

#[test]
fn tool_timeout_policy_bounds_max() {
    let policy = ToolTimeoutPolicy::with_bounds(10000, 20000);
    let mut ext = ExtensionStorage::new();
    let mut calls = vec![ToolCallDescriptor {
        tool_id: "t1".into(),
        tool_name: "calc".into(),
        arguments: "{}".into(),
        timeout_ms: Some(50000),
    }];

    assert!(policy.on_tool_calls(&mut calls, &mut ext).is_ok());
    assert_eq!(calls[0].timeout_ms, Some(20000));
}

// ---------------------------------------------------------------------------
// TelemetryPlugin
// ---------------------------------------------------------------------------

#[test]
fn telemetry_plugin_counts_tool_calls() {
    let plugin = TelemetryPlugin::new();
    let mut ext = ExtensionStorage::new();

    let start = StreamEvent::ToolCallStart {
        path: Default::default(),
        id: "tc1".into(),
        name: "calc".into(),
    };
    assert!(plugin.on_stream_event(&start, &mut ext).is_ok());

    let done = StreamEvent::ToolCallDone {
        path: Default::default(),
    };
    assert!(plugin.on_stream_event(&done, &mut ext).is_ok());

    let state =
        ext.get_or_insert_default::<brioche_governance_default::telemetry::ToolCallDetectorState>();
    assert_eq!(state.total_detected, 1);
    assert_eq!(state.total_completed, 1);
}

#[test]
fn telemetry_plugin_observes_after_prediction() {
    let plugin = TelemetryPlugin::new();
    let mut ext = ExtensionStorage::new();
    assert!(plugin.after_prediction(&mut ext).is_ok());
}

// ---------------------------------------------------------------------------
// TieredUndoFrameGuard
// ---------------------------------------------------------------------------

#[test]
fn tiered_undo_frame_guard_restores_critical_type() {
    let mut guard = TieredUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(brioche_core::EpochState {
            current_generation: 42,
        })
        .is_ok()
    );

    guard.begin_hook("on_input");

    let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
    let vtable = brioche_core::EpochState::build_vtable();
    let current = ext.get_or_insert_default::<brioche_core::EpochState>();
    guard.on_mutation(type_id, &vtable, current);

    current.current_generation = 999;

    guard.rollback_hook(&mut ext);

    let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(restored.current_generation, 42);
}

// ---------------------------------------------------------------------------
// TreeDecisionAggregator
// ---------------------------------------------------------------------------

#[test]
fn tree_decision_aggregator_blocks_when_any_block() {
    let aggregator = TreeDecisionAggregator::new();
    let mut ext = ExtensionStorage::new();

    let decisions = vec![
        PolicyDecision::Allow,
        PolicyDecision::Block {
            reason: "blocked".into(),
        },
    ];

    let result = match aggregator.aggregate_decisions(decisions, &mut ext) {
        Ok(r) => r,
        Err(err) => {
            assert_eq!(1, 0, "aggregation failed: {}", err);
            return;
        }
    };

    assert!(
        matches!(result, PolicyDecision::Block { .. }),
        "tree aggregator should return Block when any decision is Block"
    );
}

// ---------------------------------------------------------------------------
// GovernanceCompatibilityMatrix
// ---------------------------------------------------------------------------

#[test]
fn compatibility_matrix_has_entries() {
    let entries = GovernanceCompatibilityMatrix::entries();
    assert!(
        !entries.is_empty(),
        "compatibility matrix should have entries"
    );
}

#[test]
fn compatibility_matrix_lookup_epoch_guard_subroutine_orchestrator() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "EpochInterceptor",
        "EpochGuard",
        "SubRoutineHandler",
        "SubRoutineOrchestrator",
    );
    assert!(level.is_some());
}

// ---------------------------------------------------------------------------
// AdaptiveUndoFrameGuard
// ---------------------------------------------------------------------------

#[test]
fn adaptive_undo_frame_guard_restores_on_budget() {
    let mut guard = AdaptiveUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(brioche_core::EpochState {
            current_generation: 7,
        })
        .is_ok()
    );

    guard.begin_hook("on_input");

    let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
    let vtable = brioche_core::EpochState::build_vtable();
    let current = ext.get_or_insert_default::<brioche_core::EpochState>();
    guard.on_mutation(type_id, &vtable, current);

    current.current_generation = 77;

    guard.rollback_hook(&mut ext);

    let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(restored.current_generation, 7);
}
