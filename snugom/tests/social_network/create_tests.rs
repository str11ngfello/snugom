use super::support::*;

#[tokio::test]
async fn create_post_with_author_and_followers() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let alice_builder = UserRecord::validation_builder()
        .display_name(String::from("Alice"))
        .created_at(created_at);
    let mut executor = RedisExecutor::new(&mut conn);
    let alice = users.create(&mut executor, alice_builder).await.expect("create alice");
    let alice_id = alice.id;

    let mut builder = PostRecord::validation_builder()
        .title(String::from("First Post"))
        .created_at(created_at)
        .connect("liked_by_ids", vec![alice_id.clone()]);
    builder = builder.relation("author", vec![alice_id.clone()], Vec::new());

    let post = posts.create(&mut executor, builder).await.expect("create post");
    let post_id = post.id;

    let key = posts.entity_key(&post_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("post json");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("arr");
    assert_eq!(arr[0]["title"], Value::String(String::from("First Post")));
    assert_eq!(arr[0]["id"], Value::String(post_id.clone()));

    let likes_key = posts.relation_key("liked_by_ids", &post_id);
    let members: Vec<String> = conn.smembers(&likes_key).await.expect("likes");
    assert!(members.contains(&alice_id));

    let author_key = posts.relation_key("author", &post_id);
    let members: Vec<String> = conn.smembers(&author_key).await.expect("authors");
    assert!(members.contains(&alice_id));
}

#[tokio::test]
async fn macro_create_post_with_author_and_followers() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let user_builder = snugom::snug! {
        UserRecord {
            display_name: "Macro Alice".to_string(),
            created_at: created_at,
        }
    };
    let mut executor = RedisExecutor::new(&mut conn);
    let user = users.create(&mut executor, user_builder).await.expect("create user via macro");
    let user_id = user.id;

    let post_builder = snugom::snug! {
        PostRecord {
            title: "Macro Post".to_string(),
            created_at: created_at,
            author: [connect user_id.clone()],
            liked_by_ids: [connect user_id.clone()],
        }
    };

    let post = posts.create(&mut executor, post_builder).await.expect("create post via macro");
    let post_id = post.id.clone();

    let key = posts.entity_key(&post_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("post json");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("arr");
    assert_eq!(arr[0]["title"], Value::String(String::from("Macro Post")));
    assert_eq!(arr[0]["id"], Value::String(post_id.clone()));

    let likes_key = posts.relation_key("liked_by_ids", &post_id);
    let members: Vec<String> = conn.smembers(&likes_key).await.expect("likes");
    assert!(members.contains(&user_id));

    let author_key = posts.relation_key("author", &post_id);
    let members: Vec<String> = conn.smembers(&author_key).await.expect("authors");
    assert!(members.contains(&user_id));
}

#[tokio::test]
async fn macro_nested_create_posts() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let builder = snugom::snug! {
        UserRecord {
            display_name: "Nested Macro".to_string(),
            created_at: created_at,
            posts: [
                create PostRecord {
                    title: "Nested Macro Post".to_string(),
                    created_at: created_at,
                }
            ],
        }
    };

    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(&mut executor, builder)
        .await
        .expect("create user with nested post");

    let posts_key = users.relation_key("posts", &user.id);
    let post_ids: Vec<String> = conn.smembers(&posts_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let author_key = posts.relation_key("author", &post_id);
    let authors: Vec<String> = conn.smembers(&author_key).await.expect("author relation");
    assert_eq!(authors, vec![user.id.clone()]);

    let post_key = posts.entity_key(&post_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch post json");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("array");
    assert_eq!(arr[0]["title"], Value::String(String::from("Nested Macro Post")));
    assert_eq!(arr[0]["id"], Value::String(post_id.clone()));
}

#[tokio::test]
async fn builder_nested_create_posts() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();
    let posts: Repo<PostRecord> = ns.post_repo();

    let created_at = Utc::now();
    let mut builder = UserRecord::validation_builder()
        .display_name("Nested Builder".to_string())
        .created_at(created_at);
    builder = builder.create_relation(
        "posts",
        PostRecord::validation_builder()
            .title("Nested Builder Post".to_string())
            .created_at(created_at),
    );

    let mut executor = RedisExecutor::new(&mut conn);
    let user = users
        .create(&mut executor, builder)
        .await
        .expect("create user with nested post via builder");

    let posts_key = users.relation_key("posts", &user.id);
    let post_ids: Vec<String> = conn.smembers(&posts_key).await.expect("post ids");
    assert_eq!(post_ids.len(), 1);
    let post_id = post_ids[0].clone();

    let author_key = posts.relation_key("author", &post_id);
    let authors: Vec<String> = conn.smembers(&author_key).await.expect("author relation");
    assert_eq!(authors, vec![user.id.clone()]);

    let post_key = posts.entity_key(&post_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&post_key)
        .arg("$")
        .query_async(&mut conn)
        .await
        .expect("fetch post json");
    let json: Value = serde_json::from_str(&json_str).expect("parse json");
    let arr = json.as_array().expect("array");
    assert_eq!(arr[0]["title"], Value::String(String::from("Nested Builder Post")));
    assert_eq!(arr[0]["id"], Value::String(post_id.clone()));
}
