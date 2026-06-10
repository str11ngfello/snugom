use super::support::*;
use snugom::repository::FindIncludeSpec;

/// Helper: serialize an entity to JSON Value for field assertions (fields are private).
fn to_json<T: serde::Serialize>(val: &T) -> Value {
    serde_json::to_value(val).expect("serialize to value")
}

// ════════════════════════════════════════════════════════════════════════════
// Repo-level find_with_includes tests (Lua script path)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_with_includes_returns_parent_no_includes() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let builder = UserRecord::validation_builder()
        .display_name("Alice".to_string())
        .created_at(Utc::now());
    let user = users.create(&mut executor, builder).await.expect("create user");

    let result = users
        .find_with_includes(&mut conn, &user.id, vec![])
        .await
        .expect("find_with_includes");

    let entity: UserRecord = result.deserialize_entity().expect("deserialize");
    let json = to_json(&entity);
    assert_eq!(json["display_name"], "Alice");
    assert!(result.includes.is_empty());
}

#[tokio::test]
async fn find_with_includes_not_found() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let err = users
        .find_with_includes(&mut conn, "nonexistent_id_xyz", vec![])
        .await;

    assert!(err.is_err());
    match err.unwrap_err() {
        snugom::errors::RepoError::NotFound { .. } => {}
        other => panic!("expected NotFound, got: {other:?}"),
    }
}

#[tokio::test]
async fn find_with_includes_one_relation() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let _posts: Repo<PostRecord> = ns.post_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let created_at = Utc::now();

    let builder = snugom::snug! {
        UserRecord {
            display_name: "Bob".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Post 1".to_string(),
                    created_at: created_at,
                },
                create PostRecord {
                    title: "Post 2".to_string(),
                    created_at: created_at,
                }
            ],
        }
    };
    let user = users.create(&mut executor, builder).await.expect("create user with posts");

    let include_spec = FindIncludeSpec {
        alias: "posts".to_string(),
        target_collection: "posts".to_string(),

        nested: vec![],
    };

    let result = users
        .find_with_includes(&mut conn, &user.id, vec![include_spec])
        .await
        .expect("find_with_includes");

    let entity: UserRecord = result.deserialize_entity().expect("deserialize user");
    let json = to_json(&entity);
    assert_eq!(json["display_name"], "Bob");

    let posts_children = result.children("posts");
    assert_eq!(posts_children.len(), 2);

    let mut titles: Vec<String> = posts_children
        .iter()
        .map(|c| {
            let post: PostRecord = c.deserialize().expect("deserialize post");
            let j = to_json(&post);
            j["title"].as_str().expect("title").to_string()
        })
        .collect();
    titles.sort();
    assert_eq!(titles, vec!["Post 1", "Post 2"]);
}

#[tokio::test]
async fn find_with_includes_empty_relation() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let builder = UserRecord::validation_builder()
        .display_name("Charlie".to_string())
        .created_at(Utc::now());
    let user = users.create(&mut executor, builder).await.expect("create user");

    let include_spec = FindIncludeSpec {
        alias: "posts".to_string(),
        target_collection: "posts".to_string(),

        nested: vec![],
    };

    let result = users
        .find_with_includes(&mut conn, &user.id, vec![include_spec])
        .await
        .expect("find_with_includes");

    let entity: UserRecord = result.deserialize_entity().expect("deserialize");
    let json = to_json(&entity);
    assert_eq!(json["display_name"], "Charlie");
    assert_eq!(result.children("posts").len(), 0);
}

#[tokio::test]
async fn find_with_includes_skips_missing_children() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let _posts: Repo<PostRecord> = ns.post_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let created_at = Utc::now();

    // Create user with one post
    let builder = snugom::snug! {
        UserRecord {
            display_name: "Eve".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Real Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    };
    let user = users.create(&mut executor, builder).await.expect("create user");

    // Manually add a nonexistent ID to the relation SET (simulates deleted entity)
    let rel_key = users.relation_key("posts", &user.id);
    let _: () = redis::cmd("SADD")
        .arg(&rel_key)
        .arg("deleted_post_id_that_no_longer_exists")
        .query_async(&mut conn)
        .await
        .expect("add phantom post id");

    let include_spec = FindIncludeSpec {
        alias: "posts".to_string(),
        target_collection: "posts".to_string(),

        nested: vec![],
    };

    let result = users
        .find_with_includes(&mut conn, &user.id, vec![include_spec])
        .await
        .expect("find_with_includes");

    // Should only return the real post, skipping the phantom
    let posts_children = result.children("posts");
    assert_eq!(posts_children.len(), 1);
    let post: PostRecord = posts_children[0].deserialize().expect("deserialize post");
    assert_eq!(to_json(&post)["title"], "Real Post");
}

