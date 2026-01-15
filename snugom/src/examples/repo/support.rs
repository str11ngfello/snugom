use anyhow::Result;
use redis::Client;
use redis::aio::ConnectionManager;

use crate::id::generate_entity_id;

/// Establish a connection manager pointing at the local Redis instance.
pub async fn redis_connection() -> Result<ConnectionManager> {
    let client = Client::open("redis://127.0.0.1/")?;
    let manager = client.get_connection_manager().await?;
    Ok(manager)
}

/// Unique namespace prefix for isolating example data.
pub fn unique_namespace(label: &str) -> String {
    let salt = generate_entity_id();
    format!("snug_example_{label}_{}", &salt[..8])
}
