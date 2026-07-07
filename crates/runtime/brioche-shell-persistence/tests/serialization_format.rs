//! Serialization, compression, checksum, and legacy format contracts.

use brioche_core::Session;
use brioche_shell_persistence::{
    COMPRESSION_THRESHOLD, FlattenedAgentState, SessionHeadDTO, SessionSchemaVersion,
    deserialize_head, maybe_compress, maybe_decompress, serialize_head,
};

#[test]
fn compress_large_payload() {
    let data = vec![0u8; COMPRESSION_THRESHOLD + 100];
    let compressed = match maybe_compress(data.clone()) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // First byte is the compression flag.
    assert_eq!(compressed[0], 1);
    // Compressed should be smaller than original.
    assert!(compressed.len() < data.len());

    let decompressed = match maybe_decompress(&compressed) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(decompressed, data);
}

#[test]
fn passthrough_small_payload() {
    let data = vec![1u8, 2, 3];
    let compressed = match maybe_compress(data.clone()) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(compressed[0], 0);
    assert_eq!(compressed[1..], data);

    let decompressed = match maybe_decompress(&compressed) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(decompressed, data);
}

#[test]
fn decompress_legacy_no_flag() {
    // Data written before the flag prefix was introduced.
    let raw = vec![7u8, 8, 9];
    let decompressed = match maybe_decompress(&raw) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(decompressed, raw);
}

#[test]
fn session_head_serialization_roundtrip() {
    let session = Session::new("roundtrip");
    let dto = SessionHeadDTO::from_session(&session);
    let blob = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let restored = match deserialize_head(&blob) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(restored.version, dto.version);
    assert_eq!(restored.id, dto.id);
    assert_eq!(restored.parent_id, dto.parent_id);
    assert_eq!(restored.state, dto.state);
    assert_eq!(restored.state_stack, dto.state_stack);
    assert_eq!(restored.extensions, dto.extensions);
    assert_eq!(restored.persisted_msg_count, dto.persisted_msg_count);
    assert_eq!(restored.compaction_index, dto.compaction_index);
    assert!(
        restored.checksum.is_some(),
        "checksum should be set after serialization"
    );
}

#[tokio::test]
async fn deserialize_head_rejects_oversized_blob() {
    let huge = vec![0u8; 64 * 1024 * 1024 + 1];
    let err = match deserialize_head(&huge) {
        Err(e) => e,
        Ok(_) => unreachable!("oversized blob should fail"),
    };
    assert!(err.to_string().contains("too large"));
}

#[test]
fn deserialize_head_rejects_checksum_mismatch() {
    let session = Session::new("checksum-test");
    let dto = SessionHeadDTO::from_session(&session);
    let mut blob = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Corrupt a byte in the middle of the blob.
    let mid = blob.len() / 2;
    blob[mid] = blob[mid].wrapping_add(1);

    let err = match deserialize_head(&blob) {
        Err(e) => e,
        Ok(_) => unreachable!("corrupted blob should fail checksum"),
    };
    assert!(err.to_string().contains("checksum mismatch"));
}

#[test]
fn deserialize_head_accepts_legacy_blob_without_checksum() {
    // Simulate a legacy V1 blob with no checksum field by manually constructing
    // a MessagePack payload for the struct fields prior to checksum.
    let dto = SessionHeadDTO {
        version: SessionSchemaVersion::V1,
        id: "legacy".into(),
        parent_id: None,
        state: FlattenedAgentState::Idle,
        state_stack: vec![],
        extensions: std::collections::BTreeMap::new(),
        persisted_msg_count: 0,
        compaction_index: 0,
        checksum: None,
    };
    let blob = match rmp_serde::to_vec(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let loaded = match deserialize_head(&blob) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(loaded.id, "legacy");
    assert_eq!(loaded.checksum, None);
}
