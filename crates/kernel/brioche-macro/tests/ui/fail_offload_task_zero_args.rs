use brioche_macro::brioche_offload_task;

#[brioche_offload_task]
fn no_args() -> u64 {
    0
}

fn main() {}
