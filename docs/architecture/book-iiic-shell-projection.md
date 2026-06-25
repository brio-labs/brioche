# Book III-C — The Shell Projection Book

> UI projection layer. Transforms kernel `Effect::ForwardToUi` instructions into structured view-model state for the Tauri / web frontend.

## Chapter 1: Extensibility Contract (UiRegistry)

The kernel never emits Vue components. It merely emits structured instructions (`Effect::ForwardToUi`) containing a widget text identifier (`widget_type`) and a JSON payload. The `UiRegistry` on the frontend maintains a dynamic mapping that resolves the identifier into an asynchronous import of the corresponding component.

### Anchor Slots

| Slot | Description |
|---|---|
| `top-bar` | Application header, horizontal alignment, fixed height. |
| `sidebar` | Left navigation, adjustable fixed width, 100% height. |
| `status-bar` | Bottom status line, telemetry, thin fixed height. |
| `input-actions` | Button bar under the prompt field. Hosts the interrupt button calling the IPC command `cancel_action`. |
| `input-overlay` | Floating contextual suggestion layer. |
| `content-renderer` | Main message rendering area (streaming). |
| `message-footer` | Meta-information or contextual actions under a message. |
| `settings-panel` | Hyperparameter adjustment drawer, anchored to the right. |

### Special Governance Widgets

| Widget | Emitter | Context |
|---|---|---|
| `system_degraded` | `QuarantineManager` | Warning banner displayed when a plugin has been quarantined. |
| `network_error` | `RecoveryPolicy` | Displayed when a `SystemSignal::NetworkUnavailable` is intercepted. |
| `status` | `RecoveryPolicy` | Generic state (e.g., "cancelled"). |
| `error` | Shell | Generic widget for errors (`Effect::Error` transformed by the shell). |
| `subroutine_timeout` | `SubRoutineTimeoutPolicy` | Displayed when a sub-routine exceeds its time limit. |

**Invariant:** No deep Proxy is applied to third-party plugin interface definitions in order to preserve the reactivity of the main rendering thread.

## Chapter 2: Streaming Rendering Engine (ContentRenderer)

### Architecture

The text flow rendering component avoids saturating Vue's VDOM without breaking it.

- A **`StreamBuffer`** encapsulates the single source of truth. It uses a Vue 3 `shallowRef` to mutate the accumulated text without granular reactivity, then trigger a single render by `requestAnimationFrame`.
- The buffer accumulates text fragments by `traceId` outside Vue's deep reactive system.
- A `requestAnimationFrame` loop flushes the buffer into the DOM via Vue's standard rendering mechanism, but at a controlled rate (max one update per frame).
- During streaming, no other component accesses partial text; it subscribes to a `stream-complete` event to read the final result. The DOM is manipulated via the Vue template, not by direct mutation.

**Invariant:** The main rendering thread never receives more than one IPC event per frame (16 ms) under normal conditions.

## Chapter 3: UiComposer

### Formal Contract

The `UiComposer` is a shell component that transforms raw `ForwardToUi` effects into semantic rendering instructions (focus, scroll, accordion expansion) without the kernel knowing their semantics.

```rust
pub struct UiComposer {
    /// Per-frame budget in ms. Default: 2ms.
    frame_budget_ms: u8,
    /// Effect priority: text > navigation > semantic > cosmetic.
    priority_tiers: [EffectPriority; 4],
    /// Per-frame effect buffer. Effects exceeding the budget
    /// slide to the next frame.
    pending_frame: Vec<Effect>,
}

pub enum EffectPriority {
    TextChunk,      // ForwardToUi with widget_type "text_chunk" — never dropped
    Navigation,     // Focus, scroll — slides if necessary
    Semantic,       // Accordion expansion, highlight — slides
    Cosmetic,       // Animations, transitions — dropped if 3 frames behind
}
```

### Properties

- The `UiComposer` consumes `ForwardToUi` effects strictly on the `requestAnimationFrame` loop. No effect is applied outside this loop (I-UI-Composer-FrameSync).
- Text effects (`text_chunk`) are flushed with absolute priority.
- Secondary semantic effects (highlight, accordion expansion, focus) can slide to the next frame if the budget is exceeded.
- The frame budget is configurable via the `UiPerformancePolicy` plugin.

## Chapter 4: Tauri and IPC Integration

### Exposed Commands

| Command | Description |
|---|---|
| `send_message(text: String)` | Injects `EngineInput::UserMessage`. |
| `cancel_action()` | Emits `SystemSignal::OperationCancelled` in the system channel. |
| `load_subroutine(id: String)` | Lazily loads a `SessionHeadDTO` from `SubRoutineCache` for the UI, then emits `EngineInput::RestoreSubRoutine` for kernel hydration. |

### Event Channel

- `stream_batch` : MessagePack payload containing a `Map<traceId, accumulatedText>`, emitted by the adaptive batching regulator.

## Chapter 5: Sub-routine Management

### Local States

When a `SubRoutine` emits text, a reactive registry (`activeStreams`) records the routine. The flow is directed to a `<details>` element (accordion) closed by default so as not to distract the user.

**Local states of a sub-routine:**

| State | Description |
|---|---|
| `idle` | Registered but never opened. |
| `loading` | The user opened the accordion, lazy request in progress (`RestoreSubRoutine` sent). |
| `loaded` | Content available, rendered via an isolated `ContentRenderer` instance. |
| `error` | Loading failure. |
| `timeout` | The sub-routine exceeded its time limit (`SubRoutineTimeoutPolicy` plugin). |

**Behavior:**

Opening an accordion in `idle` state intercepts the native event, displays a loading skeleton, and calls `load_subroutine`. Effective DOM opening is deferred until transition to `loaded` (upon receipt of `Effect::SubRoutineRestored`).

## Chapter 6: UiPerformancePolicy

### Objective

Expose a user configuration point for per-frame rendering budget, without modifying the kernel.

### Extended State

```rust
pub struct UiPerformanceState {
    pub frame_budget_ms: u8, // default 2, configurable
}
impl BriocheExtensionType for UiPerformanceState {
    const EXT_ID: &'static str = "shell::ui_performance";
}
```

### Algorithm (`Effect::ForwardToUi` consumption)

1. The `UiPerformancePolicy` plugin registers as an interceptor on the shell side (outside the kernel) on `ForwardToUi` effects before they reach the `UiComposer`.
2. It modulates the `UiComposer` frame budget according to `UiPerformanceState.frame_budget_ms`.
3. If the budget is exceeded, secondary semantic effects (highlight, expansion, focus) slide to the next frame. Text effects (`text_chunk`) are always flushed with priority.

**Note:** This plugin is a shell extension. It does not modify the `Effect` interface nor the `UiComposer`; it registers on effect consumption upstream of the composer.

## Chapter 7: Limits of the Shell Projection Layer

### What this layer does not do

- **No synchronous orchestration logic** : all transition decisions are in the kernel (Books I and II).
- **No business policy** : circuit breaker, token tracking, etc. are in the ecosystem (Book IV).
- **No modification of the extension contract** : `BriochePlugin`, `Effect`, `PolicyDecision` are fixed since Book I.
- **No persistence** : persistence is in the Shell Persistence layer (Book III-B).
- **No runtime** : the asynchronous runtime is in the Shell Runtime layer (Book III-A).
