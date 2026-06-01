#![deny(clippy::unwrap_used, clippy::expect_used)]

//! # Brioche Governance — Book II
//!
//! Trait contracts and policy interfaces for the governance layer.
//! This crate defines the plugin trait system; implementations live in
//! `brioche-governance-default`.
//!
//! ## Public interface
//! - `BriochePlugin`: Core plugin trait.
//! - `EpochInterceptor`, `SubRoutineHandler`: Governance traits.
//! - `PluginResult<T>`, `PolicyDecision`, `PluginError`: Plugin effect types.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Each trait is a standalone capability.
//! - I-Gov-NoCoreMutation: Plugins never mutate `Session` directly.
//! - I-Gov-EffectExplicit: All policy decisions return typed effects.
//!
//! Refs: SPECS.md §Book II
