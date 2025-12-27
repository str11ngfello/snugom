//! The `bundle!` macro for registering snugom entities with a service.
//!
//! This macro generates:
//! - `BundleRegistered` trait implementations for each entity
//! - Type aliases for repos (e.g., `GuildRepo`)
//! - Key pattern functions (e.g., `all_pattern()`, `guilds_pattern()`)
//! - `ensure_indexes()` function to initialize all search indexes at boot time
//! - `cleanup()` function for test cleanup
//!
//! All entities in a bundle must have at least one indexed field (filterable or sortable).
//! This is validated at compile time.

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{
    Ident, LitStr, Result, Token,
    braced,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

/// Converts a PascalCase identifier to snake_case
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
        } else {
            result.push(ch);
        }
    }
    result
}

/// Simple pluralization rules
fn pluralize(word: &str) -> String {
    if word.ends_with('s') || word.ends_with('x') || word.ends_with("ch") || word.ends_with("sh") {
        format!("{}es", word)
    } else if word.ends_with('y') && !word.ends_with("ay") && !word.ends_with("ey") && !word.ends_with("oy") && !word.ends_with("uy") {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{}s", word)
    }
}

/// An entity declaration in the bundle
#[derive(Debug)]
pub struct EntityDecl {
    /// The type name (e.g., `Guild`)
    pub name: Ident,
    /// Optional explicit collection name override (e.g., `"requests"` instead of auto-derived)
    pub collection_override: Option<String>,
}

impl Parse for EntityDecl {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;

        let collection_override = if input.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            let lit: LitStr = input.parse()?;
            Some(lit.value())
        } else {
            None
        };

        Ok(EntityDecl {
            name,
            collection_override,
        })
    }
}

/// The parsed bundle! invocation
#[derive(Debug)]
pub struct BundleInvocation {
    /// Service name (e.g., "guild")
    pub service: String,
    /// List of entities to register
    pub entities: Vec<EntityDecl>,
}

impl Parse for BundleInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut service: Option<String> = None;
        let mut entities: Vec<EntityDecl> = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            if key == "service" {
                let value: LitStr = input.parse()?;
                service = Some(value.value());
            } else if key == "entities" {
                let content;
                braced!(content in input);
                let parsed: Punctuated<EntityDecl, Token![,]> =
                    content.parse_terminated(EntityDecl::parse, Token![,])?;
                entities = parsed.into_iter().collect();
            } else {
                return Err(syn::Error::new(key.span(), format!("unknown bundle key: {}", key)));
            }

            // Optional trailing comma between top-level items
            let _ = input.parse::<Token![,]>();
        }

        let service = service.ok_or_else(|| {
            syn::Error::new(Span::call_site(), "bundle! requires `service: \"...\"`")
        })?;

        if entities.is_empty() {
            return Err(syn::Error::new(
                Span::call_site(),
                "bundle! requires at least one entity in `entities: { ... }`",
            ));
        }

        Ok(BundleInvocation { service, entities })
    }
}