#[tokio::test]
async fn find_with_nested_includes() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    ns.register_all_descriptors();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();
    let comments: Repo<CommentRecord> = ns.comment_repo();

    let created_at = Utc::now();

    let user = {
        let mut executor = RedisExecutor::new(&mut conn);
        let user_builder = UserRecord::validation_builder()
            .display_name("Dave".to_string())
            .created_at(created_at);
        users.create(&mut executor, user_builder).await.expect("create user")
    };

    let post = {
        let mut executor = RedisExecutor::new(&mut conn);
        let post_builder = snugom::snug! {
            PostRecord {
                title: "Dave's Post".to_string(),
                created_at: created_at,
                author: [connect user.id.clone()],
            }
        };
        posts.create(&mut executor, post_builder).await.expect("create post")
    };

    let rel_key = users.relation_key("posts", &user.id);
    let _: () = redis::cmd("SADD")
        .arg(&rel_key)
        .arg(&post.id)
        .query_async(&mut conn)
        .await
        .expect("add post to user relation");

    let comment = {
        let mut executor = RedisExecutor::new(&mut conn);
        let comment_builder = snugom::snug! {
            CommentRecord {
                body: "Great post!".to_string(),
                created_at: created_at,
                author: [connect user.id.clone()],
                post: [connect post.id.clone()],
            }
        };
        comments.create(&mut executor, comment_builder).await.expect("create comment")
    };

    let comments_rel_key = posts.relation_key("comments", &post.id);
    let _: () = redis::cmd("SADD")
        .arg(&comments_rel_key)
        .arg(&comment.id)
        .query_async(&mut conn)
        .await
        .expect("add comment to post relation");

    let include_spec = FindIncludeSpec {
        alias: "posts".to_string(),
        target_collection: "posts".to_string(),

        nested: vec![FindIncludeSpec {
            alias: "comments".to_string(),
            target_collection: "comments".to_string(),
    
            nested: vec![],
        }],
    };

    let result = users
        .find_with_includes(&mut conn, &user.id, vec![include_spec])
        .await
        .expect("find_with_includes");

    let entity: UserRecord = result.deserialize_entity().expect("deserialize user");
    assert_eq!(to_json(&entity)["display_name"], "Dave");

    let posts_children = result.children("posts");
    assert_eq!(posts_children.len(), 1);
    let fetched_post: PostRecord = posts_children[0].deserialize().expect("deserialize post");
    assert_eq!(to_json(&fetched_post)["title"], "Dave's Post");

    let comments_children = posts_children[0].children("comments");
    assert_eq!(comments_children.len(), 1);
    let fetched_comment: CommentRecord = comments_children[0].deserialize().expect("deserialize comment");
    assert_eq!(to_json(&fetched_comment)["body"], "Great post!");
}

// ════════════════════════════════════════════════════════════════════════════
// Repo-level get_many tests
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_many_returns_all_existing() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let created_at = Utc::now();
    let mut ids = Vec::new();
    for name in ["Alice", "Bob", "Charlie"] {
        let builder = UserRecord::validation_builder()
            .display_name(name.to_string())
            .created_at(created_at);
        let user = users.create(&mut executor, builder).await.expect("create user");
        ids.push(user.id);
    }

    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    let results = users.get_many(&mut conn, &id_refs).await.expect("get_many");

    assert_eq!(results.len(), 3);
    let mut names: Vec<String> = results
        .iter()
        .map(|u| to_json(u)["display_name"].as_str().expect("name").to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["Alice", "Bob", "Charlie"]);
}

