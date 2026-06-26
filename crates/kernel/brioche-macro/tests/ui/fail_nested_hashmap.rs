use brioche_core::BriocheExtensionType;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct BadState {
    /// Nested HashMap inside an ordered collection still leaks
    /// non-deterministic iteration order.
    pub items: BTreeMap<String, HashMap<String, u64>>,
}

fn main() {}
