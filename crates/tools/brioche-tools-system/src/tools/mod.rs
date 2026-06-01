//! Outils système individuels.
//!
//! Chaque module expose un struct implémentant `SystemTool`.
//!
//! Refs: I-Shell-ToolResult-PassThrough

pub mod filesystem;
pub mod shell;
pub mod web;

pub use filesystem::{ListDirTool, ReadFileTool, WriteFileTool};
pub use shell::ExecuteCommandTool;
pub use web::FetchUrlTool;
