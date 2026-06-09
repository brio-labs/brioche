//! SSE (Server-Sent Events) parser for OpenAI streaming.
//!
//! Each line in the stream follows the format:
//! ```text
//! data: {...json...}
//! ```
//!
//! Refs: I-Core-ChunkBudget

use bytes::Bytes;

/// SSE parser state, line by line.
#[derive(Clone, Debug, Default)]
pub struct SseParser {
    buffer: String,
}

impl SseParser {
    /// Creates a new empty SSE parser.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Ingests a block of bytes and returns complete `data:` lines.
    ///
    /// Incomplete lines at the end of the block are accumulated in the
    /// internal buffer for the next call.
    ///
    /// Parse errors are logged at `warn` level so we can diagnose
    /// provider-specific SSE malformations instead of silently
    /// dropping events.
    ///
    /// # Complexity
    /// O(n) where n = number of bytes ingested. Single scan.
    pub fn feed(&mut self, bytes: &Bytes) -> impl Iterator<Item = serde_json::Value> + '_ {
        let text = String::from_utf8_lossy(bytes);
        self.buffer.push_str(&text);

        let mut lines = Vec::new();
        while let Some(pos) = self.buffer.find('\n') {
            let line = self.buffer.drain(..=pos).collect::<String>();
            let trimmed = line.trim();
            if let Some(json_str) = trimmed.strip_prefix("data: ") {
                if json_str == "[DONE]" {
                    continue;
                }
                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(value) => lines.push(value),
                    Err(err) => {
                        let preview: String = json_str.chars().take(200).collect();
                        tracing::warn!(
                            error = %err,
                            preview = %preview,
                            "SSE data: line is not valid JSON — skipping"
                        );
                    }
                }
            }
        }
        lines.into_iter()
    }

    /// Returns any unprocessed data still in the internal buffer.
    ///
    /// Used at stream end to diagnose whether the provider sent an
    /// incomplete `data:` line before closing the connection.
    ///
    /// # Complexity
    /// O(1). Returns a string slice reference.
    pub fn remaining_buffer(&self) -> &str {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_single_line() {
        let mut parser = SseParser::new();
        let bytes = Bytes::from("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
        let results: Vec<_> = parser.feed(&bytes).collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["choices"][0]["delta"]["content"], "hi");
    }

    #[test]
    fn sse_parser_ignores_done() {
        let mut parser = SseParser::new();
        let bytes = Bytes::from("data: [DONE]\n");
        let results: Vec<_> = parser.feed(&bytes).collect();
        assert!(results.is_empty());
    }

    #[test]
    fn sse_parser_splits_fragmented() {
        let mut parser = SseParser::new();
        let b1 = Bytes::from("data: {\"a\":1}\n");
        let b2 = Bytes::from("data: {\"b\":2}\n");
        let r1: Vec<_> = parser.feed(&b1).collect();
        let r2: Vec<_> = parser.feed(&b2).collect();
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
    }
}
