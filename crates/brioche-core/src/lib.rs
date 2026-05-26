#![deny(clippy::unwrap_used, clippy::expect_used)]

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

// Allow proc-macro generated code to reference `::brioche_core` from
// inside the crate itself.
extern crate self as brioche_core;

pub mod extension;
pub mod types;

pub use extension::{BriocheExtensionType, ExtVTable, ExtensionStorage, SnapshotStrategy};
pub use types::{
    ActiveToolCall, AgentState, AgentStateTag, BriocheError, ChatMessage, Effect, EngineInput,
    ErrorCode, ExecutionPath, HistoryEdit, PluginError, PluginResult, PolicyDecision, Session,
    SessionRegistry, SessionSnapshot, StreamAction, StreamEvent, SubRoutineHandle,
    ToolCallDescriptor, ToolOutcome, ToolResultDTO, seal,
};

// Re-export dependencies so that proc-macro generated code and users
// can reference them through brioche_core without adding them to
// their own Cargo.toml.
pub use postcard;
pub use serde;

// Re-export the derive macro so users can `use brioche_core::BriocheExtensionType;`
// and apply `#[derive(BriocheExtensionType)]` with a single import.
pub use brioche_macro::BriocheExtensionType;
