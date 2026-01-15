//! Code generation for migration files.

use chrono::{DateTime, Utc};
use std::fmt::Write;

use crate::differ::{ChangeType, EntityChange, EntityDiff, FieldChange, MigrationComplexity};

/// Generated migration file
pub struct MigrationFile {
    /// Filename (without path)
    pub filename: String,
    /// Module name (valid Rust identifier)
    pub module_name: String,
    /// Full file content
    pub content: String,
    /// Migration complexity
    pub complexity: MigrationComplexity,
}

/// Generate a migration file from entity diffs.
///
/// Returns the generated migration file with filename and content.
pub fn generate_migration_file(name: &str, diffs: &[EntityDiff], timestamp: DateTime<Utc>) -> MigrationFile {
    let date_str = timestamp.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("{date_str}_{name}.rs");
    let module_name = format!("_{date_str}_{name}");

    // Determine overall complexity
    let complexity = diffs
        .iter()
        .map(|d| d.complexity)
        .max_by_key(|c| complexity_order(*c))
        .unwrap_or(MigrationComplexity::MetadataOnly);

    let content = generate_content(name, diffs, timestamp, complexity);

    MigrationFile {
        filename,
        module_name,
        content,
        complexity,
    }
}

fn complexity_order(c: MigrationComplexity) -> u8 {
    match c {
        MigrationComplexity::MetadataOnly => 0,
        MigrationComplexity::Baseline => 1,
        MigrationComplexity::Auto => 2,
        MigrationComplexity::Stub => 3,
        MigrationComplexity::Complex => 4,
    }
}

fn generate_content(
    name: &str,
    diffs: &[EntityDiff],
    timestamp: DateTime<Utc>,
    complexity: MigrationComplexity,
) -> String {
    let mut content = String::new();

    // File header
    let _ = writeln!(content, "// src/migrations/{name}.rs");
    let _ = writeln!(content, "//");
    let _ = writeln!(content, "// Migration: {name}");
    let _ = writeln!(content, "// Generated: {}", timestamp.format("%Y-%m-%dT%H:%M:%SZ"));
    let _ = writeln!(content, "//");

    // Complexity warning if needed
    match complexity {
        MigrationComplexity::Stub => {
            let _ = writeln!(content, "// ⚠ IMPLEMENTATION REQUIRED");
            let _ = writeln!(content, "//");
        }
        MigrationComplexity::Complex => {
            let _ = writeln!(content, "// ⚠ COMPLEX MIGRATION - Review carefully");
            let _ = writeln!(content, "//");
        }
        _ => {}
    }

    // Document changes
    let _ = writeln!(content, "// Changes:");
    for diff in diffs {
        if diff.is_new() {
            let _ = writeln!(content, "//   {} (NEW - baseline v{})", diff.entity, diff.new_version);
        } else if let Some(old_v) = diff.old_version {
            let _ = writeln!(content, "//   {} (v{} → v{}):", diff.entity, old_v, diff.new_version);
            for change in &diff.changes {
                let _ = writeln!(content, "//     {}", format_change(change));
            }
        }
    }
    let _ = writeln!(content, "//");

    // Add complexity note
    let _ = writeln!(content, "// Migration type: {complexity}");
    let _ = writeln!(content);

    // Imports
    let _ = writeln!(content, "#![allow(unused_imports)]");
    let _ = writeln!(content, "#![allow(dead_code)]");
    let _ = writeln!(content);
    let _ = writeln!(content, "use serde_json::json;");
    let _ = writeln!(content);

    // Generate the register function
    let _ = writeln!(content, "/// Register this migration.");
    let _ = writeln!(content, "///");
    let _ = writeln!(content, "/// Called by the migration runner to set up transforms.");
    let _ = writeln!(content, "pub fn register() {{");

    // Check if this is just baseline registrations (new entities)
    let all_new = diffs.iter().all(|d| d.is_new());
    let all_metadata_only = diffs.iter().all(|d| !d.has_changes() && !d.is_new());

    if all_new {
        let _ = writeln!(content, "    // Initial baseline migration - no transforms needed.");
        let _ = writeln!(
            content,
            "    // All documents created after this point will be at the new schema version."
        );
        for diff in diffs {
            if let Some(ref collection) = diff.collection {
                let _ = writeln!(
                    content,
                    "    // {} -> collection \"{}\" at schema v{}",
                    diff.entity, collection, diff.new_version
                );
            }
        }
    } else if all_metadata_only {
        let _ = writeln!(content, "    // Metadata-only migration - no document transforms needed.");
        for diff in diffs {
            if let Some(old_v) = diff.old_version {
                let _ = writeln!(
                    content,
                    "    // {} v{} → v{} (no data changes)",
                    diff.entity, old_v, diff.new_version
                );
            }
        }
    } else {
        // Generate transforms for each entity with changes
        for diff in diffs {
            if diff.is_new() {
                continue;
            }
            if !diff.has_changes() {
                continue;
            }

            let collection = diff.collection.as_deref().unwrap_or("unknown");
            let old_v = diff.old_version.unwrap_or(0);

            let _ = writeln!(content);
            let _ = writeln!(
                content,
                "    // {} (collection: \"{}\", v{} → v{})",
                diff.entity, collection, old_v, diff.new_version
            );

            match diff.complexity {
                MigrationComplexity::Auto => {
                    generate_auto_transform(&mut content, diff);
                }
                MigrationComplexity::Stub => {
                    generate_stub_transform(&mut content, diff);
                }
                MigrationComplexity::MetadataOnly => {
                    let _ = writeln!(content, "    // No document changes required");
                }
                _ => {}
            }
        }
    }

    let _ = writeln!(content, "}}");
    let _ = writeln!(content);

    // Add transform function placeholder for non-trivial migrations
    if !all_new && !all_metadata_only {
        let _ = writeln!(content, "/// Transform a single document.");
        let _ = writeln!(content, "///");
        let _ = writeln!(content, "/// This function is called for each document during migration.");
        let _ = writeln!(content, "#[allow(unused_variables)]");
        let _ = writeln!(
            content,
            "fn transform(mut doc: serde_json::Value) -> Result<serde_json::Value, String> {{"
        );

        // Generate field transforms
        for diff in diffs {
            if diff.is_new() || !diff.has_changes() {
                continue;
            }

            for change in &diff.changes {
                if let EntityChange::Field(fc) = change {
                    generate_field_transform(&mut content, fc, diff.complexity);
                }
            }
        }

        let _ = writeln!(content, "    Ok(doc)");
        let _ = writeln!(content, "}}");
    }

    content
}

