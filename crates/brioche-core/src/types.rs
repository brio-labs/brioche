//! Book I — The Core Book: Fundamental types for the Brioche kernel.
//!
//! This module will contain `Session`, `AgentState`, `EngineInput`, `Effect`,
//! and related mechanical types. Definitions are populated incrementally
//! across Sprints 2–5.
//!
//! Invariants upheld:
//! - I-Core-Pure: All types are deterministic and serializable.
//! - I-Core-NoPanic: Invalid state transitions produce `BriocheError`, not panics.
//!
//! Refs: SPECS.md §2, §5
