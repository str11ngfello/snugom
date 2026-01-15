use std::collections::HashMap;

use anyhow::{Context, Result};
use clap::Subcommand;
use redis::aio::ConnectionManager;
use serde_json::Value;

use crate::context::ProjectContext;
use crate::differ::{diff_schemas, load_latest_snapshots, ChangeType, EntityChange};
use crate::examples::ExampleGroup;
use crate::output::OutputManager;
use crate::scanner::{discover_entities, parse_entity_file};

pub const EXAMPLES: &[ExampleGroup] = &[
    ExampleGroup {
        title: "Schema Status",
        commands: &[
            "snugom schema status              # Show all collections' schema distribution",
            "snugom schema status users        # Show schema distribution for users collection",
        ],
    },
    ExampleGroup {
        title: "Schema Diff",
        commands: &[
            "snugom schema diff                # Show pending changes for all entities",
            "snugom schema diff users          # Show pending changes for User entity",
        ],
    },
    ExampleGroup {
        title: "Schema Validation",
        commands: &[
            "snugom schema validate guilds --field name    # Check for duplicate values",
        ],
    },
];

#[derive(Subcommand)]
pub enum SchemaCommands {
    /// Show schema version distribution in Redis
    #[command(name = "status")]
    Status {
        /// Collection to check (optional, shows all if omitted)
        collection: Option<String>,
    },

    /// Show what changes would be included in the next migration
    #[command(name = "diff")]
    Diff {
        /// Entity/collection to check (optional, shows all if omitted)
        entity: Option<String>,
    },

    /// Validate data before adding constraints
    #[command(name = "validate")]
    Validate {
        /// Collection to validate
        collection: String,

        /// Field to check for uniqueness
        #[arg(long)]
        field: String,

        /// Perform case-insensitive check
        #[arg(long)]
        case_insensitive: bool,
    },
}

pub async fn handle_schema_commands(
    command: SchemaCommands,
    output: &OutputManager,
) -> Result<()> {
    let ctx = ProjectContext::find()?;

    if !ctx.is_initialized() {
        output.error("SnugOM is not initialized in this project.");
        output.info("Run 'snugom init' first to initialize.");
        anyhow::bail!("Project not initialized");
    }

    match command {
        SchemaCommands::Status { collection } => {
            handle_status(&ctx, collection.as_deref(), output).await?;
        }
        SchemaCommands::Diff { entity } => {
            handle_diff(&ctx, entity.as_deref(), output).await?;
        }
        SchemaCommands::Validate {
            collection,
            field,
            case_insensitive,
        } => {
            handle_validate(&ctx, &collection, &field, case_insensitive, output).await?;
        }
    }

    Ok(())
}

