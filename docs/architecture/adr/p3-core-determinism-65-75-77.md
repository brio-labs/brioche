# ADR: Core determinism and extension-type auto-collection for #65, #75, #77

## Status

Accepted

## Context

The three issues span the boundary between Book I (Core / Macro) and Book II
(Governance) with side effects on Book III-A (Shell Persistence):

- **#65** — `SerializeFn` returned `Vec<u8>`, forcing callers to either silently
drop serialization failures or invent a sentinel value. Empty blobs could be
persisted by accident.
- **#75** — The `BriocheExtensionType` derive macro required an explicit
`#[brioche(nested_carrier)]` annotation on fields that carried other
extension types. This was error-prone and let non-deterministic nested
collections slip through when the annotation was forgotten.
- **#77** — Persisted/extension-serialized structs used `usize`/`isize`, making
cross-architecture replay and bit-for-bit determinism non-guaranteed.

Because the fix touches the macro, the vtable, the `ExtensionStorage` API, and
a cascade of governance/plugin types, this ADR records the coordinated decisions.

## Decisions

1. **`SerializeFn` returns `Result<Vec<u8>, String>`.**
   - The vtable serialize pointer is now `fn(&dyn Any) -> Result<Vec<u8>, String>`.
   - `ExtensionStorage::insert` returns `Result<(), BriocheError>` and propagates
     serialization failures to callers instead of persisting empty blobs.
   - All production and test call sites were updated to handle the result.

2. **`BriocheExtensionType` auto-collects nested carriers.**
   - The derive macro now inspects every struct/enum field and extracts the
     inner type from `Vec<T>`, `Option<T>`, `Box<T>`, `BTreeMap<K, V>`,
     `IndexMap<K, V>`, `HashMap<K, V>`, `BTreeSet<T>`, `HashSet<T>`, and
     `Result<T, E>`.
   - Primitives (`u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i32`, `i64`, `bool`,
     `f32`, `f64`, `char`, `String`, `str`, `()`) and their references are
     filtered out automatically.
   - The macro emits `const` assertions requiring every remaining nested carrier
     to implement `BriocheExtensionType`, ensuring that a `Vec<PolicyDecision>`
     (or similar) inside a persisted struct is checked at compile time.
   - The explicit `#[brioche(nested_carrier)]` attribute is no longer required
     and was removed from the codebase.

3. **`PolicyDecision`, `TransitionTrace`, and `DecisionNode` derive `BriocheExtensionType`.**
   - These types are now extension-persistable so their transitive carriers
     (`HistoryEdit`, `Effect`, `UiWidget`, `ErrorDetail`, `ErrorCode`,
     `InconsistencySource`, `HistoryOperation`, `RollbackEvent`, `AgentStateTag`,
     `ChatMessage`, `SubRoutineHandle`, `PluginSource`, `TaskId`, `PluginError`,
     `ActiveToolCall`, `ToolCallDescriptor`, `ToolOutcome`, `SystemSignal`,
     `AsyncTaskResult`, `ToolStatus`, `GovernanceNotification`, `DecisionNode`,
     `DecisionCondition`, `AuditEntry`) also derive `BriocheExtensionType` and
     `Default` where needed.
   - Collection fields inside the newly persisted types were annotated with
     `#[brioche(deterministic_order)]` where required by the macro.
   - `#[non_exhaustive]` enums without a unit variant received manual `Default`
     implementations because the derive macro cannot generate one.

4. **Persisted structs use fixed-width integers.**
   - `usize`/`isize` were replaced by `u64`/`u32` in the structs listed in #77:
     `brioche-shell-persistence` DTOs/storage, `HistoryEdit`, `ErrorDetail`,
     `RollbackEvent`, `gc_policy.rs`, `audit_logger.rs`, and governance-default
     tool-pipeline / decision-aggregator types.
   - In-memory use sites (slicing, indexing, `Vec::len` comparisons) cast to
     `usize` explicitly at the point of use, preserving the schema while keeping
     Rust APIs ergonomic.

5. **No business logic moved into Core.**
   - All changes are type-system, serialization, and macro-level plumbing.
   - Governance decisions still live in traits and plugins; Core only surfaces
     errors and persists deterministic values.

## Consequences

- Serialization failures now stop persistence rather than writing an empty
  blob, making debugging and replay safer.
- Adding a nested carrier to a persisted struct is a compile-time guarantee: if it
  is not `BriocheExtensionType`, the build fails.
- Cross-architecture round-trips and replay logs are now deterministic because
  no persisted struct contains `usize` or `isize`.
- `#[brioche(nested_carrier)]` is no longer part of the public API; existing
  users can remove it.

## Invariants

Refs: I-Core-Pure, I-Core-NoPanic, I-Core-ExtO1, I-Eco-OrderedCollections,
I-Eco-ExtensionOverMod, I-Gov-Rollback-BestEffort, I-Shell-Persistence-Mode
