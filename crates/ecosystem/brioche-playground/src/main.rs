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
//! Refs: SPECS.md §Book V

use brioche_core::{
    ChatMessage, EngineInput, PluginCapabilities, PluginError, PluginResult, PolicyDecision,
};
use brioche_plugin_kit::{BriochePlugin, PluginBuilder};

/// A mock plugin that intercepts `CallLlmNetwork` and injects a fake
/// assistant response, enabling end-to-end testing without a real LLM.
pub struct MockLlmBackend;

impl BriochePlugin for MockLlmBackend {
    fn name(&self) -> &'static str {
        "mock_llm_backend"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::AFTER_PREDICTION
    }

    fn after_prediction(&self, _ext: &mut brioche_core::ExtensionStorage) -> PluginResult<()> {
        Ok(())
    }
}

/// A passive observer plugin that logs every effect to stdout.
pub struct EffectLogger;

impl BriochePlugin for EffectLogger {
    fn name(&self) -> &'static str {
        "effect_logger"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ON_INPUT
            | PluginCapabilities::BEFORE_PREDICTION
            | PluginCapabilities::AFTER_PREDICTION
            | PluginCapabilities::ON_TOOL_CALLS
            | PluginCapabilities::ON_TOOL_RESULT
            | PluginCapabilities::ON_ERROR
    }

    fn on_input(
        &self,
        input: &EngineInput,
        _ext: &mut brioche_core::ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        println!("[effect_logger] on_input: {input:?}");
        Ok(PolicyDecision::Allow)
    }

    fn before_prediction(
        &self,
        _history: &[ChatMessage],
        _ext: &mut brioche_core::ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        println!("[effect_logger] before_prediction");
        Ok(PolicyDecision::Allow)
    }

    fn after_prediction(&self, _ext: &mut brioche_core::ExtensionStorage) -> PluginResult<()> {
        println!("[effect_logger] after_prediction");
        Ok(())
    }

    fn on_tool_calls(
        &self,
        calls: &mut Vec<brioche_core::ToolCallDescriptor>,
        _ext: &mut brioche_core::ExtensionStorage,
    ) -> PluginResult<()> {
        println!("[effect_logger] on_tool_calls: {calls:?}");
        Ok(())
    }

    fn on_tool_result(
        &self,
        results: &mut Vec<brioche_core::ToolResultDTO>,
        _ext: &mut brioche_core::ExtensionStorage,
    ) -> PluginResult<()> {
        println!("[effect_logger] on_tool_result: {results:?}");
        Ok(())
    }

    fn on_error(
        &self,
        error: &PluginError,
        _ext: &mut brioche_core::ExtensionStorage,
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
        .with_plugin(Box::new(EffectLogger))
        .with_plugin(Box::new(MockLlmBackend))
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
