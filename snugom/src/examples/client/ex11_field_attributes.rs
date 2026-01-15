//! Example 11 – Field Attributes
//!
//! Demonstrates the various field attributes available in SnugOM:
//! - `#[snugom(id)]` - Entity identifier
//! - `#[snugom(filterable)]` - Enables filtering/querying
//! - `#[snugom(sortable)]` - Enables sorting
//! - `#[snugom(filterable(text))]` - Full-text search field
//! - `#[snugom(filterable(tag))]` - Exact match field
//! - `#[snugom(datetime)]` - DateTime field with numeric indexing

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, SearchQuery, search::SortOrder};

/// Example entity demonstrating all field attribute types.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "documents")]
struct Document {
    /// Primary key - auto-generated if not provided
    #[snugom(id)]
    id: String,

    /// Auto-managed timestamp
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,

    /// Auto-managed update timestamp
    #[snugom(updated_at)]
    updated_at: chrono::DateTime<Utc>,

    /// TEXT field - tokenized for full-text search
    /// Supports prefix, contains, fuzzy matching
    #[snugom(filterable(text))]
    title: String,

    /// TEXT field for longer content
    #[snugom(filterable(text))]
    content: String,

    /// TAG field - exact match only
    /// Good for: status, category, owner_id, etc.
    #[snugom(filterable(tag))]
    status: String,

    /// TAG field for categorization
    #[snugom(filterable(tag))]
    category: String,

    /// NUMERIC field - supports range queries
    /// `filterable` alone on numeric types defaults to numeric index
    #[snugom(filterable, sortable)]
    priority: i64,

    /// NUMERIC field - sortable enables ORDER BY
    #[snugom(filterable, sortable)]
    views: i64,

    /// BOOLEAN field
    #[snugom(filterable)]
    published: bool,

    /// DateTime field indexed as numeric (timestamp)
    /// Enables date range queries
    #[snugom(datetime, filterable, sortable)]
    due_date: Option<chrono::DateTime<Utc>>,

    /// Regular field - not indexed, not searchable
    /// Stored but cannot be filtered or sorted
    metadata: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Document])]
struct DocumentClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("field_attrs");
    let mut client = DocumentClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut docs = client.documents();

    // Create test documents using snugom_create! macro
    let now = Utc::now();
    let next_week = now + chrono::Duration::days(7);

    let doc1_id = snugom_create!(client, Document {
        title: "Getting Started Guide".to_string(),
        content: "This guide will help you get started with our platform".to_string(),
        status: "published".to_string(),
        category: "tutorial".to_string(),
        priority: 1,
        views: 1500,
        published: true,
        due_date: Some(next_week),
        metadata: "internal use".to_string(),
        created_at: now,
        updated_at: now,
    }).await?.id;

    snugom_create!(client, Document {
        title: "API Reference".to_string(),
        content: "Complete API documentation for developers".to_string(),
        status: "draft".to_string(),
        category: "reference".to_string(),
        priority: 2,
        views: 500,
        published: false,
        due_date: None,
        metadata: "needs review".to_string(),
        created_at: now,
        updated_at: now,
    }).await?;

    // ============ TAG Field Query (Exact Match) ============
    let query = SearchQuery {
        filter: vec!["status:eq:published".to_string()],
        ..Default::default()
    };
    let published = docs.find_many(query).await?;
    assert_eq!(published.items.len(), 1);
    assert_eq!(published.items[0].status, "published");

    // ============ TEXT Field Query (Prefix) ============
    let query = SearchQuery {
        filter: vec!["title:prefix:Getting".to_string()],
        ..Default::default()
    };
    let prefixed = docs.find_many(query).await?;
    assert_eq!(prefixed.items.len(), 1);

    // ============ NUMERIC Field Query (Range) ============
    let query = SearchQuery {
        filter: vec!["views:range:1000,".to_string()], // >= 1000
        ..Default::default()
    };
    let high_views = docs.find_many(query).await?;
    assert_eq!(high_views.items.len(), 1);
    assert_eq!(high_views.items[0].views, 1500);

    // ============ BOOLEAN Field Query ============
    // Boolean fields use eq operator with true/false
    let query = SearchQuery {
        filter: vec!["published:eq:true".to_string()],
        ..Default::default()
    };
    let is_published = docs.find_many(query).await?;
    assert_eq!(is_published.items.len(), 1);

    // ============ Sorting by Priority ============
    let query = SearchQuery {
        sort_by: Some("priority".to_string()),
        sort_order: Some(SortOrder::Asc), // Explicitly ascending
        ..Default::default()
    };
    let sorted = docs.find_many(query).await?;
    assert_eq!(sorted.items.len(), 2);
    assert_eq!(sorted.items[0].priority, 1); // Lower priority first

    // ============ Non-indexed Field Cannot Be Searched ============
    // The 'metadata' field is not indexed, so we cannot filter on it
    // This is by design - not all fields need to be searchable

    // Verify document was created with correct data
    let fetched = docs.get_or_error(&doc1_id).await?;
    assert_eq!(fetched.metadata, "internal use");

    Ok(())
}
