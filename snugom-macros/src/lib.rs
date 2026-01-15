use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{ToTokens, format_ident, quote};
use syn::meta::ParseNestedMeta;
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, ExprArray, Field, Fields, Ident, LitBool, LitInt, LitStr, Path, Result,
    Token, Type, TypePath, Visibility, braced, bracketed, parenthesized, parse::Parse, parse::ParseBuffer,
    parse::ParseStream, parse_macro_input, spanned::Spanned,
};

mod client_macro;
mod client_ops_macro;
mod filters;
mod parsed;
mod snug_macro;

use client_macro::ParsedClient;
use client_ops_macro::{
    ClientCreateInvocation, ClientDeleteInvocation, ClientGetOrCreateInvocation,
    ClientUpdateInvocation, ClientUpsertInvocation,
};
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

/// Derive macro for generating a Prisma-style Snugom client.
///
/// This macro generates named collection accessor methods for each entity type.
///
/// # Example
///
/// ```ignore
/// #[derive(SnugomClient)]
/// #[snugom_client(entities = [Guild, GuildMember, Role])]
/// pub struct GuildClient {
///     conn: ConnectionManager,
///     prefix: String,
/// }
///
/// // Usage:
/// let client = GuildClient::connect("redis://localhost", "myapp").await?;
/// let guild = client.guilds().get(&id).await?;
/// let members = client.guild_members().find_many(query).await?;
/// ```
#[proc_macro_derive(SnugomClient, attributes(snugom_client))]
pub fn derive_snugom_client(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    match ParsedClient::from_input(&input) {
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

#[proc_macro_derive(SearchableFilters, attributes(filter))]
pub fn searchable_filters_derive(input: TokenStream) -> TokenStream {
    filters::derive_searchable_filters(input)
}

/// Create an entity using a client.
///
/// This is a convenience macro for creating entities with nested relations
/// using the Prisma-style client API.
///
/// # Example
///
/// ```ignore
/// snugom_create!(client, Guild {
///     name: "Knights",
///     members: [
///         create GuildMember { user_id: "u1", role: Role::Leader },
///     ],
/// }).await?;
/// ```
#[proc_macro]
pub fn snugom_create(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as ClientCreateInvocation);
    match invocation.emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Update an entity using a client.
///
/// # Example
///
/// ```ignore
/// snugom_update!(client, Guild(entity_id = &id) {
///     name: "New Name",
/// }).await?;
/// ```
#[proc_macro]
pub fn snugom_update(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as ClientUpdateInvocation);
    match invocation.emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Delete an entity using a client with optional cascade.
///
/// # Example
///
/// ```ignore
/// // Simple delete
/// snugom_delete!(client, Guild(&guild_id)).await?;
///
/// // Delete with cascade
/// snugom_delete!(client, Guild(&guild_id) {
///     members: cascade,
///     applications: cascade,
/// }).await?;
/// ```
#[proc_macro]
pub fn snugom_delete(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as ClientDeleteInvocation);
    match invocation.emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Upsert an entity using a client.
///
/// # Example
///
/// ```ignore
/// snugom_upsert!(client, Guild(id = &guild_id) {
///     create: Guild {
///         name: "New Guild",
///         member_count: 1,
///     },
///     update: Guild(entity_id = &guild_id) {
///         member_count: member_count + 1,
///     },
/// }).await?;
/// ```
#[proc_macro]
pub fn snugom_upsert(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as ClientUpsertInvocation);
    match invocation.emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Get or create an entity using a client.
///
/// Returns the existing entity if it exists, or creates and returns a new one.
/// Unlike `upsert`, this does NOT update the entity if it already exists.
///
/// # Example
///
/// ```ignore
/// let result = snugom_get_or_create!(client, UserSettings {
///     id: user_settings_id,
///     user_id: user_id.to_string(),
///     theme: "light".to_string(),
///     notifications_enabled: true,
/// }).await?;
///
/// // result is GetOrCreateResult<UserSettings>
/// let settings = result.into_inner();
/// ```
#[proc_macro]
pub fn snugom_get_or_create(input: TokenStream) -> TokenStream {
    let invocation = parse_macro_input!(input as ClientGetOrCreateInvocation);
    match invocation.emit() {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
