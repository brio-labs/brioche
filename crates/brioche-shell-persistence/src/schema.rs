//! Redb table definitions for the persistence layer.
//!
//! Two fundamental tables are defined:
//! - `SESSIONS_TABLE`: session heads (MessagePack DTOs), overwritten on save.
//! - `MESSAGES_TABLE`: append-only message history, composite key.
//! - `BLOBS_TABLE`: cold plugin blobs, keyed by `plugin_id`.
//!
//! Refs: SPECS.md §Book III-B Ch 1.1, I-Persist-AppendOnly

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
