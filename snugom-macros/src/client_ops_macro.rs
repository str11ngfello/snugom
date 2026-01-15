//! Client-aware operation macros for Prisma-style ergonomics.
//!
//! These macros provide a clean syntax for complex operations:
//!
//! ```ignore
//! // Create with nested relations
//! snugom_create!(client, Guild {
//!     name: "Knights",
//!     members: [
//!         create GuildMember { user_id: "u1", role: Role::Leader },
//!     ],
//! });
//!
//! // Update with relation mutations
//! snugom_update!(client, Guild(entity_id = &id) {
//!     name: "New Name",
//! });
//! ```

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Expr, Ident, Path, Result, Token, braced};

use crate::snug_macro::SnugInvocation;

/// Parsed invocation for snugom_create! macro
pub struct ClientCreateInvocation {
    /// The client expression (e.g., `client` or `self.snugom`)
    pub client: Expr,
    /// The entity creation specification
    pub entity: SnugInvocation,
}

impl Parse for ClientCreateInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let entity: SnugInvocation = input.parse()?;

        Ok(Self { client, entity })
    }
}

impl ClientCreateInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = self.client;
        let entity_type = self.entity.entity_type();
        let builder_tokens = self.entity.emit()?;

        // Use `async` (not `async move`) so values are borrowed, not consumed.
        // This allows callers to use the same values after the await completes.
        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let mut __snugom_handle = __snugom_client.collection::<#entity_type>();
                let __snugom_builder = #builder_tokens;
                __snugom_handle.create(__snugom_builder).await
            }
        }})
    }
}

/// Parsed invocation for snugom_update! macro
pub struct ClientUpdateInvocation {
    /// The client expression
    pub client: Expr,
    /// The entity type
    pub entity_type: Path,
    /// The update specification
    pub update: SnugInvocation,
}

impl Parse for ClientUpdateInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let update: SnugInvocation = input.parse()?;

        // Extract entity type from the invocation
        let entity_type = update.entity_type().clone();

        Ok(Self {
            client,
            entity_type,
            update,
        })
    }
}

impl ClientUpdateInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = self.client;
        let entity_type = &self.entity_type;
        let patch_tokens = self.update.emit()?;

        // Use `async` (not `async move`) so values are borrowed, not consumed.
        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let mut __snugom_handle = __snugom_client.collection::<#entity_type>();
                let __snugom_patch = #patch_tokens;
                __snugom_handle.update(__snugom_patch).await
            }
        }})
    }
}

/// Parsed invocation for snugom_delete! macro with cascade support
pub struct ClientDeleteInvocation {
    /// The client expression
    pub client: Expr,
    /// The entity type
    pub entity_type: Path,
    /// The entity ID expression
    pub entity_id: Expr,
    /// Optional cascade specifications
    pub cascade_relations: Vec<Ident>,
}

impl Parse for ClientDeleteInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let entity_type: Path = input.parse()?;

        // Parse (id = expr) or just (expr)
        let content;
        syn::parenthesized!(content in input);

        let entity_id: Expr = if content.peek(Ident) && content.peek2(Token![=]) {
            let _id_ident: Ident = content.parse()?;
            content.parse::<Token![=]>()?;
            content.parse()?
        } else {
            content.parse()?
        };

        // Parse optional cascade block
        let mut cascade_relations = Vec::new();
        if input.peek(syn::token::Brace) {
            let cascade_content;
            braced!(cascade_content in input);

            while !cascade_content.is_empty() {
                let relation_name: Ident = cascade_content.parse()?;
                cascade_content.parse::<Token![:]>()?;
                let cascade_keyword: Ident = cascade_content.parse()?;

                if cascade_keyword != "cascade" {
                    return Err(syn::Error::new(
                        cascade_keyword.span(),
                        "expected `cascade`",
                    ));
                }

                cascade_relations.push(relation_name);

                // Optional trailing comma
                if cascade_content.peek(Token![,]) {
                    cascade_content.parse::<Token![,]>()?;
                }
            }
        }

        Ok(Self {
            client,
            entity_type,
            entity_id,
            cascade_relations,
        })
    }
}

impl ClientDeleteInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = self.client;
        let entity_id = self.entity_id;
        let entity_type = self.entity_type;

        // For now, cascade is a TODO - we'll delete the main entity
        // Full cascade support would require relation metadata
        if !self.cascade_relations.is_empty() {
            // Generate cascade delete code
            let _cascade_relations = &self.cascade_relations;
            // TODO: Implement cascade delete using relation metadata
        }

        // Use `async` (not `async move`) so values are borrowed, not consumed.
        Ok(quote! {{
            let __snugom_client = &mut #client;
            async {
                let __snugom_id: String = ::std::convert::Into::into(#entity_id);
                __snugom_client.collection::<#entity_type>().delete(&__snugom_id).await
            }
        }})
    }
}

/// Parsed invocation for snugom_upsert! macro
pub struct ClientUpsertInvocation {
    /// The client expression
    pub client: Expr,
    /// The create specification
    pub create: SnugInvocation,
    /// The update specification
    pub update: SnugInvocation,
}

impl Parse for ClientUpsertInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let entity_type: Path = input.parse()?;

        // Parse (id = expr)
        let id_content;
        syn::parenthesized!(id_content in input);
        let _id_ident: Ident = id_content.parse()?;
        id_content.parse::<Token![=]>()?;
        let _entity_id: Expr = id_content.parse()?;

        // Parse { create: ..., update: ... }
        let content;
        braced!(content in input);

        let mut create_invocation: Option<SnugInvocation> = None;
        let mut update_invocation: Option<SnugInvocation> = None;

        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;

            match key.to_string().as_str() {
                "create" => {
                    create_invocation = Some(content.parse()?);
                }
                "update" => {
                    update_invocation = Some(content.parse()?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown upsert section `{other}`"),
                    ));
                }
            }

            // Optional trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        let create = create_invocation.ok_or_else(|| {
            syn::Error::new(entity_type.span(), "upsert requires `create` section")
        })?;
        let update = update_invocation.ok_or_else(|| {
            syn::Error::new(entity_type.span(), "upsert requires `update` section")
        })?;

        Ok(Self {
            client,
            create,
            update,
        })
    }
}

impl ClientUpsertInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = self.client;
        let entity_type = self.create.entity_type();
        let create_tokens = self.create.emit()?;
        let update_tokens = self.update.emit()?;

        // Use `async` (not `async move`) so values are borrowed, not consumed.
        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let mut __snugom_handle = __snugom_client.collection::<#entity_type>();
                let __snugom_create = #create_tokens;
                let __snugom_update = #update_tokens;
                __snugom_handle.upsert(__snugom_create, __snugom_update).await
            }
        }})
    }
}

/// Parsed invocation for snugom_get_or_create! macro
pub struct ClientGetOrCreateInvocation {
    /// The client expression
    pub client: Expr,
    /// The create specification
    pub create: SnugInvocation,
}

impl Parse for ClientGetOrCreateInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let create: SnugInvocation = input.parse()?;

        Ok(Self { client, create })
    }
}

impl ClientGetOrCreateInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = self.client;
        let entity_type = self.create.entity_type();
        let create_tokens = self.create.emit()?;

        // Use `async` (not `async move`) so values are borrowed, not consumed.
        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let mut __snugom_handle = __snugom_client.collection::<#entity_type>();
                let __snugom_create = #create_tokens;
                __snugom_handle.get_or_create(__snugom_create).await
            }
        }})
    }
}
