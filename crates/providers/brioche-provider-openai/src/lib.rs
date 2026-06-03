//! # Brioche Provider — OpenAI
//!
//! `LlmClient` implementation for OpenAI-compatible endpoints
//! (OpenAI, Azure OpenAI, Ollama, OpenRouter, etc.).
//!
//! This crate only depends on `brioche-shell-runtime` (traits) and
//! brioche-core (constants). It is independent of any user interface
//! (CLI, GUI, Tauri).
//!
//! ## Architecture
//! ```text
//! OpenAiLlmClient
//!   ├── HTTP SSE stream (reqwest)
//!   ├── SseParser (line by line)
//!   ├── Segmenter (MAX_INLINE_CHUNK)
//!   ├── shell.send_input(EngineInput::LlmStream) → kernel
//!   └── broadcast::Sender<ShellEvent> → projection
//! ```
//!
//! ## Invariants
//! - I-Core-ChunkBudget: any fragment > 4096 bytes is segmented.
//! - I-Shell-Network-Signal: network error → SystemSignal::NetworkUnavailable.
//!
//! Refs: SPECS.md §Book III-A

pub mod client;
pub mod config;
pub mod extractor;
pub mod request;
pub mod sse;

pub use brioche_shell_runtime::ShellEvent;
pub use client::{OpenAiLlmClient, SharedHistory};
pub use config::OpenAiConfig;
pub use extractor::{
    ChunkExtractor, ExtractedText, ReasoningModelExtractor, StandardExtractor,
    chunk_extractor_for_model,
};
