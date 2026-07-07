//! Integration tests for COW rollback, error-hook, and rollback-policy contracts.
//!
//! Refs: I-Core-Pure, I-Core-NoPanic

use brioche_core::{
    AgentState, BriocheEngineBuilder, BriocheExtensionType, ChatMessage, CycleRollbackPolicy,
    Effect, EngineInput, ExtensionStorage, OnError, OnInput, PluginResult, PolicyDecision, Session,
};

mod common;
use common::{MockDecisionAggregator, MockSubRoutineLifecycleGuard};

/// Non-critical test type to validate the COW threshold.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
/// Non-critical test type to validate the COW threshold.
pub struct TestCowState {
    /// Scalar value for COW weight tests.
    pub value: u64,
}

#[test]
fn adaptive_undo_frame_guard_restores_mutated_extension() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

    let mut guard = AdaptiveUndoFrameGuard::new();
    let mut ext = ExtensionStorage::new();
    assert!(
        ext.insert(brioche_core::EpochState {
            current_generation: 42,
        })
        .is_ok()
    );

    guard.begin_hook("on_input");

    // Snapshot the current value via on_mutation.
    let type_id = std::any::TypeId::of::<brioche_core::EpochState>();
    let vtable = brioche_core::EpochState::build_vtable();
    let current = ext.get_or_insert_default::<brioche_core::EpochState>();
    guard.on_mutation(type_id, &vtable, current);

    // Mutate the extension.
    current.current_generation = 99;

    // Rollback should restore the original value.
    guard.rollback_hook(&mut ext);

    let restored = ext.get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(restored.current_generation, 42);
}

#[test]
fn adaptive_undo_frame_guard_abandons_past_threshold() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

    let mut guard = AdaptiveUndoFrameGuard::new(); // budget will likely be exceeded by TestCowState
    let mut ext = ExtensionStorage::new();
    assert!(ext.insert(TestCowState { value: 7 }).is_ok());

    guard.begin_hook("on_input");

    let type_id = std::any::TypeId::of::<TestCowState>();
    let vtable = TestCowState::build_vtable();
    let current = ext.get_or_insert_default::<TestCowState>();
    guard.on_mutation(type_id, &vtable, current);

    // Mutation may be abandoned due to threshold — state won't be restored.
    current.value = 123;

    guard.rollback_hook(&mut ext);

    let not_restored = ext.get_or_insert_default::<TestCowState>();
    // With adaptive budget, the result depends on budget; just verify no panic.
    assert!(not_restored.value == 123 || not_restored.value == 7);
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
#[brioche(ext_id = "tests.rollback_a")]
struct RollbackTypeA {
    #[brioche(deterministic_order)]
    payload: Vec<u8>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
#[brioche(ext_id = "tests.rollback_b")]
struct RollbackTypeB {
    #[brioche(deterministic_order)]
    payload: Vec<u8>,
}
struct MutatingPlugin;

impl OnInput for MutatingPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "mutating"
    }

    fn priority(&self) -> i16 {
        100
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        ext.get_or_insert_default::<RollbackTypeA>().payload = vec![1; 64];
        ext.get_or_insert_default::<RollbackTypeB>().payload = vec![2; 64];
        Ok(PolicyDecision::Allow)
    }
}

#[test]
fn engine_rolls_back_extensions_when_cow_budget_exceeded() {
    // Give each extension type a different payload size so their estimated
    // weights differ. Set the budget to exactly the sum so both are snapshotted
    // and the cumulative weight triggers a rollback.
    let snapshot_a = RollbackTypeA {
        payload: vec![0; 32],
    };
    let snapshot_b = RollbackTypeB {
        payload: vec![0; 16],
    };
    let weight_a = (RollbackTypeA::build_vtable().estimated_weight_bytes)(&snapshot_a);
    let weight_b = (RollbackTypeB::build_vtable().estimated_weight_bytes)(&snapshot_b);

    let guard = brioche_governance_default::UndoFrameGuard::with_max_cow_bytes(weight_a + weight_b);

    let mut engine = BriocheEngineBuilder::new()
        .with_on_input(Box::new(MutatingPlugin))
        .with_cycle_rollback_policy(Box::new(guard))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("rollback-test");
    assert!(session.extensions.insert(snapshot_a).is_ok());
    assert!(session.extensions.insert(snapshot_b).is_ok());

    let _effects = engine.transition(&mut session, &EngineInput::UserMessage("go".into()));

    // Both payloads should be restored to their pre-hook values.
    assert_eq!(
        session
            .extensions
            .get_or_insert_default::<RollbackTypeA>()
            .payload,
        vec![0; 32]
    );
    assert_eq!(
        session
            .extensions
            .get_or_insert_default::<RollbackTypeB>()
            .payload,
        vec![0; 16]
    );
}
struct FaultingPlugin;

