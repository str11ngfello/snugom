//! Source file scanner for discovering SnugomEntity derives.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use syn::{Attribute, Expr, ExprLit, Lit, Meta, MetaNameValue};
use walkdir::WalkDir;

/// Information about a discovered entity.
#[derive(Debug, Clone)]
pub struct EntityInfo {
    /// The struct name (e.g., "Guild")
    pub name: String,
    /// The module path where this entity is defined (e.g., "crate::guild")
    pub module_path: String,
}

/// Scan a directory recursively for Rust files containing SnugomEntity derives.
pub fn scan_directory(path: &Path, crate_name: &str) -> Result<Vec<EntityInfo>> {
    let mut entities = Vec::new();

    for entry in WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "rs")
                && !e.path().to_string_lossy().contains("/generated/")
                && !e.path().to_string_lossy().contains("/target/")
        })
    {
        let file_path = entry.path();
        if let Ok(file_entities) = scan_file(file_path, path, crate_name) {
            entities.extend(file_entities);
        }
    }

    Ok(entities)
}

/// Scan a single Rust file for SnugomEntity derives.
fn scan_file(file_path: &Path, base_path: &Path, crate_name: &str) -> Result<Vec<EntityInfo>> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    let syntax = syn::parse_file(&content)
        .with_context(|| format!("Failed to parse {}", file_path.display()))?;

    let module_path = compute_module_path(file_path, base_path, crate_name);

    let mut entities = Vec::new();

    for item in syntax.items {
        if let syn::Item::Struct(item_struct) = item
            && has_snugom_entity_derive(&item_struct.attrs)
                && let Some(entity) = extract_entity_info(&item_struct, &module_path) {
                    entities.push(entity);
                }
    }

    Ok(entities)
}

/// Check if a struct has #[derive(SnugomEntity)]
fn has_snugom_entity_derive(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("derive")
            && let Ok(nested) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for path in nested {
                    if path.is_ident("SnugomEntity") {
                        return true;
                    }
                }
            }
    }
    false
}

/// Extract entity information from a struct with SnugomEntity derive.
fn extract_entity_info(item: &syn::ItemStruct, module_path: &str) -> Option<EntityInfo> {
    let name = item.ident.to_string();
    let mut has_collection = false;
    let mut has_service = false;

    // Look for #[snugom(...)] attribute with collection and service
    for attr in &item.attrs {
        if attr.path().is_ident("snugom")
            && let Ok(nested) = attr.parse_args_with(
                syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
            ) {
                for meta in nested {
                    if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                        if path.is_ident("collection") {
                            if let Expr::Lit(ExprLit { lit: Lit::Str(_), .. }) = value {
                                has_collection = true;
                            }
                        } else if path.is_ident("service")
                            && let Expr::Lit(ExprLit { lit: Lit::Str(_), .. }) = value {
                                has_service = true;
                            }
                    }
                }
            }
    }

    // Both collection and service are required for a valid entity
    if has_collection && has_service {
        Some(EntityInfo {
            name,
            module_path: module_path.to_string(),
        })
    } else {
        None
    }
}

/// Compute the module path from a file path.
/// e.g., "src/guild/models/domain.rs" -> "crate::guild::models::domain"
fn compute_module_path(file_path: &Path, base_path: &Path, crate_name: &str) -> String {
    let relative = file_path
        .strip_prefix(base_path)
        .unwrap_or(file_path);

    let without_extension = relative.with_extension("");
    let mut parts: Vec<&str> = without_extension
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Remove "mod" or "lib" from the end if present
    if let Some(last) = parts.last()
        && (*last == "mod" || *last == "lib") {
            parts.pop();
        }

    if parts.is_empty() {
        crate_name.to_string()
    } else {
        format!("{}::{}", crate_name, parts.join("::"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_snugom_entity_derive() {
        let code = r#"
            #[derive(Debug, SnugomEntity, Clone)]
            #[snugom(schema = 1, service = "guild", collection = "guilds")]
            struct Guild {
                id: String,
            }
        "#;

        let syntax: syn::ItemStruct = syn::parse_str(code).unwrap();
        assert!(has_snugom_entity_derive(&syntax.attrs));
    }

    #[test]
    fn test_extract_entity_info() {
        let code = r#"
            #[derive(SnugomEntity)]
            #[snugom(schema = 1, service = "guild", collection = "guilds")]
            struct Guild {
                id: String,
            }
        "#;

        let syntax: syn::ItemStruct = syn::parse_str(code).unwrap();
        let info = extract_entity_info(&syntax, "crate::guild").unwrap();

        assert_eq!(info.name, "Guild");
        assert_eq!(info.module_path, "crate::guild");
    }

    #[test]
    fn test_extract_entity_info_missing_attributes() {
        let code = r#"
            #[derive(SnugomEntity)]
            #[snugom(schema = 1)]
            struct InvalidEntity {
                id: String,
            }
        "#;

        let syntax: syn::ItemStruct = syn::parse_str(code).unwrap();
        // Should return None because collection and service are missing
        assert!(extract_entity_info(&syntax, "crate::test").is_none());
    }
}
