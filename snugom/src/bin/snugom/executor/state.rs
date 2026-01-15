//! Migration state tracking in Redis.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

/// Key prefix for migration state.
const MIGRATION_STATE_KEY: &str = "_snugom:migrations";

/// Applied migration record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedMigration {
    /// Migration name (e.g., "20241228_100000_init")
    pub name: String,
    /// When the migration was applied
    pub applied_at: DateTime<Utc>,
    /// Migration checksum for validation
    pub checksum: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Number of documents affected
    pub documents_affected: u64,
    /// Whether migration was applied in dry-run mode
    pub dry_run: bool,
}

/// Migration state manager.
pub struct MigrationState {
    conn: ConnectionManager,
}

impl MigrationState {
    /// Create a new migration state manager.
    pub fn new(conn: ConnectionManager) -> Self {
        Self { conn }
    }

    /// List all applied migrations.
    pub async fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
        let data: Option<String> = redis::cmd("JSON.GET")
            .arg(MIGRATION_STATE_KEY)
            .arg("$.applied")
            .query_async(&mut self.conn)
            .await
            .unwrap_or(None);

        match data {
            Some(json_str) => {
                // JSON.GET returns an array
                let wrapper: Vec<Vec<AppliedMigration>> =
                    serde_json::from_str(&json_str)
                        .unwrap_or_default();
                Ok(wrapper.into_iter().next().unwrap_or_default())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Check if a migration has been applied.
    pub async fn is_applied(&mut self, name: &str) -> Result<bool> {
        let applied = self.list_applied().await?;
        Ok(applied.iter().any(|m| m.name == name))
    }

    /// Record a migration as applied.
    pub async fn record_applied(&mut self, migration: AppliedMigration) -> Result<()> {
        // First, ensure the state structure exists
        let exists: bool = redis::cmd("EXISTS")
            .arg(MIGRATION_STATE_KEY)
            .query_async(&mut self.conn)
            .await
            .unwrap_or(false);

        if !exists {
            // Initialize the state structure
            let _: () = redis::cmd("JSON.SET")
                .arg(MIGRATION_STATE_KEY)
                .arg("$")
                .arg(r#"{"applied":[]}"#)
                .query_async(&mut self.conn)
                .await
                .context("Failed to initialize migration state")?;
        }

        // Append the migration to the applied list
        let migration_json = serde_json::to_string(&migration)
            .context("Failed to serialize migration record")?;

        let _: () = redis::cmd("JSON.ARRAPPEND")
            .arg(MIGRATION_STATE_KEY)
            .arg("$.applied")
            .arg(&migration_json)
            .query_async(&mut self.conn)
            .await
            .context("Failed to record applied migration")?;

        Ok(())
    }

    /// Remove a migration record (for rollback).
    #[allow(dead_code)]
    pub async fn remove_applied(&mut self, name: &str) -> Result<bool> {
        let applied = self.list_applied().await?;
        let index = applied.iter().position(|m| m.name == name);

        if let Some(idx) = index {
            // Remove the migration at this index
            let _: () = redis::cmd("JSON.ARRPOP")
                .arg(MIGRATION_STATE_KEY)
                .arg("$.applied")
                .arg(idx as i64)
                .query_async(&mut self.conn)
                .await
                .context("Failed to remove migration record")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark a migration as applied (for resolve command).
    pub async fn mark_applied(
        &mut self,
        name: &str,
        checksum: &str,
    ) -> Result<()> {
        let migration = AppliedMigration {
            name: name.to_string(),
            applied_at: Utc::now(),
            checksum: checksum.to_string(),
            execution_time_ms: 0,
            documents_affected: 0,
            dry_run: false,
        };
        self.record_applied(migration).await
    }

    /// Mark a migration as rolled back (remove from applied).
    pub async fn mark_rolled_back(&mut self, name: &str) -> Result<bool> {
        self.remove_applied(name).await
    }

    /// Get the last applied migration.
    #[allow(dead_code)]
    pub async fn last_applied(&mut self) -> Result<Option<AppliedMigration>> {
        let applied = self.list_applied().await?;
        Ok(applied.into_iter().last())
    }
}

/// Calculate a checksum for migration content.
pub fn calculate_checksum(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checksum() {
        let checksum1 = calculate_checksum("pub fn register() {}");
        let checksum2 = calculate_checksum("pub fn register() {}");
        let checksum3 = calculate_checksum("pub fn register() { /* different */ }");

        assert_eq!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_calculate_checksum_empty() {
        let checksum = calculate_checksum("");
        assert!(!checksum.is_empty());
        // Empty content should still produce a valid hex hash
        assert!(checksum.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_calculate_checksum_whitespace_matters() {
        let checksum1 = calculate_checksum("test");
        let checksum2 = calculate_checksum("test ");
        let checksum3 = calculate_checksum(" test");

        assert_ne!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);
        assert_ne!(checksum2, checksum3);
    }

    #[test]
    fn test_applied_migration_serialization() {
        let migration = AppliedMigration {
            name: "20241228_100000_init".to_string(),
            applied_at: Utc::now(),
            checksum: "abc123def".to_string(),
            execution_time_ms: 150,
            documents_affected: 1000,
            dry_run: false,
        };

        let json = serde_json::to_string(&migration).unwrap();
        let deserialized: AppliedMigration = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, migration.name);
        assert_eq!(deserialized.checksum, migration.checksum);
        assert_eq!(deserialized.execution_time_ms, migration.execution_time_ms);
        assert_eq!(deserialized.documents_affected, migration.documents_affected);
        assert_eq!(deserialized.dry_run, migration.dry_run);
    }

    #[test]
    fn test_applied_migration_dry_run_flag() {
        let dry_run_migration = AppliedMigration {
            name: "test".to_string(),
            applied_at: Utc::now(),
            checksum: "xyz".to_string(),
            execution_time_ms: 0,
            documents_affected: 0,
            dry_run: true,
        };

        let json = serde_json::to_string(&dry_run_migration).unwrap();
        assert!(json.contains("\"dry_run\":true"));
    }
}
