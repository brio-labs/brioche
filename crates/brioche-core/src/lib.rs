//! Book I — The Core Book: Synchronous kernel and pure mechanisms.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verified extension types via `BriocheExtensionType`.
//! - I-Core-ExtO1: O(1) extension access by `TypeId`.
//! - I-Core-VTableClone: VTable provides `clone_box` for COW rollback.
//! - I-Core-Pure: Kernel never produces side effects.
//! - I-Core-NoPanic: `transition()` returns `Vec<Effect>`, never panics.
//!
//! Refs: SPECS.md §Book I

pub mod extension;
pub mod types;

pub use extension::{BriocheExtensionType, SnapshotStrategy};

// Re-export the derive macro so users can `use brioche_core::BriocheExtensionType;`
// and apply `#[derive(BriocheExtensionType)]` with a single import.
pub use brioche_macro::BriocheExtensionType;
