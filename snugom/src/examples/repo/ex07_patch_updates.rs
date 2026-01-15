use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomEntity, errors::RepoError, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "patch_entities")]
struct PatchEntity {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(validate(length(min = 1)))]
    name: String,
}

/// Example 07 – partial updates with validation and immutable fields.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("patch_updates");
    let repo: Repo<PatchEntity> = Repo::new(prefix);

    let entity = repo
        .create_with_conn(
            &mut conn,
            PatchEntity::validation_builder()
                .name("Initial".to_string())
                .created_at(Utc::now()),
        )
        .await?;
    let entity_id = entity.id.clone();

    // Use the `snug!` macro to build a partial update; only the `name` field changes.
    let patch = crate::snug! {
        PatchEntity(entity_id = entity_id.clone()) {
            name: "Updated".to_string(),
        }
    };
    repo.update_patch_with_conn(&mut conn, patch).await?;

    let updated = repo.get(&mut conn, &entity_id).await?.expect("entity should exist after patch");
    assert_eq!(updated.name, "Updated");

    // Validation failures are surfaced as `RepoError::Validation`.
    let invalid_patch = crate::snug! {
        PatchEntity(entity_id = entity_id.clone()) {
            name: "".to_string(),
        }
    };
    let err = repo
        .update_patch_with_conn(&mut conn, invalid_patch)
        .await
        .expect_err("empty name should fail validation");
    match err {
        RepoError::Validation(_) => {}
        other => panic!("expected validation error, got {other:?}"),
    }

    Ok(())
}
