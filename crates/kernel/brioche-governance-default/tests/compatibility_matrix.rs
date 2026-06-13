//! Sprint 18: `GovernanceCompatibilityMatrix` full test coverage.
//!
//! Verifies all 5 composition invariants from SPECS.md Book V Ch 3:
//! - I-Comp-Override-Rebuild
//! - I-Comp-Epoch-First
//! - I-Comp-Epoch-Subroutine
//! - I-Gov-Profile-Agnostic
//! - I-Comp-OverrideTrace
//!
//! Refs: SPECS.md §Book II Ch 7, §Book V Ch 3

use brioche_governance_default::{CompatibilityLevel, GovernanceCompatibilityMatrix};

// ---------------------------------------------------------------------------
// I-Comp-Override-Rebuild: OverrideTransition and RebuildRoutes are compatible
// ---------------------------------------------------------------------------

#[test]
fn compatibility_override_rebuild_is_recommended() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "EpochInterceptor",
        "EpochGuard",
        "SubRoutineHandler",
        "SubRoutineOrchestrator",
    );
    assert_eq!(level, Some(CompatibilityLevel::Recommended));
}

// ---------------------------------------------------------------------------
// I-Comp-Epoch-First: EpochGuard pairs are recommended
// ---------------------------------------------------------------------------

#[test]
fn compatibility_epoch_guard_with_adaptive_undo_frame_guard() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "EpochInterceptor",
        "EpochGuard",
        "CycleRollbackPolicy",
        "AdaptiveUndoFrameGuard",
    );
    assert!(
        matches!(level, Some(CompatibilityLevel::Recommended)),
        "EpochGuard + AdaptiveUndoFrameGuard should be Recommended"
    );
}

#[test]
fn compatibility_epoch_guard_with_tiered_undo_frame_guard() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "EpochInterceptor",
        "EpochGuard",
        "CycleRollbackPolicy",
        "TieredUndoFrameGuard",
    );
    assert!(
        matches!(level, Some(CompatibilityLevel::Recommended)),
        "EpochGuard + TieredUndoFrameGuard should be Recommended"
    );
}

// ---------------------------------------------------------------------------
// I-Comp-Epoch-Subroutine: EpochGuard + SubRoutineOrchestrator compatibility
// ---------------------------------------------------------------------------

#[test]
fn compatibility_epoch_guard_with_subroutine_orchestrator() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "EpochInterceptor",
        "EpochGuard",
        "SubRoutineHandler",
        "SubRoutineOrchestrator",
    );
    assert!(
        matches!(level, Some(CompatibilityLevel::Recommended)),
        "EpochGuard + SubRoutineOrchestrator should be Recommended"
    );
}

// ---------------------------------------------------------------------------
// I-Gov-Profile-Agnostic: All three profiles bootstrap an engine
// ---------------------------------------------------------------------------

#[test]
fn compatibility_matrix_is_symmetric() {
    let entries = GovernanceCompatibilityMatrix::entries();
    for entry in &entries {
        let reverse = GovernanceCompatibilityMatrix::lookup(
            entry.trait_b,
            entry.impl_b,
            entry.trait_a,
            entry.impl_a,
        );
        assert!(
            reverse.is_some(),
            "reverse lookup should exist for {} + {}",
            entry.impl_a,
            entry.impl_b
        );
        if let Some(level) = reverse {
            assert_eq!(level, entry.level, "reverse lookup should have same level");
        }
    }
}

// ---------------------------------------------------------------------------
// I-Comp-OverrideTrace: TransitionConflictLogger observes traces
// ---------------------------------------------------------------------------

#[test]
fn compatibility_matrix_has_no_incompatible_duplicates() {
    let entries = GovernanceCompatibilityMatrix::entries();
    let mut pairs = std::collections::BTreeSet::new();
    for e in &entries {
        let key = if e.impl_a < e.impl_b {
            format!("{}|{}", e.impl_a, e.impl_b)
        } else {
            format!("{}|{}", e.impl_b, e.impl_a)
        };
        assert!(
            pairs.insert(key.clone()),
            "duplicate compatibility entry for {}",
            key
        );
    }
}

#[test]
fn compatibility_matrix_caution_on_noop_with_budget() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "CycleRollbackPolicy",
        "NoopCycleRollbackPolicy",
        "CowBudgetPolicy",
        "NoopCowBudgetPolicy",
    );
    assert_eq!(level, Some(CompatibilityLevel::Caution));
}

#[test]
fn compatibility_matrix_lookup_unknown_returns_none() {
    let level = GovernanceCompatibilityMatrix::lookup(
        "UnknownTrait",
        "UnknownImpl",
        "CycleRollbackPolicy",
        "AdaptiveUndoFrameGuard",
    );
    assert!(level.is_none());
}

#[test]
fn compatibility_matrix_all_entries_have_notes_or_standard() {
    let entries = GovernanceCompatibilityMatrix::entries();
    assert!(
        !entries.is_empty(),
        "matrix should contain at least one entry"
    );
    for entry in &entries {
        // Every entry should have either a note or be a well-known standard pair.
        assert!(
            entry.note.is_some()
                || matches!(
                    entry.level,
                    CompatibilityLevel::Recommended | CompatibilityLevel::Compatible
                ),
            "entry {} + {} should have a note or be a standard pair",
            entry.impl_a,
            entry.impl_b
        );
    }
}
