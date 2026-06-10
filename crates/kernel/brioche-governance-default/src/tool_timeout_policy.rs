//! ToolTimeoutPolicy — Book II §5.14 (variant).
//!
//! Bounds tool timeouts before `ExecuteTools` emission via the
//! `on_tool_calls` hook.
//!
//! Refs: I-Gov-Timeout-Bound

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, ToolCallDescriptor,
};

/// Timeout policy state.
///
/// ## Snapshot strategy
/// COW: full clone (~16 bytes). Two scalar fields.
#[derive(
    Clone,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
#[brioche(critical_state)]
pub struct ToolTimeoutState {
    /// Default timeout in ms.
    pub default_timeout_ms: u64,
    /// Maximum allowed timeout (0 = no limit).
    pub max_timeout_ms: u64,
}

/// Timeout policy for tool calls.
///
/// On `on_tool_calls`, applies `default_timeout_ms` if absent and
/// caps to `max_timeout_ms` if defined.
///
/// Refs: I-Core-ActiveToolCall
pub struct ToolTimeoutPolicy {
    default_timeout_ms: u64,
    max_timeout_ms: u64,
}

impl ToolTimeoutPolicy {
    /// Creates a policy with a default timeout.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_default_timeout(default_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            max_timeout_ms: 0,
        }
    }

    /// Creates a policy with a default timeout and a max cap.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn with_bounds(default_timeout_ms: u64, max_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            max_timeout_ms,
        }
    }
}

impl Default for ToolTimeoutPolicy {
    fn default() -> Self {
        Self::with_default_timeout(30000)
    }
}

impl BriochePlugin for ToolTimeoutPolicy {
    fn name(&self) -> &'static str {
        "tool_timeout_policy"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_CALLS
    }

    fn priority(&self) -> i16 {
        -10 // Early mutator — apply before other plugins inspect timeouts
    }

    /// Applies the default timeout and caps to `max_timeout_ms`.
    ///
    /// # Complexity
    /// O(c). `c` calls; one `ExtensionStorage` read + linear loop.
    fn on_tool_calls(
        &self,
        calls: &mut Vec<ToolCallDescriptor>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolTimeoutState>();
        state.default_timeout_ms = self.default_timeout_ms;
        state.max_timeout_ms = self.max_timeout_ms;

        for call in calls {
            let mut timeout = call.timeout_ms.unwrap_or(self.default_timeout_ms);

            if self.max_timeout_ms > 0 && timeout > self.max_timeout_ms {
                timeout = self.max_timeout_ms;
            }

            call.timeout_ms = Some(timeout);
        }

        Ok(())
    }
}
