//! # Brioche Shell Projection — Book IIIc
//!
//! UI projection layer. Transforms kernel state into view-model state
//! for the Tauri / web frontend.
//!
//! ## Public interface
//! - `Projector`: Trait for state projection.
//! - `ViewModel`: Serializable UI state representation.
//!
//! ## Invariants upheld
//! - I-Shell-Projection-Pure: Projection is a pure function of `Session`.
//! - I-Shell-Projection-Deterministic: Identical inputs produce identical view-models.
//!
//! Refs: SPECS.md §Book IIIc
