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
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Expr, Ident, Path, Result, Token, braced, bracketed, parenthesized};

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

        Ok(Self {
            client,
            entity,
        })
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
                    return Err(syn::Error::new(cascade_keyword.span(), "expected `cascade`"));
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
                    return Err(syn::Error::new(key.span(), format!("unknown upsert section `{other}`")));
                }
            }

            // Optional trailing comma
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        let create =
            create_invocation.ok_or_else(|| syn::Error::new(entity_type.span(), "upsert requires `create` section"))?;
        let update =
            update_invocation.ok_or_else(|| syn::Error::new(entity_type.span(), "upsert requires `update` section"))?;

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

        Ok(Self {
            client,
            create,
        })
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

// ═══════════════════════════════════════════════════════════════════════════════
// snugom_find! — Read entity with includes
// ═══════════════════════════════════════════════════════════════════════════════

/// Parsed invocation for snugom_find! macro.
///
/// ```ignore
/// snugom_find!(client, Guild(&guild_id))
/// snugom_find!(client, Guild(&guild_id) {
///     members: [include GuildMember],
///     apps: [include GuildApplication { limit: 5 }],
/// })
/// ```
pub struct ClientFindInvocation {
    pub client: Expr,
    pub entity_type: Path,
    pub entity_id: Expr,
    pub includes: Vec<MacroFindInclude>,
}

/// A single include specification parsed from the DSL.
/// Supports recursive nesting.
pub struct MacroFindInclude {
    pub alias: Ident,
    pub child_type: Path,
    pub options: Vec<FindOption>,
    pub nested: Vec<MacroFindInclude>,
}

/// Query options for an include (limit, sort, filter, offset).
pub enum FindOption {
    Limit(Expr),
    Sort(Expr),
    Filter(Expr),
    Offset(Expr),
}

impl MacroFindInclude {
    fn has_options(&self) -> bool {
        !self.options.is_empty()
    }

    /// Parse the body inside `[include Type { ... }]` braces.
    /// Disambiguates options vs nested includes via lookahead:
    /// - `ident: [` → nested include
    /// - `ident: expr` → option
    fn parse_body(input: ParseStream, _child_type: Path) -> Result<(Vec<FindOption>, Vec<MacroFindInclude>)> {
        let mut options = Vec::new();
        let mut nested = Vec::new();

        while !input.is_empty() {
            let name: Ident = input.parse()?;
            input.parse::<Token![:]>()?;

            if input.peek(syn::token::Bracket) {
                // Nested include: alias: [include ChildType ...]
                let inner;
                bracketed!(inner in input);
                let op: Ident = inner.parse()?;
                if op != "include" {
                    return Err(syn::Error::new(op.span(), "expected `include`"));
                }
                let nested_type: Path = inner.parse()?;

                let (nested_opts, nested_nested) = if inner.peek(syn::token::Brace) {
                    let body;
                    braced!(body in inner);
                    Self::parse_body(&body, nested_type.clone())?
                } else {
                    (Vec::new(), Vec::new())
                };

                nested.push(MacroFindInclude {
                    alias: name,
                    child_type: nested_type,
                    options: nested_opts,
                    nested: nested_nested,
                });
            } else {
                // Option: limit, sort, filter, offset
                let name_str = name.to_string();
                match name_str.as_str() {
                    "limit" => options.push(FindOption::Limit(input.parse()?)),
                    "sort" => options.push(FindOption::Sort(input.parse()?)),
                    "filter" => options.push(FindOption::Filter(input.parse()?)),
                    "offset" => options.push(FindOption::Offset(input.parse()?)),
                    other => {
                        return Err(syn::Error::new(
                            name.span(),
                            format!("unknown include option `{other}`, expected limit, sort, filter, or offset"),
                        ));
                    }
                }
            }

            // Optional trailing comma
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok((options, nested))
    }
}

impl Parse for ClientFindInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let entity_type: Path = input.parse()?;

        // Parse (expr) for entity ID
        let id_content;
        parenthesized!(id_content in input);
        let entity_id: Expr = id_content.parse()?;

        // Parse optional includes block
        let mut includes = Vec::new();
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);

            while !content.is_empty() {
                let alias: Ident = content.parse()?;
                content.parse::<Token![:]>()?;

                // Parse [include Type ...]
                let inner;
                bracketed!(inner in content);
                let op: Ident = inner.parse()?;
                if op != "include" {
                    return Err(syn::Error::new(op.span(), "expected `include`"));
                }
                let child_type: Path = inner.parse()?;

                let (options, nested) = if inner.peek(syn::token::Brace) {
                    let body;
                    braced!(body in inner);
                    MacroFindInclude::parse_body(&body, child_type.clone())?
                } else {
                    (Vec::new(), Vec::new())
                };

                includes.push(MacroFindInclude {
                    alias,
                    child_type,
                    options,
                    nested,
                });

                // Optional trailing comma
                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
        }

        Ok(Self {
            client,
            entity_type,
            entity_id,
            includes,
        })
    }
}

