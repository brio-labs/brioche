use brioche_core::BriocheExtensionType;
use std::collections::BTreeMap;

#[derive(Clone, BriocheExtensionType)]
pub struct TokenTrackerState {
    pub total_input_tokens: u64,
    pub counts: BTreeMap<String, u64>,
}

#[derive(Clone, BriocheExtensionType)]
#[brioche(critical_state)]
pub struct EpochState {
    pub current_generation: u64,
}

#[derive(Clone, BriocheExtensionType)]
#[brioche(no_snapshot)]
pub struct VolatileCache {
    pub temp_data: String,
}

fn main() {
    let s = TokenTrackerState {
        total_input_tokens: 0,
        counts: BTreeMap::new(),
    };
    assert_eq!(s.estimated_weight_bytes(), std::mem::size_of_val(&s));
    assert_eq!(
        <TokenTrackerState as BriocheExtensionType>::EXT_ID,
        concat!(module_path!(), "::TokenTrackerState")
    );
}
