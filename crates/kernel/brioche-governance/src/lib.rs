//! # Brioche Governance — Book II (Trait Contracts)
//!
//! Atomic capability traits for policy code that the synchronous kernel routes.
//! The crate owns trait contracts only; it contains no policy implementations
//! and depends on no Book I kernel types.
//!
//! ## Public interface
//! - `OnInput`, `BeforePrediction`, `OnStreamEvent`, `AfterPrediction`,
//!   `OnToolCalls`, `OnToolResult`, and `OnError`: one hook capability each.
//! - `PluginPersistence`: optional state serialization capability.
//! - Governance anchor traits such as `EpochInterceptor`,
//!   `DecisionAggregator`, and `HookEffectConstraint`.
//!
//! ## Invariants upheld
//! - I-Gov-TraitAtomic: Hook traits are atomic capabilities, not taxonomy.
//! - I-Eco-ExtensionOverMod: Policy extends the kernel through traits only.
//! - I-Core-StreamNoBranch: Trait objects are pre-routed by the kernel.
//!
//! Refs: docs/SPECS.md §Book II

#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::any::{Any, TypeId};

/// Optional persistence contract for plugins with serialized state.
///
/// Persistence is independent from hook capability traits. Stateless hook
/// implementations do not implement this trait.
///
/// Refs: I-Persist-Idempotence, I-Gov-TraitAtomic
pub trait PluginPersistence: Send + Sync {
    /// Reserved keys in extension storage. Format: `"plugin_name::state_name"`.
    ///
    /// # Complexity
    /// O(1) for the default implementation. Implementations may scan static
    /// metadata but must not inspect runtime storage.
    ///
    /// # Panics
    /// Must not panic.
    fn owned_state_keys(&self) -> &'static [&'static str] {
        &[]
    }

    /// Serialize the default state into a binary blob.
    ///
    /// # Complexity
    /// O(1) for the default empty blob. Implementations are O(serialization
    /// cost) for their own state.
    ///
    /// # Panics
    /// Must not panic; serialization failures belong in typed state codecs.
    fn default_state_blob(&self) -> Vec<u8> {
        vec![]
    }

    /// Deserialize a binary state blob.
    ///
    /// # Complexity
    /// O(1) for the default rejection. Implementations are O(deserialization
    /// cost) for their own state.
    ///
    /// # Errors
    /// Returns `Err` when the blob is unsupported, corrupted, truncated, or
    /// version-incompatible.
    ///
    /// # Panics
    /// Must not panic.
    fn deserialize_state(&self, _raw: &[u8]) -> Result<Box<dyn Any + Send + Sync>, String> {
        Err("not implemented".into())
    }
}

macro_rules! ordered_capability_header {
    () => {
        /// Unique plugin name, used for deterministic total ordering.
        ///
        /// # Complexity
        /// O(1). Returns a static string reference.
        ///
        /// # Panics
        /// Must not panic.
        fn name(&self) -> &'static str;

        /// Deterministic evaluation order. Lower priority runs earlier.
        ///
        /// # Complexity
        /// O(1). Returns a scalar ordering key.
        ///
        /// # Panics
        /// Must not panic.
        fn priority(&self) -> i16 {
            0
        }
    };
}

/// Intercepts engine input before standard dispatch.
///
/// Refs: I-Gov-TraitAtomic, I-Core-NoPanic
pub trait OnInput: Send + Sync {
    ordered_capability_header!();

    /// Engine input type supplied by Book I.
    type EngineInput;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Policy decision type supplied by Book I.
    type PolicyDecision;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Evaluate input policy.
    ///
    /// # Complexity
    /// O(policy cost). The kernel has already selected this hook route.
    ///
    /// # Errors
    /// Returns `PluginError` when the policy cannot produce a decision for
    /// the supplied input and extension state.
    ///
    /// # Panics
    /// Must not panic; policy failures must be returned as `Err`.
    fn on_input(
        &self,
        input: &Self::EngineInput,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::PolicyDecision, Self::PluginError>;
}

/// Evaluates policy immediately before LLM prediction.
///
/// Refs: I-Gov-TraitAtomic, I-Gov-Decision-Required
pub trait BeforePrediction: Send + Sync {
    ordered_capability_header!();

    /// Chat message type supplied by Book I.
    type ChatMessage;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Policy decision type supplied by Book I.
    type PolicyDecision;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Evaluate prediction policy for the current history.
    ///
    /// # Complexity
    /// O(policy cost + h) where h is any history scanned by the implementation.
    ///
    /// # Errors
    /// Returns `PluginError` when prediction policy evaluation fails.
    ///
    /// # Panics
    /// Must not panic; policy failures must be returned as `Err`.
    fn before_prediction(
        &self,
        history: &[Self::ChatMessage],
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::PolicyDecision, Self::PluginError>;
}

/// Handles a single stream event on the streaming hot path.
///
/// Refs: I-Gov-TraitAtomic, I-Core-StreamNoBranch
pub trait OnStreamEvent: Send + Sync {
    ordered_capability_header!();