#[tokio::test]
async fn get_many_skips_missing() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let mut executor = RedisExecutor::new(&mut conn);

    let builder = UserRecord::validation_builder()
        .display_name("Exists".to_string())
        .created_at(Utc::now());
    let user = users.create(&mut executor, builder).await.expect("create user");

    let results = users
        .get_many(&mut conn, &[&user.id, "nonexistent_id"])
        .await
        .expect("get_many");

    assert_eq!(results.len(), 1);
    assert_eq!(to_json(&results[0])["display_name"], "Exists");
}

#[tokio::test]
async fn get_many_empty_input() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let results: Vec<UserRecord> = users.get_many(&mut conn, &[]).await.expect("get_many empty");
    assert!(results.is_empty());
}

// ════════════════════════════════════════════════════════════════════════════
// CollectionHandle find_related tests (FT.SEARCH path)
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_related_basic() {
    let client = test_client().await;

    // Ensure search index exists for posts
    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    // Create a user
    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "FK User".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create user");

    for title in ["Post X", "Post Y", "Post Z"] {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: title.to_string(),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut handle = client.collection::<PostRecord>();
    let result = handle
        .find_related("author_id", &user_result.id, snugom::RelationQueryOptions::new())
        .await
        .expect("find_related");

    assert_eq!(result.items.len(), 3);
    let mut titles: Vec<String> = result
        .items
        .iter()
        .map(|p| to_json(p)["title"].as_str().expect("title").to_string())
        .collect();
    titles.sort();
    assert_eq!(titles, vec!["Post X", "Post Y", "Post Z"]);
}

#[tokio::test]
async fn find_related_with_limit() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Limit User".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create user");

    for i in 0..5 {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: format!("Post {i}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut handle = client.collection::<PostRecord>();
    let result = handle
        .find_related(
            "author_id",
            &user_result.id,
            snugom::RelationQueryOptions::new().with_limit(2),
        )
        .await
        .expect("find_related with limit");

    assert_eq!(result.items.len(), 2);
    assert_eq!(result.total, Some(5));
    assert_eq!(result.has_more, Some(true));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! macro tests
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_no_includes() {
    let client = test_client().await;

    let create_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Macro Find User".to_string(),
            created_at: Utc::now(),
        }
    )
    .await
    .expect("create user");

    let entity: UserRecord = snugom::snugom_find!(client, UserRecord(&create_result.id))
        .await
        .expect("find");
    assert_eq!(to_json(&entity)["display_name"], "Macro Find User");
}

#[tokio::test]
async fn macro_find_not_found() {
    let client = test_client().await;

    let err: Result<UserRecord, _> = snugom::snugom_find!(client, UserRecord("nonexistent_xyz")).await;
    assert!(err.is_err());
}

#[tokio::test]
async fn macro_find_with_one_include() {
    let client = test_client().await;

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Include User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Post A".to_string(),
                    created_at: created_at,
                },
                create PostRecord {
                    title: "Post B".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user with posts");

    let (user, posts): (UserRecord, Vec<PostRecord>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord],
        })
        .await
        .expect("find with include");

    assert_eq!(to_json(&user)["display_name"], "Include User");
    assert_eq!(posts.len(), 2);
    let mut titles: Vec<String> = posts
        .iter()
        .map(|p| to_json(p)["title"].as_str().expect("title").to_string())
        .collect();
    titles.sort();
    assert_eq!(titles, vec!["Post A", "Post B"]);
}

#[tokio::test]
async fn macro_find_with_empty_relation() {
    let client = test_client().await;

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Empty Relations".to_string(),
            created_at: Utc::now(),
        }
    )
    .await
    .expect("create user");

    let (user, posts): (UserRecord, Vec<PostRecord>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord],
        })
        .await
        .expect("find with empty include");

    assert_eq!(to_json(&user)["display_name"], "Empty Relations");
    assert!(posts.is_empty());
}

