//! # Brioche Tools — System
//!
//! System tool executor for the Brioche Shell Runtime.
//!
//! Provides basic tools (files, shell, web) implementing the
//! `SystemTool` trait. The `SystemToolExecutor` aggregates these tools and
//! implements `ToolExecutor` from the Shell Runtime.
//!
//! ## Invariants
//! - I-Shell-ToolResult-PassThrough: results are not transformed.
//! - I-Shell-Runtime-OnlyIO: all I/O stays in the Shell.
//!
//! Refs: docs/SPECS.md §Book III-A

pub mod registry;
pub mod tools;

pub use registry::{
    AllowList, ConfirmHandler, SandboxPolicy, SystemTool, SystemToolExecutor, ToolError,
};
pub use tools::{ExecuteCommandTool, FetchUrlTool, ListDirTool, ReadFileTool, WriteFileTool};
