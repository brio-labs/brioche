//! `PersistenceMode` — controls `SaveSession` flush semantics.
//!
//! The shell exposes this mode so integrators can choose between
//! durability and latency.  The kernel remains agnostic.
//!
//! ## Modes
//! - `Async` (default): `SaveSession` is executed non-blockingly on
//!   a background task.  Fast, but a crash may lose the last few
//!   transitions.
//! - `Sync`: the Redb transaction is committed before the effect
//!   handler returns.  Slower, but guarantees durability.
//!
//! Refs: SPECS.md §Book III-A Ch 1, I-Shell-Persistence-Mode

/// Controls the flush behavior of `SaveSession` effects.
///
/// Injected into the shell at startup via `ShellConfig`.
///
/// Refs: I-Shell-Persistence-Mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PersistenceMode {
    /// Non-blocking save (default).
    ///
    /// The effect handler spawns a background task and returns
    /// immediately.  Best for interactive latency.
    #[default]
    Async,
    /// Blocking save.
    ///
    /// The effect handler awaits the Redb commit before returning.
    /// Best for strict durability guarantees.
    Sync,
}

impl PersistenceMode {
    /// Returns `true` if this mode requires synchronous flush.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Shell-Persistence-Mode
    pub fn is_sync(self) -> bool {
        matches!(self, PersistenceMode::Sync)
    }

    /// Returns `true` if this mode allows asynchronous flush.
    ///
    /// Complexity: O(1).
    ///
    /// Refs: I-Shell-Persistence-Mode
    pub fn is_async(self) -> bool {
        matches!(self, PersistenceMode::Async)
    }
}
