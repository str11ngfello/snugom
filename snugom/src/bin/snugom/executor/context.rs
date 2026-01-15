//! Migration context providing Redis access during migration execution.

use anyhow::{Context as AnyhowContext, Result};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde_json::Value;

/// Context for executing migrations.
///
/// Provides access to Redis for document scanning and updates.
pub struct MigrationContext {
    /// Redis connection manager
    conn: ConnectionManager,
    /// Optional dry-run mode (no actual writes)
    dry_run: bool,
}

#[allow(dead_code)]
impl MigrationContext {
    /// Create a new migration context.
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url).context("Failed to create Redis client")?;

        let conn = ConnectionManager::new(client).await.context("Failed to connect to Redis")?;

        Ok(Self { conn, dry_run: false })
    }

    /// Enable dry-run mode (no writes).
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Check if we're in dry-run mode.
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Get the Redis connection manager.
    pub fn conn(&mut self) -> &mut ConnectionManager {
        &mut self.conn
    }

    /// Scan documents in a collection by schema version.
    ///
    /// Returns documents with the specified schema version (or all if None).
    pub async fn scan_documents(
        &mut self,
        collection: &str,
        schema_version: Option<u32>,
        limit: usize,
    ) -> Result<Vec<DocumentInfo>> {
        let pattern = format!("{}:*", collection);
        let mut documents = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.conn)
                .await
                .context("Failed to scan Redis keys")?;

            for key in keys {
                if documents.len() >= limit {
                    break;
                }

                // Get the document
                let doc: Option<String> = redis::cmd("JSON.GET")
                    .arg(&key)
                    .arg("$")
                    .query_async(&mut self.conn)
                    .await
                    .unwrap_or(None);

                if let Some(json_str) = doc {
                    // Parse the JSON array from JSON.GET (it returns an array)
                    if let Ok(values) = serde_json::from_str::<Vec<Value>>(&json_str)
                        && let Some(value) = values.into_iter().next()
                    {
                        // Check schema version if specified
                        let doc_version = value.get("__schema_version").and_then(|v| v.as_u64()).map(|v| v as u32);

                        let matches = match (schema_version, doc_version) {
                            (Some(want), Some(have)) => want == have,
                            (Some(_), None) => false, // Version required but doc has none
                            (None, _) => true,        // No version filter
                        };

                        if matches {
                            documents.push(DocumentInfo {
                                key: key.clone(),
                                id: extract_id_from_key(&key),
                                schema_version: doc_version,
                                data: value,
                            });
                        }
                    }
                }
            }

            cursor = new_cursor;
            if cursor == 0 || documents.len() >= limit {
                break;
            }
        }

        Ok(documents)
    }

    /// Update a document.
    pub async fn update_document(&mut self, key: &str, data: &Value) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }

        let json_str = serde_json::to_string(data).context("Failed to serialize document")?;

        let _: () = redis::cmd("JSON.SET")
            .arg(key)
            .arg("$")
            .arg(&json_str)
            .query_async(&mut self.conn)
            .await
            .context("Failed to update document")?;

        Ok(())
    }

    /// Update the schema version of a document.
    pub async fn update_schema_version(&mut self, key: &str, new_version: u32) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }

        let _: () = redis::cmd("JSON.SET")
            .arg(key)
            .arg("$.__schema_version")
            .arg(new_version)
            .query_async(&mut self.conn)
            .await
            .context("Failed to update schema version")?;

        Ok(())
    }

    /// Delete a document.
    #[allow(dead_code)]
    pub async fn delete_document(&mut self, key: &str) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }

        let _: () = self.conn.del(key).await.context("Failed to delete document")?;

        Ok(())
    }
}

/// Information about a document during migration.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DocumentInfo {
    /// Full Redis key
    pub key: String,
    /// Document ID extracted from key
    pub id: String,
    /// Current schema version (if present)
    pub schema_version: Option<u32>,
    /// Document data
    pub data: Value,
}

/// Extract the document ID from a Redis key.
#[allow(dead_code)]
fn extract_id_from_key(key: &str) -> String {
    // Key format: collection:id
    key.split(':').next_back().unwrap_or(key).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_id_from_key() {
        assert_eq!(extract_id_from_key("users:abc123"), "abc123");
        assert_eq!(extract_id_from_key("posts:xyz"), "xyz");
        assert_eq!(extract_id_from_key("simple"), "simple");
    }

    #[test]
    fn test_extract_id_from_key_multiple_colons() {
        // Takes last segment after colon
        assert_eq!(extract_id_from_key("prefix:users:abc123"), "abc123");
        assert_eq!(extract_id_from_key("a:b:c:d"), "d");
    }

    #[test]
    fn test_extract_id_from_key_empty() {
        assert_eq!(extract_id_from_key(""), "");
    }

    #[test]
    fn test_extract_id_from_key_trailing_colon() {
        assert_eq!(extract_id_from_key("users:"), "");
    }

    #[test]
    fn test_document_info_structure() {
        let doc = DocumentInfo {
            key: "users:abc123".to_string(),
            id: "abc123".to_string(),
            schema_version: Some(2),
            data: serde_json::json!({"name": "test"}),
        };

        assert_eq!(doc.key, "users:abc123");
        assert_eq!(doc.id, "abc123");
        assert_eq!(doc.schema_version, Some(2));
    }

    #[test]
    fn test_document_info_no_version() {
        let doc = DocumentInfo {
            key: "users:xyz".to_string(),
            id: "xyz".to_string(),
            schema_version: None,
            data: serde_json::json!({}),
        };

        assert!(doc.schema_version.is_none());
    }
}
