use brioche_core::BriocheExtensionType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct BadState {
    /// `nested_carrier` promises a `BriocheExtensionType` carrier, but
    /// `String` does not implement the trait.
    #[brioche(deterministic_order)]
    #[brioche(nested_carrier)]
    pub items: Vec<String>,
}

fn main() {}
