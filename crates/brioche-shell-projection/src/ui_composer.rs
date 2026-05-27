//! UiComposer — Book III-C §3
//!
//! Per-frame effect scheduler. Consumes `ForwardToUi` effects produced
//! by the kernel and orders them by priority so that the frontend can
//! apply them within a fixed `requestAnimationFrame` budget.
//!
//! ## Invariants upheld
//! - I-UI-Composer-FrameSync: Effects are scheduled for the frame loop;
//!   no effect is applied outside it.
//! - I-UI-StreamBuffer: `TextChunk` effects are never dropped.
//!
//! Refs: SPECS.md §Book III-C Ch 3

use brioche_core::Effect;
use std::collections::VecDeque;

/// Priority tier for a `ForwardToUi` effect.
///
/// The frontend applies effects in this order, dropping or sliding
/// lower-priority effects when the per-frame budget is exceeded.
///
/// Refs: I-UI-Composer-FrameSync
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectPriority {
    /// `ForwardToUi` with `widget_type == "text_chunk"` — never dropped.
    TextChunk,
    /// Focus, scroll — slides to next frame if necessary.
    Navigation,
    /// Accordion expansion, highlight — slides to next frame.
    Semantic,
    /// Animations, transitions — dropped if 3 frames behind.
    Cosmetic,
}

/// An `Effect` paired with its computed priority.
///
/// Used internally by [`UiComposer`] to sort the pending frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledEffect {
    pub effect: Effect,
    pub priority: EffectPriority,
    /// Number of frames this effect has been waiting.
    pub age_frames: u8,
}

/// Frame budget composer for `ForwardToUi` effects.
///
/// `UiComposer` receives raw effects from the kernel, classifies them
/// by priority, and produces a ordered sequence for the frontend's
/// `requestAnimationFrame` loop. Text chunks are always emitted first;
/// cosmetic effects are dropped if they age beyond the threshold.
///
/// # Frame budget
/// The budget is expressed in milliseconds (default: 2 ms). Because
/// the Rust side cannot measure actual DOM rendering time, the budget
/// is advisory: the frontend enforces it, while the composer uses a
/// simple heuristic (effect count) to approximate load.
///
/// Refs: I-UI-Composer-FrameSync
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiComposer {
    /// Per-frame budget in milliseconds. Default: 2.
    frame_budget_ms: u8,
    /// Effects queued for the current / next frame.
    pending: VecDeque<ScheduledEffect>,
    /// Estimated cost heuristic per priority tier.
    /// TextChunk = 0 (free), Navigation = 1, Semantic = 2, Cosmetic = 3.
    cost_per_priority: [u8; 4],
}

impl UiComposer {
    /// Create a composer with the default 2 ms frame budget.
    ///
    /// Complexity: O(1).
    pub fn new() -> Self {
        Self::with_budget(2)
    }

    /// Create a composer with a custom frame budget.
    ///
    /// Complexity: O(1).
    pub fn with_budget(frame_budget_ms: u8) -> Self {
        Self {
            frame_budget_ms,
            pending: VecDeque::new(),
            cost_per_priority: [0, 1, 2, 3],
        }
    }

    /// Update the per-frame budget.
    ///
    /// This is called by the shell when `UiPerformanceState` changes.
    ///
    /// Complexity: O(1).
    pub fn set_frame_budget(&mut self, ms: u8) {
        self.frame_budget_ms = ms;
    }

    /// Current frame budget in milliseconds.
    ///
    /// Complexity: O(1).
    pub fn frame_budget(&self) -> u8 {
        self.frame_budget_ms
    }

    /// Enqueue a raw `Effect` for scheduling.
    ///
    /// Only `ForwardToUi` effects are accepted; all other variants are
    /// ignored and should be handled by the shell's main effect executor.
    ///
    /// Complexity: O(1) amortized.
    pub fn enqueue(&mut self, effect: Effect) {
        if let Effect::ForwardToUi { widget_type, .. } = &effect {
            let priority = classify_widget(widget_type);
            self.pending.push_back(ScheduledEffect {
                effect,
                priority,
                age_frames: 0,
            });
        }
    }

