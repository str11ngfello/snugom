//! Integration tests for the Prisma-style Client API.
//!
//! These tests verify the complete client workflow including:
//! - Client and CollectionHandle operations
//! - SnugomClient derive macro with named accessors
//! - Bulk operations (create_many, delete_many, update_many)
//! - Entity auto-registration

use chrono::Utc;
use serde::{Deserialize, Serialize};
use snugom::{Client, CollectionHandle, SnugomClient, SnugomEntity};

// ============ Test Entities ============

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[snugom(schema = 1, service = "test_client", collection = "widgets")]
struct Widget {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(tag))]
    category: String,
    #[snugom(filterable, sortable)]
    price: i64,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[snugom(schema = 1, service = "test_client", collection = "gadgets")]
struct Gadget {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(tag))]
    widget_id: String,
}

// ============ Custom Client with Named Accessors ============

#[derive(SnugomClient)]
#[snugom_client(entities = [Widget, Gadget])]
struct TestClient {
    conn: snugom::ConnectionManager,
    prefix: String,
}

// ============ Helper Functions ============

async fn create_test_client() -> Client {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let prefix = format!("test_client_{}", uuid::Uuid::new_v4());
    Client::connect(&redis_url, prefix).await.expect("Failed to connect to Redis")
}

async fn create_custom_client() -> TestClient {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let prefix = format!("test_client_{}", uuid::Uuid::new_v4());
    TestClient::connect(&redis_url, prefix).await.expect("Failed to connect to Redis")
}

async fn cleanup_client(client: &Client) {
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Tests: Basic CRUD ============

#[tokio::test]
async fn test_client_create_and_get() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Test Widget".to_string())
        .category("electronics".to_string())
        .price(100)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");
    let widget_id = result.id.clone();

    // Get by ID
    let fetched = widgets.get(&widget_id).await.expect("get failed");
    assert!(fetched.is_some());
    let widget = fetched.unwrap();
    assert_eq!(widget.name, "Test Widget");
    assert_eq!(widget.category, "electronics");
    assert_eq!(widget.price, 100);

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_get_or_error() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Widget".to_string())
        .category("tools".to_string())
        .price(50)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");

    // get_or_error should succeed
    let widget = widgets.get_or_error(&result.id).await.expect("get_or_error failed");
    assert_eq!(widget.name, "Widget");

    // get_or_error on non-existent should fail
    let err = widgets.get_or_error("nonexistent").await;
    assert!(err.is_err());

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_exists_and_count() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Initially empty
    assert_eq!(widgets.count().await.expect("count failed"), 0);

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Counter".to_string())
        .category("test".to_string())
        .price(1)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");

    // Check existence
    assert!(widgets.exists(&result.id).await.expect("exists failed"));
    assert!(!widgets.exists("nonexistent").await.expect("exists failed"));

    // Check count
    assert_eq!(widgets.count().await.expect("count failed"), 1);

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_delete() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create and then delete
    let builder = Widget::validation_builder()
        .name("Deletable".to_string())
        .category("temp".to_string())
        .price(0)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");
    assert!(widgets.exists(&result.id).await.expect("exists failed"));

    widgets.delete(&result.id).await.expect("delete failed");
    assert!(!widgets.exists(&result.id).await.expect("exists failed"));

    cleanup_client(&client).await;
}

// ============ Tests: Named Accessors ============

