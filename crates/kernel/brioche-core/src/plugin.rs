//! Book I — The Core Book: Concrete type family for governance traits.
//!
//! This module defines `CoreTypes`, the `BriocheTypes` implementation that
//! wires the generic governance traits from `brioche-governance` to the
//! concrete mechanism types in `brioche-core`. All policy traits live in
//! Book II; Book I only provides the type family and re-exports the
//! trait contracts for downstream convenience.
//!
//! Invariants upheld:
//! - I-Core-PluginOrder: Total order via `priority` + `name`.
//! - I-Core-StreamNoBranch: Pre-routed `UnifiedRoutingTable` eliminates hot-path branching.
//! - I-Gov-TraitAtomic: Each trait is a standalone capability.
//!
//! Refs: docs/SPECS.md §4, §Book II

// Re-export the entire Book II governance surface so downstream crates can
// import `brioche_core::{BriochePlugin, EpochInterceptor, ...}` as before.
pub use brioche_governance::*;

use crate::{
    AgentState, AsyncTaskResult, BriocheError, ChatMessage, Effect, EffectBit, EngineInput,
    EpochAction, EpochState, ExecutionPath, ExtensionStorage, GovernanceNotification,
    PluginError, PluginSource, PolicyDecision, Session, SessionRegistry, SignalDrainBatch,
    StreamAction, StreamEvent, SubRoutineHandle, SystemSignal, TaskId, ToolCallDescriptor,
    ToolResultDTO,
};
use crate::types::InconsistencySource;

/// Concrete type family wiring Book II governance traits to Book I types.
///
/// All trait methods in `brioche-governance` are generic over `T: BriocheTypes`.
/// `CoreTypes` is the singleton implementation used by the synchronous kernel.
///
/// Refs: I-Gov-TraitAtomic
/// # Complexity
/// O(1). Zero-sized type.
/// # Panics
/// Never panics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoreTypes;

impl BriocheTypes for CoreTypes {
    type Input = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type ChatMessage = ChatMessage;
    type StreamEvent = StreamEvent;
    type StreamAction = StreamAction;
    type ToolCallDescriptor = ToolCallDescriptor;
    type ToolResultDTO = ToolResultDTO;
    type Effect = Effect;
    type PluginError = PluginError;
    type PolicyDecision = PolicyDecision;
    type EpochAction = EpochAction;
    type Session = Session;
    type SessionRegistry = SessionRegistry;
    type AgentState = AgentState;
    type BriocheError = BriocheError;
    type SignalDrainBatch = SignalDrainBatch;
    type GovernanceNotification = GovernanceNotification;
    type AsyncTaskResult = AsyncTaskResult;
    type SystemSignal = SystemSignal;
    type InconsistencySource = InconsistencySource;
    type PluginSource = PluginSource;
    type SubRoutineHandle = SubRoutineHandle;
    type TaskId = TaskId;
    type EpochState = EpochState;
    type ExecutionPath = ExecutionPath;
    type EffectBit = EffectBit;
}
