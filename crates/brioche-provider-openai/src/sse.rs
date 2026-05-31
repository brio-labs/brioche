//! Parser SSE (Server-Sent Events) pour le streaming OpenAI.
//!
//! Chaque ligne du stream suit le format :
//! ```text
//! data: {...json...}
//! ```
//!
//! Refs: I-Core-ChunkBudget

use bytes::Bytes;

/// État du parser SSE ligne par ligne.
#[derive(Clone, Debug, Default)]
pub struct SseParser {
    buffer: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Ingeste un bloc d'octets et retourne les lignes `data:` complètes.
    ///
    /// Les lignes incomplètes en fin de bloc sont accumulées dans le buffer
    /// interne pour le prochain appel.
    ///
    /// # Complexity
    /// O(n) où n = nombre d'octets ingérés. Un seul scan.
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
                if let Ok(value) = serde_json::from_str(json_str) {
                    lines.push(value);
                }
            }
        }
        lines.into_iter()
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
