//! Desktop extension framework (Book IV — Ecosystem layer).
//!
//! Brioche Desktop 0.1 exposes typed extension points so that features such as
//! context engines, memory systems, tools, skills, settings sections and footer
//! metrics can be added without modifying the Tauri shell or the kernel.
//!
//! Each extension implements [`DesktopExtension`] and registers itself with the
//! [`ExtensionRegistry`] held in the desktop application state. The registry is
//! read at startup and wired into the shell builder, settings panel and footer.
//!
//! ## Extension points
//!
//! | Point | Trait | Default implementation |
//! |-------|-------|------------------------|
//! | Context engine | [`ContextEngine`] | [`CompressorContextEngine`] |
//! | Memory provider | [`MemoryProvider`] | [`LocalMemoryProvider`] |
//! | Tool provider | [`ToolProvider`] | [`ToolRegistry`] |
//! | Skill provider | [`SkillProvider`] | [`SkillRegistry`] |
//! | Settings section | [`SettingsSectionProvider`] | module-defined |
//! | Footer metric | [`FooterMetricProvider`] | module-defined |
//!
//! Refs: I-Shell-Runtime-OnlyIO, SPECS (SPECS.md §1.1)

pub mod amp_memory_client;
pub mod context;
pub mod footer;
pub mod memory_provider;
pub mod settings_sections;
pub mod skill_provider;
pub mod tool_provider;

use std::sync::Arc;

pub use amp_memory_client::{AmpMemoryEndpoint, AmpMemoryProvider};
pub use context::{CompressorContextEngine, ContextEngine, ContextEngineInput};
pub use footer::{FooterMetric, FooterMetricProvider};
pub use memory_provider::{LocalMemoryProvider, MemoryProvider, MemoryQuery, MemorySessionContext};
use serde::{Deserialize, Serialize};
pub use settings_sections::{SettingsSection, SettingsSectionProvider};
pub use skill_provider::{SkillProvider, SkillRegistry};
pub use tool_provider::{ToolProvider, ToolRegistry, UserDefinedTool, UserToolDefinition};

use crate::settings::Settings;

/// A panel slot where a frontend extension can render by default.
///
/// Users may move extensions between slots at runtime; this value is only the
/// installation default.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// O(1) Copy enum.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PanelSlot {
    /// Left vertical panel.
    #[default]
    Left,
    /// Right vertical panel.
    Right,
    /// Main central area.
    Center,
    /// Bottom horizontal panel.
    Bottom,
}

/// Metadata describing a desktop extension.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Struct containing heap-allocated strings. O(1) creation.
///
/// # Panic / Safety
/// Never panics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    /// Machine-readable identifier (e.g. `chat`, `memory-local`).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Default panel slot for the frontend widget, if any.
    pub default_panel: Option<PanelSlot>,
    /// Whether the extension is enabled by the user.
    pub enabled: bool,
}

/// Common trait implemented by every desktop extension backend.
///
/// Extensions are thread-safe so they can be shared between Tauri commands and
/// the async shell runtime.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Implementation dependent.
///
/// # Panic / Safety
/// Implementation dependent.
pub trait DesktopExtension: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;
}

/// Registry of all loaded desktop extensions.
///
/// The registry owns the extension trait objects behind [`Arc`]s so they can be
/// cheaply cloned into shell factories and Tauri managed state.
///
/// Refs: I-Shell-Runtime-OnlyIO
///
/// # Complexity
/// Holds vectors of Arc'd trait objects. O(1) clone.
///
/// # Panic / Safety
/// Never panics.
#[derive(Default, Clone)]
pub struct ExtensionRegistry {
    context_engines: Vec<Arc<dyn ContextEngine>>,
    memory_providers: Vec<Arc<dyn MemoryProvider>>,
    tool_providers: Vec<Arc<dyn ToolProvider>>,
    skill_providers: Vec<Arc<dyn SkillProvider>>,
    settings_sections: Vec<Arc<dyn SettingsSectionProvider>>,
    footer_metrics: Vec<Arc<dyn FooterMetricProvider>>,
    metadata: Vec<ExtensionMetadata>,
}

