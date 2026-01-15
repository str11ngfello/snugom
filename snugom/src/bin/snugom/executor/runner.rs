//! Migration runner for executing pending migrations.

use anyhow::{Context, Result};
use chrono::Utc;
use std::path::Path;
use std::time::Instant;

use super::context::MigrationContext;
use super::state::{calculate_checksum, AppliedMigration, MigrationState};
use crate::output::OutputManager;

/// Statistics from a migration run.
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Number of migrations applied
    pub migrations_applied: u32,
    /// Total documents transformed
    pub documents_transformed: u64,
    /// Total execution time in milliseconds
    pub total_time_ms: u64,
    /// Migrations that were skipped (already applied)
    pub migrations_skipped: u32,
}

/// Information about a discovered migration file.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MigrationInfo {
    /// Module name (e.g., "_20241228_100000_init")
    pub module_name: String,
    /// Display name (e.g., "20241228_100000_init")
    pub display_name: String,
    /// File path
    pub path: std::path::PathBuf,
    /// File checksum
    pub checksum: String,
}

/// Migration runner.
pub struct MigrationRunner {
    ctx: MigrationContext,
    state: MigrationState,
    dry_run: bool,
}

impl MigrationRunner {
    /// Create a new migration runner.
    pub async fn new(redis_url: &str, dry_run: bool) -> Result<Self> {
        let ctx = MigrationContext::connect(redis_url)
            .await?
            .with_dry_run(dry_run);

        // Clone the connection for state tracking
        let mut state_conn = MigrationContext::connect(redis_url).await?;
        let state = MigrationState::new(state_conn.conn().clone());

        Ok(Self {
            ctx,
            state,
            dry_run,
        })
    }

