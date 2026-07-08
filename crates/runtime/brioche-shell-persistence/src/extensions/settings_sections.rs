//! Per-module settings sections.
//!
//! The settings panel is built dynamically from registered [`SettingsSection`]
//! providers. Each section has a module id, title, order, searchable keywords
//! and a JSON schema describing its fields. The frontend renders generic editors
//! from this schema.
//!
//! Refs: I-Shell-Runtime-OnlyIO

use serde::{Deserialize, Serialize};

use super::ExtensionMetadata;

/// A settings field type.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// Single-line text input.
    String,
    /// Multi-line text area.
    Text,
    /// Password input.
    Password,
    /// Number input.
    Number,
    /// Boolean toggle.
    Boolean,
    /// Single-select from options.
    Select,
    /// Multi-select from options.
    MultiSelect,
    /// List of sub-objects.
    List,
    /// Object with nested fields.
    Object,
    /// File/directory path string.
    Path,
    /// Markdown text with edit warning and reset button.
    ProtectedMarkdown,
}

/// A single settings field descriptor.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsField {
    /// Dotted path in the settings object.
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// Field type.
    pub field_type: FieldType,
    /// Brief help text.
    pub description: Option<String>,
    /// Placeholder for text inputs.
    pub placeholder: Option<String>,
    /// Options for select/multi-select fields.
    #[serde(default)]
    pub options: Vec<SettingsOption>,
    /// Default value as JSON.
    pub default_value: Option<serde_json::Value>,
    /// Whether the field requires confirmation before editing.
    #[serde(default)]
    pub protected: bool,
    /// Searchable keywords in addition to label/description.
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// An option for select/multi-select fields.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsOption {
    /// Option value.
    pub value: String,
    /// Display label.
    pub label: String,
}

/// A settings section contributed by a module.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsSection {
    /// Section identifier (e.g. `chat-model`).
    pub id: String,
    /// Owning module identifier (e.g. `chat`).
    pub module_id: String,
    /// Display title.
    pub title: String,
    /// Display order; lower values appear first.
    pub order: i16,
    /// Searchable keywords.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Fields in this section.
    pub fields: Vec<SettingsField>,
}

/// Extension trait for settings sections.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub trait SettingsSectionProvider: Send + Sync {
    /// Returns the extension metadata.
    fn metadata(&self) -> ExtensionMetadata;

    /// Returns the sections contributed by this provider.
    ///
    /// Refs: I-Shell-Runtime-OnlyIO
    fn sections(&self) -> Vec<SettingsSection>;
}

/// Built-in chat model settings section.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ChatModelSection;

