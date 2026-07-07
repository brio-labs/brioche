//! Sub-routine cache, lazy loading, hydration, and replay roundtrip contracts.

use std::num::NonZeroUsize;

use brioche_core::{AgentState, ChatMessage, Session, SubRoutineHydrator};
use brioche_shell_persistence::{
    PersistenceSubRoutineHydrator, RedbStorage, SessionHeadDTO, SubRoutineCache, load_subroutine,
    new_session_store, serialize_head,
};

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
// Lazy session loading
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
// On-demand sub-routine loading
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
// Persistence roundtrip (save → load → replay)
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
