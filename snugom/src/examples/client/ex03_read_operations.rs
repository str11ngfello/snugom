//! Example 03 – Read Operations
//!
//! Demonstrates the various ways to read entities:
//! - `get()` - Returns Option<T>
//! - `get_or_error()` - Returns T or error if not found
//! - `exists()` - Check if entity exists
//! - `count()` - Count all entities in collection
//!
//! Note: Read operations use the collection-level API since there's no
//! macro DSL for reads (reads don't need the complex nesting support).

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_delete, errors::RepoError};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "users")]
struct User {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(text))]
    email: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [User])]
struct UserClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("read_ops");
    let mut client = UserClient::new(conn, prefix);
    let mut users = client.users();

    // Create some test data using snugom_create! macro
    let alice = snugom_create!(client, User {
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        created_at: Utc::now(),
    }).await?;

    let bob = snugom_create!(client, User {
        name: "Bob".to_string(),
        email: "bob@example.com".to_string(),
        created_at: Utc::now(),
    }).await?;

    // ============ get() ============
    // Returns Option<T> - None if not found
    let maybe_alice = users.get(&alice.id).await?;
    assert!(maybe_alice.is_some());
    assert_eq!(maybe_alice.unwrap().name, "Alice");

    // Returns None for non-existent ID
    let not_found = users.get("nonexistent_id").await?;
    assert!(not_found.is_none(), "should return None for missing entity");

    // ============ get_or_error() ============
    // Returns T or RepoError::NotFound
    let bob_user = users.get_or_error(&bob.id).await?;
    assert_eq!(bob_user.email, "bob@example.com");

    // Returns error for non-existent ID
    let err = users.get_or_error("nonexistent_id").await;
    assert!(err.is_err(), "should error for missing entity");
    match err {
        Err(RepoError::NotFound { .. }) => { /* expected */ }
        other => panic!("expected NotFound error, got {other:?}"),
    }

    // ============ exists() ============
    // Returns bool indicating if entity exists
    assert!(users.exists(&alice.id).await?, "alice should exist");
    assert!(users.exists(&bob.id).await?, "bob should exist");
    assert!(!users.exists("nonexistent").await?, "nonexistent should not exist");

    // ============ count() ============
    // Returns total count of entities in collection
    assert_eq!(users.count().await?, 2, "should have 2 users");

    // Count updates after delete
    snugom_delete!(client, User(&alice.id)).await?;
    assert_eq!(users.count().await?, 1, "should have 1 user after delete");

    Ok(())
}
