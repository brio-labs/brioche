use brioche_core::BriocheExtensionType;

pub struct TauriWindow;

#[derive(Clone, BriocheExtensionType)]
pub struct BadState {
    pub window: TauriWindow,
}

fn main() {}
