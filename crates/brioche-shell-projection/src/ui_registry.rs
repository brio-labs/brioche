//! UiRegistry — Book III-C §1
//!
//! Extensibility contract mapping widget type identifiers to anchor slots.
//! The kernel emits `Effect::ForwardToUi` with a `widget_type` string;
//! the frontend resolves this string via the registry to determine where
//! to mount the component.
//!
//! ## Invariants upheld
//! - I-UI-NoUIType: Registry stores only strings and enums; no Vue/Tauri types.
//! - I-UI-NoDirectDOM: The registry is declarative; DOM mounting is frontend-only.
//!
//! Refs: SPECS.md §Book III-C Ch 1

use std::collections::BTreeMap;

/// Anchor slots where UI widgets can be mounted in the frontend layout.
///
/// These slots correspond to fixed regions of the application chrome.
/// The frontend is responsible for actual layout and rendering.
///
/// Refs: SPECS.md §Book III-C Ch 1.2
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
    pub fn with_special_widgets() -> Self {
        let mut reg = Self::new();
        reg.register(crate::widget::WIDGET_SYSTEM_DEGRADED, AnchorSlot::TopBar);
        reg.register(crate::widget::WIDGET_NETWORK_ERROR, AnchorSlot::TopBar);
        reg.register(crate::widget::WIDGET_STATUS, AnchorSlot::StatusBar);
        reg.register(crate::widget::WIDGET_ERROR, AnchorSlot::ContentRenderer);
        reg.register(
            crate::widget::WIDGET_SUBROUTINE_TIMEOUT,
            AnchorSlot::ContentRenderer,
        );
        reg
    }

    /// Register a widget type identifier to an anchor slot.
    ///
    /// If `widget_type` was already registered, the old slot is overwritten.
    ///
    /// Complexity: O(log n) where n = number of registered types.
    pub fn register(&mut self, widget_type: impl Into<String>, slot: AnchorSlot) {
        self.mappings.insert(widget_type.into(), slot);
    }

    /// Resolve a widget type to its anchor slot, if known.
    ///
    /// Complexity: O(log n).
    pub fn resolve(&self, widget_type: &str) -> Option<AnchorSlot> {
        self.mappings.get(widget_type).copied()
    }

    /// Returns `true` if the given widget type is registered.
    ///
    /// Complexity: O(log n).
    pub fn contains(&self, widget_type: &str) -> bool {
        self.mappings.contains_key(widget_type)
    }

    /// Returns `true` if the widget type is one of the special governance widgets.
    ///
    /// Special widgets are emitted by governance plugins and have predefined
    /// semantics in the frontend.
    ///
    /// Complexity: O(1). String comparison against constants.
    pub fn is_special_widget(widget_type: &str) -> bool {
        matches!(
            widget_type,
            crate::widget::WIDGET_SYSTEM_DEGRADED
                | crate::widget::WIDGET_NETWORK_ERROR
                | crate::widget::WIDGET_STATUS
                | crate::widget::WIDGET_ERROR
                | crate::widget::WIDGET_SUBROUTINE_TIMEOUT
        )
    }

    /// Iterate over all registered mappings in deterministic order.
    ///
    /// Complexity: O(1) for iterator creation.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &AnchorSlot)> {
        self.mappings.iter()
    }
}
