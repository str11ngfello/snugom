use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Ident, LitStr, Path, Result};

struct IncludeField {
    field_name: Ident,
    alias: String,
    domain_type: Path,
}

struct ScalarField {
    field_name: Ident,
}

struct ParsedResponse {
    name: Ident,
    entity_type: Path,
    auto_from: bool,
    includes: Vec<IncludeField>,
    scalars: Vec<ScalarField>,
    has_relations_metadata: bool,
}

impl ParsedResponse {
    fn from_input(input: &DeriveInput) -> Result<Self> {
        let name = input.ident.clone();

        let mut entity_type: Option<Path> = None;
        let mut custom_from = false;
        for attr in &input.attrs {
            if attr.path().is_ident("snugom_response") {
                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("entity") {
                        let value = meta.value()?;
                        entity_type = Some(value.parse()?);
                    } else if meta.path.is_ident("custom_from") {
                        custom_from = true;
                    } else if meta.path.is_ident("auto_from") {
                        // Accepted for backwards compat but is now the default
                    }
                    Ok(())
                })?;
            }
        }
        let auto_from = !custom_from;

        let entity_type = entity_type.ok_or_else(|| {
            Error::new_spanned(&input.ident, "SnugomResponse requires #[snugom_response(entity = EntityType)]")
        })?;

        let fields = match &input.data {
            Data::Struct(data) => match &data.fields {
                Fields::Named(fields) => &fields.named,
                _ => return Err(Error::new_spanned(&input.ident, "SnugomResponse requires named fields")),
            },
            _ => return Err(Error::new_spanned(&input.ident, "SnugomResponse only works on structs")),
        };

        let mut includes = Vec::new();
        let mut scalars = Vec::new();
        let mut has_relations_metadata = false;

        for field in fields {
            let field_name = field.ident.clone().unwrap_or_else(|| Ident::new("_", proc_macro2::Span::call_site()));
            let is_include = field.attrs.iter().any(|a| a.path().is_ident("snugom_include"));
            let is_metadata = field_name == "relations_metadata";

            if is_metadata {
                has_relations_metadata = true;
            } else if !is_include {
                scalars.push(ScalarField { field_name: field_name.clone() });
            }

            for attr in &field.attrs {
                if attr.path().is_ident("snugom_include") {
                    let mut alias: Option<String> = None;
                    let mut from_type: Option<Path> = None;

                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("alias") {
                            let value = meta.value()?;
                            let lit: LitStr = value.parse()?;
                            alias = Some(lit.value());
                        } else if meta.path.is_ident("from") {
                            let value = meta.value()?;
                            from_type = Some(value.parse()?);
                        }
                        Ok(())
                    })?;

                    let field_name = field.ident.clone().ok_or_else(|| {
                        Error::new_spanned(field, "snugom_include requires a named field")
                    })?;

                    let alias = alias.unwrap_or_else(|| field_name.to_string());

                    let domain_type = from_type.ok_or_else(|| {
                        Error::new_spanned(attr, "snugom_include requires from = DomainType")
                    })?;

                    includes.push(IncludeField { field_name, alias, domain_type });
                }
            }
        }

        Ok(ParsedResponse { name, entity_type, auto_from, includes, scalars, has_relations_metadata })
    }

    fn emit(&self) -> TokenStream2 {
        let name = &self.name;
        let entity_type = &self.entity_type;

        let include_blocks: Vec<TokenStream2> = self.includes.iter().map(|inc| {
            let field_name = &inc.field_name;
            let alias = &inc.alias;
            let domain_type = &inc.domain_type;

            quote! {
                if result.includes.contains_key(#alias) || result.relation_metadata.contains_key(#alias) {
                    let __children = result.children(#alias);
                    let __items: Vec<_> = __children
                        .iter()
                        .map(|c| c.deserialize::<#domain_type>().map(::std::convert::Into::into))
                        .collect::<::std::result::Result<Vec<_>, _>>()?;
                    let __returned = __items.len();
                    response.#field_name = ::snugom::RelationState::Loaded(__items);
                    if let Some(__meta) = result.relation_metadata.get(#alias) {
                        response.relations_metadata.insert(
                            #alias.to_string(),
                            ::snugom::repository::RelationMetadataResponse {
                                total: __meta.total,
                                returned: __returned,
                                has_more: __meta.has_more,
                            },
                        );
                    }
                }
            }
        }).collect();

        let from_impl = if self.auto_from {
            let scalar_assignments: Vec<TokenStream2> = self.scalars.iter().map(|s| {
                let field_name = &s.field_name;
                quote! { #field_name: entity.#field_name, }
            }).collect();

            let include_defaults: Vec<TokenStream2> = self.includes.iter().map(|inc| {
                let field_name = &inc.field_name;
                quote! { #field_name: ::std::default::Default::default(), }
            }).collect();

            let metadata_default = if self.has_relations_metadata {
                quote! { relations_metadata: ::std::default::Default::default(), }
            } else {
                quote! {}
            };

            quote! {
                impl ::std::convert::From<#entity_type> for #name {
                    fn from(entity: #entity_type) -> Self {
                        Self {
                            #(#scalar_assignments)*
                            #(#include_defaults)*
                            #metadata_default
                        }
                    }
                }
            }
        } else {
            quote! {}
        };

        quote! {
            #from_impl

            impl #name {
                #[allow(unused_mut)]
                pub fn from_find_result(
                    result: ::snugom::repository::FindResult,
                ) -> ::std::result::Result<Self, ::snugom::errors::RepoError> {
                    let __entity: #entity_type = result.deserialize_entity()?;
                    let mut response = Self::from(__entity);
                    #(#include_blocks)*
                    Ok(response)
                }
            }
        }
    }
}

pub fn derive_snugom_response(input: DeriveInput) -> Result<TokenStream2> {
    let parsed = ParsedResponse::from_input(&input)?;
    Ok(parsed.emit())
}