#[tokio::test]
async fn test_snugom_client_named_accessors() {
    let client = create_custom_client().await;

    // Use named accessors
    let mut widgets = client.widgets();
    let mut gadgets = client.gadgets();

    // Create a widget
    let widget_builder = Widget::validation_builder()
        .name("Named Widget".to_string())
        .category("named".to_string())
        .price(200)
        .created_at(Utc::now());

    let widget_result = widgets.create(widget_builder).await.expect("create widget failed");

    // Create a gadget linked to the widget
    let gadget_builder = Gadget::validation_builder()
        .name("Named Gadget".to_string())
        .widget_id(widget_result.id.clone())
        .created_at(Utc::now());

    let gadget_result = gadgets.create(gadget_builder).await.expect("create gadget failed");

    // Verify both exist
    assert!(widgets.exists(&widget_result.id).await.expect("exists failed"));
    assert!(gadgets.exists(&gadget_result.id).await.expect("exists failed"));

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Tests: Bulk Operations ============

#[tokio::test]
async fn test_client_create_many() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create multiple widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Widget 1".to_string())
            .category("bulk".to_string())
            .price(10)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget 2".to_string())
            .category("bulk".to_string())
            .price(20)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget 3".to_string())
            .category("bulk".to_string())
            .price(30)
            .created_at(Utc::now()),
    ];

    let result = widgets.create_many(builders).await.expect("create_many failed");
    assert_eq!(result.count, 3);
    assert_eq!(result.ids.len(), 3);

    // Verify count
    assert_eq!(widgets.count().await.expect("count failed"), 3);

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_delete_many_by_ids() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Delete 1".to_string())
            .category("delete".to_string())
            .price(1)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Delete 2".to_string())
            .category("delete".to_string())
            .price(2)
            .created_at(Utc::now()),
    ];

    let result = widgets.create_many(builders).await.expect("create_many failed");

    // Delete by IDs
    let ids: Vec<&str> = result.ids.iter().map(|s| s.as_str()).collect();
    let deleted = widgets.delete_many_by_ids(&ids).await.expect("delete_many_by_ids failed");
    assert_eq!(deleted, 2);

    // Verify gone
    assert_eq!(widgets.count().await.expect("count failed"), 0);

    cleanup_client(&client).await;
}

// ============ Tests: Entity Registration ============

#[test]
fn test_entity_registration() {
    use snugom::client::{get_entity_by_collection, get_entity_by_name, is_entity_registered};

    // Widget should be registered
    assert!(is_entity_registered::<Widget>());
    assert!(is_entity_registered::<Gadget>());

    // Can look up by name
    let widget_reg = get_entity_by_name("Widget");
    assert!(widget_reg.is_some());
    assert_eq!(widget_reg.unwrap().collection_name, "widgets");
    assert_eq!(widget_reg.unwrap().service_name, "test_client");

    // Can look up by collection
    let gadget_reg = get_entity_by_collection("gadgets");
    assert!(gadget_reg.is_some());
    assert_eq!(gadget_reg.unwrap().type_name, "Gadget");
}

// ============ Tests: SnugomModel Trait ============

#[test]
fn test_snugom_model_trait() {
    use snugom::SnugomModel;

    // Check constants
    assert_eq!(Widget::SERVICE, "test_client");
    assert_eq!(Widget::COLLECTION, "widgets");

    assert_eq!(Gadget::SERVICE, "test_client");
    assert_eq!(Gadget::COLLECTION, "gadgets");
}

#[test]
fn test_snugom_model_get_id() {
    use snugom::SnugomModel;

    let widget = Widget {
        id: "test_id_123".to_string(),
        created_at: Utc::now(),
        name: "Test".to_string(),
        category: "test".to_string(),
        price: 100,
    };

    assert_eq!(widget.get_id(), "test_id_123");
}

// ============ Tests: Connection Methods ============

