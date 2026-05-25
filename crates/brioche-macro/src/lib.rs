//! Procedural macros for Brioche.

use proc_macro::TokenStream;

/// Placeholder derive macro.
#[proc_macro_derive(BriocheExtensionType)]
pub fn derive_brioche_extension_type(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
