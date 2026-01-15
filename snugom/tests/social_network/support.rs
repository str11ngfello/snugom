pub(crate) use chrono::{Duration, Utc};
pub(crate) use redis::{AsyncCommands, aio::ConnectionManager};
pub(crate) use serde_json::Value;
pub(crate) use snugom::{
    SnugomEntity,
    errors::RepoError,
    id::generate_entity_id,
    repository::{RelationPlan, Repo},
    runtime::RedisExecutor,
};
pub(crate) use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(SnugomEntity, serde::Serialize, serde::Deserialize)]
#[snugom(schema = 1, service = "sn", collection = "users")]
pub(crate) struct UserRecord {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    display_name: String,
    #[serde(default)]
    #[snugom(relation(many_to_many = "users", cascade = "detach"))]
    followers_ids: Vec<String>,
    #[serde(default)]
    #[snugom(relation(many_to_many = "users", cascade = "detach"))]
    following_ids: Vec<String>,
    /// has_many posts - for nested creation
    #[serde(default)]
    #[snugom(relation(target = "posts", cascade = "delete"))]
    posts: Vec<String>,
}

#[derive(SnugomEntity, serde::Serialize, serde::Deserialize)]
#[snugom(schema = 1, service = "sn", collection = "posts")]
pub(crate) struct PostRecord {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    title: String,
    #[snugom(datetime)]
    published_at: Option<chrono::DateTime<Utc>>,
    #[snugom(relation(target = "users", cascade = "delete"), filterable(tag))]
    author_id: String,
    #[serde(default)]
    #[snugom(relation(many_to_many = "users"))]
    liked_by_ids: Vec<String>,
    /// has_many comments - for nested creation
    #[serde(default)]
    #[snugom(relation(target = "comments", cascade = "delete"))]
    comments: Vec<String>,
}

#[derive(SnugomEntity, serde::Serialize, serde::Deserialize)]
#[snugom(schema = 1, service = "sn", collection = "comments")]
pub(crate) struct CommentRecord {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    body: String,
    #[snugom(relation(target = "users"), filterable(tag))]
    author_id: String,
    #[snugom(relation(target = "posts", cascade = "delete"), filterable(tag))]
    post_id: String,
}

pub(crate) static TEST_NAMESPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TestNamespace {
    prefix: String,
}

impl TestNamespace {
    pub(crate) fn unique() -> Self {
        let idx = TEST_NAMESPACE_COUNTER.fetch_add(1, Ordering::SeqCst);
        let salt = generate_entity_id();
        Self {
            prefix: format!("snug_test_prefix_{idx}_{}", &salt[..8]),
        }
    }

    pub(crate) fn user_repo(&self) -> Repo<UserRecord> {
        Repo::new(self.prefix.clone())
    }

    pub(crate) fn post_repo(&self) -> Repo<PostRecord> {
        Repo::new(self.prefix.clone())
    }

    pub(crate) fn comment_repo(&self) -> Repo<CommentRecord> {
        Repo::new(self.prefix.clone())
    }
}

pub(crate) async fn redis_conn() -> ConnectionManager {
    let client = redis::Client::open("redis://127.0.0.1/").expect("redis client");
    client.get_connection_manager().await.expect("connection manager")
}
