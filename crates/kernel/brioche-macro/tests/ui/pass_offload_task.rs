use brioche_core::{Effect, TaskId};
use brioche_macro::brioche_offload_task;

#[brioche_offload_task]
fn add_one(x: u64) -> u64 {
    x + 1
}

fn main() {
    let input = 5u64;
    let output = add_one(input);

    // Round-trip the output payload through the generated helpers.
    let payload = __brioche_offload_add_one::serialize_input(&output);
    let roundtrip = __brioche_offload_add_one::deserialize_output(&payload);
    assert_eq!(roundtrip, output);

    // Verify the `effect` helper builds the expected `Effect::ExecuteCpuTask`.
    let effect = __brioche_offload_add_one::effect("task-1", &input);
    let expected = Effect::ExecuteCpuTask {
        task_id: TaskId::from("task-1"),
        payload: __brioche_offload_add_one::serialize_input(&input),
    };
    assert_eq!(effect, expected);
}
