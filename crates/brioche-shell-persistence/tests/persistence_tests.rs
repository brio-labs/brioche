//! Integration tests for `brioche-shell-persistence`.
//!
//! Covers DTO roundtrips, Redb storage, compression, delta protocol,
//! and the `SubRoutineCache`.
//!
//! Refs: SPECS.md §Book III-B, I-Persist-Idempotence

use brioche_core::{AgentState, ChatMessage, Session};
use brioche_shell_persistence::{
    COMPRESSION_THRESHOLD, FlattenedAgentState, RedbStorage, SessionHeadDTO, SessionSchemaVersion,
    SessionStoreEntry, SubRoutineCache, deserialize_head, extract_delta, maybe_compress,
    maybe_decompress, new_session_store,
};
use redb::ReadableDatabase;
use std::num::NonZeroUsize;

// ---------------------------------------------------------------------------
// DTO conversion
// ---------------------------------------------------------------------------

#[test]
fn session_head_dto_from_idle_session() {
    let session = Session::new("test-1");
    let dto = SessionHeadDTO::from_session(&session);

    assert_eq!(dto.id, "test-1");
    assert_eq!(dto.version, SessionSchemaVersion::V1);
    assert!(matches!(dto.state, FlattenedAgentState::Idle));
    assert!(dto.state_stack.is_empty());
    assert_eq!(dto.persisted_msg_count, 0);
    assert_eq!(dto.compaction_index, 0);
}

#[test]
fn session_head_dto_flattened_state_stack() {
    let mut session = Session::new("test-2");
    session
        .push_state(AgentState::Predicting { generation_id: 7 })
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    session
        .push_state(AgentState::ExecutingTools { generation_id: 7 })
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    let dto = SessionHeadDTO::from_session(&session);

    assert_eq!(dto.state_stack.len(), 2);
    assert!(matches!(dto.state_stack[0], FlattenedAgentState::Idle));
    assert!(matches!(
        dto.state_stack[1],
        FlattenedAgentState::Predicting { generation_id: 7 }
    ));
    assert!(matches!(
        dto.state,
        FlattenedAgentState::ExecutingTools { generation_id: 7 }
    ));
}

#[test]
fn session_head_dto_subroutine_handle() {
    let mut session = Session::new("test-3");
    session.state = AgentState::SubRoutine(brioche_core::SubRoutineHandle::new("child-42"));

    let dto = SessionHeadDTO::from_session(&session);

    assert!(matches!(
        dto.state,
        FlattenedAgentState::SubRoutine(ref s) if s == "child-42"
    ));
}

// ---------------------------------------------------------------------------
// Serialization / compression
// ---------------------------------------------------------------------------

#[test]
fn compress_large_payload() {
    let data = vec![0u8; COMPRESSION_THRESHOLD + 100];
    let compressed = maybe_compress(data.clone()).unwrap_or_else(|e| unreachable!("{:?}", e));

    // First byte is the compression flag.
    assert_eq!(compressed[0], 1);
    // Compressed should be smaller than original.
    assert!(compressed.len() < data.len());

    let decompressed = maybe_decompress(&compressed).unwrap_or_else(|e| unreachable!("{:?}", e));
    assert_eq!(decompressed, data);
}

#[test]
fn passthrough_small_payload() {
    let data = vec![1u8, 2, 3];
    let compressed = maybe_compress(data.clone()).unwrap_or_else(|e| unreachable!("{:?}", e));

    assert_eq!(compressed[0], 0);
    assert_eq!(compressed[1..], data);

    let decompressed = maybe_decompress(&compressed).unwrap_or_else(|e| unreachable!("{:?}", e));
    assert_eq!(decompressed, data);
}

#[test]
fn decompress_legacy_no_flag() {
    // Data written before the flag prefix was introduced.
    let raw = vec![7u8, 8, 9];
    let decompressed = maybe_decompress(&raw).unwrap_or_else(|e| unreachable!("{:?}", e));
    assert_eq!(decompressed, raw);
}

#[test]
fn session_head_serialization_roundtrip() {
    let session = Session::new("roundtrip");
    let dto = SessionHeadDTO::from_session(&session);
    let blob =
        brioche_shell_persistence::serialize_head(&dto).unwrap_or_else(|e| unreachable!("{:?}", e));
    let restored = deserialize_head(&blob).unwrap_or_else(|e| unreachable!("{:?}", e));

    assert_eq!(dto, restored);
}