#[tokio::test]
async fn macro_find_with_nested_include() {
    let client = test_client().await;
    let _: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<CommentRecord> = Repo::new(client.prefix().to_string());

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Nested User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Nested Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user");

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let posts_rel_key = user_repo.relation_key("posts", &user_result.id);
    let post_ids: Vec<String> = redis::AsyncCommands::smembers(&mut client.connection(), &posts_rel_key)
        .await
        .expect("get post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = &post_ids[0];

    let comment_result = snugom::snugom_create!(
        client,
        CommentRecord {
            body: "Nested comment".to_string(),
            created_at: created_at,
            author: [connect user_result.id.clone()],
            post: [connect post_id.clone()],
        }
    )
    .await
    .expect("create comment");

    let comments_rel_key = post_repo.relation_key("comments", post_id);
    let _: () = redis::AsyncCommands::sadd(&mut client.connection(), &comments_rel_key, &comment_result.id)
        .await
        .expect("link comment");

    let (user, posts_with_comments): (UserRecord, Vec<(PostRecord, Vec<CommentRecord>)>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord {
                comments: [include CommentRecord],
            }],
        })
        .await
        .expect("find with nested include");

    assert_eq!(to_json(&user)["display_name"], "Nested User");
    assert_eq!(posts_with_comments.len(), 1);

    let (post, comments) = &posts_with_comments[0];
    assert_eq!(to_json(post)["title"], "Nested Post");
    assert_eq!(comments.len(), 1);
    assert_eq!(to_json(&comments[0])["body"], "Nested comment");
}

#[tokio::test]
async fn macro_find_matches_manual() {
    let client = test_client().await;

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Manual Match".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Manual Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user");

    // Macro path
    let (macro_user, macro_posts): (UserRecord, Vec<PostRecord>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord],
        })
        .await
        .expect("macro find");

    // Manual path using CollectionHandle
    let mut handle = client.collection::<UserRecord>();
    let manual_result = handle
        .find_with_includes(
            &user_result.id,
            vec![FindIncludeSpec {
                alias: "posts".to_string(),
                target_collection: "posts".to_string(),
        
                nested: vec![],
            }],
        )
        .await
        .expect("manual find");

    let manual_user: UserRecord = manual_result.deserialize_entity().expect("deserialize");
    let manual_posts: Vec<PostRecord> = manual_result
        .children("posts")
        .iter()
        .map(|c| c.deserialize().expect("deserialize post"))
        .collect();

    // Compare
    assert_eq!(to_json(&macro_user)["id"], to_json(&manual_user)["id"]);
    assert_eq!(to_json(&macro_user)["display_name"], to_json(&manual_user)["display_name"]);
    assert_eq!(macro_posts.len(), manual_posts.len());
}

#[tokio::test]
async fn macro_find_with_include_options() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    // Create user
    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Options User".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create user");

    // Create 5 posts with author_id FK
    for i in 0..5 {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: format!("Option Post {i}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Find user with limited posts via FT.SEARCH path
    let (user, posts): (UserRecord, snugom::RelationData<Vec<PostRecord>>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord { limit: 2 }],
        })
        .await
        .expect("find with options");

    assert_eq!(to_json(&user)["display_name"], "Options User");
    assert_eq!(posts.items.len(), 2);
    assert_eq!(posts.total, Some(5));
    assert_eq!(posts.has_more, Some(true));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find_many! macro tests
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_many_no_includes() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    for name in ["Alpha", "Beta", "Gamma"] {
        snugom::snugom_create!(
            client,
            UserRecord {
                display_name: name.to_string(),
                created_at: created_at,
            }
        )
        .await
        .expect("create user");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
    ))
    .await
    .expect("find_many");

    assert_eq!(result.total, 3);
    assert_eq!(result.items.len(), 3);
}

#[tokio::test]
async fn find_many_with_includes() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    // Create 2 users, each with posts
    let _user1 = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "User One".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "U1 Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user1");

    let _user2 = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "User Two".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "U2 Post A".to_string(),
                    created_at: created_at,
                },
                create PostRecord {
                    title: "U2 Post B".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user2");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
    ) {
        posts: [include PostRecord],
    })
    .await
    .expect("find_many with includes");

    assert_eq!(result.total, 2);
    assert_eq!(result.items.len(), 2);

    // Each item is (UserRecord, Vec<PostRecord>)
    let mut user_post_counts: Vec<(String, usize)> = result
        .items
        .iter()
        .map(|(user, posts)| {
            let name = to_json(user)["display_name"].as_str().expect("name").to_string();
            (name, posts.len())
        })
        .collect();
    user_post_counts.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(user_post_counts[0], ("User One".to_string(), 1));
    assert_eq!(user_post_counts[1], ("User Two".to_string(), 2));
}

