//! The `SnugomClient` derive macro for generating named collection accessors.
//!
//! This macro generates methods like `guilds()`, `members()` that return
//! typed `CollectionHandle<T>` instances for each entity type.
//!
//! # Example
//!
//! ```ignore
//! #[derive(SnugomClient)]
//! #[snugom_client(entities = [Guild, GuildMember, Role])]
//! pub struct GuildClient {
//!     conn: ConnectionManager,
//!     prefix: String,
//! }
//!
//! // Generates:
//! impl GuildClient {
//!     pub fn new(conn: ConnectionManager, prefix: String) -> Self { ... }
//!     pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, RedisError> { ... }
//!     pub fn guilds(&self) -> CollectionHandle<Guild> { ... }
//!     pub fn guild_members(&self) -> CollectionHandle<GuildMember> { ... }
//!     pub fn roles(&self) -> CollectionHandle<Role> { ... }
//! }
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Error, Fields, Ident, Result, Token,
    bracketed,
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
        format!("{word}es")
    } else if word.ends_with('y')
        && !word.ends_with("ay")
        && !word.ends_with("ey")
        && !word.ends_with("oy")
        && !word.ends_with("uy")
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// Parsed entity entry in the entities list
#[derive(Debug)]
pub struct EntityEntry {
    /// The entity type name (e.g., Guild)
    pub type_name: Ident,
    /// Optional method name override
    pub method_name: Option<Ident>,
}

impl Parse for EntityEntry {
    fn parse(input: ParseStream) -> Result<Self> {
        let type_name: Ident = input.parse()?;

        let method_name = if input.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            Some(input.parse::<Ident>()?)
        } else {
            None
        };

        Ok(EntityEntry {
            type_name,
            method_name,
        })
    }
}

/// Parsed attributes for SnugomClient derive
#[derive(Debug, Default)]
pub struct ClientAttributes {
    /// List of entity types to generate accessors for
    pub entities: Vec<EntityEntry>,
    /// Field to use as connection (if not using standard field detection)
    pub conn_field: Option<Ident>,
    /// Field to use as prefix (if not using standard field detection)
    pub prefix_field: Option<Ident>,
}

impl ClientAttributes {
    fn parse_snugom_client_attr(input: ParseStream) -> Result<Self> {
        let mut attrs = ClientAttributes::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if key == "entities" {
                let content;
                bracketed!(content in input);
                let parsed: Punctuated<EntityEntry, Token![,]> =
                    content.parse_terminated(EntityEntry::parse, Token![,])?;
                attrs.entities = parsed.into_iter().collect();
            } else if key == "connection" {
                attrs.conn_field = Some(input.parse()?);
            } else if key == "prefix" {
                attrs.prefix_field = Some(input.parse()?);
            } else {
                return Err(Error::new(key.span(), format!("unknown attribute: {key}")));
            }

            // Optional trailing comma
            let _ = input.parse::<Token![,]>();
        }

        Ok(attrs)
    }
}

/// The parsed SnugomClient derive input
#[derive(Debug)]
pub struct ParsedClient {
    /// The struct name
    pub name: Ident,
    /// Parsed attributes
    pub attrs: ClientAttributes,
    /// The connection field name
    pub conn_field: Ident,
    /// The prefix field name
    pub prefix_field: Ident,
}

impl ParsedClient {
    pub fn from_input(input: &DeriveInput) -> Result<Self> {
        let name = input.ident.clone();

        // Parse #[snugom_client(...)] attribute
        let mut attrs = ClientAttributes::default();
        for attr in &input.attrs {
            if attr.path().is_ident("snugom_client") {
                attrs = attr.parse_args_with(ClientAttributes::parse_snugom_client_attr)?;
            }
        }

        if attrs.entities.is_empty() {
            return Err(Error::new(
                name.span(),
                "SnugomClient requires at least one entity in #[snugom_client(entities = [...])]",
            ));
        }

        // Find connection and prefix fields
        let (conn_field, prefix_field) = Self::find_fields(input, &attrs)?;

        Ok(ParsedClient {
            name,
            attrs,
            conn_field,
            prefix_field,
        })
    }

