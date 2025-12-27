use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::meta::ParseNestedMeta;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, ExprArray, Field, Fields, Ident, LitBool, LitInt, LitStr, Path, Result,
    Token, Type, TypePath, Visibility, braced, bracketed, parenthesized, parse::Parse, parse::ParseBuffer,
    parse::ParseStream, parse_macro_input, spanned::Spanned,
};

mod bundle_macro;
mod filters;
mod parsed;
mod run_macro;
mod snug_macro;

use bundle_macro::BundleInvocation;
use parsed::ParsedEntity;
use snug_macro::SnugInvocation;

#[proc_macro_derive(SnugomEntity, attributes(snugom))]
pub fn derive_snugom_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match ParsedEntity::from_input(&input) {
        Ok(parsed) => parsed.emit().into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro]
pub fn snug(input: TokenStream) -> TokenStream {
    match parse_macro_input!(input as SnugInvocation).emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro]
pub fn run(input: TokenStream) -> TokenStream {
    match parse_macro_input!(input as run_macro::RunInvocation).emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(SearchableFilters, attributes(filter))]
pub fn searchable_filters_derive(input: TokenStream) -> TokenStream {
    filters::derive_searchable_filters(input)
}

/// Register entities with a service bundle.
///
/// This macro generates:
/// - `BundleRegistered` trait implementations for each entity
/// - Type aliases for repos (e.g., `GuildRepo`)
/// - Key pattern functions (e.g., `all_pattern()`, `guilds_pattern()`)
/// - `ensure_indexes()` function to initialize all search indexes at boot time
/// - `cleanup()` function for test cleanup
///
/// All entities in a bundle must have at least one indexed field (filterable or sortable).
/// This is validated at compile time with a clear error message.
///
/// # Example
///
/// ```text
/// bundle! {
///     service: "guild",
///     entities: {
///         Guild,
///         GuildMember,
///         GuildApplication => "apps",  // Override auto-pluralization
///     }
/// }
///
/// // Generated:
/// // - guild::GuildRepo, guild::GuildMemberRepo, guild::GuildApplicationRepo
/// // - guild::all_pattern(prefix), guild::guilds_pattern(prefix), etc.
/// // - guild::ensure_indexes(conn, prefix) - call at boot time
/// // - guild::cleanup(conn, prefix)
/// ```
#[proc_macro]
pub fn bundle(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as BundleInvocation);
    invocation.emit().into()
}