fn format_change(change: &EntityChange) -> String {
    match change {
        EntityChange::Field(fc) => {
            let prefix = match fc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            if let Some(ref field) = fc.new_field {
                format!("{} {}: {}", prefix, fc.name, field.field_type)
            } else if let Some(ref field) = fc.old_field {
                format!("{} {}: {} (removed)", prefix, fc.name, field.field_type)
            } else {
                format!("{} {}", prefix, fc.name)
            }
        }
        EntityChange::Index(ic) => {
            let prefix = match ic.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            format!("{} index on {}", prefix, ic.field)
        }
        EntityChange::Relation(rc) => {
            let prefix = match rc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            if let Some(ref rel) = rc.new_relation {
                format!("{} relation {} -> {}", prefix, rc.field, rel.target)
            } else {
                format!("{} relation {}", prefix, rc.field)
            }
        }
        EntityChange::UniqueConstraint(uc) => {
            let prefix = match uc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            format!("{} unique({})", prefix, uc.fields.join(", "))
        }
    }
}

fn generate_auto_transform(content: &mut String, diff: &EntityDiff) {
    let _ = writeln!(content, "    // AUTO-GENERATED transforms:");

    for change in &diff.changes {
        if let EntityChange::Field(fc) = change {
            match fc.change_type {
                ChangeType::Added => {
                    if let Some(ref field) = fc.new_field {
                        let default = get_default_value(&field.field_type, field.serde_default.as_deref());
                        let _ = writeln!(content, "    //   doc[\"{}\"] = {};", fc.name, default);
                    }
                }
                ChangeType::Removed => {
                    let _ = writeln!(content, "    //   doc.as_object_mut().unwrap().remove(\"{}\");", fc.name);
                }
                _ => {}
            }
        }
    }
}

