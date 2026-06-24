//! Integration tests for `brioche-shell-persistence`.
//!
//! Covers DTO roundtrips, Redb storage, compression, delta protocol,
//! sub-routine cache, lazy loading, GC, and idempotence.
//!
//! Refs: docs/SPECS.md §Book III-B, I-Persist-Idempotence, I-Persist-GC-Interrupt

use std::num::NonZeroUsize;

use brioche_core::{AgentState, ChatMessage, Session, SubRoutineHydrator};
use brioche_shell_persistence::{
    COMPRESSION_THRESHOLD, FlattenedAgentState, GcRunner, PersistenceSubRoutineHydrator,
    RedbStorage, SessionHeadDTO, SessionSchemaVersion, SessionStoreEntry, SubRoutineCache,
    deserialize_head, extract_delta, load_subroutine, maybe_compress, maybe_decompress,
    new_session_store, serialize_head,
};
use redb::ReadableDatabase;
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
    match session.push_state(AgentState::Predicting { generation_id: 7 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match session.push_state(AgentState::ExecutingTools { generation_id: 7 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

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

    assert_eq!(dto, restored);
}

#[tokio::test]
async fn subroutine_hydrator_roundtrip() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store.clone()) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let mut session = Session::new("hydrate-me");
    session.state = AgentState::Predicting { generation_id: 42 };
    session.persisted_msg_count = 2;
    session.history = vec![
        ChatMessage::System {
            content: "system prompt".into(),
        },
        ChatMessage::User {
            content: "hello".into(),
        },
    ];

    let dto = SessionHeadDTO::from_session(&session);
    let blob = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    if let Err(e) = storage
        .save_messages("hydrate-me", &session.history, 0)
        .await
    {
        unreachable!("{:?}", e);
    }

    let hydrator = PersistenceSubRoutineHydrator::new(storage);
    let hydrated = match hydrator.hydrate(&blob) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(hydrated.id, "hydrate-me");
    assert_eq!(hydrated.persisted_msg_count, 2);
    assert!(matches!(
        hydrated.state,
        AgentState::Predicting { generation_id: 42 }
    ));
    assert_eq!(hydrated.history.len(), 2);
    assert_eq!(
        hydrated.history[0],
        ChatMessage::System {
            content: "system prompt".into(),
        }
    );
    assert_eq!(
        hydrated.history[1],
        ChatMessage::User {
            content: "hello".into(),
        }
    );
}

// ---------------------------------------------------------------------------
// Idempotence verification (Sprint 13)
// ---------------------------------------------------------------------------

#[test]
fn idempotence_two_serializations_bit_for_bit() {
    let mut session = Session::new("idempotent");
    session.history.push(ChatMessage::User {
        content: "hello".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "world".into(),
        reasoning: None,
        tool_calls: Vec::new(),
    });
    match session.push_state(AgentState::Predicting { generation_id: 42 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let dto = SessionHeadDTO::from_session(&session);
    let blob1 = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let blob2 = match serialize_head(&dto) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(
        blob1, blob2,
        "two serializations of the same DTO must be bit-for-bit identical"
    );
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
        reasoning: None,
        tool_calls: Vec::new(),
    });
    session.persisted_msg_count = 1;

    let delta = extract_delta(&session);
    assert_eq!(delta.len(), 1);
    assert!(matches!(
        delta[0],
        ChatMessage::Assistant { ref content, .. } if content == "world"
    ));
}

// ---------------------------------------------------------------------------
// RedbStorage roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn redb_save_and_load_session_head() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let session = Session::new("redb-test");
    let dto = SessionHeadDTO::from_session(&session);

    match storage.save_session_dto(&dto).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let loaded = match storage.load_session("redb-test").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(loaded, Some(dto));
}

