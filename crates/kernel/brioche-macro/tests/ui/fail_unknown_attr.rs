use brioche_core::BriocheExtensionType;
use std::collections::BTreeMap;

#[derive(Clone, BriocheExtensionType)]
#[brioche(unknwon_typo)]
pub struct BadState {
    pub data: BTreeMap<String, u64>,
}

fn main() {}
