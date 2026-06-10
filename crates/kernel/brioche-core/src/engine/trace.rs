//! Tool-call accumulation, sealing, and transition trace logging.
//!
//! Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget, I-Gov-Trace-Log

use crate::{
    ActiveToolCall, Effect, EpochState, PluginSource, PolicyDecision, Session,
    StreamToolAccumulator, SupersededTransitionTrace, SupersededTransitionTraceLog,
    ToolCallDescriptor, TransitionTrace, TransitionTraceLog,
};

// ---------------------------------------------------------------------------
// ToolCallAccumulator
// ---------------------------------------------------------------------------

/// State machine for accumulating tool-call descriptors from an LLM stream.
///
/// Wraps `StreamToolAccumulator` in `ExtensionStorage` and provides typed
/// methods for each stream event variant. Extracted from `dispatch_llm_stream`
/// to separate the accumulation concern from dispatch orchestration and to
/// enable standalone unit testing of the state machine.
///
/// # State Machine
/// ```text
/// Idle --ToolCallStart--> Accumulating(id)
/// Accumulating(id) --ToolArgumentChunk--> Accumulating(id)
/// Accumulating(id) --ToolCallDone--> Done(Vec<ToolCallDescriptor>)
/// ```
///
/// Refs: I-Core-ActiveToolCall, I-Core-ChunkBudget
/// # Panics
/// Panics only if an index is out of bounds; callers must validate lengths.
pub struct ToolCallAccumulator;

impl ToolCallAccumulator {
    /// Register the start of a new tool call.
    ///
    /// Inserts a fresh `ToolCallDescriptor` into the pending map.
    /// If the same `id` is started twice, the second overwrites the first.
    ///
    /// Complexity: O(log t) where t = pending descriptors.
    ///
    /// Refs: I-Core-ActiveToolCall
    /// # Panics
    /// Never panics.
    pub fn on_start(ext: &mut crate::ExtensionStorage, id: String, name: String) {
        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        acc.pending.insert(
            id.clone(),
            ToolCallDescriptor {
                tool_id: id,
                tool_name: name,
                arguments: String::new(),
                timeout_ms: None,
            },
        );
    }

    /// Append an argument chunk to an in-flight tool call.
    ///
    /// No-op if the `id` is not present (e.g., the stream was interleaved
    /// or the start event was dropped).
    ///
    /// Complexity: O(log t) for the map lookup + O(k) for the string append,
    /// where k = chunk length.
    ///
    /// Refs: I-Core-ActiveToolCall
    /// # Panics
    /// Panics only if an index is out of bounds; callers must validate lengths.
    pub fn on_argument_chunk(ext: &mut crate::ExtensionStorage, id: &str, chunk: &[u8]) {
        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        if let Some(descriptor) = acc.pending.get_mut(id) {
            descriptor
                .arguments
                .push_str(&String::from_utf8_lossy(chunk));
        }
    }

    /// Drain all pending descriptors and return them.
    ///
    /// The internal accumulator is cleared. Subsequent calls return an empty
    /// `Vec` until new `ToolCallStart` events arrive.
    ///
    /// Complexity: O(t) where t = number of pending descriptors.
    ///
    /// Refs: I-Core-ActiveToolCall
    /// # Panics
    /// Never panics.
    pub fn drain(ext: &mut crate::ExtensionStorage) -> Vec<ToolCallDescriptor> {
        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        std::mem::take(&mut acc.pending).into_values().collect()
    }
}

// ---------------------------------------------------------------------------
// Sealing
// ---------------------------------------------------------------------------

/// Pure function: convert `ToolCallDescriptor`s to `ActiveToolCall`s.
///
/// Any descriptor missing `timeout_ms` receives `default_timeout_ms`
/// and an `Effect::Error(StateInconsistency)` is returned alongside.
///
/// # Complexity
/// O(n) where n = number of descriptors. Allocates one `Vec<ActiveToolCall>`.
///
/// Refs: I-Core-ActiveToolCall, I-Core-NoPanic
/// # Panics
/// Never panics.
pub fn seal_tool_descriptors(
    descriptors: Vec<ToolCallDescriptor>,
    default_timeout_ms: u64,
) -> (Vec<ActiveToolCall>, Option<Effect>) {
    let mut missing = false;
    let active = descriptors
        .into_iter()
        .map(|d| {
            let timeout_ms = d.timeout_ms.unwrap_or_else(|| {
                missing = true;
                default_timeout_ms
            });
            ActiveToolCall {
                tool_id: d.tool_id,
                tool_name: d.tool_name,
                arguments: d.arguments,
                timeout_ms,
            }
        })
        .collect();
    let effect = if missing {
        Some(Effect::Error {
            code: crate::ErrorCode::StateInconsistency,
            detail: crate::ErrorDetail::MissingToolTimeout { default_timeout_ms },
        })
    } else {
        None
    };
    (active, effect)
}

