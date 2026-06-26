//! Book I — The Core Book: Procedural macros for `BriocheExtensionType`.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verification of extension types.
//!
//! The `#[derive(BriocheExtensionType)]` macro performs compile-time
//! verification of extension types and generates required trait
//! implementations.
//!
//! Refs: docs/SPECS.md §3.2

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataStruct, DeriveInput, Fields, Type, parse_macro_input};

/// Errors raised by the derive macro.
#[derive(Debug)]
enum DeriveError {
    HashMap {
        span: Span,
        field_name: String,
    },
    HashSet {
        span: Span,
        field_name: String,
    },
    NonOrderedMap {
        span: Span,
        field_name: String,
        type_name: String,
    },
    NonOrderedSet {
        span: Span,
        field_name: String,
        type_name: String,
    },
    UiType {
        span: Span,
        field_name: String,
    },
    UndeterminedVec {
        span: Span,
        field_name: String,
    },
}

/// Parsed `#[brioche(...)]` attributes on a struct/enum.
#[derive(Debug, Default)]
struct BriocheAttrs {
    critical_state: bool,
    no_snapshot: bool,
    incremental_snapshot: bool,
    ext_id: Option<String>,
}

impl BriocheAttrs {
    fn snapshot_strategy(&self) -> proc_macro2::TokenStream {
        if self.critical_state {
            quote!(::brioche_core::SnapshotStrategy::CriticalFullClone)
        } else if self.no_snapshot {
            quote!(::brioche_core::SnapshotStrategy::NoSnapshot)
        } else if self.incremental_snapshot {
            quote!(::brioche_core::SnapshotStrategy::Incremental)
        } else {
            quote!(::brioche_core::SnapshotStrategy::FullClone)
        }
    }
}

/// Parse `#[brioche(...)]` attributes. Returns an error for unknown
/// nested meta items or malformed syntax.
fn parse_brioche_attrs(attrs: &[Attribute]) -> Result<BriocheAttrs, syn::Error> {
    let mut result = BriocheAttrs::default();
    for attr in attrs {
        if !attr.path().is_ident("brioche") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("critical_state") {
                result.critical_state = true;
            } else if meta.path.is_ident("no_snapshot") {
                result.no_snapshot = true;
            } else if meta.path.is_ident("incremental_snapshot") {
                result.incremental_snapshot = true;
            } else if meta.path.is_ident("ext_id") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                result.ext_id = Some(lit.value());
            } else {
                let path = match meta.path.get_ident() {
                    Some(ident) => ident.to_string(),
                    None => "unknown".to_string(),
                };
                return Err(meta.error(format!(
                    "unknown brioche attribute `{}`. Expected one of: critical_state, no_snapshot, incremental_snapshot, ext_id",
                    path
                )));
            }
            Ok(())
        })?;
    }
    Ok(result)
}

