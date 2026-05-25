//! # Brioche Shell Persistence — Book IIIb
//!
//! Persistence layer for shell-side state. Handles serialization,
//! disk I/O, and hydration of `ExtensionStorage` snapshots.
//!
//! ## Public interface
//! - `PersistedStorage`: Disk-backed storage engine.
//! - `SnapshotWriter` / `SnapshotReader`: Streaming serialization.
//!
//! ## Invariants upheld
//! - I-Shell-Persistence-Atomic: Writes are atomic (tempfile + rename).
//! - I-Shell-Persistence-Versioned: Snapshots include a format version header.
//!
//! Refs: SPECS.md §Book IIIb
