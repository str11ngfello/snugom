//! Rust source file parser using syn to extract SnugomEntity definitions.

use anyhow::{Context, Result};
use std::path::Path;
use syn::{Attribute, Field, Fields, GenericArgument, Ident, Lit, Meta, PathArguments, Type};

use super::schema::{
    CascadeStrategy, DateTimeFormat, EntitySchema, FieldInfo, FilterableType, IndexInfo, IndexType, RelationInfo,
    RelationKind, UniqueConstraint,
};

/// Parse a Rust file and extract all SnugomEntity definitions.
///
/// # Arguments
/// * `path` - Absolute path to the Rust source file
/// * `relative_path` - Path relative to project root for display/tracking
pub fn parse_entity_file(path: &Path, relative_path: &str) -> Result<Vec<EntitySchema>> {
    let content = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path.display()))?;

    // Parse the file with syn
    let syntax = syn::parse_file(&content).with_context(|| format!("Failed to parse Rust file: {}", path.display()))?;

    let mut schemas = Vec::new();

    // Track line numbers manually by counting newlines
    let lines: Vec<&str> = content.lines().collect();

    for item in syntax.items {
        if let syn::Item::Struct(item_struct) = item {
            // Check if this struct derives SnugomEntity
            if has_snugom_entity_derive(&item_struct.attrs) {
                // Find approximate line number by searching for struct definition
                let struct_name = item_struct.ident.to_string();
                let line_num = find_struct_line(&lines, &struct_name).unwrap_or(1);

                let schema = parse_struct(&item_struct, relative_path, line_num)?;
                schemas.push(schema);
            }
        }
    }

    Ok(schemas)
}

/// Find the line number where a struct is defined
fn find_struct_line(lines: &[&str], struct_name: &str) -> Option<usize> {
    let pattern = format!("struct {struct_name}");
    for (i, line) in lines.iter().enumerate() {
        if line.contains(&pattern) {
            return Some(i + 1); // 1-indexed
        }
    }
    None
}

/// Check if attributes include derive(SnugomEntity)
fn has_snugom_entity_derive(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("derive")
            && let Ok(Meta::List(list)) = attr.meta.clone().try_into() as Result<Meta, _>
        {
            let tokens = list.tokens.to_string();
            if tokens.contains("SnugomEntity") {
                return true;
            }
        }
    }
    false
}

/// Parse a struct definition into an EntitySchema
fn parse_struct(item: &syn::ItemStruct, relative_path: &str, line: usize) -> Result<EntitySchema> {
    let entity_name = item.ident.to_string();

    let mut schema = EntitySchema::new(entity_name.clone(), relative_path.to_string(), line);

    // Parse struct-level snugom attributes (includes collection from entity-level attribute)
    parse_struct_attrs(&item.attrs, &mut schema)?;

    // Parse fields
    if let Fields::Named(fields) = &item.fields {
        for field in &fields.named {
            if let Some(field_info) = parse_field(field)? {
                // Build index info from field
                if let Some(ref ft) = field_info.filterable {
                    schema.indexes.push(IndexInfo {
                        field: field_info.name.clone(),
                        index_type: IndexType::from(*ft),
                    });
                } else if field_info.sortable {
                    // Sortable numeric fields
                    schema.indexes.push(IndexInfo {
                        field: field_info.name.clone(),
                        index_type: IndexType::Numeric,
                    });
                }

                schema.fields.push(field_info);
            }
        }
    }

    Ok(schema)
}

/// Parse struct-level #[snugom(...)] attributes
fn parse_struct_attrs(attrs: &[Attribute], schema: &mut EntitySchema) -> Result<()> {
    for attr in attrs {
        if !attr.path().is_ident("snugom") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            // version = N
            if meta.path.is_ident("version") {
                let _eq: syn::Token![=] = meta.input.parse()?;
                let lit: Lit = meta.input.parse()?;
                if let Lit::Int(lit_int) = lit {
                    schema.schema = lit_int.base10_parse()?;
                }
                return Ok(());
            }

            // schema = N (alias for version)
            if meta.path.is_ident("schema") {
                let _eq: syn::Token![=] = meta.input.parse()?;
                let lit: Lit = meta.input.parse()?;
                if let Lit::Int(lit_int) = lit {
                    schema.schema = lit_int.base10_parse()?;
                }
                return Ok(());
            }

            // collection = "name" (entity-level collection attribute)
            if meta.path.is_ident("collection") {
                let _eq: syn::Token![=] = meta.input.parse()?;
                let lit: Lit = meta.input.parse()?;
                if let Lit::Str(lit_str) = lit {
                    schema.collection = Some(lit_str.value());
                }
                return Ok(());
            }

            // unique_together = ["field1", "field2"]
            if meta.path.is_ident("unique_together") {
                let _eq: syn::Token![=] = meta.input.parse()?;
                let content;
                syn::bracketed!(content in meta.input);

                let mut fields = Vec::new();
                while !content.is_empty() {
                    let lit: Lit = content.parse()?;
                    if let Lit::Str(lit_str) = lit {
                        fields.push(lit_str.value());
                    }
                    if content.peek(syn::Token![,]) {
                        let _: syn::Token![,] = content.parse()?;
                    }
                }

                if !fields.is_empty() {
                    schema.unique_constraints.push(UniqueConstraint {
                        fields,
                        case_insensitive: false,
                    });
                }
                return Ok(());
            }

            Ok(())
        })?;
    }

    Ok(())
}

