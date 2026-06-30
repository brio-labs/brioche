//! Shared infrastructure for Brioche linting tools — Book V.
//!
//! Provides directory walking, CLI argument helpers, and reporting
//! utilities used by `cargo-brioche-lint` and
//! `cargo-brioche-lint-invariants`.
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.4, docs/SPECS.md §Book IV Ch 3 §3.5

#![warn(missing_docs)]

pub mod cli;
pub mod report;
pub mod walk;
