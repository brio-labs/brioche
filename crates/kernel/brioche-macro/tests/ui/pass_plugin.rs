use brioche_core::{EngineInput, ExtensionStorage, OnInput, PluginResult, PolicyDecision};
use brioche_macro::brioche_plugin;

struct ExamplePlugin;

#[brioche_plugin(
    name = "example_plugin",
    capabilities = "ON_INPUT | BEFORE_PREDICTION",
    priority = 10
)]
impl OnInput for ExamplePlugin {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = brioche_core::PluginError;

    fn on_input(
        &self,
        _input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        Ok(PolicyDecision::Allow)
    }
}

fn main() {
    let plugin = ExamplePlugin;
    assert_eq!(plugin.name(), "example_plugin");
    assert_eq!(plugin.priority(), 10);
}