/// Parse a field definition
fn parse_field(field: &Field) -> Result<Option<FieldInfo>> {
    let field_name = field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();

    if field_name.is_empty() {
        return Ok(None);
    }

    let field_type = type_to_string(&field.ty);
    let mut info = FieldInfo::new(field_name, field_type);

    // Parse snugom attributes on the field
    for attr in &field.attrs {
        if attr.path().is_ident("snugom") {
            parse_field_snugom_attr(attr, &mut info)?;
        } else if attr.path().is_ident("serde") {
            parse_field_serde_attr(attr, &mut info)?;
        }
    }

    // Check if this is a relation field (ends with _id or _ids)
    if info.name.ends_with("_id") || info.name.ends_with("_ids") {
        // Relation handling is done separately
    }

    Ok(Some(info))
}

/// Parse #[snugom(...)] attribute on a field
fn parse_field_snugom_attr(attr: &Attribute, info: &mut FieldInfo) -> Result<()> {
    attr.parse_nested_meta(|meta| {
        // id
        if meta.path.is_ident("id") {
            info.id = true;
            return Ok(());
        }

        // filterable or filterable(type)
        if meta.path.is_ident("filterable") {
            if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let filter_type: Ident = content.parse()?;
                info.filterable = Some(parse_filterable_type(&filter_type.to_string()));
            } else {
                // Default based on field type
                info.filterable = Some(infer_filterable_type(&info.field_type));
            }
            return Ok(());
        }

        // sortable
        if meta.path.is_ident("sortable") {
            info.sortable = true;
            return Ok(());
        }

        // unique or unique(case_insensitive)
        if meta.path.is_ident("unique") {
            info.unique = true;
            if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let modifier: Ident = content.parse()?;
                if modifier == "case_insensitive" {
                    info.unique_case_insensitive = true;
                }
            }
            return Ok(());
        }

        // datetime(format)
        if meta.path.is_ident("datetime") {
            if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let format: Ident = content.parse()?;
                info.datetime_format = Some(parse_datetime_format(&format.to_string()));
            } else {
                info.datetime_format = Some(DateTimeFormat::EpochMillis);
            }
            return Ok(());
        }

        // relation(...) - we capture but don't fully parse here
        // Relations are processed separately to get full context
        if meta.path.is_ident("relation") {
            // Skip the parenthesized content for now
            if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                // Just consume the tokens
                let _: proc_macro2::TokenStream = content.parse()?;
            }
            return Ok(());
        }

        // validate(...) - skip for now
        if meta.path.is_ident("validate") {
            if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: proc_macro2::TokenStream = content.parse()?;
            }
            return Ok(());
        }

        Ok(())
    })?;

    Ok(())
}

/// Parse #[serde(...)] attribute on a field
fn parse_field_serde_attr(attr: &Attribute, info: &mut FieldInfo) -> Result<()> {
    attr.parse_nested_meta(|meta| {
        // default or default = "function_name"
        if meta.path.is_ident("default") {
            if meta.input.peek(syn::Token![=]) {
                let _eq: syn::Token![=] = meta.input.parse()?;
                let lit: Lit = meta.input.parse()?;
                if let Lit::Str(lit_str) = lit {
                    info.serde_default = Some(lit_str.value());
                }
            } else {
                info.serde_default = Some("Default::default".to_string());
            }
        }
        Ok(())
    })?;

    Ok(())
}

/// Convert syn::Type to a string representation
fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|seg| {
                    let ident = seg.ident.to_string();
                    match &seg.arguments {
                        PathArguments::None => ident,
                        PathArguments::AngleBracketed(args) => {
                            let inner: Vec<String> = args
                                .args
                                .iter()
                                .filter_map(|arg| {
                                    if let GenericArgument::Type(inner_ty) = arg {
                                        Some(type_to_string(inner_ty))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if inner.is_empty() {
                                ident
                            } else {
                                format!("{}<{}>", ident, inner.join(", "))
                            }
                        }
                        PathArguments::Parenthesized(_) => ident,
                    }
                })
                .collect();
            segments.join("::")
        }
        _ => "unknown".to_string(),
    }
}

/// Parse filterable type string
fn parse_filterable_type(s: &str) -> FilterableType {
    match s.to_lowercase().as_str() {
        "tag" => FilterableType::Tag,
        "text" => FilterableType::Text,
        "numeric" => FilterableType::Numeric,
        "geo" => FilterableType::Geo,
        _ => FilterableType::Tag,
    }
}

