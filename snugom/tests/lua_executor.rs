use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::ConnectionManager};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use snugom::{
    SnugomEntity,
    errors::RepoError,
    repository::{RelationPlan, Repo},
    runtime::{
        RedisExecutor,
        commands::{MutationCommand, MutationPlan, build_entity_mutation},
    },
};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "tl", collection = "articles")]
struct ArticleRecord {
    #[snugom(id)]
    id: String,
    #[allow(dead_code)]
    #[snugom(datetime, filterable, sortable)]
    published_at: Option<DateTime<Utc>>,
    #[snugom(relation(many_to_many = "articles"))]
    articles_followers_ids: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "arr", collection = "items")]
struct ArrayRecord {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    tags: Vec<String>,
}

async fn redis_connection() -> ConnectionManager {
    let client = redis::Client::open("redis://127.0.0.1/").expect("redis client");
    client.get_connection_manager().await.expect("connection manager")
}

#[tokio::test]
async fn upsert_and_delete_entity() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("snug");
    let key = repo.entity_key("abc");
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap();

    let published_at = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    let builder = ArticleRecord::validation_builder()
        .id(String::from("abc"))
        .published_at(Some(published_at))
        .articles_followers_ids(Vec::new());

    {
        let mut executor = RedisExecutor::new(&mut conn);
        let result = repo.create(&mut executor, builder).await.expect("create plan");
        assert_eq!(result.id, "abc");
        assert_eq!(result.responses.len(), 1);
        assert_eq!(result.responses[0]["ok"], Value::Bool(true));
    }

    let stored_json: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch stored entity");
    let stored: Value = serde_json::from_str(&stored_json).expect("parse stored json");
    let arr = stored.as_array().expect("json array");
    assert_eq!(arr[0]["id"], Value::String(String::from("abc")));
    assert!(arr[0]["published_at_ts"].as_i64().is_some());
    assert_eq!(arr[0]["metadata"]["version"], Value::Number(1.into()));

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.delete(&mut executor, "abc", Some(1)).await.expect("delete plan");
    }

    let exists: bool = conn.exists(&key).await.expect("exists");
    assert!(!exists);
}

#[tokio::test]
async fn mutate_relation_set() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("mutate_builder");
    let rel_key = repo.relation_key("articles_followers_ids", "builder_articles");
    let _: () = redis::cmd("DEL").arg(&rel_key).query_async(&mut conn).await.unwrap();

    let mut executor = RedisExecutor::new(&mut conn);
    let create_result = repo
        .create(
            &mut executor,
            ArticleRecord::validation_builder()
                .id("builder_articles".to_string())
                .published_at(None)
                .articles_followers_ids(Vec::new()),
        )
        .await
        .expect("seed parent");
    drop(executor);
    assert_eq!(create_result.id, "builder_articles");

    {
        let relation = RelationPlan::with_left(
            "articles_followers_ids",
            "builder_articles",
            vec![String::from("one"), String::from("two")],
            Vec::new(),
        );
        let mut executor = RedisExecutor::new(&mut conn);
        repo.mutate_relations(&mut executor, vec![relation])
            .await
            .expect("relation add");
    }

    let members: Vec<String> = conn.smembers(&rel_key).await.expect("members");
    assert!(members.contains(&String::from("one")));
    assert!(members.contains(&String::from("two")));

    {
        let relation =
            RelationPlan::with_left("articles_followers_ids", "builder_articles", Vec::new(), vec![String::from("one")]);
        let mut executor = RedisExecutor::new(&mut conn);
        repo.mutate_relations(&mut executor, vec![relation])
            .await
            .expect("relation remove");
    }

    let members: Vec<String> = conn.smembers(&rel_key).await.expect("members");
    assert!(!members.contains(&String::from("one")));
}

#[tokio::test]
async fn macro_mutate_relation_set() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("mutate_macro");
    let rel_key = repo.relation_key("articles_followers_ids", "macro_articles");
    let _: () = redis::cmd("DEL").arg(&rel_key).query_async(&mut conn).await.unwrap();

    let builder = snugom::snug! {
        ArticleRecord {
            id: "macro_articles".to_string(),
            published_at: None,
            articles_followers_ids: Vec::new(),
        }
    };
    let mut executor = RedisExecutor::new(&mut conn);
    let create_result = repo.create(&mut executor, builder).await.expect("create base");
    drop(executor);
    assert_eq!(create_result.id, "macro_articles");

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.mutate_relations(
            &mut executor,
            vec![RelationPlan::with_left(
                "articles_followers_ids",
                "macro_articles",
                vec![String::from("one")],
                Vec::new(),
            )],
        )
        .await
        .expect("add follower");
    }

    let members: Vec<String> = conn.smembers(&rel_key).await.expect("members");
    assert!(members.contains(&String::from("one")));

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.mutate_relations(
            &mut executor,
            vec![RelationPlan::with_left(
                "articles_followers_ids",
                "macro_articles",
                Vec::new(),
                vec![String::from("one")],
            )],
        )
        .await
        .expect("remove follower");
    }

    let members: Vec<String> = conn.smembers(&rel_key).await.expect("members");
    assert!(!members.contains(&String::from("one")));
}

