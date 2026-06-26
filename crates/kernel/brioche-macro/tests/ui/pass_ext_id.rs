use brioche_core::BriocheExtensionType;
use brioche_core::serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(ext_id = "custom.namespace.id")]
pub struct Tagged;

fn main() {
    assert_eq!(
        <Tagged as BriocheExtensionType>::EXT_ID,
        "custom.namespace.id"
    );
}
