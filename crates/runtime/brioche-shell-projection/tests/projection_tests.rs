//! Integration tests for `brioche-shell-projection`.
//!
//! Covers:
//! - UiRegistry registration, resolution, and special widgets
//! - StreamBuffer accumulation and deterministic ordering
//! - ContentRenderer consumption of text_chunk effects
//! - UiComposer priority scheduling and frame budget
//! - UiPerformancePolicy storage synchronization
//!
//! Refs: SPECS.md §Book III-C

use brioche_core::{Effect, ExtensionStorage, SubRoutineHandle};
use brioche_shell_projection::widget::{
    WIDGET_ERROR, WIDGET_NETWORK_ERROR, WIDGET_STATUS, WIDGET_SUBROUTINE_TIMEOUT,
    WIDGET_SYSTEM_DEGRADED, WIDGET_TEXT_CHUNK,
};
use brioche_shell_projection::{
    AnchorSlot, ContentRenderer, IpcRateLimiter, StreamBatch, StreamBatchEmitter, StreamBuffer,
    SubRoutineAccordionState, SubRoutineManager, UiComposer, UiPerformancePolicy,
    UiPerformanceState, UiRegistry,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn empty_payload() -> Vec<u8> {
    vec![]
}

fn make_text_chunk_effect(trace_id: &str, text: &str) -> Effect {
    Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
        trace_id: trace_id.to_string(),
        text: text.to_string(),
    })
}

// ---------------------------------------------------------------------------
// UiRegistry
// ---------------------------------------------------------------------------

#[test]
fn ui_registry_empty_has_no_mappings() {
    let reg = UiRegistry::new();
    assert!(!reg.contains("anything"));
    assert_eq!(reg.resolve("anything"), None);
    assert_eq!(reg.iter().count(), 0);
}

#[test]
fn ui_registry_register_and_resolve() {
    let mut reg = UiRegistry::new();
    reg.register("my_widget", AnchorSlot::Sidebar);
    assert_eq!(reg.resolve("my_widget"), Some(AnchorSlot::Sidebar));
    assert!(reg.contains("my_widget"));
}

#[test]
fn ui_registry_overwrite_slot() {
    let mut reg = UiRegistry::new();
    reg.register("widget", AnchorSlot::TopBar);
    reg.register("widget", AnchorSlot::StatusBar);
    assert_eq!(reg.resolve("widget"), Some(AnchorSlot::StatusBar));
}

#[test]
fn ui_registry_special_widgets_pre_registered() {
    let reg = UiRegistry::with_special_widgets();
    assert!(reg.contains(WIDGET_SYSTEM_DEGRADED));
    assert!(reg.contains(WIDGET_NETWORK_ERROR));
    assert!(reg.contains(WIDGET_STATUS));
    assert!(reg.contains(WIDGET_ERROR));
    assert!(reg.contains(WIDGET_SUBROUTINE_TIMEOUT));
}

#[test]
fn ui_registry_is_special_widget_detects_known_types() {
    assert!(UiRegistry::is_special_widget(WIDGET_SYSTEM_DEGRADED));
    assert!(UiRegistry::is_special_widget(WIDGET_NETWORK_ERROR));
    assert!(UiRegistry::is_special_widget(WIDGET_STATUS));
    assert!(UiRegistry::is_special_widget(WIDGET_ERROR));
    assert!(UiRegistry::is_special_widget(WIDGET_SUBROUTINE_TIMEOUT));
    assert!(!UiRegistry::is_special_widget("unknown"));
    assert!(!UiRegistry::is_special_widget(WIDGET_TEXT_CHUNK));
}

#[test]
fn ui_registry_iter_is_deterministic() {
    let mut reg = UiRegistry::new();
    reg.register("z_widget", AnchorSlot::TopBar);
    reg.register("a_widget", AnchorSlot::Sidebar);
    reg.register("m_widget", AnchorSlot::StatusBar);

    let keys: Vec<String> = reg.iter().map(|(k, _)| k.clone()).collect();
    assert_eq!(
        keys,
        vec!["a_widget", "m_widget", "z_widget"],
        "BTreeMap iteration must be lexicographically ordered"
    );
}