// ---------------------------------------------------------------------------
// Trace logging (impl BriocheEngine)
// ---------------------------------------------------------------------------

use super::BriocheEngine;

impl BriocheEngine {
    /// Log an `OverrideTransition` to the in-memory trace log.
    ///
    /// Refs: I-Gov-Trace-Log
    /// Complexity: O(1). One Vec push (amortized).
    /// # Panics
    /// Never panics.
    pub(crate) fn log_override_transition(
        &self,
        session: &mut Session,
        source_plugin: &PluginSource,
    ) {
        let epoch = session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation;
        let log = session
            .extensions
            .get_or_insert_default::<TransitionTraceLog>();
        log.push(TransitionTrace {
            source_plugin: source_plugin.0.clone(),
            decision: PolicyDecision::OverrideTransition(vec![]),
            epoch,
        });
    }

    /// Log a superseded `OverrideTransition` to the in-memory trace log.
    ///
    /// Refs: I-Gov-Trace-Log
    /// Complexity: O(1). One Vec push (amortized).
    /// # Panics
    /// Never panics.
    pub(crate) fn log_superseded_transition(
        &self,
        session: &mut Session,
        source_plugin: &PluginSource,
        attempted_decision: &PolicyDecision,
    ) {
        let epoch = session
            .extensions
            .get_or_insert_default::<EpochState>()
            .current_generation;
        let log = session
            .extensions
            .get_or_insert_default::<SupersededTransitionTraceLog>();
        log.push(SupersededTransitionTrace {
            source_plugin: source_plugin.0.clone(),
            attempted_decision: attempted_decision.clone(),
            preempted_by: "prior_override_plugin".to_string(),
            epoch,
        });
    }
}

#[cfg(test)]
mod tool_call_accumulator_tests {
    use super::*;
    use crate::ExtensionStorage;

    #[test]
    fn accumulator_start_inserts_descriptor() {
        let mut ext = ExtensionStorage::new();
        ToolCallAccumulator::on_start(&mut ext, "t1".into(), "calc".into());

        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        assert_eq!(acc.pending.len(), 1);
        assert!(acc.pending.contains_key("t1"));
        assert_eq!(acc.pending["t1"].tool_name, "calc");
        assert!(acc.pending["t1"].arguments.is_empty());
        assert_eq!(acc.pending["t1"].timeout_ms, None);
    }

    #[test]
    fn accumulator_argument_chunk_appends() {
        let mut ext = ExtensionStorage::new();
        ToolCallAccumulator::on_start(&mut ext, "t1".into(), "calc".into());
        ToolCallAccumulator::on_argument_chunk(&mut ext, "t1", b"{\"x\":");
        ToolCallAccumulator::on_argument_chunk(&mut ext, "t1", b"1}");

        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        assert_eq!(acc.pending["t1"].arguments, "{\"x\":1}");
    }

    #[test]
    fn accumulator_argument_chunk_noop_for_unknown_id() {
        let mut ext = ExtensionStorage::new();
        ToolCallAccumulator::on_argument_chunk(&mut ext, "missing", b"data");

        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        assert!(acc.pending.is_empty());
    }

    #[test]
    fn accumulator_drain_returns_and_clears() {
        let mut ext = ExtensionStorage::new();
        ToolCallAccumulator::on_start(&mut ext, "t1".into(), "calc".into());
        ToolCallAccumulator::on_argument_chunk(&mut ext, "t1", b"{}");
        ToolCallAccumulator::on_start(&mut ext, "t2".into(), "grep".into());

        let drained = ToolCallAccumulator::drain(&mut ext);
        assert_eq!(drained.len(), 2);

        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        assert!(acc.pending.is_empty());

        // Second drain is empty.
        let empty = ToolCallAccumulator::drain(&mut ext);
        assert!(empty.is_empty());
    }

    #[test]
    fn accumulator_start_overwrites_duplicate_id() {
        let mut ext = ExtensionStorage::new();
        ToolCallAccumulator::on_start(&mut ext, "t1".into(), "first".into());
        ToolCallAccumulator::on_argument_chunk(&mut ext, "t1", b"old");
        ToolCallAccumulator::on_start(&mut ext, "t1".into(), "second".into());

        let acc = ext.get_or_insert_default::<StreamToolAccumulator>();
        assert_eq!(acc.pending["t1"].tool_name, "second");
        assert!(acc.pending["t1"].arguments.is_empty());
    }
}
