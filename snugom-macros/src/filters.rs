use super::*;
use crate::parsed::{FieldInfo, FilterFieldType};

pub(crate) fn derive_searchable_filters(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;

    // Extract fields from struct
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return syn::Error::new_spanned(&input, "SearchableFilters only supports structs with named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new_spanned(&input, "SearchableFilters can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Parse all fields and their attributes
    let mut field_infos = Vec::new();

    for field in fields.iter() {
        let field_name = field.ident.as_ref().unwrap().to_string();

        // Determine field type from the type name
        let field_type = match &field.ty {
            Type::Path(TypePath { path, .. }) => {
                if let Some(segment) = path.segments.last() {
                    let type_name = segment.ident.to_string();
                    match type_name.as_str() {
                        "Tag" => FilterFieldType::Tag,
                        "Numeric" => FilterFieldType::Numeric,
                        "Text" => FilterFieldType::Text,
                        "Boolean" => FilterFieldType::Boolean,
                        unknown => {
                            return syn::Error::new_spanned(
                                &field.ty,
                                format!(
                                    "Unsupported filter field type '{}'. Supported types: Tag, Numeric, Text, Boolean",
                                    unknown
                                ),
                            )
                            .to_compile_error()
                            .into();
                        }
                    }
                } else {
                    return syn::Error::new_spanned(
                        &field.ty,
                        "Unable to determine field type. Expected one of: Tag, Numeric, Text, Boolean",
                    )
                    .to_compile_error()
                    .into();
                }
            }
            _ => {
                return syn::Error::new_spanned(
                    &field.ty,
                    "Field type must be a simple path type (Tag, Numeric, Text, or Boolean)",
                )
                .to_compile_error()
                .into();
            }
        };

        let mut normalizer = None;
        let mut aliases = Vec::new();

        // Parse filter attributes using syn 2.0 API
        for attr in &field.attrs {
            if attr.path().is_ident("filter") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("normalizer") {
                        let value = meta.value()?;
                        let s: syn::LitStr = value.parse()?;
                        normalizer = Some(s.value());
                    } else if meta.path.is_ident("alias") {
                        let value = meta.value()?;
                        let s: syn::LitStr = value.parse()?;
                        aliases.push(s.value());
                    }
                    Ok(())
                });
            }
        }

        field_infos.push(FieldInfo {
            name: field_name,
            field_type,
            normalizer,
            aliases,
        });
    }

    // Generate match arms for each field
    let mut match_arms = Vec::new();

    for field_info in &field_infos {
        let field_name = &field_info.name;
        let field_name_str = field_name.as_str();

        match field_info.field_type {
            FilterFieldType::Tag => {
                let arm = if let Some(normalizer) = &field_info.normalizer {
                    let normalizer_ident = syn::Ident::new(normalizer, proc_macro2::Span::call_site());
                    // Field with normalizer
                    quote! {
                        #field_name_str => {
                            if descriptor.operator != ::snugom::search::FilterOperator::Eq {
                                return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                    "{} filter only supports eq operator",
                                    #field_name_str
                                ) });
                            }
                            if descriptor.values.is_empty() {
                                return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                    "{} filter requires at least one value",
                                    #field_name_str
                                ) });
                            }
                            let mut values = Vec::with_capacity(descriptor.values.len());
                            for value in descriptor.values {
                                values.push(::snugom::filters::normalizers::#normalizer_ident(&value)?.to_string());
                            }
                            Ok(::snugom::search::FilterCondition::TagEquals {
                                field: #field_name_str.to_string(),
                                values,
                            })
                        }
                    }
                } else {
                    // No normalizer - require exact values
                    quote! {
                        #field_name_str => {
                            if descriptor.operator != ::snugom::search::FilterOperator::Eq {
                                return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                    "{} filter only supports eq operator",
                                    #field_name_str
                                ) });
                            }
                            if descriptor.values.is_empty() {
                                return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                    "{} filter requires at least one value",
                                    #field_name_str
                                ) });
                            }
                            Ok(::snugom::search::FilterCondition::TagEquals {
                                field: #field_name_str.to_string(),
                                values: descriptor.values,
                            })
                        }
                    }
                };
                match_arms.push(arm);

                // Add aliases as separate match arms
                for alias in &field_info.aliases {
                    let alias_arm = quote! {
                        #alias => ::snugom::filters::normalizers::build_numeric_filter(descriptor, #field_name_str)
                    };
                    match_arms.push(alias_arm);
                }
            }
            FilterFieldType::Numeric => {
                let arm = quote! {
                    #field_name_str => ::snugom::filters::normalizers::build_numeric_filter(descriptor, #field_name_str)
                };
                match_arms.push(arm);

                // Add aliases as separate match arms
                for alias in &field_info.aliases {
                    let alias_arm = quote! {
                        #alias => ::snugom::filters::normalizers::build_numeric_filter(descriptor, #field_name_str)
                    };
                    match_arms.push(alias_arm);
                }
            }
            FilterFieldType::Text => {
                // Text fields support prefix, contains, exact, fuzzy, and eq operators
                let arm = quote! {
                    #field_name_str => {
                        ::snugom::filters::normalizers::build_text_filter(descriptor, #field_name_str)
                    }
                };
                match_arms.push(arm);
            }
            FilterFieldType::Boolean => {
                // Boolean fields require exact "true" or "false"
                let arm = quote! {
                    #field_name_str => {
                        if descriptor.operator != ::snugom::search::FilterOperator::Eq {
                            return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                "{} filter only supports eq operator",
                                #field_name_str
                            ) });
                        }
                        let value = descriptor.values.get(0).ok_or_else(|| {
                            ::snugom::errors::RepoError::InvalidRequest { message: format!(
                                "{} filter requires a value",
                                #field_name_str
                            ) }
                        })?;

                        // Parse boolean value - require exact "true" or "false"
                        let bool_value = match value.trim() {
                            "true" => true,
                            "false" => false,
                            _ => return Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                                "Invalid boolean value for {}: {}. Must be exactly 'true' or 'false'",
                                #field_name_str,
                                value
                            ) })
                        };

                        Ok(::snugom::search::FilterCondition::BooleanEquals {
                            field: #field_name_str.to_string(),
                            value: bool_value,
                        })
                    }
                };
                match_arms.push(arm);
            }
            FilterFieldType::Geo => {
                // Geo fields - pass through for now (geo queries handled separately)
                let arm = quote! {
                    #field_name_str => {
                        Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                            "Geo filter for {} not yet implemented in SearchableFilters derive",
                            #field_name_str
                        ) })
                    }
                };
                match_arms.push(arm);
            }
        }
    }

    // Generate the implementation
    let expanded = quote! {
        impl #struct_name {
            /// Generated filter mapping implementation
            pub fn map_filter(descriptor: ::snugom::search::FilterDescriptor) -> Result<::snugom::search::FilterCondition, ::snugom::errors::RepoError> {
                // Match exact field names only (case-sensitive)
                match descriptor.field.as_str() {
                    #(#match_arms,)*
                    _ => Err(::snugom::errors::RepoError::InvalidRequest { message: format!(
                        "Unknown filter field: '{}'. Field names are case-sensitive.",
                        descriptor.field
                    ) })
                }
            }
        }
    };

    TokenStream::from(expanded)
}
