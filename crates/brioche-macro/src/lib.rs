//! Book I — The Core Book: Procedural macros for `BriocheExtensionType`.
//!
//! This crate upholds:
//! - I-Core-ExtensionType: Compile-time verification of extension types.
//!
//! The `#[derive(BriocheExtensionType)]` macro performs compile-time
//! verification of extension types and generates required trait
//! implementations.
//!
//! Refs: SPECS.md §3.2

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, Data, DataStruct, DeriveInput, Fields, Type, parse_macro_input};

/// Errors raised by the derive macro.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum DeriveError {
    HashMapInField { span: Span, field_name: String },
    HashSetInField { span: Span, field_name: String },
    UiTypeInField { span: Span, field_name: String },
    UndeterminedVecInField { span: Span, field_name: String },
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
                let path = meta
                    .path
                    .get_ident()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
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

            // Check for banned collections.
            for seg in &segments {
                if seg == "HashMap" {
                    errors.push(DeriveError::HashMapInField {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                    });
                }
                if seg == "HashSet" {
                    errors.push(DeriveError::HashSetInField {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                    });
                }
            }

            // Check last segment for banned collections (catches imported aliases).
            if let Some(last) = segments.last() {
                if last == "HashMap" {
                    errors.push(DeriveError::HashMapInField {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                    });
                }
                if last == "HashSet" {
                    errors.push(DeriveError::HashSetInField {
                        span: type_path.span(),
                        field_name: field_name.to_string(),
                    });
                }

                // UI type detection.
                let ui_keywords = ["tauri", "vue", "dom", "web_sys", "js_sys"];
                if ui_keywords.iter().any(|k| last.to_lowercase().contains(k)) {
                    errors.push(DeriveError::UiTypeInField {
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
                let name = f
                    .ident
                    .as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "_".to_string());
                scan_type(&f.ty, errors, &name);
                if type_contains_vec(&f.ty) && !field_has_deterministic_order(&f.attrs) {
                    errors.push(DeriveError::UndeterminedVecInField {
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
                    errors.push(DeriveError::UndeterminedVecInField {
                        span: f.ty.span(),
                        field_name: name,
                    });
                }
            }
        }
        Fields::Unit => {}
    }
}

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
            DeriveError::HashMapInField { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses HashMap. HashMap is prohibited in BriocheExtensionType persisted state. Use BTreeMap or IndexMap instead."));
                }
            }
            DeriveError::HashSetInField { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses HashSet. HashSet is prohibited in BriocheExtensionType persisted state. Use BTreeSet instead."));
                }
            }
            DeriveError::UiTypeInField { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` appears to contain a UI type. UI types are prohibited in BriocheExtensionType."));
                }
            }
            DeriveError::UndeterminedVecInField { span, field_name } => {
                quote_spanned! { *span =>
                    compile_error!(concat!("Field `", #field_name, "` uses Vec without #[brioche(deterministic_order)]. Persisted Vec fields must have deterministic insertion order. Use #[brioche(deterministic_order)] to certify determinism, or replace with BTreeMap/IndexMap."));
                }
            }
        })
        .collect();

    let expanded = quote! {
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
    };

    TokenStream::from(expanded)
}