async fn handle_status(
    ctx: &ProjectContext,
    collection: Option<&str>,
    output: &OutputManager,
) -> Result<()> {
    output.heading("Schema Status");

    // Get Redis URL
    let redis_url = ctx.redis_url().context(
        "REDIS_URL environment variable not set. Set it to connect to Redis.",
    )?;

    // Connect to Redis
    output.progress("Connecting to Redis...");
    let client = redis::Client::open(redis_url.as_str())
        .context("Failed to create Redis client")?;
    let mut conn = ConnectionManager::new(client)
        .await
        .context("Failed to connect to Redis")?;
    output.clear_line();
    output.success("Connected to Redis");

    // Determine which collections to scan
    let collections_to_scan: Vec<String> = if let Some(coll) = collection {
        vec![coll.to_string()]
    } else {
        // Discover collections from entity files
        output.progress("Discovering collections from entities...");
        let discovered = discover_entities(&ctx.project_root)
            .context("Failed to discover entity files")?;

        let mut collections = Vec::new();
        for file in &discovered {
            if let Ok(schemas) = parse_entity_file(&file.path, &file.relative_path) {
                for schema in schemas {
                    // Use entity name as collection if not explicitly set
                    let coll_name = schema.collection.clone()
                        .unwrap_or_else(|| to_snake_case(&schema.entity));
                    collections.push(coll_name);
                }
            }
        }
        output.clear_line();

        if collections.is_empty() {
            output.warning("No SnugomEntity types found in project");
            output.info("Make sure your entities use #[derive(SnugomEntity)]");
            return Ok(());
        }

        collections
    };

    output.info(&format!(
        "Scanning {} collection(s)...",
        collections_to_scan.len()
    ));

    // Scan each collection
    let mut total_documents = 0u64;
    let mut total_with_version = 0u64;
    let mut total_without_version = 0u64;

    for coll in &collections_to_scan {
        output.heading(&format!("Collection: {coll}"));

        let stats = scan_collection_versions(&mut conn, coll, output).await?;

        total_documents += stats.total;
        total_with_version += stats.with_version;
        total_without_version += stats.without_version;

        if stats.total == 0 {
            output.info("  No documents found");
        } else {
            // Show version distribution
            let mut versions: Vec<_> = stats.by_version.iter().collect();
            versions.sort_by_key(|(v, _)| *v);

            for (version, count) in versions {
                let pct = (*count as f64 / stats.total as f64) * 100.0;
                output.bullet(&format!(
                    "v{version}: {count} document(s) ({pct:.1}%)"
                ));
            }

            if stats.without_version > 0 {
                let pct = (stats.without_version as f64 / stats.total as f64) * 100.0;
                output.warning(&format!(
                    "No version: {} document(s) ({:.1}%) - needs migration",
                    stats.without_version, pct
                ));
            }
        }
    }

    // Summary
    output.heading("Summary");
    output.bullet(&format!("Total documents scanned: {total_documents}"));
    if total_with_version > 0 {
        output.bullet(&format!("Documents with schema version: {total_with_version}"));
    }
    if total_without_version > 0 {
        output.warning(&format!(
            "Documents without schema version: {total_without_version}"
        ));
        output.info("Run 'snugom migrate deploy' to apply pending migrations");
    } else if total_documents > 0 {
        output.success("All documents have schema versions");
    }

    Ok(())
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
        } else {
            result.push(c);
        }
    }
    result
}

/// Statistics for a collection scan.
struct CollectionStats {
    total: u64,
    with_version: u64,
    without_version: u64,
    by_version: HashMap<u32, u64>,
}

/// Scan a collection and count documents by schema version.
async fn scan_collection_versions(
    conn: &mut ConnectionManager,
    collection: &str,
    output: &OutputManager,
) -> Result<CollectionStats> {
    let pattern = format!("{collection}:*");
    let mut stats = CollectionStats {
        total: 0,
        with_version: 0,
        without_version: 0,
        by_version: HashMap::new(),
    };

    let mut cursor: u64 = 0;
    let mut scanned = 0u64;

    loop {
        // Show progress every 100 keys
        if scanned > 0 && scanned.is_multiple_of(100) {
            output.progress(&format!("Scanned {scanned} keys..."));
        }

        let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(conn)
            .await
            .context("Failed to scan Redis keys")?;

        for key in &keys {
            scanned += 1;

            // Skip internal keys
            if key.starts_with("_snugom:") {
                continue;
            }

            // Get the schema version from the document
            let version_result: Option<String> = redis::cmd("JSON.GET")
                .arg(key)
                .arg("$.__schema_version")
                .query_async(conn)
                .await
                .unwrap_or(None);

            stats.total += 1;

            // JSON.GET returns an array like [1] or [null]
            if let Some(version_json) = version_result
                && let Ok(versions) = serde_json::from_str::<Vec<Value>>(&version_json)
                && let Some(Value::Number(n)) = versions.first()
                && let Some(v) = n.as_u64()
            {
                stats.with_version += 1;
                *stats.by_version.entry(v as u32).or_insert(0) += 1;
                continue;
            }

            // No valid version found
            stats.without_version += 1;
        }

        cursor = new_cursor;
        if cursor == 0 {
            break;
        }
    }

    if scanned > 0 {
        output.clear_line();
    }

    Ok(stats)
}

