//! Example 20 – Error Handling
//!
//! Demonstrates handling various error conditions:
//! - Entity not found
//! - Unique constraint violations
//! - Validation errors
//! - Version conflicts

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, errors::RepoError};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "error_users")]
struct User {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    /// Uses serde(default) so validation doesn't require it - create sets to 1.
    #[serde(default)]
    #[snugom(version)]
    version: i64,

    #[snugom(unique(case_insensitive))]
    #[snugom(filterable(tag))]
    email: String,

    #[snugom(filterable(text))]
    name: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [User])]
struct UserClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("error_handling");
    let mut client = UserClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut users = client.users();

    // ============ Entity Not Found ============
    {
        // Using get() returns None for missing entities
        let maybe_user = users.get("nonexistent-id").await?;
        assert!(maybe_user.is_none(), "get() should return None for missing entity");

        // Using get_or_error() returns an error for missing entities
        let result = users.get_or_error("nonexistent-id").await;
        match result {
            Err(RepoError::NotFound { .. }) => {
                // Expected - entity doesn't exist
            }
            Ok(_) => panic!("should have returned NotFound error"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    // ============ Unique Constraint Violation ============
    {
        // Create first user
        users
            .create(
                User::validation_builder()
                    .email("alice@example.com".to_string())
                    .name("Alice".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Try to create with same email (case-insensitive)
        let result = users
            .create(
                User::validation_builder()
                    .email("ALICE@EXAMPLE.COM".to_string()) // Same email, different case
                    .name("Another Alice".to_string())
                    .created_at(Utc::now()),
            )
            .await;

        match result {
            Err(RepoError::UniqueConstraintViolation { fields, values, .. }) => {
                assert!(fields.contains(&"email".to_string()));
                assert!(!values.is_empty());
            }
            Ok(_) => panic!("should have returned UniqueConstraintViolation"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    // ============ Version Conflict (Optimistic Locking) ============
    {
        // Create a user
        let user = users
            .create_and_get(
                User::validation_builder()
                    .email("bob@example.com".to_string())
                    .name("Bob".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Update with correct version - succeeds
        users
            .update(
                User::patch_builder()
                    .entity_id(&user.id)
                    .version(user.version)
                    .name("Bob Updated".to_string()),
            )
            .await?;

        // Try to update with old version - fails
        let result = users
            .update(
                User::patch_builder()
                    .entity_id(&user.id)
                    .version(user.version) // Old version (already incremented)
                    .name("Bob Updated Again".to_string()),
            )
            .await;

        match result {
            Err(RepoError::VersionConflict { expected, actual, .. }) => {
                assert_eq!(expected, Some(user.version as u64));
                assert!(actual > expected);
            }
            Ok(_) => panic!("should have returned VersionConflict"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    // ============ Delete with Wrong Version ============
    {
        let user = users
            .create_and_get(
                User::validation_builder()
                    .email("charlie@example.com".to_string())
                    .name("Charlie".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Update to increment version
        users
            .update(
                User::patch_builder()
                    .entity_id(&user.id)
                    .version(user.version)
                    .name("Charlie Updated".to_string()),
            )
            .await?;

        // Try to delete with old version
        let result = users.delete_with_version(&user.id, user.version as u64).await;

        match result {
            Err(RepoError::VersionConflict { .. }) => {
                // Expected - version has changed
            }
            Ok(_) => panic!("should have returned VersionConflict"),
            Err(other) => panic!("unexpected error: {other:?}"),
        }

        // Delete with correct version
        let current = users.get_or_error(&user.id).await?;
        users
            .delete_with_version(&user.id, current.version as u64)
            .await?;

        // Verify deleted
        assert!(!users.exists(&user.id).await?);
    }

    // ============ Graceful Error Handling Pattern ============
    {
        // Pattern: Get-or-create with proper error handling
        let email = "dave@example.com".to_string();

        let user = match users.get("dave-id").await? {
            Some(existing) => existing,
            None => {
                // Try to create, but handle race condition
                match users
                    .create_and_get(
                        User::validation_builder()
                            .email(email.clone())
                            .name("Dave".to_string())
                            .created_at(Utc::now()),
                    )
                    .await
                {
                    Ok(created) => created,
                    Err(RepoError::UniqueConstraintViolation { .. }) => {
                        // Another process created it - fetch by email
                        users
                            .find_first(crate::SearchQuery {
                                filter: vec![format!("email:eq:{email}")],
                                ..Default::default()
                            })
                            .await?
                            .expect("user should exist after unique violation")
                    }
                    Err(other) => return Err(other.into()),
                }
            }
        };

        assert_eq!(user.email, "dave@example.com");
    }

    Ok(())
}
