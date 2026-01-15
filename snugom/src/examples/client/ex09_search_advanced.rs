//! Example 09 – Advanced Search
//!
//! Demonstrates advanced search features:
//! - Multiple filter types (tag, text, numeric, boolean)
//! - Filter operators (eq, range, prefix, contains, exact, fuzzy)
//! - Combining multiple filters
//! - Free-text search with `q` parameter

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, SearchQuery};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "events")]
struct Event {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text), searchable)]
    title: String,
    #[snugom(filterable(text), searchable)]
    description: String,
    #[snugom(filterable(tag))]
    category: String,
    #[snugom(filterable(tag))]
    location: String,
    #[snugom(filterable, sortable)]
    capacity: i64,
    #[snugom(filterable, sortable)]
    price: i64,
    #[snugom(filterable)]
    is_online: bool,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Event])]
struct EventClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("search_advanced");
    let mut client = EventClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut events = client.events();

    // Create diverse test data
    let test_events = vec![
        Event::validation_builder()
            .title("Rust Workshop for Beginners".to_string())
            .description("Learn Rust from scratch in this hands-on workshop".to_string())
            .category("tech".to_string())
            .location("new-york".to_string())
            .capacity(50)
            .price(99)
            .is_online(false)
            .created_at(Utc::now()),
        Event::validation_builder()
            .title("Advanced Rust Patterns".to_string())
            .description("Deep dive into advanced Rust programming patterns".to_string())
            .category("tech".to_string())
            .location("san-francisco".to_string())
            .capacity(30)
            .price(199)
            .is_online(false)
            .created_at(Utc::now()),
        Event::validation_builder()
            .title("Online Rust Meetup".to_string())
            .description("Virtual gathering for Rust enthusiasts worldwide".to_string())
            .category("tech".to_string())
            .location("online".to_string())
            .capacity(500)
            .price(0)
            .is_online(true)
            .created_at(Utc::now()),
        Event::validation_builder()
            .title("Yoga in the Park".to_string())
            .description("Morning yoga session in Central Park".to_string())
            .category("wellness".to_string())
            .location("new-york".to_string())
            .capacity(25)
            .price(15)
            .is_online(false)
            .created_at(Utc::now()),
        Event::validation_builder()
            .title("Photography Workshop".to_string())
            .description("Learn landscape photography techniques".to_string())
            .category("art".to_string())
            .location("san-francisco".to_string())
            .capacity(15)
            .price(75)
            .is_online(false)
            .created_at(Utc::now()),
    ];

    events.create_many(test_events).await?;

    // ============ TAG Filter (eq) ============
    // Find all tech events
    let query = SearchQuery {
        filter: vec!["category:eq:tech".to_string()],
        ..Default::default()
    };
    let tech_events = events.find_many(query).await?;
    assert_eq!(tech_events.items.len(), 3, "should find 3 tech events");

    // ============ BOOLEAN Filter ============
    // Find online events (boolean fields use eq operator with true/false)
    let query = SearchQuery {
        filter: vec!["is_online:eq:true".to_string()],
        ..Default::default()
    };
    let online_events = events.find_many(query).await?;
    assert_eq!(online_events.items.len(), 1, "should find 1 online event");

    // ============ NUMERIC Range Filter ============
    // Find events with price between 50 and 150
    let query = SearchQuery {
        filter: vec!["price:range:50,150".to_string()],
        ..Default::default()
    };
    let mid_price = events.find_many(query).await?;
    assert_eq!(mid_price.items.len(), 2, "should find 2 mid-priced events");

    // ============ NUMERIC Range with Open Bound ============
    // Find events with capacity >= 100 (no upper bound)
    let query = SearchQuery {
        filter: vec!["capacity:range:100,".to_string()],
        ..Default::default()
    };
    let large_events = events.find_many(query).await?;
    assert_eq!(large_events.items.len(), 1, "should find 1 large event");

    // ============ Combining Multiple Filters (AND) ============
    // Find tech events in new-york
    let query = SearchQuery {
        filter: vec![
            "category:eq:tech".to_string(),
            "location:eq:new-york".to_string(),
        ],
        ..Default::default()
    };
    let ny_tech = events.find_many(query).await?;
    assert_eq!(ny_tech.items.len(), 1, "should find 1 NYC tech event");

    // ============ TEXT Prefix Filter ============
    // Find events with title starting with "Rust"
    let query = SearchQuery {
        filter: vec!["title:prefix:Rust".to_string()],
        ..Default::default()
    };
    let rust_prefix = events.find_many(query).await?;
    // Note: depends on tokenization - "Rust" matches start of title words
    assert!(rust_prefix.items.len() >= 2, "should find Rust-prefixed events");

    // ============ TEXT Contains Filter ============
    // Find events with "workshop" anywhere in description
    let query = SearchQuery {
        filter: vec!["description:contains:workshop".to_string()],
        ..Default::default()
    };
    let workshop_events = events.find_many(query).await?;
    assert!(!workshop_events.items.is_empty(), "should find workshop events");

    // ============ Free-text Search (q parameter) ============
    // Search for "beginners" across text fields
    let query = SearchQuery {
        q: Some("beginners".to_string()),
        ..Default::default()
    };
    // Note: `q` is processed by the entity's text_search_fields()
    let beginner_events = events.find_many(query).await?;
    assert!(!beginner_events.items.is_empty(), "should find beginner events");

    // ============ Combined: Filter + Free-text ============
    // Tech events containing "rust"
    let query = SearchQuery {
        filter: vec!["category:eq:tech".to_string()],
        q: Some("rust".to_string()),
        ..Default::default()
    };
    let rust_tech = events.find_many(query).await?;
    assert!(rust_tech.items.len() >= 2, "should find Rust tech events");

    Ok(())
}