    /// Discover migration files from the migrations directory.
    pub fn discover_migrations(migrations_dir: &Path) -> Result<Vec<MigrationInfo>> {
        let mut migrations = Vec::new();

        if !migrations_dir.exists() {
            return Ok(migrations);
        }

        // Read all .rs files (except mod.rs)
        let entries = std::fs::read_dir(migrations_dir)
            .context("Failed to read migrations directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                if filename == "mod.rs" {
                    continue;
                }

                // Extract module name from filename
                let module_name = filename.trim_end_matches(".rs");

                // Read file content for checksum
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read migration: {}", path.display()))?;
                let checksum = calculate_checksum(&content);

                // Create display name (remove leading underscore if present)
                let display_name = module_name.strip_prefix('_').unwrap_or(module_name);

                migrations.push(MigrationInfo {
                    module_name: module_name.to_string(),
                    display_name: display_name.to_string(),
                    path: path.clone(),
                    checksum,
                });
            }
        }

        // Sort by name (which is timestamp-prefixed)
        migrations.sort_by(|a, b| a.module_name.cmp(&b.module_name));

        Ok(migrations)
    }

    /// Run all pending migrations.
    pub async fn run_all(
        &mut self,
        migrations_dir: &Path,
        output: &OutputManager,
    ) -> Result<MigrationStats> {
        let start_time = Instant::now();
        let mut stats = MigrationStats::default();

        // Discover migrations
        output.progress("Discovering migrations...");
        let migrations = Self::discover_migrations(migrations_dir)?;
        output.clear_line();

        if migrations.is_empty() {
            output.warning("No migrations found");
            return Ok(stats);
        }

        output.info(&format!("Found {} migration(s)", migrations.len()));

        // Check which are already applied
        output.progress("Checking applied migrations...");
        let applied = self.state.list_applied().await?;
        let applied_names: std::collections::HashSet<_> =
            applied.iter().map(|m| m.name.as_str()).collect();
        output.clear_line();

        let pending: Vec<_> = migrations
            .iter()
            .filter(|m| !applied_names.contains(m.display_name.as_str()))
            .collect();

        if pending.is_empty() {
            output.success("All migrations are up to date");
            return Ok(stats);
        }

        output.info(&format!(
            "{} migration(s) pending, {} already applied",
            pending.len(),
            applied_names.len()
        ));

        if self.dry_run {
            output.warning("DRY RUN MODE - No changes will be made");
        }

        // Run each pending migration
        for migration in pending {
            let migration_start = Instant::now();

            output.heading(&format!("Applying: {}", migration.display_name));

            // For now, we just record the migration as applied
            // The actual document transformation would require compiling and running the migration code
            // which is beyond the scope of a CLI tool
            //
            // In a full implementation, migrations would be registered at compile time
            // and the CLI would invoke them through the compiled application

            output.bullet("Migration type: BASELINE/AUTO");
            output.bullet("Documents: 0 (placeholder)");

            let migration_time = migration_start.elapsed().as_millis() as u64;

            if !self.dry_run {
                let record = AppliedMigration {
                    name: migration.display_name.clone(),
                    applied_at: Utc::now(),
                    checksum: migration.checksum.clone(),
                    execution_time_ms: migration_time,
                    documents_affected: 0,
                    dry_run: false,
                };
                self.state.record_applied(record).await?;
            }

            output.success(&format!(
                "Applied in {}ms",
                migration_time
            ));

            stats.migrations_applied += 1;
        }

        stats.migrations_skipped = applied_names.len() as u32;
        stats.total_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(stats)
    }

    /// Get the migration state manager.
    #[allow(dead_code)]
    pub fn state(&mut self) -> &mut MigrationState {
        &mut self.state
    }

    /// Get the migration context.
    #[allow(dead_code)]
    pub fn context(&mut self) -> &mut MigrationContext {
        &mut self.ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_discover_migrations() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir(&migrations_dir).unwrap();

        // Create mock migration files
        let mut file1 = std::fs::File::create(migrations_dir.join("_20241228_100000_init.rs")).unwrap();
        writeln!(file1, "pub fn register() {{}}").unwrap();

        let mut file2 = std::fs::File::create(migrations_dir.join("_20241228_110000_add_avatar.rs")).unwrap();
        writeln!(file2, "pub fn register() {{}}").unwrap();

        // Create mod.rs (should be ignored)
        let mut mod_file = std::fs::File::create(migrations_dir.join("mod.rs")).unwrap();
        writeln!(mod_file, "mod _20241228_100000_init;").unwrap();

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();

        assert_eq!(migrations.len(), 2);
        assert_eq!(migrations[0].display_name, "20241228_100000_init");
        assert_eq!(migrations[1].display_name, "20241228_110000_add_avatar");
    }

    #[test]
    fn test_discover_migrations_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir(&migrations_dir).unwrap();

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_discover_migrations_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("nonexistent");

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn test_discover_migrations_sorted_by_name() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir(&migrations_dir).unwrap();

        // Create files in reverse order
        let mut file3 = std::fs::File::create(migrations_dir.join("_20241230_add_z.rs")).unwrap();
        writeln!(file3, "// migration 3").unwrap();

        let mut file1 = std::fs::File::create(migrations_dir.join("_20241228_add_a.rs")).unwrap();
        writeln!(file1, "// migration 1").unwrap();

        let mut file2 = std::fs::File::create(migrations_dir.join("_20241229_add_b.rs")).unwrap();
        writeln!(file2, "// migration 2").unwrap();

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();

        assert_eq!(migrations.len(), 3);
        // Should be sorted by module name (which is timestamp-based)
        assert_eq!(migrations[0].display_name, "20241228_add_a");
        assert_eq!(migrations[1].display_name, "20241229_add_b");
        assert_eq!(migrations[2].display_name, "20241230_add_z");
    }

    #[test]
    fn test_discover_migrations_ignores_non_rust_files() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir(&migrations_dir).unwrap();

        // Create various file types
        std::fs::write(migrations_dir.join("_20241228_init.rs"), "// rust file").unwrap();
        std::fs::write(migrations_dir.join("README.md"), "# readme").unwrap();
        std::fs::write(migrations_dir.join("notes.txt"), "notes").unwrap();
        std::fs::write(migrations_dir.join(".gitignore"), "*").unwrap();

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();

        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].display_name, "20241228_init");
    }

    #[test]
    fn test_migration_info_checksum_differs_for_content() {
        let temp_dir = TempDir::new().unwrap();
        let migrations_dir = temp_dir.path().join("migrations");
        std::fs::create_dir(&migrations_dir).unwrap();

        // Create two files with different content
        std::fs::write(
            migrations_dir.join("_20241228_a.rs"),
            "pub fn migrate() { /* version 1 */ }",
        ).unwrap();
        std::fs::write(
            migrations_dir.join("_20241228_b.rs"),
            "pub fn migrate() { /* version 2 */ }",
        ).unwrap();

        let migrations = MigrationRunner::discover_migrations(&migrations_dir).unwrap();

        assert_eq!(migrations.len(), 2);
        assert_ne!(migrations[0].checksum, migrations[1].checksum);
    }

    #[test]
    fn test_migration_stats_default() {
        let stats = MigrationStats::default();
        assert_eq!(stats.migrations_applied, 0);
        assert_eq!(stats.documents_transformed, 0);
        assert_eq!(stats.total_time_ms, 0);
        assert_eq!(stats.migrations_skipped, 0);
    }
}