// ---------------------------------------------------------------------------
// Delta extraction
// ---------------------------------------------------------------------------

#[test]
fn extract_delta_empty() {
    let session = Session::new("delta-empty");
    assert!(extract_delta(&session).is_empty());
}

#[test]
fn extract_delta_non_empty() {
    let mut session = Session::new("delta-non-empty");
    session.history.push(ChatMessage::User {
        content: "hello".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "world".into(),
    });
    session.persisted_msg_count = 1;

    let delta = extract_delta(&session);
    assert_eq!(delta.len(), 1);
    assert!(matches!(
        delta[0],
        ChatMessage::Assistant { ref content } if content == "world"
    ));
}

// ---------------------------------------------------------------------------
// RedbStorage roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn redb_save_and_load_session_head() {
    let tmp = tempfile::NamedTempFile::new().unwrap_or_else(|e| unreachable!("{:?}", e));
    let store = new_session_store();
    let storage = RedbStorage::new(tmp.path(), store).unwrap_or_else(|e| unreachable!("{:?}", e));

    let session = Session::new("redb-test");
    let dto = SessionHeadDTO::from_session(&session);

    storage
        .save_session_dto(&dto)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    let loaded = storage
        .load_session("redb-test")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    assert_eq!(loaded, Some(dto));
}

#[tokio::test]
async fn redb_save_and_load_messages() {
    let tmp = tempfile::NamedTempFile::new().unwrap_or_else(|e| unreachable!("{:?}", e));
    let store = new_session_store();
    let storage = RedbStorage::new(tmp.path(), store).unwrap_or_else(|e| unreachable!("{:?}", e));

    let msgs = vec![
        ChatMessage::User {
            content: "first".into(),
        },
        ChatMessage::Assistant {
            content: "second".into(),
        },
    ];

    storage
        .save_messages("sess-1", &msgs, 0)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    let m0 = storage
        .load_message("sess-1", 0)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    let m1 = storage
        .load_message("sess-1", 1)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    assert_eq!(m0, Some(msgs[0].clone()));
    assert_eq!(m1, Some(msgs[1].clone()));
}

#[tokio::test]
async fn redb_load_messages_for_session_sorted() {
    let tmp = tempfile::NamedTempFile::new().unwrap_or_else(|e| unreachable!("{:?}", e));
    let store = new_session_store();
    let storage = RedbStorage::new(tmp.path(), store).unwrap_or_else(|e| unreachable!("{:?}", e));

    let msgs = [
        ChatMessage::User {
            content: "a".into(),
        },
        ChatMessage::User {
            content: "b".into(),
        },
        ChatMessage::User {
            content: "c".into(),
        },
    ];

    // Save out of order to verify deterministic sort on load.
    storage
        .save_messages("sess-2", &[msgs[2].clone()], 2)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    storage
        .save_messages("sess-2", &[msgs[0].clone()], 0)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    storage
        .save_messages("sess-2", &[msgs[1].clone()], 1)
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    let loaded = storage
        .load_messages_for_session("sess-2")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0].0, 0);
    assert_eq!(loaded[1].0, 1);
    assert_eq!(loaded[2].0, 2);
    assert_eq!(loaded[0].1, msgs[0]);
    assert_eq!(loaded[1].1, msgs[1]);
    assert_eq!(loaded[2].1, msgs[2]);
}

#[tokio::test]
async fn redb_save_plugin_blob_roundtrip() {
    use brioche_shell_runtime::Persistence;

    let tmp = tempfile::NamedTempFile::new().unwrap_or_else(|e| unreachable!("{:?}", e));
    let store = new_session_store();
    let storage = RedbStorage::new(tmp.path(), store).unwrap_or_else(|e| unreachable!("{:?}", e));

    let data = vec![0xAB, 0xCD, 0xEF];
    storage
        .save_plugin_blob("plugin::x", data.clone())
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    // Drop storage to close the database so we can reopen it for verification.
    drop(storage);

    // Verify via direct Redb read.
    let loaded = tokio::task::spawn_blocking({
        let path = tmp.path().to_path_buf();
        move || {
            let db = redb::Database::open(path).unwrap_or_else(|e| unreachable!("{:?}", e));
            let txn = db.begin_read().unwrap_or_else(|e| unreachable!("{:?}", e));
            let table = txn
                .open_table(brioche_shell_persistence::schema::BLOBS_TABLE)
                .unwrap_or_else(|e| unreachable!("{:?}", e));
            let guard = table
                .get("plugin::x")
                .unwrap_or_else(|e| unreachable!("{:?}", e));
            guard.map(|g| g.value().to_vec())
        }
    })
    .await
    .unwrap_or_else(|e| unreachable!("{:?}", e));

    assert_eq!(loaded, Some(data));
}

