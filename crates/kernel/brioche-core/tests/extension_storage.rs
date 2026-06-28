//! Integration tests for `ExtensionStorage` and `BriocheExtensionType`.
//!
//! Refs: I-Core-ExtensionType

use std::any::{Any, TypeId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use brioche_core::{BriocheExtensionType, CycleRollbackPolicy, ExtVTable, ExtensionStorage};
use proptest::prelude::*;
use serde::ser::Error as SerdeError;
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
    let result = storage.insert(state.clone());
    assert!(result.is_ok(), "TestState serializes: {result:?}");
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
    let result = storage.insert(original.clone());
    assert!(result.is_ok(), "TestState serializes: {result:?}");

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
    let result = storage.insert(state);
    assert!(result.is_ok(), "TestState serializes: {result:?}");
    let snapshot = storage.cold_snapshot();
    assert!(snapshot.contains_key(TestState::EXT_ID));
    assert!(
        snapshot
            .get(TestState::EXT_ID)
            .is_some_and(|v| !v.is_empty())
    );
}

#[test]
fn multiple_extension_types_coexist() {
    let mut storage = ExtensionStorage::new();
    let result = storage.insert(TestState {
        counter: 1,
        tags: BTreeMap::new(),
    });
    assert!(result.is_ok(), "TestState serializes: {result:?}");
    let result = storage.insert(EpochState {
        current_generation: 7,
    });
    assert!(result.is_ok(), "EpochState serializes: {result:?}");

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

struct RecordingPolicy {
    counter: Arc<AtomicUsize>,
}

impl RecordingPolicy {
    fn new() -> (Self, Arc<AtomicUsize>) {
        let counter = Arc::new(AtomicUsize::new(0));
        (
            Self {
                counter: counter.clone(),
            },
            counter,
        )
    }
}

impl CycleRollbackPolicy for RecordingPolicy {
    fn begin_hook(&mut self, _hook_name: &'static str) {}

    fn on_mutation(&mut self, _type_id: TypeId, _vtable: &ExtVTable, _current: &dyn Any) {
        self.counter.fetch_add(1, AtomicOrdering::SeqCst);
    }

    fn commit_hook(&mut self, _ext: &mut ExtensionStorage) {}

    fn rollback_hook(&mut self, _ext: &mut ExtensionStorage) {}
}

/// Sort `String` keyed pairs and remove duplicate keys.
///
/// Refs: I-Eco-OrderedCollections
fn dedup_sorted_pairs(pairs: Vec<(String, u64)>) -> Vec<(String, u64)> {
    let mut sorted = pairs;
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut unique = Vec::new();
    for (k, v) in sorted {
        if unique.last().is_some_and(|(last_k, _)| last_k == &k) {
            continue;
        }
        unique.push((k, v));
    }
    unique
}

#[test]
fn attach_rollback_policy_stores_policy_and_resets_snapshot_tracking() {
    let mut storage = ExtensionStorage::new();
    let (policy, counter) = RecordingPolicy::new();
    let result = storage.insert(TestState {
        counter: 0,
        tags: BTreeMap::new(),
    });
    assert!(result.is_ok(), "TestState serializes: {result:?}");
    storage.attach_rollback_policy(Box::new(policy));

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 1;
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 2;
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
}

#[test]
fn detach_rollback_policy_returns_policy_and_clears_tracking() {
    let mut storage = ExtensionStorage::new();
    let (policy, counter) = RecordingPolicy::new();
    let result = storage.insert(TestState {
        counter: 0,
        tags: BTreeMap::new(),
    });
    assert!(result.is_ok(), "TestState serializes: {result:?}");
    storage.attach_rollback_policy(Box::new(policy));

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 1;
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);

    let detached = storage.detach_rollback_policy();
    if let Some(policy_box) = detached {
        storage.attach_rollback_policy(policy_box);
    } else {
        assert_eq!(1, 0, "expected detached policy");
    }

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 2;
    }
    assert_eq!(
        counter.load(AtomicOrdering::SeqCst),
        2,
        "reattached policy should be notified again after tracking is cleared"
    );
}

#[test]
fn attach_rollback_policy_notifies_on_first_get_or_insert_default_mutation() {
    let mut storage = ExtensionStorage::new();
    let (policy, counter) = RecordingPolicy::new();
    storage.attach_rollback_policy(Box::new(policy));

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 7;
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);

    {
        let state = storage.get_or_insert_default::<TestState>();
        state.counter = 8;
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
}

