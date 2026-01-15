//! Comprehensive tests for the atomic upsert operation.
//!
//! These tests verify the Lua-based atomic upsert that prevents race conditions
//! when creating or updating entities.

use chrono::{DateTime, Utc};
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use snugom::{
    SnugomEntity, UpsertResult,
    id::generate_entity_id,
    repository::Repo,
};
use std::sync::atomic::{AtomicUsize, Ordering};

// ============================================================================
// Test Entities
// ============================================================================

/// Entity with a unique name field for testing unique constraints.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "upsert_test", collection = "unique_names")]
struct UniqueNameEntity {
    #[snugom(id)]
    id: String,
    #[snugom(unique, filterable(tag))]
    name: String,
    #[snugom(filterable(tag))]
    status: String,
}

/// Entity with case-insensitive unique constraint.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "upsert_test", collection = "ci_slugs")]
struct CaseInsensitiveEntity {
    #[snugom(id)]
    id: String,
    #[snugom(unique(case_insensitive), filterable(tag))]
    slug: String,
}

/// Entity with relations for testing upsert with relation mutations.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "upsert_test", collection = "parents")]
struct UpsertParentEntity {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    name: String,
    #[snugom(created_at)]
    created_at: DateTime<Utc>,
    #[serde(default)]
    #[snugom(relation(target = "children", many_to_many = "parents"))]
    children_ids: Vec<String>,
}

/// Child entity for relation tests.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "upsert_test", collection = "children")]
struct UpsertChildEntity {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    name: String,
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
            prefix: format!("upsert_test_{idx}_{}", &salt[..8]),
        }
    }

    fn unique_name_repo(&self) -> Repo<UniqueNameEntity> {
        Repo::new(self.prefix.clone())
    }

    fn ci_entity_repo(&self) -> Repo<CaseInsensitiveEntity> {
        Repo::new(self.prefix.clone())
    }

    fn parent_repo(&self) -> Repo<UpsertParentEntity> {
        Repo::new(self.prefix.clone())
    }
}

async fn redis_conn() -> ConnectionManager {
    let client = redis::Client::open("redis://127.0.0.1/").expect("redis client");
    client.get_connection_manager().await.expect("connection manager")
}

// ============================================================================
// Basic Upsert Tests
// ============================================================================

/// Test that upsert creates a new entity when it doesn't exist.
#[tokio::test]
async fn upsert_creates_when_not_exists() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    let create_builder = UniqueNameEntity::validation_builder()
        .name("new-entity".to_string())
        .status("created".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = "nonexistent".to_string()) {
            status?: Some("updated".to_string()),
        }
    };

    let result = repo.upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("upsert should succeed");

    match result {
        UpsertResult::Created(create_result) => {
            // Verify the entity was created
            let fetched = repo.get(&mut conn, &create_result.id)
                .await
                .expect("fetch")
                .expect("entity should exist");

            let json = serde_json::to_value(&fetched).expect("serialize");
            assert_eq!(json["name"], "new-entity");
            assert_eq!(json["status"], "created");
        }
        UpsertResult::Updated(_) => panic!("expected Created branch, got Updated"),
    }
}

/// Test that upsert updates an existing entity.
#[tokio::test]
async fn upsert_updates_when_exists() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    // Create the entity first
    let create_builder = UniqueNameEntity::validation_builder()
        .name("existing-entity".to_string())
        .status("original".to_string());

    let created = repo.create_with_conn(&mut conn, create_builder)
        .await
        .expect("create seed entity");

    // Now upsert - should update
    let upsert_create_builder = UniqueNameEntity::validation_builder()
        .name("should-not-create".to_string())
        .status("fallback".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = created.id.clone()) {
            status?: Some("updated".to_string()),
        }
    };

    let result = repo.upsert(&mut conn, upsert_create_builder, update_builder)
        .await
        .expect("upsert should succeed");

    match result {
        UpsertResult::Updated(_) => {
            // Verify the entity was updated
            let fetched = repo.get(&mut conn, &created.id)
                .await
                .expect("fetch")
                .expect("entity should exist");

            let json = serde_json::to_value(&fetched).expect("serialize");
            assert_eq!(json["name"], "existing-entity"); // name unchanged
            assert_eq!(json["status"], "updated"); // status updated
        }
        UpsertResult::Created(_) => panic!("expected Updated branch, got Created"),
    }
}

// ============================================================================
// Unique Constraint Tests
// ============================================================================