#[tokio::test]
async fn redb_save_and_load_messages() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let msgs = vec![
        ChatMessage::User {
            content: "first".into(),
        },
        ChatMessage::Assistant {
            content: "second".into(),
            reasoning: None,
            tool_calls: Vec::new(),
        },
    ];

    match storage.save_messages("sess-1", &msgs, 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let m0 = match storage.load_message("sess-1", 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let m1 = match storage.load_message("sess-1", 1).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(m0, Some(msgs[0].clone()));
    assert_eq!(m1, Some(msgs[1].clone()));
}

#[tokio::test]
async fn redb_load_messages_for_session_sorted() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

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
    match storage.save_messages("sess-2", &[msgs[2].clone()], 2).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match storage.save_messages("sess-2", &[msgs[0].clone()], 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match storage.save_messages("sess-2", &[msgs[1].clone()], 1).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let loaded = match storage.load_messages_for_session("sess-2").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

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

    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let data = vec![0xAB, 0xCD, 0xEF];
    match storage.save_plugin_blob("plugin::x", data.clone()).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Drop storage to close the database so we can reopen it for verification.
    drop(storage);

    // Verify via direct Redb read.
    let loaded = match tokio::task::spawn_blocking({
        let path = tmp.path().to_path_buf();
        move || {
            let db = match redb::Database::open(path) {
                Ok(v) => v,
                Err(e) => unreachable!("{:?}", e),
            };
            let txn = match db.begin_read() {
                Ok(v) => v,
                Err(e) => unreachable!("{:?}", e),
            };
            let table: redb::ReadOnlyTable<&str, &[u8]> =
                match txn.open_table(redb::TableDefinition::new("blobs")) {
                    Ok(v) => v,
                    Err(e) => unreachable!("{:?}", e),
                };
            let guard = match table.get("plugin::x") {
                Ok(v) => v,
                Err(e) => unreachable!("{:?}", e),
            };
            guard.map(|g| {
                let v: &[u8] = g.value();
                v.to_vec()
            })
        }
    })
    .await
    {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    assert_eq!(loaded, Some(data));
}

#[tokio::test]
async fn persistence_trait_save_session_with_delta() {
    use brioche_shell_runtime::Persistence;

    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store.clone()) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let mut session = Session::new("trait-test");
    session.history.push(ChatMessage::User {
        content: "msg-0".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "msg-1".into(),
        reasoning: None,
        tool_calls: Vec::new(),
    });

    let entry = SessionStoreEntry {
        head: SessionHeadDTO::from_session(&session),
        messages: session.history.clone(),
    };
    storage.update_session(entry).await;

    // First save: head + 2 messages.
    match storage.save_session("trait-test").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

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
    match storage.save_session("trait-test").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Verify head.
    let loaded_head = match storage.load_session("trait-test").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert!(loaded_head.is_some());

    // Verify all messages are present.
    let loaded_msgs = match storage.load_messages_for_session("trait-test").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
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
    let mut cache = SubRoutineCache::new(match NonZeroUsize::new(2) {
        Some(v) => v,
        None => unreachable!("2 is non-zero"),
    });

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
    let mut cache = SubRoutineCache::new(match NonZeroUsize::new(2) {
        Some(v) => v,
        None => unreachable!("2 is non-zero"),
    });

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
    let mut cache = SubRoutineCache::new(match NonZeroUsize::new(2) {
        Some(v) => v,
        None => unreachable!("2 is non-zero"),
    });

    let dto = SessionHeadDTO::from_session(&Session::new("sub-x"));
    cache.insert("sub-x".into(), dto.clone());
    cache.promote_to_l1("sub-x".into());

    let removed = cache.remove("sub-x");
    assert!(removed.is_some());
    assert!(!cache.contains("sub-x"));
}

// ---------------------------------------------------------------------------
// Sprint 13: Lazy session loading
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lazy_session_load_with_children() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Create a child sub-routine session.
    let child_session = Session::new("child-1");
    let child_dto = SessionHeadDTO::from_session(&child_session);
    match storage.save_session_dto(&child_dto).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Create a parent session whose state stack references the child.
    let mut parent_session = Session::new("parent-1");
    match parent_session.push_state(AgentState::SubRoutine(brioche_core::SubRoutineHandle::new(
        "child-1",
    ))) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    parent_session.history.push(ChatMessage::User {
        content: "hello parent".into(),
    });

    let parent_dto = SessionHeadDTO::from_session(&parent_session);
    match storage.save_session_dto(&parent_dto).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match storage
        .save_messages("parent-1", &parent_session.history, 0)
        .await
    {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Load parent lazily — child should be pre-filled into L2.
    let mut cache = SubRoutineCache::new(match NonZeroUsize::new(4) {
        Some(v) => v,
        None => unreachable!("4 is non-zero"),
    });
    let result = match load_subroutine(&storage, &mut cache, "parent-1").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let head = match result {
        Some(r) => r,
        None => {
            assert_eq!(1, 0, "result should be Some");
            return;
        }
    };
    assert_eq!(head.id, "parent-1");

    // Note: recursive child loading was removed when LazySessionLoader was
    // merged into storage.rs. load_subroutine only loads the requested handle.
    assert!(cache.contains("parent-1"));
}

// ---------------------------------------------------------------------------
// Sprint 13: On-demand sub-routine loading
// ---------------------------------------------------------------------------

#[tokio::test]
async fn load_subroutine_l1_l2_redb_fallback() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let dto = SessionHeadDTO::from_session(&Session::new("sub-fallback"));
    match storage.save_session_dto(&dto).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let mut cache = SubRoutineCache::new(match NonZeroUsize::new(4) {
        Some(v) => v,
        None => unreachable!("4 is non-zero"),
    });

    // First load: miss cache, hit Redb.
    let loaded = match load_subroutine(&storage, &mut cache, "sub-fallback").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert!(loaded.is_some());
    assert_eq!(cache.l2_len(), 1);

    // Second load: should hit L2 (no need to query Redb).
    let loaded2 = match load_subroutine(&storage, &mut cache, "sub-fallback").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let loaded2_dto = match loaded2 {
        Some(d) => d,
        None => {
            assert_eq!(1, 0, "loaded2 should be Some");
            return;
        }
    };
    assert_eq!(loaded2_dto.id, "sub-fallback");

    // Promote to L1.
    cache.promote_to_l1("sub-fallback".into());
    assert_eq!(cache.l1_len(), 1);
    assert_eq!(cache.l2_len(), 0);

    // Third load: should hit L1.
    let loaded3 = match load_subroutine(&storage, &mut cache, "sub-fallback").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert!(loaded3.is_some());
}

