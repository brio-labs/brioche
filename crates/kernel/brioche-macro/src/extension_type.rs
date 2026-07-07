//! Book I — Core extension-type derive implementation.
//!
//! This module owns scanning, validation, and code generation for
//! `BriocheExtensionType` persisted state.
//!
//! Refs: I-Core-ExtensionType, I-Eco-OrderedCollections

use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataStruct, DeriveInput, Fields, Type};
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
    UndeterminedIndexMap {
        span: Span,
        field_name: String,
    },
}

/// Ordered map types permitted in persisted extension state.
///
/// Refs: I-Eco-OrderedCollections
const ALLOWED_MAPS: &[&str] = &["BTreeMap", "IndexMap"];

/// Ordered set types permitted in persisted extension state.
///
/// Refs: I-Eco-OrderedCollections
const ALLOWED_SETS: &[&str] = &["BTreeSet", "IndexSet"];

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

/// Recursively scan a type for banned collections (`HashMap`, `HashSet`),
/// non-ordered map/set aliases, and UI types.
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

            // Check the final path segment for aliases of non-ordered maps/sets.
            // The `ends_with("Map")` / `ends_with("Set")` heuristic intentionally
            // accepts false positives (e.g. `type OrderedMap = BTreeMap<...>`)
            // because proc-macros cannot resolve type aliases. Users should either
            // use the allowed concrete types directly or choose a suffix that does
            // not look like a map/set.
            if let Some(last) = segments.last() {
                if !has_hashmap && last.ends_with("Map") && !ALLOWED_MAPS.contains(&last.as_str()) {
                    errors.push(DeriveError::NonOrderedMap {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                        type_name: last.clone(),
                    });
                }
                if !has_hashset && last.ends_with("Set") && !ALLOWED_SETS.contains(&last.as_str()) {
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

/// Recursively check whether a type contains `IndexMap`.
///
/// `IndexMap` preserves insertion order but its default hasher is not
/// guaranteed to be deterministic across processes. We treat it like
/// `Vec`: the field must carry `#[brioche(deterministic_order)]` to
/// certify deterministic ordering.
fn type_contains_indexmap(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.iter().any(|s| s == "IndexMap") {
                return true;
            }
            if let Some(seg) = type_path.path.segments.last()
                && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
            {
                for arg in &args.args {
                    if let syn::GenericArgument::Type(inner) = arg
                        && type_contains_indexmap(inner)
                    {
                        return true;
                    }
                }
            }
            false
        }
        Type::Array(arr) => type_contains_indexmap(&arr.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_indexmap),
        Type::Reference(rf) => type_contains_indexmap(&rf.elem),
        Type::Paren(paren) => type_contains_indexmap(&paren.elem),
        Type::Slice(slice) => type_contains_indexmap(&slice.elem),
        Type::BareFn(bare) => {
            bare.inputs.iter().any(|inp| type_contains_indexmap(&inp.ty))
                || matches!(&bare.output, syn::ReturnType::Type(_, output) if type_contains_indexmap(output))
        }
        Type::ImplTrait(impl_trait) => impl_trait.bounds.iter().any(|bound| {
            if let syn::TypeParamBound::Trait(trait_bound) = bound {
                trait_bound.path.segments.iter().any(|seg| {
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        args.args.iter().any(|arg| {
                            matches!(arg, syn::GenericArgument::Type(inner) if type_contains_indexmap(inner))
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

/// Check whether a field carries a given `#[brioche(...)]` flag.
fn field_has_brioche_flag(attrs: &[Attribute], flag: &str) -> bool {
    for attr in attrs {
        if !attr.path().is_ident("brioche") {
            continue;
        }
        let mut found = false;
        // Ignore parse errors — malformed attributes are handled elsewhere.
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(flag) {
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
/// `Vec` / `IndexMap` fields.
fn scan_fields(fields: &Fields, errors: &mut Vec<DeriveError>) {
    match fields {
        Fields::Named(named) => {
            for f in &named.named {
                let name = match f.ident.as_ref() {
                    Some(ident) => ident.to_string(),
                    None => "_".to_string(),
                };
                scan_type(&f.ty, errors, &name);
                if type_contains_vec(&f.ty)
                    && !field_has_brioche_flag(&f.attrs, "deterministic_order")
                {
                    errors.push(DeriveError::UndeterminedVec {
                        span: f.ty.span(),
                        field_name: name.clone(),
                    });
                }
                if type_contains_indexmap(&f.ty)
                    && !field_has_brioche_flag(&f.attrs, "deterministic_order")
                {
                    errors.push(DeriveError::UndeterminedIndexMap {
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
                if type_contains_vec(&f.ty)
                    && !field_has_brioche_flag(&f.attrs, "deterministic_order")
                {
                    errors.push(DeriveError::UndeterminedVec {
                        span: f.ty.span(),
                        field_name: name.clone(),
                    });
                }
                if type_contains_indexmap(&f.ty)
                    && !field_has_brioche_flag(&f.attrs, "deterministic_order")
                {
                    errors.push(DeriveError::UndeterminedIndexMap {
                        span: f.ty.span(),
                        field_name: name,
                    });
                }
            }
        }
        Fields::Unit => {}
    }
}

/// Recursively extract immediate nested carrier types from a type.
///
/// Collection wrappers (`Vec`, `Option`, `Box`, `BTreeMap`, `IndexMap`,
/// `HashMap`, `BTreeSet`, `IndexSet`, `HashSet`) are unwrapped; their
/// element/value types are returned. Non-wrapper types are returned as-is.
/// Bare pointers, references, arrays, slices, and tuples are recursed.
///
/// This does not expand into the definitions of returned carriers; each
/// carrier's own `#[derive(BriocheExtensionType)]` will scan its own fields.
fn extract_immediate_carriers(ty: &Type) -> Vec<Type> {
    let mut out = Vec::new();
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let last = segments.last().map(String::as_str);

            if let Some(seg) = type_path.path.segments.last()
                && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
            {
                let type_args: Vec<&Type> = args
                    .args
                    .iter()
                    .filter_map(|arg| match arg {
                        syn::GenericArgument::Type(inner) => Some(inner),
                        _ => None,
                    })
                    .collect();

                match last {
                    Some("Vec") | Some("Option") | Some("Box") if !type_args.is_empty() => {
                        out.extend(extract_immediate_carriers(type_args[0]));
                        return out;
                    }
                    Some("BTreeMap") | Some("IndexMap") | Some("HashMap")
                        if type_args.len() >= 2 =>
                    {
                        // Value type is the carrier; key type is typically a
                        // scalar or string and is filtered out later.
                        out.extend(extract_immediate_carriers(type_args[1]));
                        return out;
                    }
                    Some("BTreeSet") | Some("IndexSet") | Some("HashSet")
                        if !type_args.is_empty() =>
                    {
                        out.extend(extract_immediate_carriers(type_args[0]));
                        return out;
                    }
                    Some("Result") if type_args.len() >= 2 => {
                        out.extend(extract_immediate_carriers(type_args[0]));
                        return out;
                    }
                    _ => {}
                }
            }

            // Non-wrapper type: return it as a carrier (filtering happens later).
            out.push(ty.clone());
        }
        Type::Array(arr) => out.extend(extract_immediate_carriers(&arr.elem)),
        Type::Tuple(tuple) => {
            for elem in &tuple.elems {
                out.extend(extract_immediate_carriers(elem));
            }
        }
        Type::Reference(rf) => out.extend(extract_immediate_carriers(&rf.elem)),
        Type::Paren(paren) => out.extend(extract_immediate_carriers(&paren.elem)),
        Type::Slice(slice) => out.extend(extract_immediate_carriers(&slice.elem)),
        Type::BareFn(bare) => {
            for inp in &bare.inputs {
                out.extend(extract_immediate_carriers(&inp.ty));
            }
            if let syn::ReturnType::Type(_, output) = &bare.output {
                out.extend(extract_immediate_carriers(output));
            }
        }
        Type::ImplTrait(impl_trait) => {
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    for seg in &trait_bound.path.segments {
                        if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                            for arg in &args.args {
                                if let syn::GenericArgument::Type(inner) = arg {
                                    out.extend(extract_immediate_carriers(inner));
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// Returns `true` if the type is a primitive scalar or `String`.
///
/// These types are never required to implement `BriocheExtensionType`,
/// even when they appear as nested carriers in persisted state.
fn is_primitive_or_string(ty: &Type) -> bool {
    let primitives = [
        "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
        "f32", "f64", "bool", "char", "str",
    ];
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if let Some(last) = segments.last() {
                if last == "String" {
                    return true;
                }
                if primitives.contains(&last.as_str()) && segments.len() == 1 {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Collect all nested carrier types from fields of a struct/enum.
///
/// Unlike the previous implementation, this does not require explicit
/// `#[brioche(nested_carrier)]` annotations. Every non-primitive field type
/// or carrier extracted from a collection wrapper is asserted to implement
/// `BriocheExtensionType` at compile time, ensuring nested `Vec`/`HashMap`
/// checks cannot escape by hiding inside another carrier.
///
/// Primitives and `String` are skipped because they do not implement the
/// trait and are deterministic by construction.
fn collect_nested_carriers(fields: &Fields, out: &mut Vec<Type>) {
    match fields {
        Fields::Named(named) => {
            for f in &named.named {
                for carrier in extract_immediate_carriers(&f.ty) {
                    if !is_primitive_or_string(&carrier) {
                        out.push(carrier);
                    }
                }
            }
        }
        Fields::Unnamed(unnamed) => {
            for f in &unnamed.unnamed {
                for carrier in extract_immediate_carriers(&f.ty) {
                    if !is_primitive_or_string(&carrier) {
                        out.push(carrier);
                    }
                }
            }
        }
        Fields::Unit => {}
    }
}

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
pub(crate) fn expand_brioche_extension_type(input: DeriveInput) -> proc_macro2::TokenStream {
    let attrs = match parse_brioche_attrs(&input.attrs) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error(),
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

    // Collect validation errors and nested carrier fields.
    let mut errors: Vec<DeriveError> = Vec::new();
    let mut nested_carriers: Vec<Type> = Vec::new();

    match &input.data {
        Data::Struct(DataStruct { fields, .. }) => {
            scan_fields(fields, &mut errors);
            collect_nested_carriers(fields, &mut nested_carriers);
        }
        Data::Enum(data_enum) => {
            for variant in &data_enum.variants {
                scan_fields(&variant.fields, &mut errors);
                collect_nested_carriers(&variant.fields, &mut nested_carriers);
            }
        }
        Data::Union(data_union) => {
            let fields = Fields::Named(data_union.fields.clone());
            scan_fields(&fields, &mut errors);
            collect_nested_carriers(&fields, &mut nested_carriers);
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
                    compile_error!(concat!("Field `", #field_name, "` uses HashSet. HashSet is prohibited in BriocheExtensionType persisted state. Use BTreeSet or IndexSet instead."));
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
            DeriveError::UndeterminedIndexMap { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses IndexMap without #[brioche(deterministic_order)]. Persisted IndexMap fields must certify deterministic ordering, or replace with BTreeMap."));
                }
            }
        })
        .collect();
    // Const assertions for fields marked as nested carriers. These fail
    // at compile time unless the field type implements `BriocheExtensionType`.
    let nested_carrier_assertions: Vec<proc_macro2::TokenStream> = nested_carriers
        .iter()
        .map(|ty| {
            quote_spanned! { ty.span() =>
                const _: () = {
                    let _ = <#ty as ::brioche_core::BriocheExtensionType>::EXT_ID;
                };
            }
        })
        .collect();

    if errors.is_empty() {
        quote! {
            #(#error_tokens)*
            #(#nested_carrier_assertions)*

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
                    fn serialize(any: &dyn ::core::any::Any) -> ::core::result::Result<::std::vec::Vec<u8>, ::std::string::String> {
                        if let Some(this) = any.downcast_ref::<#name>() {
                            ::brioche_core::postcard::to_stdvec(this)
                                .map_err(|e| ::std::string::String::from("postcard: ") + &e.to_string())
                        } else {
                            ::core::result::Result::Err(::std::string::String::from("downcast failed"))
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
    }
}