// ---------------------------------------------------------------------------
// StreamBuffer
// ---------------------------------------------------------------------------

#[test]
fn stream_buffer_append_and_get() {
    let mut buf = StreamBuffer::new();
    buf.append("trace-1", "Hello");
    buf.append("trace-1", " world");
    assert_eq!(buf.get("trace-1"), Some("Hello world"));
}

#[test]
fn stream_buffer_multiple_traces() {
    let mut buf = StreamBuffer::new();
    buf.append("t1", "alpha");
    buf.append("t2", "beta");
    buf.append("t1", "gamma");

    assert_eq!(buf.get("t1"), Some("alphagamma"));
    assert_eq!(buf.get("t2"), Some("beta"));
}

#[test]
fn stream_buffer_flush_removes_trace() {
    let mut buf = StreamBuffer::new();
    buf.append("t", "data");
    assert_eq!(buf.flush("t"), Some("data".to_string()));
    assert_eq!(buf.get("t"), None);
    assert!(buf.is_empty());
}

#[test]
fn stream_buffer_clear_removes_all() {
    let mut buf = StreamBuffer::new();
    buf.append("a", "1");
    buf.append("b", "2");
    buf.clear();
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
}

#[test]
fn stream_buffer_traces_is_deterministic() {
    let mut buf = StreamBuffer::new();
    buf.append("z", "z");
    buf.append("a", "a");
    buf.append("m", "m");

    let keys: Vec<String> = buf.traces().map(|(k, _)| k.clone()).collect();
    assert_eq!(keys, vec!["a", "m", "z"]);
}

#[test]
fn stream_buffer_empty_get_returns_none() {
    let mut buf = StreamBuffer::new();
    assert_eq!(buf.get("missing"), None);
    assert_eq!(buf.flush("missing"), None);
}

// ---------------------------------------------------------------------------
// ContentRenderer
// ---------------------------------------------------------------------------

#[test]
fn content_renderer_consumes_text_chunk() {
    let mut renderer = ContentRenderer::new();
    let effect = make_text_chunk_effect("t1", "hello");
    assert!(renderer.process_effect(&effect));
    assert_eq!(renderer.buffer().get("t1"), Some("hello"));
}

#[test]
fn content_renderer_ignores_non_text_chunk() {
    let mut renderer = ContentRenderer::new();
    let effect = Effect::ForwardToUi(brioche_core::UiWidget::Status("ok".to_string()));
    assert!(!renderer.process_effect(&effect));
    assert!(renderer.buffer().is_empty());
}

#[test]
fn content_renderer_ignores_non_forward_to_ui() {
    let mut renderer = ContentRenderer::new();
    let effect = Effect::SaveSession;
    assert!(!renderer.process_effect(&effect));
}

#[test]
fn content_renderer_appends_multiple_fragments() {
    let mut renderer = ContentRenderer::new();
    renderer.process_effect(&make_text_chunk_effect("t1", "The "));
    renderer.process_effect(&make_text_chunk_effect("t1", "quick "));
    renderer.process_effect(&make_text_chunk_effect("t1", "fox"));
    assert_eq!(renderer.buffer().get("t1"), Some("The quick fox"));
}

#[test]
fn content_renderer_drain_trace_clears_it() {
    let mut renderer = ContentRenderer::new();
    renderer.process_effect(&make_text_chunk_effect("t1", "data"));
    assert_eq!(renderer.drain_trace("t1"), Some("data".to_string()));
    assert_eq!(renderer.buffer().get("t1"), None);
}

#[test]
fn content_renderer_clear_empties_buffer() {
    let mut renderer = ContentRenderer::new();
    renderer.process_effect(&make_text_chunk_effect("t1", "a"));
    renderer.clear();
    assert!(renderer.buffer().is_empty());
}

// ---------------------------------------------------------------------------
// UiComposer
// ---------------------------------------------------------------------------

#[test]
fn ui_composer_default_budget_is_2ms() {
    let composer = UiComposer::new();
    assert_eq!(composer.frame_budget(), 2);
}

