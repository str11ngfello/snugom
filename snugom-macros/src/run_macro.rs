use super::SnugInvocation;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Path, Result, Token, parenthesized};

pub struct RunInvocation {
    repo: Expr,
    connection: Expr,
    operation: Operation,
}

#[allow(clippy::large_enum_variant)]
pub enum Operation {
    Create(SnugInvocation),
    Update(SnugInvocation),
    Delete(DeleteInvocation),
    Get(GetInvocation),
    Find(FindInvocation),
    Upsert(UpsertInvocation),
}

pub struct DeleteInvocation {
    entity_id: Expr,
    expected_version: Option<Expr>,
}

pub struct GetInvocation {
    entity_id: Expr,
}

pub struct FindInvocation {
    query: Expr,
}

pub struct UpsertInvocation {
    create_invocation: SnugInvocation,
    update_invocation: SnugInvocation,
}

impl Parse for RunInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let repo: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let connection: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let op_ident: Ident = input.parse()?;
        input.parse::<Token![=>]>()?;

        let operation = match op_ident.to_string().as_str() {
            "create" => Operation::Create(input.parse()?),
            "update" => Operation::Update(input.parse()?),
            "delete" => Operation::Delete(input.parse()?),
            "get" => Operation::Get(input.parse()?),
            "find" => Operation::Find(input.parse()?),
            "upsert" => Operation::Upsert(input.parse()?),
            other => return Err(syn::Error::new(op_ident.span(), format!("unsupported operation `{}`", other))),
        };

        Ok(Self {
            repo,
            connection,
            operation,
        })
    }
}

impl Parse for DeleteInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let _entity: Path = input.parse()?;
        let content;
        parenthesized!(content in input);
        let entity_id = parse_named_expr(&content, "entity_id")?;
        let expected_version = if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
            Some(parse_named_expr(&content, "expected_version")?)
        } else {
            None
        };
        Ok(Self {
            entity_id,
            expected_version,
        })
    }
}

impl Parse for GetInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let _entity: Path = input.parse()?;
        let content;
        parenthesized!(content in input);
        let entity_id = parse_named_expr(&content, "entity_id")?;
        Ok(Self { entity_id })
    }
}

impl Parse for FindInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let _entity: Path = input.parse()?;
        let content;
        parenthesized!(content in input);
        let query = parse_named_expr(&content, "query")?;
        Ok(Self { query })
    }
}

impl Parse for UpsertInvocation {
    fn parse(input: ParseStream) -> Result<Self> {
        let _entity: Path = input.parse()?;
        let options;
        parenthesized!(options in input);
        // entity_id/expected_version must be specified directly in the update invocation, so options are ignored
        if !options.is_empty() {
            return Err(options.error("upsert options are specified inside create/update sections"));
        }
        let content;
        syn::braced!(content in input);
        let mut create_invocation: Option<SnugInvocation> = None;
        let mut update_invocation: Option<SnugInvocation> = None;

        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "create" => {
                    let inv: SnugInvocation = content.parse()?;
                    create_invocation = Some(inv);
                }
                "update" => {
                    let inv: SnugInvocation = content.parse()?;
                    update_invocation = Some(inv);
                }
                other => return Err(syn::Error::new(key.span(), format!("unknown upsert section `{}`", other))),
            }
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            } else {
                break;
            }
        }

        let create_invocation =
            create_invocation.ok_or_else(|| syn::Error::new(content.span(), "upsert requires `create` section"))?;
        let update_invocation =
            update_invocation.ok_or_else(|| syn::Error::new(content.span(), "upsert requires `update` section"))?;

        Ok(Self {
            create_invocation,
            update_invocation,
        })
    }
}

fn parse_named_expr(content: &syn::parse::ParseBuffer<'_>, name: &str) -> Result<Expr> {
    let ident: Ident = content.parse()?;
    if ident != name {
        return Err(syn::Error::new(ident.span(), format!("expected `{}`", name)));
    }
    content.parse::<Token![=]>()?;
    content.parse()
}

impl RunInvocation {
    pub fn emit(self) -> Result<TokenStream2> {
        let repo = self.repo;
        let connection = self.connection;
        let body = match self.operation {
            Operation::Create(invocation) => emit_create(invocation)?,
            Operation::Update(invocation) => emit_update(invocation)?,
            Operation::Delete(invocation) => emit_delete(invocation)?,
            Operation::Get(invocation) => emit_get(invocation)?,
            Operation::Find(invocation) => emit_find(invocation)?,
            Operation::Upsert(invocation) => emit_upsert(invocation)?,
        };

        Ok(quote! {{
            let __snug_repo = #repo;
            let __snug_conn = &mut *#connection;
            #body
        }})
    }
}

fn emit_create(invocation: SnugInvocation) -> Result<TokenStream2> {
    let builder_tokens = invocation.emit()?;
    Ok(quote! {{
        let __snug_builder = #builder_tokens;
        __snug_repo.create_with_conn(__snug_conn, __snug_builder).await
    }})
}

fn emit_update(invocation: SnugInvocation) -> Result<TokenStream2> {
    let builder_tokens = invocation.emit()?;
    Ok(quote! {{
        let __snug_patch = #builder_tokens;
        __snug_repo.update_patch_with_conn(__snug_conn, __snug_patch).await
    }})
}

fn emit_delete(invocation: DeleteInvocation) -> Result<TokenStream2> {
    let entity_id = invocation.entity_id;
    let expected = invocation
        .expected_version
        .map(|expr| quote! { Some(#expr) })
        .unwrap_or_else(|| quote! { None });
    Ok(quote! {{
        let __snug_entity_id: ::std::string::String = ::std::convert::Into::into(#entity_id);
        __snug_repo.delete_with_conn(__snug_conn, &__snug_entity_id, #expected).await
    }})
}

fn emit_get(invocation: GetInvocation) -> Result<TokenStream2> {
    let entity_id = invocation.entity_id;
    Ok(quote! {{
        let __snug_entity_id: ::std::string::String = ::std::convert::Into::into(#entity_id);
        __snug_repo.get(__snug_conn, &__snug_entity_id).await
    }})
}

fn emit_find(invocation: FindInvocation) -> Result<TokenStream2> {
    let query = invocation.query;
    Ok(quote! {{
        let __snug_query: ::snugom::search::SearchQuery = #query;
        __snug_repo.search_with_query(__snug_conn, __snug_query).await
    }})
}

fn emit_upsert(invocation: UpsertInvocation) -> Result<TokenStream2> {
    let update_tokens = invocation.update_invocation.emit()?;
    let create_tokens = invocation.create_invocation.emit()?;
    Ok(quote! {{
        match __snug_repo.update_patch_with_conn(__snug_conn, #update_tokens).await {
            Ok(updated) => Ok(::snugom::UpsertResult::Updated(updated)),
            Err(::snugom::errors::RepoError::NotFound { .. }) => {
                let __snug_create_builder = #create_tokens;
                __snug_repo
                    .create_with_conn(__snug_conn, __snug_create_builder)
                    .await
                    .map(::snugom::UpsertResult::Created)
            }
            Err(err) => Err(err),
        }
    }})
}
