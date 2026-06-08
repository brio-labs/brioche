//! Delta save protocol and compression helpers.
//!
//! The save protocol extracts messages from `persisted_msg_count` to the
//! end of the history vector, serializes each to MessagePack, and applies
//! Zstd compression if the payload exceeds `COMPRESSION_THRESHOLD` bytes.
//!
//! Refs: SPECS.md §Book III-B Ch 3, I-Persist-AppendOnly

use brioche_core::{ChatMessage, Session};

use crate::dto::SessionHeadDTO;
use crate::error::PersistenceError;

/// Size threshold above which a message or session head blob is Zstd-compressed.
///
/// Refs: SPECS.md §Book III-B Ch 1.1
pub const COMPRESSION_THRESHOLD: usize = 1024;

/// Compression flag prefix: `0x00` = uncompressed, `0x01` = zstd-compressed.
const FLAG_UNCOMPRESSED: u8 = 0x00;
const FLAG_COMPRESSED: u8 = 0x01;

/// Extract the delta slice of messages not yet persisted.
///
/// Returns messages from index `persisted_msg_count` onwards.
///
/// Complexity: O(1) slice creation; no allocation.
///
/// Refs: SPECS.md §Book III-B Ch 3.1
pub fn extract_delta(session: &Session) -> &[ChatMessage] {
    &session.history()[session.persisted_msg_count()..]
}

/// Serialize a `SessionHeadDTO` to MessagePack.
///
/// Complexity: O(serialization cost). Allocates one `Vec`.
pub fn serialize_head(dto: &SessionHeadDTO) -> Result<Vec<u8>, PersistenceError> {
    rmp_serde::to_vec(dto).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a `SessionHeadDTO` from a MessagePack blob.
pub fn deserialize_head(blob: &[u8]) -> Result<SessionHeadDTO, PersistenceError> {
    rmp_serde::from_slice(blob).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Serialize a single `ChatMessage` to MessagePack.
pub fn serialize_message(msg: &ChatMessage) -> Result<Vec<u8>, PersistenceError> {
    rmp_serde::to_vec(msg).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a single `ChatMessage` from a MessagePack blob.
pub fn deserialize_message(blob: &[u8]) -> Result<ChatMessage, PersistenceError> {
    rmp_serde::from_slice(blob).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Compress a payload with Zstd if it exceeds the threshold.
///
/// A one-byte flag prefix indicates whether compression was applied:
/// - `0x00` followed by raw data (uncompressed)
/// - `0x01` followed by zstd-compressed data
///
/// This makes decompression deterministic and branch-free.
///
/// Refs: SPECS.md §Book III-B Ch 1.1
pub fn maybe_compress(data: Vec<u8>) -> Result<Vec<u8>, PersistenceError> {
    if data.len() > COMPRESSION_THRESHOLD {
        let compressed = zstd::encode_all(data.as_slice(), 3)
            .map_err(|e| PersistenceError::Compression(e.to_string()))?;
        let mut result = Vec::with_capacity(1 + compressed.len());
        result.push(FLAG_COMPRESSED);
        result.extend_from_slice(&compressed);
        Ok(result)
    } else {
        let mut result = Vec::with_capacity(1 + data.len());
        result.push(FLAG_UNCOMPRESSED);
        result.extend_from_slice(&data);
        Ok(result)
    }
}

/// Decompress a payload based on the leading flag byte.
///
/// Falls back to returning the raw slice if no flag is recognized
/// (backward compatibility for legacy data written without flag).
///
/// Refs: SPECS.md §Book III-B Ch 1.1
pub fn maybe_decompress(data: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    match data.first() {
        Some(&FLAG_COMPRESSED) => {
            zstd::decode_all(&data[1..]).map_err(|e| PersistenceError::Compression(e.to_string()))
        }
        Some(&FLAG_UNCOMPRESSED) => Ok(data[1..].to_vec()),
        _ => Ok(data.to_vec()),
    }
}