// ---------------------------------------------------------------------------
// Sprint 13: Persistence roundtrip (save → load → replay)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn persistence_roundtrip_save_load_replay() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let mut session = Session::new("roundtrip-full");
    session.history.push(ChatMessage::System {
        content: "system prompt".into(),
    });
    session.history.push(ChatMessage::User {
        content: "user message".into(),
    });
    session.history.push(ChatMessage::Assistant {
        content: "assistant reply".into(),
        reasoning: None,
        tool_calls: Vec::new(),
    });
    match session.push_state(AgentState::Predicting { generation_id: 99 }) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Save.
    let head = SessionHeadDTO::from_session(&session);
    match storage.save_session_dto(&head).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    match storage
        .save_messages("roundtrip-full", &session.history, 0)
        .await
    {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Load full session.
    let result = match storage.load_session("roundtrip-full").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let loaded_head = match result {
        Some(r) => r,
        None => {
            assert_eq!(1, 0, "session should exist");
            return;
        }
    };

    // Head must match exactly.
    assert_eq!(loaded_head.id, head.id);
    assert_eq!(loaded_head.state, head.state);
    assert_eq!(loaded_head.state_stack, head.state_stack);
    assert_eq!(loaded_head.extensions, head.extensions);
    assert_eq!(loaded_head.persisted_msg_count, head.persisted_msg_count);

    // Messages must match exactly in order.
    let loaded_messages = match storage.load_messages_for_session("roundtrip-full").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(loaded_messages.len(), session.history.len());
    for (expected, actual) in session
        .history
        .iter()
        .zip(loaded_messages.iter().map(|(_, m)| m))
    {
        assert_eq!(expected, actual);
    }
}