#[tokio::test]
async fn find_many_pagination() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    for i in 0..5 {
        snugom::snugom_create!(
            client,
            UserRecord {
                display_name: format!("Page User {i}"),
                created_at: created_at,
            }
        )
        .await
        .expect("create user");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Page 1
    let page1 = snugom::snugom_find_many!(client, UserRecord(
        page_size = 2,
        page = 1,
    ))
    .await
    .expect("page 1");

    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.total, 5);
    assert_eq!(page1.page_size, 2);

    // Page 2
    let page2 = snugom::snugom_find_many!(client, UserRecord(
        page_size = 2,
        page = 2,
    ))
    .await
    .expect("page 2");

    assert_eq!(page2.items.len(), 2);
    assert_eq!(page2.total, 5);
}

#[tokio::test]
async fn find_many_empty_result() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    // No users created — should return empty
    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
    ))
    .await
    .expect("empty find_many");

    assert_eq!(result.total, 0);
    assert!(result.items.is_empty());
}

#[tokio::test]
async fn find_many_with_nested_includes() {
    let client = test_client().await;

    let _: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<CommentRecord> = Repo::new(client.prefix().to_string());

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    // Create user with post
    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Nested Many User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "NM Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user");

    // Get post ID and add a comment
    let user_repo2: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let posts_rel_key = user_repo2.relation_key("posts", &user_result.id);
    let post_ids: Vec<String> = redis::AsyncCommands::smembers(&mut client.connection(), &posts_rel_key)
        .await
        .expect("get post ids");
    let post_id = &post_ids[0];

    let comment_result = snugom::snugom_create!(
        client,
        CommentRecord {
            body: "NM Comment".to_string(),
            created_at: created_at,
            author: [connect user_result.id.clone()],
            post: [connect post_id.clone()],
        }
    )
    .await
    .expect("create comment");

    let comments_rel_key = post_repo.relation_key("comments", post_id);
    let _: () = redis::AsyncCommands::sadd(&mut client.connection(), &comments_rel_key, &comment_result.id)
        .await
        .expect("link comment");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
    ) {
        posts: [include PostRecord {
            comments: [include CommentRecord],
        }],
    })
    .await
    .expect("find_many with nested includes");

    assert_eq!(result.items.len(), 1);
    let (user, posts_with_comments) = &result.items[0];
    assert_eq!(to_json(user)["display_name"], "Nested Many User");
    assert_eq!(posts_with_comments.len(), 1);

    let (post, comments) = &posts_with_comments[0];
    assert_eq!(to_json(post)["title"], "NM Post");
    assert_eq!(comments.len(), 1);
    assert_eq!(to_json(&comments[0])["body"], "NM Comment");
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! — multiple includes
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_with_multiple_simple_includes() {
    let client = test_client().await;

    let created_at = Utc::now();

    // Create user with posts and followers
    let other_user = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Other".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create other user");

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Multi Include".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "MI Post".to_string(),
                    created_at: created_at,
                }
            ],
            followers_ids: [connect other_user.id.clone()],
        }
    )
    .await
    .expect("create user");

    let (user, posts, followers): (UserRecord, Vec<PostRecord>, Vec<UserRecord>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord],
            followers_ids: [include UserRecord],
        })
        .await
        .expect("find with multiple includes");

    assert_eq!(to_json(&user)["display_name"], "Multi Include");
    assert_eq!(posts.len(), 1);
    assert_eq!(to_json(&posts[0])["title"], "MI Post");
    assert_eq!(followers.len(), 1);
    assert_eq!(to_json(&followers[0])["display_name"], "Other");
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! — option include variations
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_with_include_sort() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let now = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Sort User".to_string(),
            created_at: now,
        }
    )
    .await
    .expect("create user");

    // Create posts with different timestamps so sort by created_at_ts is deterministic
    let titles_and_times = [
        ("Third", now + Duration::seconds(30)),
        ("First", now + Duration::seconds(10)),
        ("Second", now + Duration::seconds(20)),
    ];
    for (title, ts) in titles_and_times {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: title.to_string(),
                created_at: ts,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (user, posts): (UserRecord, snugom::RelationData<Vec<PostRecord>>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord { sort: "created_at" }],
        })
        .await
        .expect("find with sort");

    assert_eq!(to_json(&user)["display_name"], "Sort User");
    assert_eq!(posts.items.len(), 3);
    let titles: Vec<String> = posts
        .items
        .iter()
        .map(|p| to_json(p)["title"].as_str().expect("title").to_string())
        .collect();
    assert_eq!(titles, vec!["First", "Second", "Third"]);
}

