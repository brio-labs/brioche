//! Delta extraction and Redb storage protocol contracts.

use brioche_core::{ChatMessage, Session};
use brioche_shell_persistence::{
    RedbStorage, SessionHeadDTO, SessionStoreEntry, extract_delta, new_session_store,
};
use redb::ReadableDatabase;

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
    let loaded = match loaded {
        Some(v) => v,
        None => unreachable!("session should exist"),
    };
    assert_eq!(loaded.version, dto.version);
    assert_eq!(loaded.id, dto.id);
    assert_eq!(loaded.parent_id, dto.parent_id);
    assert_eq!(loaded.state, dto.state);
    assert_eq!(loaded.state_stack, dto.state_stack);
    assert_eq!(loaded.extensions, dto.extensions);
    assert_eq!(loaded.persisted_msg_count, dto.persisted_msg_count);
    assert_eq!(loaded.compaction_index, dto.compaction_index);
    assert!(
        loaded.checksum.is_some(),
        "checksum should be set after save/load"
    );
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