    /// Stream event type supplied by Book I.
    type StreamEvent;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Stream action type supplied by Book I.
    type StreamAction;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Evaluate stream policy for one event.
    ///
    /// # Complexity
    /// O(policy cost) for one stream event on the hot path; implementations
    /// should avoid allocation unless their returned action requires it.
    ///
    /// # Errors
    /// Returns `PluginError` when the stream event cannot be evaluated.
    ///
    /// # Panics
    /// Must not panic; stream policy faults must be returned as `Err`.
    fn on_stream_event(
        &self,
        event: &Self::StreamEvent,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::StreamAction, Self::PluginError>;
}

/// Runs after prediction completes and before tool execution.
///
/// Refs: I-Gov-TraitAtomic, I-Core-NoPanic
pub trait AfterPrediction: Send + Sync {
    ordered_capability_header!();

    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Observe or update extension state after prediction.
    ///
    /// # Complexity
    /// O(policy cost) for post-prediction observation or state update.
    ///
    /// # Errors
    /// Returns `PluginError` when post-prediction state handling fails.
    ///
    /// # Panics
    /// Must not panic; policy faults must be returned as `Err`.
    fn after_prediction(&self, ext: &mut Self::ExtensionStorage) -> Result<(), Self::PluginError>;
}

/// Mutates tool call descriptors before tool execution.
///
/// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
pub trait OnToolCalls: Send + Sync {
    ordered_capability_header!();

    /// Tool descriptor type supplied by Book I.
    type ToolCallDescriptor;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Mutate pending tool call descriptors in place.
    ///
    /// # Complexity
    /// O(policy cost + c) where c is the number of tool call descriptors the
    /// implementation inspects or mutates.
    ///
    /// # Errors
    /// Returns `PluginError` when pending tool calls cannot be validated or
    /// rewritten.
    ///
    /// # Panics
    /// Must not panic; tool-call policy faults must be returned as `Err`.
    fn on_tool_calls(
        &self,
        calls: &mut Vec<Self::ToolCallDescriptor>,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<(), Self::PluginError>;
}

/// Mutates tool results before they are persisted to history.
///
/// Refs: I-Gov-TraitAtomic, I-Core-ActiveToolCall
pub trait OnToolResult: Send + Sync {
    ordered_capability_header!();

    /// Tool result type supplied by Book I.
    type ToolResultDto;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Mutate tool results in place.
    ///
    /// # Complexity
    /// O(policy cost + r) where r is the number of tool results the
    /// implementation inspects or mutates.
    ///
    /// # Errors
    /// Returns `PluginError` when tool results cannot be validated or rewritten.
    ///
    /// # Panics
    /// Must not panic; tool-result policy faults must be returned as `Err`.
    fn on_tool_result(
        &self,
        results: &mut Vec<Self::ToolResultDto>,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<(), Self::PluginError>;
}

/// Handles plugin errors through policy.
///
/// Refs: I-Gov-TraitAtomic, I-Gov-Failover
pub trait OnError: Send + Sync {
    ordered_capability_header!();

    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Policy decision type supplied by Book I.
    type PolicyDecision;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Evaluate recovery policy for a plugin error.
    ///
    /// # Complexity
    /// O(policy cost). The kernel invokes this only for materialized plugin
    /// errors on the error-handling route.
    ///
    /// # Errors
    /// Returns `PluginError` when recovery policy itself fails.
    ///
    /// # Panics
    /// Must not panic; recovery faults must be returned as `Err`.
    fn on_error(
        &self,
        error: &Self::PluginError,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::PolicyDecision, Self::PluginError>;
}

/// First governance trait evaluated in every transition cycle.
///
/// Refs: I-Comp-Epoch-First, I-Gov-Epoch-Reject
pub trait EpochInterceptor: Send + Sync {
    /// Engine input type supplied by Book I.
    type EngineInput;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Epoch action type supplied by Book I.
    type EpochAction;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Intercept the current epoch to enforce temporal isolation.
    ///
    /// # Complexity
    /// O(policy cost) for one transition epoch.
    ///
    /// # Errors
    /// Returns `PluginError` when temporal policy cannot accept, reject, or
    /// transform the epoch.
    ///
    /// # Panics
    /// Must not panic; epoch policy faults must be returned as `Err`.
    fn intercept_epoch(
        &self,
        input: &Self::EngineInput,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::EpochAction, Self::PluginError>;
}

/// Handles sub-routine delegation and resolution.
///
/// Refs: I-Comp-Epoch-Subroutine
pub trait SubRoutineHandler: Send + Sync {
    /// Session type supplied by Book I.
    type Session;
    /// Engine input type supplied by Book I.
    type EngineInput;
    /// Effect type supplied by Book I.
    type Effect;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Delegate or resolve a sub-routine transition.
    ///
    /// # Complexity
    /// O(policy cost) plus any parent/child session mutations.
    ///
    /// # Errors
    /// Returns `PluginError` when delegation or resolution cannot be completed.
    ///
    /// # Panics
    /// Must not panic; sub-routine policy faults must be returned as `Err`.
    fn handle_subroutine(
        &self,
        parent: &mut Self::Session,
        child: &mut Self::Session,
        input: &Self::EngineInput,
    ) -> Result<Option<Vec<Self::Effect>>, Self::PluginError>;
}

/// Hydrates a sub-routine session from a persisted head blob.
///
/// Refs: I-Shell-Session-NoSend, I-Persist-Idempotence
pub trait SubRoutineHydrator: Send + Sync {
    /// Session type supplied by Book I.
    type Session;
    /// Kernel error type supplied by Book I.
    type BriocheError;

