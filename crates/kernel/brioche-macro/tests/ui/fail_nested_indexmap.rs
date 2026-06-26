use brioche_core::BriocheExtensionType;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// Placeholder type with the same path segment name as `indexmap::IndexMap`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct IndexMap<K, V>(PhantomData<(K, V)>);

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct BadState {
    /// IndexMap requires an explicit determinism certification.
    pub items: IndexMap<String, u64>,
}

fn main() {}
