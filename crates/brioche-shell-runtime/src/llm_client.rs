//! LLM network client boundary.
//!
//! The [`LlmClient`] trait abstracts the SSE streaming connection to an
//! LLM provider. The shell segments payloads according to
//! [`MAX_INLINE_CHUNK`](brioche_core::MAX_INLINE_CHUNK) before injecting
/// them as `EngineInput::LlmStream`.
///
/// Refs: I-Core-ChunkBudget, I-Shell-Network-Signal
use crate::BriocheShell;
use brioche_core::{EngineInput, StreamEvent};
use bytes::Bytes;

/// Abstract LLM client.
///
/// Implementations handle the transport (HTTP/SSE), error recovery,
/// and payload segmentation. The shell runtime drives the loop that
/// converts provider chunks into `StreamEvent`s.
///
/// Refs: SPECS.md §Book III-A Ch 1
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
}

// ---------------------------------------------------------------------------
// Mock implementation
// ---------------------------------------------------------------------------

/// A mock LLM client that yields a fixed sequence of text chunks.
#[derive(Clone, Debug)]
pub struct MockLlmClient {
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
        for text in &self.chunks {
            // Segment into MAX_INLINE_CHUNK fragments.
            let bytes = Bytes::from(text.clone());
            for chunk in segment_bytes(bytes, brioche_core::MAX_INLINE_CHUNK) {
                shell
                    .send_input(EngineInput::LlmStream(StreamEvent::TextChunk {
                        path: Default::default(),
                        chunk,
                    }))
                    .await?;
            }
        }
        Ok(())
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