impl SettingsSectionProvider for ChatModelSection {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "settings-chat".into(),
            name: "Chat settings".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn sections(&self) -> Vec<SettingsSection> {
        vec![SettingsSection {
            id: "chat-model".into(),
            module_id: "chat".into(),
            title: "Model".into(),
            order: 10,
            keywords: vec![
                "provider".into(),
                "model".into(),
                "api key".into(),
                "base url".into(),
                "reasoning".into(),
                "thinking".into(),
                "context window".into(),
                "fallback".into(),
                "max tokens".into(),
            ],
            fields: vec![
                SettingsField {
                    key: "chat.provider".into(),
                    label: "Provider".into(),
                    field_type: FieldType::Select,
                    description: Some("LLM provider backend".into()),
                    placeholder: None,
                    options: vec![
                        SettingsOption { value: "openai".into(), label: "OpenAI (or OpenAI-compatible)".into() },
                        SettingsOption { value: "openrouter".into(), label: "OpenRouter".into() },
                        SettingsOption { value: "anthropic".into(), label: "Anthropic (or Anthropic-compatible)".into() },
                    ],
                    default_value: Some(serde_json::Value::String("openrouter".into())),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "chat.base_url".into(),
                    label: "Base URL".into(),
                    field_type: FieldType::String,
                    description: Some("API endpoint (required for custom providers)".into()),
                    placeholder: Some("https://openrouter.ai/api/v1".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::String(
                        "https://openrouter.ai/api/v1".into(),
                    )),
                    protected: false,
                    keywords: vec!["endpoint".into(), "url".into()],
                },
                SettingsField {
                    key: "chat.model".into(),
                    label: "Model".into(),
                    field_type: FieldType::String,
                    description: Some("Primary model identifier".into()),
                    placeholder: Some("qwen/qwen3.7-plus".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::String("qwen/qwen3.7-plus".into())),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "chat.api_key".into(),
                    label: "API key".into(),
                    field_type: FieldType::Password,
                    description: Some("API key for the selected provider".into()),
                    placeholder: Some("sk-...".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::String(String::new())),
                    protected: false,
                    keywords: vec!["key".into(), "token".into()],
                },
                SettingsField {
                    key: "chat.max_tokens".into(),
                    label: "Max tokens".into(),
                    field_type: FieldType::Number,
                    description: Some("Maximum tokens per response".into()),
                    placeholder: Some("4096".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Number(4096.into())),
                    protected: false,
                    keywords: vec!["tokens".into(), "length".into()],
                },
                SettingsField {
                    key: "chat.context_window".into(),
                    label: "Context window".into(),
                    field_type: FieldType::Number,
                    description: Some("Model context window in tokens".into()),
                    placeholder: Some("128000".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Number(128_000.into())),
                    protected: false,
                    keywords: vec!["context".into(), "window".into()],
                },
                SettingsField {
                    key: "chat.reasoning_enabled".into(),
                    label: "Reasoning mode".into(),
                    field_type: FieldType::Boolean,
                    description: Some("Enable thinking/reasoning for reasoning models".into()),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::Bool(false)),
                    protected: false,
                    keywords: vec!["thinking".into(), "reason".into()],
                },
                SettingsField {
                    key: "chat.reasoning_effort".into(),
                    label: "Reasoning effort".into(),
                    field_type: FieldType::Select,
                    description: Some("Depth of reasoning when enabled".into()),
                    placeholder: None,
                    options: vec![
                        SettingsOption { value: "low".into(), label: "Low".into() },
                        SettingsOption { value: "medium".into(), label: "Medium".into() },
                        SettingsOption { value: "high".into(), label: "High".into() },
                    ],
                    default_value: Some(serde_json::Value::String("medium".into())),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "chat.fallback_models".into(),
                    label: "Fallback models".into(),
                    field_type: FieldType::List,
                    description: Some(
                        "Models to try if the primary model fails. Each entry can override context window and reasoning effort."
                            .into(),
                    ),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::Array(vec![])),
                    protected: false,
                    keywords: vec!["fallback".into(), "backup".into()],
                },
            ],
        }]
    }
}

/// Built-in model identity settings section.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ModelIdentitySection;

