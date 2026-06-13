//! UiRegistry — Book III-C §1
//!
//! Extensibility contract mapping widget type identifiers to anchor slots.
//! The kernel emits structured `Effect::ForwardToUi(UiWidget)` effects;
//! the projection layer serializes the enum to JSON at the IPC boundary.
//! The frontend resolves the widget type string via the registry to
//! determine where to mount the component. Third-party widgets that do
//! not match a known `UiWidget` variant use `UiWidget::Custom`.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: Registry stores only strings and enums; no Vue/Tauri types.
//! - I-UI-NoDirectDOM: The registry is declarative; DOM mounting is frontend-only.
//!
//! Refs: docs/SPECS.md §Book III-C Ch 1

use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Widget type constants (merged from widget.rs)
// ---------------------------------------------------------------------------

/// Warning banner displayed when a plugin has been quarantined.
///
/// Emitter: `QuarantineManager`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SYSTEM_DEGRADED: &str = "system_degraded";

/// Displayed when a `SystemSignal::NetworkUnavailable` is intercepted.
///
/// Emitter: `RecoveryPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_NETWORK_ERROR: &str = "network_error";

/// Generic state widget (e.g. "cancelled").
///
/// Emitter: `RecoveryPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_STATUS: &str = "status";

/// Generic widget for errors (`Effect::Error` transformed by the shell).
///
/// Emitter: Shell runtime
///
/// Refs: I-UI-NoUIType
pub const WIDGET_ERROR: &str = "error";

/// Displayed when a sub-routine exceeds its time limit.
///
/// Emitter: `SubRoutineTimeoutPolicy`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SUBROUTINE_TIMEOUT: &str = "subroutine_timeout";

/// Text chunk emitted during LLM streaming.
///
/// This is the primary content widget. It is never dropped by the
/// `UiComposer` and always flushed with absolute priority.
///
/// Emitter: Shell runtime (from `LlmStream` events)
///
/// Refs: I-UI-NoUIType
pub const WIDGET_TEXT_CHUNK: &str = "text_chunk";

/// Displayed when a sub-routine has been successfully restored.
///
/// Emitter: Shell runtime (on `Effect::SubRoutineRestored`)
///
/// Refs: I-UI-NoUIType
pub const WIDGET_SUBROUTINE_LOADED: &str = "subroutine_loaded";

/// Pending task status widget for long-running tool calls.
///
/// Emitter: `PendingTaskManager`
///
/// Refs: I-UI-NoUIType
pub const WIDGET_PENDING_TASK: &str = "pending_task";

// ---------------------------------------------------------------------------
// Anchor slots where UI widgets can be mounted in the frontend layout.
// ---------------------------------------------------------------------------
///
/// These slots correspond to fixed regions of the application chrome.
/// The frontend is responsible for actual layout and rendering.
///
/// Refs: docs/SPECS.md §Book III-C Ch 1.2
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AnchorSlot {
    /// Application header, horizontal alignment, fixed height.
    TopBar,
    /// Left navigation, adjustable fixed width, 100 % height.
    Sidebar,
    /// Bottom status line, telemetry, thin fixed height.
    StatusBar,
    /// Button bar under the prompt field.
    InputActions,
    /// Floating contextual suggestion layer.
    InputOverlay,
    /// Main message rendering area (streaming).
    #[default]
    ContentRenderer,
    /// Meta-information or contextual actions under a message.
    MessageFooter,
    /// Hyperparameter adjustment drawer, anchored to the right.
    SettingsPanel,
}

/// Registry mapping widget type identifiers to anchor slots.
///
/// `UiRegistry` is instantiated by the shell at startup and shared
/// with the frontend via IPC. Widgets emitted by governance plugins
/// are pre-registered; third-party plugins may register additional
/// mappings at runtime via the shell's plugin initialization path.
///
/// # Determinism
/// Uses `BTreeMap` for deterministic iteration order.
///
/// Refs: I-Eco-OrderedCollections
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiRegistry {
    mappings: BTreeMap<String, AnchorSlot>,
}

impl UiRegistry {
    /// Create an empty registry.
    ///
    /// Complexity: O(1). Allocates empty map.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn new() -> Self {
        Self {
            mappings: BTreeMap::new(),
        }
    }

    /// Create a registry pre-populated with standard governance widgets.
    ///
    /// This is the recommended initialization path for the shell.
    ///
    /// Complexity: O(k log k) where k = number of special widgets (5).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn with_special_widgets() -> Self {
        let mut reg = Self::new();
        reg.register(WIDGET_SYSTEM_DEGRADED, AnchorSlot::TopBar);
        reg.register(WIDGET_NETWORK_ERROR, AnchorSlot::TopBar);
        reg.register(WIDGET_STATUS, AnchorSlot::StatusBar);
        reg.register(WIDGET_ERROR, AnchorSlot::ContentRenderer);
        reg.register(WIDGET_SUBROUTINE_TIMEOUT, AnchorSlot::ContentRenderer);
        reg
    }

    /// Register a widget type identifier to an anchor slot.
    ///
    /// If `widget_type` was already registered, the old slot is overwritten.
    ///
    /// Complexity: O(log n) where n = number of registered types.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn register(&mut self, widget_type: impl Into<String>, slot: AnchorSlot) {
        self.mappings.insert(widget_type.into(), slot);
    }

    /// Resolve a widget type to its anchor slot, if known.
    ///
    /// Complexity: O(log n).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn resolve(&self, widget_type: &str) -> Option<AnchorSlot> {
        self.mappings.get(widget_type).copied()
    }

    /// Returns `true` if the given widget type is registered.
    ///
    /// Complexity: O(log n).
    /// Refs: docs/SPECS.md §Book III-A
    pub fn contains(&self, widget_type: &str) -> bool {
        self.mappings.contains_key(widget_type)
    }

    /// Returns `true` if the widget type is one of the special governance widgets.
    ///
    /// Special widgets are emitted by governance plugins and have predefined
    /// semantics in the frontend.
    ///
    /// Complexity: O(1). String comparison against constants.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn is_special_widget(widget_type: &str) -> bool {
        matches!(
            widget_type,
            WIDGET_SYSTEM_DEGRADED
                | WIDGET_NETWORK_ERROR
                | WIDGET_STATUS
                | WIDGET_ERROR
                | WIDGET_SUBROUTINE_TIMEOUT
        )
    }

    /// Iterate over all registered mappings in deterministic order.
    ///
    /// Complexity: O(1) for iterator creation.
    /// Refs: docs/SPECS.md §Book III-A
    pub fn iter(&self) -> impl Iterator<Item = (&String, &AnchorSlot)> {
        self.mappings.iter()
    }
}
