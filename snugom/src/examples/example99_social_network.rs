use anyhow::Result;
use chrono::{Duration, Utc};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, UpsertResult, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct UserRecord {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    display_name: String,
    #[snugom(relation(many_to_many = "users"))]
    followers_ids: Vec<String>,
    #[snugom(relation(many_to_many = "users"))]
    following_ids: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct PostRecord {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    title: String,
    #[snugom(datetime(epoch_millis))]
    published_at: Option<chrono::DateTime<Utc>>,
    #[snugom(relation(target = "users"))]
    author_id: String,
    #[snugom(relation(many_to_many = "users"))]
    liked_by_ids: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct CommentRecord {
    #[snugom(id)]
    id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    body: String,
    #[snugom(relation(target = "users"))]
    author_id: String,
    #[snugom(relation)]
    post_id: String,
}

bundle! {
    service: "sn",
    entities: {
        UserRecord => "users",
        PostRecord => "posts",
        CommentRecord => "comments",
    }
}

/// Example 99 – social network tour combining nested creates, cascades, idempotency, and relations.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("social_network");
    let users: Repo<UserRecord> = Repo::new(prefix.clone());
    let posts: Repo<PostRecord> = Repo::new(prefix.clone());
    let comments: Repo<CommentRecord> = Repo::new(prefix);

    let created_at = Utc::now();
    // Create a user with a nested post and comment. The run! macro handles all nested wiring.
    let create_result = crate::run! {
        &users,
        &mut conn,
        create => UserRecord {
            created_at: created_at,
            display_name: "Macro User".to_string(),
            posts: [
                create PostRecord {
                    title: "Hello SnugOM".to_string(),
                    created_at: created_at,
                    published_at: None,
                    comments: [
                        create CommentRecord {
                            body: "First!".to_string(),
                            created_at: created_at + Duration::seconds(1),
                        }
                    ],
                }
            ],
        }
    }?;
    let user_id = create_result.id.clone();

    let post_ids: Vec<String> = conn.smembers(users.relation_key("posts", &user_id)).await?;
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    // Add another comment during an update to demonstrate nested create in update payload.
    crate::run! {
        &posts,
        &mut conn,
        update => PostRecord(entity_id = post_id.clone()) {
            comments: [
                create CommentRecord {
                    body: "Second!".to_string(),
                    created_at: created_at + Duration::seconds(2),
                }
            ],
        }
    }?;
    let comment_ids: Vec<String> = conn.smembers(posts.relation_key("comments", &post_id)).await?;
    assert_eq!(comment_ids.len(), 2, "nested create appended another comment");

    // Demonstrate idempotent create: the second call reuses the first mutation result.
    let idempotent = users
        .create_with_conn(
            &mut conn,
            UserRecord::validation_builder()
                .display_name("Idempotent".to_string())
                .created_at(created_at)
                .idempotency_key("user-create-1"),
        )
        .await?;
    let duplicate = users
        .create_with_conn(
            &mut conn,
            UserRecord::validation_builder()
                .display_name("Should Not Replace".to_string())
                .created_at(created_at + Duration::seconds(5))
                .idempotency_key("user-create-1"),
        )
        .await?;
    assert_eq!(idempotent.id, duplicate.id);

    // Upsert branch demonstration: update existing user; create a fresh one when missing.
    let update_branch = crate::run! {
        &users,
        &mut conn,
        upsert => UserRecord() {
            update: UserRecord(entity_id = user_id.clone()) {
                display_name: "Macro User Updated".to_string(),
            },
            create: UserRecord {
                created_at: created_at,
                display_name: "Should Not Create".to_string(),
            }
        }
    }?;
    assert!(matches!(update_branch, UpsertResult::Updated(_)));

    let create_branch = crate::run! {
        &users,
        &mut conn,
        upsert => UserRecord() {
            update: UserRecord(entity_id = "does-not-exist".to_string()) {
                display_name: "Unused".to_string(),
            },
            create: UserRecord {
                created_at: created_at,
                display_name: "Macro Fresh".to_string(),
            }
        }
    }?;
    let new_user_id = match create_branch {
        UpsertResult::Created(result) => result.id,
        _ => panic!("expected create branch"),
    };

    // Cascade delete: removing the first user tears down posts and comments.
    crate::run! {
        &users,
        &mut conn,
        delete => UserRecord(entity_id = user_id.clone())
    }?;
    assert!(
        !conn.exists(posts.entity_key(&post_id)).await?,
        "posts removed via cascade delete"
    );
    for comment_id in comment_ids {
        assert!(
            !conn.exists(comments.entity_key(&comment_id)).await?,
            "comments removed via cascade delete"
        );
    }

    // Clean up the extra user created during the upsert step.
    crate::run! {
        &users,
        &mut conn,
        delete => UserRecord(entity_id = new_user_id)
    }?;

    Ok(())
}
