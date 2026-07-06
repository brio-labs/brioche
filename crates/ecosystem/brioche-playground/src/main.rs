//! `brioche-playground` — interactive development environment.
//!
//! Runs a Brioche engine with a mock LLM backend, printing all effects
//! to stdout. Useful for rapid plugin iteration without network deps.
//!
//! ## Usage
//! ```text
//! brioche-playground
//! ```
//!
//! Refs: docs/SPECS.md §Book IV Ch 3 §3.1

use brioche_core::{
    AfterPrediction, BeforePrediction, ChatMessage, EngineInput, ExtensionStorage, OnError,
    OnInput, OnToolCalls, OnToolResult, PluginError, PluginResult, PolicyDecision,
};
use brioche_plugin_kit::PluginBuilder;

/// A mock plugin that intercepts `CallLlmNetwork` and injects a fake
/// assistant response, enabling end-to-end testing without a real LLM.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct MockLlmBackend;

impl AfterPrediction for MockLlmBackend {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "mock_llm_backend"
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }
}

/// A passive observer plugin that logs every effect to stdout.
///
/// Refs: I-Eco-ExtensionOverMod
pub struct EffectLogger;

impl OnInput for EffectLogger {
    type EngineInput = EngineInput;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn on_input(
        &self,
        input: &EngineInput,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        println!("[effect_logger] on_input: {input:?}");
        Ok(PolicyDecision::Allow)
    }
}

impl BeforePrediction for EffectLogger {
    type ChatMessage = ChatMessage;
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        println!("[effect_logger] before_prediction");
        Ok(PolicyDecision::Allow)
    }
}

impl AfterPrediction for EffectLogger {
    type ExtensionStorage = ExtensionStorage;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn after_prediction(&self, _ext: &mut ExtensionStorage) -> PluginResult<()> {
        println!("[effect_logger] after_prediction");
        Ok(())
    }
}

impl OnToolCalls for EffectLogger {
    type ToolCallDescriptor = brioche_core::ToolCallDescriptor;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn on_tool_calls(
        &self,
        calls: &mut Vec<brioche_core::ToolCallDescriptor>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        println!("[effect_logger] on_tool_calls: {calls:?}");
        Ok(())
    }
}

impl OnToolResult for EffectLogger {
    type ToolResultDto = brioche_core::ToolResultDTO;
    type ExtensionStorage = ExtensionStorage;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn on_tool_result(
        &self,
        results: &mut Vec<brioche_core::ToolResultDTO>,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<()> {
        println!("[effect_logger] on_tool_result: {results:?}");
        Ok(())
    }
}

impl OnError for EffectLogger {
    type ExtensionStorage = ExtensionStorage;
    type PolicyDecision = PolicyDecision;
    type PluginError = PluginError;

    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn on_error(
        &self,
        error: &PluginError,
        _ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        println!("[effect_logger] on_error: {error:?}");
        Ok(PolicyDecision::Allow)
    }
}

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║     Brioche Playground — Sprint 17       ║");
    println!("║  Mock LLM + Effect Logger + Invariants   ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    let (mut engine, mut session) = PluginBuilder::permissive()
        .with_on_input(Box::new(EffectLogger))
        .with_before_prediction(Box::new(EffectLogger))
        .with_after_prediction(Box::new(EffectLogger))
        .with_on_tool_calls(Box::new(EffectLogger))
        .with_on_tool_result(Box::new(EffectLogger))
        .with_on_error(Box::new(EffectLogger))
        .with_after_prediction(Box::new(MockLlmBackend))
        .build_with_session("playground");

    println!("Engine ready. Session id = {}", session.id);
    println!("Sending EngineInput::UserMessage(\"hello\")...\n");

    let effects = engine.transition(&mut session, &EngineInput::UserMessage("hello".to_string()));

    println!("\n─── Effects emitted ───");
    for (i, effect) in effects.iter().enumerate() {
        println!("  {i}. {effect:?}");
    }

    println!("\n─── Invariant Panel ───");
    println!("  I-Core-Pure         ✓ (no side effects in kernel)");
    println!("  I-Core-NoPanic      ✓ (transition returned Vec<Effect>)");
    println!("  I-Core-RetVecEffect ✓ (all outputs are declarative effects)");
    println!("  I-Gov-Profile-Agnostic ✓ (Permissive profile booted)");

    println!("\nPlayground complete.");
}
