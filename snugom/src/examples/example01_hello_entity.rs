use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct HelloEntity {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(validate(length(min = 1)))]
    name: String,
}

bundle! {
    service: "examples",
    entities: { HelloEntity => "hello" }
}

/// Example 01 – basic CRUD with builders and repos (no `run!` macro yet).
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("hello");
    let repo: Repo<HelloEntity> = Repo::new(prefix);

    // Start with a clean namespace.
    let initial_count = repo.count(&mut conn).await?;
    assert_eq!(initial_count, 0, "namespace should be empty");

    let builder = HelloEntity::validation_builder()
        .name("Getting Started".to_string())
        .created_at(Utc::now());
    let created = repo.create_with_conn(&mut conn, builder).await?;
    let entity_id = created.id.clone();

    assert!(repo.exists(&mut conn, &entity_id).await?, "entity exists after insert");
    assert_eq!(repo.count(&mut conn).await?, 1, "count reflects single insert");

    repo.delete_with_conn(&mut conn, &entity_id, None).await?;
    assert!(!repo.exists(&mut conn, &entity_id).await?, "entity removed after delete");
    assert_eq!(repo.count(&mut conn).await?, 0, "count returns to zero");

    // Ensure Redis keys are cleaned up to avoid contaminating other examples.
    let key = repo.entity_key(&entity_id);
    let _: () = conn.del(key).await?;
    Ok(())
}
