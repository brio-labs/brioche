use brioche_core::BriocheExtensionType;
use std::collections::HashSet;

#[derive(Clone, BriocheExtensionType)]
pub struct BadState {
    pub tags: HashSet<String>,
}

fn main() {}