async fn handle_diff(
    ctx: &ProjectContext,
    entity_filter: Option<&str>,
    output: &OutputManager,
) -> Result<()> {
    output.heading("Schema Diff");

    // Step 1: Discover entities
    output.progress("Discovering SnugomEntity types...");
    let discovered = discover_entities(&ctx.project_root)
        .context("Failed to discover entity files")?;
    output.clear_line();

    if discovered.is_empty() {
        output.warning("No SnugomEntity types found in project");
        output.info("Make sure your entities use #[derive(SnugomEntity)]");
        return Ok(());
    }

    output.success(&format!("Found {} file(s) with SnugomEntity", discovered.len()));

    // Step 2: Parse entities
    output.progress("Parsing entity schemas...");
    let mut all_schemas = Vec::new();

    for file in &discovered {
        match parse_entity_file(&file.path, &file.relative_path) {
            Ok(schemas) => {
                for schema in schemas {
                    // Filter by entity name if specified
                    if entity_filter.is_none() || entity_filter == Some(&schema.entity) {
                        all_schemas.push(schema);
                    }
                }
            }
            Err(err) => {
                output.clear_line();
                output.warning(&format!(
                    "Failed to parse {}: {err}",
                    file.relative_path
                ));
            }
        }
    }
    output.clear_line();

    if all_schemas.is_empty() {
        if let Some(ent) = entity_filter {
            output.warning(&format!("Entity '{}' not found", ent));
        } else {
            output.warning("No parseable SnugomEntity structs found");
        }
        return Ok(());
    }

    output.success(&format!("Parsed {} entity schema(s)", all_schemas.len()));

    // Step 3: Load snapshots
    output.progress("Loading existing snapshots...");
    let existing_snapshots = load_latest_snapshots(&ctx.schemas_dir)
        .context("Failed to load existing snapshots")?;
    output.clear_line();

    output.info(&format!("Found {} existing snapshot(s)", existing_snapshots.len()));

    // Step 4: Compute diffs
    output.heading("Pending Changes");

    let mut has_changes = false;
    let mut new_entities = 0;
    let mut modified_entities = 0;

    for schema in &all_schemas {
        let old_snapshot = existing_snapshots.get(&schema.entity);
        let diff = diff_schemas(old_snapshot, schema);

        if diff.is_new() {
            has_changes = true;
            new_entities += 1;
            output.bullet(&format!(
                "{} (NEW) - will be baseline v{}",
                diff.entity, diff.new_version
            ));
            output.info(&format!("    Source: {}", diff.source_file));
        } else if diff.has_changes() {
            has_changes = true;
            modified_entities += 1;
            output.bullet(&format!(
                "{} (v{} → v{}) - {} change(s) [{}]",
                diff.entity,
                diff.old_version.unwrap_or(0),
                diff.new_version,
                diff.changes.len(),
                diff.complexity
            ));

            // Show individual changes
            for change in &diff.changes {
                let change_str = format_change_detail(change);
                output.info(&format!("    {change_str}"));
            }
        } else {
            output.info(&format!("  {} - no changes", diff.entity));
        }
    }

    // Summary
    output.heading("Summary");

    if !has_changes {
        output.success("All entities are up to date with their snapshots");
        output.info("No migration needed");
    } else {
        if new_entities > 0 {
            output.bullet(&format!("{} new entity/entities", new_entities));
        }
        if modified_entities > 0 {
            output.bullet(&format!("{} modified entity/entities", modified_entities));
        }
        output.info("Run 'snugom migrate create --name <name>' to generate a migration");
    }

    Ok(())
}

/// Format a change for detailed display
fn format_change_detail(change: &EntityChange) -> String {
    match change {
        EntityChange::Field(fc) => {
            let prefix = match fc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            if let Some(ref field) = fc.new_field {
                format!("{prefix} field {}: {}", fc.name, field.field_type)
            } else if let Some(ref field) = fc.old_field {
                format!("{prefix} field {}: {} (removed)", fc.name, field.field_type)
            } else {
                format!("{prefix} field {}", fc.name)
            }
        }
        EntityChange::Index(ic) => {
            let prefix = match ic.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            format!("{prefix} index on {}", ic.field)
        }
        EntityChange::Relation(rc) => {
            let prefix = match rc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            if let Some(ref rel) = rc.new_relation {
                format!("{prefix} relation {} -> {}", rc.field, rel.target)
            } else {
                format!("{prefix} relation {}", rc.field)
            }
        }
        EntityChange::UniqueConstraint(uc) => {
            let prefix = match uc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            format!("{prefix} unique({})", uc.fields.join(", "))
        }
    }
}

