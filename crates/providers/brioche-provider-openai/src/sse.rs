//! SSE (Server-Sent Events) parser for OpenAI streaming.
//!
//! Each line in the stream follows the format:
//! ```text
//! data: {...json...}
//! ```
//!
//! Refs: I-Core-ChunkBudget

use std::fmt;

use bytes::Bytes;

/// Default maximum consecutive malformed `data:` lines before the parser
/// aborts the stream.
///
/// This bound prevents a malicious or broken provider from flooding the shell
/// with an infinite sequence of non-JSON SSE events.
const DEFAULT_MAX_CONSECUTIVE_FAILURES: usize = 5;

/// Error returned when the SSE parser aborts because the provider sent too
/// many consecutive malformed lines.
///
/// Refs: I-Shell-Network-Signal
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseError {
    /// Too many consecutive `data:` lines failed JSON parsing.
    TooManyMalformedLines {
        /// Number of consecutive failures observed before aborting.
        count: usize,
        /// Preview of the last malformed line (truncated for logging).
        preview: String,
    },
}

impl fmt::Display for SseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyMalformedLines { count, preview } => write!(
                f,
                "aborting SSE stream after {count} consecutive malformed data: lines; last line preview: {preview}"
            ),
        }
    }
}

impl std::error::Error for SseError {}

/// SSE parser state, line by line.
///
/// Refs: docs/SPECS.md §Book III-B
#[derive(Clone, Debug)]
pub struct SseParser {
    buffer: String,
    consecutive_failures: usize,
    max_consecutive_failures: usize,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SseParser {
    /// Creates a new empty SSE parser with the default failure threshold.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn new() -> Self {
        Self::with_threshold(DEFAULT_MAX_CONSECUTIVE_FAILURES)
    }

    /// Creates a parser with a custom consecutive-failure threshold.
    ///
    /// A threshold of zero means any malformed `data:` line aborts immediately.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn with_threshold(max_consecutive_failures: usize) -> Self {
        Self {
            buffer: String::new(),
            consecutive_failures: 0,
            max_consecutive_failures,
        }
    }

    /// Ingests a block of bytes and returns complete `data:` lines.
    ///
    /// Incomplete lines at the end of the block are accumulated in the
    /// internal buffer for the next call.
    ///
    /// Individual parse errors are logged at `warn` level so we can diagnose
    /// provider-specific SSE malformations. If the number of consecutive
    /// malformed `data:` lines reaches `max_consecutive_failures`, parsing
    /// aborts and this method returns [`SseError::TooManyMalformedLines`].
    ///
    /// # Complexity
    /// O(n) where n = number of bytes ingested. Single scan.
    ///
    /// # Errors
    /// Returns `SseError::TooManyMalformedLines` when the consecutive malformed
    /// line threshold is exceeded.
    ///
    /// Refs: docs/SPECS.md §Book III-B
    pub fn feed(
        &mut self,
        bytes: &Bytes,
    ) -> Result<impl Iterator<Item = serde_json::Value> + '_, SseError> {
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
                    Ok(value) => {
                        self.consecutive_failures = 0;
                        lines.push(value);
                    }
                    Err(err) => {
                        self.consecutive_failures += 1;
                        let preview: String = json_str.chars().take(200).collect();
                        if self.consecutive_failures >= self.max_consecutive_failures {
                            return Err(SseError::TooManyMalformedLines {
                                count: self.consecutive_failures,
                                preview,
                            });
                        }
                        tracing::warn!(
                            error = %err,
                            preview = %preview,
                            "SSE data: line is not valid JSON — skipping"
                        );
                    }
                }
            }
        }
        Ok(lines.into_iter())
    }

    /// Returns any unprocessed data still in the internal buffer.
    ///
    /// Used at stream end to diagnose whether the provider sent an
    /// incomplete `data:` line before closing the connection.
    ///
    /// # Complexity
    /// O(1). Returns a string slice reference.
    /// Refs: docs/SPECS.md §Book III-B
    pub fn remaining_buffer(&self) -> &str {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_parser_single_line() -> Result<(), SseError> {
        let mut parser = SseParser::new();
        let bytes = Bytes::from("data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n");
        let results: Vec<_> = parser.feed(&bytes)?.collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["choices"][0]["delta"]["content"], "hi");
        Ok(())
    }

    #[test]
    fn sse_parser_ignores_done() -> Result<(), SseError> {
        let mut parser = SseParser::new();
        let bytes = Bytes::from("data: [DONE]\n");
        let results: Vec<_> = parser.feed(&bytes)?.collect();
        assert!(results.is_empty());
        Ok(())
    }

    #[test]
    fn sse_parser_splits_fragmented() -> Result<(), SseError> {
        let mut parser = SseParser::new();
        let b1 = Bytes::from("data: {\"a\":1}\n");
        let b2 = Bytes::from("data: {\"b\":2}\n");
        let r1: Vec<_> = parser.feed(&b1)?.collect();
        let r2: Vec<_> = parser.feed(&b2)?.collect();
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        Ok(())
    }

    #[test]
    fn sse_parser_aborts_after_threshold() {
        let mut parser = SseParser::with_threshold(3);
        let bytes = Bytes::from("data: not-json-1\ndata: not-json-2\ndata: not-json-3\n");
        let result = parser.feed(&bytes);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(SseError::TooManyMalformedLines { count: 3, .. })
        ));
    }

    #[test]
    fn sse_parser_resets_counter_on_valid_json() -> Result<(), SseError> {
        let mut parser = SseParser::with_threshold(3);
        let b1 = Bytes::from("data: not-json-1\ndata: not-json-2\n");
        let r1: Vec<_> = parser.feed(&b1)?.collect();
        assert!(r1.is_empty());

        let b2 = Bytes::from("data: {\"valid\":true}\n");
        let r2: Vec<_> = parser.feed(&b2)?.collect();
        assert_eq!(r2.len(), 1);

        let b3 = Bytes::from("data: not-json-3\ndata: not-json-4\ndata: not-json-5\n");
        let result = parser.feed(&b3);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn sse_parser_zero_threshold_aborts_immediately() {
        let mut parser = SseParser::with_threshold(0);
        let bytes = Bytes::from("data: not-json\n");
        let result = parser.feed(&bytes);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(SseError::TooManyMalformedLines { count: 1, .. })
        ));
    }

    /// A single `data:` line may be split across multiple TCP chunks.
    ///
    /// The parser must buffer partial lines and only emit a JSON value
    /// once the terminating newline arrives.
    ///
    /// Refs: I-Core-ChunkBudget
    #[test]
    fn sse_parser_reassembles_fragmented_data_line() -> Result<(), SseError> {
        let mut parser = SseParser::new();
        let part1 = Bytes::from("data: {\"choices\":[{\"delta\":{\"content\":\"Hello");
        let part2 = Bytes::from(" World\"}}]}\n\n");

        let r1: Vec<_> = parser.feed(&part1)?.collect();
        assert!(r1.is_empty(), "partial line must not emit an event");

        let r2: Vec<_> = parser.feed(&part2)?.collect();
        assert_eq!(r2.len(), 1, "complete line must emit one event");
        assert_eq!(r2[0]["choices"][0]["delta"]["content"], "Hello World");
        Ok(())
    }
}
