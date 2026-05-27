//! GovernanceCompatibilityMatrix — Book II §Ch 7.
//!
//! Composition invariant defaults injected at init.
//!
//! This matrix documents which governance trait implementations are
//! compatible with each other and which combinations are discouraged.
//!
//! Refs: I-Comp-Override-Rebuild, I-Comp-Epoch-First

/// Niveau de compatibilité entre deux implémentations de traits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompatibilityLevel {
    /// Combinationsion recommandée et testée.
    Recommended,
    /// Compatible mais non testé exhaustivement.
    Compatible,
    /// Fonctionnel mais avec limitations documentées.
    Caution,
    /// Incompatible — ne pas combiner.
    Incompatible,
}

/// Entrée de la matrice de compatibilité.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityEntry {
    pub trait_a: &'static str,
    pub impl_a: &'static str,
    pub trait_b: &'static str,
    pub impl_b: &'static str,
    pub level: CompatibilityLevel,
    pub note: Option<&'static str>,
}

/// Matrice de compatibilité des traits de gouvernance.
///
/// Cette structure est purement documentaire/constante. Elle ne modifie
/// pas le comportement runtime du kernel.
pub struct GovernanceCompatibilityMatrix;

impl GovernanceCompatibilityMatrix {
    /// Retourne la matrice complète des compatibilités connues.
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

    /// Retourne le niveau de compatibilité pour une paire donnée.
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
