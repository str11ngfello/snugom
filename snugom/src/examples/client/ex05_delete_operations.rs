//! Example 05 – Delete Operations
//!
//! Demonstrates deleting entities using the `snugom_delete!` macro DSL:
//! - Single entity deletion
//! - Bulk deletes with `delete_many_by_ids()` and `delete_many()`
//!   (collection-level operations)

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_delete, SearchQuery};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "logs")]
struct LogEntry {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    level: String,
    #[snugom(filterable(text))]
    message: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [LogEntry])]
struct LogClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("delete_ops");
    let mut client = LogClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search-based delete_many
    let mut logs = client.log_entries();

    // Create test log entries using snugom_create! macro
    let info1 = snugom_create!(client, LogEntry {
        level: "info".to_string(),
        message: "System started".to_string(),
        created_at: Utc::now(),
    }).await?;

    let info2 = snugom_create!(client, LogEntry {
        level: "info".to_string(),
        message: "User logged in".to_string(),
        created_at: Utc::now(),
    }).await?;

    let warn1 = snugom_create!(client, LogEntry {
        level: "warn".to_string(),
        message: "High memory usage".to_string(),
        created_at: Utc::now(),
    }).await?;

    let error1 = snugom_create!(client, LogEntry {
        level: "error".to_string(),
        message: "Connection failed".to_string(),
        created_at: Utc::now(),
    }).await?;

    let error2 = snugom_create!(client, LogEntry {
        level: "error".to_string(),
        message: "Timeout occurred".to_string(),
        created_at: Utc::now(),
    }).await?;

    assert_eq!(logs.count().await?, 5, "should have 5 logs");

    // ============ snugom_delete! macro ============
    // Simple delete by ID
    snugom_delete!(client, LogEntry(&info1.id)).await?;
    assert!(!logs.exists(&info1.id).await?, "info1 should be deleted");
    assert_eq!(logs.count().await?, 4, "should have 4 logs");

    // ============ Delete another entity ============
    snugom_delete!(client, LogEntry(&info2.id)).await?;
    assert!(!logs.exists(&info2.id).await?, "info2 should be deleted");
    assert_eq!(logs.count().await?, 3, "should have 3 logs");

    // ============ delete_many_by_ids() ============
    // Bulk delete specific IDs (collection-level API)
    let ids = [error1.id.as_str(), error2.id.as_str()];
    let deleted_count = logs.delete_many_by_ids(&ids).await?;
    assert_eq!(deleted_count, 2, "should delete 2 errors");

    // ============ delete_many() ============
    // Bulk delete by search query
    // First add more entries
    snugom_create!(client, LogEntry {
        level: "debug".to_string(),
        message: "Debug 1".to_string(),
        created_at: Utc::now(),
    }).await?;

    snugom_create!(client, LogEntry {
        level: "debug".to_string(),
        message: "Debug 2".to_string(),
        created_at: Utc::now(),
    }).await?;

    // Delete all debug logs (collection-level API)
    let query = SearchQuery {
        filter: vec!["level:eq:debug".to_string()],
        ..Default::default()
    };
    let deleted_count = logs.delete_many(query).await?;
    assert_eq!(deleted_count, 2, "should delete 2 debug logs");

    // Only warn1 should remain
    assert_eq!(logs.count().await?, 1, "should have 1 log remaining");
    assert!(logs.exists(&warn1.id).await?, "warn1 should still exist");

    Ok(())
}
