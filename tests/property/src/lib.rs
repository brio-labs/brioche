//! Cross-crate property tests.
//!
//! Verifies deterministic invariants of the Brioche kernel when wired with
//! default governance profiles.
//!
//! Refs: I-Core-Pure, I-Core-NoPanic

#![cfg(test)]

use brioche_core::{BriocheEngineBuilder, EngineInput, Session};
use brioche_governance_default::{BriocheEngineBuilderExt, GovernanceProfile};
use proptest::prelude::*;

/// Build a deterministic engine using the permissive profile.
fn build_permissive_engine() -> brioche_core::BriocheEngine {
    BriocheEngineBuilder::new()
        .with_profile(GovernanceProfile::Permissive)
        .build()
}

/// Strategy generating arbitrary user messages.
fn user_message_strategy() -> impl Strategy<Value = EngineInput> {
    "[a-zA-Z0-9 ]{0,64}".prop_map(EngineInput::UserMessage)
}

proptest! {
    /// Identical inputs produce identical effects from fresh sessions.
    #[test]
    fn transition_is_deterministic(input in user_message_strategy()) {
        let mut engine_a = build_permissive_engine();
        let mut engine_b = build_permissive_engine();
        let mut session_a = Session::new("property_determinism");
        let mut session_b = Session::new("property_determinism");

        let effects_a = engine_a.transition(&mut session_a, &input);
        let effects_b = engine_b.transition(&mut session_b, &input);

        assert_eq!(effects_a, effects_b);
    }
}