impl SettingsSectionProvider for ModelIdentitySection {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "settings-identity".into(),
            name: "Model identity".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn sections(&self) -> Vec<SettingsSection> {
        vec![SettingsSection {
            id: "chat-identity".into(),
            module_id: "chat".into(),
            title: "Model Identity".into(),
            order: 20,
            keywords: vec![
                "personality".into(),
                "identity".into(),
                "system prompt".into(),
                "helpful".into(),
                "teacher".into(),
                "creative".into(),
                "concise".into(),
            ],
            fields: vec![
                SettingsField {
                    key: "chat.personality".into(),
                    label: "Personality".into(),
                    field_type: FieldType::Select,
                    description: Some("Default conversational style".into()),
                    placeholder: None,
                    options: vec![
                        SettingsOption { value: "helpful".into(), label: "Helpful".into() },
                        SettingsOption { value: "teacher".into(), label: "Teacher".into() },
                        SettingsOption { value: "creative".into(), label: "Creative".into() },
                        SettingsOption { value: "concise".into(), label: "Concise".into() },
                    ],
                    default_value: Some(serde_json::Value::String("helpful".into())),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "chat.custom_identity".into(),
                    label: "Custom identity".into(),
                    field_type: FieldType::Text,
                    description: Some("Additional identity instructions".into()),
                    placeholder: Some("You are a senior Rust engineer...".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::String(String::new())),
                    protected: false,
                    keywords: vec!["persona".into(), "role".into()],
                },
                SettingsField {
                    key: "chat.system_prompt".into(),
                    label: "System prompt".into(),
                    field_type: FieldType::ProtectedMarkdown,
                    description: Some(
                        "The system prompt sent at the start of every session. Editing this can change model behavior significantly."
                            .into(),
                    ),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::String(
                        "You are a helpful AI coding assistant with access to filesystem tools."
                            .into(),
                    )),
                    protected: true,
                    keywords: vec!["prompt".into(), "instructions".into()],
                },
            ],
        }]
    }
}

/// Built-in context engine settings section.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ContextEngineSection;

impl SettingsSectionProvider for ContextEngineSection {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "settings-context".into(),
            name: "Context engine".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn sections(&self) -> Vec<SettingsSection> {
        vec![SettingsSection {
            id: "context-compressor".into(),
            module_id: "context".into(),
            title: "Context Compressor".into(),
            order: 30,
            keywords: vec![
                "context".into(),
                "compress".into(),
                "sliding window".into(),
                "token limit".into(),
            ],
            fields: vec![
                SettingsField {
                    key: "context.enabled".into(),
                    label: "Enable compressor".into(),
                    field_type: FieldType::Boolean,
                    description: Some("Compress context when it grows too large".into()),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::Bool(true)),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "context.trigger_percentage".into(),
                    label: "Trigger percentage".into(),
                    field_type: FieldType::Number,
                    description: Some(
                        "Activate compression when this percentage of the context window is used"
                            .into(),
                    ),
                    placeholder: Some("75".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Number(75.into())),
                    protected: false,
                    keywords: vec!["threshold".into()],
                },
                SettingsField {
                    key: "context.target_percentage".into(),
                    label: "Target percentage".into(),
                    field_type: FieldType::Number,
                    description: Some("Target context size after compression".into()),
                    placeholder: Some("50".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Number(50.into())),
                    protected: false,
                    keywords: vec![],
                },
                SettingsField {
                    key: "context.preserve_recent".into(),
                    label: "Preserve recent".into(),
                    field_type: FieldType::Number,
                    description: Some("Number of recent messages to always keep".into()),
                    placeholder: Some("6".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Number(6.into())),
                    protected: false,
                    keywords: vec![],
                },
            ],
        }]
    }
}

/// Built-in memory settings section.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct MemorySettingsSection;

impl SettingsSectionProvider for MemorySettingsSection {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "settings-memory".into(),
            name: "Memory".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn sections(&self) -> Vec<SettingsSection> {
        vec![SettingsSection {
            id: "memory-providers".into(),
            module_id: "memory".into(),
            title: "Memory Providers".into(),
            order: 40,
            keywords: vec!["memory".into(), "provider".into()],
            fields: vec![
                SettingsField {
                    key: "memory.active_providers".into(),
                    label: "Active providers".into(),
                    field_type: FieldType::MultiSelect,
                    description: Some("Memory systems consulted during conversations. Brioche 0.1 supports Local memory and configured AMP endpoints only.".into()),
                    placeholder: None,
                    options: vec![
                        SettingsOption {
                            value: "memory-local".into(),
                            label: "Local memory".into(),
                        },
                    ],
                    default_value: Some(serde_json::Value::Array(vec![serde_json::Value::String(
                        "memory-local".into(),
                    )])),
                    protected: false,
                    keywords: vec!["active".into(), "provider".into()],
                },
                SettingsField {
                    key: "memory.endpoints".into(),
                    label: "AMP endpoints".into(),
                    field_type: FieldType::List,
                    description: Some(
                        "Generic AMP Core-compatible memory endpoints. Any backend that implements /v1/encode, /v1/recall and /v1/forget can be added here without code changes.".into(),
                    ),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::Array(vec![serde_json::Value::Object(
                        [
                            ("id".into(), serde_json::Value::String("memory-amp-1".into())),
                            ("name".into(), serde_json::Value::String("Remote memory".into())),
                            (
                                "url".into(),
                                serde_json::Value::String("http://localhost:9471".into()),
                            ),
                            ("api_key".into(), serde_json::Value::Null),
                            ("scope".into(), serde_json::Value::Null),
                        ]
                        .into_iter()
                        .collect(),
                    )])),
                    protected: false,
                    keywords: vec!["amp".into(), "endpoint".into(), "url".into(), "api key".into()],
                },
            ],
        }]
    }
}

