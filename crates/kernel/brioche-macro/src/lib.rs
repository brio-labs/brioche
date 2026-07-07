//! Book I — The Core Book: Procedural macros for Brioche extension types and plugins.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verification of extension types.
//! - I-Eco-OrderedCollections: Persisted extension state rejects unordered collections.
//!
//! The `#[derive(BriocheExtensionType)]` macro performs compile-time
//! verification of extension types and generates required trait
//! implementations. Attribute macros support plugin authoring and CPU offload
//! task declaration while keeping proc-macro entrypoints at the crate root.
//!
//! Refs: docs/SPECS.md §3.2

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod extension_type;
mod offload_task;
mod plugin;

/// Derive macro for `BriocheExtensionType`.
///
/// Generates the sealed trait impl, VTable, and compile-time checks
/// for `HashMap`/`HashSet` bans and deterministic `Vec`/`IndexMap` ordering.
///
/// Supported attributes:
/// - `#[brioche(critical_state)]` — always snapshot, exempt from budget.
/// - `#[brioche(no_snapshot)]` — rollback forbidden for this type.
/// - `#[brioche(incremental_snapshot)]` — use incremental COW.
/// - `#[brioche(ext_id = "...")]` — override the auto-generated EXT_ID.
/// - `#[brioche(deterministic_order)]` — certify `Vec`/`IndexMap` field ordering.
/// - `#[brioche(nested_carrier)]` — field contains a nested `BriocheExtensionType`
///   carrier; a const assertion requires the type to implement the trait.
///
/// # Complexity
/// Compile-time only. Field scanning is O(total type nodes) in the derive input;
/// generated code is otherwise constant-time to expand.
///
/// # Panics
/// This proc-macro never panics at runtime. Invalid input is surfaced to the
/// compiler via `compile_error!` tokens or `syn::Error`.
///
/// Refs: I-Core-ExtensionType
#[proc_macro_derive(BriocheExtensionType, attributes(brioche))]
pub fn derive_brioche_extension_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    extension_type::expand_brioche_extension_type(input).into()
}

/// `#[brioche_plugin(name = "...", capabilities = "ON_INPUT")]`
///
/// Attribute macro applied to an atomic capability impl block. It injects
/// `fn name()` and optionally `fn priority()` into the impl. The capability
/// argument is accepted for source compatibility and documentation, but route
/// selection is now determined by the trait being implemented.
///
/// Helper attributes `#[hook(...)]` on methods are stripped by this macro.
///
/// # Example
/// ```ignore
/// #[brioche_plugin(name = "my_plugin", capabilities = "ON_INPUT")]
/// impl OnInput for MyPlugin {
///     #[hook(on_input)]
///     fn on_input(&self, input: &EngineInput, ext: &mut ExtensionStorage) -> PluginResult<PolicyDecision> {
///         Ok(PolicyDecision::Allow)
///     }
/// }
/// ```
/// Refs: docs/SPECS.md §Book I
#[proc_macro_attribute]
pub fn brioche_plugin(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as plugin::PluginArgs);
    let item_impl = parse_macro_input!(input as syn::ItemImpl);
    plugin::expand_brioche_plugin(args, item_impl).into()
}

/// `#[hook(on_input)]` — helper attribute consumed by `#[brioche_plugin]`.
///
/// When used outside of a `#[brioche_plugin]` impl block, this macro
/// is a no-op (it passes the item through unchanged). This allows
/// IDE syntax highlighting and macro expansion to work incrementally.
/// Refs: docs/SPECS.md §Book I
#[proc_macro_attribute]
pub fn hook(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Pass through unchanged. The outer `#[brioche_plugin]` macro strips
    // this attribute when it processes the impl block.
    input
}

/// `#[brioche_offload_task]` — wraps a function for CPU-task offloading.
///
/// Generates a companion module with `effect(task_id, input) -> Effect`
/// and (de)serialization helpers.
///
/// # Requirements
/// - The function must take exactly one argument (the input payload).
/// - The argument and return types must implement `Serialize` and `DeserializeOwned`.
///
/// # Example
/// ```ignore
/// #[brioche_offload_task]
/// fn heavy_computation(input: Vec<u8>) -> Vec<u8> {
///     // CPU-intensive work
///     input
/// }
/// ```
/// Refs: docs/SPECS.md §Book I
#[proc_macro_attribute]
pub fn brioche_offload_task(_args: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as syn::ItemFn);
    offload_task::expand_brioche_offload_task(func).into()
}
