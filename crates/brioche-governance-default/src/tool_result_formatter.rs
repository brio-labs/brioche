//! ToolResultFormatter — Book II §5.7.
//!
//! Serializes `ToolOutcome`s to JSON for history injection.
//!
//! In the current architecture, the kernel already serializes outcomes
//! as strings into `ChatMessage::ToolResult`. This plugin provides
//! policy-level formatting (truncation, structured wrapping) via the
//! `on_tool_result` hook.
//!
//! Refs: I-Core-ActiveToolCall

use brioche_core::{
    BriochePlugin, ExtensionStorage, PluginCapabilities, PluginResult, ToolResultDTO,
};

/// Configuration de formatage des résultats d'outils.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    brioche_core::BriocheExtensionType,
)]
pub struct ToolResultFormatterState {
    /// Taille maximale d'un résultat JSON en octets (0 = pas de limite).
    pub max_result_bytes: usize,
    /// Nombre total de résultats formatés.
    pub formatted_count: u64,
}

impl Default for ToolResultFormatterState {
    fn default() -> Self {
        Self {
            max_result_bytes: 65536,
            formatted_count: 0,
        }
    }
}

/// Formateur des résultats d'outils.
///
/// Sur `on_tool_result`, formate et tronque les résultats si nécessaire.
pub struct ToolResultFormatter {
    max_result_bytes: usize,
}

impl ToolResultFormatter {
    /// Crée une nouvelle instance avec les valeurs par défaut.
    pub fn new() -> Self {
        Self {
            max_result_bytes: 65536,
        }
    }

    /// Crée une instance avec une limite de taille personnalisée.
    pub fn with_max_result_bytes(max_result_bytes: usize) -> Self {
        Self { max_result_bytes }
    }
}

impl Default for ToolResultFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl BriochePlugin for ToolResultFormatter {
    fn name(&self) -> &'static str {
        "tool_result_formatter"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_TOOL_RESULT
    }

    fn priority(&self) -> i16 {
        10 // Early formatter — apply limits before other plugins inspect
    }

    fn on_tool_result(
        &self,
        results: &mut Vec<ToolResultDTO>,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        let state = ext.get_or_insert_default::<ToolResultFormatterState>();
        state.max_result_bytes = self.max_result_bytes;

        for result in results {
            let content = match &result.outcome {
                brioche_core::ToolOutcome::Success(s)
                | brioche_core::ToolOutcome::BusinessError(s)
                | brioche_core::ToolOutcome::SystemError(s) => s.clone(),
                brioche_core::ToolOutcome::TimeoutWithPartialData { partial_output } => {
                    partial_output.clone().unwrap_or_default()
                }
            };

            if self.max_result_bytes > 0 && content.len() > self.max_result_bytes {
                let truncated = format!(
                    "{{\"truncated\":true,\"original_len\":{},\"preview\":\"{}\"}}",
                    content.len(),
                    &content[..self.max_result_bytes.min(content.len())]
                );
                result.outcome = brioche_core::ToolOutcome::Success(truncated);
            }

            state.formatted_count += 1;
        }

        Ok(())
    }
}
