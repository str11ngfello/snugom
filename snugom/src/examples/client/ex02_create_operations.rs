//! Example 02 – Create Operations
//!
//! Demonstrates creating entities using the `snugom_create!` macro DSL:
//! - Single entity creation with struct-literal syntax
//! - Getting the created entity back
//! - Bulk creation with `create_many()` (collection-level operation)

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "products")]
struct Product {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable, sortable)]
    price: i64,
    #[snugom(filterable(tag))]
    category: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Product])]
struct ProductClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("create_ops");
    let client = ProductClient::new(conn, prefix);
    let mut products = client.products();

    // ============ snugom_create! macro ============
    // Uses Prisma-style struct literal syntax
    // Returns CreateResult with id and responses
    let created = snugom_create!(client, Product {
        name: "Laptop".to_string(),
        price: 999,
        category: "electronics".to_string(),
        created_at: Utc::now(),
    }).await?;

    // CreateResult gives you the ID
    let laptop_id = created.id.clone();
    assert!(!laptop_id.is_empty(), "should have an ID");

    // Fetch full entity if needed
    let laptop = products.get_or_error(&laptop_id).await?;
    assert_eq!(laptop.name, "Laptop");
    assert_eq!(laptop.price, 999);

    // ============ Create another entity ============
    let created = snugom_create!(client, Product {
        name: "Smartphone".to_string(),
        price: 699,
        category: "electronics".to_string(),
        created_at: Utc::now(),
    }).await?;

    let phone = products.get_or_error(&created.id).await?;
    assert_eq!(phone.name, "Smartphone");
    assert_eq!(phone.price, 699);
    assert!(!phone.id.is_empty());

    // ============ create_many() ============
    // Bulk create uses the collection-level API (still builder pattern for bulk ops)
    let builders = vec![
        Product::validation_builder()
            .name("Headphones".to_string())
            .price(199)
            .category("audio".to_string())
            .created_at(Utc::now()),
        Product::validation_builder()
            .name("Keyboard".to_string())
            .price(149)
            .category("accessories".to_string())
            .created_at(Utc::now()),
        Product::validation_builder()
            .name("Mouse".to_string())
            .price(79)
            .category("accessories".to_string())
            .created_at(Utc::now()),
    ];

    let bulk_result = products.create_many(builders).await?;

    // BulkCreateResult contains count and all IDs
    assert_eq!(bulk_result.count, 3, "should create 3 products");
    assert_eq!(bulk_result.ids.len(), 3, "should have 3 IDs");

    // Verify total count
    assert_eq!(products.count().await?, 5, "should have 5 products total");

    Ok(())
}