/// Recursively scan a type for banned collections (`HashMap`, `HashSet`)
/// and UI types.
fn scan_type(ty: &Type, errors: &mut Vec<DeriveError>, field_name: &str) {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();

            let has_hashmap = segments.iter().any(|s| s == "HashMap");
            let has_hashset = segments.iter().any(|s| s == "HashSet");

            // Check for banned collections by exact segment name.
            if has_hashmap {
                errors.push(DeriveError::HashMap {
                    span: type_path.span(),
                    field_name: field_name.to_string(),
                });
            }
            if has_hashset {
                errors.push(DeriveError::HashSet {
                    span: type_path.span(),
                    field_name: field_name.to_string(),
                });
            }

            // Check last segment for aliases of non-ordered maps/sets.
            if let Some(last) = segments.last() {
                let allowed_maps: [&str; 2] = ["BTreeMap", "IndexMap"];
                let allowed_sets: [&str; 2] = ["BTreeSet", "IndexSet"];
                if !has_hashmap && last.ends_with("Map") && !allowed_maps.contains(&last.as_str()) {
                    errors.push(DeriveError::NonOrderedMap {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                        type_name: last.clone(),
                    });
                }
                if !has_hashset && last.ends_with("Set") && !allowed_sets.contains(&last.as_str()) {
                    errors.push(DeriveError::NonOrderedSet {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                        type_name: last.clone(),
                    });
                }

                // UI type detection.
                let ui_keywords = ["tauri", "vue", "dom", "web_sys", "js_sys"];
                if ui_keywords.iter().any(|k| last.to_lowercase().contains(k)) {
                    errors.push(DeriveError::UiType {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                    });
                }
            }

            // Recurse into generic arguments.
            if let Some(seg) = type_path.path.segments.last() {
                match &seg.arguments {
                    syn::PathArguments::AngleBracketed(args) => {
                        for arg in &args.args {
                            if let syn::GenericArgument::Type(inner) = arg {
                                scan_type(inner, errors, field_name);
                            }
                        }
                    }
                    syn::PathArguments::Parenthesized(args) => {
                        for inp in &args.inputs {
                            scan_type(inp, errors, field_name);
                        }
                        if let syn::ReturnType::Type(_, output) = &args.output {
                            scan_type(output, errors, field_name);
                        }
                    }
                    _ => {}
                }
            }
        }
        Type::Array(arr) => scan_type(&arr.elem, errors, field_name),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                scan_type(elem, errors, field_name);
            }
        }
        Type::Reference(rf) => scan_type(&rf.elem, errors, field_name),
        Type::Paren(paren) => scan_type(&paren.elem, errors, field_name),
        Type::Slice(slice) => scan_type(&slice.elem, errors, field_name),
        Type::BareFn(bare) => {
            for inp in &bare.inputs {
                scan_type(&inp.ty, errors, field_name);
            }
            if let syn::ReturnType::Type(_, output) = &bare.output {
                scan_type(output, errors, field_name);
            }
        }
        Type::ImplTrait(impl_trait) => {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    for seg in &trait_bound.path.segments {
                        if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                            for arg in &args.args {
                                if let syn::GenericArgument::Type(inner) = arg {
                                    scan_type(inner, errors, field_name);
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Recursively check whether a type contains `Vec`.
///
/// Returns `true` if the type path (or any nested generic) contains a
/// segment named exactly `Vec`. This catches `Vec<T>`,
/// `std::vec::Vec<T>`, and nested usages like `BTreeMap<K, Vec<V>>`.
fn type_contains_vec(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.iter().any(|s| s == "Vec") {
                return true;
            }
            if let Some(seg) = type_path.path.segments.last()
                && let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner) = arg
                            && type_contains_vec(inner) {
                                return true;
                            }
                    }
                }
            false
        }
        Type::Array(arr) => type_contains_vec(&arr.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_vec),
        Type::Reference(rf) => type_contains_vec(&rf.elem),
        Type::Paren(paren) => type_contains_vec(&paren.elem),
        Type::Slice(slice) => type_contains_vec(&slice.elem),
        Type::BareFn(bare) => {
            bare.inputs.iter().any(|inp| type_contains_vec(&inp.ty))
                || matches!(&bare.output, syn::ReturnType::Type(_, output) if type_contains_vec(output))
        }
        Type::ImplTrait(impl_trait) => impl_trait.bounds.iter().any(|bound| {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                trait_bound.path.segments.iter().any(|seg| {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Type(inner) if type_contains_vec(inner))
                        })
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        }),
        _ => false,
    }
}

/// Check whether a field carries `#[brioche(deterministic_order)]`.
fn field_has_deterministic_order(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("brioche") {
            continue;
        }
        let mut found = false;
        // Ignore parse errors — malformed attributes are handled elsewhere.
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("deterministic_order") {
                found = true;
            }
            Ok(())
        });
        if found {
            return true;
        }
    }
    false
}

