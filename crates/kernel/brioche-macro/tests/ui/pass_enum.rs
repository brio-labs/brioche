use brioche_core::{BriocheExtensionType, SnapshotStrategy};
use brioche_core::serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
pub enum Event {
    #[default]
    Unit,
    Tuple(u64, String),
    Struct { x: u32, y: String },
}

fn main() {
    assert_eq!(
        <Event as BriocheExtensionType>::EXT_ID,
        concat!(module_path!(), "::Event")
    );
    assert_eq!(
        <Event as BriocheExtensionType>::snapshot_strategy(),
        SnapshotStrategy::FullClone
    );
}
