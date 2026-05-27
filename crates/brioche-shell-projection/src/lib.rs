#![deny(clippy::unwrap_used, clippy::expect_used)]

//! # Brioche Shell Projection — Book III-C
//!
//! UI projection layer. Transforms kernel `Effect::ForwardToUi` instructions
//! into structured view-model state for the Tauri / web frontend.
//!
//! ## Public interface
//! - [`UiRegistry`]: Dynamic widget-to-slot registry with anchor slots.
//! - [`ContentRenderer`]: Streaming text accumulation by trace ID.//! - [`UiComposer`]: Per-frame effect scheduler with priority tiers.//! - [`UiPerformancePolicy`]: Shell-side policy that configures the composer
//!   budget via [`UiPerformanceState`] in `ExtensionStorage`.
//! - [`widget`]: Special governance widget type constants.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: No UI crate types in kernel-facing data structures.
//! - I-UI-NoDirectDOM: DOM manipulation is strictly frontend; Rust side
//!   only produces declarative instructions.//! - I-UI-StreamBuffer: Text fragments accumulate by trace ID without
//!   granular reactivity overhead.//! - I-UI-Composer-FrameSync: `UiComposer` schedules effects for the
//!   `requestAnimationFrame` loop; no effect is applied outside it.
//!
//! Refs: SPECS.md §Book III-C

pub mod content_renderer;
pub mod stream_buffer;
pub mod ui_composer;
pub mod ui_performance_policy;
pub mod ui_registry;
pub mod widget;

pub use content_renderer::ContentRenderer;
pub use stream_buffer::StreamBuffer;
pub use ui_composer::{EffectPriority, ScheduledEffect, UiComposer};
pub use ui_performance_policy::{UiPerformancePolicy, UiPerformanceState};
pub use ui_registry::{AnchorSlot, UiRegistry};
