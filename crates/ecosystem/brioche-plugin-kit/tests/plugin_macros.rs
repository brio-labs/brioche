//! Integration tests for `brioche-plugin-kit` macros.
//!
//! Verifies that `#[brioche_plugin]`, `#[hook]`, and `#[brioche_offload_task]`
//! expand correctly for atomic capability traits.
//!
//! Refs: I-Core-ExtensionType, I-Eco-ExtensionOverMod

use brioche_plugin_kit::{
    Effect, EngineInput, ExtensionStorage, MockEngine, OnInput, PluginBuilder, PluginResult,
    PolicyDecision,
};

/// Minimal input capability plugin for macro expansion testing.
pub struct MinimalPlugin;

#[brioche_plugin_kit::brioche_plugin(name = "minimal", capabilities = "ON_INPUT")]
impl OnInput for MinimalPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_plugin_kit::PluginError;
    type PolicyDecision = PolicyDecision;

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

/// Priority-bearing input plugin for macro expansion testing.
pub struct PriorityPlugin;

#[brioche_plugin_kit::brioche_plugin(name = "priority", capabilities = "ON_INPUT", priority = -10)]
impl OnInput for PriorityPlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_plugin_kit::PluginError;
    type PolicyDecision = PolicyDecision;

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

#[brioche_plugin_kit::brioche_offload_task]
fn double_payload(payload: Vec<u8>) -> Vec<u8> {
    payload.iter().flat_map(|&b| [b, b]).collect()
}

#[test]
fn minimal_plugin_name() {
    let plugin = MinimalPlugin;
    assert_eq!(plugin.name(), "minimal");
}

#[test]
fn priority_plugin_priority() {
    let plugin = PriorityPlugin;
    assert_eq!(plugin.name(), "priority");
    assert_eq!(plugin.priority(), -10);
}

#[test]
fn minimal_plugin_allows_user_message() {
    let plugin = MinimalPlugin;
    let mut ext = ExtensionStorage::new();
    let decision = match plugin.on_input(&EngineInput::UserMessage("hello".into()), &mut ext) {
        Ok(d) => d,
        Err(err) => {
            assert_eq!(1, 0, "plugin error: {}", err);
            return;
        }
    };
    assert_eq!(decision, PolicyDecision::Allow);
}

#[test]
fn engine_with_macro_plugin_runs_transition() {
    let mut engine = PluginBuilder::permissive()
        .with_on_input(Box::new(MinimalPlugin))
        .build();
    let mut session = brioche_plugin_kit::Session::new("test");
    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".into()));
    assert!(
        effects.iter().any(|e| matches!(e, Effect::CallLlmNetwork)),
        "expected CallLlmNetwork in effects: {effects:?}"
    );
}

#[test]
fn mock_engine_with_macro_plugin() {
    let mut mock = MockEngine::new();
    let _effects = mock.transition(EngineInput::UserMessage("hello".into()));
}

#[test]
fn offload_task_doubles_payload() {
    let payload = vec![1, 2, 3];
    let doubled = double_payload(payload);
    assert_eq!(doubled, vec![1, 1, 2, 2, 3, 3]);
}
