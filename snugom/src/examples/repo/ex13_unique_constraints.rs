use anyhow::Result;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::errors::RepoError;
use super::support;
use crate::repository::Repo;
use crate::SnugomEntity;

/// An entity with a unique field - like a guild with a unique name.
#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "unique_name")]
struct UniqueNameEntity {
    #[snugom(id)]
    id: String,
    /// The name must be unique across all entities.
    #[snugom(unique, filterable(tag))]
    name: String,
    description: Option<String>,
}

/// An entity with case-insensitive unique constraint.
#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "case_insensitive")]
struct CaseInsensitiveEntity {
    #[snugom(id)]
    id: String,
    /// The slug must be unique (case-insensitive).
    #[snugom(unique(case_insensitive), filterable(tag))]
    slug: String,
}

/// An entity with a compound unique constraint.
#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "tenant_scoped")]
#[snugom(unique_together = ["tenant_id", "name"])]
struct TenantScopedEntity {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    tenant_id: String,
    #[snugom(filterable(tag))]
    name: String,
}

/// Example 15 - Unique field constraints (SQL-like UNIQUE).
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("unique_constraints");

    // Test 1: Single-field unique constraint
    println!("Test 1: Single-field unique constraint");
    let repo: Repo<UniqueNameEntity> = Repo::new(prefix.clone());

    // Create first entity with unique name
    let first = UniqueNameEntity::validation_builder()
        .name("Avengers".to_string())
        .description(Some("Earth's mightiest heroes".to_string()));
    let first_created = repo.create_with_conn(&mut conn, first).await?;
    let first_id = first_created.id.clone();
    println!("  Created first entity: {}", first_id);

    // Try to create second entity with same name - should fail
    let second = UniqueNameEntity::validation_builder()
        .name("Avengers".to_string())
        .description(Some("Another team".to_string()));
    let result = repo.create_with_conn(&mut conn, second).await;
    match result {
        Err(RepoError::UniqueConstraintViolation {
            fields,
            values,
            existing_entity_id,
        }) => {
            assert_eq!(fields, vec!["name"]);
            assert_eq!(values, vec!["Avengers"]);
            assert_eq!(existing_entity_id, first_id);
            println!("  Correctly rejected duplicate: fields={:?}, values={:?}", fields, values);
        }
        Ok(_) => panic!("Should have rejected duplicate name"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    // Create with different name - should succeed
    let third = UniqueNameEntity::validation_builder()
        .name("Justice League".to_string())
        .description(Some("DC's finest".to_string()));
    let third_created = repo.create_with_conn(&mut conn, third).await?;
    let third_id = third_created.id.clone();
    println!("  Created third entity with unique name: {}", third_id);

    // Clean up
    repo.delete_with_conn(&mut conn, &first_id, None).await?;
    repo.delete_with_conn(&mut conn, &third_id, None).await?;

    // After delete, we should be able to reuse the name
    let reused = UniqueNameEntity::validation_builder()
        .name("Avengers".to_string())
        .description(Some("Reformed team".to_string()));
    let reused_created = repo.create_with_conn(&mut conn, reused).await?;
    let reused_id = reused_created.id.clone();
    println!("  Created entity with reused name after delete: {}", reused_id);
    repo.delete_with_conn(&mut conn, &reused_id, None).await?;

    // Test 2: Case-insensitive unique constraint
    println!("\nTest 2: Case-insensitive unique constraint");
    let ci_repo: Repo<CaseInsensitiveEntity> = Repo::new(prefix.clone());

    let ci_first = CaseInsensitiveEntity::validation_builder().slug("hello-world".to_string());
    let ci_created = ci_repo.create_with_conn(&mut conn, ci_first).await?;
    let ci_id = ci_created.id.clone();
    println!("  Created entity with slug: hello-world (id: {})", ci_id);

    // Same slug with different case - should fail
    let ci_second = CaseInsensitiveEntity::validation_builder().slug("HELLO-WORLD".to_string());
    let ci_result = ci_repo.create_with_conn(&mut conn, ci_second).await;
    match ci_result {
        Err(RepoError::UniqueConstraintViolation { fields, .. }) => {
            assert_eq!(fields, vec!["slug"]);
            println!("  Correctly rejected case-insensitive duplicate");
        }
        Ok(_) => panic!("Should have rejected case-insensitive duplicate"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    ci_repo.delete_with_conn(&mut conn, &ci_id, None).await?;

    // Test 3: Compound unique constraint
    println!("\nTest 3: Compound unique constraint (tenant_id + name)");
    let ts_repo: Repo<TenantScopedEntity> = Repo::new(prefix.clone());

    // Create entity for tenant A
    let ts_a1 = TenantScopedEntity::validation_builder()
        .tenant_id("acme".to_string())
        .name("Alpha".to_string());
    let ts_a1_created = ts_repo.create_with_conn(&mut conn, ts_a1).await?;
    let ts_a1_id = ts_a1_created.id.clone();
    println!("  Created entity: tenant=acme, name=Alpha (id: {})", ts_a1_id);

    // Same name for different tenant - should succeed
    let ts_b1 = TenantScopedEntity::validation_builder()
        .tenant_id("globex".to_string())
        .name("Alpha".to_string());
    let ts_b1_created = ts_repo.create_with_conn(&mut conn, ts_b1).await?;
    let ts_b1_id = ts_b1_created.id.clone();
    println!("  Created entity: tenant=globex, name=Alpha (different tenant, same name OK) (id: {})", ts_b1_id);

    // Same name for same tenant - should fail
    let ts_a2 = TenantScopedEntity::validation_builder()
        .tenant_id("acme".to_string())
        .name("Alpha".to_string());
    let ts_result = ts_repo.create_with_conn(&mut conn, ts_a2).await;
    match ts_result {
        Err(RepoError::UniqueConstraintViolation { fields, .. }) => {
            assert_eq!(fields, vec!["tenant_id", "name"]);
            println!("  Correctly rejected duplicate (tenant_id, name) combination");
        }
        Ok(_) => panic!("Should have rejected duplicate compound key"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }

    // Different name for same tenant - should succeed
    let ts_a3 = TenantScopedEntity::validation_builder()
        .tenant_id("acme".to_string())
        .name("Beta".to_string());
    let ts_a3_created = ts_repo.create_with_conn(&mut conn, ts_a3).await?;
    let ts_a3_id = ts_a3_created.id.clone();
    println!("  Created entity: tenant=acme, name=Beta (same tenant, different name OK) (id: {})", ts_a3_id);

    // Clean up
    ts_repo.delete_with_conn(&mut conn, &ts_a1_id, None).await?;
    ts_repo.delete_with_conn(&mut conn, &ts_b1_id, None).await?;
    ts_repo.delete_with_conn(&mut conn, &ts_a3_id, None).await?;

    // Clean up unique index keys
    let unique_key = format!("{}:examples:unique_name:unique:name", prefix);
    let _: () = conn.del(&unique_key).await?;
    let ci_unique_key = format!("{}:examples:case_insensitive:unique:slug", prefix);
    let _: () = conn.del(&ci_unique_key).await?;
    let ts_unique_key = format!("{}:examples:tenant_scoped:unique_compound:tenant_id_name", prefix);
    let _: () = conn.del(&ts_unique_key).await?;

    println!("\nAll unique constraint tests passed!");
    Ok(())
}