#[tokio::test]
async fn macro_find_option_include_no_results() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "No Posts User".to_string(),
            created_at: Utc::now(),
        }
    )
    .await
    .expect("create user");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (user, posts): (UserRecord, snugom::RelationData<Vec<PostRecord>>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord { limit: 10 }],
        })
        .await
        .expect("find with options, no results");

    assert_eq!(to_json(&user)["display_name"], "No Posts User");
    assert!(posts.items.is_empty());
    assert_eq!(posts.total, Some(0));
    assert_eq!(posts.has_more, Some(false));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! — mixed simple + option includes
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_mixed_simple_and_option_includes() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    let follower = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Follower".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create follower");

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Mixed User".to_string(),
            created_at: created_at,
            followers_ids: [connect follower.id.clone()],
        }
    )
    .await
    .expect("create user");

    for i in 0..4 {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: format!("Mixed Post {i}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // followers_ids = simple Lua include, posts = option FT.SEARCH include
    let (user, followers, posts): (UserRecord, Vec<UserRecord>, snugom::RelationData<Vec<PostRecord>>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            followers_ids: [include UserRecord],
            posts: [include PostRecord { limit: 2 }],
        })
        .await
        .expect("find with mixed includes");

    assert_eq!(to_json(&user)["display_name"], "Mixed User");
    assert_eq!(followers.len(), 1);
    assert_eq!(to_json(&followers[0])["display_name"], "Follower");
    assert_eq!(posts.items.len(), 2);
    assert_eq!(posts.total, Some(4));
    assert_eq!(posts.has_more, Some(true));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! — nested include edge cases
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_nested_empty_grandchildren() {
    let client = test_client().await;
    let _: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<CommentRecord> = Repo::new(client.prefix().to_string());

    let created_at = Utc::now();

    // Create user with a post but no comments
    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "No Comments User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Commentless Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user");

    let (user, posts_with_comments): (UserRecord, Vec<(PostRecord, Vec<CommentRecord>)>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord {
                comments: [include CommentRecord],
            }],
        })
        .await
        .expect("find with empty grandchildren");

    assert_eq!(to_json(&user)["display_name"], "No Comments User");
    assert_eq!(posts_with_comments.len(), 1);
    let (post, comments) = &posts_with_comments[0];
    assert_eq!(to_json(post)["title"], "Commentless Post");
    assert!(comments.is_empty());
}

