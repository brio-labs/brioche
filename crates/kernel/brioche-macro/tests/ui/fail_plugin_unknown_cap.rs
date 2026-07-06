use brioche_macro::brioche_plugin;

struct UnknownCapPlugin;

#[brioche_plugin(name = "x", capabilities = "UNKNOWN")]
impl OnInput for UnknownCapPlugin {}

fn main() {}
