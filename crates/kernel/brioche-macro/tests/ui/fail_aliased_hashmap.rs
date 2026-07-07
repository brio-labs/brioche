use brioche_core::BriocheExtensionType;

type BadMap<K, V> = std::collections::HashMap<K, V>;

#[derive(Clone, BriocheExtensionType)]
pub struct BadState {
    pub data: BadMap<String, u64>,
}

fn main() {}