/// Test that upsert enforces unique constraints on the create path.
#[tokio::test]
async fn upsert_create_enforces_unique_constraint() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    // Create first entity with unique name
    let first_builder = UniqueNameEntity::validation_builder()
        .name("unique-name".to_string())
        .status("active".to_string());

    repo.create_with_conn(&mut conn, first_builder)
        .await
        .expect("create first entity");

    // Try upsert with same unique name - should fail on create path
    let create_builder = UniqueNameEntity::validation_builder()
        .name("unique-name".to_string()) // conflicts with existing
        .status("should-fail".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = "nonexistent".to_string()) {
            status?: Some("updated".to_string()),
        }
    };

    let result = repo.upsert(&mut conn, create_builder, update_builder).await;

    assert!(result.is_err(), "should fail due to unique constraint violation");
    let err = result.unwrap_err();
    assert!(
        matches!(err, snugom::errors::RepoError::UniqueConstraintViolation { .. }),
        "expected UniqueConstraintViolation, got: {:?}",
        err
    );
}

/// Test that upsert enforces unique constraints on the update path when changing unique field.
#[tokio::test]
async fn upsert_update_enforces_unique_constraint() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    // Create two entities with different unique names
    let first_builder = UniqueNameEntity::validation_builder()
        .name("name-one".to_string())
        .status("active".to_string());

    repo.create_with_conn(&mut conn, first_builder)
        .await
        .expect("create first entity");

    let second_builder = UniqueNameEntity::validation_builder()
        .name("name-two".to_string())
        .status("active".to_string());

    let second = repo.create_with_conn(&mut conn, second_builder)
        .await
        .expect("create second entity");

    // Try to update second entity's name to conflict with first
    let create_builder = UniqueNameEntity::validation_builder()
        .name("fallback".to_string())
        .status("fallback".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = second.id.clone()) {
            name?: Some("name-one".to_string()), // conflicts with first
        }
    };

    let result = repo.upsert(&mut conn, create_builder, update_builder).await;

    assert!(result.is_err(), "should fail due to unique constraint violation");
    let err = result.unwrap_err();
    assert!(
        matches!(err, snugom::errors::RepoError::UniqueConstraintViolation { .. }),
        "expected UniqueConstraintViolation, got: {:?}",
        err
    );
}

/// Test case-insensitive unique constraint enforcement.
#[tokio::test]
async fn upsert_create_enforces_case_insensitive_unique() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.ci_entity_repo();

    // Create first entity with slug
    let first_builder = CaseInsensitiveEntity::validation_builder()
        .slug("my-slug".to_string());

    repo.create_with_conn(&mut conn, first_builder)
        .await
        .expect("create first entity");

    // Try upsert with same slug in different case - should fail
    let create_builder = CaseInsensitiveEntity::validation_builder()
        .slug("MY-SLUG".to_string()); // same as "my-slug" case-insensitive

    let update_builder = snugom::snug! {
        CaseInsensitiveEntity(entity_id = "nonexistent".to_string()) {
            slug?: Some("updated".to_string()),
        }
    };

    let result = repo.upsert(&mut conn, create_builder, update_builder).await;

    assert!(result.is_err(), "should fail due to case-insensitive unique constraint");
}

// ============================================================================
// Idempotency Tests (via Builder API)
// ============================================================================

/// Test that upsert with idempotency key returns same result on replay.
#[tokio::test]
async fn upsert_idempotency_on_create_path() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    let idempotency_key = format!("idem-{}", generate_entity_id());
    let unique_name = format!("idempotent-{}", generate_entity_id());

    // First upsert - creates entity
    let create_builder = UniqueNameEntity::validation_builder()
        .name(unique_name.clone())
        .status("first call".to_string())
        .idempotency_key(idempotency_key.clone());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = "nonexistent".to_string()) {
            status?: Some("updated".to_string()),
        }
    };

    let first_result = repo
        .upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("first upsert");

    let first_id = match &first_result {
        UpsertResult::Created(r) => r.id.clone(),
        UpsertResult::Updated(_) => panic!("expected Created"),
    };

    // Second upsert with same idempotency key - should return cached result
    let create_builder2 = UniqueNameEntity::validation_builder()
        .name(format!("different-{}", generate_entity_id()))
        .status("second call".to_string())
        .idempotency_key(idempotency_key.clone());

    let update_builder2 = snugom::snug! {
        UniqueNameEntity(entity_id = "nonexistent".to_string()) {
            status?: Some("updated".to_string()),
        }
    };

    let second_result = repo
        .upsert(&mut conn, create_builder2, update_builder2)
        .await
        .expect("second upsert");

    let second_id = match &second_result {
        UpsertResult::Created(r) => r.id.clone(),
        UpsertResult::Updated(_) => panic!("expected Created from cached response"),
    };

    assert_eq!(first_id, second_id, "idempotency should return same entity ID");
}

