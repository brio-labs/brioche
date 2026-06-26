use brioche_core::{
    BriochePlugin, EngineInput, ExtensionStorage, PluginCapabilities, PluginResult, PolicyDecision,
};
use brioche_macro::brioche_plugin;

struct ExamplePlugin;

#[brioche_plugin(
    name = "example_plugin",
    capabilities = "ON_INPUT | BEFORE_PREDICTION",
    priority = 10
)]
impl BriochePlugin for ExamplePlugin {
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
    assert!(plugin.capabilities().contains(PluginCapabilities::ON_INPUT));
    assert!(plugin
        .capabilities()
        .contains(PluginCapabilities::BEFORE_PREDICTION));
    assert_eq!(plugin.priority(), 10);
}