/// Built-in tool settings section.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, Default)]
pub struct ToolSettingsSection;

impl SettingsSectionProvider for ToolSettingsSection {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata {
            id: "settings-tools".into(),
            name: "Tools".into(),
            version: "0.1.0".into(),
            default_panel: None,
            enabled: true,
        }
    }

    fn sections(&self) -> Vec<SettingsSection> {
        vec![SettingsSection {
            id: "tools-security".into(),
            module_id: "tools".into(),
            title: "Tools".into(),
            order: 50,
            keywords: vec![
                "tools".into(),
                "user tools".into(),
                "shell".into(),
                "command".into(),
                "security".into(),
            ],
            fields: vec![
                SettingsField {
                    key: "tools.user_tools_enabled".into(),
                    label: "Enable user-defined tools".into(),
                    field_type: FieldType::Boolean,
                    description: Some(
                        "Allow custom tools that execute shell commands or HTTP requests. Disabled by default for security.".into(),
                    ),
                    placeholder: None,
                    options: vec![],
                    default_value: Some(serde_json::Value::Bool(false)),
                    protected: true,
                    keywords: vec!["user tools".into(), "security".into(), "enable".into()],
                },
                SettingsField {
                    key: "tools.allowed_commands".into(),
                    label: "Allowed commands".into(),
                    field_type: FieldType::List,
                    description: Some(
                        "Additional command names allowed for the built-in `execute_command` tool. Each entry should be a single command name (for example, `rg` or `pnpm`). These extend the default allow-list and do not affect user-defined tools.".into(),
                    ),
                    placeholder: Some("[\"rg\", \"pnpm\"]".into()),
                    options: vec![],
                    default_value: Some(serde_json::Value::Array(Vec::new())),
                    protected: false,
                    keywords: vec!["execute_command".into(), "allowlist".into(), "sandbox".into(), "shell".into()],
                },
            ],
        }]
    }
}

/// Helper: tool settings section provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn tool_section() -> std::sync::Arc<dyn SettingsSectionProvider> {
    std::sync::Arc::new(ToolSettingsSection)
}

/// Helper: chat model section provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn chat_section() -> std::sync::Arc<dyn SettingsSectionProvider> {
    std::sync::Arc::new(ChatModelSection)
}

/// Helper: model identity section provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn model_identity_section() -> std::sync::Arc<dyn SettingsSectionProvider> {
    std::sync::Arc::new(ModelIdentitySection)
}

/// Helper: context engine section provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn context_engine_section() -> std::sync::Arc<dyn SettingsSectionProvider> {
    std::sync::Arc::new(ContextEngineSection)
}

/// Helper: memory settings section provider.
///
/// Refs: I-Shell-Runtime-OnlyIO
pub fn memory_section() -> std::sync::Arc<dyn SettingsSectionProvider> {
    std::sync::Arc::new(MemorySettingsSection)
}