impl BundleInvocation {
    pub fn emit(&self) -> TokenStream2 {
        let service = &self.service;
        let service_ident = format_ident!("{}", service);

        // Collect all collection names for target validation
        let collection_names: Vec<String> = self.entities.iter().map(|e| {
            e.collection_override.clone().unwrap_or_else(|| {
                pluralize(&to_snake_case(&e.name.to_string()))
            })
        }).collect();

        // Generate entity metadata
        let mut bundle_registered_impls = Vec::new();
        let mut repo_aliases = Vec::new();
        let mut pattern_fns = Vec::new();
        let mut key_fns = Vec::new();
        let mut relation_validations = Vec::new();
        let mut indexed_field_validations = Vec::new();
        let mut ensure_index_calls = Vec::new();

        for entity in &self.entities {
            let entity_name = &entity.name;
            let snake_name = to_snake_case(&entity_name.to_string());

            // Derive collection name: explicit override or auto-pluralize
            let collection = entity.collection_override.clone().unwrap_or_else(|| {
                pluralize(&snake_name)
            });

            // BundleRegistered impl
            bundle_registered_impls.push(quote! {
                impl ::snugom::types::BundleRegistered for #entity_name {
                    const SERVICE: &'static str = #service;
                    const COLLECTION: &'static str = #collection;
                }
            });

            // Repo type alias (e.g., pub type GuildRepo = Repo<Guild>;)
            let repo_alias = format_ident!("{}Repo", entity_name);
            repo_aliases.push(quote! {
                pub type #repo_alias = ::snugom::Repo<#entity_name>;
            });

            // Pattern function for this collection (e.g., fn guilds_pattern() -> String)
            let pattern_fn_name = format_ident!("{}_pattern", collection);
            pattern_fns.push(quote! {
                /// Pattern for all #entity_name entities
                #[inline]
                pub fn #pattern_fn_name(prefix: &str) -> String {
                    format!("{}:{}:{}:*", prefix, #service, #collection)
                }
            });

            // Key function for specific entity (e.g., fn guild_key(id: &str) -> String)
            let key_fn_name = format_ident!("{}_key", snake_name);
            key_fns.push(quote! {
                /// Key for a specific #entity_name entity
                #[inline]
                pub fn #key_fn_name(prefix: &str, id: &str) -> String {
                    format!("{}:{}:{}:{}", prefix, #service, #collection, id)
                }
            });

            // Generate compile-time validation for relation targets
            let entity_name_str = entity_name.to_string();
            let valid_targets = &collection_names;
            relation_validations.push(quote! {
                ::snugom::validate_relation_targets(
                    #entity_name_str,
                    #entity_name::RELATION_TARGETS,
                    &[#(#valid_targets),*],
                );
            });

            // Generate compile-time validation for indexed fields
            indexed_field_validations.push(quote! {
                ::snugom::validate_entity_has_indexed_fields(
                    #entity_name_str,
                    <#entity_name as ::snugom::types::EntityMetadata>::HAS_INDEXED_FIELDS,
                );
            });

            // Generate ensure_search_index call for this entity
            ensure_index_calls.push(quote! {
                {
                    let repo: ::snugom::Repo<#entity_name> = ::snugom::Repo::new(prefix.to_string());
                    repo.ensure_search_index(conn).await?;
                }
            });
        }

        quote! {
            // BundleRegistered implementations for each entity
            #(#bundle_registered_impls)*

            /// Generated module for the #service_ident service bundle
            pub mod #service_ident {
                use super::*;

                /// Service name constant
                pub const SERVICE: &str = #service;

                // Type aliases for repos
                #(#repo_aliases)*

                /// Pattern for all keys in this service
                #[inline]
                pub fn all_pattern(prefix: &str) -> String {
                    format!("{}:{}:*", prefix, #service)
                }

                // Pattern functions for each collection
                #(#pattern_fns)*

                // Key functions for each entity
                #(#key_fns)*

                /// Ensure all search indexes for this service's entities are created.
                /// Call this once at service boot time.
                pub async fn ensure_indexes(
                    conn: &mut ::redis::aio::ConnectionManager,
                    prefix: &str,
                ) -> Result<(), ::snugom::RepoError> {
                    #(#ensure_index_calls)*
                    Ok(())
                }

                /// Delete all data for this service (useful for testing)
                pub async fn cleanup(
                    conn: &mut ::redis::aio::ConnectionManager,
                    prefix: &str,
                ) -> Result<u64, ::snugom::RepoError> {
                    ::snugom::cleanup_pattern(conn, &all_pattern(prefix)).await
                }

                // Compile-time validation of relation targets and indexed fields
                // This const block ensures validation runs at compile time
                const _: () = {
                    #(#relation_validations)*
                    #(#indexed_field_validations)*
                };
            }
        }
    }
}
