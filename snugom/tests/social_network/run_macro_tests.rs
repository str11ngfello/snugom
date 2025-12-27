use super::support::*;

#[tokio::test]
async fn run_macro_create_with_nested_relations() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();

    let created_at = Utc::now();
    let comment_at = created_at + Duration::seconds(5);

    let create_result = snugom::run! {
        ns.user_repo(),
        &mut conn,
        create => UserRecord {
            display_name: "Run Macro User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Run Macro Post".to_string(),
                    created_at: created_at,
                    published_at: None,
                    comments: [
                        create CommentRecord {
                            body: "First!".to_string(),
                            created_at: comment_at,
                        }
                    ],
                }
            ],
        }
    }
    .expect("create via run! macro");

    let user_id = create_result.id.clone();

    let fetched = snugom::run! {
        ns.user_repo(),
        &mut conn,
        get => UserRecord(entity_id = user_id.clone())
    }
    .expect("fetch created user")
    .expect("user should exist");
    let user_json = serde_json::to_value(&fetched).expect("serialize user");
    assert_eq!(user_json["display_name"], "Run Macro User");

    let post_repo = ns.post_repo();
    let comment_repo = ns.comment_repo();
    let user_repo = ns.user_repo();

    let post_ids: Vec<String> = conn
        .smembers(user_repo.relation_key("posts", &user_id))
        .await
        .expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let post_authors: Vec<String> = conn
        .smembers(post_repo.relation_key("author", &post_id))
        .await
        .expect("post authors");
    assert_eq!(post_authors, vec![user_id.clone()]);

    let comment_ids: Vec<String> = conn
        .smembers(post_repo.relation_key("comments", &post_id))
        .await
        .expect("comment ids");
    assert_eq!(comment_ids.len(), 1);
    let comment_id = comment_ids[0].clone();

    let comment_exists: bool = conn.exists(comment_repo.entity_key(&comment_id)).await.expect("comment exists");
    assert!(comment_exists, "comment created via nested run! block should exist");
}

#[tokio::test]
async fn run_macro_update_optional_fields() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();

    let created_at = Utc::now();
    let create_result = snugom::run! {
        ns.user_repo(),
        &mut conn,
        create => UserRecord {
            display_name: "Initial Name".to_string(),
            created_at: created_at,
        }
    }
    .expect("initial create");

    snugom::run! {
        ns.user_repo(),
        &mut conn,
        update => UserRecord(entity_id = create_result.id.clone()) {
            display_name?: Some("Updated Name".to_string()),
        }
    }
    .expect("update display name");

    let updated = snugom::run! {
        ns.user_repo(),
        &mut conn,
        get => UserRecord(entity_id = create_result.id.clone())
    }
    .expect("fetch updated")
    .expect("user should exist");
    let user_json = serde_json::to_value(&updated).expect("serialize updated user");
    assert_eq!(user_json["display_name"], "Updated Name");
}

#[tokio::test]
async fn run_macro_upsert_behaviour() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();

    let now = Utc::now();
    let created = snugom::run! {
        ns.user_repo(),
        &mut conn,
        create => UserRecord {
            display_name: "Upsert Target".to_string(),
            created_at: now,
        }
    }
    .expect("seed user");

    let update_result = snugom::run! {
        ns.user_repo(),
        &mut conn,
        upsert => UserRecord() {
            update: UserRecord(entity_id = created.id.clone()) {
                display_name?: Some("Upsert Updated".to_string()),
            },
            create: UserRecord {
                display_name: "Should Not Create".to_string(),
                created_at: now,
            }
        }
    }
    .expect("upsert update");

    match update_result {
        snugom::UpsertResult::Updated(_) => {}
        other => panic!("expected updated branch, got {:?}", other),
    }

    let new_user_result = snugom::run! {
        ns.user_repo(),
        &mut conn,
        upsert => UserRecord() {
            update: UserRecord(entity_id = "non-existent".to_string()) {
                display_name?: Some("No-op".to_string()),
            },
            create: UserRecord {
                display_name: "Upsert Created".to_string(),
                created_at: now + Duration::seconds(30),
            }
        }
    }
    .expect("upsert create");

    match new_user_result {
        snugom::UpsertResult::Created(result) => {
            let fetched = snugom::run! {
                ns.user_repo(),
                &mut conn,
                get => UserRecord(entity_id = result.id.clone())
            }
            .expect("fetch upsert created")
            .expect("new user should exist");
            let user_json = serde_json::to_value(&fetched).expect("serialize new user");
            assert_eq!(user_json["display_name"], "Upsert Created");
        }
        other => panic!("expected created branch, got {:?}", other),
    }
}

#[tokio::test]
async fn run_macro_delete_and_get() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();

    let create_result = snugom::run! {
        ns.user_repo(),
        &mut conn,
        create => UserRecord {
            display_name: "Delete Me".to_string(),
            created_at: Utc::now(),
        }
    }
    .expect("create user");

    snugom::run! {
        ns.user_repo(),
        &mut conn,
        delete => UserRecord(entity_id = create_result.id.clone())
    }
    .expect("delete user");

    let fetched = snugom::run! {
        ns.user_repo(),
        &mut conn,
        get => UserRecord(entity_id = create_result.id.clone())
    }
    .expect("fetch after delete");
    assert!(fetched.is_none(), "user should be absent after delete");
}
