//! # Brioche Provider — OpenAI
//!
//! Implémentation `LlmClient` pour les endpoints compatibles OpenAI
//! (OpenAI, Azure OpenAI, Ollama, OpenRouter, etc.).
//!
//! Cette crate ne dépend que de `brioche-shell-runtime` (traits) et
//! de brioche-core (constantes). Elle est indépendante de toute
//! interface utilisateur (CLI, GUI, Tauri).
//!
//! ## Architecture
//! ```text
//! OpenAiLlmClient
//!   ├── HTTP SSE stream (reqwest)
//!   ├── SseParser (ligne par ligne)
//!   ├── Segmenteur (MAX_INLINE_CHUNK)
//!   ├── shell.send_input(EngineInput::LlmStream) → kernel
//!   └── broadcast::Sender<LlmChunk> → projection
//! ```
//!
//! ## Invariants
//! - I-Core-ChunkBudget : tout fragment > 4096 octets est segmenté.
//! - I-Shell-Network-Signal : erreur réseau → SystemSignal::NetworkUnavailable.
//!
//! Refs: SPECS.md §Book III-A

pub mod client;
pub mod config;
pub mod request;
pub mod sse;

pub use client::{LlmChunk, OpenAiLlmClient, SharedHistory};
pub use config::OpenAiConfig;