#[tokio::test]
async fn version_conflict_returns_error() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("snug");
    let key = repo.entity_key("conflict");
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap();

    let builder = ArticleRecord::validation_builder()
        .id(String::from("conflict"))
        .published_at(None)
        .articles_followers_ids(Vec::new());
    let payload = builder.build_payload().expect("payload");
    let mutation = build_entity_mutation(
        repo.descriptor(),
        key.clone(),
        payload.payload,
        payload.mirrors,
        Some(41),
        payload.idempotency_key,
        payload.idempotency_ttl,
        Vec::new(),
    )
    .expect("mutation");

    let mut plan = MutationPlan::new();
    plan.push(MutationCommand::UpsertEntity(mutation));
    let result = {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.execute(&mut executor, plan).await
    };
    assert!(result.is_err());
}

#[tokio::test]
async fn patch_missing_entity_returns_not_found() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("snug");
    let key = repo.entity_key("missing");
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap();

    let patch = snugom::snug! {
        ArticleRecord(entity_id = "missing".to_string()) {
            published_at: Some(Utc::now()),
        }
    };

    let mut executor = RedisExecutor::new(&mut conn);
    let err = repo.update_patch(&mut executor, patch).await.expect_err("should fail");
    assert!(matches!(err, RepoError::NotFound { .. }));
}

#[tokio::test]
async fn create_with_relation_connect() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArticleRecord> = Repo::new("snug");
    let key = repo.entity_key("rel");
    let relation_key = repo.relation_key("articles_followers_ids", "rel");
    let _: () = redis::cmd("DEL")
        .arg(&[&key, &relation_key])
        .query_async(&mut conn)
        .await
        .unwrap();

    let builder = ArticleRecord::validation_builder()
        .id(String::from("rel"))
        .published_at(None)
        .articles_followers_ids(Vec::new())
        .connect("articles_followers_ids", vec!["follower-1".to_string()]);

    {
        let mut executor = RedisExecutor::new(&mut conn);
        let result = repo.create(&mut executor, builder).await.expect("create with relation");
        assert_eq!(result.id, "rel");
    }

    let members: Vec<String> = conn.smembers(&relation_key).await.expect("members");
    assert!(members.contains(&"follower-1".to_string()));

    {
        let relation = RelationPlan::with_left("articles_followers_ids", "rel", Vec::new(), vec!["follower-1".to_string()]);
        let mut executor = RedisExecutor::new(&mut conn);
        repo.mutate_relations(&mut executor, vec![relation])
            .await
            .expect("disconnect follower");
    }

    let members: Vec<String> = conn.smembers(&relation_key).await.expect("members after remove");
    assert!(!members.contains(&"follower-1".to_string()));

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.delete(&mut executor, "rel", Some(1)).await.expect("cleanup");
    }
}

#[tokio::test]
async fn empty_arrays_survive_assign_and_merge() {
    let mut conn = redis_connection().await;
    let repo: Repo<ArrayRecord> = Repo::new("arrtest");
    let key = repo.entity_key("item-1");
    let _: () = redis::cmd("DEL").arg(&key).query_async(&mut conn).await.unwrap();

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.create(
            &mut executor,
            snugom::snug! {
                ArrayRecord {
                    id: "item-1".to_string(),
                    tags: Vec::<String>::new(),
                }
            },
        )
        .await
        .expect("create array record");
    }

    let stored_raw: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch stored entity");
    let stored: Value = serde_json::from_str(&stored_raw).expect("parse stored json");
    let tags = stored
        .as_array()
        .and_then(|a| a.get(0))
        .and_then(|o| o.get("tags"))
        .and_then(|t| t.as_array())
        .expect("tags array after create");
    assert!(tags.is_empty(), "tags should remain empty array on create");

    {
        let mut executor = RedisExecutor::new(&mut conn);
        repo.update_patch(
            &mut executor,
            snugom::snug! {
                ArrayRecord(entity_id = "item-1".to_string()) {
                    tags: Vec::<String>::new(),
                }
            },
        )
        .await
        .expect("assign empty array");
    }

    let patched_raw: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch patched entity");
    let patched: Value = serde_json::from_str(&patched_raw).expect("parse patched json");
    let tags_after = patched
        .as_array()
        .and_then(|a| a.get(0))
        .and_then(|o| o.get("tags"))
        .and_then(|t| t.as_array())
        .expect("tags array after patch");
    assert!(tags_after.is_empty(), "tags should remain empty array after assign patch");
}