fn generate_stub_transform(content: &mut String, diff: &EntityDiff) {
    let _ = writeln!(content, "    // TODO: Implement migration logic");
    let _ = writeln!(content, "    //");
    let _ = writeln!(content, "    // The following changes require manual implementation:");

    for change in &diff.changes {
        if let EntityChange::Field(fc) = change {
            if fc.is_type_change() {
                if let (Some(old), Some(new)) = (&fc.old_field, &fc.new_field) {
                    let _ = writeln!(
                        content,
                        "    //   - {}: {} → {} (type change)",
                        fc.name, old.field_type, new.field_type
                    );
                }
            } else if fc.change_type == ChangeType::Added
                && let Some(ref field) = fc.new_field
                && !field.field_type.starts_with("Option<")
                && !field.field_type.starts_with("Vec<")
            {
                let _ = writeln!(
                    content,
                    "    //   - {}: {} (new required field - needs default or logic)",
                    fc.name, field.field_type
                );
            }
        }
    }

    let _ = writeln!(content, "    //");
    let _ = writeln!(content, "    // Example:");
    let _ = writeln!(
        content,
        "    //   let old_value = doc.get(\"old_field\").and_then(|v| v.as_str()).unwrap_or(\"\");"
    );
    let _ = writeln!(content, "    //   doc[\"new_field\"] = json!(convert(old_value));");
    let _ = writeln!(content, "    //   doc.as_object_mut().unwrap().remove(\"old_field\");");
}

fn generate_field_transform(content: &mut String, fc: &FieldChange, complexity: MigrationComplexity) {
    match fc.change_type {
        ChangeType::Added => {
            if let Some(ref field) = fc.new_field {
                let default = get_default_value(&field.field_type, field.serde_default.as_deref());
                if complexity == MigrationComplexity::Auto {
                    let _ = writeln!(content, "    doc[\"{}\"] = {};", fc.name, default);
                } else {
                    let _ = writeln!(content, "    // TODO: Set {} (type: {})", fc.name, field.field_type);
                    let _ = writeln!(content, "    // doc[\"{}\"] = todo!(\"set value\");", fc.name);
                }
            }
        }
        ChangeType::Removed => {
            let _ = writeln!(content, "    // ⚠️ DATA LOSS: Removing field '{}'", fc.name);
            let _ = writeln!(
                content,
                "    if let Some(obj) = doc.as_object_mut() {{ obj.remove(\"{}\"); }}",
                fc.name
            );
        }
        ChangeType::Modified => {
            if fc.is_type_change()
                && let (Some(old), Some(new)) = (&fc.old_field, &fc.new_field)
            {
                let _ = writeln!(
                    content,
                    "    // TODO: Convert '{}' from {} to {}",
                    fc.name, old.field_type, new.field_type
                );
                let _ = writeln!(content, "    // let old_value = doc.get(\"{}\").cloned();", fc.name);
                let _ = writeln!(content, "    // doc[\"{}\"] = todo!(\"convert value\");", fc.name);
            }
        }
    }
}

fn get_default_value(field_type: &str, serde_default: Option<&str>) -> String {
    // If serde default is specified, try to use it
    if let Some(default_fn) = serde_default {
        if default_fn == "Default::default" {
            // Use the type's default
        } else {
            // Custom default function - we can't evaluate it, use a placeholder
            return format!("json!(/* {} */)", default_fn);
        }
    }

    // Determine default based on type
    if field_type.starts_with("Option<") {
        return "json!(null)".to_string();
    }
    if field_type.starts_with("Vec<") {
        return "json!([])".to_string();
    }
    if field_type.starts_with("HashMap<") || field_type.starts_with("BTreeMap<") {
        return "json!({})".to_string();
    }

    match field_type {
        "String" => "json!(\"\")".to_string(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => "json!(0)".to_string(),
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "json!(0)".to_string(),
        "f32" | "f64" => "json!(0.0)".to_string(),
        "bool" => "json!(false)".to_string(),
        _ => {
            // Unknown type - might be an enum or custom type
            if field_type.chars().next().is_some_and(|c| c.is_uppercase()) {
                // Looks like an enum or struct - use null as placeholder
                format!("json!(null) /* TODO: default for {} */", field_type)
            } else {
                "json!(null)".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_default_value() {
        assert_eq!(get_default_value("String", None), "json!(\"\")");
        assert_eq!(get_default_value("i32", None), "json!(0)");
        assert_eq!(get_default_value("bool", None), "json!(false)");
        assert_eq!(get_default_value("Option<String>", None), "json!(null)");
        assert_eq!(get_default_value("Vec<String>", None), "json!([])");
    }

    #[test]
    fn test_complexity_order() {
        assert!(complexity_order(MigrationComplexity::Complex) > complexity_order(MigrationComplexity::Stub));
        assert!(complexity_order(MigrationComplexity::Stub) > complexity_order(MigrationComplexity::Auto));
        assert!(complexity_order(MigrationComplexity::Auto) > complexity_order(MigrationComplexity::Baseline));
    }
}
