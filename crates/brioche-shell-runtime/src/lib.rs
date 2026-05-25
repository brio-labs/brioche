//! # Brioche Shell Runtime — Book IIIa
//!
//! Async runtime, networking, and system I/O. The shell is the only
//! layer permitted to perform side effects.
//!
//! ## Public interface
//! - `BriocheShell`: Main async runtime coordinator.
//! - `SystemSignal`: Tick and lifecycle events injected into Core.
//! - `EffectExecutor`: Dispatches `Effect` to async handlers.
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: Core never performs I/O; shell handles all effects.
//! - I-Shell-Runtime-DeterministicClock: `SystemSignal::Tick` is the only time source.
//!
//! Refs: SPECS.md §Book IIIa
