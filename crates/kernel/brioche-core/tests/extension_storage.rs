//! Integration tests for `ExtensionStorage` and `BriocheExtensionType`.
//!
//! Refs: I-Core-ExtensionType

use std::collections::BTreeMap;

use brioche_core::{BriocheExtensionType, ExtensionStorage};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, BriocheExtensionType)]
/// Test extension state for storage roundtrips.
pub struct TestState {
    /// Simple counter field.
    pub counter: u64,
    /// Ordered map for deterministic serialization tests.
    pub tags: BTreeMap<String, u64>,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, BriocheExtensionType)]
#[brioche(critical_state)]
/// Epoch extension state for COW rollback tests.
pub struct EpochState {
    /// Current generation ID for epoch tracking.
    pub current_generation: u64,
}

#[test]
fn insert_and_get_mut_roundtrip() {
    let mut storage = ExtensionStorage::new();
    let mut tags = BTreeMap::new();
    tags.insert("a".to_string(), 1);
    let state = TestState { counter: 42, tags };
    storage.insert(state.clone());
    if let Some(retrieved) = storage.get_mut::<TestState>() {
        assert_eq!(retrieved.counter, 42);
        assert_eq!(retrieved.tags.get("a"), Some(&1));
    } else {
        assert_eq!(1, 0, "TestState not found");
    }
}

#[test]
fn get_mut_returns_none_when_empty() {
    let mut storage = ExtensionStorage::new();
    assert!(storage.get_mut::<TestState>().is_none());
}

#[test]
fn get_or_insert_default_when_empty() {
    let mut storage = ExtensionStorage::new();
    let state = storage.get_or_insert_default::<TestState>();
    assert_eq!(state.counter, 0);
    assert!(state.tags.is_empty());
}

#[test]
fn get_or_insert_default_restores_from_cold_snapshot() {
    let mut storage = ExtensionStorage::new();
    let mut tags = BTreeMap::new();
    tags.insert("key".to_string(), 123);
    let original = TestState { counter: 99, tags };
    storage.insert(original.clone());

    // Evict from hot_map to force restore from cold_snapshot.
    assert!(storage.evict_from_hot::<TestState>());
    assert!(storage.get_mut::<TestState>().is_none());

    let restored = storage.get_or_insert_default::<TestState>();
    assert_eq!(restored.counter, 99);
    assert_eq!(restored.tags.get("key"), Some(&123));
}

#[test]
fn hydrate_plugin_unknown_ext_id_returns_false() {
    let mut storage = ExtensionStorage::new();
    assert!(!storage.hydrate_plugin("unknown::id", b"blob"));
}

#[test]
fn hydrate_plugin_known_ext_id() {
    let mut storage = ExtensionStorage::new();
    storage.register::<TestState>();
    let original = TestState {
        counter: 77,
        tags: BTreeMap::new(),
    };
    let blob = match postcard::to_stdvec(&original) {
        Ok(b) => b,
        Err(_) => {
            assert_eq!(1, 0, "serialize failed");
            return;
        }
    };
    assert!(storage.hydrate_plugin(TestState::EXT_ID, &blob));
    if let Some(restored) = storage.get_mut::<TestState>() {
        assert_eq!(restored.counter, 77);
    } else {
        assert_eq!(1, 0, "TestState not found after hydrate");
    }
}

#[test]
fn cold_snapshot_persists_binary_blobs() {
    let mut storage = ExtensionStorage::new();
    let state = TestState {
        counter: 55,
        tags: BTreeMap::new(),
    };
    storage.insert(state);
    let snapshot = storage.cold_snapshot();
    assert!(snapshot.contains_key(TestState::EXT_ID));
    assert!(
        snapshot
            .get(TestState::EXT_ID)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    );
}

#[test]
fn multiple_extension_types_coexist() {
    let mut storage = ExtensionStorage::new();
    storage.insert(TestState {
        counter: 1,
        tags: BTreeMap::new(),
    });
    storage.insert(EpochState {
        current_generation: 7,
    });

    if let Some(test_state) = storage.get_mut::<TestState>() {
        assert_eq!(test_state.counter, 1);
    } else {
        assert_eq!(1, 0, "TestState not found");
    }

    if let Some(epoch_state) = storage.get_mut::<EpochState>() {
        assert_eq!(epoch_state.current_generation, 7);
    } else {
        assert_eq!(1, 0, "EpochState not found");
    }
}

#[test]
fn hydrate_plugin_corrupted_blob_fallback() {
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
    if let Some(state) = storage.get_mut::<RecoverableState>() {
        assert_eq!(state.counter, 0);
        assert!(state.tags.is_empty());
    } else {
        assert_eq!(1, 0, "state should exist after hydrate");
    }

    // The corrupted blob should still be stored in cold_snapshot.
    assert_eq!(
        storage.cold_snapshot().get(RecoverableState::EXT_ID),
        Some(&corrupted_blob)
    );
}

proptest! {
    #[test]
    fn prop_insert_get_mut_roundtrip(counter: u64, key: String, val: u64) {
        let mut storage = ExtensionStorage::new();
        let mut tags = BTreeMap::new();
        if !key.is_empty() {
            tags.insert(key, val);
        }
        let state = TestState { counter, tags };
        storage.insert(state.clone());
        if let Some(retrieved) = storage.get_mut::<TestState>() {
            prop_assert_eq!(retrieved.counter, counter);
            prop_assert_eq!(&retrieved.tags, &state.tags);
        } else {
            prop_assert_eq!(1, 0, "TestState not found");
        }
    }

    #[test]
    fn prop_get_or_insert_default_infallible(counter: u64) {
        let mut storage = ExtensionStorage::new();
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = counter;
        let retrieved = storage.get_or_insert_default::<TestState>();
        prop_assert_eq!(retrieved.counter, counter);
    }
}
