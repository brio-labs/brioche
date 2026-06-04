//! `MockEngine` — test utility for plugin authors.
//!
//! Provides a pre-wired `BriocheEngine` with all mandatory governance
//! traits and a fresh `Session`, eliminating boilerplate in unit tests.
//!
//! # Example
//! ```ignore
//! let mut mock = MockEngine::new();
//! let effects = mock.transition(EngineInput::UserMessage("hello".into()));
//! assert!(effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)));
//! ```
//!
//! Refs: I-Eco-ExtensionOverMod

use brioche_core::{BriocheEngine, Effect, EngineInput, Session};

use crate::PluginBuilder;

/// Pre-wired test engine with a fresh session.
///
/// Uses the `Permissive` governance profile so that policy plugins do
/// not interfere with the behavior under test. All mandatory governance
/// traits are injected with no-op or permissive implementations.
pub struct MockEngine {
    engine: BriocheEngine,
    session: Session,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    /// Create a new `MockEngine` with the `Permissive` profile.
    ///
    /// The session id is `"test"`.
    pub fn new() -> Self {
        let (engine, session) = PluginBuilder::permissive().build_with_session("test");
        Self { engine, session }
    }

    /// Create a new `MockEngine` with the `Standard` profile.
    pub fn standard() -> Self {
        let (engine, session) = PluginBuilder::standard().build_with_session("test");
        Self { engine, session }
    }

    /// Create a new `MockEngine` with the `Strict` profile.
    pub fn strict() -> Self {
        let (engine, session) = PluginBuilder::strict().build_with_session("test");
        Self { engine, session }
    }

    /// Execute one transition cycle.
    pub fn transition(&mut self, input: EngineInput) -> Vec<Effect> {
        self.engine.transition(&mut self.session, &input)
    }

    /// Mutable access to the underlying engine.
    pub fn engine(&mut self) -> &mut BriocheEngine {
        &mut self.engine
    }

    /// Mutable access to the session.
    pub fn session(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Immutable access to the session.
    pub fn session_ref(&self) -> &Session {
        &self.session
    }

    /// Consume the mock, returning the engine and session.
    pub fn into_parts(self) -> (BriocheEngine, Session) {
        (self.engine, self.session)
    }
}