    /// Compose the next frame: return effects that fit within budget,
    /// retaining the rest (aged by one frame).
    ///
    /// # Algorithm
    /// 1. Sort pending effects by priority (TextChunk > Navigation > Semantic > Cosmetic).
    /// 2. Accumulate costs until the budget is exceeded.
    /// 3. Drop cosmetic effects that have aged beyond 3 frames.
    /// 4. Return the chosen effects in priority order.
    /// 5. Retain unchosen effects with `age_frames += 1`.
    ///
    /// Complexity: O(n log n) where n = pending effects (sorting).
    /// No heap allocation beyond the output `Vec`.
    pub fn compose_frame(&mut self) -> Vec<Effect> {
        let budget = self.frame_budget_ms as u16;
        let mut cost: u16 = 0;
        let mut chosen = Vec::new();
        let mut retained = VecDeque::new();

        // Drain pending so we can partition.
        while let Some(mut scheduled) = self.pending.pop_front() {
            let tier_cost = self.cost_for_priority(scheduled.priority) as u16;

            // Cosmetic effects aged > 3 frames are dropped.
            if scheduled.priority == EffectPriority::Cosmetic && scheduled.age_frames > 3 {
                continue;
            }

            // TextChunk is always included (cost 0).
            // Others are included while budget permits.
            if scheduled.priority == EffectPriority::TextChunk || cost + tier_cost <= budget {
                cost += tier_cost;
                chosen.push(scheduled.effect);
            } else {
                scheduled.age_frames = scheduled.age_frames.saturating_add(1);
                retained.push_back(scheduled);
            }
        }

        self.pending = retained;

        // We drain the VecDeque in insertion order, then re-sort the chosen
        // effects by priority to guarantee the
        // TextChunk > Navigation > Semantic > Cosmetic ordering invariant
        // (I-UI-Composer-FrameSync).
        // Re-classification via `classify_widget` is O(k log k) where
        // k = chosen effects, acceptable because k <= n and n is typically
        // small (< 50).
        chosen.sort_by_key(|eff| {
            if let Effect::ForwardToUi { widget_type, .. } = eff {
                classify_widget(widget_type) as u8
            } else {
                // unreachable because we only enqueue ForwardToUi
                255
            }
        });

        chosen
    }

    /// Number of effects still pending for future frames.
    ///
    /// Complexity: O(1).
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Remove all pending effects.
    ///
    /// Called by the shell on session reset.
    ///
    /// Complexity: O(1) (drops the VecDeque).
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// Lookup the heuristic cost for a priority tier.
    fn cost_for_priority(&self, priority: EffectPriority) -> u8 {
        match priority {
            EffectPriority::TextChunk => self.cost_per_priority[0],
            EffectPriority::Navigation => self.cost_per_priority[1],
            EffectPriority::Semantic => self.cost_per_priority[2],
            EffectPriority::Cosmetic => self.cost_per_priority[3],
        }
    }
}

/// Classify a widget type string into an [`EffectPriority`] tier.
///
/// This is the authoritative mapping used by both `UiComposer` and
/// the frontend to agree on priority semantics.
///
/// Complexity: O(1). Match on string constants.
fn classify_widget(widget_type: &str) -> EffectPriority {
    match widget_type {
        crate::widget::WIDGET_TEXT_CHUNK => EffectPriority::TextChunk,
        // Navigation-like widgets
        "focus" | "scroll" => EffectPriority::Navigation,
        // Semantic widgets
        "accordion_expand" | "highlight" | "subroutine_loaded" => EffectPriority::Semantic,
        // Known cosmetic widgets
        "animation" | "transition" | "spinner" => EffectPriority::Cosmetic,
        // Default: special governance widgets are treated as semantic
        // because they convey important state changes.
        _ if UiRegistry::is_special_widget(widget_type) => EffectPriority::Semantic,
        // Everything else defaults to cosmetic (lowest priority).
        _ => EffectPriority::Cosmetic,
    }
}

use crate::ui_registry::UiRegistry;