#[test]
fn ui_composer_custom_budget() {
    let composer = UiComposer::with_budget(5);
    assert_eq!(composer.frame_budget(), 5);
}

#[test]
fn ui_composer_ignores_non_forward_to_ui() {
    let mut composer = UiComposer::new();
    composer.enqueue(Effect::SaveSession);
    assert_eq!(composer.pending_count(), 0);
}

#[test]
fn ui_composer_enqueues_forward_to_ui() {
    let mut composer = UiComposer::new();
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
        trace_id: "default".to_string(),
        text: String::new(),
    }));
    assert_eq!(composer.pending_count(), 1);
}

#[test]
fn ui_composer_text_chunk_never_dropped() {
    let mut composer = UiComposer::with_budget(0); // impossible budget
    for i in 0..10 {
        composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
            trace_id: "default".to_string(),
            text: i.to_string(),
        }));
    }
    let frame = composer.compose_frame();
    assert_eq!(frame.len(), 10, "text chunks must never be dropped");
}

#[test]
fn ui_composer_cosmetic_dropped_after_3_frames() {
    let mut composer = UiComposer::with_budget(0);
    for _ in 0..4 {
        composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::Custom {
            widget_type: "animation".to_string(),
            payload_json: empty_payload(),
        }));
    }

    // With budget 0, cosmetic effects (cost 3) never fit, so they age each frame.
    // They are dropped when age_frames > 3.
    // Initial age = 0.
    // Frame 1: age 0 -> not dropped -> age becomes 1 (retained)
    // Frame 2: age 1 -> not dropped -> age becomes 2 (retained)
    // Frame 3: age 2 -> not dropped -> age becomes 3 (retained)
    // Frame 4: age 3 -> not dropped -> age becomes 4 (retained)
    // Frame 5: age 4 -> dropped.
    let _ = composer.compose_frame();
    let _ = composer.compose_frame();
    let _ = composer.compose_frame();
    let _ = composer.compose_frame();
    let frame = composer.compose_frame();

    // All cosmetic effects should have been dropped by now.
    assert_eq!(frame.len(), 0);
    assert_eq!(composer.pending_count(), 0);
}

#[test]
fn ui_composer_priority_ordering() {
    let mut composer = UiComposer::with_budget(10);
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::Custom {
        widget_type: "animation".to_string(),
        payload_json: empty_payload(),
    }));
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
        trace_id: "default".to_string(),
        text: String::new(),
    }));
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::Custom {
        widget_type: "focus".to_string(),
        payload_json: empty_payload(),
    }));

    let frame = composer.compose_frame();
    assert_eq!(frame.len(), 3);

    // First effect must be text chunk (highest priority).
    match &frame[0] {
        Effect::ForwardToUi(widget) => {
            assert_eq!(widget.widget_type(), WIDGET_TEXT_CHUNK);
        }
        _ => unreachable!("expected ForwardToUi"),
    }
}

#[test]
fn ui_composer_clear_empties_pending() {
    let mut composer = UiComposer::new();
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
        trace_id: "default".to_string(),
        text: String::new(),
    }));
    composer.clear();
    assert_eq!(composer.pending_count(), 0);
}

// ---------------------------------------------------------------------------
// UiPerformancePolicy
// ---------------------------------------------------------------------------

#[test]
fn ui_performance_state_default_budget() {
    let state = UiPerformanceState::default();
    assert_eq!(state.frame_budget_ms, 2);
}

#[test]
fn ui_performance_state_with_budget_clamps() {
    let state = UiPerformanceState::with_budget(0);
    assert_eq!(state.frame_budget_ms, 1);
    let state = UiPerformanceState::with_budget(20);
    assert_eq!(state.frame_budget_ms, 16);
    let state = UiPerformanceState::with_budget(5);
    assert_eq!(state.frame_budget_ms, 5);
}

#[test]
fn ui_performance_policy_new_has_default_budget() {
    let policy = UiPerformancePolicy::new();
    assert_eq!(policy.composer().frame_budget(), 2);
}

#[test]
fn ui_performance_policy_with_budget() {
    let policy = UiPerformancePolicy::with_budget(8);
    assert_eq!(policy.composer().frame_budget(), 8);
}

