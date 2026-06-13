//! UiPerformancePolicy — Book III-C §6
//!
//! Shell-side policy that configures the [`UiComposer`] frame budget.
//!
//! `UiPerformancePolicy` is **not** a kernel plugin; it runs in the
//! shell's effect consumption path, intercepting `ForwardToUi` effects
//! before they reach the `UiComposer`.
//!
//! ## Invariants upheld
//! - I-UI-Composer-FrameSync: Budget is configurable without kernel changes.
//!
//! Refs: SPECS.md §Book III-C Ch 6

use brioche_core::Effect;

use crate::UiComposer;

/// Shell-side interceptor that configures `UiComposer` frame budget.
///
/// `UiPerformancePolicy` wraps a [`UiComposer`] and synchronizes its
/// frame budget from `ExtensionStorage` on demand. It also filters
/// `ForwardToUi` effects through the composer, returning the scheduled
/// frame effects.
///
/// # Usage
/// ```ignore
/// let mut policy = UiPerformancePolicy::new();
/// policy.sync_from_storage(&mut session.extensions);
/// let frame_effects = policy.process_effects(kernel_effects);
/// ```
///
/// Refs: I-UI-Composer-FrameSync
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiPerformancePolicy {
    composer: UiComposer,
}

impl UiPerformancePolicy {
    /// Create a policy with the default 2 ms composer budget.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            composer: UiComposer::new(),
        }
    }

    /// Create a policy with a custom initial budget.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn with_budget(frame_budget_ms: u8) -> Self {
        Self {
            composer: UiComposer::with_budget(frame_budget_ms),
        }
    }

    /// Enqueue a batch of effects and compose the next frame.
    ///
    /// Only `ForwardToUi` effects are consumed by the composer;
    /// all other effect variants pass through unchanged and are
    /// prepended to the returned frame.
    ///
    /// The returned `Vec<Effect>` is ordered: non-UI effects first,
    /// then UI effects sorted by composer priority.
    ///
    /// Complexity: O(m + n log n) where m = total effects,
    /// n = number of `ForwardToUi` effects.
    /// Refs: SPECS.md §Book III-A
    pub fn process_effects(&mut self, effects: Vec<Effect>) -> Vec<Effect> {
        let mut non_ui = Vec::new();

        for effect in effects {
            if matches!(effect, Effect::ForwardToUi(_)) {
                self.composer.enqueue(effect);
            } else {
                non_ui.push(effect);
            }
        }

        let mut frame = non_ui;
        frame.extend(self.composer.compose_frame());
        frame
    }

    /// Read-only access to the internal composer.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn composer(&self) -> &UiComposer {
        &self.composer
    }

    /// Mutable access to the internal composer.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn composer_mut(&mut self) -> &mut UiComposer {
        &mut self.composer
    }

    /// Directly set the frame budget without reading storage.
    ///
    /// Useful for tests and for shell-level overrides.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn set_frame_budget(&mut self, ms: u8) {
        self.composer.set_frame_budget(ms);
    }

    /// Returns `true` if the policy's composer has pending effects.
    ///
    /// Complexity: O(1).
    /// Refs: SPECS.md §Book III-A
    pub fn has_pending(&self) -> bool {
        self.composer.pending_count() > 0
    }
}
