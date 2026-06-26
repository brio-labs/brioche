use brioche_core::BriochePlugin;
use brioche_macro::brioche_plugin;

struct MissingNamePlugin;

#[brioche_plugin(capabilities = "ON_INPUT")]
impl BriochePlugin for MissingNamePlugin {}

fn main() {}
