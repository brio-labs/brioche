//! # Brioche Tools — System
//!
//! Exécuteur d'outils système pour le Shell Runtime de Brioche.
//!
//! Fournit des outils de base (fichiers, shell, web) implémentant le
//! trait `SystemTool`. Le `SystemToolExecutor` agrège ces outils et
//! implémente `ToolExecutor` du Shell Runtime.
//!
//! ## Invariants
//! - I-Shell-ToolResult-PassThrough : les résultats ne sont pas transformés.
//! - I-Shell-Runtime-OnlyIO : tout I/O reste dans le Shell.
//!
//! Refs: SPECS.md §Book III-A

pub mod registry;
pub mod sandbox;
pub mod tools;

pub use registry::{SystemTool, SystemToolExecutor, ToolError};
pub use sandbox::{AllowList, SandboxPolicy};
pub use tools::{ExecuteCommandTool, FetchUrlTool, ListDirTool, ReadFileTool, WriteFileTool};