/// Test that upsert with idempotency key works on update path.
#[tokio::test]
async fn upsert_idempotency_on_update_path() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    // Create seed entity
    let seed_builder = UniqueNameEntity::validation_builder()
        .name(format!("seed-{}", generate_entity_id()))
        .status("initial".to_string());

    let seed = repo.create_with_conn(&mut conn, seed_builder)
        .await
        .expect("create seed");

    let idempotency_key = format!("idem-update-{}", generate_entity_id());

    // First upsert - updates entity
    let create_builder = UniqueNameEntity::validation_builder()
        .name("fallback".to_string())
        .status("fallback".to_string())
        .idempotency_key(idempotency_key.clone());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = seed.id.clone()) {
            status?: Some("first update".to_string()),
        }
    }
    .idempotency_key(idempotency_key.clone());

    let _first = repo
        .upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("first upsert");

    // Second upsert with same idempotency key
    let create_builder2 = UniqueNameEntity::validation_builder()
        .name("fallback".to_string())
        .status("fallback".to_string())
        .idempotency_key(idempotency_key.clone());

    let update_builder2 = snugom::snug! {
        UniqueNameEntity(entity_id = seed.id.clone()) {
            status?: Some("second update - should be ignored".to_string()),
        }
    }
    .idempotency_key(idempotency_key.clone());

    let _second = repo
        .upsert(&mut conn, create_builder2, update_builder2)
        .await
        .expect("second upsert");

    // Verify the status wasn't changed by second call
    let fetched = repo.get(&mut conn, &seed.id)
        .await
        .expect("fetch")
        .expect("entity should exist");

    let json = serde_json::to_value(&fetched).expect("serialize");
    assert_eq!(json["status"], "first update", "second call should have been idempotent");
}

// ============================================================================
// Relation Tests (via Builder API)
// ============================================================================

/// Test that upsert creates relations on the create path.
#[tokio::test]
async fn upsert_create_with_relations() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.parent_repo();

    let create_builder = UpsertParentEntity::validation_builder()
        .name("parent-with-children".to_string())
        .created_at(Utc::now())
        .children_ids(Vec::new())
        .connect("children_ids", vec!["child-1".to_string(), "child-2".to_string()]);

    let update_builder = snugom::snug! {
        UpsertParentEntity(entity_id = "nonexistent".to_string()) {
            name: "updated".to_string(),
        }
    };

    let result = repo
        .upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("upsert with relations");

    let entity_id = match result {
        UpsertResult::Created(r) => r.id,
        UpsertResult::Updated(_) => panic!("expected Created"),
    };

    // Verify relations were created
    let rel_key = repo.relation_key("children_ids", &entity_id);
    let members: Vec<String> = conn.smembers(&rel_key).await.expect("get members");
    assert!(members.contains(&"child-1".to_string()));
    assert!(members.contains(&"child-2".to_string()));
}

/// Test that upsert mutates relations on the update path.
#[tokio::test]
async fn upsert_update_with_relations() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.parent_repo();

    // Create seed entity with one child using builder API
    let seed_builder = UpsertParentEntity::validation_builder()
        .name("parent".to_string())
        .created_at(Utc::now())
        .children_ids(Vec::new())
        .connect("children_ids", vec!["existing-child".to_string()]);

    let seed = repo
        .create_with_conn(&mut conn, seed_builder)
        .await
        .expect("create seed");

    // Upsert to add more children and remove existing
    let create_builder = UpsertParentEntity::validation_builder()
        .name("fallback".to_string())
        .created_at(Utc::now())
        .children_ids(Vec::new());

    let update_builder = snugom::snug! {
        UpsertParentEntity(entity_id = seed.id.clone()) {
            name: "updated-parent".to_string(),
        }
    }
    .connect("children_ids", vec!["new-child-1".to_string(), "new-child-2".to_string()])
    .disconnect("children_ids", vec!["existing-child".to_string()]);

    let _result = repo
        .upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("upsert with relation mutations");

    // Verify relations
    let rel_key = repo.relation_key("children_ids", &seed.id);
    let members: Vec<String> = conn.smembers(&rel_key).await.expect("get members");
    assert!(!members.contains(&"existing-child".to_string()), "existing-child should be removed");
    assert!(members.contains(&"new-child-1".to_string()));
    assert!(members.contains(&"new-child-2".to_string()));
}

