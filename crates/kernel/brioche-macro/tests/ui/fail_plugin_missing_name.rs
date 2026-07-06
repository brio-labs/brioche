use brioche_macro::brioche_plugin;

struct MissingNamePlugin;

#[brioche_plugin(capabilities = "ON_INPUT")]
impl OnInput for MissingNamePlugin {}

fn main() {}
