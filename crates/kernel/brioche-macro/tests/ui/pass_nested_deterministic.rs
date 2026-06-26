use brioche_core::BriocheExtensionType;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// A deterministic nested carrier.
#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct InnerCarrier {
    pub value: u64,
}

/// Placeholder type with the same path segment name as `indexmap::IndexMap`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct IndexMap<K, V>(PhantomData<(K, V)>);

/// Outer state wrapping deterministic nested carriers.
#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct OuterState {
    #[brioche(deterministic_order)]
    #[brioche(nested_carrier)]
    pub vec_items: Vec<InnerCarrier>,

    #[brioche(deterministic_order)]
    #[brioche(nested_carrier)]
    pub index_items: IndexMap<String, InnerCarrier>,
}

fn main() {
    let state = OuterState {
        vec_items: vec![InnerCarrier { value: 1 }],
        index_items: IndexMap(PhantomData),
    };
    assert_eq!(state.vec_items.len(), 1);
}
