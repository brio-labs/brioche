use brioche_core::BriocheExtensionType;
use brioche_core::serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct Nested {
    #[brioche(deterministic_order)]
    pub data: BTreeMap<String, Vec<u8>>,
}

fn main() {
    assert_eq!(
        <Nested as BriocheExtensionType>::EXT_ID,
        concat!(module_path!(), "::Nested")
    );
}