#[test]
fn ui_performance_policy_sync_from_storage() {
    let mut ext = ExtensionStorage::new();
    ext.insert(UiPerformanceState::with_budget(7));

    let mut policy = UiPerformancePolicy::new();
    policy.sync_from_storage(&mut ext);
    assert_eq!(policy.composer().frame_budget(), 7);
}

#[test]
fn ui_performance_policy_sync_inserts_default_if_absent() {
    let mut ext = ExtensionStorage::new();
    let mut policy = UiPerformancePolicy::new();
    policy.sync_from_storage(&mut ext);
    // Default state has frame_budget_ms = 2.
    assert_eq!(policy.composer().frame_budget(), 2);
}

#[test]
fn ui_performance_policy_process_effects_separates_ui_and_non_ui() {
    let mut policy = UiPerformancePolicy::new();
    let effects = vec![
        Effect::SaveSession,
        Effect::ForwardToUi(brioche_core::UiWidget::TextChunk {
            trace_id: "t1".to_string(),
            text: "hi".to_string(),
        }),
        Effect::TriggerGc,
    ];

    let frame = policy.process_effects(effects);
    // Non-UI effects pass through first.
    assert!(frame.iter().any(|e| matches!(e, Effect::SaveSession)));
    assert!(frame.iter().any(|e| matches!(e, Effect::TriggerGc)));
    assert!(frame.iter().any(|e| matches!(e, Effect::ForwardToUi(_))));
}

#[test]
fn ui_performance_policy_set_frame_budget_directly() {
    let mut policy = UiPerformancePolicy::new();
    policy.set_frame_budget(12);
    assert_eq!(policy.composer().frame_budget(), 12);
}

#[test]
fn ui_performance_policy_has_pending_tracks_composer() {
    let mut policy = UiPerformancePolicy::new();
    assert!(!policy.has_pending());
    policy.process_effects(vec![Effect::ForwardToUi(brioche_core::UiWidget::Custom {
        widget_type: "animation".to_string(),
        payload_json: empty_payload(),
    })]);
    assert!(policy.has_pending());
}

// ---------------------------------------------------------------------------
// Cross-cutting: UiRegistry + UiComposer + ContentRenderer integration
// ---------------------------------------------------------------------------

#[test]
fn end_to_end_stream_accumulation_and_composition() {
    let mut renderer = ContentRenderer::new();
    let mut policy = UiPerformancePolicy::new();

    // Simulate kernel emitting text chunks.
    let chunks = vec![
        make_text_chunk_effect("main", "The "),
        make_text_chunk_effect("main", "quick "),
        make_text_chunk_effect("main", "brown "),
        make_text_chunk_effect("main", "fox"),
    ];

    for chunk in &chunks {
        renderer.process_effect(chunk);
    }

    assert_eq!(renderer.buffer().get("main"), Some("The quick brown fox"));

    // Simulate the shell forwarding effects through the policy.
    let frame = policy.process_effects(chunks);
    assert_eq!(frame.len(), 4, "all text chunks should be in frame");
}

#[test]
fn special_widget_maps_to_semantic_priority() {
    let mut composer = UiComposer::with_budget(10);
    composer.enqueue(Effect::ForwardToUi(
        brioche_core::UiWidget::SystemDegraded {
            plugin: "test".to_string(),
        },
    ));
    composer.enqueue(Effect::ForwardToUi(brioche_core::UiWidget::Custom {
        widget_type: "animation".to_string(),
        payload_json: empty_payload(),
    }));

    let frame = composer.compose_frame();
    assert_eq!(frame.len(), 2);

    // Special governance widgets are semantic priority; cosmetic is lowest.
    // Both should fit within budget 10 (semantic cost 2 + cosmetic cost 3 = 5 <= 10).
    match &frame[0] {
        Effect::ForwardToUi(widget) => {
            assert_eq!(widget.widget_type(), WIDGET_SYSTEM_DEGRADED);
        }
        _ => unreachable!("expected ForwardToUi"),
    }
}

