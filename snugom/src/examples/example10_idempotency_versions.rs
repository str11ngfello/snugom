use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::RepoError;
use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct VersionedRecord {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    name: String,
    #[snugom(validate(range(min = 0)))]
    revision: i64,
}

bundle! {
    service: "examples",
    entities: { VersionedRecord => "versioned_records" }
}

/// Example 10 – idempotency keys and optimistic version checks.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("idempotency");
    let repo: Repo<VersionedRecord> = Repo::new(prefix);

    let created = repo
        .create_with_conn(
            &mut conn,
            VersionedRecord::validation_builder()
                .name("Idempotent".to_string())
                .created_at(Utc::now())
                .revision(1)
                .idempotency_key("record-create-1"),
        )
        .await?;
    let record_id = created.id.clone();

    // Reusing the same idempotency key should return the original mutation result.
    let duplicate = repo
        .create_with_conn(
            &mut conn,
            VersionedRecord::validation_builder()
                .name("Updated".to_string())
                .created_at(Utc::now() + chrono::Duration::seconds(5))
                .revision(99)
                .idempotency_key("record-create-1"),
        )
        .await?;
    assert_eq!(duplicate.id, record_id, "idempotency key reused prior result");

    let stored = repo.get(&mut conn, &record_id).await?.expect("record should exist");
    assert_eq!(stored.name, "Idempotent");
    assert_eq!(stored.revision, 1);

    // Version-aware patch update.
    let patch = crate::snug! {
        VersionedRecord(entity_id = record_id.clone(), expected_version = 1) {
            revision: 2,
        }
    };
    let response = repo.update_patch_with_conn(&mut conn, patch).await?;
    let new_version = response
        .last()
        .and_then(|value| value.get("version"))
        .and_then(|value| value.as_u64())
        .expect("returned version");

    // Retry with stale version should yield a conflict.
    let stale_patch = crate::snug! {
        VersionedRecord(entity_id = record_id.clone(), expected_version = 1) {
            revision: 3,
        }
    };
    let err = repo
        .update_patch_with_conn(&mut conn, stale_patch)
        .await
        .expect_err("stale version should conflict");
    match err {
        RepoError::VersionConflict { expected, actual } => {
            assert_eq!(expected, Some(1));
            assert_eq!(actual, Some(new_version));
        }
        other => panic!("expected version conflict, got {other:?}"),
    }

    Ok(())
}
