//! # {{project-name}}
//!
//! A Brioche plugin generated from the official `brioche-plugin-template`.
//!
//! ## Overview
//! This plugin demonstrates the minimal structure required for a Brioche
//! plugin using the `brioche-plugin-kit` SDK.
//!
//! ## Invariants
//! - I-Eco-ExtensionOverMod: Plugin is pure policy, never modifies mechanism.
//! - I-Eco-OrderedCollections: State uses `BTreeMap` for determinism.
//!
//! Refs: SPECS.md §Book IV

#![deny(clippy::unwrap_used, clippy::expect_used)]

use brioche_plugin_kit::prelude::*;

/// Plugin state stored in `ExtensionStorage`.
///
/// All persisted state must derive `BriocheExtensionType` and use ordered
/// collections (`BTreeMap`, `BTreeSet`, `IndexMap`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, BriocheExtensionType)]
pub struct {{ProjectName}}State {
    /// Count of processed inputs (deterministic counter).
    pub counter: u64,
}

/// The plugin struct.
///
/// Stateless container; all mutable data lives in `ExtensionStorage`.
pub struct {{ProjectName}};

#[brioche_plugin(name = "{{project-name}}", capabilities = "ON_INPUT")]
impl BriochePlugin for {{ProjectName}} {
    #[hook(on_input)]
    fn on_input(
        &self,
        input: &EngineInput,
        ext: &mut ExtensionStorage,
    ) -> PluginResult<PolicyDecision> {
        let state = ext.get_or_insert_default::<{{ProjectName}}State>();
        state.counter += 1;

        match input {
            EngineInput::UserMessage(msg) => {
                println!("[{{project-name}}] received user message: {msg}");
            }
            _ => {}
        }

        Ok(PolicyDecision::Allow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brioche_plugin_kit::MockEngine;

    #[test]
    fn plugin_counts_inputs() {
        let mut mock = match MockEngine::new() {
            Ok(m) => m,
            Err(err) => {
                assert_eq!(1, 0, "mock engine failed: {}", err);
                return;
            }
        };
        let _effects = mock.transition(EngineInput::UserMessage("hello".into()));

        let state = mock
            .session()
            .extensions
            .get_or_insert_default::<{{ProjectName}}State>();
        assert_eq!(state.counter, 1);
    }
}
