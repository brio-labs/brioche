//! Book I — CPU offload task macro implementation.
//!
//! This module generates CPU-task effect helpers for explicit offload boundaries.
//!
//! Refs: docs/SPECS.md §Book I

use quote::quote;
pub(crate) fn expand_brioche_offload_task(func: syn::ItemFn) -> proc_macro2::TokenStream {
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
        .to_compile_error();
    }

    let mod_name = syn::Ident::new(&format!("__brioche_offload_{}", fn_name), fn_name.span());

    let arg = match inputs.first() {
        Some(arg) => arg,
        None => {
            return syn::Error::new_spanned(
                func.sig,
                "#[brioche_offload_task] function must take exactly one argument",
            )
            .to_compile_error();
        }
    };
    let arg_ty = match arg {
        syn::FnArg::Typed(pat_ty) => &pat_ty.ty,
        _ => {
            return syn::Error::new_spanned(arg, "expected typed argument").to_compile_error();
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

    expanded
}