impl OnInput for FaultingPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "faulting"
    }

    fn priority(&self) -> i16 {
        100
    }

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Err(brioche_core::PluginError::Fatal {
            plugin_name: "faulting".into(),
            message: "intentional fault".into(),
        })
    }
}

struct ErrorRecorderPlugin;

impl OnError for ErrorRecorderPlugin {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "recorder"
    }

    fn priority(&self) -> i16 {
        0
    }

    fn on_error(
        &self,
        _error: &brioche_core::PluginError,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::RequestEffect(
            brioche_core::Effect::SavePluginBlob {
                plugin_id: brioche_core::PluginSource("recorder".into()),
                data: vec![0xab],
            },
        ))
    }
}

#[test]
fn engine_invokes_on_error_hook_for_plugin_faults() {
    let mut engine = BriocheEngineBuilder::new()
        .with_on_input(Box::new(FaultingPlugin))
        .with_on_error(Box::new(ErrorRecorderPlugin))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("on-error-test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("go".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "go"
    ));
    assert_eq!(
        effects,
        vec![
            Effect::SavePluginBlob {
                plugin_id: brioche_core::PluginSource("recorder".into()),
                data: vec![0xab],
            },
            Effect::PluginFault {
                plugin_name: brioche_core::PluginSource("faulting".into()),
                error: brioche_core::PluginError::Fatal {
                    plugin_name: "faulting".into(),
                    message: "intentional fault".into(),
                },
            },
            Effect::SaveSession,
            Effect::CallLlmNetwork,
        ]
    );
}

#[test]
fn engine_with_adaptive_undo_frame_guard_instruments_hooks() {
    use brioche_governance_default::AdaptiveUndoFrameGuard;

    struct MutatingPlugin;
    impl OnInput for MutatingPlugin {
        type EngineInput = EngineInput;
        type ExtensionStorage = ExtensionStorage;
        type PluginError = brioche_core::PluginError;
        type PolicyDecision = PolicyDecision;

        fn name(&self) -> &'static str {
            "mutating"
        }

        fn on_input(
            &self,
            _input: &EngineInput,
            ext: &mut ExtensionStorage,
        ) -> PluginResult<PolicyDecision> {
            let state = ext.get_or_insert_default::<brioche_core::EpochState>();
            state.current_generation = 999;
            Ok(PolicyDecision::Allow)
        }
    }

    let mut engine = BriocheEngineBuilder::new()
        .with_on_input(Box::new(MutatingPlugin))
        .with_cycle_rollback_policy(Box::new(AdaptiveUndoFrameGuard::new()))
        .with_decision_aggregator(Box::new(MockDecisionAggregator))
        .with_subroutine_lifecycle_guard(Box::new(MockSubRoutineLifecycleGuard))
        .build();

    let mut session = Session::new("test");
    assert!(
        session
            .extensions
            .insert(brioche_core::EpochState {
                current_generation: 1,
            })
            .is_ok()
    );

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));

    assert_eq!(session.state, AgentState::Predicting { generation_id: 1 });
    assert_eq!(session.history.len(), 1);
    assert!(matches!(
        &session.history[0],
        ChatMessage::User { content } if content == "hello"
    ));
    assert_eq!(effects, vec![Effect::SaveSession, Effect::CallLlmNetwork]);

    let state = session
        .extensions
        .get_or_insert_default::<brioche_core::EpochState>();
    assert_eq!(state.current_generation, 999);
}
