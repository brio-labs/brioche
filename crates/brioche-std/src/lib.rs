//! # Brioche Standard Plugins — Book IV
//!
//! Reference plugin implementations and ecosystem utilities.
//! All types here are policy, not mechanism.
//!
//! ## Public interface
//! - Standard governance plugins (rate limiter, quarantine, etc.).
//! - `brioche_std` prelude for plugin authors.
//!
//! ## Invariants upheld
//! - I-Eco-ExtensionOverMod: Plugins extend via traits, never modify Core.
//! - I-Eco-OrderedCollections: All persisted state uses ordered collections.
//!
//! Refs: SPECS.md §Book IV
