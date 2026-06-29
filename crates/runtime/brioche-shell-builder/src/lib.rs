//! Shared shell construction helpers for Brioche applications.
//!
//! This crate sits above `brioche-shell-runtime` and `brioche-shell-persistence`
//! and provides the common logic used by both `agent-terminal` and
//! `brioche-desktop` to assemble a [`BriocheShell`], its LLM client, and the
//! standard effect executor.
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub mod builder;
pub mod config;

pub use builder::{ShellBuilder, default_session_factory, session_factory_with_head};
pub use config::{assemble_openai_config_from_env, assemble_openai_config_from_settings};
