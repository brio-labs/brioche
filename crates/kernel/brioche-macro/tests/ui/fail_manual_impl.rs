use brioche_core::BriocheExtensionType;

pub struct EvilState;

impl BriocheExtensionType for EvilState {
    const EXT_ID: &'static str = "evil";
    fn estimated_weight_bytes(&self) -> usize {
        0
    }
    fn snapshot_strategy() -> brioche_core::SnapshotStrategy {
        brioche_core::SnapshotStrategy::FullClone
    }
}

fn main() {}
