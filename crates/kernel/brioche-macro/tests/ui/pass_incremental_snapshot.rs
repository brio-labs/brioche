use brioche_core::{BriocheExtensionType, SnapshotStrategy};
use brioche_core::serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(incremental_snapshot)]
pub struct IncrementalState;

fn main() {
    assert_eq!(
        <IncrementalState as BriocheExtensionType>::snapshot_strategy(),
        SnapshotStrategy::Incremental
    );
}
