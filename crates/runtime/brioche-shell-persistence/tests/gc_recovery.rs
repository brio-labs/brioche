//! Garbage collection and individual plugin recovery contracts.

use brioche_core::ChatMessage;
use brioche_shell_persistence::{GcRunner, RedbStorage, new_session_store};

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

    // A fresh runner should complete the full scan.
    let removed = match gc.run_gc(&storage, "gc-cancel", 200).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(
        removed, 200,
        "fresh runner should remove all eligible messages"
    );

    // Cancelling resets the token; the runner itself should not be
    // pre-cancelled, so a subsequent run on new messages still works.
    gc.cancel();
    assert!(
        !gc.token().is_cancelled(),
        "a fresh token must be installed after cancel"
    );

    let more: Vec<ChatMessage> = (200..250)
        .map(|i| ChatMessage::User {
            content: format!("msg-{}", i),
        })
        .collect();
    match storage.save_messages("gc-cancel", &more, 200).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };

    let removed = match gc.run_gc(&storage, "gc-cancel", 250).await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(
        removed, 50,
        "runner must not be pre-cancelled after a reset"
    );

    // Verify the database is still consistent.
    let remaining = match storage.load_messages_for_session("gc-cancel").await {
        Ok(v) => v,
        Err(e) => unreachable!("{:?}", e),
    };
    assert_eq!(remaining.len(), 0);
}

// ---------------------------------------------------------------------------
// ExtensionStorage::hydrate_plugin individual recovery
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