#[tokio::test]
async fn macro_find_nested_varying_grandchild_counts() {
    let client = test_client().await;
    let _: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<CommentRecord> = Repo::new(client.prefix().to_string());

    let created_at = Utc::now();

    // Create user with 2 posts
    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Varying User".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Post With Comments".to_string(),
                    created_at: created_at,
                },
                create PostRecord {
                    title: "Post Without Comments".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create user");

    // Get post IDs
    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let posts_rel_key = user_repo.relation_key("posts", &user_result.id);
    let post_ids: Vec<String> = redis::AsyncCommands::smembers(&mut client.connection(), &posts_rel_key)
        .await
        .expect("get post ids");
    assert_eq!(post_ids.len(), 2);

    // Find the post titled "Post With Comments" by checking each
    let mut with_comments_id = String::new();
    for pid in &post_ids {
        let post: PostRecord = Repo::<PostRecord>::new(client.prefix().to_string())
            .get(&mut client.connection(), pid)
            .await
            .expect("get post")
            .expect("post exists");
        if to_json(&post)["title"] == "Post With Comments" {
            with_comments_id = pid.clone();
        }
    }

    // Add 3 comments to the first post only
    for body in ["Comment A", "Comment B", "Comment C"] {
        let cr = snugom::snugom_create!(
            client,
            CommentRecord {
                body: body.to_string(),
                created_at: created_at,
                author: [connect user_result.id.clone()],
                post: [connect with_comments_id.clone()],
            }
        )
        .await
        .expect("create comment");

        let comments_rel_key = post_repo.relation_key("comments", &with_comments_id);
        let _: () = redis::AsyncCommands::sadd(&mut client.connection(), &comments_rel_key, &cr.id)
            .await
            .expect("link comment");
    }

    let (user, posts_with_comments): (UserRecord, Vec<(PostRecord, Vec<CommentRecord>)>) =
        snugom::snugom_find!(client, UserRecord(&user_result.id) {
            posts: [include PostRecord {
                comments: [include CommentRecord],
            }],
        })
        .await
        .expect("find with varying grandchildren");

    assert_eq!(to_json(&user)["display_name"], "Varying User");
    assert_eq!(posts_with_comments.len(), 2);

    let mut counts: Vec<(String, usize)> = posts_with_comments
        .iter()
        .map(|(p, c)| (to_json(p)["title"].as_str().expect("title").to_string(), c.len()))
        .collect();
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(counts[0], ("Post With Comments".to_string(), 3));
    assert_eq!(counts[1], ("Post Without Comments".to_string(), 0));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find! — options + nested combined
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn macro_find_options_with_nested_includes() {
    let client = test_client().await;
    let _: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    let _: Repo<CommentRecord> = Repo::new(client.prefix().to_string());

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Opts+Nested User".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create user");

    // Create 4 posts with author_id FK
    let mut post_ids = Vec::new();
    for i in 0..4 {
        let pr = snugom::snugom_create!(
            client,
            PostRecord {
                title: format!("ON Post {i}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
        post_ids.push(pr.id);
    }

    // Add comments to the first 2 posts
    let post_repo2: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    for pid in &post_ids[..2] {
        let cr = snugom::snugom_create!(
            client,
            CommentRecord {
                body: format!("Comment on {pid}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
                post: [connect pid.clone()],
            }
        )
        .await
        .expect("create comment");

        let comments_rel_key = post_repo2.relation_key("comments", pid);
        let _: () = redis::AsyncCommands::sadd(&mut client.connection(), &comments_rel_key, &cr.id)
            .await
            .expect("link comment");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Options (limit: 2) + nested (comments)
    let (user, posts_data): (
        UserRecord,
        snugom::RelationData<Vec<(PostRecord, Vec<CommentRecord>)>>,
    ) = snugom::snugom_find!(client, UserRecord(&user_result.id) {
        posts: [include PostRecord {
            limit: 2,
            comments: [include CommentRecord],
        }],
    })
    .await
    .expect("find with options + nested");

    assert_eq!(to_json(&user)["display_name"], "Opts+Nested User");
    assert_eq!(posts_data.items.len(), 2);
    assert_eq!(posts_data.total, Some(4));
    assert_eq!(posts_data.has_more, Some(true));

    // Each returned post should have its comments loaded
    for (post, comments) in &posts_data.items {
        let title = to_json(post)["title"].as_str().expect("title").to_string();
        assert!(title.starts_with("ON Post"));
        // Comments may or may not be present depending on which 2 posts were returned
        // but the structure should be correct
        assert!(comments.len() <= 1);
    }
}

// ════════════════════════════════════════════════════════════════════════════
// find_related — additional coverage
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_related_with_sort() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let now = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Sort Related".to_string(),
            created_at: now,
        }
    )
    .await
    .expect("create user");

    let titles_and_times = [
        ("Third", now + Duration::seconds(30)),
        ("First", now + Duration::seconds(10)),
        ("Second", now + Duration::seconds(20)),
    ];
    for (title, ts) in titles_and_times {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: title.to_string(),
                created_at: ts,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut handle = client.collection::<PostRecord>();
    let result = handle
        .find_related(
            "author_id",
            &user_result.id,
            snugom::RelationQueryOptions::new().with_sort("created_at"),
        )
        .await
        .expect("find_related with sort");

    assert_eq!(result.items.len(), 3);
    let titles: Vec<String> = result
        .items
        .iter()
        .map(|p| to_json(p)["title"].as_str().expect("title").to_string())
        .collect();
    assert_eq!(titles, vec!["First", "Second", "Third"]);
}

#[tokio::test]
async fn find_related_no_results() {
    let client = test_client().await;

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Lonely User".to_string(),
            created_at: Utc::now(),
        }
    )
    .await
    .expect("create user");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut handle = client.collection::<PostRecord>();
    let result = handle
        .find_related(
            "author_id",
            &user_result.id,
            snugom::RelationQueryOptions::new(),
        )
        .await
        .expect("find_related no results");

    assert!(result.items.is_empty());
    assert_eq!(result.total, Some(0));
}

// ════════════════════════════════════════════════════════════════════════════
// snugom_find_many! — additional coverage
// ════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_many_with_filter() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    for name in ["Alice", "Bob", "Alice2"] {
        snugom::snugom_create!(
            client,
            UserRecord {
                display_name: name.to_string(),
                created_at: created_at,
            }
        )
        .await
        .expect("create user");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        filter = "display_name:eq:Bob",
        page_size = 10,
    ))
    .await
    .expect("find_many with filter");

    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);
    assert_eq!(to_json(&result.items[0])["display_name"], "Bob");
}

