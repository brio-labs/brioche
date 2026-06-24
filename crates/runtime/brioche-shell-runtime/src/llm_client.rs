//! LLM network client boundary.
//!
//! The [`LlmClient`] trait abstracts the SSE streaming connection to an
//! LLM provider. The shell segments payloads according to
//! [`MAX_INLINE_CHUNK`](brioche_core::MAX_INLINE_CHUNK) before injecting
use brioche_core::{ChatMessage, EngineInput, StreamEvent, ToolResultDTO};
use bytes::Bytes;

/// them as `EngineInput::LlmStream`.
///
/// Refs: I-Core-ChunkBudget, I-Shell-Network-Signal
use crate::BriocheShell;

/// Chunk broadcast to the projection (CLI, GUI…). The kernel never sees it.
///
/// This type lives in the runtime because it is the protocol between the LLM
/// client and the projection layer, independent of the provider.
///
/// Refs: I-Shell-Projection-Independent
#[derive(Clone, Debug)]
pub enum LlmChunk {
    /// Plain text fragment from the LLM response.
    Text(String),
    /// Reasoning / chain-of-thought fragment from the LLM.
    ///
    /// Separate from `Text` so consumers can choose whether
    /// to display reasoning inline, in a sidebar, or not at all.
    Reasoning(String),
    /// Beginning of a tool call declaration.
    ToolCallStart {
        /// Stable identifier for the tool call or result.
        id: String,
        /// Name of the tool being invoked.
        name: String,
    },
    /// Fragment of tool call arguments (JSON).
    ToolArgument {
        /// Stable identifier for the tool call or result.
        id: String,
        /// JSON-encoded argument fragment.
        fragment: String,
    },
    /// End of a tool call declaration.
    ToolCallDone {
        /// Stable identifier for the tool call or result.
        id: String,
    },
    /// Result of a tool execution.
    ToolResult {
        /// Name of the tool that was executed.
        name: String,
        /// Serialized output of the tool.
        output: String,
    },
    /// End-of-stream marker.
    Done,
    /// Error from the LLM provider or transport layer.
    Error(String),
    /// Warning message from the provider or runtime.
    Warning(String),
    /// Status update for long-running operations.
    Status(String),
}

/// Abstract LLM client.
///
/// Implementations handle the transport (HTTP/SSE), error recovery,
/// and payload segmentation. The shell runtime drives the loop that
/// converts provider chunks into `StreamEvent`s.
///
/// Refs: docs/SPECS.md §Book III-A Ch 1
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    /// Initiate an LLM call and stream fragments back via `shell.send_input`.
    ///
    /// The implementation should:
    /// 1. Open an SSE connection.
    /// 2. For each received payload, segment it into chunks ≤ `MAX_INLINE_CHUNK`.
    /// 3. Send each chunk as `EngineInput::LlmStream(StreamEvent::TextChunk { ... })`.
    /// 4. On network failure, send `SystemSignal::NetworkUnavailable` via the shell.
    ///
    /// Refs: I-Core-ChunkBudget
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), crate::ShellError>;

    /// Summarize a slice of chat history into a single compressed message.
    ///
    /// This is used by context compression: the returned system message
    /// replaces the summarized messages in the conversation history.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    async fn summarize(
        &self,
        shell: &BriocheShell,
        messages: &[ChatMessage],
    ) -> Result<ChatMessage, crate::ShellError>;
    /// Push tool execution results into the conversational history.
    ///
    /// Results MUST be pushed in the same order as the original
    /// `ActiveToolCall`s (i.e. the order of `tool_calls` in the
    /// assistant message). Some providers reject requests where
    /// tool results appear in a different order.
    ///
    /// Refs: I-Shell-ToolResult-PassThrough
    async fn push_tool_results(&self, results: &[ToolResultDTO]);
}

// ---------------------------------------------------------------------------
// Mock implementation
// ---------------------------------------------------------------------------

/// A mock LLM client that yields a fixed sequence of text chunks.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct MockLlmClient {
    /// Fixed sequence of text chunks yielded by the mock client.
    pub chunks: Vec<String>,
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self {
            chunks: vec!["Hello".into(), " ".into(), "world".into()],
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for MockLlmClient {
    async fn call_llm(&self, shell: &BriocheShell) -> Result<(), crate::ShellError> {
        for chunk in &self.chunks {
            let bytes = Bytes::from(chunk.clone());
            for fragment in segment_bytes(bytes, brioche_core::MAX_INLINE_CHUNK) {
                shell
                    .send_input(EngineInput::LlmStream(StreamEvent::TextChunk {
                        path: Default::default(),
                        chunk: fragment,
                    }))
                    .await?;
            }
        }
        shell
            .send_input(EngineInput::LlmStream(StreamEvent::Done))
            .await?;
        Ok(())
    }

    async fn summarize(
        &self,
        _shell: &BriocheShell,
        messages: &[ChatMessage],
    ) -> Result<ChatMessage, crate::ShellError> {
        Ok(ChatMessage::System {
            content: format!("Mock summary of {} messages", messages.len()),
        })
    }

    async fn push_tool_results(&self, _results: &[ToolResultDTO]) {
        // Mock client has no persistent mirror.
    }
}

/// Segment a `Bytes` payload into sub-fragments of at most `max_chunk` size.
///
/// Complexity: O(n / max_chunk). Zero-copy via `Bytes::slice_ref`.
///
/// Refs: I-Core-ChunkBudget
fn segment_bytes(bytes: Bytes, max_chunk: usize) -> Vec<Bytes> {
    if bytes.len() <= max_chunk {
        return vec![bytes];
    }
    let mut fragments = Vec::with_capacity(bytes.len().div_ceil(max_chunk));
    let mut offset = 0;
    while offset < bytes.len() {
        let end = (offset + max_chunk).min(bytes.len());
        fragments.push(bytes.slice(offset..end));
        offset = end;
    }
    fragments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_bytes_small() {
        let b = Bytes::from("hello");
        let frags = segment_bytes(b.clone(), 4096);
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0], b);
    }

    #[test]
    fn segment_bytes_large() {
        let data = vec![b'x'; 10_000];
        let b = Bytes::from(data);
        let frags = segment_bytes(b, 4096);
        assert_eq!(frags.len(), 3);
        assert_eq!(frags[0].len(), 4096);
        assert_eq!(frags[1].len(), 4096);
        assert_eq!(frags[2].len(), 1808);
    }
}