impl ClientFindInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = &self.client;
        let entity_type = &self.entity_type;
        let entity_id = &self.entity_id;

        if self.includes.is_empty() {
            // No includes — simple get_or_error
            return Ok(quote! {{
                let __snugom_client = &#client;
                async {
                    let __snugom_id: String = ::std::convert::Into::into(#entity_id);
                    __snugom_client.collection::<#entity_type>().get_or_error(&__snugom_id).await
                }
            }});
        }

        // Separate simple includes (Lua path) from option includes (FT.SEARCH path)
        let simple_includes: Vec<&MacroFindInclude> =
            self.includes.iter().filter(|inc| !inc.has_options()).collect();
        let option_includes: Vec<&MacroFindInclude> =
            self.includes.iter().filter(|inc| inc.has_options()).collect();

        // Build FindIncludeSpec construction for simple includes (sent to Lua)
        let spec_tokens: Vec<TokenStream2> = simple_includes
            .iter()
            .map(|inc| Self::emit_include_spec(inc))
            .collect();

        // Build deserialization code for simple includes
        let simple_deser: Vec<TokenStream2> = simple_includes
            .iter()
            .enumerate()
            .map(|(idx, inc)| Self::emit_include_deser(inc, idx, quote! { __snugom_find_result }))
            .collect();

        let simple_vars: Vec<Ident> = simple_includes
            .iter()
            .enumerate()
            .map(|(idx, _)| format_ident!("__snugom_inc_{}", idx))
            .collect();

        // Build find_related calls for option includes
        let option_deser: Vec<TokenStream2> = option_includes
            .iter()
            .enumerate()
            .map(|(idx, inc)| Self::emit_option_include(inc, idx, entity_type))
            .collect();

        let option_vars: Vec<Ident> = option_includes
            .iter()
            .enumerate()
            .map(|(idx, _)| format_ident!("__snugom_opt_{}", idx))
            .collect();

        // Build the result tuple: (Parent, simple_0, simple_1, ..., opt_0, opt_1, ...)
        let all_vars: Vec<&Ident> = simple_vars.iter().chain(option_vars.iter()).collect();

        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let __snugom_id: String = ::std::convert::Into::into(#entity_id);

                // Build include specs for Lua script
                let __snugom_specs: Vec<::snugom::repository::FindIncludeSpec> = vec![#(#spec_tokens),*];

                // Execute atomic find via Lua
                let __snugom_repo = ::snugom::repository::Repo::<#entity_type>::new(
                    __snugom_client.prefix().to_string(),
                );
                let mut __snugom_conn = __snugom_client.connection();
                let __snugom_find_result = __snugom_repo.find_with_includes(
                    &mut __snugom_conn,
                    &__snugom_id,
                    __snugom_specs,
                ).await?;

                // Deserialize parent entity
                let __snugom_parent: #entity_type = __snugom_find_result.deserialize_entity()?;

                // Deserialize simple includes from Lua result
                #(#simple_deser)*

                // Execute option includes via find_related
                #(#option_deser)*

                Ok::<_, ::snugom::errors::RepoError>((__snugom_parent, #(#all_vars),*))
            }
        }})
    }

    /// Generate the `FindIncludeSpec` construction for a simple include (recursive).
    fn emit_include_spec(inc: &MacroFindInclude) -> TokenStream2 {
        let alias = inc.alias.to_string();
        let child_type = &inc.child_type;

        let nested_specs: Vec<TokenStream2> = inc.nested.iter().map(Self::emit_include_spec).collect();

        quote! {
            ::snugom::repository::FindIncludeSpec {
                alias: #alias.to_string(),
                target_collection: <#child_type as ::snugom::types::SnugomModel>::COLLECTION.to_string(),
                nested: vec![#(#nested_specs),*],
            }
        }
    }

    /// Recursively generate deserialization code for an include from FindResult.
    /// Creates a variable `__snugom_inc_{idx}` with the deserialized children.
    /// Handles arbitrary nesting depth — generates unique variable names per level.
    fn emit_include_deser(
        inc: &MacroFindInclude,
        idx: usize,
        result_var: TokenStream2,
    ) -> TokenStream2 {
        let var = format_ident!("__snugom_inc_{}", idx);
        Self::emit_children_deser(inc, &var, result_var, 0)
    }

    fn emit_children_deser(
        inc: &MacroFindInclude,
        var: &Ident,
        parent_var: TokenStream2,
        depth: usize,
    ) -> TokenStream2 {
        let alias = inc.alias.to_string();
        let child_type = &inc.child_type;
        let iter_var = format_ident!("__c_d{}_{}", depth, alias);
        let children_var = format_ident!("__cs_d{}_{}", depth, alias);
        let vec_var = format_ident!("__v_d{}_{}", depth, alias);

        if inc.nested.is_empty() {
            quote! {
                let #var: Vec<#child_type> = {
                    let #children_var = #parent_var.children(#alias);
                    let mut #vec_var = Vec::with_capacity(#children_var.len());
                    for #iter_var in #children_var {
                        #vec_var.push(#iter_var.deserialize::<#child_type>()?);
                    }
                    #vec_var
                };
            }
        } else {
            let nested_deser: Vec<TokenStream2> = inc
                .nested
                .iter()
                .enumerate()
                .map(|(nidx, nested)| {
                    let nested_var = format_ident!("__n_d{}_{}", depth, nidx);
                    Self::emit_children_deser(nested, &nested_var, quote! { #iter_var }, depth + 1)
                })
                .collect();

            let nested_vars: Vec<Ident> = inc
                .nested
                .iter()
                .enumerate()
                .map(|(nidx, _)| format_ident!("__n_d{}_{}", depth, nidx))
                .collect();

            quote! {
                let #var = {
                    let #children_var = #parent_var.children(#alias);
                    let mut #vec_var = Vec::with_capacity(#children_var.len());
                    for #iter_var in #children_var {
                        let __entity: #child_type = #iter_var.deserialize()?;
                        #(#nested_deser)*
                        #vec_var.push((__entity, #(#nested_vars),*));
                    }
                    #vec_var
                };
            }
        }
    }

    /// Generate a find_related call for an include with options.
    ///
    /// When the include also has nested includes, fetches children via FT.SEARCH
    /// then loads each child's nested relations via Lua find_with_includes.
    fn emit_option_include(
        inc: &MacroFindInclude,
        idx: usize,
        parent_type: &Path,
    ) -> TokenStream2 {
        let var = format_ident!("__snugom_opt_{}", idx);
        let child_type = &inc.child_type;

        let mut limit_token = quote! { None };
        let mut sort_token = quote! { None };
        let mut filter_token = quote! { None };
        let mut offset_token = quote! { None };

        for opt in &inc.options {
            match opt {
                FindOption::Limit(expr) => limit_token = quote! { Some(#expr as u32) },
                FindOption::Sort(expr) => sort_token = quote! { Some(::std::convert::Into::<String>::into(#expr)) },
                FindOption::Filter(expr) => filter_token = quote! { Some(::std::convert::Into::<String>::into(#expr)) },
                FindOption::Offset(expr) => offset_token = quote! { Some(#expr as u32) },
            }
        }

        let find_related_call = quote! {
            let __fk = ::snugom::types::resolve_foreign_key(
                &<#child_type as ::snugom::types::EntityMetadata>::entity_descriptor(),
                <#parent_type as ::snugom::types::SnugomModel>::COLLECTION,
            ).ok_or_else(|| ::snugom::errors::RepoError::Other {
                message: ::std::borrow::Cow::Borrowed("no foreign key found for include relation"),
            })?;
            let __opts = ::snugom::types::RelationQueryOptions {
                limit: #limit_token,
                sort: #sort_token,
                filter: #filter_token,
                offset: #offset_token,
            };
            let mut __handle = __snugom_client.collection::<#child_type>();
            let __rel_data = __handle.find_related(&__fk, &__snugom_id, __opts).await?;
        };

        if inc.nested.is_empty() {
            // No nested includes — return RelationData<Vec<ChildType>> directly
            quote! {
                let #var = {
                    #find_related_call
                    __rel_data
                };
            }
        } else {
            // Options + nested: FT.SEARCH for children, then Lua per child for nested
            let nested_specs: Vec<TokenStream2> = inc.nested.iter().map(Self::emit_include_spec).collect();

            let nested_deser: Vec<TokenStream2> = inc
                .nested
                .iter()
                .enumerate()
                .map(|(nidx, nested)| {
                    let nested_var = format_ident!("__opt_nested_{}", nidx);
                    let nested_alias = nested.alias.to_string();
                    let nested_type = &nested.child_type;
                    quote! {
                        let #nested_var: Vec<#nested_type> = {
                            let __nc = __child_find.children(#nested_alias);
                            let mut __nv = Vec::with_capacity(__nc.len());
                            for __n in __nc {
                                __nv.push(__n.deserialize::<#nested_type>()?);
                            }
                            __nv
                        };
                    }
                })
                .collect();

            let nested_vars: Vec<Ident> = inc
                .nested
                .iter()
                .enumerate()
                .map(|(nidx, _)| format_ident!("__opt_nested_{}", nidx))
                .collect();

            quote! {
                let #var = {
                    #find_related_call

                    let __nested_specs: Vec<::snugom::repository::FindIncludeSpec> = vec![#(#nested_specs),*];
                    let __child_repo = ::snugom::repository::Repo::<#child_type>::new(
                        __snugom_client.prefix().to_string(),
                    );

                    let mut __tuples = Vec::with_capacity(__rel_data.items.len());
                    for __child_entity in &__rel_data.items {
                        let __child_id = <#child_type as ::snugom::types::SnugomModel>::get_id(__child_entity);
                        let __child_find = __child_repo.find_with_includes(
                            &mut __snugom_conn,
                            &__child_id,
                            __nested_specs.clone(),
                        ).await?;

                        #(#nested_deser)*
                        __tuples.push((__child_find.deserialize_entity::<#child_type>()?, #(#nested_vars),*));
                    }

                    ::snugom::RelationData::with_metadata(
                        __tuples,
                        __rel_data.total.unwrap_or(0),
                        __rel_data.has_more.unwrap_or(false),
                    )
                };
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// snugom_find_many! — List query with includes
// ═══════════════════════════════════════════════════════════════════════════════

/// Parsed invocation for snugom_find_many! macro.
///
/// ```ignore
/// snugom_find_many!(client, Guild(filter = "status:eq:active", page_size = 10))
/// snugom_find_many!(client, Guild(filter = "status:eq:active") {
///     members: [include GuildMember],
/// })
/// ```
pub struct ClientFindManyInvocation {
    pub client: Expr,
    pub entity_type: Path,
    pub query_options: Vec<FindManyQueryOption>,
    pub includes: Vec<MacroFindInclude>,
}

pub enum FindManyQueryOption {
    Filter(Expr),
    PageSize(Expr),
    Page(Expr),
    SortBy(Expr),
    SortOrder(Expr),
    Q(Expr),
}

impl Parse for ClientFindManyInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let client: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let entity_type: Path = input.parse()?;

        // Parse query options in parentheses
        let opts_content;
        parenthesized!(opts_content in input);
        let mut query_options = Vec::new();
        while !opts_content.is_empty() {
            let name: Ident = opts_content.parse()?;
            opts_content.parse::<Token![=]>()?;
            let expr: Expr = opts_content.parse()?;

            match name.to_string().as_str() {
                "filter" => query_options.push(FindManyQueryOption::Filter(expr)),
                "page_size" => query_options.push(FindManyQueryOption::PageSize(expr)),
                "page" => query_options.push(FindManyQueryOption::Page(expr)),
                "sort_by" => query_options.push(FindManyQueryOption::SortBy(expr)),
                "sort_order" => query_options.push(FindManyQueryOption::SortOrder(expr)),
                "q" => query_options.push(FindManyQueryOption::Q(expr)),
                other => {
                    return Err(syn::Error::new(
                        name.span(),
                        format!("unknown query option `{other}`, expected filter, page_size, page, sort_by, sort_order, or q"),
                    ));
                }
            }

            if opts_content.peek(Token![,]) {
                opts_content.parse::<Token![,]>()?;
            }
        }

        // Parse optional includes block (same syntax as snugom_find!)
        let mut includes = Vec::new();
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);

            while !content.is_empty() {
                let alias: Ident = content.parse()?;
                content.parse::<Token![:]>()?;

                let inner;
                bracketed!(inner in content);
                let op: Ident = inner.parse()?;
                if op != "include" {
                    return Err(syn::Error::new(op.span(), "expected `include`"));
                }
                let child_type: Path = inner.parse()?;

                let (options, nested) = if inner.peek(syn::token::Brace) {
                    let body;
                    braced!(body in inner);
                    MacroFindInclude::parse_body(&body, child_type.clone())?
                } else {
                    (Vec::new(), Vec::new())
                };

                includes.push(MacroFindInclude {
                    alias,
                    child_type,
                    options,
                    nested,
                });

                if content.peek(Token![,]) {
                    content.parse::<Token![,]>()?;
                }
            }
        }

        Ok(Self {
            client,
            entity_type,
            query_options,
            includes,
        })
    }
}

impl ClientFindManyInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let client = &self.client;
        let entity_type = &self.entity_type;

        // Build SearchQuery from options
        let mut filter_tokens = Vec::new();
        let mut page_size_token = quote! { None };
        let mut page_token = quote! { None };
        let mut sort_by_token = quote! { None };
        let mut sort_order_token = quote! { None };
        let mut q_token = quote! { None };

        for opt in &self.query_options {
            match opt {
                FindManyQueryOption::Filter(expr) => {
                    filter_tokens.push(quote! { ::std::convert::Into::<String>::into(#expr) });
                }
                FindManyQueryOption::PageSize(expr) => {
                    page_size_token = quote! { Some(#expr as u64) };
                }
                FindManyQueryOption::Page(expr) => {
                    page_token = quote! { Some(#expr as u64) };
                }
                FindManyQueryOption::SortBy(expr) => {
                    sort_by_token = quote! { Some(::std::convert::Into::<String>::into(#expr)) };
                }
                FindManyQueryOption::SortOrder(expr) => {
                    sort_order_token = quote! { Some(#expr) };
                }
                FindManyQueryOption::Q(expr) => {
                    q_token = quote! { Some(::std::convert::Into::<String>::into(#expr)) };
                }
            }
        }

        if self.includes.is_empty() {
            // No includes — simple find_many
            return Ok(quote! {{
                let __snugom_client = &#client;
                async {
                    let __query = ::snugom::SearchQuery {
                        page: #page_token,
                        page_size: #page_size_token,
                        sort_by: #sort_by_token,
                        sort_order: #sort_order_token,
                        q: #q_token,
                        filter: vec![#(#filter_tokens),*],
                    };
                    __snugom_client.collection::<#entity_type>().find_many(__query).await
                }
            }});
        }

        // With includes — search for parents, then load includes per parent
        let simple_includes: Vec<&MacroFindInclude> =
            self.includes.iter().filter(|inc| !inc.has_options()).collect();

        let spec_tokens: Vec<TokenStream2> = simple_includes
            .iter()
            .map(|inc| ClientFindInvocation::emit_include_spec(inc))
            .collect();

        // For each parent, generate include deserialization
        let include_deser: Vec<TokenStream2> = self
            .includes
            .iter()
            .enumerate()
            .map(|(idx, inc)| {
                if inc.has_options() {
                    // Option include via find_related
                    ClientFindInvocation::emit_option_include(inc, idx, entity_type)
                } else {
                    // Simple include from Lua result
                    ClientFindInvocation::emit_include_deser(inc, idx, quote! { __find_result })
                }
            })
            .collect();

        let all_vars: Vec<Ident> = self
            .includes
            .iter()
            .enumerate()
            .map(|(idx, inc)| {
                if inc.has_options() {
                    format_ident!("__snugom_opt_{}", idx)
                } else {
                    format_ident!("__snugom_inc_{}", idx)
                }
            })
            .collect();

        Ok(quote! {{
            let __snugom_client = &#client;
            async {
                let __query = ::snugom::SearchQuery {
                    page: #page_token,
                    page_size: #page_size_token,
                    sort_by: #sort_by_token,
                    sort_order: #sort_order_token,
                    q: #q_token,
                    filter: vec![#(#filter_tokens),*],
                };
                let __search_result = __snugom_client.collection::<#entity_type>().find_many(__query).await?;

                let __snugom_repo = ::snugom::repository::Repo::<#entity_type>::new(
                    __snugom_client.prefix().to_string(),
                );
                let mut __snugom_conn = __snugom_client.connection();

                let __snugom_specs: Vec<::snugom::repository::FindIncludeSpec> = vec![#(#spec_tokens),*];

                let mut __items = Vec::with_capacity(__search_result.items.len());
                for __parent in &__search_result.items {
                    let __snugom_id = <#entity_type as ::snugom::types::SnugomModel>::get_id(__parent);

                    let __find_result = __snugom_repo.find_with_includes(
                        &mut __snugom_conn,
                        &__snugom_id,
                        __snugom_specs.clone(),
                    ).await?;

                    // Deserialize parent from Lua result (avoids Clone bound on entity type)
                    let __snugom_parent: #entity_type = __find_result.deserialize_entity()?;

                    #(#include_deser)*

                    __items.push((__snugom_parent, #(#all_vars),*));
                }

                Ok::<_, ::snugom::errors::RepoError>(::snugom::search::SearchResult {
                    items: __items,
                    total: __search_result.total,
                    page: __search_result.page,
                    page_size: __search_result.page_size,
                })
            }
        }})
    }
}