#[tokio::test]
async fn find_many_with_sort() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let now = Utc::now();

    let names_and_times = [
        ("Charlie", now + Duration::seconds(30)),
        ("Alice", now + Duration::seconds(10)),
        ("Bob", now + Duration::seconds(20)),
    ];
    for (name, ts) in names_and_times {
        snugom::snugom_create!(
            client,
            UserRecord {
                display_name: name.to_string(),
                created_at: ts,
            }
        )
        .await
        .expect("create user");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
        sort_by = "created_at",
        sort_order = snugom::SortOrder::Asc,
    ))
    .await
    .expect("find_many with sort");

    assert_eq!(result.items.len(), 3);
    let names: Vec<String> = result
        .items
        .iter()
        .map(|u| to_json(u)["display_name"].as_str().expect("name").to_string())
        .collect();
    assert_eq!(names, vec!["Alice", "Bob", "Charlie"]);
}

#[tokio::test]
async fn find_many_with_option_includes() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let post_repo: Repo<PostRecord> = Repo::new(client.prefix().to_string());
    post_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure post index");

    let created_at = Utc::now();

    let user_result = snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "Option Many User".to_string(),
            created_at: created_at,
        }
    )
    .await
    .expect("create user");

    for i in 0..5 {
        snugom::snugom_create!(
            client,
            PostRecord {
                title: format!("OM Post {i}"),
                created_at: created_at,
                author: [connect user_result.id.clone()],
            }
        )
        .await
        .expect("create post");
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        page_size = 10,
    ) {
        posts: [include PostRecord { limit: 2 }],
    })
    .await
    .expect("find_many with option includes");

    assert_eq!(result.items.len(), 1);
    let (_user, posts) = &result.items[0];
    assert_eq!(posts.items.len(), 2);
    assert_eq!(posts.total, Some(5));
    assert_eq!(posts.has_more, Some(true));
}

#[tokio::test]
async fn find_many_with_filter_and_includes() {
    let client = test_client().await;

    let user_repo: Repo<UserRecord> = Repo::new(client.prefix().to_string());
    user_repo
        .ensure_search_index(&mut client.connection())
        .await
        .expect("ensure user index");

    let created_at = Utc::now();

    snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "TargetUser".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Target Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create target");

    snugom::snugom_create!(
        client,
        UserRecord {
            display_name: "OtherUser".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Other Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    )
    .await
    .expect("create other");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let result = snugom::snugom_find_many!(client, UserRecord(
        filter = "display_name:eq:TargetUser",
        page_size = 10,
    ) {
        posts: [include PostRecord],
    })
    .await
    .expect("find_many with filter + includes");

    assert_eq!(result.total, 1);
    assert_eq!(result.items.len(), 1);
    let (user, posts) = &result.items[0];
    assert_eq!(to_json(user)["display_name"], "TargetUser");
    assert_eq!(posts.len(), 1);
    assert_eq!(to_json(&posts[0])["title"], "Target Post");
}

// ════════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════════

async fn test_client() -> snugom::Client {
    let redis_url = super::common::test_redis_url();
    let prefix = format!("snug_find_test_{}", snugom::id::generate_entity_id());
    snugom::Client::connect(&redis_url, prefix)
        .await
        .expect("connect")
}