// ============================================================================
// Race Condition Prevention Test
// ============================================================================

/// Test that concurrent upserts don't create duplicate entities.
/// This is the main reason for the atomic upsert implementation.
#[tokio::test]
async fn upsert_prevents_race_condition() {
    let mut conn1 = redis_conn().await;
    let mut conn2 = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo1 = ns.unique_name_repo();
    let repo2 = ns.unique_name_repo();

    // Use a shared entity ID for both upserts
    let target_id = format!("race-target-{}", generate_entity_id());
    let unique_name = format!("race-name-{}", generate_entity_id());

    // Run two upserts concurrently with the same target entity ID
    let target_id_1 = target_id.clone();
    let unique_name_1 = unique_name.clone();
    let target_id_2 = target_id.clone();
    let unique_name_2 = unique_name.clone();

    let (result1, result2) = tokio::join!(
        async {
            let create_builder = UniqueNameEntity::validation_builder()
                .name(unique_name_1)
                .status("created by task 1".to_string());
            let update_builder = snugom::snug! {
                UniqueNameEntity(entity_id = target_id_1) {
                    status?: Some("update from task 1".to_string()),
                }
            };
            repo1.upsert(&mut conn1, create_builder, update_builder).await
        },
        async {
            let create_builder = UniqueNameEntity::validation_builder()
                .name(unique_name_2)
                .status("created by task 2".to_string());
            let update_builder = snugom::snug! {
                UniqueNameEntity(entity_id = target_id_2) {
                    status?: Some("update from task 2".to_string()),
                }
            };
            repo2.upsert(&mut conn2, create_builder, update_builder).await
        }
    );

    // At least one should succeed
    let successful_results: Vec<_> = [result1, result2]
        .into_iter()
        .filter(|r| r.is_ok())
        .collect();

    assert!(
        !successful_results.is_empty(),
        "at least one upsert should succeed"
    );

    // Count how many entities were created vs updated
    let mut created_count = 0;
    for result in successful_results {
        match result.unwrap() {
            UpsertResult::Created(_) => created_count += 1,
            UpsertResult::Updated(_) => {} // Updated is acceptable
        }
    }

    // With atomic upsert, exactly one should create and possibly one updates
    // (if both succeeded, the second one would see the first's creation)
    assert!(
        created_count <= 1,
        "at most one entity should be created, but got {} creations",
        created_count
    );
}

// ============================================================================
// Version Increment Tests
// ============================================================================

/// Test that upsert increments version on update path.
#[tokio::test]
async fn upsert_increments_version_on_update() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    // Create seed entity (version 1)
    let seed_builder = UniqueNameEntity::validation_builder()
        .name(format!("version-test-{}", generate_entity_id()))
        .status("v1".to_string());

    let seed = repo.create_with_conn(&mut conn, seed_builder)
        .await
        .expect("create seed");

    // Verify initial version
    let key = repo.entity_key(&seed.id);
    let raw: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("get json");
    let json: Value = serde_json::from_str(&raw).expect("parse");
    assert_eq!(json[0]["metadata"]["version"], 1);

    // Upsert to update (should increment to version 2)
    let create_builder = UniqueNameEntity::validation_builder()
        .name("fallback".to_string())
        .status("fallback".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = seed.id.clone()) {
            status?: Some("v2".to_string()),
        }
    };

    let _result = repo.upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("upsert update");

    // Verify version incremented
    let raw: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("get json");
    let json: Value = serde_json::from_str(&raw).expect("parse");
    assert_eq!(json[0]["metadata"]["version"], 2);
}

/// Test that upsert sets version to 1 on create path.
#[tokio::test]
async fn upsert_sets_version_on_create() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let repo = ns.unique_name_repo();

    let create_builder = UniqueNameEntity::validation_builder()
        .name(format!("version-create-{}", generate_entity_id()))
        .status("created".to_string());

    let update_builder = snugom::snug! {
        UniqueNameEntity(entity_id = "nonexistent".to_string()) {
            status?: Some("updated".to_string()),
        }
    };

    let result = repo.upsert(&mut conn, create_builder, update_builder)
        .await
        .expect("upsert create");

    let entity_id = match result {
        UpsertResult::Created(r) => r.id,
        UpsertResult::Updated(_) => panic!("expected Created"),
    };

    // Verify version is 1
    let key = repo.entity_key(&entity_id);
    let raw: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("get json");
    let json: Value = serde_json::from_str(&raw).expect("parse");
    assert_eq!(json[0]["metadata"]["version"], 1);
}
