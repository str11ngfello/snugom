//! Example 19 – Multi-Entity Client
//!
//! Demonstrates working with multiple entity types in a single client:
//! - Registering multiple entities in one client
//! - Using typed collection accessors
//! - Cross-entity workflows

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update};

/// A user in the system.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "shop_users")]
struct User {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(unique(case_insensitive))]
    #[snugom(filterable(tag))]
    email: String,
}

/// A product in the catalog.
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

/// An order placed by a user.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "shop_orders")]
struct Order {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag), relation(target = "shop_users"))]
    user_id: String,
    #[snugom(filterable(tag))]
    status: String,
    #[snugom(filterable, sortable)]
    total: i64,

    /// Products in this order
    #[serde(default)]
    #[snugom(relation(target = "products"))]
    products: Vec<String>,
}

/// A review for a product.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "reviews")]
struct Review {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag), relation(target = "shop_users"))]
    user_id: String,
    #[snugom(filterable(tag), relation(target = "products"))]
    product_id: String,
    #[snugom(filterable, sortable)]
    rating: i64,
    content: String,
}

/// A single client manages all entity types.
/// This provides a unified interface to the entire data model.
#[derive(SnugomClient)]
#[snugom_client(entities = [User, Product, Order, Review])]
struct ShopClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("multi_entity");
    let mut client = ShopClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries

    // ============ Get Typed Collection Accessors ============
    // Each accessor is typed to its entity
    let mut users = client.users();
    let mut products = client.products();
    let mut orders = client.orders();
    let mut reviews = client.reviews();

    // ============ Create Entities Across Collections ============

    // Create a user
    let user_id = snugom_create!(client, User {
        name: "Alice".to_string(),
        email: "alice@shop.com".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let user = users.get_or_error(&user_id).await?;

    // Create products
    let laptop_id = snugom_create!(client, Product {
        name: "Laptop Pro".to_string(),
        price: 129900,
        category: "electronics".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let laptop = products.get_or_error(&laptop_id).await?;

    let mouse_id = snugom_create!(client, Product {
        name: "Wireless Mouse".to_string(),
        price: 4999,
        category: "electronics".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let mouse = products.get_or_error(&mouse_id).await?;

    // ============ Cross-Entity Workflow: Place Order ============

    // Create an order for the user
    let order_id = snugom_create!(client, Order {
        user_id: user.id.clone(),
        status: "pending".to_string(),
        total: laptop.price + mouse.price,
        created_at: Utc::now(),
    }).await?.id;

    let order = orders.get_or_error(&order_id).await?;

    // Connect products to the order
    snugom_update!(client, Order(entity_id = order.id.clone()) {
        products: [
            connect laptop.id.clone(),
            connect mouse.id.clone(),
        ],
    }).await?;

    // Update order status
    snugom_update!(client, Order(entity_id = &order.id) {
        status: "confirmed".to_string(),
    }).await?;

    // ============ Cross-Entity Workflow: Add Review ============

    // User reviews the laptop
    snugom_create!(client, Review {
        user_id: user.id.clone(),
        product_id: laptop.id.clone(),
        rating: 5,
        content: "Excellent laptop, highly recommended!".to_string(),
        created_at: Utc::now(),
    }).await?;

    // ============ Query Across Collections ============

    // Find all orders for a user
    let user_orders = orders
        .find_many(crate::SearchQuery {
            filter: vec![format!("user_id:eq:{}", user.id)],
            ..Default::default()
        })
        .await?;

    assert_eq!(user_orders.items.len(), 1);
    assert_eq!(user_orders.items[0].status, "confirmed");

    // Find all reviews for a product
    let product_reviews = reviews
        .find_many(crate::SearchQuery {
            filter: vec![format!("product_id:eq:{}", laptop.id)],
            ..Default::default()
        })
        .await?;

    assert_eq!(product_reviews.items.len(), 1);
    assert_eq!(product_reviews.items[0].rating, 5);

    // Find electronics under a certain price (use range with open lower bound)
    let affordable = products
        .find_many(crate::SearchQuery {
            filter: vec![
                "category:eq:electronics".to_string(),
                "price:range:,9999".to_string(), // Less than 10000
            ],
            ..Default::default()
        })
        .await?;

    assert_eq!(affordable.items.len(), 1);
    assert_eq!(affordable.items[0].name, "Wireless Mouse");

    // ============ Aggregate Queries ============

    // Count products by category
    let electronics_count = products
        .count_where(crate::SearchQuery {
            filter: vec!["category:eq:electronics".to_string()],
            ..Default::default()
        })
        .await?;

    assert_eq!(electronics_count, 2);

    // Check if user has reviews
    let has_reviews = reviews
        .exists_where(crate::SearchQuery {
            filter: vec![format!("user_id:eq:{}", user.id)],
            ..Default::default()
        })
        .await?;

    assert!(has_reviews);

    // ============ Verify Final State ============
    assert_eq!(users.count().await?, 1);
    assert_eq!(products.count().await?, 2);
    assert_eq!(orders.count().await?, 1);
    assert_eq!(reviews.count().await?, 1);

    Ok(())
}
