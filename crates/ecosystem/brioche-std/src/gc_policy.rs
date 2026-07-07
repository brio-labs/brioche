//! GcPolicy and ContextOptimizer — Book IV §1.7, §1.3.
//!
//! `GcPolicy` decides whether to trigger opportunistic garbage collection.
//! `ContextOptimizer` monitors history size and triggers summarization.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    AfterPrediction, AgentStateTag, AsyncTaskResult, BeforePrediction, BriocheExtensionType,
    ChatMessage, Effect, ExtensionStorage, HistoryEdit, PluginResult, PolicyDecision,
    SessionSnapshot, SignalBuffer,
};

use crate::Priority;

/// GC policy telemetry.
///
/// `GcPolicy` constructor fields are runtime policy configuration, not
/// persisted replay state. This state stores only durable cycle counters.
///
/// ## Snapshot strategy
/// COW: full clone (~16 bytes). Two scalar counters.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct GcPolicyState {
    /// Cycles since last GC.
    pub cycles_since_gc: u64,
    /// Total number of GCs triggered.
    pub gcs_triggered: u64,
}

/// GC policy plugin.
///
/// Requests `TriggerGc` based on cycle count and idle state.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct GcPolicy {
    cycle_interval: u64,
    only_when_idle: bool,
}

impl GcPolicy {
    /// Creates a policy with a cycle interval.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_cycle_interval(cycle_interval: u64) -> Self {
        Self {
            cycle_interval,
            only_when_idle: true,
        }
    }

    /// Creates a policy that triggers unconditionally every N cycles.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_unconditional_interval(cycle_interval: u64) -> Self {
        Self {
            cycle_interval,
            only_when_idle: false,
        }
    }
}

impl Default for GcPolicy {
    fn default() -> Self {
        Self::with_cycle_interval(10)
    }
}

impl AfterPrediction for GcPolicy {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;

    fn name(&self) -> &'static str {
        "gc_policy"
    }

    fn priority(&self) -> i16 {
        Priority::GC_OBSERVER // Late observer — let interceptors run first
    }

    /// Increments the GC cycle counter without persisting static policy config.
    ///
    /// # Complexity
    /// O(1). One ExtensionStorage read/write. No heap allocation.
    ///
    /// # Panics
    /// Never panics. No indexing or conditional allocation.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn after_prediction(&self, ext: &mut ExtensionStorage) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<GcPolicyState>();
        state.cycles_since_gc += 1;
        Ok(())
    }
}

impl BeforePrediction for GcPolicy {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "gc_policy"
    }

    fn priority(&self) -> i16 {
        Priority::GC_OBSERVER
    }

    /// Requests `Effect::TriggerGc` when the cycle threshold is met and the
    /// idle policy is satisfied.
    ///
    /// # Complexity
    /// O(1). Reads `SessionSnapshot` and updates `GcPolicyState`. No heap
    /// allocation; cycle interval and idle gating stay in runtime config.
    ///
    /// # Panics
    /// Never panics. No indexing or conditional allocation.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        if self.cycle_interval == 0 {
            return Ok(PolicyDecision::Allow);
        }

        let is_idle = {
            let Some(snapshot) = ext.get::<SessionSnapshot>() else {
                return Err(brioche_core::PluginError::Fatal {
                    plugin_name: "gc_policy".into(),
                    message: "missing SessionSnapshot".into(),
                });
            };
            snapshot.current_state == AgentStateTag::Idle
        };

        let state = ext.get_or_insert_default::<GcPolicyState>();

        if state.cycles_since_gc >= self.cycle_interval && (!self.only_when_idle || is_idle) {
            state.cycles_since_gc = 0;
            state.gcs_triggered += 1;
            return Ok(PolicyDecision::RequestEffect(Effect::TriggerGc));
        }

        Ok(PolicyDecision::Allow)
    }
}

// ---------------------------------------------------------------------------
// ContextOptimizer (merged from context_optimizer.rs)
// ---------------------------------------------------------------------------

/// Context optimizer telemetry.
///
/// Message threshold fields are runtime policy configuration owned by
/// `ContextOptimizer`; persisted state stores only summarization observations.
///
/// ## Snapshot strategy
/// COW: full clone (~8 bytes). One scalar counter.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct ContextOptimizerState {
    /// Number of times summarization has been triggered.
    pub summarizations_triggered: u64,
}

/// Context optimizer plugin.
///
/// Requests `TriggerSummarization` when history length exceeds
/// `max_messages * threshold_percent / 100`.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct ContextOptimizer {
    max_messages: u64,
    threshold_percent: u8,
}

impl ContextOptimizer {
    /// Creates an optimizer with a message limit and threshold.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_threshold(max_messages: u64, threshold_percent: u8) -> Self {
        Self {
            max_messages,
            threshold_percent: threshold_percent.min(100),
        }
    }
}

impl Default for ContextOptimizer {
    fn default() -> Self {
        Self::with_threshold(100, 85)
    }
}

impl BeforePrediction for ContextOptimizer {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = brioche_core::PluginError;
    type PolicyDecision = PolicyDecision;

    fn name(&self) -> &'static str {
        "context_optimizer"
    }

    fn priority(&self) -> i16 {
        Priority::CONTEXT_OPTIMIZER // After interceptors, before prediction
    }

    /// Applies completed summaries or requests summarization at the runtime threshold.
    ///
    /// # Complexity
    /// O(a + h) where a = buffered async task results and h = history length.
    /// Allocates only when cloning a completed summary into `HistoryEdit`;
    /// static threshold config is not copied into persisted state.
    ///
    /// # Panics
    /// Never panics. Watermarks are clamped to history length.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn before_prediction(
        &self,
        history: &[ChatMessage],
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // If a summarization task completed, replace the summarized prefix
        // with the compressed system message before deciding whether further
        // compression is needed.
        let buffer = ext.get_or_insert_default::<SignalBuffer>();
        if let Some(result) = buffer.async_task_results.iter().find_map(|ar| match ar {
            AsyncTaskResult::SummarizationDone { summary, watermark } => {
                Some((summary.clone(), *watermark))
            }
            _ => None,
        }) {
            let (summary, watermark) = result;
            let watermark = (watermark as usize).min(history.len());
            let keep_last = (history.len() - watermark) as u64;
            return Ok(PolicyDecision::MutateHistory(vec![
                HistoryEdit::Truncate { keep_last },
                HistoryEdit::Insert {
                    index: 0,
                    message: summary,
                },
            ]));
        }

        let threshold = (self.max_messages * self.threshold_percent as u64) / 100;
        if threshold > 0 && history.len() >= threshold as usize {
            let state = ext.get_or_insert_default::<ContextOptimizerState>();
            state.summarizations_triggered += 1;
            return Ok(PolicyDecision::RequestEffect(Effect::TriggerSummarization));
        }

        Ok(PolicyDecision::Allow)
    }
}
