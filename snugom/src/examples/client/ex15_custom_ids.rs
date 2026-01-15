//! Example 15 – Custom IDs
//!
//! Demonstrates ID customization options:
//! - Auto-generated IDs (default)
//! - Custom ID field names
//! - Providing your own ID values

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create};

/// Entity with default ID field named "id" and auto-generation.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "default_id_items")]
struct DefaultIdItem {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    name: String,
}

/// Entity with custom ID field name.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "orders")]
struct Order {
    /// Custom ID field name - "order_id" instead of "id"
    #[snugom(id)]
    order_id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    customer_id: String,
    #[snugom(filterable, sortable)]
    total: i64,
}

/// Entity with product_sku as the ID.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "inventory")]
struct InventoryItem {
    #[snugom(id)]
    sku: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    name: String,
    #[snugom(filterable, sortable)]
    quantity: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [DefaultIdItem, Order, InventoryItem])]
struct ExampleClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("custom_ids");
    let client = ExampleClient::new(conn, prefix);

    // ============ Auto-generated ID ============
    {
        let mut items = client.default_id_items();

        // Create without specifying ID - it will be auto-generated
        let created_id = snugom_create!(client, DefaultIdItem {
            name: "Auto-generated ID item".to_string(),
            created_at: Utc::now(),
        }).await?.id;

        // ID is automatically generated (typically a UUID-like string)
        assert!(!created_id.is_empty(), "ID should be auto-generated");
        assert!(created_id.len() > 10, "auto-generated ID should be substantial");

        // Fetch by auto-generated ID
        let fetched = items.get_or_error(&created_id).await?;
        assert_eq!(fetched.name, "Auto-generated ID item");
    }

    // ============ Custom ID Field Name ============
    {
        let mut orders = client.orders();

        let order_id = snugom_create!(client, Order {
            customer_id: "cust_123".to_string(),
            total: 9999,
            created_at: Utc::now(),
        }).await?.id;

        // The field is named order_id, but it's still auto-generated
        assert!(!order_id.is_empty());

        // Access by the custom ID field
        let fetched = orders.get_or_error(&order_id).await?;
        assert_eq!(fetched.total, 9999);
    }

    // ============ User-Provided ID ============
    {
        let mut inventory = client.inventory_items();

        // Provide your own SKU as the ID
        let created = snugom_create!(client, InventoryItem {
            sku: "SKU-001-WIDGET".to_string(), // User-provided ID
            name: "Widget".to_string(),
            quantity: 100,
            created_at: Utc::now(),
        }).await?;

        // The provided SKU is used as the ID
        assert_eq!(created.id, "SKU-001-WIDGET");

        // Fetch by the user-provided SKU
        let fetched = inventory.get_or_error("SKU-001-WIDGET").await?;
        assert_eq!(fetched.name, "Widget");
        assert_eq!(fetched.quantity, 100);

        // Create another with different SKU
        let created2 = snugom_create!(client, InventoryItem {
            sku: "SKU-002-GADGET".to_string(),
            name: "Gadget".to_string(),
            quantity: 50,
            created_at: Utc::now(),
        }).await?;

        assert_eq!(created2.id, "SKU-002-GADGET");

        // Verify both exist
        assert!(inventory.exists("SKU-001-WIDGET").await?);
        assert!(inventory.exists("SKU-002-GADGET").await?);
        assert_eq!(inventory.count().await?, 2);
    }

    Ok(())
}