async fn handle_validate(
    ctx: &ProjectContext,
    collection: &str,
    field: &str,
    case_insensitive: bool,
    output: &OutputManager,
) -> Result<()> {
    output.heading(&format!("Validate Uniqueness: {collection}.{field}"));

    // Get Redis URL
    let redis_url = ctx.redis_url().context(
        "REDIS_URL environment variable not set. Set it to connect to Redis.",
    )?;

    // Connect to Redis
    output.progress("Connecting to Redis...");
    let client = redis::Client::open(redis_url.as_str())
        .context("Failed to create Redis client")?;
    let mut conn = ConnectionManager::new(client)
        .await
        .context("Failed to connect to Redis")?;
    output.clear_line();
    output.success("Connected to Redis");

    output.info(&format!("Checking for duplicate values in '{field}'"));
    if case_insensitive {
        output.bullet("Mode: case-insensitive");
    } else {
        output.bullet("Mode: case-sensitive");
    }

    // Scan collection and check for duplicates
    let validation = validate_field_uniqueness(&mut conn, collection, field, case_insensitive, output).await?;

    // Report results
    output.heading("Results");

    if validation.duplicates.is_empty() {
        output.success(&format!(
            "No duplicates found! Field '{field}' is unique across {} document(s)",
            validation.total_documents
        ));
        output.info("Safe to add a unique constraint");
    } else {
        output.error(&format!(
            "Found {} duplicate value(s) across {} document(s)",
            validation.duplicates.len(),
            validation.documents_with_duplicates
        ));

        // Show duplicate values
        output.heading("Duplicate Values");
        let max_show = 10;
        for (i, dup) in validation.duplicates.iter().take(max_show).enumerate() {
            output.bullet(&format!(
                "{}: \"{}\" appears {} time(s)",
                i + 1, dup.value, dup.count
            ));
            // Show some document keys
            let keys_preview: Vec<_> = dup.document_keys.iter().take(3).collect();
            for key in keys_preview {
                output.info(&format!("    - {key}"));
            }
            if dup.document_keys.len() > 3 {
                output.info(&format!("    ... and {} more", dup.document_keys.len() - 3));
            }
        }
        if validation.duplicates.len() > max_show {
            output.warning(&format!(
                "... and {} more duplicate values",
                validation.duplicates.len() - max_show
            ));
        }

        output.heading("Required Action");
        output.warning("You must resolve these duplicates before adding a unique constraint");
        output.info("Options:");
        output.bullet("Update duplicate values to be unique");
        output.bullet("Delete duplicate documents");
        output.bullet("Choose a different field for the unique constraint");
    }

    Ok(())
}

/// Duplicate value information.
struct DuplicateValue {
    value: String,
    count: u64,
    document_keys: Vec<String>,
}

/// Validation result.
struct ValidationResult {
    total_documents: u64,
    documents_with_duplicates: u64,
    duplicates: Vec<DuplicateValue>,
}

