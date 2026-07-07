//! Deterministic serialization, compression, and session-store helpers.
//!
//! This module owns pure DTO/message encoding and the in-memory bridge used by
//! async persistence, separate from Redb transaction code.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1.1, I-Persist-PluginBlob

use std::collections::BTreeMap;
use std::sync::Arc;

use brioche_core::{ChatMessage, Session};
use tokio::sync::RwLock;

use crate::dto::{SessionHeadDTO, SessionSchemaVersion};

use super::PersistenceError;

/// Size threshold above which a message or session head blob is Zstd-compressed.
///
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
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
/// Refs: docs/SPECS.md §Book III-B Ch 3.1
pub fn extract_delta(session: &Session) -> &[ChatMessage] {
    &session.history[session.persisted_msg_count as usize..]
}

/// Maximum size of a session head blob (uncompressed).
///
/// Heads are small metadata; anything larger indicates corruption or abuse.
const MAX_HEAD_BYTES: usize = 64 * 1024 * 1024;

/// Serialize a `SessionHeadDTO` to MessagePack and compute its checksum.
///
/// Complexity: O(serialization cost). Allocates a few `Vec`s.
/// Refs: docs/SPECS.md §Book III-A
pub fn serialize_head(dto: &SessionHeadDTO) -> Result<Vec<u8>, PersistenceError> {
    let mut dto = dto.clone();
    dto.checksum = None;
    let checksum_bytes =
        rmp_serde::to_vec(&dto).map_err(|e| PersistenceError::Serialization(e.to_string()))?;
    let checksum = crc32fast::hash(&checksum_bytes);
    dto.checksum = Some(checksum);
    rmp_serde::to_vec(&dto).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a `SessionHeadDTO` from a MessagePack blob.
///
/// Validates length, schema version, and checksum (if present).
/// Refs: docs/SPECS.md §Book III-A
pub fn deserialize_head(blob: &[u8]) -> Result<SessionHeadDTO, PersistenceError> {
    if blob.len() > MAX_HEAD_BYTES {
        return Err(PersistenceError::Serialization(format!(
            "session head blob too large: {} bytes (max {})",
            blob.len(),
            MAX_HEAD_BYTES
        )));
    }

    let dto: SessionHeadDTO =
        rmp_serde::from_slice(blob).map_err(|e| PersistenceError::Serialization(e.to_string()))?;

    if !matches!(dto.version, SessionSchemaVersion::V1) {
        return Err(PersistenceError::Serialization(format!(
            "unsupported session schema version: {:?}",
            dto.version
        )));
    }

    if let Some(expected) = dto.checksum {
        let mut dto_for_checksum = dto.clone();
        dto_for_checksum.checksum = None;
        let checksum_bytes = rmp_serde::to_vec(&dto_for_checksum)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let actual = crc32fast::hash(&checksum_bytes);
        if actual != expected {
            return Err(PersistenceError::Serialization(format!(
                "session head checksum mismatch: expected {expected:#010x}, got {actual:#010x}"
            )));
        }
    }

    Ok(dto)
}

/// Serialize a single `ChatMessage` to MessagePack.
/// Refs: docs/SPECS.md §Book III-A
pub fn serialize_message(msg: &ChatMessage) -> Result<Vec<u8>, PersistenceError> {
    rmp_serde::to_vec(msg).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

/// Deserialize a single `ChatMessage` from a MessagePack blob.
/// Refs: docs/SPECS.md §Book III-A
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
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
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
/// Refs: docs/SPECS.md §Book III-B Ch 1.1
pub fn maybe_decompress(data: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    match data.first() {
        Some(&FLAG_COMPRESSED) => {
            zstd::decode_all(&data[1..]).map_err(|e| PersistenceError::Compression(e.to_string()))
        }
        Some(&FLAG_UNCOMPRESSED) => Ok(data[1..].to_vec()),
        _ => Ok(data.to_vec()),
    }
}

/// Shared in-memory store bridging the engine thread and async persistence.
///
/// The engine thread (which owns the `!Send` `Session`) inserts entries
/// after each transition. The async `RedbStorage` reads from this map
/// when processing `SaveSession` effects.
///
/// Refs: I-Shell-Session-NoSend
pub type SessionStore = Arc<RwLock<BTreeMap<String, SessionStoreEntry>>>;

/// Create a new empty `SessionStore`.
/// Refs: docs/SPECS.md §Book III-A
pub fn new_session_store() -> SessionStore {
    Arc::new(RwLock::new(BTreeMap::new()))
}

/// A single entry in the `SessionStore`, holding both the head DTO and
/// the full message history needed for delta persistence.
/// Refs: docs/SPECS.md §Book III-A
#[derive(Clone, Debug)]
pub struct SessionStoreEntry {
    /// Flattened session head.
    pub head: SessionHeadDTO,
    /// Complete message history (including already-persisted messages).
    pub messages: Vec<ChatMessage>,
}