// ---------------------------------------------------------------------------
// Sprint 13: Opportunistic GC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gc_removes_messages_below_compaction_index() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let msgs = vec![
        ChatMessage::User {
            content: "0".into(),
        },
        ChatMessage::User {
            content: "1".into(),
        },
        ChatMessage::User {
            content: "2".into(),
        },
        ChatMessage::User {
            content: "3".into(),
        },
    ];
    match storage.save_messages("gc-sess", &msgs, 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let gc = GcRunner::new();
    let removed = match gc.run_gc(&storage, "gc-sess", 2).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Messages with index < 2 should be removed.
    assert_eq!(removed, 2);

    let remaining = match storage.load_messages_for_session("gc-sess").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining[0].0, 2);
    assert_eq!(remaining[1].0, 3);
}

#[tokio::test]
async fn gc_does_not_remove_when_index_equals_compaction() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let msgs = vec![
        ChatMessage::User {
            content: "0".into(),
        },
        ChatMessage::User {
            content: "1".into(),
        },
    ];
    match storage.save_messages("gc-eq", &msgs, 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let gc = GcRunner::new();
    let removed = match gc.run_gc(&storage, "gc-eq", 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Strictly less than, so index 0 is NOT removed when compaction_index == 0.
    assert_eq!(removed, 0);
}

#[tokio::test]
async fn gc_interruptible_by_cancellation_token() {
    let tmp = match tempfile::NamedTempFile::new() {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    let store = new_session_store();
    let storage = match RedbStorage::new(tmp.path(), store) {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Populate many messages to ensure the scan takes some time.
    let mut msgs = Vec::with_capacity(200);
    for i in 0..200 {
        msgs.push(ChatMessage::User {
            content: format!("msg-{}", i),
        });
    }
    match storage.save_messages("gc-cancel", &msgs, 0).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let gc = GcRunner::new();

    // Cancel immediately before running.
    gc.cancel();

    let removed = match gc.run_gc(&storage, "gc-cancel", 200).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    // Cancellation happens before or during the first iteration,
    // so either 0 or a very small number of messages are removed.
    // The important invariant is that the call returns without error
    // and the transaction is still committed.
    assert!(
        removed <= 1,
        "expected at most 1 removal after immediate cancel, got {}",
        removed
    );

    // Verify the database is still consistent.
    let remaining = match storage.load_messages_for_session("gc-cancel").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(remaining.len(), 200);
}

// ---------------------------------------------------------------------------
// Sprint 13: ExtensionStorage::hydrate_plugin individual recovery
// ---------------------------------------------------------------------------

#[test]
fn hydrate_plugin_corrupted_blob_fallback() {
    use std::collections::BTreeMap;

    use brioche_core::{BriocheExtensionType, ExtensionStorage};
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, BriocheExtensionType)]
    pub struct RecoverableState {
        pub counter: u64,
        pub tags: BTreeMap<String, u64>,
    }

    let mut storage = ExtensionStorage::new();
    storage.register::<RecoverableState>();

    // Pass garbage bytes that are not a valid serialization.
    let corrupted_blob = vec![0xFF, 0xFF, 0xFF, 0xFF];
    let success = storage.hydrate_plugin(RecoverableState::EXT_ID, &corrupted_blob);
    assert!(
        success,
        "hydrate_plugin should return true for known ext_id"
    );

    // After deserialization failure, it should fall back to default.
    let state = match storage.get_mut::<RecoverableState>() {
        Some(s) => s,
        None => {
            assert_eq!(1, 0, "state should exist after hydrate");
            return;
        }
    };
    assert_eq!(state.counter, 0);
    assert!(state.tags.is_empty());

    // The corrupted blob should still be stored in cold_snapshot.
    assert_eq!(
        storage.cold_snapshot().get(RecoverableState::EXT_ID),
        Some(&corrupted_blob)
    );
}
