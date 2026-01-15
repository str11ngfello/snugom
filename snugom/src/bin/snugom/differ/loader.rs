//! Snapshot loading utilities.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::scanner::EntitySchema;

/// Load the latest snapshots for all entities from the schemas directory.
///
/// Returns a map of entity name -> latest EntitySchema.
/// For each entity, finds the highest version snapshot.
pub fn load_latest_snapshots(schemas_dir: &Path) -> Result<HashMap<String, EntitySchema>> {
    let mut snapshots: HashMap<String, EntitySchema> = HashMap::new();

    if !schemas_dir.exists() {
        return Ok(snapshots);
    }

    // Read all JSON files in the schemas directory
    let entries = std::fs::read_dir(schemas_dir)
        .with_context(|| format!("Failed to read schemas directory: {}", schemas_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Skip non-JSON files
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }

        // Parse the snapshot
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read snapshot: {}", path.display()))?;

        let schema: EntitySchema = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse snapshot: {}", path.display()))?;

        // Keep only the latest version for each entity
        let entity_name = schema.entity.clone();
        if let Some(existing) = snapshots.get(&entity_name) {
            if schema.schema > existing.schema {
                snapshots.insert(entity_name, schema);
            }
        } else {
            snapshots.insert(entity_name, schema);
        }
    }

    Ok(snapshots)
}

/// Load a specific version snapshot for an entity.
#[allow(dead_code)]
pub fn load_snapshot(schemas_dir: &Path, entity: &str, version: u32) -> Result<Option<EntitySchema>> {
    let snake_name = to_snake_case(entity);
    let filename = format!("{snake_name}_v{version}.json");
    let path = schemas_dir.join(&filename);

    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read snapshot: {}", path.display()))?;

    let schema: EntitySchema = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse snapshot: {}", path.display()))?;

    Ok(Some(schema))
}

/// List all snapshot versions for an entity.
#[allow(dead_code)]
pub fn list_snapshot_versions(schemas_dir: &Path, entity: &str) -> Result<Vec<u32>> {
    let mut versions = Vec::new();
    let snake_name = to_snake_case(entity);
    let prefix = format!("{snake_name}_v");

    if !schemas_dir.exists() {
        return Ok(versions);
    }

    let entries = std::fs::read_dir(schemas_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(filename) = path.file_name().and_then(|n| n.to_str())
            && filename.starts_with(&prefix) && filename.ends_with(".json") {
                // Extract version number
                let version_str = &filename[prefix.len()..filename.len() - 5];
                if let Ok(version) = version_str.parse::<u32>() {
                    versions.push(version);
                }
            }
    }

    versions.sort();
    Ok(versions)
}

/// Convert PascalCase to snake_case
#[allow(dead_code)]
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("GuildMember"), "guild_member");
    }
}
