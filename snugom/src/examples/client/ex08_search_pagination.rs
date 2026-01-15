//! Example 08 – Search with Pagination
//!
//! Demonstrates paginated search results using page-based pagination.

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, SearchQuery, search::SortOrder};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "items")]
struct Item {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable, sortable)]
    order: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Item])]
struct ItemClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("search_pagination");
    let mut client = ItemClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut items = client.items();

    // Create 25 items for pagination testing
    let builders: Vec<_> = (1..=25)
        .map(|i| {
            Item::validation_builder()
                .name(format!("Item {i:02}"))
                .order(i)
                .created_at(Utc::now())
        })
        .collect();

    items.create_many(builders).await?;

    // ============ First Page ============
    let query = SearchQuery {
        page: Some(1),
        page_size: Some(10),
        sort_by: Some("order".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let page1 = items.find_many(query).await?;

    assert_eq!(page1.items.len(), 10, "first page should have 10 items");
    assert_eq!(page1.total, 25, "total should be 25");
    assert_eq!(page1.page, 1);
    assert_eq!(page1.page_size, 10);
    assert!(page1.has_more(), "should have more pages");

    // Verify order
    assert_eq!(page1.items[0].name, "Item 01");
    assert_eq!(page1.items[9].name, "Item 10");

    // ============ Second Page ============
    let query = SearchQuery {
        page: Some(2),
        page_size: Some(10),
        sort_by: Some("order".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let page2 = items.find_many(query).await?;

    assert_eq!(page2.items.len(), 10, "second page should have 10 items");
    assert_eq!(page2.page, 2);
    assert!(page2.has_more(), "should have more pages");

    // Verify continuity
    assert_eq!(page2.items[0].name, "Item 11");
    assert_eq!(page2.items[9].name, "Item 20");

    // ============ Last Page ============
    let query = SearchQuery {
        page: Some(3),
        page_size: Some(10),
        sort_by: Some("order".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let page3 = items.find_many(query).await?;

    assert_eq!(page3.items.len(), 5, "last page should have 5 items");
    assert_eq!(page3.page, 3);
    assert!(!page3.has_more(), "should be last page");

    // ============ Custom Page Size ============
    let query = SearchQuery {
        page: Some(1),
        page_size: Some(5),
        sort_by: Some("order".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let small_page = items.find_many(query).await?;

    assert_eq!(small_page.items.len(), 5);
    assert_eq!(small_page.page_size, 5);
    assert_eq!(small_page.total, 25);

    // ============ Empty Page (Beyond Data) ============
    let query = SearchQuery {
        page: Some(10), // Way beyond data
        page_size: Some(10),
        sort_by: Some("order".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let empty = items.find_many(query).await?;

    assert_eq!(empty.items.len(), 0, "page beyond data should be empty");
    assert_eq!(empty.total, 25, "total should still be accurate");

    Ok(())
}
