//! Book I bindings for Book II governance trait contracts.
//!
//! `brioche-governance` owns policy trait definitions without depending on
//! kernel types. This module binds those generic contracts to concrete Book I
//! types and re-exports the trait names used by engine code.
//!
//! Invariants upheld:
//! - I-Core-PluginOrder: Hook implementations expose deterministic ordering.
//! - I-Core-StreamNoBranch: Hooks are stored in pre-routed capability vectors.
//! - I-Gov-TraitAtomic: Each hook trait represents one lifecycle capability.
//!
//! Refs: docs/SPECS.md §4, §Book II

pub use brioche_governance::{
    AfterPrediction, BeforePrediction, ConsistencyVerifier, CowBudgetPolicy, CycleRollbackPolicy,
    DecisionAggregator, EpochInterceptor, GovernanceFailoverHandler, HookEffectConstraint, OnError,
    OnInput, OnStreamEvent, OnToolCalls, OnToolResult, PluginPersistence, SignalDrainOrder,
    SubRoutineHandler, SubRoutineHydrator, SubRoutineLifecycleGuard,
};

use crate::{
    BriocheError, ChatMessage, Effect, EngineInput, EpochAction, ExtVTable, ExtensionStorage,
    PluginError, PolicyDecision, Session, SessionRegistry, SignalDrainBatch, StreamAction,
    StreamEvent, SubRoutineHandle, ToolCallDescriptor, ToolResultDTO,
};

/// Concrete `OnInput` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Core-PluginOrder
pub type OnInputPlugin = dyn OnInput<
        EngineInput = EngineInput,
        ExtensionStorage = ExtensionStorage,
        PolicyDecision = PolicyDecision,
        PluginError = PluginError,
    >;

/// Concrete `BeforePrediction` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Gov-Decision-Required
pub type BeforePredictionPlugin = dyn BeforePrediction<
        ChatMessage = ChatMessage,
        ExtensionStorage = ExtensionStorage,
        PolicyDecision = PolicyDecision,
        PluginError = PluginError,
    >;

/// Concrete `OnStreamEvent` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Core-StreamNoBranch
pub type OnStreamEventPlugin = dyn OnStreamEvent<
        StreamEvent = StreamEvent,
        ExtensionStorage = ExtensionStorage,
        StreamAction = StreamAction,
        PluginError = PluginError,
    >;

/// Concrete `AfterPrediction` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Core-NoPanic
pub type AfterPredictionPlugin =
    dyn AfterPrediction<ExtensionStorage = ExtensionStorage, PluginError = PluginError>;

/// Concrete `OnToolCalls` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
pub type OnToolCallsPlugin = dyn OnToolCalls<
        ToolCallDescriptor = ToolCallDescriptor,
        ExtensionStorage = ExtensionStorage,
        PluginError = PluginError,
    >;

/// Concrete `OnToolResult` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
pub type OnToolResultPlugin = dyn OnToolResult<
        ToolResultDto = ToolResultDTO,
        ExtensionStorage = ExtensionStorage,
        PluginError = PluginError,
    >;

/// Concrete `OnError` trait object used by the kernel.
///
/// Refs: I-Gov-TraitAtomic, I-Gov-Failover
pub type OnErrorPlugin = dyn OnError<
        ExtensionStorage = ExtensionStorage,
        PolicyDecision = PolicyDecision,
        PluginError = PluginError,
    >;

/// Concrete `EpochInterceptor` trait object used by the kernel.
///
/// Refs: I-Comp-Epoch-First, I-Gov-Epoch-Reject
pub type EpochInterceptorPlugin = dyn EpochInterceptor<
        EngineInput = EngineInput,
        ExtensionStorage = ExtensionStorage,
        EpochAction = EpochAction,
        PluginError = PluginError,
    >;

/// Concrete `SubRoutineHandler` trait object used by the kernel.
///
/// Refs: I-Comp-Epoch-Subroutine
pub type SubRoutineHandlerPlugin = dyn SubRoutineHandler<
        Session = Session,
        EngineInput = EngineInput,
        Effect = Effect,
        PluginError = PluginError,
    >;

/// Concrete `SubRoutineHydrator` trait object used by the kernel.
///
/// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
pub type SubRoutineHydratorPlugin =
    dyn SubRoutineHydrator<Session = Session, BriocheError = BriocheError>;

/// Concrete `ConsistencyVerifier` trait object used by the kernel.
///
/// Refs: I-Core-NoPanic, I-Gov-NoCoreMutation
pub type ConsistencyVerifierPlugin = dyn ConsistencyVerifier<
        Session = Session,
        PolicyDecision = PolicyDecision,
        PluginError = PluginError,
    >;

/// Concrete `DecisionAggregator` trait object used by the kernel.
///
/// Refs: I-Gov-Decision-Required
pub type DecisionAggregatorPlugin = dyn DecisionAggregator<
        PolicyDecision = PolicyDecision,
        ExtensionStorage = ExtensionStorage,
        PluginError = PluginError,
    >;

/// Concrete `SignalDrainOrder` trait object used by the shell boundary.
///
/// Refs: I-Shell-Drain-Atomic
pub type SignalDrainOrderPlugin = dyn SignalDrainOrder<SignalDrainBatch = SignalDrainBatch>;

/// Concrete `CowBudgetPolicy` trait object used by rollback policies.
///
/// Refs: I-Gov-CowBudget-Adaptative
pub type CowBudgetPolicyPlugin = dyn CowBudgetPolicy;

/// Concrete `CycleRollbackPolicy` trait object used by the kernel.
///
/// Refs: I-Gov-Rollback-BestEffort
pub type CycleRollbackPolicyPlugin = dyn CycleRollbackPolicy<
        ExtensionStorage = ExtensionStorage,
        ExtVTable = ExtVTable,
        CowBudgetPolicy = CowBudgetPolicyPlugin,
    >;

/// Concrete `SubRoutineLifecycleGuard` trait object used by the kernel.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard
pub type SubRoutineLifecycleGuardPlugin = dyn SubRoutineLifecycleGuard<
        SubRoutineHandle = SubRoutineHandle,
        Session = Session,
        SessionRegistry = SessionRegistry,
        Effect = Effect,
        PluginError = PluginError,
    >;

/// Concrete `GovernanceFailoverHandler` trait object used by the kernel.
///
/// Refs: I-Gov-Failover
pub type GovernanceFailoverHandlerPlugin =
    dyn GovernanceFailoverHandler<Session = Session, Effect = Effect, PluginError = PluginError>;