    fn find_fields(input: &DeriveInput, attrs: &ClientAttributes) -> Result<(Ident, Ident)> {
        let Data::Struct(data_struct) = &input.data else {
            return Err(Error::new(
                input.ident.span(),
                "SnugomClient can only be derived on structs",
            ));
        };

        let Fields::Named(fields) = &data_struct.fields else {
            return Err(Error::new(
                input.ident.span(),
                "SnugomClient requires named fields",
            ));
        };

        // Use explicit fields from attributes if provided
        if let (Some(conn), Some(prefix)) = (&attrs.conn_field, &attrs.prefix_field) {
            return Ok((conn.clone(), prefix.clone()));
        }

        // Auto-detect fields by name
        let mut conn_field: Option<Ident> = None;
        let mut prefix_field: Option<Ident> = None;

        for field in &fields.named {
            let field_name = field.ident.as_ref().unwrap();
            let name_str = field_name.to_string();

            if name_str == "conn" || name_str == "connection" {
                conn_field = Some(field_name.clone());
            } else if name_str == "prefix" {
                prefix_field = Some(field_name.clone());
            }
        }

        let conn = conn_field.ok_or_else(|| {
            Error::new(
                input.ident.span(),
                "Could not find connection field. Add a field named 'conn' or 'connection', \
                 or specify with #[snugom_client(connection = field_name)]",
            )
        })?;

        let prefix = prefix_field.ok_or_else(|| {
            Error::new(
                input.ident.span(),
                "Could not find prefix field. Add a field named 'prefix', \
                 or specify with #[snugom_client(prefix = field_name)]",
            )
        })?;

        Ok((conn, prefix))
    }

    pub fn emit(&self) -> TokenStream2 {
        let name = &self.name;
        let conn_field = &self.conn_field;
        let prefix_field = &self.prefix_field;

        // Generate accessor methods for each entity
        let accessors: Vec<TokenStream2> = self
            .attrs
            .entities
            .iter()
            .map(|entity| {
                let entity_type = &entity.type_name;

                // Method name: explicit override or auto-derived from type name
                let method_name = entity.method_name.clone().unwrap_or_else(|| {
                    let snake = to_snake_case(&entity_type.to_string());
                    let plural = pluralize(&snake);
                    format_ident!("{}", plural)
                });

                quote! {
                    /// Get a collection handle for #entity_type entities.
                    pub fn #method_name(&self) -> ::snugom::CollectionHandle<#entity_type> {
                        let repo = ::snugom::Repo::new(self.#prefix_field.clone());
                        ::snugom::CollectionHandle::new(repo, self.#conn_field.clone())
                    }
                }
            })
            .collect();

        let constructor = quote! {
            /// Create a new client with the given connection and prefix.
            pub fn new(conn: ::snugom::ConnectionManager, prefix: impl Into<String>) -> Self {
                Self {
                    #conn_field: conn,
                    #prefix_field: prefix.into(),
                }
            }

            /// Create a client by connecting to Redis.
            pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, ::redis::RedisError> {
                let redis_client = ::redis::Client::open(url)?;
                let conn = ::snugom::ConnectionManager::new(redis_client).await?;
                Ok(Self::new(conn, prefix))
            }

            /// Get a clone of the connection manager.
            pub fn connection(&self) -> ::snugom::ConnectionManager {
                self.#conn_field.clone()
            }

            /// Get a mutable reference to the connection manager.
            pub fn connection_mut(&mut self) -> &mut ::snugom::ConnectionManager {
                &mut self.#conn_field
            }

            /// Get the key prefix.
            pub fn prefix(&self) -> &str {
                &self.#prefix_field
            }

            /// Get a generic collection handle for any entity type.
            pub fn collection<E: ::snugom::SnugomModel>(&self) -> ::snugom::CollectionHandle<E> {
                let repo = ::snugom::Repo::new(self.#prefix_field.clone());
                ::snugom::CollectionHandle::new(repo, self.#conn_field.clone())
            }
        };

        // Generate ensure_indexes method
        let entity_types: Vec<_> = self.attrs.entities.iter().map(|e| &e.type_name).collect();
        let ensure_indexes = quote! {
            /// Ensure all Redis indexes exist for the registered entity types.
            ///
            /// This creates FT.CREATE indexes for each entity that implements SearchEntity.
            /// It also registers all entity descriptors in the global registry for cascade operations.
            /// Call this at application startup to ensure all indexes are ready for queries.
            pub async fn ensure_indexes(&mut self) -> Result<(), ::snugom::errors::RepoError> {
                // First, register all entity descriptors in the global registry
                // This is required for cascade delete/update operations to work
                #(
                    <#entity_types as ::snugom::types::EntityMetadata>::ensure_registered();
                )*

                // Then ensure search indexes exist
                #(
                    {
                        use ::snugom::search::SearchEntity;
                        let definition = <#entity_types as SearchEntity>::index_definition(&self.#prefix_field);
                        ::snugom::search::ensure_index(&mut self.#conn_field, &definition).await?;
                    }
                )*
                Ok(())
            }
        };

        quote! {
            impl #name {
                #constructor

                #(#accessors)*

                #ensure_indexes
            }
        }
    }
}
