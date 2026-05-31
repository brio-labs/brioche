//! Configuration explicite pour le provider OpenAI.
//!
//! Aucune variable d'environnement n'est lue ici. L'assembleur (CLI)
//! lit l'environnement et injecte une `OpenAiConfig` construite.
//!
//! Refs: I-Shell-Runtime-OnlyIO

/// Configuration du client OpenAI.
///
/// `base_url` permet de cibler Ollama, OpenRouter, ou tout autre
/// endpoint compatible OpenAI.
///
/// # Invariants
/// - `model` et `api_key` ne sont jamais vides après construction.
///
/// Refs: I-Shell-Runtime-OnlyIO
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
    pub timeout_ms: u64,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
            max_tokens: 4096,
            timeout_ms: 120_000,
        }
    }
}
