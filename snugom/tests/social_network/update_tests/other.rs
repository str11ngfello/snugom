#[tokio::test]
async fn repo_create_returns_validation_errors() {
    let mut conn = redis_conn().await;
    let ns = TestNamespace::unique();
    let users: Repo<UserRecord> = ns.user_repo();

    let builder = UserRecord::validation_builder().created_at(Utc::now());
    let mut executor = RedisExecutor::new(&mut conn);
    let err = users
        .create(&mut executor, builder)
        .await
        .expect_err("expected validation error");
    assert!(matches!(err, RepoError::Validation(_)));
}
