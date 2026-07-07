//! Shared mock governance traits for core integration suites.

use brioche_core::{
    DecisionAggregator, Effect, ExtensionStorage, PluginResult, PolicyDecision, Session,
    SessionRegistry, SubRoutineHandle, SubRoutineLifecycleGuard,
};

pub struct MockDecisionAggregator;

impl DecisionAggregator for MockDecisionAggregator {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn aggregate_decisions(
        &self,
        decisions: Vec<PolicyDecision>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // First Block wins; accumulate MutateHistory; OverrideTransition/RequestEffect short-circuit.
        let mut edits = Vec::new();
        for d in decisions {
            match d {
                PolicyDecision::Block { reason } => {
                    return Ok(PolicyDecision::Block { reason });
                }
                PolicyDecision::MutateHistory(mut e) => {
                    edits.append(&mut e);
                }
                PolicyDecision::OverrideTransition(eff) => {
                    return Ok(PolicyDecision::OverrideTransition(eff));
                }
                PolicyDecision::RequestEffect(eff) => {
                    return Ok(PolicyDecision::RequestEffect(eff));
                }
                PolicyDecision::Allow => {}
                _ => {}
            }
        }
        if edits.is_empty() {
            Ok(PolicyDecision::Allow)
        } else {
            Ok(PolicyDecision::MutateHistory(edits))
        }
    }
}

pub struct MockSubRoutineLifecycleGuard;

impl SubRoutineLifecycleGuard for MockSubRoutineLifecycleGuard {
    type Effect = Effect;
    type PluginError = brioche_core::PluginError;
    type Session = Session;
    type SessionRegistry = SessionRegistry;
    type SubRoutineHandle = SubRoutineHandle;

    fn on_exit(
        &self,
        _handle: SubRoutineHandle,
        _parent: &mut Session,
        _registry: &mut SessionRegistry,
    ) -> PluginResult<Vec<Effect>> {
        Ok(vec![])
    }
}
