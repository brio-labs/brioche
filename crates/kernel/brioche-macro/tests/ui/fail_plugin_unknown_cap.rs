use brioche_core::BriochePlugin;
use brioche_macro::brioche_plugin;

struct UnknownCapPlugin;

#[brioche_plugin(name = "x", capabilities = "UNKNOWN")]
impl BriochePlugin for UnknownCapPlugin {}

fn main() {}