/// Scan all fields of a struct/enum for banned types and undetermined
/// `Vec` fields.
fn scan_fields(fields: &Fields, errors: &mut Vec<DeriveError>) {
    match fields {
        Fields::Named(named) => {
            for f in &named.named {
                let name = match f.ident.as_ref() {
                    Some(ident) => ident.to_string(),
                    None => "_".to_string(),
                };
                scan_type(&f.ty, errors, &name);
                if type_contains_vec(&f.ty) && !field_has_deterministic_order(&f.attrs) {
                    errors.push(DeriveError::UndeterminedVec {
                        span: f.ty.span(),
                        field_name: name,
                    });
                }
            }
        }
        Fields::Unnamed(unnamed) => {
            for (i, f) in unnamed.unnamed.iter().enumerate() {
                let name = format!("_{}", i);
                scan_type(&f.ty, errors, &name);
                if type_contains_vec(&f.ty) && !field_has_deterministic_order(&f.attrs) {
                    errors.push(DeriveError::UndeterminedVec {
                        span: f.ty.span(),
                        field_name: name,
                    });
                }
            }
        }
        Fields::Unit => {}
    }
}

/// Derive macro for `BriocheExtensionType`.
///
/// Generates the sealed trait impl, VTable, and compile-time checks
/// for `HashMap`/`HashSet` bans and deterministic `Vec` ordering.
///
/// Supported attributes:
/// - `#[brioche(critical_state)]` — always snapshot, exempt from budget.
/// - `#[brioche(no_snapshot)]` — rollback forbidden for this type.
/// - `#[brioche(incremental_snapshot)]` — use incremental COW.
/// - `#[brioche(ext_id = "...")]` — override the auto-generated EXT_ID.
/// - `#[brioche(deterministic_order)]` — certify `Vec` field ordering.
///
/// Refs: I-Core-ExtensionType
#[proc_macro_derive(BriocheExtensionType, attributes(brioche))]
pub fn derive_brioche_extension_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = match parse_brioche_attrs(&input.attrs) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Build EXT_ID from optional attribute or auto-generate.
    let ext_id_tokens = match &attrs.ext_id {
        Some(id) => {
            let lit = proc_macro2::Literal::string(id);
            quote! { #lit }
        }
        None => {
            quote! {
                {
                    const ID: &str = concat!(module_path!(), "::", stringify!(#name));
                    ID
                }
            }
        }
    };

    let snapshot_strategy = attrs.snapshot_strategy();

    // Collect validation errors.
    let mut errors: Vec<DeriveError> = Vec::new();

    match &input.data {
        Data::Struct(DataStruct { fields, .. }) => {
            scan_fields(fields, &mut errors);
        }
        Data::Enum(data_enum) => {
            for variant in &data_enum.variants {
                scan_fields(&variant.fields, &mut errors);
            }
        }
        Data::Union(data_union) => {
            scan_fields(&Fields::Named(data_union.fields.clone()), &mut errors);
        }
    }

    // Emit error tokens.
    let error_tokens: Vec<proc_macro2::TokenStream> = errors
        .iter()
        .map(|e| match e {
            DeriveError::HashMap { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses HashMap. HashMap is prohibited in BriocheExtensionType persisted state. Use BTreeMap or IndexMap instead."));
                }
            }
            DeriveError::HashSet { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses HashSet. HashSet is prohibited in BriocheExtensionType persisted state. Use BTreeSet instead."));
                }
            }
            DeriveError::NonOrderedMap { span, field_name, type_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses non-ordered map type `", #type_name, "`. Only ordered maps (BTreeMap, IndexMap) are allowed in BriocheExtensionType persisted state."));
                }
            }
            DeriveError::NonOrderedSet { span, field_name, type_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses non-ordered set type `", #type_name, "`. Only ordered sets (BTreeSet, IndexSet) are allowed in BriocheExtensionType persisted state."));
                }
            }
            DeriveError::UiType { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` appears to contain a UI type. UI types are prohibited in BriocheExtensionType."));
                }
            }
            DeriveError::UndeterminedVec { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses Vec without #[brioche(deterministic_order)]. Persisted Vec fields must have deterministic insertion order. Use #[brioche(deterministic_order)] to certify determinism, or replace with BTreeMap/IndexMap."));
                }
            }
        })
        .collect();

    let expanded = if errors.is_empty() {
        quote! {
            #(#error_tokens)*

            // Sealed trait implementation — required by BriocheExtensionType.
            // This impl is only valid here because brioche-macro is part of
            // the SDK. Manual impls by users are prevented by the sealed pattern.
            impl #impl_generics ::brioche_core::extension::__private::Sealed for #name #ty_generics #where_clause {}

            impl #impl_generics ::brioche_core::BriocheExtensionType for #name #ty_generics #where_clause {
                const EXT_ID: &'static str = #ext_id_tokens;

                fn estimated_weight_bytes(&self) -> usize {
                    // Pragmatic estimate: size_of_val. The VTable function
                    // refines this via binary serialization when available.
                    ::core::mem::size_of_val(self)
                }

                fn snapshot_strategy() -> ::brioche_core::SnapshotStrategy {
                    #snapshot_strategy
                }

                fn build_vtable() -> ::brioche_core::ExtVTable
                where
                    Self: Sized,
                {
                    fn serialize(any: &dyn ::core::any::Any) -> ::std::vec::Vec<u8> {
                        if let Some(this) = any.downcast_ref::<#name>() {
                            match ::brioche_core::postcard::to_stdvec(this) {
                                Ok(v) => v,
                                Err(_) => ::std::vec::Vec::new(),
                            }
                        } else {
                            ::std::vec::Vec::new()
                        }
                    }
                    fn deserialize(bytes: &[u8]) -> ::core::result::Result<::std::boxed::Box<dyn ::core::any::Any + Send + Sync>, ::std::string::String> {
                        ::brioche_core::postcard::from_bytes::<#name>(bytes)
                            .map(|v| ::std::boxed::Box::new(v) as ::std::boxed::Box<dyn ::core::any::Any + Send + Sync>)
                            .map_err(|_| ::std::string::String::from("deserialize failed"))
                    }
                    fn clone_box(any: &dyn ::core::any::Any) -> ::std::boxed::Box<dyn ::core::any::Any + Send + Sync> {
                        if let Some(this) = any.downcast_ref::<#name>() {
                            ::std::boxed::Box::new(this.clone())
                        } else {
                            ::std::boxed::Box::new(<#name as ::core::default::Default>::default())
                        }
                    }
                    fn estimated_weight_bytes(any: &dyn ::core::any::Any) -> usize {
                        if let Some(this) = any.downcast_ref::<#name>() {
                            match ::brioche_core::postcard::to_stdvec(this) {
                                Ok(v) => v.len(),
                                Err(_) => ::core::mem::size_of_val(this),
                            }
                        } else {
                            0
                        }
                    }
                    fn default_construct() -> ::std::boxed::Box<dyn ::core::any::Any + Send + Sync> {
                        ::std::boxed::Box::new(<#name as ::core::default::Default>::default())
                    }

                    ::brioche_core::ExtVTable {
                        ext_id: <#name as ::brioche_core::BriocheExtensionType>::EXT_ID,
                        serialize,
                        deserialize,
                        clone_box,
                        estimated_weight_bytes,
                        snapshot_strategy: <#name as ::brioche_core::BriocheExtensionType>::snapshot_strategy(),
                        default_construct,
                    }
                }
            }
        }
    } else {
        quote! {
            #(#error_tokens)*
        }
    };

    TokenStream::from(expanded)
}

// ---------------------------------------------------------------------------
// Plugin authoring macros — Sprint 17
// ---------------------------------------------------------------------------

use syn::parse::{Parse, ParseStream};
use syn::{ImplItem, ItemImpl, LitStr, Token};

/// Parsed arguments for `#[brioche_plugin(...)]`.
struct PluginArgs {
    name: LitStr,
    capabilities: Vec<String>,
    priority: Option<i16>,
}

impl Parse for PluginArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut name: Option<LitStr> = None;
        let mut capabilities: Vec<String> = Vec::new();
        let mut priority: Option<i16> = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if ident == "name" {
                name = Some(input.parse()?);
            } else if ident == "capabilities" {
                let lit: LitStr = input.parse()?;
                capabilities = lit
                    .value()
                    .split('|')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            } else if ident == "priority" {
                let lit: syn::LitInt = input.parse()?;
                priority = Some(lit.base10_parse()?);
            } else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "unknown argument; expected `name`, `capabilities`, or `priority`",
                ));
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let name = name.ok_or_else(|| {
            syn::Error::new(input.span(), "`name` is required in #[brioche_plugin(...)]")
        })?;

        Ok(PluginArgs {
            name,
            capabilities,
            priority,
        })
    }
}