/// Validate that a field has unique values across a collection.
async fn validate_field_uniqueness(
    conn: &mut ConnectionManager,
    collection: &str,
    field: &str,
    case_insensitive: bool,
    output: &OutputManager,
) -> Result<ValidationResult> {
    let pattern = format!("{collection}:*");
    let json_path = format!("$.{field}");

    // Map: value -> list of document keys
    let mut value_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut total_documents = 0u64;
    let mut cursor: u64 = 0;
    let mut scanned = 0u64;

    loop {
        // Show progress
        if scanned.is_multiple_of(100) && scanned > 0 {
            output.progress(&format!("Scanned {scanned} documents..."));
        }

        let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(conn)
            .await
            .context("Failed to scan Redis keys")?;

        for key in &keys {
            scanned += 1;

            // Skip internal keys
            if key.starts_with("_snugom:") {
                continue;
            }

            // Get the field value
            let value_result: Option<String> = redis::cmd("JSON.GET")
                .arg(key)
                .arg(&json_path)
                .query_async(conn)
                .await
                .unwrap_or(None);

            total_documents += 1;

            if let Some(value_json) = value_result {
                // JSON.GET returns an array
                if let Ok(values) = serde_json::from_str::<Vec<Value>>(&value_json)
                    && let Some(value) = values.first() {
                        let value_str = match value {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            Value::Null => continue, // Skip null values
                            _ => serde_json::to_string(value).unwrap_or_default(),
                        };

                        let normalized_value = if case_insensitive {
                            value_str.to_lowercase()
                        } else {
                            value_str
                        };

                        value_map
                            .entry(normalized_value)
                            .or_default()
                            .push(key.clone());
                    }
            }
        }

        cursor = new_cursor;
        if cursor == 0 {
            break;
        }
    }

    if scanned > 0 {
        output.clear_line();
    }

    // Find duplicates (values with more than one document)
    let mut duplicates = Vec::new();
    let mut documents_with_duplicates = 0u64;

    for (value, keys) in value_map {
        if keys.len() > 1 {
            documents_with_duplicates += keys.len() as u64;
            duplicates.push(DuplicateValue {
                value,
                count: keys.len() as u64,
                document_keys: keys,
            });
        }
    }

    // Sort by count descending
    duplicates.sort_by(|a, b| b.count.cmp(&a.count));

    Ok(ValidationResult {
        total_documents,
        documents_with_duplicates,
        duplicates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("User"), "user");
        assert_eq!(to_snake_case("UserProfile"), "user_profile");
        assert_eq!(to_snake_case("HTTPRequest"), "h_t_t_p_request");
        assert_eq!(to_snake_case("getHTTPResponse"), "get_h_t_t_p_response");
        assert_eq!(to_snake_case("simple"), "simple");
        assert_eq!(to_snake_case(""), "");
    }

    #[test]
    fn test_to_snake_case_single_char() {
        assert_eq!(to_snake_case("A"), "a");
        assert_eq!(to_snake_case("a"), "a");
    }

    #[test]
    fn test_collection_stats_structure() {
        let stats = CollectionStats {
            total: 100,
            with_version: 80,
            without_version: 20,
            by_version: {
                let mut map = HashMap::new();
                map.insert(1, 50);
                map.insert(2, 30);
                map
            },
        };

        assert_eq!(stats.total, 100);
        assert_eq!(stats.with_version, 80);
        assert_eq!(stats.without_version, 20);
        assert_eq!(stats.by_version.get(&1), Some(&50));
        assert_eq!(stats.by_version.get(&2), Some(&30));
    }

    #[test]
    fn test_duplicate_value_structure() {
        let dup = DuplicateValue {
            value: "duplicate@example.com".to_string(),
            count: 3,
            document_keys: vec![
                "users:abc".to_string(),
                "users:def".to_string(),
                "users:ghi".to_string(),
            ],
        };

        assert_eq!(dup.value, "duplicate@example.com");
        assert_eq!(dup.count, 3);
        assert_eq!(dup.document_keys.len(), 3);
    }

    #[test]
    fn test_validation_result_no_duplicates() {
        let result = ValidationResult {
            total_documents: 100,
            documents_with_duplicates: 0,
            duplicates: vec![],
        };

        assert_eq!(result.total_documents, 100);
        assert!(result.duplicates.is_empty());
    }

    #[test]
    fn test_validation_result_with_duplicates() {
        let result = ValidationResult {
            total_documents: 50,
            documents_with_duplicates: 6,
            duplicates: vec![
                DuplicateValue {
                    value: "test1".to_string(),
                    count: 3,
                    document_keys: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                },
                DuplicateValue {
                    value: "test2".to_string(),
                    count: 3,
                    document_keys: vec!["d".to_string(), "e".to_string(), "f".to_string()],
                },
            ],
        };

        assert_eq!(result.duplicates.len(), 2);
        assert_eq!(result.documents_with_duplicates, 6);
    }

    #[test]
    fn test_collection_stats_empty() {
        let stats = CollectionStats {
            total: 0,
            with_version: 0,
            without_version: 0,
            by_version: HashMap::new(),
        };

        assert_eq!(stats.total, 0);
        assert!(stats.by_version.is_empty());
    }
}
