//! Redb table definitions for shell persistence.
//!
//! Schema constants stay together because save, load, GC, and blob writes must
//! agree on the same table keys and append-only message layout.
//!
//! Refs: docs/SPECS.md §Book III-B Ch 1–3, I-Persist-AppendOnly

use redb::TableDefinition;

/// Session head table. Key = session ID, Value = MessagePack blob.
///
/// Overwritten at each `SaveSession` effect.
pub const SESSIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");

/// Message history table. Key = (session_id, message_index), Value = MessagePack blob.
///
/// Append-only: existing entries are never updated, only new indices inserted.
pub const MESSAGES_TABLE: TableDefinition<(&str, u32), &[u8]> = TableDefinition::new("messages");

/// Cold plugin blob table. Key = plugin_id, Value = raw binary blob.
///
/// Written asynchronously by `SavePluginBlob` without blocking the engine.
pub const BLOBS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("blobs");