/// Infer filterable type from field type
fn infer_filterable_type(field_type: &str) -> FilterableType {
    let ty = field_type.to_lowercase();
    if ty.contains("i8")
        || ty.contains("i16")
        || ty.contains("i32")
        || ty.contains("i64")
        || ty.contains("u8")
        || ty.contains("u16")
        || ty.contains("u32")
        || ty.contains("u64")
        || ty.contains("f32")
        || ty.contains("f64")
        || ty.contains("datetime")
    {
        FilterableType::Numeric
    } else {
        FilterableType::Tag
    }
}

/// Parse datetime format string (reserved for future format options)
fn parse_datetime_format(s: &str) -> DateTimeFormat {
    match s.to_lowercase().as_str() {
        "epoch_secs" => DateTimeFormat::EpochSecs,
        "iso8601" => DateTimeFormat::Iso8601,
        _ => DateTimeFormat::EpochMillis, // default
    }
}

/// Parse relation attributes from a field to extract RelationInfo.
/// This is a separate function for more complex relation parsing.
#[allow(dead_code)]
pub fn parse_relation_from_field(field: &Field) -> Option<RelationInfo> {
    let field_name = field.ident.as_ref()?.to_string();

    for attr in &field.attrs {
        if !attr.path().is_ident("snugom") {
            continue;
        }

        let mut has_relation = false;
        let mut target = None;
        let mut cascade = CascadeStrategy::default();
        let mut kind = RelationKind::BelongsTo;

        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("relation") {
                has_relation = true;

                if meta.input.peek(syn::token::Paren) {
                    let content;
                    syn::parenthesized!(content in meta.input);

                    // Parse relation options
                    while !content.is_empty() {
                        let ident: Ident = content.parse()?;
                        let ident_str = ident.to_string();

                        match ident_str.as_str() {
                            "target" => {
                                let _eq: syn::Token![=] = content.parse()?;
                                let lit: Lit = content.parse()?;
                                if let Lit::Str(s) = lit {
                                    target = Some(s.value());
                                }
                            }
                            "cascade" => {
                                let _eq: syn::Token![=] = content.parse()?;
                                let lit: Lit = content.parse()?;
                                if let Lit::Str(s) = lit {
                                    cascade = match s.value().as_str() {
                                        "delete" => CascadeStrategy::Delete,
                                        "restrict" => CascadeStrategy::Restrict,
                                        _ => CascadeStrategy::Detach,
                                    };
                                }
                            }
                            "many_to_many" => {
                                let _eq: syn::Token![=] = content.parse()?;
                                let lit: Lit = content.parse()?;
                                if let Lit::Str(s) = lit {
                                    target = Some(s.value());
                                    kind = RelationKind::ManyToMany;
                                }
                            }
                            "kind" => {
                                let _eq: syn::Token![=] = content.parse()?;
                                let lit: Lit = content.parse()?;
                                if let Lit::Str(s) = lit {
                                    kind = match s.value().as_str() {
                                        "has_many" => RelationKind::HasMany,
                                        "many_to_many" => RelationKind::ManyToMany,
                                        _ => RelationKind::BelongsTo,
                                    };
                                }
                            }
                            _ => {}
                        }

                        if content.peek(syn::Token![,]) {
                            let _: syn::Token![,] = content.parse()?;
                        }
                    }
                }
            }
            Ok(())
        });

        if has_relation {
            // If no target specified, infer from field name
            let inferred_target = target.unwrap_or_else(|| infer_relation_target(&field_name));

            return Some(RelationInfo {
                field: field_name,
                target: inferred_target,
                kind,
                cascade,
            });
        }
    }

    None
}

/// Infer relation target from field name.
/// e.g., "author_id" -> "author", "user_ids" -> "user"
#[allow(dead_code)]
fn infer_relation_target(field_name: &str) -> String {
    if field_name.ends_with("_ids") {
        field_name[..field_name.len() - 4].to_string()
    } else if field_name.ends_with("_id") {
        field_name[..field_name.len() - 3].to_string()
    } else {
        field_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_to_string() {
        let ty: Type = syn::parse_str("String").unwrap();
        assert_eq!(type_to_string(&ty), "String");

        let ty: Type = syn::parse_str("Option<String>").unwrap();
        assert_eq!(type_to_string(&ty), "Option<String>");

        let ty: Type = syn::parse_str("Vec<String>").unwrap();
        assert_eq!(type_to_string(&ty), "Vec<String>");

        let ty: Type = syn::parse_str("chrono::DateTime<Utc>").unwrap();
        assert_eq!(type_to_string(&ty), "chrono::DateTime<Utc>");
    }

    #[test]
    fn test_infer_relation_target() {
        assert_eq!(infer_relation_target("author_id"), "author");
        assert_eq!(infer_relation_target("user_ids"), "user");
        assert_eq!(infer_relation_target("name"), "name");
    }

    #[test]
    fn test_infer_filterable_type() {
        assert_eq!(infer_filterable_type("String"), FilterableType::Tag);
        assert_eq!(infer_filterable_type("i32"), FilterableType::Numeric);
        assert_eq!(infer_filterable_type("DateTime<Utc>"), FilterableType::Numeric);
    }
}
