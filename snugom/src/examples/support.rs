use anyhow::Result;
use redis::Client;
use redis::aio::ConnectionManager;

use crate::id::generate_entity_id;

/// Establish a connection manager using `TEST_REDIS_URL`.
pub async fn redis_connection() -> Result<ConnectionManager> {
    let url = std::env::var("TEST_REDIS_URL")
        .expect("TEST_REDIS_URL must be set to run examples");
    let client = Client::open(url.as_str())?;
    let manager = client.get_connection_manager().await?;
    Ok(manager)
}

/// Unique namespace prefix for isolating example data.
pub fn unique_namespace(label: &str) -> String {
    let salt = generate_entity_id();
    format!("snug_example_{label}_{}", &salt[..8])
}
