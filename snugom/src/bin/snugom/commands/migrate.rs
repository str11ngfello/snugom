use anyhow::{Context, Result};
use chrono::Utc;
use clap::Subcommand;

use crate::context::ProjectContext;
use crate::differ::{diff_schemas, load_latest_snapshots, EntityDiff, MigrationComplexity};
use crate::examples::ExampleGroup;
use crate::generator::{generate_migration_file, update_migrations_mod, update_source_schema_version};
use crate::output::OutputManager;
use crate::scanner::{discover_entities, parse_entity_file};

pub const EXAMPLES: &[ExampleGroup] = &[
    ExampleGroup {
        title: "Generate Migrations",
        commands: &[
            "snugom migrate --name init           # Create initial migration",
            "snugom migrate --name add_avatar     # Create migration for schema changes",
        ],
    },
    ExampleGroup {
        title: "Deploy Migrations",
        commands: &[
            "snugom migrate deploy                # Run all pending migrations",
            "snugom migrate deploy --dry-run      # Preview what would be migrated",
        ],
    },
    ExampleGroup {
        title: "Recovery",
        commands: &[
            "snugom migrate resolve init --applied       # Mark migration as applied",
            "snugom migrate resolve init --rolled-back   # Mark migration as rolled back",
        ],
    },
];

#[derive(Subcommand)]
pub enum MigrateCommands {
    /// Generate a new migration by detecting schema changes
    #[command(name = "create")]
    Create {
        /// Name for the migration (e.g., add_avatar, split_name)
        #[arg(short, long)]
        name: String,
    },

    /// Run pending migrations against Redis
    #[command(name = "deploy")]
    Deploy {
        /// Preview what would be migrated without making changes
        #[arg(long)]
        dry_run: bool,
    },

    /// Manually mark a migration as applied or rolled back
    #[command(name = "resolve")]
    Resolve {
        /// Migration name to resolve
        migration_name: String,

        /// Mark the migration as applied
        #[arg(long, conflicts_with = "rolled_back")]
        applied: bool,

        /// Mark the migration as rolled back
        #[arg(long, conflicts_with = "applied")]
        rolled_back: bool,
    },
}

pub async fn handle_migrate_commands(
    command: MigrateCommands,
    output: &OutputManager,
) -> Result<()> {
    let ctx = ProjectContext::find()?;

    if !ctx.is_initialized() {
        output.error("SnugOM is not initialized in this project.");
        output.info("Run 'snugom init' first to initialize.");
        anyhow::bail!("Project not initialized");
    }

    match command {
        MigrateCommands::Create { name } => {
            handle_create(&ctx, &name, output).await?;
        }
        MigrateCommands::Deploy { dry_run } => {
            handle_deploy(&ctx, dry_run, output).await?;
        }
        MigrateCommands::Resolve {
            migration_name,
            applied,
            rolled_back,
        } => {
            handle_resolve(&ctx, &migration_name, applied, rolled_back, output).await?;
        }
    }

    Ok(())
}

