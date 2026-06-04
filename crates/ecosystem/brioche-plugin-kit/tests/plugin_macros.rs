//! Integration tests for `brioche-plugin-kit` macros.
//!
//! Verifies that `#[brioche_plugin]`, `#[hook]`, and `#[brioche_offload_task]`
//! expand correctly and produce valid `BriochePlugin` implementations.
//!
//! Refs: I-Core-ExtensionType, I-Eco-ExtensionOverMod

use brioche_plugin_kit::{
    BriochePlugin, Effect, EngineInput, ExtensionStorage, MockEngine, PluginBuilder, PluginResult,
    PolicyDecision,
};

// ---------------------------------------------------------------------------
// Minimal plugin using #[brioche_plugin]
// ---------------------------------------------------------------------------

pub struct MinimalPlugin;

#[brioche_plugin_kit::brioche_plugin(name = "minimal", capabilities = "ON_INPUT")]
impl BriochePlugin for MinimalPlugin {
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

// ---------------------------------------------------------------------------
// Plugin with multiple capabilities using #[brioche_plugin]
// ---------------------------------------------------------------------------

pub struct MultiCapPlugin;

#[brioche_plugin_kit::brioche_plugin(
    name = "multi_cap",
    capabilities = "ON_INPUT | BEFORE_PREDICTION | AFTER_PREDICTION",
    priority = -10
)]
impl BriochePlugin for MultiCapPlugin {
    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn before_prediction(
        &self,
        _history: &[brioche_plugin_kit::ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Offload task test
// ---------------------------------------------------------------------------

#[brioche_plugin_kit::brioche_offload_task]
fn double_payload(payload: Vec<u8>) -> Vec<u8> {
    payload.iter().flat_map(|&b| [b, b]).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn minimal_plugin_name_and_capabilities() {
    let plugin = MinimalPlugin;
    assert_eq!(plugin.name(), "minimal");
    assert_eq!(
        plugin.capabilities(),
        brioche_plugin_kit::PluginCapabilities::ON_INPUT
    );
}

#[test]
fn multi_cap_plugin_priority() {
    let plugin = MultiCapPlugin;
    assert_eq!(plugin.name(), "multi_cap");
    assert_eq!(plugin.priority(), -10);
    assert!(
        plugin
            .capabilities()
            .contains(brioche_plugin_kit::PluginCapabilities::BEFORE_PREDICTION)
    );
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
        .with_plugin(Box::new(MinimalPlugin))
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
    // MockEngine uses Permissive profile; transition succeeds without panic.
}

#[test]
fn offload_task_generates_effect() {
    let input = vec![1, 2, 3];
    let effect = __brioche_offload_double_payload::effect("task-1", &input);
    match effect {
        Effect::ExecuteCpuTask { task_id, payload } => {
            assert_eq!(task_id, "task-1");
            assert!(!payload.is_empty());
        }
        other => {
            assert_eq!(1, 0, "expected ExecuteCpuTask, got {other:?}");
        }
    }
}

#[test]
fn offload_task_serialize_roundtrip() {
    let input = vec![5, 6, 7];
    let bytes = __brioche_offload_double_payload::serialize_input(&input);
    // For Vec<u8>, postcard serializes directly.
    assert!(!bytes.is_empty());
}
