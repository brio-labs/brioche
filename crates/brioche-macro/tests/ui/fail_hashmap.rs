use brioche_core::BriocheExtensionType;
use std::collections::HashMap;

#[derive(Clone, BriocheExtensionType)]
pub struct BadState {
    pub data: HashMap<String, u64>,
}

fn main() {}