impl ExtensionRegistry {
    /// Creates an empty registry.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(1) memory allocation.
    ///
    /// # Panic / Safety
    /// Never panics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds the default extension registry from the provided settings.
    ///
    /// This is used when settings change at runtime so that AMP endpoints,
    /// tools, and sections are updated without restarting the application.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn default_set_from_settings(settings: &Settings) -> Self {
        let mut registry = Self::new();
        registry.register_context_engine(Arc::new(CompressorContextEngine::default()));
        registry.register_memory_provider(Arc::new(LocalMemoryProvider::default()));
        for endpoint in settings.memory_endpoints() {
            if endpoint.url.trim().is_empty() {
                continue;
            }
            let amp_endpoint = AmpMemoryEndpoint {
                id: endpoint.id,
                name: endpoint.name,
                url: endpoint.url,
                api_key: endpoint.api_key,
                scope: endpoint.scope,
            };
            registry.register_memory_provider(Arc::new(AmpMemoryProvider::new(amp_endpoint)));
        }
        registry.register_tool_provider(Arc::new(match ToolRegistry::load() {
            Ok(registry) => registry,
            Err(err) => {
                tracing::warn!("Failed to load tool registry, using defaults: {err}");
                ToolRegistry::default()
            }
        }));
        registry.register_skill_provider(Arc::new(SkillRegistry::default()));
        registry.register_settings_section(settings_sections::chat_section());
        registry.register_settings_section(settings_sections::model_identity_section());
        registry.register_settings_section(settings_sections::context_engine_section());
        registry.register_settings_section(settings_sections::memory_section());
        registry.register_settings_section(settings_sections::tool_section());
        registry.register_footer_metric(footer::version_metric());
        registry.register_footer_metric(footer::session_duration_metric());
        registry.register_footer_metric(footer::current_model_metric());
        registry.register_footer_metric(footer::context_remaining_metric());
        registry.register_footer_metric(footer::context_engine_note_metric());
        registry
    }

    /// Loads the default Brioche 0.1 extension set from disk settings.
    ///
    /// This wires the compressor context engine, the local memory provider, the
    /// built-in tool registry, the skill scanner and the core settings sections.
    /// Additional extensions can be registered afterwards with the `register_*`
    /// methods.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    ///
    /// # Complexity
    /// O(N) where N is the configuration size, since it loads settings from disk.
    ///
    /// # Panic / Safety
    /// Never panics. Returns standard fallback defaults if settings file is corrupt.
    pub fn default_set() -> Self {
        Self::default_set_from_settings(&Settings::load())
    }

    /// Registers a context engine.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_context_engine(&mut self, engine: Arc<dyn ContextEngine>) {
        self.metadata.push(engine.metadata());
        self.context_engines.push(engine);
    }

    /// Registers a memory provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_memory_provider(&mut self, provider: Arc<dyn MemoryProvider>) {
        self.metadata.push(provider.metadata());
        self.memory_providers.push(provider);
    }

    /// Registers a tool provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_tool_provider(&mut self, provider: Arc<dyn ToolProvider>) {
        self.metadata.push(provider.metadata());
        self.tool_providers.push(provider);
    }

    /// Registers a skill provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_skill_provider(&mut self, provider: Arc<dyn SkillProvider>) {
        self.metadata.push(provider.metadata());
        self.skill_providers.push(provider);
    }

    /// Registers a settings section provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_settings_section(&mut self, section: Arc<dyn SettingsSectionProvider>) {
        self.metadata.push(section.metadata());
        self.settings_sections.push(section);
    }

    /// Registers a footer metric provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn register_footer_metric(&mut self, metric: Arc<dyn FooterMetricProvider>) {
        self.metadata.push(metric.metadata());
        self.footer_metrics.push(metric);
    }

    /// Returns metadata for all registered extensions.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn metadata(&self) -> &[ExtensionMetadata] {
        &self.metadata
    }

    /// Returns the active context engines.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn context_engines(&self) -> &[Arc<dyn ContextEngine>] {
        &self.context_engines
    }

    /// Returns the active memory providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn memory_providers(&self) -> &[Arc<dyn MemoryProvider>] {
        &self.memory_providers
    }

    /// Returns the active tool providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn tool_providers(&self) -> &[Arc<dyn ToolProvider>] {
        &self.tool_providers
    }

    /// Returns the active skill providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn skill_providers(&self) -> &[Arc<dyn SkillProvider>] {
        &self.skill_providers
    }

    /// Returns the active settings section providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn settings_sections(&self) -> &[Arc<dyn SettingsSectionProvider>] {
        &self.settings_sections
    }

    /// Returns the active footer metric providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn footer_metrics(&self) -> &[Arc<dyn FooterMetricProvider>] {
        &self.footer_metrics
    }

    /// Returns mutable access to tool providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn tool_providers_mut(&mut self) -> &mut [Arc<dyn ToolProvider>] {
        &mut self.tool_providers
    }

    /// Returns mutable access to skill providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn skill_providers_mut(&mut self) -> &mut [Arc<dyn SkillProvider>] {
        &mut self.skill_providers
    }

    /// Returns mutable access to memory providers.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    pub fn memory_providers_mut(&mut self) -> &mut [Arc<dyn MemoryProvider>] {
        &mut self.memory_providers
    }
}
