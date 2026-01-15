//! Source file updates for schema versions and migrations/mod.rs.

use anyhow::{Context, Result};
use std::path::Path;

/// Update the schema version in a source file.
///
/// Finds the `#[snugom(schema = N)]` attribute
/// and updates it to the new version. If no such attribute exists, adds one.
pub fn update_source_schema_version(
    source_path: &Path,
    entity_name: &str,
    old_version: Option<u32>,
    new_version: u32,
) -> Result<bool> {
    let content = std::fs::read_to_string(source_path)
        .with_context(|| format!("Failed to read source file: {}", source_path.display()))?;

    // Find the struct definition
    let struct_pattern = format!("struct {entity_name}");
    let Some(struct_pos) = content.find(&struct_pattern) else {
        return Ok(false); // Struct not found
    };

    // Look for #[snugom(...)] before the struct
    let before_struct = &content[..struct_pos];

    // Find the last #[snugom or #[derive before struct
    let snugom_attr_start = before_struct.rfind("#[snugom(");
    let derive_attr_start = before_struct.rfind("#[derive(");

    let mut new_content = content.clone();
    let mut updated = false;

    if let Some(snugom_start) = snugom_attr_start {
        // Find the closing bracket
        let snugom_content = &before_struct[snugom_start..];
        if let Some(end_offset) = snugom_content.find(")]") {
            let attr_end = snugom_start + end_offset + 2;
            let attr_content = &before_struct[snugom_start..attr_end];

            // Check if it has schema
            if let Some(schema_match) = find_schema_in_attr(attr_content) {
                // Replace the schema number
                let old_attr = attr_content;
                let new_attr = attr_content.replace(
                    &schema_match,
                    &schema_match.replace(
                        &format!("= {}", old_version.unwrap_or(1)),
                        &format!("= {new_version}"),
                    ),
                );

                let new_attr = if new_attr == old_attr {
                    replace_schema_value(attr_content, new_version)
                } else {
                    new_attr
                };

                new_content = content.replacen(old_attr, &new_attr, 1);
                updated = true;
            } else {
                // No version in existing #[snugom(...)], add it
                // Insert "schema = N, " after "#[snugom("
                let insert_pos = snugom_start + 9; // len of "#[snugom("
                new_content = format!(
                    "{}schema = {}, {}",
                    &content[..insert_pos],
                    new_version,
                    &content[insert_pos..]
                );
                updated = true;
            }
        }
    } else if let Some(derive_start) = derive_attr_start {
        // No #[snugom(...)] attribute, need to add one after #[derive(...)]
        let derive_content = &before_struct[derive_start..];
        if let Some(end_offset) = derive_content.find(")]") {
            let derive_end = derive_start + end_offset + 2;
            // Insert new attribute after derive
            let insertion = format!("\n#[snugom(schema = {new_version})]");
            new_content = format!(
                "{}{}{}",
                &content[..derive_end],
                insertion,
                &content[derive_end..]
            );
            updated = true;
        }
    }

    if updated {
        std::fs::write(source_path, new_content)
            .with_context(|| format!("Failed to write source file: {}", source_path.display()))?;
    }

    Ok(updated)
}

/// Find schema attribute value in an attribute string
fn find_schema_in_attr(attr: &str) -> Option<String> {
    let pattern = "schema = ";
    if let Some(pos) = attr.find(pattern) {
        let start = pos + pattern.len();
        let rest = &attr[start..];
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if num_end > 0 {
            return Some(format!("{}{}", pattern, &rest[..num_end]));
        }
    }
    None
}

/// Replace schema value in attribute string
fn replace_schema_value(attr: &str, new_schema: u32) -> String {
    let pattern = "schema = ";
    if let Some(pos) = attr.find(pattern) {
        let start = pos + pattern.len();
        let rest = &attr[start..];
        let num_end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if num_end > 0 {
            return format!(
                "{}schema = {}{}",
                &attr[..pos],
                new_schema,
                &rest[num_end..]
            );
        }
    }
    attr.to_string()
}

/// Update the migrations/mod.rs file to include a new migration.
pub fn update_migrations_mod(migrations_dir: &Path, module_name: &str) -> Result<()> {
    let mod_path = migrations_dir.join("mod.rs");

    let content = if mod_path.exists() {
        std::fs::read_to_string(&mod_path)
            .with_context(|| format!("Failed to read {}", mod_path.display()))?
    } else {
        // Create initial mod.rs content
        r#"//! Generated migrations module.
//!
//! This file is auto-updated by `snugom migrate`.
//! Do not edit the module declarations manually.

"#
        .to_string()
    };

    // Check if module is already registered
    let mod_decl = format!("mod {module_name};");
    if content.contains(&mod_decl) {
        return Ok(()); // Already registered
    }

    // Find where to insert the new mod declaration
    // We want to insert after any existing mod declarations
    let mut insert_pos = content.len();
    let mut last_mod_end = 0;

    for (i, line) in content.lines().enumerate() {
        if line.trim().starts_with("mod ") && line.contains(';') {
            // Track the end of the last mod line
            last_mod_end = content
                .lines()
                .take(i + 1)
                .map(|l| l.len() + 1)
                .sum::<usize>();
        }
    }

    if last_mod_end > 0 {
        insert_pos = last_mod_end;
    } else {
        // No existing mod declarations, add after header comments
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
                insert_pos = content
                    .lines()
                    .take(i)
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                break;
            }
        }
    }

    // Build new content
    let new_content = format!(
        "{}{}\n{}",
        &content[..insert_pos],
        mod_decl,
        &content[insert_pos..]
    );

    std::fs::write(&mod_path, new_content)
        .with_context(|| format!("Failed to write {}", mod_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_schema_in_attr() {
        assert_eq!(
            find_schema_in_attr("#[snugom(schema = 1)]"),
            Some("schema = 1".to_string())
        );
        assert_eq!(
            find_schema_in_attr("#[snugom(schema = 2)]"),
            Some("schema = 2".to_string())
        );
        assert_eq!(
            find_schema_in_attr("#[snugom(id, schema = 3)]"),
            Some("schema = 3".to_string())
        );
        assert_eq!(find_schema_in_attr("#[snugom(id)]"), None);
    }

    #[test]
    fn test_replace_schema_value() {
        assert_eq!(
            replace_schema_value("#[snugom(schema = 1)]", 2),
            "#[snugom(schema = 2)]"
        );
        assert_eq!(
            replace_schema_value("#[snugom(schema = 1, other)]", 3),
            "#[snugom(schema = 3, other)]"
        );
    }
}
