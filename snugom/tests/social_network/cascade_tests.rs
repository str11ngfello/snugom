use super::support::*;

#[tokio::test]
async fn delete_user_cascades_posts() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();
    let comments: Repo<CommentRecord> = ns.comment_repo();

    let now = Utc::now();
    let user_builder = UserRecord::validation_builder()
        .display_name(String::from("Cascade"))
        .created_at(now);
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users.create(&mut executor, user_builder).await.expect("create user");
    let user_id = user.id;

    let post_builder = PostRecord::validation_builder()
        .title(String::from("Cascade Post"))
        .created_at(now)
        .relation("author", vec![user_id.clone()], Vec::new());
    let post = posts.create(&mut executor, post_builder).await.expect("create post");
    let post_id = post.id;

    let comment = comments
        .create(
            &mut executor,
            CommentRecord::validation_builder()
                .body(String::from("First"))
                .created_at(now + Duration::seconds(1)),
        )
        .await
        .expect("create comment");
    let comment_id = comment.id.clone();

    let relation = RelationPlan::with_left("comments", post_id.clone(), vec![comment_id.clone()], Vec::new());
    posts
        .mutate_relations(&mut executor, vec![relation])
        .await
        .expect("attach comment");

    let relation = RelationPlan::with_left("posts", user_id.clone(), vec![post_id.clone()], Vec::new());
    users
        .mutate_relations(&mut executor, vec![relation])
        .await
        .expect("link post relation");

    users.delete(&mut executor, &user_id, None).await.expect("delete user");

    let user_exists: bool = conn.exists(users.entity_key(&user_id)).await.expect("user exists");
    assert!(!user_exists, "user key should be deleted");

    let post_exists: bool = conn.exists(posts.entity_key(&post_id)).await.expect("post exists");
    assert!(!post_exists, "post key should be deleted by cascade");

    let comment_exists: bool = conn.exists(comments.entity_key(&comment_id)).await.expect("comment exists");
    assert!(!comment_exists, "comment key should be deleted by cascade");

    let relation_exists: bool = conn
        .exists(users.relation_key("posts", &user_id))
        .await
        .expect("relation exists");
    assert!(!relation_exists, "relation set should be cleared");
}

#[tokio::test]
async fn macro_delete_user_cascades_posts() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();
    let comments: Repo<CommentRecord> = ns.comment_repo();

    let created_at = Utc::now();
    let user_builder = snugom::snug! {
        UserRecord {
            display_name: "Cascade Macro".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Cascade Macro Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    };
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users.create(&mut executor, user_builder).await.expect("create user via macro");

    let posts_relation = users.relation_key("posts", &user.id);
    drop(executor);
    let post_ids: Vec<String> = conn.smembers(&posts_relation).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let mut executor = RedisExecutor::new(&mut conn);
    let comment = comments
        .create(
            &mut executor,
            snugom::snug! {
                CommentRecord {
                    body: "Macro Comment".to_string(),
                    created_at: created_at + Duration::seconds(1),
                }
            },
        )
        .await
        .expect("create comment");
    let comment_id = comment.id.clone();

    posts
        .mutate_relations(
            &mut executor,
            vec![RelationPlan::with_left(
                "comments",
                post_id.clone(),
                vec![comment_id.clone()],
                Vec::new(),
            )],
        )
        .await
        .expect("attach comment");

    let mut executor = RedisExecutor::new(&mut conn);
    users.delete(&mut executor, &user.id, None).await.expect("delete user");

    drop(executor);

    let user_exists: bool = conn.exists(users.entity_key(&user.id)).await.expect("user exists");
    assert!(!user_exists);

    let post_exists: bool = conn.exists(posts.entity_key(&post_id)).await.expect("post exists");
    assert!(!post_exists);

    let comment_exists: bool = conn.exists(comments.entity_key(&comment_id)).await.expect("comment exists");
    assert!(!comment_exists);
}

#[tokio::test]
async fn builder_update_creates_nested_post() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            UserRecord::validation_builder()
                .display_name("Nested Builder".to_string())
                .created_at(created_at),
        )
        .await
        .expect("create user");
    let user_id = created.id.clone();
    let initial_version = created.responses[0]["version"].as_u64().expect("version");

    drop(executor);

    let mut executor = RedisExecutor::new(&mut conn);
    let nested_patch = snugom::snug! {
        UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
            posts: [
                create PostRecord {
                    title: "Created During Update".to_string(),
                    created_at: created_at + Duration::seconds(1),
                    author: [connect user_id.clone()],
                }
            ],
        }
    };
    users
        .update_patch(&mut executor, nested_patch)
        .await
        .expect("update with nested create");

    drop(executor);

    let relation_key = users.relation_key("posts", &user_id);
    let post_ids: Vec<String> = conn.smembers(&relation_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let post_key = posts.entity_key(&post_id);
    let post_json: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("post json");
    let post_value: Value = serde_json::from_str(&post_json).expect("parse post");
    let post_array = post_value.as_array().expect("json array");
    assert_eq!(post_array[0]["title"], Value::String(String::from("Created During Update")));
    assert_eq!(post_array[0]["metadata"]["version"], Value::Number(1.into()));

    let author_key = posts.relation_key("author", &post_id);
    let authors: Vec<String> = conn.smembers(&author_key).await.expect("author relation");
    assert!(authors.contains(&user_id));
}