    /// Deserialize a head blob into a session.
    ///
    /// # Complexity
    /// O(deserialization cost) for the supplied head blob.
    ///
    /// # Errors
    /// Returns `BriocheError` when the head blob is corrupted, truncated,
    /// version-incompatible, or otherwise cannot hydrate a session.
    ///
    /// # Panics
    /// Must not panic; hydration faults must be returned as `Err`.
    fn hydrate(&self, head_blob: &[u8]) -> Result<Self::Session, Self::BriocheError>;
}

/// Post-transition mechanical consistency check.
///
/// Refs: I-Core-NoPanic, I-Gov-NoCoreMutation
pub trait ConsistencyVerifier: Send + Sync {
    /// Session type supplied by Book I.
    type Session;
    /// Policy decision type supplied by Book I.
    type PolicyDecision;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Verify mechanical consistency after a transition.
    ///
    /// # Complexity
    /// O(verification cost) for one post-transition session snapshot.
    ///
    /// # Errors
    /// Returns `PluginError` when the verifier cannot complete its consistency
    /// check.
    ///
    /// # Panics
    /// Must not panic; verifier faults must be returned as `Err`.
    fn verify_consistency(
        &self,
        session: &Self::Session,
    ) -> Result<Option<Self::PolicyDecision>, Self::PluginError>;
}

/// Aggregates `before_prediction` decisions from multiple plugins.
///
/// Refs: I-Gov-Decision-Required
pub trait DecisionAggregator: Send + Sync {
    /// Policy decision type supplied by Book I.
    type PolicyDecision;
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Reduce per-plugin decisions into one decision.
    ///
    /// # Complexity
    /// O(policy cost + d) where d is the number of decisions supplied.
    ///
    /// # Errors
    /// Returns `PluginError` when decisions cannot be reduced to one required
    /// policy decision.
    ///
    /// # Panics
    /// Must not panic; aggregation faults must be returned as `Err`.
    fn aggregate_decisions(
        &self,
        decisions: Vec<Self::PolicyDecision>,
        ext: &mut Self::ExtensionStorage,
    ) -> Result<Self::PolicyDecision, Self::PluginError>;
}

/// Defines the invariant drainage order of separate shell channels.
///
/// Refs: I-Shell-Drain-Atomic
pub trait SignalDrainOrder: Send + Sync {
    /// Signal drain batch type supplied by Book I.
    type SignalDrainBatch;

    /// Drain pending signals from all channels into a batch.
    ///
    /// # Complexity
    /// O(drain cost) for the implementation's signal sources.
    ///
    /// # Panics
    /// Must not panic; signal drain implementations must choose a recoverable
    /// empty or terminal batch representation instead.
    fn drain(&self) -> Self::SignalDrainBatch;
}

/// O(1) validation of effects requested by plugins on specific hooks.
///
/// Refs: I-Core-HookEffect-O1
pub trait HookEffectConstraint: Send + Sync {
    /// O(1) validation by bitmask lookup.
    ///
    /// # Complexity
    /// O(1). Intended for bitmask or table lookup on the hot path.
    ///
    /// # Panics
    /// Must not panic.
    fn is_allowed_fast(&self, hook_index: u8, effect_mask: u64) -> bool;

