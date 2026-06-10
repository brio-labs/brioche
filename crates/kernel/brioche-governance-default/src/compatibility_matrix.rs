//! GovernanceCompatibilityMatrix — Book II §Ch 7.
//!
//! Composition invariant defaults injected at init.
//!
//! This matrix documents which governance trait implementations are
//! compatible with each other and which combinations are discouraged.
//!
//! Refs: I-Comp-Override-Rebuild, I-Comp-Epoch-First

/// Compatibility level between two trait implementations.
///
/// Refs: I-Gov-Profile-Agnostic
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompatibilityLevel {
    /// Recommended and tested combination.
    Recommended,
    /// Compatible but not exhaustively tested.
    Compatible,
    /// Functional but with documented limitations.
    Caution,
    /// Incompatible — ne pas combiner.
    Incompatible,
}

/// Compatibility matrix entry.
///
/// Refs: I-Gov-Profile-Agnostic
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityEntry {
    /// Name of the first governance trait.
    pub trait_a: &'static str,
    /// Name of the first trait implementation.
    pub impl_a: &'static str,
    /// Name of the second governance trait.
    pub trait_b: &'static str,
    /// Name of the second trait implementation.
    pub impl_b: &'static str,
    /// Compatibility level between the two implementations.
    pub level: CompatibilityLevel,
    /// Optional human-readable note explaining the compatibility.
    pub note: Option<&'static str>,
}

/// Governance trait compatibility matrix.
///
/// This structure is purely documentary/constant. It does not modify
/// the kernel's runtime behavior.
///
/// Refs: I-Gov-Profile-Agnostic
pub struct GovernanceCompatibilityMatrix;

impl GovernanceCompatibilityMatrix {
    /// Returns the full matrix of known compatibilities.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn entries() -> Vec<CompatibilityEntry> {
        vec![
            // EpochGuard combinations
            CompatibilityEntry {
                trait_a: "EpochInterceptor",
                impl_a: "EpochGuard",
                trait_b: "SubRoutineHandler",
                impl_b: "SubRoutineOrchestrator",
                level: CompatibilityLevel::Recommended,
                note: Some("Standard production pair."),
            },
            CompatibilityEntry {
                trait_a: "EpochInterceptor",
                impl_a: "EpochGuard",
                trait_b: "CycleRollbackPolicy",
                impl_b: "UndoFrameGuard",
                level: CompatibilityLevel::Recommended,
                note: Some("Granular COW + epoch isolation."),
            },
            CompatibilityEntry {
                trait_a: "EpochInterceptor",
                impl_a: "EpochGuard",
                trait_b: "CycleRollbackPolicy",
                impl_b: "TieredUndoFrameGuard",
                level: CompatibilityLevel::Recommended,
                note: Some("Strict profile combination."),
            },
            // DecisionAggregator combinations
            CompatibilityEntry {
                trait_a: "DecisionAggregator",
                impl_a: "LexicographicDecisionAggregator",
                trait_b: "CycleRollbackPolicy",
                impl_b: "NoopCycleRollbackPolicy",
                level: CompatibilityLevel::Recommended,
                note: Some("Permissive profile standard pair."),
            },
            CompatibilityEntry {
                trait_a: "DecisionAggregator",
                impl_a: "NegotiationBroker",
                trait_b: "CycleRollbackPolicy",
                impl_b: "AdaptiveUndoFrameGuard",
                level: CompatibilityLevel::Recommended,
                note: Some("Multi-phase negotiation + adaptive budget."),
            },
            CompatibilityEntry {
                trait_a: "DecisionAggregator",
                impl_a: "TreeDecisionAggregator",
                trait_b: "CycleRollbackPolicy",
                impl_b: "TieredUndoFrameGuard",
                level: CompatibilityLevel::Compatible,
                note: Some("Conditional decisions + tiered rollback."),
            },
            // HookEffectConstraint combinations
            CompatibilityEntry {
                trait_a: "HookEffectConstraint",
                impl_a: "FastHookEffectConstraint",
                trait_b: "GovernanceFailoverHandler",
                impl_b: "SystemFailoverGuard",
                level: CompatibilityLevel::Recommended,
                note: Some("Strict effects + failover safety net."),
            },
            CompatibilityEntry {
                trait_a: "HookEffectConstraint",
                impl_a: "PermissiveHookEffectConstraint",
                trait_b: "GovernanceFailoverHandler",
                impl_b: "NoopGovernanceFailoverHandler",
                level: CompatibilityLevel::Recommended,
                note: Some("Permissive profile pair."),
            },
            // Incompatible combinations
            CompatibilityEntry {
                trait_a: "CycleRollbackPolicy",
                impl_a: "NoopCycleRollbackPolicy",
                trait_b: "CowBudgetPolicy",
                impl_b: "HistoricalCowBudgetPolicy",
                level: CompatibilityLevel::Caution,
                note: Some("Budget policy has no effect with noop rollback."),
            },
        ]
    }

    /// Returns the compatibility level for a given pair.
    ///
    /// Refs: I-Gov-TraitAtomic
    pub fn lookup(
        trait_a: &str,
        impl_a: &str,
        trait_b: &str,
        impl_b: &str,
    ) -> Option<CompatibilityLevel> {
        Self::entries()
            .into_iter()
            .find(|e| {
                (e.trait_a == trait_a
                    && e.impl_a == impl_a
                    && e.trait_b == trait_b
                    && e.impl_b == impl_b)
                    || (e.trait_a == trait_b
                        && e.impl_a == impl_b
                        && e.trait_b == trait_a
                        && e.impl_b == impl_a)
            })
            .map(|e| e.level)
    }
}
