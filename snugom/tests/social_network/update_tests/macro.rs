#[tokio::test]
async fn macro_update_disconnects_follower() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Connect Base".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = user.id.clone();
    let mut version = user.responses[0]["version"].as_u64().expect("version");

    let follower = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower".to_string(),
                    created_at: created_at + Duration::seconds(5),
                }
            },
        )
        .await
        .expect("create follower");
    let follower_id = follower.id.clone();

    let connect_responses = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = version) {
                    followers_ids: [connect follower_id.clone()],
                }
            },
        )
        .await
        .expect("connect follower");
    version = connect_responses[0]["version"].as_u64().expect("version after connect");

    drop(executor);

    let relation_key = users.relation_key("followers_ids", &user_id);
    let members: Vec<String> = conn.smembers(&relation_key).await.expect("followers_ids");
    assert!(members.contains(&follower_id));

    let mut executor = RedisExecutor::new(&mut conn);
    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = version) {
                    followers_ids: [disconnect follower_id.clone()],
                }
            },
        )
        .await
        .expect("disconnect follower");

    drop(executor);

    let members: Vec<String> = conn.smembers(&relation_key).await.expect("followers_ids");
    assert!(!members.contains(&follower_id));
}
#[tokio::test]
async fn macro_delete_detaches_followers() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Leader".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create leader");
    let user_id = user.id.clone();

    let follower = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower".to_string(),
                    created_at: created_at + Duration::seconds(1),
                }
            },
        )
        .await
        .expect("create follower");
    let follower_id = follower.id.clone();

    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = user.responses[0]["version"].as_u64().expect("version")) {
                    followers_ids: [connect follower_id.clone()],
                }
            },
        )
        .await
        .expect("connect follower");

    drop(executor);

    let forward_key = users.relation_key("followers_ids", &user_id);
    let reverse_key = users.relation_reverse_key("followers_ids", &follower_id);
    let forward_members: Vec<String> = conn.smembers(&forward_key).await.expect("forward members");
    assert!(forward_members.contains(&follower_id));

    let mut executor = RedisExecutor::new(&mut conn);
    users.delete(&mut executor, &user_id, None).await.expect("delete user");
    drop(executor);

    let user_exists: bool = conn.exists(users.entity_key(&user_id)).await.expect("user exists");
    assert!(!user_exists);
    let follower_exists: bool = conn.exists(users.entity_key(&follower_id)).await.expect("follower exists");
    assert!(follower_exists);

    let forward_members: Vec<String> = conn.smembers(&forward_key).await.expect("forward cleared");
    assert!(forward_members.is_empty());
    let reverse_members: Vec<String> = conn.smembers(&reverse_key).await.expect("reverse cleared");
    assert!(reverse_members.is_empty());
    let reverse_exists: bool = conn.exists(&reverse_key).await.expect("reverse key exists");
    assert!(!reverse_exists);
}
#[tokio::test]
async fn macro_delete_follower_detaches_from_leaders() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let leader = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Leader".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create leader");
    let leader_id = leader.id.clone();

    let follower = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower".to_string(),
                    created_at: created_at + Duration::seconds(1),
                }
            },
        )
        .await
        .expect("create follower");
    let follower_id = follower.id.clone();

    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = leader_id.clone(), expected_version = leader.responses[0]["version"].as_u64().expect("version")) {
                    followers_ids: [connect follower_id.clone()],
                }
            },
        )
        .await
        .expect("connect follower");

    drop(executor);

    let forward_key = users.relation_key("followers_ids", &leader_id);
    let reverse_key = users.relation_reverse_key("followers_ids", &follower_id);
    let forward_members: Vec<String> = conn.smembers(&forward_key).await.expect("forward members");
    assert!(forward_members.contains(&follower_id));

    let mut executor = RedisExecutor::new(&mut conn);
    users.delete(&mut executor, &follower_id, None).await.expect("delete follower");
    drop(executor);

    let follower_exists: bool = conn.exists(users.entity_key(&follower_id)).await.expect("follower exists");
    assert!(!follower_exists);
    let leader_exists: bool = conn.exists(users.entity_key(&leader_id)).await.expect("leader exists");
    assert!(leader_exists);

    let forward_members: Vec<String> = conn.smembers(&forward_key).await.expect("forward cleared");
    assert!(!forward_members.contains(&follower_id));
    let reverse_exists: bool = conn.exists(&reverse_key).await.expect("reverse exists");
    assert!(!reverse_exists);
}
#[tokio::test]
async fn macro_update_clears_optional_datetime() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let author = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Author".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create author");
    let author_id = author.id.clone();

    let scheduled_at = created_at + Duration::minutes(10);
    let post = posts
        .create(
            &mut executor,
            snugom::snug! {
                PostRecord {
                    title: "Macro Scheduled".to_string(),
                    created_at: created_at,
                    published_at: Some(scheduled_at),
                    author: [connect author_id.clone()],
                }
            },
        )
        .await
        .expect("create post");
    let post_id = post.id.clone();
    let initial_version = post.responses[0]["version"].as_u64().expect("version");

    drop(executor);

    let post_key = posts.entity_key(&post_id);
    let json_before: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("json before");
    let value_before: Value = serde_json::from_str(&json_before).expect("parse before");
    let before_arr = value_before.as_array().expect("array before");
    assert!(before_arr[0]["published_at_ts"].as_i64().is_some());

    let mut executor = RedisExecutor::new(&mut conn);
    let clear_patch = snugom::snug! {
        PostRecord(entity_id = post_id.clone(), expected_version = initial_version) {
            published_at: None,
        }
    };
    posts
        .update_patch(&mut executor, clear_patch)
        .await
        .expect("clear published_at");
    drop(executor);

    let json_after: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("json after");
    let value_after: Value = serde_json::from_str(&json_after).expect("parse after");
    let after_arr = value_after.as_array().expect("array after");
    assert!(after_arr[0].get("published_at_ts").is_none());
    assert!(after_arr[0]["published_at"].is_null());
}
#[tokio::test]
async fn macro_update_unknown_relation_errors() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Alias".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = user.id.clone();

    let err = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = user.responses[0]["version"].as_u64().expect("version")) {
                    not_a_relation: [connect "x".to_string()],
                }
            },
        )
        .await
        .expect_err("expected validation failure");
    drop(executor);
    assert!(matches!(err, RepoError::Validation(_)));
}
#[tokio::test]
async fn macro_update_returns_validation_error() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Valid".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = created.id.clone();
    let version = created.responses[0]["version"].as_u64().expect("version");

    let err = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = version) {}
            },
        )
        .await
        .expect_err("expected validation failure");
    drop(executor);

    assert!(matches!(err, RepoError::Validation(_)));
}
#[tokio::test]
async fn macro_update_version_conflict() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Version".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = created.id.clone();

    let err = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = 999) {
                    display_name: "Macro Should Fail".to_string(),
                }
            },
        )
        .await
        .expect_err("expected version conflict");
    drop(executor);

    match err {
        RepoError::VersionConflict { expected, actual } => {
            assert_eq!(expected, Some(999));
            assert_eq!(actual, Some(1));
        }
        other => panic!("expected version conflict error, got {:?}", other),
    }
}
#[tokio::test]
async fn macro_update_idempotency_replay() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Idempotent".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = created.id.clone();
    let base_version = created.responses[0]["version"].as_u64().expect("version");

    let macro_builder = |name: &str| {
        snugom::snug! {
            UserRecord(entity_id = user_id.clone(), expected_version = base_version, idempotency_key = "macro-update-1".to_string()) {
                display_name: name.to_string(),
            }
        }
    };

    let builder_once = macro_builder("Macro Once");

    let first = users
        .update_patch(&mut executor, builder_once)
        .await
        .expect("first update");
    let version_after_first = first[0]["version"].as_u64().expect("version");

    drop(executor);

    let mut executor = RedisExecutor::new(&mut conn);

    let second = users
        .update_patch(&mut executor, macro_builder("Macro Twice"))
        .await
        .expect("second update");
    let version_after_second = second[0]["version"].as_u64().expect("version");

    drop(executor);

    assert_eq!(version_after_first, version_after_second);

    let json: String = redis::cmd("JSON.GET")
        .arg(users.entity_key(&user_id))
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch user");
    let value: Value = serde_json::from_str(&json).expect("parse json");
    let arr = value.as_array().expect("array");
    assert_eq!(arr[0]["metadata"]["version"].as_u64().unwrap(), version_after_first);
    assert_eq!(arr[0]["display_name"], Value::String(String::from("Macro Once")));
}
#[tokio::test]
async fn macro_update_mixed_relation_ops() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let base = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Mixer".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create base");
    let user_id = base.id.clone();
    let mut version = base.responses[0]["version"].as_u64().expect("version");

    let follower_a = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower A".to_string(),
                    created_at: created_at + Duration::seconds(1),
                }
            },
        )
        .await
        .expect("create follower a");
    let follower_a_id = follower_a.id.clone();

    let follower_b = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower B".to_string(),
                    created_at: created_at + Duration::seconds(2),
                }
            },
        )
        .await
        .expect("create follower b");
    let follower_b_id = follower_b.id.clone();

    let responses = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = version) {
                    followers_ids: [connect follower_a_id.clone()],
                }
            },
        )
        .await
        .expect("initial connect");
    version = responses[0]["version"].as_u64().expect("version after connect");

    let responses = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = version) {
                    followers_ids: [
                        connect follower_b_id.clone(),
                        disconnect follower_a_id.clone(),
                    ],
                }
            },
        )
        .await
        .expect("mixed update");
    version = responses[0]["version"].as_u64().expect("version after mixed");

    drop(executor);

    let forward_key = users.relation_key("followers_ids", &user_id);
    let reverse_a = users.relation_reverse_key("followers_ids", &follower_a_id);
    let reverse_b = users.relation_reverse_key("followers_ids", &follower_b_id);

    let members: Vec<String> = conn.smembers(&forward_key).await.expect("forward members");
    assert!(members.contains(&follower_b_id));
    assert!(!members.contains(&follower_a_id));

    let reverse_members_a: Vec<String> = conn.smembers(&reverse_a).await.expect("reverse a");
    assert!(!reverse_members_a.contains(&user_id));
    let reverse_members_b: Vec<String> = conn.smembers(&reverse_b).await.expect("reverse b");
    assert!(reverse_members_b.contains(&user_id));

    let json: String = redis::cmd("JSON.GET")
        .arg(users.entity_key(&user_id))
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch user");
    let value: Value = serde_json::from_str(&json).expect("parse json");
    let arr = value.as_array().expect("array");
    assert_eq!(arr[0]["metadata"]["version"].as_u64().unwrap(), version);
}
#[tokio::test]
async fn macro_update_nested_post_with_comments() {
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
                    display_name: "Macro Thread".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = user.id.clone();
    let initial_version = user.responses[0]["version"].as_u64().expect("version");

    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    posts: [
                        create PostRecord {
                            title: "Macro Nested Post".to_string(),
                            created_at: created_at + Duration::seconds(1),
                            published_at: Some(created_at + Duration::seconds(2)),
                            comments: [
                                create CommentRecord {
                                    body: "Macro comment".to_string(),
                                    created_at: created_at + Duration::seconds(3),
                                }
                            ],
                        }
                    ],
                }
            },
        )
        .await
        .expect("nested macro update");

    drop(executor);

    let posts_key = users.relation_key("posts", &user_id);
    let post_ids: Vec<String> = conn.smembers(&posts_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let author_key = posts.relation_key("author", &post_id);
    let authors: Vec<String> = conn.smembers(&author_key).await.expect("author relation");
    assert_eq!(authors, vec![user_id.clone()]);

    let comments_key = posts.relation_key("comments", &post_id);
    let comment_ids: Vec<String> = conn.smembers(&comments_key).await.expect("comment ids");
    assert_eq!(comment_ids.len(), 1);
    let comment_id = comment_ids[0].clone();

    let comment_json: String = redis::cmd("JSON.GET")
        .arg(comments.entity_key(&comment_id))
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch comment");
    let comment_value: Value = serde_json::from_str(&comment_json).expect("parse comment");
    let comment_arr = comment_value.as_array().expect("comment array");
    assert_eq!(comment_arr[0]["body"], Value::String(String::from("Macro comment")));
}
#[tokio::test]
async fn macro_update_user_profile() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Original".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user");
    let user_id = created.id.clone();
    let initial_version = created.responses[0]["version"].as_u64().unwrap();

    let follower = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Follower".to_string(),
                    created_at: created_at + Duration::seconds(1),
                }
            },
        )
        .await
        .expect("create follower");
    let follower_id = follower.id.clone();

    drop(executor);

    let mut executor = RedisExecutor::new(&mut conn);
    users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    display_name: "Macro Updated".to_string(),
                    followers_ids: [connect follower_id.clone()],
                }
            },
        )
        .await
        .expect("update user via macro");

    drop(executor);

    let key = users.entity_key(&user_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch user");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("arr");
    assert_eq!(arr[0]["display_name"], Value::String(String::from("Macro Updated")));
    assert_eq!(arr[0]["metadata"]["version"], Value::Number(2.into()));

    let followers_key = users.relation_key("followers_ids", &user_id);
    let members: Vec<String> = conn.smembers(&followers_key).await.expect("followers_ids");
    assert!(members.contains(&follower_id));
}
#[tokio::test]
async fn macro_update_without_expected_version() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Blind".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user via macro");
    let user_id = created.id.clone();

    drop(executor);

    let mut executor = RedisExecutor::new(&mut conn);
    let first = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone()) {
                    display_name: "Macro Blind v2".to_string(),
                }
            },
        )
        .await
        .expect("macro blind update");
    let version_after_first = first[0]["version"].as_u64().unwrap();
    assert_eq!(version_after_first, 2);

    let second = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone()) {
                    display_name: "Macro Blind v3".to_string(),
                }
            },
        )
        .await
        .expect("second macro blind update");
    let version_after_second = second[0]["version"].as_u64().unwrap();
    assert_eq!(version_after_second, 3);

    drop(executor);

    let key = users.entity_key(&user_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch user");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("arr");
    assert_eq!(arr[0]["display_name"], Value::String(String::from("Macro Blind v3")));
    assert_eq!(arr[0]["metadata"]["version"], Value::Number(3.into()));
}
#[tokio::test]
async fn macro_update_detects_version_conflict() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let created_at = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let created = users
        .create(
            &mut executor,
            snugom::snug! {
                UserRecord {
                    display_name: "Macro Conflict".to_string(),
                    created_at: created_at,
                }
            },
        )
        .await
        .expect("create user via macro");
    let user_id = created.id.clone();
    let initial_version = created.responses[0]["version"].as_u64().unwrap();

    let committed = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    display_name: "Macro Updated".to_string(),
                }
            },
        )
        .await
        .expect("macro update");
    let committed_version = committed[0]["version"].as_u64().unwrap();

    let err = users
        .update_patch(
            &mut executor,
            snugom::snug! {
                UserRecord(entity_id = user_id.clone(), expected_version = initial_version) {
                    display_name: "Macro Conflict Second".to_string(),
                }
            },
        )
        .await
        .expect_err("macro stale version should conflict");

    match err {
        RepoError::VersionConflict { expected, actual } => {
            assert_eq!(expected, Some(initial_version));
            assert_eq!(actual, Some(committed_version));
        }
        other => panic!("expected version conflict, got {other:?}"),
    }
}
#[tokio::test]
async fn macro_repo_create_returns_validation_errors() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let builder = snugom::snug! {
        UserRecord {
            created_at: Utc::now(),
        }
    };
    let mut executor = RedisExecutor::new(&mut conn);
    let err = users
        .create(&mut executor, builder)
        .await
        .expect_err("expected validation error");
    assert!(matches!(err, RepoError::Validation(_)));
}
