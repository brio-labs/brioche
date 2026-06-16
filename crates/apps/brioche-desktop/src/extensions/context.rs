//! Modular context engine for the desktop app.
//!
//! The context engine is consulted before a conversation is sent to the LLM.
//! It receives the current message history and the configured context budget,
//! and returns a possibly re-ordered or compressed list of messages.
//!
//! Multiple engines can be registered; they are evaluated in registration order
//! and may be toggled by the user via settings.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use super::{ExtensionMetadata, PanelSlot};
use brioche_core::ChatMessage;
use serde::{Deserialize, Serialize};

/// Input to a context engine.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug)]
pub struct ContextEngineInput<'a> {
    /// Current conversation history.
    pub history: &'a [ChatMessage],
    /// Maximum tokens the configured model can accept.
    pub context_window: usize,
    /// Estimated tokens currently consumed (rough estimate).
    pub estimated_tokens: usize,
}

/// Output from a context engine.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ContextEngineOutput {
    /// Messages to send to the LLM, in the desired order.
    pub messages: Vec<ChatMessage>,
    /// Optional note explaining what the engine did (shown in the footer).
    pub note: Option<String>,
}

/// A context-engine extension.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub trait ContextEngine: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Processes a conversation before it is sent to the LLM.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn process(&self, input: ContextEngineInput<'_>) -> ContextEngineOutput;
}

/// Default compressor context engine.
///
/// When the estimated token count exceeds a configurable percentage of the
/// context window, the engine drops the oldest non-system messages and keeps
/// the most recent ones. This is the classic "sliding window" compression
/// strategy.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct CompressorContextEngine {
    /// Percentage of the context window that triggers compression.
    pub trigger_percentage: u8,
    /// Target percentage of the context window to keep after compression.
    pub target_percentage: u8,
    /// Number of recent messages to always preserve.
    pub preserve_recent: usize,
}

impl CompressorContextEngine {
    /// Creates a compressor with the given thresholds.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn new(trigger_percentage: u8, target_percentage: u8, preserve_recent: usize) -> Self {
        Self {
            trigger_percentage,
            target_percentage,
            preserve_recent,
        }
    }

    /// Rough token estimate: ~4 characters per token.
    pub(crate) fn estimate_tokens(messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| match m {
                brioche_core::ChatMessage::System { content }
                | brioche_core::ChatMessage::User { content }
                | brioche_core::ChatMessage::Assistant { content, .. } => content.len() / 4,
                brioche_core::ChatMessage::ToolRequest { arguments, .. } => arguments.len() / 4,
                brioche_core::ChatMessage::ToolResult { content, .. } => content.len() / 4,
                _ => 0,
            })
            .sum()
    }
}

impl ContextEngine for CompressorContextEngine {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "context-compressor".into(),
            name: "Sliding-window context compressor".into(),
            version: "0.1.0".into(),
            default_panel: Some(PanelSlot::Bottom),
            enabled: true,
        }
    }

    fn process(&self, input: ContextEngineInput<'_>) -> ContextEngineOutput {
        let estimated = Self::estimate_tokens(input.history);
        let trigger_tokens = input.context_window * self.trigger_percentage as usize / 100;

        if estimated <= trigger_tokens || input.history.len() <= self.preserve_recent + 1 {
            return ContextEngineOutput {
                messages: input.history.to_vec(),
                note: None,
            };
        }

        let target_tokens = input.context_window * self.target_percentage as usize / 100;
        let mut kept = Vec::with_capacity(input.history.len());
        let mut system_msgs = Vec::new();
        let mut non_system = Vec::new();

        for msg in input.history {
            match msg {
                brioche_core::ChatMessage::System { .. } => system_msgs.push(msg.clone()),
                _ => non_system.push(msg.clone()),
            }
        }

        // Always keep system messages and the most recent non-system messages.
        let preserve = self.preserve_recent.max(1);
        let recent = non_system.split_off(non_system.len().saturating_sub(preserve));
        kept.extend(system_msgs);

        // Greedily add older non-system messages until we approach the target.
        let mut current_tokens = Self::estimate_tokens(&kept) + Self::estimate_tokens(&recent);
        for msg in non_system.into_iter().rev() {
            let msg_tokens = Self::estimate_tokens(std::slice::from_ref(&msg));
            if current_tokens + msg_tokens > target_tokens {
                break;
            }
            kept.push(msg);
            current_tokens += msg_tokens;
        }
        kept.extend(recent);

        ContextEngineOutput {
            messages: kept,
            note: Some(format!(
                "Context compressed: kept {}/{} messages",
                input.history.len(),
                input.history.len()
            )),
        }
    }
}

/// Settings for the compressor context engine.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompressorSettings {
    /// Percentage at which compression activates.
    pub trigger_percentage: u8,
    /// Target percentage after compression.
    pub target_percentage: u8,
    /// Recent messages to preserve.
    pub preserve_recent: usize,
}

impl Default for CompressorSettings {
    fn default() -> Self {
        Self {
            trigger_percentage: 75,
            target_percentage: 50,
            preserve_recent: 6,
        }
    }
}
