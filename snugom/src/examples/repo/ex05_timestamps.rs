use anyhow::Result;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::support;
use crate::{SnugomEntity, repository::Repo};

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1, service = "examples", collection = "managed_entities")]
struct ManagedEntity {
    #[snugom(id)]
    record_id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    updated_at: chrono::DateTime<Utc>,
    #[snugom(validate(length(min = 1)))]
    name: String,
}

/// Example 05 – managed timestamps and epoch mirrors.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("timestamps");
    let repo: Repo<ManagedEntity> = Repo::new(prefix);

    // Auto-managed `updated_at` when omitted on create.
    let created = repo
        .create_with_conn(
            &mut conn,
            crate::snug! {
                ManagedEntity {
                    name: "Alpha".to_string(),
                    created_at: Utc::now(),
                }
            },
        )
        .await?;
    let stored = repo
        .get(&mut conn, &created.id)
        .await?
        .expect("entity should exist after create");

    let now = Utc::now();
    assert!(
        stored.updated_at <= now && now - stored.updated_at < Duration::seconds(5),
        "updated_at should be auto-populated with a recent timestamp"
    );

    // Mirror field (`updated_at_ts`) is maintained alongside the ISO string.
    let mirror_raw: String = redis::cmd("JSON.GET")
        .arg(repo.entity_key(&created.id))
        .arg("$.updated_at_ts")
        .query_async(&mut conn)
        .await?;
    let mirror_value: JsonValue = serde_json::from_str(&mirror_raw)?;
    let millis_opt = mirror_value
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|value| value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok())));
    assert_eq!(
        millis_opt.expect("mirror timestamp present"),
        stored.updated_at.timestamp_millis()
    );

    // Supplying an explicit timestamp is respected.
    let explicit = stored.updated_at - Duration::days(7);
    let explicit_entity = repo
        .create_with_conn(
            &mut conn,
            crate::snug! {
                ManagedEntity {
                    name: "Beta".to_string(),
                    created_at: Utc::now(),
                    updated_at: explicit,
                }
            },
        )
        .await?;
    let explicit_stored = repo.get(&mut conn, &explicit_entity.id).await?.expect("explicit entity");
    assert_eq!(explicit_stored.updated_at, explicit);

    // Patch without `updated_at` triggers an automatic refresh.
    let before = repo.get(&mut conn, &created.id).await?.expect("entity before patch").updated_at;
    // Use `snug!` to build a partial update; leaving out `updated_at` lets SnugOM refresh it.
    let patch = crate::snug! {
        ManagedEntity(entity_id = created.id.clone()) {
            name: "Alpha Updated".to_string(),
        }
    };
    repo.update_patch_with_conn(&mut conn, patch).await?;
    let after = repo.get(&mut conn, &created.id).await?.expect("entity after patch").updated_at;
    assert!(after >= before, "updated_at should refresh on patch");

    // Explicit override during update is preserved.
    let custom = after - Duration::hours(6);
    let override_patch = crate::snug! {
        ManagedEntity(entity_id = created.id.clone()) {
            updated_at: custom,
            name: "Alpha Custom".to_string(),
        }
    };
    repo.update_patch_with_conn(&mut conn, override_patch).await?;
    let final_state = repo.get(&mut conn, &created.id).await?.expect("entity after override");
    assert_eq!(final_state.updated_at, custom);

    Ok(())
}
