//! Tauri IPC commands for the desktop app.
//!
//! These commands are called by the frontend via `invoke()`.
//! All commands return `Result<T, String>` for simple frontend error handling.
//!
//! Refs: I-Shell-Runtime-OnlyIO

pub mod config;
pub mod extensions;
pub mod fs;
pub mod session;
pub mod shell;

pub use config::*;
pub use extensions::*;
pub use fs::*;
pub use session::*;
