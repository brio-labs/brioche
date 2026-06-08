//! AuditLogger — Book IV §1.8.
//!
//! Records every `EngineInput` into a deterministic replay log.
//! Entries are batched and flushed via `SavePluginBlob` effects
//! requested on `on_input`.
//!
//! Refs: I-Eco-ExtensionOverMod, I-Eco-OrderedCollections

use brioche_core::{
    BriocheExtensionType, BriochePlugin, Effect, EngineInput, ExtensionStorage, PluginCapabilities,
    PluginResult, PolicyDecision,
};

/// Single audit log entry.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// Sequence number (monotonically increasing).
    pub sequence: u64,
    /// Serialized input.
    pub input_json: String,
    /// Epoch at the time of logging.
    pub epoch: u64,
}

/// Audit logger state.
///
/// Refs: I-Eco-OrderedCollections
#[derive(
    Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType,
)]
pub struct AuditLoggerState {
    /// Pending entries not yet flushed.
    #[brioche(deterministic_order)]
    pub pending: Vec<AuditEntry>,
    /// Total entries ever logged.
    pub total_logged: u64,
    /// Sequence counter.
    pub next_sequence: u64,
    /// Flush batch size.
    pub batch_size: usize,
}

/// Deterministic audit logger.
///
/// Records every input for replay verification. Requests `SavePluginBlob`
/// when the pending batch reaches `batch_size`.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct AuditLogger {
    batch_size: usize,
}

impl AuditLogger {
    /// Creates a logger with a batch size.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    pub fn with_batch_size(batch_size: usize) -> Self {
        Self { batch_size }
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::with_batch_size(32)
    }
}

impl BriochePlugin for AuditLogger {
    fn name(&self) -> &'static str {
        "audit_logger"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
    }

    fn priority(&self) -> i16 {
        -100 // Very early — log before any interceptor can block
    }

    /// Logs the input and requests a blob flush if batch is full.
    ///
    /// # Complexity
    /// O(1) amortized. One JSON serialization + optional Vec push.
    ///
    /// Refs: I-Eco-ExtensionOverMod
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        // Read epoch first so the mutable borrow ends before state access.
        let epoch =
            ext.with_or_insert_default::<brioche_core::EpochState, _>(|es| es.current_generation);

        let should_flush = ext.with_or_insert_default::<AuditLoggerState, _>(|state| {
            state.batch_size = self.batch_size;

            let input_json = match serde_json::to_string(input) {
                Ok(s) => s,
                Err(e) => format!("{{\"serialization_error\":\"{}\"}}", e),
            };

            let entry = AuditEntry {
                sequence: state.next_sequence,
                input_json,
                epoch,
            };
            state.next_sequence += 1;
            state.total_logged += 1;
            state.pending.push(entry);

            state.pending.len() >= self.batch_size
        });

        if should_flush {
            let blob = ext.with_or_insert_default::<AuditLoggerState, _>(|state| {
                let blob = brioche_core::postcard::to_allocvec(&state.pending).unwrap_or_default();
                state.pending.clear();
                blob
            });
            return Ok(PolicyDecision::RequestEffect(Effect::SavePluginBlob {
                plugin_id: "audit_logger".into(),
                data: blob,
            }));
        }

        Ok(PolicyDecision::Allow)
    }
}