#[tokio::test]
async fn persistence_trait_save_session_with_delta() {
    use brioche_shell_runtime::Persistence;

    let tmp = tempfile::NamedTempFile::new().unwrap_or_else(|e| unreachable!("{:?}", e));
    let store = new_session_store();
    let storage =
        RedbStorage::new(tmp.path(), store.clone()).unwrap_or_else(|e| unreachable!("{:?}", e));

    let mut session = Session::new("trait-test");
    session.history.push(ChatMessage::User {
        content: "msg-0".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "msg-1".into(),
    });

    let entry = SessionStoreEntry {
        head: SessionHeadDTO::from_session(&session),
        messages: session.history.clone(),
    };
    storage.update_session(entry).await;

    // First save: head + 2 messages.
    storage
        .save_session("trait-test")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    // Add a third message, update store.
    session.history.push(ChatMessage::User {
        content: "msg-2".into(),
    });
    let entry2 = SessionStoreEntry {
        head: {
            let mut h = SessionHeadDTO::from_session(&session);
            h.persisted_msg_count = 2; // simulate prior save watermark
            h
        },
        messages: session.history.clone(),
    };
    storage.update_session(entry2).await;

    // Second save: only the delta (msg-2).
    storage
        .save_session("trait-test")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));

    // Verify head.
    let loaded_head = storage
        .load_session("trait-test")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    assert!(loaded_head.is_some());

    // Verify all messages are present.
    let loaded_msgs = storage
        .load_messages_for_session("trait-test")
        .await
        .unwrap_or_else(|e| unreachable!("{:?}", e));
    assert_eq!(loaded_msgs.len(), 3);
    assert_eq!(loaded_msgs[0].0, 0);
    assert_eq!(loaded_msgs[1].0, 1);
    assert_eq!(loaded_msgs[2].0, 2);
}

// ---------------------------------------------------------------------------
// SubRoutineCache
// ---------------------------------------------------------------------------

#[test]
fn subroutine_cache_l1_l2_promotion() {
    let mut cache =
        SubRoutineCache::new(NonZeroUsize::new(2).unwrap_or_else(|| unreachable!("2 is non-zero")));

    let dto = SessionHeadDTO::from_session(&Session::new("sub-1"));
    cache.insert("sub-1".into(), dto.clone());

    assert!(cache.contains("sub-1"));
    assert_eq!(cache.l2_len(), 1);

    cache.promote_to_l1("sub-1".into());
    assert_eq!(cache.l1_len(), 1);
    assert_eq!(cache.l2_len(), 0);
    assert!(cache.get("sub-1").is_some());

    cache.demote_to_l2("sub-1".into());
    assert_eq!(cache.l1_len(), 0);
    assert_eq!(cache.l2_len(), 1);
}

#[test]
fn subroutine_cache_lru_eviction() {
    let mut cache =
        SubRoutineCache::new(NonZeroUsize::new(2).unwrap_or_else(|| unreachable!("2 is non-zero")));

    for i in 0..4 {
        let dto = SessionHeadDTO::from_session(&Session::new(format!("sub-{}", i)));
        cache.insert(format!("sub-{}", i), dto);
    }

    // L2 capacity is 2, so only the two most recent remain.
    assert!(!cache.contains("sub-0"));
    assert!(!cache.contains("sub-1"));
    assert!(cache.contains("sub-2"));
    assert!(cache.contains("sub-3"));
}

#[test]
fn subroutine_cache_remove() {
    let mut cache =
        SubRoutineCache::new(NonZeroUsize::new(2).unwrap_or_else(|| unreachable!("2 is non-zero")));

    let dto = SessionHeadDTO::from_session(&Session::new("sub-x"));
    cache.insert("sub-x".into(), dto.clone());
    cache.promote_to_l1("sub-x".into());

    let removed = cache.remove("sub-x");
    assert!(removed.is_some());
    assert!(!cache.contains("sub-x"));
}