async fn handle_create(ctx: &ProjectContext, name: &str, output: &OutputManager) -> Result<()> {
    output.heading("Generate Migration");
    output.bullet(&format!("Migration name: {name}"));

    // Step 1: Discover files with SnugomEntity
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

    // Step 2: Parse each file and extract entity schemas
    output.progress("Parsing entity schemas...");
    let mut all_schemas = Vec::new();

    for file in &discovered {
        match parse_entity_file(&file.path, &file.relative_path) {
            Ok(schemas) => {
                for schema in schemas {
                    all_schemas.push(schema);
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
        output.warning("No parseable SnugomEntity structs found");
        return Ok(());
    }

    output.success(&format!("Parsed {} entity schema(s)", all_schemas.len()));

    // Step 3: Load existing snapshots and compute diffs
    output.progress("Loading existing snapshots...");
    let existing_snapshots = load_latest_snapshots(&ctx.schemas_dir)
        .context("Failed to load existing snapshots")?;
    output.clear_line();

    output.info(&format!("Found {} existing snapshot(s)", existing_snapshots.len()));

    // Step 4: Compute diffs for each entity
    output.heading("Detecting Changes");
    let mut diffs: Vec<EntityDiff> = Vec::new();

    for schema in &all_schemas {
        let old_snapshot = existing_snapshots.get(&schema.entity);
        let diff = diff_schemas(old_snapshot, schema);

        if diff.is_new() {
            output.bullet(&format!(
                "{} (NEW) - baseline v{}",
                diff.entity, diff.new_version
            ));
        } else if diff.has_changes() {
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
                output.info(&format!("    {}", format_change(change)));
            }
        } else {
            output.info(&format!("  {} - no changes", diff.entity));
        }

        diffs.push(diff);
    }

    // Filter to only entities with changes
    let diffs_with_changes: Vec<&EntityDiff> = diffs
        .iter()
        .filter(|d| d.has_changes() || d.is_new())
        .collect();

    if diffs_with_changes.is_empty() {
        output.success("No schema changes detected");
        output.info("Your entities match the latest snapshots");
        return Ok(());
    }

    // Step 5: Generate migration file
    output.heading("Generating Migration");
    let timestamp = Utc::now();
    let diffs_owned: Vec<EntityDiff> = diffs_with_changes.into_iter().cloned().collect();
    let migration = generate_migration_file(name, &diffs_owned, timestamp);

    // Write migration file
    std::fs::create_dir_all(&ctx.migrations_dir)
        .context("Failed to create migrations directory")?;

    let migration_path = ctx.migrations_dir.join(&migration.filename);
    std::fs::write(&migration_path, &migration.content)
        .with_context(|| format!("Failed to write migration: {}", migration_path.display()))?;

    output.success(&format!("Created: {}", migration.filename));
    output.bullet(&format!("Type: {}", migration.complexity));

    // Update migrations/mod.rs
    update_migrations_mod(&ctx.migrations_dir, &migration.module_name)
        .context("Failed to update migrations/mod.rs")?;
    output.bullet("Updated: src/migrations/mod.rs");

    // Step 6: Update source files with new schema versions
    output.heading("Updating Source Files");
    for diff in &diffs_owned {
        if diff.is_new() || diff.has_changes() {
            let source_path = ctx.project_root.join(&diff.source_file);
            match update_source_schema_version(
                &source_path,
                &diff.entity,
                diff.old_version,
                diff.new_version,
            ) {
                Ok(true) => {
                    output.bullet(&format!(
                        "{}: schema {} → {}",
                        diff.source_file,
                        diff.old_version.map(|v| v.to_string()).unwrap_or_else(|| "NEW".to_string()),
                        diff.new_version
                    ));
                }
                Ok(false) => {
                    output.info(&format!("  {} (no update needed)", diff.source_file));
                }
                Err(err) => {
                    output.warning(&format!("  {} - failed: {err}", diff.source_file));
                }
            }
        }
    }

    // Step 7: Save new snapshots
    output.heading("Saving Snapshots");
    std::fs::create_dir_all(&ctx.schemas_dir)
        .context("Failed to create schemas directory")?;

    for diff in &diffs_owned {
        // Find the corresponding schema
        if let Some(schema) = all_schemas.iter().find(|s| s.entity == diff.entity) {
            // Create updated schema with new version
            let mut updated_schema = schema.clone();
            updated_schema.schema = diff.new_version;
            updated_schema.generated_at = timestamp;

            let filename = updated_schema.snapshot_filename();
            let snapshot_path = ctx.schemas_dir.join(&filename);

            let json = serde_json::to_string_pretty(&updated_schema)
                .context("Failed to serialize schema")?;

            std::fs::write(&snapshot_path, json)
                .with_context(|| format!("Failed to write snapshot: {}", snapshot_path.display()))?;

            output.bullet(&format!("Saved: {filename}"));
        }
    }

    // Summary
    output.heading("Summary");
    output.success("Migration generated successfully!");

    if migration.complexity == MigrationComplexity::Stub {
        output.warning("⚠ This migration requires implementation:");
        output.info(&format!("   Review and complete: src/migrations/{}", migration.filename));
    }

    output.info("Next steps:");
    output.bullet("Review the generated migration");
    output.bullet("Commit the changes");
    output.bullet("Run 'snugom migrate deploy' to apply");

    Ok(())
}

/// Format a change for display
fn format_change(change: &crate::differ::EntityChange) -> String {
    use crate::differ::{ChangeType, EntityChange};

    match change {
        EntityChange::Field(fc) => {
            let prefix = match fc.change_type {
                ChangeType::Added => "+",
                ChangeType::Removed => "-",
                ChangeType::Modified => "~",
            };
            if let Some(ref field) = fc.new_field {
                format!("{prefix} {}: {}", fc.name, field.field_type)
            } else if let Some(ref field) = fc.old_field {
                format!("{prefix} {}: {} (removed)", fc.name, field.field_type)
            } else {
                format!("{prefix} {}", fc.name)
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

async fn handle_deploy(ctx: &ProjectContext, dry_run: bool, output: &OutputManager) -> Result<()> {
    use crate::executor::MigrationRunner;

    output.heading("Deploy Migrations");

    if dry_run {
        output.warning("DRY RUN MODE - No changes will be made");
    }

    // Get Redis URL
    let redis_url = ctx.redis_url().map_err(|_| {
        output.error("REDIS_URL environment variable not set");
        anyhow::anyhow!("REDIS_URL is required for migration deployment")
    })?;

    output.bullet(&format!("Redis: {redis_url}"));

    // Connect to Redis
    output.progress("Connecting to Redis...");
    let mut runner = MigrationRunner::new(&redis_url, dry_run)
        .await
        .context("Failed to connect to Redis")?;
    output.clear_line();
    output.success("Connected to Redis");

    // Run migrations
    let stats = runner.run_all(&ctx.migrations_dir, output).await?;

    // Summary
    output.heading("Summary");

    if stats.migrations_applied > 0 {
        output.success(&format!(
            "{} migration(s) applied in {}ms",
            stats.migrations_applied,
            stats.total_time_ms
        ));
    }

    if stats.migrations_skipped > 0 {
        output.info(&format!(
            "{} migration(s) already applied",
            stats.migrations_skipped
        ));
    }

    if stats.documents_transformed > 0 {
        output.bullet(&format!(
            "{} document(s) transformed",
            stats.documents_transformed
        ));
    }

    if dry_run {
        output.warning("DRY RUN - No actual changes were made");
    }

    Ok(())
}

async fn handle_resolve(
    ctx: &ProjectContext,
    migration_name: &str,
    applied: bool,
    rolled_back: bool,
    output: &OutputManager,
) -> Result<()> {
    use crate::executor::{MigrationRunner, MigrationState};
    use crate::executor::state::calculate_checksum;

    if !applied && !rolled_back {
        output.error("Must specify either --applied or --rolled-back");
        anyhow::bail!("Missing resolution flag");
    }

    let status = if applied { "applied" } else { "rolled-back" };

    output.heading(&format!("Resolve Migration: {migration_name}"));
    output.info(&format!("Marking migration as: {status}"));

    // Get Redis URL
    let redis_url = ctx.redis_url().map_err(|_| {
        output.error("REDIS_URL environment variable not set");
        anyhow::anyhow!("REDIS_URL is required for migration resolution")
    })?;

    // Connect to Redis
    output.progress("Connecting to Redis...");
    let mut context = crate::executor::MigrationContext::connect(&redis_url)
        .await
        .context("Failed to connect to Redis")?;
    output.clear_line();

    let mut state = MigrationState::new(context.conn().clone());

    // Find the migration file to get its checksum
    let migrations = MigrationRunner::discover_migrations(&ctx.migrations_dir)?;
    let migration = migrations.iter().find(|m| m.display_name == migration_name);

    if applied {
        let checksum = migration
            .map(|m| m.checksum.clone())
            .unwrap_or_else(|| calculate_checksum("unknown"));

        // Check if already applied
        if state.is_applied(migration_name).await? {
            output.warning(&format!("Migration '{migration_name}' is already marked as applied"));
            return Ok(());
        }

        state.mark_applied(migration_name, &checksum).await?;
        output.success(&format!("Marked '{migration_name}' as applied"));
    } else {
        // Rolled back
        if !state.is_applied(migration_name).await? {
            output.warning(&format!("Migration '{migration_name}' is not marked as applied"));
            return Ok(());
        }

        state.mark_rolled_back(migration_name).await?;
        output.success(&format!("Marked '{migration_name}' as rolled back"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::differ::{ChangeType, EntityChange, FieldChange, IndexChange, RelationChange, UniqueConstraintChange};
    use crate::scanner::{FieldInfo, RelationInfo, RelationKind, CascadeStrategy, UniqueConstraint, FilterableType};

    #[test]
    fn test_format_change_field_added() {
        let field = FieldInfo::new("email".to_string(), "String".to_string());
        let fc = FieldChange {
            name: "email".to_string(),
            change_type: ChangeType::Added,
            old_field: None,
            new_field: Some(field),
        };
        let change = EntityChange::Field(fc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "+ email: String");
    }

    #[test]
    fn test_format_change_field_removed() {
        let field = FieldInfo::new("legacy".to_string(), "i32".to_string());
        let fc = FieldChange {
            name: "legacy".to_string(),
            change_type: ChangeType::Removed,
            old_field: Some(field),
            new_field: None,
        };
        let change = EntityChange::Field(fc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "- legacy: i32 (removed)");
    }

    #[test]
    fn test_format_change_field_modified() {
        let old_field = FieldInfo::new("count".to_string(), "String".to_string());
        let new_field = FieldInfo::new("count".to_string(), "i32".to_string());
        let fc = FieldChange {
            name: "count".to_string(),
            change_type: ChangeType::Modified,
            old_field: Some(old_field),
            new_field: Some(new_field),
        };
        let change = EntityChange::Field(fc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "~ count: i32");
    }

    #[test]
    fn test_format_change_index_added() {
        let ic = IndexChange {
            field: "email".to_string(),
            change_type: ChangeType::Added,
            old_type: None,
            new_type: Some(FilterableType::Tag),
        };
        let change = EntityChange::Index(ic);

        let formatted = format_change(&change);
        assert_eq!(formatted, "+ index on email");
    }

    #[test]
    fn test_format_change_index_removed() {
        let ic = IndexChange {
            field: "name".to_string(),
            change_type: ChangeType::Removed,
            old_type: Some(FilterableType::Text),
            new_type: None,
        };
        let change = EntityChange::Index(ic);

        let formatted = format_change(&change);
        assert_eq!(formatted, "- index on name");
    }

    #[test]
    fn test_format_change_relation_added() {
        let rel = RelationInfo {
            field: "author_id".to_string(),
            target: "users".to_string(),
            kind: RelationKind::BelongsTo,
            cascade: CascadeStrategy::Detach,
        };
        let rc = RelationChange {
            field: "author_id".to_string(),
            change_type: ChangeType::Added,
            old_relation: None,
            new_relation: Some(rel),
        };
        let change = EntityChange::Relation(rc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "+ relation author_id -> users");
    }

    #[test]
    fn test_format_change_relation_removed() {
        let rel = RelationInfo {
            field: "org_id".to_string(),
            target: "organizations".to_string(),
            kind: RelationKind::BelongsTo,
            cascade: CascadeStrategy::Detach,
        };
        let rc = RelationChange {
            field: "org_id".to_string(),
            change_type: ChangeType::Removed,
            old_relation: Some(rel),
            new_relation: None,
        };
        let change = EntityChange::Relation(rc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "- relation org_id");
    }

    #[test]
    fn test_format_change_unique_constraint_added() {
        let uc = UniqueConstraintChange {
            fields: vec!["email".to_string()],
            change_type: ChangeType::Added,
            old_constraint: None,
            new_constraint: Some(UniqueConstraint {
                fields: vec!["email".to_string()],
                case_insensitive: false,
            }),
        };
        let change = EntityChange::UniqueConstraint(uc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "+ unique(email)");
    }

    #[test]
    fn test_format_change_compound_unique_constraint() {
        let uc = UniqueConstraintChange {
            fields: vec!["tenant_id".to_string(), "email".to_string()],
            change_type: ChangeType::Added,
            old_constraint: None,
            new_constraint: Some(UniqueConstraint {
                fields: vec!["tenant_id".to_string(), "email".to_string()],
                case_insensitive: true,
            }),
        };
        let change = EntityChange::UniqueConstraint(uc);

        let formatted = format_change(&change);
        assert_eq!(formatted, "+ unique(tenant_id, email)");
    }
}
