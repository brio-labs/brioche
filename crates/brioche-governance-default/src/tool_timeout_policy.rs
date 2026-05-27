//! ToolTimeoutPolicy — Book II §5.14 (variante).
//!
//! Bounds tool timeouts before `ExecuteTools` emission via the
//! `on_tool_calls` hook.
//!
//! Refs: I-Gov-Timeout-Bound

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, ToolCallDescriptor,
};

/// État de la politique de timeout.
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
    /// Timeout par défaut en ms.
    pub default_timeout_ms: u64,
    /// Timeout maximum autorisé (0 = pas de limite).
    pub max_timeout_ms: u64,
}

/// Politique de timeout pour les appels d'outils.
///
/// Sur `on_tool_calls`, applique `default_timeout_ms` si absent et
/// borne à `max_timeout_ms` si défini.
pub struct ToolTimeoutPolicy {
    default_timeout_ms: u64,
    max_timeout_ms: u64,
}

impl ToolTimeoutPolicy {
    /// Crée une politique avec un timeout par défaut.
    pub fn with_default_timeout(default_timeout_ms: u64) -> Self {
        Self {
            default_timeout_ms,
            max_timeout_ms: 0,
        }
    }

    /// Crée une politique avec un timeout par défaut et une borne max.
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

    /// Applique le timeout par défaut et borne à `max_timeout_ms`.
    ///
    /// # Complexity
    /// O(c). `c` appels ; une lecture `ExtensionStorage` + boucle linéaire.
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
