use brioche_core::BriocheExtensionType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub struct BadState {
    pub history: Vec<String>,
}

fn main() {}
