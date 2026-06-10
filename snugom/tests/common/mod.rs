use redis::aio::ConnectionManager;

/// Get the Redis URL for tests. Requires `TEST_REDIS_URL` to be set.
/// Panics if not set to prevent accidentally running tests against the development database.
pub fn test_redis_url() -> String {
    std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| {
        panic!(
            "\n\nTEST_REDIS_URL is not set!\n\
            Tests require a dedicated Redis instance to prevent \
            accidental data loss in your development database.\n\
            Set TEST_REDIS_URL before running tests:\n  \
            export TEST_REDIS_URL=redis://localhost:6379\n"
        )
    })
}

/// Create a Redis connection for tests.
#[allow(dead_code)]
pub async fn redis_conn() -> ConnectionManager {
    let client = redis::Client::open(test_redis_url()).expect("redis client");
    client
        .get_connection_manager()
        .await
        .expect("connection manager")
}
