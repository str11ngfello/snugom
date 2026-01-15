use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use snugom::{
    SnugomEntity,
    repository::Repo,
    runtime::RedisExecutor,
    types::{EntityMetadata, RelationKind, ValidationDescriptor, ValidationRule, ValidationScope},
};
use tokio::runtime::Runtime;

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "tl", collection = "org")]
struct Org {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    name: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 2, service = "tl", collection = "users")]
struct UserDescriptor {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "org", alias = "organization"), filterable(tag))]
    organization_id: String,
}

#[test]
fn descriptor_contains_relationships() {
    let descriptor = UserDescriptor::entity_descriptor();
    assert_eq!(descriptor.service, "tl");
    assert_eq!(descriptor.collection, "users");
    assert_eq!(descriptor.version, 2);
    assert_eq!(descriptor.id_field.as_deref(), Some("id"));
    assert_eq!(descriptor.relations.len(), 1);
    assert_eq!(descriptor.fields.len(), 2);

    let org = &descriptor.relations[0];
    assert_eq!(org.alias, "organization");
    assert_eq!(org.target, "org");
    assert!(matches!(org.kind, RelationKind::BelongsTo));
    assert_eq!(org.foreign_key, Some("organization_id".to_string()));

    let id_field = &descriptor.fields[0];
    assert_eq!(id_field.name, "id");
    assert!(!id_field.optional);
    assert!(id_field.validations.is_empty());
    assert!(id_field.is_id);
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "tl", collection = "articles")]
struct Article {
    #[snugom(id)]
    id: String,
    #[snugom(validate(length(min = 3, max = 12)), filterable(tag))]
    title: String,
    #[snugom(validate(range(min = 0, max = 10)), filterable, sortable)]
    rating: i32,
    #[snugom(validate(regex = "^[a-z-]+$"))]
    slug: String,
    #[snugom(validate(length(min = 1)))]
    tags: Vec<String>,
    #[snugom(validate(length(min = 3)))]
    summary: Option<String>,
}

#[test]
fn validation_reports_issues() {
    let descriptor = Article::entity_descriptor();
    assert_eq!(descriptor.id_field.as_deref(), Some("id"));
    assert_eq!(descriptor.fields.len(), 6);
    let title_field = descriptor
        .fields
        .iter()
        .find(|field| field.name == "title")
        .expect("title field present");
    assert!(matches!(
        title_field.validations[0],
        ValidationDescriptor {
            scope: ValidationScope::Field,
            rule: ValidationRule::Length { .. }
        }
    ));

    let valid = Article {
        id: String::from("article-1"),
        title: String::from("Welcome"),
        rating: 5,
        slug: String::from("welcome-post"),
        tags: vec![String::from("snugom")],
        summary: Some(String::from("Great")),
    };
    assert!(valid.validate().is_ok());

    let invalid = Article {
        id: String::from("article-2"),
        title: String::from("hi"),
        rating: 42,
        slug: String::from("Not Valid"),
        tags: Vec::new(),
        summary: Some(String::from("no")),
    };
    let err = invalid.validate().expect_err("expected validation failure");
    assert!(err.issues.iter().any(|issue| issue.field == "title"));
    assert!(err.issues.iter().any(|issue| issue.field == "rating"));
    assert!(err.issues.iter().any(|issue| issue.field == "slug"));
    assert!(err.issues.iter().any(|issue| issue.field == "tags"));
    assert!(err.issues.iter().any(|issue| issue.field == "summary"));
}

async fn redis_conn() -> ConnectionManager {
    let client = redis::Client::open("redis://127.0.0.1/").expect("redis client");
    client.get_connection_manager().await.expect("connection manager")
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "cycle", collection = "alpha")]
struct AlphaEntity {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "beta", cascade = "delete"), filterable(tag))]
    beta_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "cycle", collection = "beta")]
struct BetaEntity {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "alpha", cascade = "delete"), filterable(tag))]
    alpha_id: String,
}

#[test]
fn cascade_cycle_is_rejected() {
    let rt = Runtime::new().expect("runtime");
    rt.block_on(async {
        let mut conn = redis_conn().await;
        let mut executor = RedisExecutor::new(&mut conn);
        let repo: Repo<AlphaEntity> = Repo::new("cycle");
        let _ = Repo::<BetaEntity>::new("cycle");
        let err = repo
            .delete(&mut executor, "does-not-matter", None)
            .await
            .expect_err("expected cascade cycle error");
        match err {
            snugom::RepoError::Other { message } => {
                assert!(message.contains("cycle detected"), "Got message: {}", message);
            }
            other => panic!("expected cycle error, got {other:?}"),
        }
    });
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node0")]
struct DepthNode0 {
    #[snugom(id)]
    id: String,
    #[snugom(filterable(tag))]
    name: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node1")]
struct DepthNode1 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node0", cascade = "delete"), filterable(tag))]
    node0_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node2")]
struct DepthNode2 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node1", cascade = "delete"), filterable(tag))]
    node1_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node3")]
struct DepthNode3 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node2", cascade = "delete"), filterable(tag))]
    node2_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node4")]
struct DepthNode4 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node3", cascade = "delete"), filterable(tag))]
    node3_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node5")]
struct DepthNode5 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node4", cascade = "delete"), filterable(tag))]
    node4_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node6")]
struct DepthNode6 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node5", cascade = "delete"), filterable(tag))]
    node5_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node7")]
struct DepthNode7 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node6", cascade = "delete"), filterable(tag))]
    node6_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node8")]
struct DepthNode8 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node7", cascade = "delete"), filterable(tag))]
    node7_id: String,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "depth", collection = "node9")]
struct DepthNode9 {
    #[snugom(id)]
    id: String,
    #[snugom(relation(target = "node8", cascade = "delete"), filterable(tag))]
    node8_id: String,
}

#[test]
fn cascade_depth_limit_is_enforced() {
    // This test verifies that cascade delete has a depth limit (MAX_CASCADE_DEPTH = 8)
    // The entities form a chain: Node0 <- Node1 <- ... <- Node9 (10 levels)
    // When deleting Node0, the cascade should fail because it exceeds 8 levels
    let rt = Runtime::new().expect("runtime");
    rt.block_on(async {
        let mut conn = redis_conn().await;
        let mut executor = RedisExecutor::new(&mut conn);

        // Register all repos to populate the registry with descriptors
        let _ = Repo::<DepthNode0>::new("depth");
        let _ = Repo::<DepthNode1>::new("depth");
        let _ = Repo::<DepthNode2>::new("depth");
        let _ = Repo::<DepthNode3>::new("depth");
        let _ = Repo::<DepthNode4>::new("depth");
        let _ = Repo::<DepthNode5>::new("depth");
        let _ = Repo::<DepthNode6>::new("depth");
        let _ = Repo::<DepthNode7>::new("depth");
        let _ = Repo::<DepthNode8>::new("depth");
        let _ = Repo::<DepthNode9>::new("depth");

        // Delete from the root (Node0) - this should trigger cascade through all child nodes
        // via the incoming belongs_to relations (Node1->Node0, Node2->Node1, etc.)
        let repo: Repo<DepthNode0> = Repo::new("depth");
        let err = repo
            .delete(&mut executor, "does-not-matter", None)
            .await
            .expect_err("expected cascade depth error");
        match err {
            snugom::RepoError::Other { message } => {
                assert!(message.contains("cascade depth exceeded"), "Got message: {}", message);
            }
            other => panic!("expected depth error, got {other:?}"),
        }
    });
}