    /// Validation by hook and effect name for future extensions.
    ///
    /// # Complexity
    /// O(policy lookup cost). This fallback is not the hot path.
    ///
    /// # Panics
    /// Must not panic.
    fn is_allowed_fallback(&self, hook_name: &str, effect_variant: &str) -> bool;
}

/// Granular COW rollback of extension storage on budget overrun.
///
/// Refs: I-Gov-Rollback-BestEffort
pub trait CycleRollbackPolicy: Send + Sync {
    /// Extension storage type supplied by Book I.
    type ExtensionStorage;
    /// Extension vtable type supplied by Book I.
    type ExtVTable;
    /// COW budget policy type supplied by Book I.
    type CowBudgetPolicy: ?Sized;

    /// Start monitoring a hook.
    ///
    /// # Complexity
    /// O(policy cost) to initialize one hook rollback frame.
    ///
    /// # Panics
    /// Must not panic.
    fn begin_hook(&mut self, hook_name: &'static str);

    /// Record first mutation of an extension during the hook.
    ///
    /// # Complexity
    /// O(snapshot cost) for the first mutation of one extension type.
    ///
    /// # Panics
    /// Must not panic; failed snapshots must be represented by policy state.
    fn on_mutation(&mut self, type_id: TypeId, vtable: &Self::ExtVTable, current: &dyn Any);

    /// Keep hook mutations.
    ///
    /// # Complexity
    /// O(committed mutation count) for the current hook.
    ///
    /// # Panics
    /// Must not panic.
    fn commit_hook(&mut self, ext: &mut Self::ExtensionStorage);

    /// Restore snapshotted mutations.
    ///
    /// # Complexity
    /// O(rolled-back mutation count) for the current hook.
    ///
    /// # Panics
    /// Must not panic.
    fn rollback_hook(&mut self, ext: &mut Self::ExtensionStorage);

    /// Return `true` when the hook exceeded its COW budget.
    ///
    /// # Complexity
    /// O(1). Returns accumulated budget state.
    ///
    /// # Panics
    /// Must not panic.
    fn is_budget_exceeded(&self) -> bool {
        false
    }

    /// Attach a per-hook COW budget policy.
    ///
    /// # Complexity
    /// O(1) to store the boxed policy; drop cost is implementation-defined
    /// when replacing or rejecting an existing policy.
    ///
    /// # Panics
    /// Must not panic.
    #[allow(
        clippy::boxed_local,
        reason = "Trait-object injection requires owned policy storage."
    )]
    fn set_cow_budget_policy(&mut self, _policy: Box<Self::CowBudgetPolicy>) {}
}

/// Cleanup of `SessionRegistry` on outgoing sub-routine transition.
///
/// Refs: I-Gov-SubRoutineLifecycle-Guard
pub trait SubRoutineLifecycleGuard: Send + Sync {
    /// Sub-routine handle type supplied by Book I.
    type SubRoutineHandle;
    /// Session type supplied by Book I.
    type Session;
    /// Registry type supplied by Book I.
    type SessionRegistry;
    /// Effect type supplied by Book I.
    type Effect;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Clean up registry state when a sub-routine exits.
    ///
    /// # Complexity
    /// O(policy cost) plus registry/session mutation cost for one exit.
    ///
    /// # Errors
    /// Returns `PluginError` when lifecycle cleanup cannot produce a safe
    /// effect sequence.
    ///
    /// # Panics
    /// Must not panic; lifecycle faults must be returned as `Err`.
    fn on_exit(
        &self,
        handle: Self::SubRoutineHandle,
        parent: &mut Self::Session,
        registry: &mut Self::SessionRegistry,
    ) -> Result<Vec<Self::Effect>, Self::PluginError>;
}

/// Safety net for cascading governance trait failures.
///
/// Refs: I-Gov-Failover
pub trait GovernanceFailoverHandler: Send + Sync {
    /// Session type supplied by Book I.
    type Session;
    /// Effect type supplied by Book I.
    type Effect;
    /// Plugin error type supplied by Book I.
    type PluginError;

    /// Transform a plugin fault into a safe terminal effect sequence.
    ///
    /// # Complexity
    /// O(policy cost) to translate one governance fault.
    ///
    /// # Errors
    /// Returns `PluginError` when failover handling itself fails.
    ///
    /// # Panics
    /// Must not panic; failover faults must be returned as `Err`.
    fn handle_failure(
        &self,
        session: &mut Self::Session,
        fault: &Self::Effect,
    ) -> Result<Option<Vec<Self::Effect>>, Self::PluginError>;
}

/// Per-hook COW budget for rollback policy implementations.
///
/// Refs: I-Gov-CowBudget-Adaptative
pub trait CowBudgetPolicy: Send + Sync {
    /// Return the COW budget in bytes for the named hook.
    ///
    /// # Complexity
    /// O(policy lookup cost) for one hook name.
    ///
    /// # Panics
    /// Must not panic.
    fn max_cow_bytes(&self, hook_name: &str) -> usize;
}