#[test]
fn attach_rollback_policy_notifies_on_first_get_mut_mutation() {
    let mut storage = ExtensionStorage::new();
    let result = storage.insert(TestState {
        counter: 0,
        tags: BTreeMap::new(),
    });
    assert!(result.is_ok(), "TestState serializes: {result:?}");
    let (policy, counter) = RecordingPolicy::new();
    storage.attach_rollback_policy(Box::new(policy));

    {
        let state = storage.get_mut::<TestState>();
        assert!(state.is_some());
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);

    {
        let state = storage.get_mut::<TestState>();
        assert!(state.is_some());
    }
    assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
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
        let result = storage.insert(state.clone());
        prop_assert!(result.is_ok(), "TestState serializes: {result:?}");
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

proptest! {
    #[test]
    fn prop_btreemap_serialization_is_order_independent(
        pairs in prop::collection::vec((any::<String>(), any::<u64>()), 0..20)
    ) {
        let unique = dedup_sorted_pairs(pairs);

        let mut map1 = BTreeMap::new();
        let mut map2 = BTreeMap::new();
        for (k, v) in &unique {
            map1.insert(k.clone(), *v);
        }
        for (k, v) in unique.iter().rev() {
            map2.insert(k.clone(), *v);
        }

        let bytes1 = match postcard::to_stdvec(&map1) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize map1 failed");
                return Ok(());
            }
        };
        let bytes2 = match postcard::to_stdvec(&map2) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize map2 failed");
                return Ok(());
            }
        };
        prop_assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn prop_btreeset_serialization_is_order_independent(
        values in prop::collection::vec(any::<String>(), 0..20)
    ) {
        let set: BTreeSet<String> = values.iter().cloned().collect();

        let mut set1 = BTreeSet::new();
        let mut set2 = BTreeSet::new();
        for v in &set {
            set1.insert(v.clone());
        }
        for v in set.iter().rev() {
            set2.insert(v.clone());
        }

        let bytes1 = match postcard::to_stdvec(&set1) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize set1 failed");
                return Ok(());
            }
        };
        let bytes2 = match postcard::to_stdvec(&set2) {
            Ok(b) => b,
            Err(_) => {
                prop_assert!(false, "serialize set2 failed");
                return Ok(());
            }
        };
        prop_assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn prop_teststate_cold_snapshot_is_deterministic(
        pairs in prop::collection::vec((any::<String>(), any::<u64>()), 0..20)
    ) {
        let unique = dedup_sorted_pairs(pairs);

        let mut tags1 = BTreeMap::new();
        let mut tags2 = BTreeMap::new();
        for (k, v) in &unique {
            tags1.insert(k.clone(), *v);
        }
        for (k, v) in unique.iter().rev() {
            tags2.insert(k.clone(), *v);
        }

        let state1 = TestState { counter: 42, tags: tags1 };
        let state2 = TestState { counter: 42, tags: tags2 };

        let mut storage1 = ExtensionStorage::new();
        let mut storage2 = ExtensionStorage::new();
        let result1 = storage1.insert(state1);
        prop_assert!(result1.is_ok(), "TestState serializes: {result1:?}");
        let result2 = storage2.insert(state2);
        prop_assert!(result2.is_ok(), "TestState serializes: {result2:?}");

        prop_assert_eq!(storage1.cold_snapshot(), storage2.cold_snapshot());
    }

/// Serialize helper that always fails.
///
/// Used by `FailingState` to force `postcard::to_stdvec` to return an error
/// without removing the `Serialize` trait bound required by the derive macro.
fn fail_serialize<S>(_value: &u64, _serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    Err(SerdeError::custom("intentional serialization failure"))
}

/// Extension type whose `Serialize` impl always fails.
///
/// Used to verify that `ExtensionStorage::insert` surfaces serialization
/// errors instead of silently persisting an empty blob.
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize, BriocheExtensionType)]
pub struct FailingState {
    /// Dummy counter field whose serialization is forced to fail.
    #[serde(serialize_with = "fail_serialize")]
    pub counter: u64,
}

#[test]
fn insert_fails_on_serialization_error() {
    let mut storage = ExtensionStorage::new();
    let result = storage.insert(FailingState { counter: 1 });
    assert!(
        result.is_err(),
        "insert should fail when serialization fails"
    );
    assert!(
        storage.get_mut::<FailingState>().is_none(),
        "value should not be inserted into hot_map on serialization failure"
    );
    assert!(
        !storage.cold_snapshot().contains_key(FailingState::EXT_ID),
        "no empty blob should be persisted on serialization failure"
    );
}
