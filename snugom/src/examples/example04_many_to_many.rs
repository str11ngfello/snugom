use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct User {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
    handle: String,
    #[snugom(relation(many_to_many = "topics"))]
    topics_ids: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct Topic {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    name: String,
    #[snugom(relation(many_to_many = "users"))]
    users_ids: Vec<String>,
}

bundle! {
    service: "examples",
    entities: {
        User => "users",
        Topic => "topics",
    }
}

/// Example 04 – many-to-many connect/disconnect using `snug!` patch directives.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("many_to_many");
    let user_repo: Repo<User> = Repo::new(prefix.clone());
    let topic_repo: Repo<Topic> = Repo::new(prefix);

    let user = user_repo
        .create_with_conn(
            &mut conn,
            User::validation_builder()
                .handle("rustacean".to_string())
                .joined_at(Utc::now())
                .topics_ids(Vec::new()),
        )
        .await?;
    let user_id = user.id.clone();

    let topic = topic_repo
        .create_with_conn(
            &mut conn,
            Topic::validation_builder()
                .name("redis".to_string())
                .created_at(Utc::now())
                .users_ids(Vec::new()),
        )
        .await?;
    let topic_id = topic.id.clone();

    // Connect the topic to the user via a many-to-many relation.
    let connect_patch = crate::snug! {
        User(entity_id = user_id.clone(), expected_version = 1) {
            // Attach `topic_id` to the user's `topics_ids` relation.
            topics_ids: [connect topic_id.clone()],
        }
    };
    let connect_response = user_repo.update_patch_with_conn(&mut conn, connect_patch).await?;
    let version_after_connect = connect_response
        .last()
        .and_then(|value| value.get("version"))
        .and_then(|value| value.as_u64())
        .expect("version present after connect");

    // Disconnect the same topic, demonstrating the detach direction.
    let disconnect_patch = crate::snug! {
        User(entity_id = user_id.clone(), expected_version = version_after_connect) {
            // Remove `topic_id` from the user's `topics_ids` relation.
            topics_ids: [disconnect topic_id.clone()],
        }
    };
    user_repo.update_patch_with_conn(&mut conn, disconnect_patch).await?;

    Ok(())
}