#[tokio::test]
async fn macro_update_creates_nested_post() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Nested Macro".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user via macro");
    let user_id = created.id.clone();
    let initial_version = created.responses[0]["version"].as_u64().expect("version");

    drop(executor);

    let mut executor = RedisExecutor::new(&mut conn);
    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    posts: [
                        create PostRecord {
                            title: "Macro Create During Update".to_string(),
                            created_at: created_at + Duration::seconds(1),
                            author: [connect user_id.clone()],
                        }
                    ],
                }
            },
        )
        .await
        .expect("macro update with nested create");

    drop(executor);

    let relation_key = users.relation_key("posts", &user_id);
    let post_ids: Vec<String> = conn.smembers(&relation_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let post_key = posts.entity_key(&post_id);
    let post_json: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("post json");
    let post_value: Value = serde_json::from_str(&post_json).expect("parse post");
    let post_array = post_value.as_array().expect("post array");
    assert_eq!(
        post_array[0]["title"],
        Value::String(String::from("Macro Create During Update"))
    );
    assert_eq!(post_array[0]["metadata"]["version"], Value::Number(1.into()));

    let author_key = posts.relation_key("author", &post_id);
    let authors: Vec<String> = conn.smembers(&author_key).await.expect("author relation");
    assert!(authors.contains(&user_id));
}

#[tokio::test]
async fn builder_update_delete_cascades_post() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();
    let comments: Repo<CommentRecord> = ns.comment_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(
            &mut executor,
            UserRecord::validation_builder()
                .display_name("Cascade Builder".to_string())
                .created_at(created_at)
                .create_relation(
                    "posts",
                    PostRecord::validation_builder()
                        .title("Delete Me".to_string())
                        .created_at(created_at),
                ),
        )
        .await
        .expect("create user with post");
    let user_id = user.id.clone();
    let initial_version = user.responses[0]["version"].as_u64().expect("version");
    drop(executor);

    let relation_key = users.relation_key("posts", &user_id);
    let post_ids: Vec<String> = conn.smembers(&relation_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let mut executor = RedisExecutor::new(&mut conn);
    let comment = comments
        .create(
            &mut executor,
            CommentRecord::validation_builder()
                .body("Cascade Comment".to_string())
                .created_at(created_at + Duration::seconds(2)),
        )
        .await
        .expect("create comment");
    let comment_id = comment.id.clone();

    posts
        .mutate_relations(
            &mut executor,
            vec![RelationPlan::with_left(
                "comments",
                post_id.clone(),
                vec![comment_id.clone()],
                Vec::new(),
            )],
        )
        .await
        .expect("attach comment");

    let mut executor = RedisExecutor::new(&mut conn);
    let delete_patch = snugom::snug! {
        UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
            posts: [delete post_id.clone()],
        }
    };
    users
        .update_patch(&mut executor, delete_patch)
        .await
        .expect("update with cascade delete");

    drop(executor);

    let post_exists: bool = conn.exists(posts.entity_key(&post_id)).await.expect("post exists");
    assert!(!post_exists, "post should be removed by cascade");

    let remaining_posts: Vec<String> = conn.smembers(&relation_key).await.expect("post relation");
    assert!(remaining_posts.is_empty(), "relation set should be cleared");

    let comment_exists: bool = conn.exists(comments.entity_key(&comment_id)).await.expect("comment exists");
    assert!(!comment_exists, "comment should be removed by cascade");
}

#[tokio::test]
async fn macro_update_delete_cascades_post() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();
    let comments: Repo<CommentRecord> = ns.comment_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Cascade Macro".to_string(),
                    created_at: created_at,
                    posts: [
                        create PostRecord {
                            title: "Delete Macro Post".to_string(),
                            created_at: created_at,
                        }
                    ],
                }
            },
        )
        .await
        .expect("create user via macro");
    let user_id = user.id.clone();
    let initial_version = user.responses[0]["version"].as_u64().expect("version");
    drop(executor);

    let relation_key = users.relation_key("posts", &user_id);
    let post_ids: Vec<String> = conn.smembers(&relation_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let mut executor = RedisExecutor::new(&mut conn);
    let comment = comments
        .create(
            &mut executor,
            snugom::snug! {
                CommentRecord {
                    body: "Macro Cascade Comment".to_string(),
                    created_at: created_at + Duration::seconds(2),
                }
            },
        )
        .await
        .expect("create comment");
    let comment_id = comment.id.clone();

    posts
        .mutate_relations(
            &mut executor,
            vec![RelationPlan::with_left(
                "comments",
                post_id.clone(),
                vec![comment_id.clone()],
                Vec::new(),
            )],
        )
        .await
        .expect("attach comment");

    let mut executor = RedisExecutor::new(&mut conn);
    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    posts: [delete post_id.clone()],
                }
            },
        )
        .await
        .expect("macro update delete cascade");

    drop(executor);

    let post_exists: bool = conn.exists(posts.entity_key(&post_id)).await.expect("post exists");
    assert!(!post_exists, "post should be removed by cascade");

    let remaining_posts: Vec<String> = conn.smembers(&relation_key).await.expect("post relation");
    assert!(remaining_posts.is_empty());

    let comment_exists: bool = conn.exists(comments.entity_key(&comment_id)).await.expect("comment exists");
    assert!(!comment_exists);
}
