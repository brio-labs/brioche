//! # Brioche Reedline — Book III
//!
//! Shared terminal infrastructure for Brioche agents.
//!
//! Provides reusable components for building terminal-based Brioche agents:
//! - [`repl`] — Reedline loop with pluggable completer
//! - [`ui`] — Terminal rendering primitives for LLM chunks
//!
//! Agent-specific orchestration (bridge, shell building, slash commands)
//! lives in each agent crate.
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: This crate performs I/O only; no kernel logic.
//! - I-Shell-Projection-Independent: UI rendering is independent of Core state.
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub mod repl;
pub mod ui;

pub use repl::SessionManager;
