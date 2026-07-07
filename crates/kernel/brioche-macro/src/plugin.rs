//! Book I — plugin authoring macro implementation.
//!
//! This module parses plugin metadata and injects static capability methods while
//! preserving trait-owned routing semantics.
//!
//! Refs: docs/SPECS.md §Book I

use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{ImplItem, ItemImpl, LitStr, Token};
/// Parsed arguments for `#[brioche_plugin(...)]`.
///
/// Refs: I-Core-PluginAuthoring
pub(crate) struct PluginArgs {
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
        for capability in &capabilities {
            match capability.as_str() {
                "ON_INPUT" | "BEFORE_PREDICTION" | "ON_STREAM_EVENT" | "AFTER_PREDICTION"
                | "ON_TOOL_CALLS" | "ON_TOOL_RESULT" | "ON_ERROR" => {}
                unknown => {
                    return Err(syn::Error::new(
                        input.span(),
                        format!("unknown capability `{unknown}`"),
                    ));
                }
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

pub(crate) fn expand_brioche_plugin(
    args: PluginArgs,
    mut item_impl: ItemImpl,
) -> proc_macro2::TokenStream {
    let name_lit = &args.name;
    let _name_str = name_lit.value();
    let _capability_count = args.capabilities.len();

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

    // We need to be careful: if the impl already contains `name()` or
    // `priority()`, we must not duplicate them. Check for existing.
    let has_name = item_impl
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "name"));
    let has_priority = item_impl
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Fn(f) if f.sig.ident == "priority"));

    let mut new_items = Vec::new();
    if !has_name {
        new_items.push(injected);
    }
    if !has_priority {
        new_items.push(injected_priority);
    }
    new_items.extend(item_impl.items);
    item_impl.items = new_items;

    quote!(#item_impl)
}
