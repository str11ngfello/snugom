use super::support::*;

#[tokio::test]
async fn idempotency_prevents_duplicate_mutation() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let now = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let builder = UserRecord::validation_builder()
        .display_name(String::from("Idempotent"))
        .created_at(now)
        .idempotency_key("user-create-1");
    let first = users.create(&mut executor, builder).await.expect("first create");
    let user_id = first.id.clone();

    let builder = UserRecord::validation_builder()
        .display_name(String::from("Updated"))
        .created_at(now + Duration::seconds(5))
        .idempotency_key("user-create-1");
    let second = users
        .create(&mut executor, builder)
        .await
        .expect("second create should reuse idempotent result");
    assert_eq!(second.id, user_id);

    let key = users.entity_key(&user_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$.display_name")
        .query_async(&mut conn)
        .await
        .expect("fetch user name");
    let values: Vec<String> = serde_json::from_str(&json_str).expect("parse values");
    assert_eq!(values[0], "Idempotent", "display name should remain from first mutation");
}

#[tokio::test]
async fn macro_idempotency_prevents_duplicate_mutation() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let now = Utc::now();
    let mut executor = RedisExecutor::new(&mut conn);
    let builder = snugom::snug! {
        UserRecord {
            display_name: "Idempotent Macro".to_string(),
            created_at: now,
        }
    }
    .idempotency_key("macro-user-create-1");
    let first = users.create(&mut executor, builder).await.expect("first create");
    let user_id = first.id.clone();

    let builder = snugom::snug! {
        UserRecord {
            display_name: "Updated Macro".to_string(),
            created_at: now + Duration::seconds(5),
        }
    }
    .idempotency_key("macro-user-create-1");
    let second = users
        .create(&mut executor, builder)
        .await
        .expect("second create should reuse idempotent result");
    assert_eq!(second.id, user_id);

    let key = users.entity_key(&user_id);
    let json_str: String = redis::cmd("JSON.GET")
        .arg(&key)
        .arg("$.display_name")
        .query_async(&mut conn)
        .await
        .expect("fetch user name");
    let values: Vec<String> = serde_json::from_str(&json_str).expect("parse values");
    assert_eq!(values[0], "Idempotent Macro", "display name should remain from first mutation");
}
