//! # Brioche Desktop — Book IV
//!
//! Tauri v2 desktop GUI for Brioche.
//!
//! Provides a chat interface with slash-command support,
//! multi-session management, and persistence.
//! Backed by the same shell runtime as `agent-terminal`.
//!
//! ## Architecture
//!
//! The backend exposes Tauri commands that the frontend calls via IPC.
//! State is managed in a `DesktopState` struct stored in Tauri's managed state.
//!
//! ```text
//! Frontend (React) → Tauri IPC → DesktopState → SessionManager → BriocheShell → Core
//!                                    ↑
//!                           Events (LLM chunks) → Frontend
//! ```
//!
//! ## Invariants upheld
//! - I-Shell-Runtime-OnlyIO: Backend performs I/O only; no kernel logic.
//! - I-App-NoPrintln: Library code uses `tracing`, never `println!`.
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub mod commands;
pub mod memory;
pub mod profiles;
pub mod settings;
pub mod skills;
pub mod state;

pub use commands::*;
pub use memory::*;
pub use profiles::*;
pub use settings::*;
pub use skills::*;
pub use state::*;