/// Convert a capability string (e.g. `"ON_INPUT"`) into a token stream
/// referencing the corresponding `PluginCapabilities` constant.
fn capability_to_tokens(cap: &str) -> proc_macro2::TokenStream {
    match cap {
        "NONE" => quote!(::brioche_core::PluginCapabilities::NONE),
        "ON_INPUT" => quote!(::brioche_core::PluginCapabilities::ON_INPUT),
        "BEFORE_PREDICTION" => quote!(::brioche_core::PluginCapabilities::BEFORE_PREDICTION),
        "ON_STREAM_EVENT" => quote!(::brioche_core::PluginCapabilities::ON_STREAM_EVENT),
        "AFTER_PREDICTION" => quote!(::brioche_core::PluginCapabilities::AFTER_PREDICTION),
        "ON_TOOL_CALLS" => quote!(::brioche_core::PluginCapabilities::ON_TOOL_CALLS),
        "ON_TOOL_RESULT" => quote!(::brioche_core::PluginCapabilities::ON_TOOL_RESULT),
        "ON_ERROR" => quote!(::brioche_core::PluginCapabilities::ON_ERROR),
        _ => {
            let msg = format!("unknown capability `{}`", cap);
            quote!(compile_error!(#msg))
        }
    }
}

/// `#[brioche_plugin(name = "...", capabilities = "ON_INPUT | BEFORE_PREDICTION")]`
///
/// Attribute macro applied to an `impl BriochePlugin for MyPlugin` block.
/// It injects `fn name()`, `fn capabilities()`, and optionally `fn priority()`
/// into the impl. All other items are passed through unchanged.
///
/// Helper attributes `#[hook(...)]` on methods are stripped by this macro.
///
/// # Example
/// ```ignore
/// #[brioche_plugin(name = "my_plugin", capabilities = "ON_INPUT")]
/// impl BriochePlugin for MyPlugin {
///     #[hook(on_input)]
///     fn on_input(&self, input: &EngineInput, ext: &mut ExtensionStorage) -> PluginResult<PolicyDecision> {
///         Ok(PolicyDecision::Allow)
///     }
/// }
/// ```
/// Refs: docs/SPECS.md §Book I
#[proc_macro_attribute]
pub fn brioche_plugin(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as PluginArgs);
    let mut item_impl = parse_macro_input!(input as ItemImpl);

    let name_lit = &args.name;
    let _name_str = name_lit.value();

    // Build capabilities expression via BitOr chain.
    let caps_expr = if args.capabilities.is_empty() {
        quote!(::brioche_core::PluginCapabilities::NONE)
    } else {
        let first = capability_to_tokens(&args.capabilities[0]);
        let rest: Vec<_> = args
            .capabilities
            .iter()
            .skip(1)
            .map(|s| capability_to_tokens(s.as_str()))
            .collect();
        quote! {
            #first #(| #rest)*
        }
    };

    let priority_expr = match args.priority {
        Some(p) => quote!(#p),
        None => quote!(0),
    };

    // Strip #[hook(...)] attributes from methods.
    for item in &mut item_impl.items {
        if let ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| !attr.path().is_ident("hook"));
        }
    }

    // Build the injected trait methods.
    let injected: syn::ImplItem = syn::parse_quote! {
        fn name(&self) -> &'static str {
            #name_lit
        }
    };
    let injected_caps: syn::ImplItem = syn::parse_quote! {
        fn capabilities(&self) -> ::brioche_core::PluginCapabilities {
            #caps_expr
        }
    };
    let injected_priority: syn::ImplItem = syn::parse_quote! {
        fn priority(&self) -> i16 {
            #priority_expr
        }
    };

    // Prepend injected methods so they appear first.
    item_impl.items.insert(
        0,
        syn::parse_quote!(
            fn __plugin_marker(&self) {}
        ),
    );
    // Remove the dummy marker we just inserted (it was just to satisfy type check).
    item_impl.items.remove(0);

    // We need to be careful: if the impl already contains `name()`, `capabilities()`,
    // or `priority()`, we must not duplicate them. Check for existing.
    let has_name = item_impl
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "name"));
    let has_caps = item_impl
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "capabilities"));
    let has_priority = item_impl
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "priority"));

    let mut new_items = Vec::new();
    if !has_name {
        new_items.push(injected);
    }
    if !has_caps {
        new_items.push(injected_caps);
    }
    if !has_priority {
        new_items.push(injected_priority);
    }
    new_items.extend(item_impl.items);
    item_impl.items = new_items;

    TokenStream::from(quote!(#item_impl))
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
    let fn_name = &func.sig.ident;
    let vis = &func.vis;
    let inputs = &func.sig.inputs;
    let output = &func.sig.output;
    let block = &func.block;
    let generics = &func.sig.generics;

    if inputs.len() != 1 {
        return syn::Error::new_spanned(
            func.sig,
            "#[brioche_offload_task] function must take exactly one argument",
        )
        .to_compile_error()
        .into();
    }

    let mod_name = syn::Ident::new(&format!("__brioche_offload_{}", fn_name), fn_name.span());

    let arg = match inputs.first() {
        Some(arg) => arg,
        None => {
            return syn::Error::new_spanned(
                func.sig,
                "#[brioche_offload_task] function must take exactly one argument",
            )
            .to_compile_error()
            .into();
        }
    };
    let arg_ty = match arg {
        syn::FnArg::Typed(pat_ty) => &pat_ty.ty,
        _ => {
            return syn::Error::new_spanned(arg, "expected typed argument")
                .to_compile_error()
                .into();
        }
    };

    let output_ty = match output {
        syn::ReturnType::Default => quote!(()),
        syn::ReturnType::Type(_, ty) => quote!(#ty),
    };

    let expanded = quote! {
        #vis fn #fn_name #generics (#inputs) #output #block

        #[doc(hidden)]
        #vis mod #mod_name {
            use super::*;

            /// Serialize the input payload for CPU task offloading.
            /// Refs: docs/SPECS.md §Book I
            pub fn serialize_input(input: &#arg_ty) -> ::std::vec::Vec<u8> {
                match ::brioche_core::postcard::to_stdvec(input) {
                    Ok(v) => v,
                    Err(_) => ::std::vec::Vec::new(),
                }
            }

            /// Deserialize the result payload after CPU task completion.
            /// Refs: docs/SPECS.md §Book I
            pub fn deserialize_output(bytes: &[u8]) -> #output_ty {
                match ::brioche_core::postcard::from_bytes(bytes) {
                    Ok(v) => v,
                    Err(_) => {
                        // Fallback: return a default value.
                        // This path is best-effort; the caller should validate.
                        ::core::default::Default::default()
                    }
                }
            }

            /// Build an `Effect::ExecuteCpuTask` from a task id and input.
            /// Refs: docs/SPECS.md §Book I
            pub fn effect(task_id: impl Into<::brioche_core::TaskId>, input: &#arg_ty) -> ::brioche_core::Effect {
                ::brioche_core::Effect::ExecuteCpuTask {
                    task_id: task_id.into(),
                    payload: serialize_input(input),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