#[tokio::test]
async fn test_client_connection_methods() {
    let client = create_test_client().await;

    // Test prefix accessor
    assert!(client.prefix().starts_with("test_client_"));

    // Test connection accessor
    let _conn = client.connection();

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_custom_client_collection_generic() {
    let client = create_custom_client().await;

    // Test generic collection method
    let mut widgets: CollectionHandle<Widget> = client.collection::<Widget>();

    let builder = Widget::validation_builder()
        .name("Generic".to_string())
        .category("generic".to_string())
        .price(999)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");
    assert!(widgets.exists(&result.id).await.expect("exists failed"));

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Tests: Query-based Operations ============

#[tokio::test]
async fn test_client_find_first() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Alpha Widget".to_string())
            .category("electronics".to_string())
            .price(100)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Beta Widget".to_string())
            .category("electronics".to_string())
            .price(200)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Gamma Widget".to_string())
            .category("tools".to_string())
            .price(150)
            .created_at(Utc::now()),
    ];

    widgets.create_many(builders).await.expect("create_many failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Find first with tag filter
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:electronics".to_string()],
        ..Default::default()
    };
    let result = widgets.find_first(query).await.expect("find_first failed");
    assert!(result.is_some());
    let widget = result.unwrap();
    assert_eq!(widget.category, "electronics");

    // Find first with no matches
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:nonexistent".to_string()],
        ..Default::default()
    };
    let result = widgets.find_first(query).await.expect("find_first failed");
    assert!(result.is_none());

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_find_first_or_error() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Findable".to_string())
        .category("searchable".to_string())
        .price(50)
        .created_at(Utc::now());
    widgets.create(builder).await.expect("create failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // find_first_or_error should succeed
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:searchable".to_string()],
        ..Default::default()
    };
    let widget = widgets.find_first_or_error(query).await.expect("find_first_or_error failed");
    assert_eq!(widget.name, "Findable");

    // find_first_or_error on no match should fail
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:nonexistent".to_string()],
        ..Default::default()
    };
    let err = widgets.find_first_or_error(query).await;
    assert!(err.is_err());

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_find_many() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create widgets with different categories
    let builders = vec![
        Widget::validation_builder()
            .name("Widget A".to_string())
            .category("category_a".to_string())
            .price(100)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget B".to_string())
            .category("category_a".to_string())
            .price(200)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget C".to_string())
            .category("category_b".to_string())
            .price(150)
            .created_at(Utc::now()),
    ];

    widgets.create_many(builders).await.expect("create_many failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Find all in category_a
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:category_a".to_string()],
        ..Default::default()
    };
    let result = widgets.find_many(query).await.expect("find_many failed");
    assert_eq!(result.total, 2);
    assert_eq!(result.items.len(), 2);

    // Find all - no filter
    let query = snugom::search::SearchQuery::default();
    let result = widgets.find_many(query).await.expect("find_many failed");
    assert_eq!(result.total, 3);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_count_where() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Count 1".to_string())
            .category("countable".to_string())
            .price(10)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Count 2".to_string())
            .category("countable".to_string())
            .price(20)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Count 3".to_string())
            .category("other".to_string())
            .price(30)
            .created_at(Utc::now()),
    ];

    widgets.create_many(builders).await.expect("create_many failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Count with filter
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:countable".to_string()],
        ..Default::default()
    };
    let count = widgets.count_where(query).await.expect("count_where failed");
    assert_eq!(count, 2);

    // Count all
    assert_eq!(widgets.count().await.expect("count failed"), 3);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_exists_where() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Exists Test".to_string())
        .category("exists_category".to_string())
        .price(100)
        .created_at(Utc::now());
    widgets.create(builder).await.expect("create failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // exists_where should find it
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:exists_category".to_string()],
        ..Default::default()
    };
    assert!(widgets.exists_where(query).await.expect("exists_where failed"));

    // exists_where should not find nonexistent
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:nonexistent".to_string()],
        ..Default::default()
    };
    assert!(!widgets.exists_where(query).await.expect("exists_where failed"));

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_delete_many_query() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Delete A".to_string())
            .category("deletable".to_string())
            .price(10)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Delete B".to_string())
            .category("deletable".to_string())
            .price(20)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Keep".to_string())
            .category("keep".to_string())
            .price(30)
            .created_at(Utc::now()),
    ];

    widgets.create_many(builders).await.expect("create_many failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Delete all deletable
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:deletable".to_string()],
        ..Default::default()
    };
    let deleted = widgets.delete_many(query).await.expect("delete_many failed");
    assert_eq!(deleted, 2);

    // Wait for index update
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Only 1 should remain
    assert_eq!(widgets.count().await.expect("count failed"), 1);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_update_many_query() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Ensure index exists
    client.ensure_indexes().await.expect("ensure_indexes failed");

    // Create widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Update A".to_string())
            .category("updatable".to_string())
            .price(10)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Update B".to_string())
            .category("updatable".to_string())
            .price(20)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("No Update".to_string())
            .category("not_updatable".to_string())
            .price(30)
            .created_at(Utc::now()),
    ];

    widgets.create_many(builders).await.expect("create_many failed");

    // Wait for indexing
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Update all updatable widgets - change price to 999
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:updatable".to_string()],
        ..Default::default()
    };
    let updated = widgets
        .update_many(query, |id| {
            Widget::patch_builder()
                .entity_id(id)
                .price(999)
        })
        .await
        .expect("update_many failed");
    assert_eq!(updated, 2);

    // Verify prices were updated
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:updatable".to_string()],
        ..Default::default()
    };
    let result = widgets.find_many(query).await.expect("find_many failed");
    for widget in result.items {
        assert_eq!(widget.price, 999);
    }

    // Not updatable should still have original price
    let query = snugom::search::SearchQuery {
        filter: vec!["category:eq:not_updatable".to_string()],
        ..Default::default()
    };
    let widget = widgets.find_first(query).await.expect("find_first failed").unwrap();
    assert_eq!(widget.price, 30);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_upsert() {
    let client = create_custom_client().await;
    let mut widgets = client.widgets();

    let unique_id = format!("upsert_test_{}", uuid::Uuid::new_v4());

    // First upsert - should create
    let create_builder = Widget::validation_builder()
        .id(unique_id.clone())
        .name("Upserted".to_string())
        .category("upsert".to_string())
        .price(100)
        .created_at(Utc::now());

    let update_builder = Widget::patch_builder()
        .entity_id(&unique_id)
        .price(200);

    let result = widgets.upsert(create_builder, update_builder).await.expect("upsert failed");
    assert!(matches!(result, snugom::UpsertResult::Created(_)));

    // Verify created with price 100
    let widget = widgets.get_or_error(&unique_id).await.expect("get failed");
    assert_eq!(widget.price, 100);
    assert_eq!(widget.name, "Upserted");

    // Second upsert - should update
    let create_builder = Widget::validation_builder()
        .id(unique_id.clone())
        .name("Upserted Again".to_string())
        .category("upsert".to_string())
        .price(100)
        .created_at(Utc::now());

    let update_builder = Widget::patch_builder()
        .entity_id(&unique_id)
        .price(200);

    let result = widgets.upsert(create_builder, update_builder).await.expect("upsert failed");
    assert!(matches!(result, snugom::UpsertResult::Updated(_)));

    // Verify updated - price should be 200 (from update), name should still be "Upserted"
    let widget = widgets.get_or_error(&unique_id).await.expect("get failed");
    assert_eq!(widget.price, 200);
    assert_eq!(widget.name, "Upserted"); // Name not in update patch

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_client_create_and_get_method() {
    let client = create_custom_client().await;
    let mut widgets = client.widgets();

    // Create and get in one step
    let builder = Widget::validation_builder()
        .name("Create And Get".to_string())
        .category("test".to_string())
        .price(42)
        .created_at(Utc::now());

    let widget = widgets.create_and_get(builder).await.expect("create_and_get failed");
    assert_eq!(widget.name, "Create And Get");
    assert_eq!(widget.price, 42);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Test: Run Client Hello Example ============

#[tokio::test]
async fn test_client_hello_example() {
    snugom::examples::client::ex01_hello_client::run()
        .await
        .expect("client hello example should succeed");
}

// ============ Tests: Macro DSL (snugom_create!, snugom_update!, etc.) ============

#[tokio::test]
async fn test_snugom_create_macro() {
    let client = create_custom_client().await;

    // Use snugom_create! macro to create a widget
    let result = snugom::snugom_create!(client, Widget {
        name: "Macro Created".to_string(),
        category: "macro_test".to_string(),
        price: 150,
        created_at: Utc::now(),
    }).await.expect("snugom_create failed");

    // Verify the widget was created
    let mut widgets = client.widgets();
    let widget = widgets.get_or_error(&result.id).await.expect("get failed");
    assert_eq!(widget.name, "Macro Created");
    assert_eq!(widget.category, "macro_test");
    assert_eq!(widget.price, 150);

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_snugom_update_macro() {
    let client = create_custom_client().await;
    let mut widgets = client.widgets();

    // First create a widget
    let builder = Widget::validation_builder()
        .name("Before Update".to_string())
        .category("update_test".to_string())
        .price(100)
        .created_at(Utc::now());

    let created = widgets.create(builder).await.expect("create failed");
    let widget_id = created.id.clone();

    // Use snugom_update! macro to update the widget
    // With borrowing semantics, we can use &widget_id directly without cloning
    snugom::snugom_update!(client, Widget(entity_id = &widget_id) {
        name: "After Update".to_string(),
        price: 200,
    }).await.expect("snugom_update failed");

    // Verify the update - widget_id is still available after the macro
    let updated = widgets.get_or_error(&widget_id).await.expect("get failed");
    assert_eq!(updated.name, "After Update");
    assert_eq!(updated.price, 200);
    assert_eq!(updated.category, "update_test"); // Unchanged

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_snugom_delete_macro() {
    let mut client = create_custom_client().await;
    let mut widgets = client.widgets();

    // First create a widget
    let builder = Widget::validation_builder()
        .name("To Be Deleted".to_string())
        .category("delete_test".to_string())
        .price(50)
        .created_at(Utc::now());

    let created = widgets.create(builder).await.expect("create failed");
    let widget_id = created.id.clone();

    // Verify it exists
    assert!(widgets.exists(&widget_id).await.expect("exists failed"));

    // Use snugom_delete! macro to delete the widget
    snugom::snugom_delete!(client, Widget(&widget_id)).await.expect("snugom_delete failed");

    // Verify it's deleted
    assert!(!widgets.exists(&widget_id).await.expect("exists failed"));

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

#[tokio::test]
async fn test_snugom_upsert_macro() {
    let client = create_custom_client().await;
    let mut widgets = client.widgets();

    let unique_id = uuid::Uuid::new_v4().to_string();

    // With borrowing semantics, we can reuse the same ID without cloning for each use
    // First upsert - should create
    snugom::snugom_upsert!(client, Widget(id = &unique_id) {
        create: Widget {
            id: unique_id.clone(),
            name: "Upsert Created".to_string(),
            category: "upsert_test".to_string(),
            price: 100,
            created_at: Utc::now(),
        },
        update: Widget(entity_id = &unique_id) {
            price: 200,
        },
    }).await.expect("snugom_upsert failed");

    // Verify it was created - unique_id is still available after the macro
    let widget = widgets.get_or_error(&unique_id).await.expect("get failed");
    assert_eq!(widget.name, "Upsert Created");
    assert_eq!(widget.price, 100); // Create price, not update price

    // Second upsert - should update (entity already exists)
    snugom::snugom_upsert!(client, Widget(id = &unique_id) {
        create: Widget {
            id: unique_id.clone(),
            name: "New Name".to_string(),
            category: "upsert_test".to_string(),
            price: 300,
            created_at: Utc::now(),
        },
        update: Widget(entity_id = &unique_id) {
            price: 500,
        },
    }).await.expect("snugom_upsert failed");

    // Verify it was updated - unique_id is still available
    let widget = widgets.get_or_error(&unique_id).await.expect("get failed");
    assert_eq!(widget.name, "Upsert Created"); // Unchanged - update only touches price
    assert_eq!(widget.price, 500); // Update price

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Tests: Constructor Methods ============

#[tokio::test]
async fn test_client_new_constructor() {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let redis_client = redis::Client::open(redis_url).expect("Failed to open redis client");
    let conn = snugom::ConnectionManager::new(redis_client).await.expect("Failed to create connection");

    let prefix = format!("test_new_{}", uuid::Uuid::new_v4());
    let client = TestClient::new(conn, prefix.clone());

    // Verify prefix is set correctly
    assert_eq!(client.prefix(), prefix);

    // Verify client works
    let mut widgets = client.widgets();
    let builder = Widget::validation_builder()
        .name("New Constructor Test".to_string())
        .category("test".to_string())
        .price(40)
        .created_at(Utc::now());

    let result = widgets.create(builder).await.expect("create failed");
    assert!(widgets.exists(&result.id).await.expect("exists failed"));

    // Cleanup
    let pattern = format!("{}:*", client.prefix());
    let _ = snugom::cleanup_pattern(&mut client.connection(), &pattern).await;
}

// ============ Tests: Additional CollectionHandle Methods ============

#[tokio::test]
async fn test_client_update_and_get() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create a widget first
    let builder = Widget::validation_builder()
        .name("Before Update".to_string())
        .category("update_and_get_test".to_string())
        .price(100)
        .created_at(Utc::now());

    let created = widgets.create(builder).await.expect("create failed");
    let widget_id = created.id.clone();

    // Use update_and_get to update and get the result in one operation
    let patch = Widget::patch_builder()
        .entity_id(&widget_id)
        .name("After Update".to_string())
        .price(200);

    let updated = widgets.update_and_get(&widget_id, patch).await.expect("update_and_get failed");

    // Verify the returned entity has the updated values
    assert_eq!(updated.name, "After Update");
    assert_eq!(updated.price, 200);
    assert_eq!(updated.category, "update_and_get_test"); // Unchanged

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_delete_with_version() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Version Delete Test".to_string())
        .category("version_test".to_string())
        .price(50)
        .created_at(Utc::now());

    let created = widgets.create(builder).await.expect("create failed");
    let widget_id = created.id.clone();

    // Get the version from the create response
    let version = created.responses[0]["version"].as_u64().expect("version should exist");

    // Delete with correct version should succeed
    widgets.delete_with_version(&widget_id, version).await.expect("delete_with_version failed");

    // Verify deletion
    assert!(!widgets.exists(&widget_id).await.expect("exists failed"));

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_delete_with_version_conflict() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create a widget
    let builder = Widget::validation_builder()
        .name("Version Conflict Test".to_string())
        .category("version_test".to_string())
        .price(50)
        .created_at(Utc::now());

    let created = widgets.create(builder).await.expect("create failed");
    let widget_id = created.id.clone();

    // Try to delete with wrong version - should fail
    let wrong_version = 999;
    let result = widgets.delete_with_version(&widget_id, wrong_version).await;

    // Should get a version conflict error
    assert!(result.is_err());

    // Widget should still exist
    assert!(widgets.exists(&widget_id).await.expect("exists failed"));

    cleanup_client(&client).await;
}

#[tokio::test]
async fn test_client_update_many_by_ids() {
    let client = create_test_client().await;
    let mut widgets = client.collection::<Widget>();

    // Create multiple widgets
    let builders = vec![
        Widget::validation_builder()
            .name("Widget A".to_string())
            .category("bulk_update".to_string())
            .price(100)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget B".to_string())
            .category("bulk_update".to_string())
            .price(200)
            .created_at(Utc::now()),
        Widget::validation_builder()
            .name("Widget C".to_string())
            .category("other".to_string())
            .price(300)
            .created_at(Utc::now()),
    ];

    let bulk_result = widgets.create_many(builders).await.expect("create_many failed");
    assert_eq!(bulk_result.count, 3);

    // Update first two widgets by their IDs
    let ids_to_update: Vec<&str> = bulk_result.ids[0..2].iter().map(|s| s.as_str()).collect();
    let updated_count = widgets.update_many_by_ids(&ids_to_update, |id| {
        Widget::patch_builder()
            .entity_id(id)
            .category("updated_category".to_string())
    }).await.expect("update_many_by_ids failed");

    assert_eq!(updated_count, 2);

    // Verify first two were updated
    let widget_a = widgets.get_or_error(&bulk_result.ids[0]).await.expect("get failed");
    assert_eq!(widget_a.category, "updated_category");

    let widget_b = widgets.get_or_error(&bulk_result.ids[1]).await.expect("get failed");
    assert_eq!(widget_b.category, "updated_category");

    // Third should be unchanged
    let widget_c = widgets.get_or_error(&bulk_result.ids[2]).await.expect("get failed");
    assert_eq!(widget_c.category, "other");

    cleanup_client(&client).await;
}