// ---------------------------------------------------------------------------
// Sprint 15: IPC Rate Limiter
// ---------------------------------------------------------------------------

#[test]
fn ipc_rate_limiter_clone_shares_state() {
    let limiter = IpcRateLimiter::new(10_000);
    let clone = limiter.clone();

    // First emit on original succeeds.
    assert!(limiter.try_emit());
    // Clone sees the same timestamp and blocks.
    assert!(!clone.try_emit());
}

// ---------------------------------------------------------------------------
// Sprint 15: Stream Batch
// ---------------------------------------------------------------------------

#[test]
fn stream_batch_messagepack_roundtrip() {
    let mut batch = StreamBatch::new();
    batch.append("trace-1", "Hello");
    batch.append("trace-2", "world");

    let bytes = match batch.to_messagepack() {
        Ok(v) => v,
        Err(e) => unreachable!("serialize failed: {}", e),
    };
    let decoded: StreamBatch = match rmp_serde::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => unreachable!("deserialize failed: {}", e),
    };
    assert_eq!(decoded, batch);
}

#[test]
fn stream_batch_emitter_integration() {
    let limiter = IpcRateLimiter::new(0); // always allows after sentinel
    let mut emitter = StreamBatchEmitter::new(limiter);

    emitter.accumulate("main", "The ");
    emitter.accumulate("main", "quick ");
    emitter.accumulate("side", "fox");

    let bytes = match emitter.try_emit() {
        Some(v) => v,
        None => unreachable!("should emit"),
    };
    let decoded: StreamBatch = match rmp_serde::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => unreachable!("deserialize failed: {}", e),
    };
    assert_eq!(decoded.traces.get("main"), Some(&"The quick ".to_string()));
    assert_eq!(decoded.traces.get("side"), Some(&"fox".to_string()));
}

// ---------------------------------------------------------------------------
// Sprint 15: SubRoutineManager
// ---------------------------------------------------------------------------

#[test]
fn subroutine_manager_end_to_end_lifecycle() {
    let mut mgr = SubRoutineManager::new();
    let handle = SubRoutineHandle::new("sub-1");

    // Initial: not tracked → begin_load creates Loading.
    let state = mgr.begin_load(handle.clone());
    assert_eq!(state.accordion, SubRoutineAccordionState::Loading);

    // Kernel confirms restoration.
    let state = mgr.mark_loaded(handle.clone());
    assert_eq!(state.accordion, SubRoutineAccordionState::Loaded);

    // Error transition.
    let state = match mgr.mark_error(&handle) {
        Some(v) => v,
        None => unreachable!("handle must exist"),
    };
    assert_eq!(state.accordion, SubRoutineAccordionState::Error);

    // Timeout transition.
    let state = match mgr.mark_timeout(&handle) {
        Some(v) => v,
        None => unreachable!("handle must exist"),
    };
    assert_eq!(state.accordion, SubRoutineAccordionState::Timeout);

    // Cleanup.
    assert!(mgr.remove(&handle).is_some());
    assert!(mgr.is_empty());
}

#[test]
fn subroutine_manager_renderer_isolation_integration() {
    let mut mgr = SubRoutineManager::new();
    let h1 = SubRoutineHandle::new("sub-a");
    let h2 = SubRoutineHandle::new("sub-b");

    mgr.begin_load(h1.clone());
    mgr.begin_load(h2.clone());

    // Simulate streaming into sub-a.
    let effect = make_text_chunk_effect("trace", "alpha");
    let state_h1 = match mgr.get_mut(&h1) {
        Some(v) => v,
        None => unreachable!("h1 must exist"),
    };
    let renderer = &mut state_h1.renderer;
    assert!(renderer.process_effect(&effect));

    // sub-b remains untouched.
    let state_h2 = match mgr.get(&h2) {
        Some(v) => v,
        None => unreachable!("h2 must exist"),
    };
    assert!(state_h2.renderer.buffer().is_empty());
    let state_h1 = match mgr.get(&h1) {
        Some(v) => v,
        None => unreachable!("h1 must exist"),
    };
    assert_eq!(state_h1.renderer.buffer().get("trace"), Some("alpha"));
}
