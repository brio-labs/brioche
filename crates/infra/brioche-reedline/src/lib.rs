//! # Brioche Reedline
//!
//! Shared terminal infrastructure for Brioche agents.
//!
//! Provides reusable components for building terminal-based Brioche agents:
//! - [`repl`] — Reedline loop with pluggable completer
//! - [`session`] — Multi-session manager
//! - [`ui`] — Terminal rendering primitives for LLM chunks
//!
//! Agent-specific orchestration (bridge, shell building, slash commands)
//! lives in each agent crate.

pub mod repl;
pub mod session;
pub mod ui;
