use brioche_core::BriocheExtensionType;
use std::collections::HashMap;

#[derive(Clone, BriocheExtensionType)]
pub enum BadEnum {
    Variant { data: HashMap<String, u64> },
}

fn main() {}
