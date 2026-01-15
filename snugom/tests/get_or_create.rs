//! Comprehensive tests for the atomic get_or_create operation.
//!
//! These tests verify the Lua-based atomic get_or_create that prevents race conditions
//! when getting or creating entities.

use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use snugom::{
    SnugomEntity,
    id::generate_entity_id,
    repository::Repo,
};
use std::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// Test Entities
// ============================================================================

/// Entity with a unique name field for testing unique constraints.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[snugom(schema = 1, service = "get_or_create_test", collection = "settings")]
struct TestSettings {
    #[snugom(id)]
    id: String,
    #[snugom(unique, filterable(tag))]
    name: String,
    #[snugom(filterable(tag))]
    value: String,
    #[snugom(created_at)]
    created_at: DateTime<Utc>,
    #[snugom(updated_at)]
    updated_at: DateTime<Utc>,
}

// ============================================================================
// Test Utilities
// ============================================================================

static TEST_NAMESPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestNamespace {
    prefix: String,
}

impl TestNamespace {
    fn unique() -> Self {
        let idx = TEST_NAMESPACE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let salt = generate_entity_id();
        Self {
            prefix: format!("goc_test_{idx}_{}", &salt[..8]),
        }
    }

    fn settings_repo(&self) -> Repo<TestSettings> {
        Repo::new(self.prefix.clone())
    }
}

async fn redis_conn() -> ConnectionManager {
    let client = redis::Client::open("redis://127.0.0.1/").expect("redis client");
    client.get_connection_manager().await.expect("connection manager")
}

// ============================================================================
// Basic Get Or Create Tests
// ============================================================================

/// Test that get_or_create creates a new entity when it doesn't exist.
#[tokio::test]
async fn get_or_create_creates_when_not_exists() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.settings_repo();

    let entity_id = generate_entity_id();
    let create_builder = TestSettings::validation_builder()
        .id(entity_id.clone())
        .name("test-setting".to_string())
        .value("initial-value".to_string());

    let result = repo.get_or_create(&mut conn, create_builder)
        .await
        .expect("get_or_create should succeed");

    assert!(result.was_created(), "entity should be created");
    let entity = result.into_inner();
    assert_eq!(entity.id, entity_id);
    assert_eq!(entity.name, "test-setting");
    assert_eq!(entity.value, "initial-value");
}

/// Test that get_or_create returns existing entity without modification.
#[tokio::test]
async fn get_or_create_returns_existing_unchanged() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.settings_repo();

    let entity_id = generate_entity_id();

    // First call - creates the entity
    let create_builder1 = TestSettings::validation_builder()
        .id(entity_id.clone())
        .name("test-setting".to_string())
        .value("original-value".to_string());

    let result1 = repo.get_or_create(&mut conn, create_builder1)
        .await
        .expect("first get_or_create should succeed");

    assert!(result1.was_created(), "first call should create");
    let created_entity = result1.into_inner();

    // Second call with DIFFERENT values - should return original
    let create_builder2 = TestSettings::validation_builder()
        .id(entity_id.clone())
        .name("different-name".to_string())
        .value("different-value".to_string());

    let result2 = repo.get_or_create(&mut conn, create_builder2)
        .await
        .expect("second get_or_create should succeed");

    assert!(result2.was_found(), "second call should find existing");
    let found_entity = result2.into_inner();

    // Verify the entity was NOT modified
    assert_eq!(found_entity.id, entity_id);
    assert_eq!(found_entity.name, "test-setting", "name should not change");
    assert_eq!(found_entity.value, "original-value", "value should not change");
    assert_eq!(found_entity.created_at, created_entity.created_at, "created_at should match");
}

/// Test that get_or_create enforces unique constraints on the create path.
#[tokio::test]
async fn get_or_create_enforces_unique_constraint() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.settings_repo();

    // Create first entity
    let entity_id1 = generate_entity_id();
    let create_builder1 = TestSettings::validation_builder()
        .id(entity_id1.clone())
        .name("unique-name".to_string())
        .value("value1".to_string());

    let result1 = repo.get_or_create(&mut conn, create_builder1)
        .await
        .expect("first create should succeed");
    assert!(result1.was_created());

    // Try to create another entity with same unique name but different ID
    let entity_id2 = generate_entity_id();
    let create_builder2 = TestSettings::validation_builder()
        .id(entity_id2)
        .name("unique-name".to_string()) // Same name - unique violation
        .value("value2".to_string());

    let result2 = repo.get_or_create(&mut conn, create_builder2).await;
    assert!(result2.is_err(), "should fail due to unique constraint violation");
}

/// Test that concurrent get_or_create calls don't create duplicate entities.
#[tokio::test]
async fn get_or_create_prevents_race_condition() {
    let ns = TestNamespace::unique();

    // Use a shared entity ID for both calls
    let shared_id = generate_entity_id();

    // Run two get_or_create calls concurrently with the same entity ID
    let ns_clone = ns.prefix.clone();
    let id_clone = shared_id.clone();
    let handle1 = tokio::spawn(async move {
        let mut conn = redis_conn().await;
        let repo: Repo<TestSettings> = Repo::new(ns_clone);
        let builder = TestSettings::validation_builder()
            .id(id_clone)
            .name("concurrent-test".to_string())
            .value("value-a".to_string());
        repo.get_or_create(&mut conn, builder).await
    });

    let ns_clone = ns.prefix.clone();
    let id_clone = shared_id.clone();
    let handle2 = tokio::spawn(async move {
        let mut conn = redis_conn().await;
        let repo: Repo<TestSettings> = Repo::new(ns_clone);
        let builder = TestSettings::validation_builder()
            .id(id_clone)
            .name("concurrent-test".to_string())
            .value("value-b".to_string());
        repo.get_or_create(&mut conn, builder).await
    });

    let (result1, result2) = tokio::join!(handle1, handle2);
    let result1 = result1.expect("task 1").expect("get_or_create 1");
    let result2 = result2.expect("task 2").expect("get_or_create 2");

    // Exactly one should be Created, the other should be Found
    let created_count = [result1.was_created(), result2.was_created()]
        .iter()
        .filter(|&&b| b)
        .count();
    let found_count = [result1.was_found(), result2.was_found()]
        .iter()
        .filter(|&&b| b)
        .count();

    assert_eq!(created_count, 1, "exactly one should create");
    assert_eq!(found_count, 1, "exactly one should find");

    // Both should have the same entity data
    let entity1 = result1.into_inner();
    let entity2 = result2.into_inner();
    assert_eq!(entity1.id, entity2.id, "both should have same ID");
    assert_eq!(entity1.name, entity2.name, "both should have same name");
}

/// Test GetOrCreateResult helper methods.
#[tokio::test]
async fn get_or_create_result_helpers() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.settings_repo();

    let entity_id = generate_entity_id();
    let create_builder = TestSettings::validation_builder()
        .id(entity_id.clone())
        .name("helper-test".to_string())
        .value("test-value".to_string());

    let result = repo.get_or_create(&mut conn, create_builder)
        .await
        .expect("get_or_create should succeed");

    // Test was_created/was_found
    assert!(result.was_created());
    assert!(!result.was_found());

    // Test as_inner
    assert_eq!(result.as_inner().name, "helper-test");

    // Test into_inner
    let entity = result.into_inner();
    assert_eq!(entity.value, "test-value");
}
