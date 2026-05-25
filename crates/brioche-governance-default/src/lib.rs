//! # Brioche Governance Default — Book II (Implementations)
//!
//! Reference implementations of governance traits defined in
//! `brioche-governance`. These are the default plugins shipped with
//! Brioche.
//!
//! ## Public interface
//! - `EpochGuard`: Default `EpochInterceptor` implementation.
//! - `CycleBudgetGuard`: Default `CycleBudgetPolicy` implementation.
//! - `SubRoutineHandler`: Default sub-routine lifecycle handler.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each plugin implements exactly one trait.
//! - I-Gov-NoCoreMutation: Plugins only mutate their own `ExtensionStorage` state.
//!
//! Refs: SPECS.md §Book II
