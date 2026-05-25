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

/// Scan all fields of a struct/enum for banned types.
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
            }
        }
        Fields::Unnamed(unnamed) => {
            for (i, f) in unnamed.unnamed.iter().enumerate() {
                scan_type(&f.ty, errors, &format!("_{}", i));
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
                // Pragmatic estimate: size_of_val. Will be refined with
                // actual serialization weight in Sprint 2.
                ::core::mem::size_of_val(self)
            }

            fn snapshot_strategy() -> ::brioche_core::SnapshotStrategy {
                #snapshot_strategy
            }
        }
    };

    TokenStream::from(expanded)
}
