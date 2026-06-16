//! Footer metric extension point.
//!
//! Extensions can contribute small read-only indicators that appear in the
//! application footer. Each metric has an identifier, label, optional value and
//! optional tooltip.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use super::ExtensionMetadata;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A footer metric value returned to the frontend.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FooterMetric {
    /// Unique metric identifier (e.g. `version`, `session-duration`).
    pub id: String,
    /// Short label shown before the value.
    pub label: String,
    /// Current value as a string; may be empty for static labels.
    pub value: String,
    /// Optional longer explanation shown on hover.
    pub tooltip: Option<String>,
    /// Ordering hint; lower values appear first.
    pub priority: i16,
}

/// Context used to compute footer metrics.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct FooterContext {
    /// Current Brioche Desktop version (from Cargo).
    pub version: String,
    /// Session start instant as seconds since UNIX epoch.
    pub session_started_at: u64,
    /// Identifier of the currently active model.
    pub current_model: String,
    /// Estimated remaining tokens in the current context window.
    pub context_remaining: i64,
}

/// Extension trait for footer metrics.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub trait FooterMetricProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Computes the metric value for the given context.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn compute(&self, ctx: &FooterContext) -> FooterMetric;
}

/// Built-in version metric.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct VersionMetric;

impl FooterMetricProvider for VersionMetric {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "footer-version".into(),
            name: "Brioche version".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn compute(&self, ctx: &FooterContext) -> FooterMetric {
        FooterMetric {
            id: "version".into(),
            label: "Brioche".into(),
            value: ctx.version.clone(),
            tooltip: Some("Brioche Desktop version".into()),
            priority: -100,
        }
    }
}

/// Built-in session duration metric.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct SessionDurationMetric;

impl FooterMetricProvider for SessionDurationMetric {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "footer-session-duration".into(),
            name: "Session duration".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn compute(&self, ctx: &FooterContext) -> FooterMetric {
        let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs(),
            Err(_) => ctx.session_started_at,
        };
        let elapsed = now.saturating_sub(ctx.session_started_at);
        let hours = elapsed / 3600;
        let minutes = (elapsed % 3600) / 60;
        let seconds = elapsed % 60;
        let value = if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            format!("{:02}:{:02}", minutes, seconds)
        };

        FooterMetric {
            id: "session-duration".into(),
            label: "Session".into(),
            value,
            tooltip: Some("Time since the current session started".into()),
            priority: -90,
        }
    }
}

/// Built-in current model metric.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct CurrentModelMetric;

impl FooterMetricProvider for CurrentModelMetric {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "footer-current-model".into(),
            name: "Current model".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn compute(&self, ctx: &FooterContext) -> FooterMetric {
        FooterMetric {
            id: "current-model".into(),
            label: "Model".into(),
            value: ctx.current_model.clone(),
            tooltip: Some("Active LLM model".into()),
            priority: -80,
        }
    }
}

/// Built-in remaining context metric.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ContextRemainingMetric;

impl FooterMetricProvider for ContextRemainingMetric {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "footer-context-remaining".into(),
            name: "Remaining context".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn compute(&self, ctx: &FooterContext) -> FooterMetric {
        let value = if ctx.context_remaining >= 0 {
            format!("{} tokens", ctx.context_remaining)
        } else {
            format!("over by {} tokens", ctx.context_remaining.abs())
        };
        FooterMetric {
            id: "context-remaining".into(),
            label: "Context".into(),
            value,
            tooltip: Some("Estimated remaining context for the active model".into()),
            priority: -70,
        }
    }
}

/// Helper: creates the version metric boxed as a trait object.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn version_metric() -> Arc<dyn FooterMetricProvider> {
    Arc::new(VersionMetric)
}

/// Helper: creates the session duration metric boxed as a trait object.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn session_duration_metric() -> Arc<dyn FooterMetricProvider> {
    Arc::new(SessionDurationMetric)
}

/// Helper: creates the current model metric boxed as a trait object.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn current_model_metric() -> Arc<dyn FooterMetricProvider> {
    Arc::new(CurrentModelMetric)
}

/// Helper: creates the remaining context metric boxed as a trait object.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn context_remaining_metric() -> Arc<dyn FooterMetricProvider> {
    Arc::new(ContextRemainingMetric)
}
