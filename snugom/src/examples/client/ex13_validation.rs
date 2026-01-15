//! Example 13 – Validation
//!
//! Demonstrates field validation rules and error handling:
//! - Length constraints
//! - Required fields (via builder)
//! - Custom validation errors
//!
//! Note: Validation error testing uses collection-level API to demonstrate
//! error handling patterns. The macro DSL also validates but returns the same errors.

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update, errors::RepoError};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "profiles")]
struct Profile {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,

    /// Username must be 3-20 characters
    #[snugom(validate(length(min = 3, max = 20)))]
    #[snugom(filterable(tag))]
    username: String,

    /// Email must be at least 5 characters
    #[snugom(validate(length(min = 5)))]
    #[snugom(filterable(text))]
    email: String,

    /// Bio has a maximum length
    #[snugom(validate(length(max = 500)))]
    bio: String,

    /// Age must be within range (using numeric validation would require custom impl)
    #[snugom(filterable, sortable)]
    age: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Profile])]
struct ProfileClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("validation");
    let client = ProfileClient::new(conn, prefix);
    let mut profiles = client.profiles();

    // ============ Valid Entity ============
    let profile_id = snugom_create!(client, Profile {
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        bio: "Hello, I'm Alice!".to_string(),
        age: 25,
        created_at: Utc::now(),
    }).await?.id;

    let profile = profiles.get_or_error(&profile_id).await?;
    assert_eq!(profile.username, "alice");

    // ============ Username Too Short ============
    // Using collection API to demonstrate error handling
    let too_short = profiles
        .create(
            Profile::validation_builder()
                .username("ab".to_string()) // Only 2 chars, min is 3
                .email("ab@example.com".to_string())
                .bio("Bio".to_string())
                .age(30)
                .created_at(Utc::now()),
        )
        .await;

    assert!(too_short.is_err(), "username too short should fail");
    match too_short {
        Err(RepoError::Validation(err)) => {
            assert!(!err.issues.is_empty(), "should have validation issues");
            let issue = &err.issues[0];
            assert_eq!(issue.field, "username");
        }
        other => panic!("expected Validation error, got {other:?}"),
    }

    // ============ Username Too Long ============
    let too_long = profiles
        .create(
            Profile::validation_builder()
                .username("a".repeat(25)) // 25 chars, max is 20
                .email("long@example.com".to_string())
                .bio("Bio".to_string())
                .age(30)
                .created_at(Utc::now()),
        )
        .await;

    assert!(too_long.is_err(), "username too long should fail");
    match too_long {
        Err(RepoError::Validation(err)) => {
            let issue = &err.issues[0];
            assert_eq!(issue.field, "username");
        }
        other => panic!("expected Validation error, got {other:?}"),
    }

    // ============ Email Too Short ============
    let bad_email = profiles
        .create(
            Profile::validation_builder()
                .username("validname".to_string())
                .email("a@b".to_string()) // Only 3 chars, min is 5
                .bio("Bio".to_string())
                .age(30)
                .created_at(Utc::now()),
        )
        .await;

    assert!(bad_email.is_err(), "email too short should fail");

    // ============ Bio Too Long ============
    let long_bio = profiles
        .create(
            Profile::validation_builder()
                .username("validuser".to_string())
                .email("valid@example.com".to_string())
                .bio("x".repeat(501)) // 501 chars, max is 500
                .age(30)
                .created_at(Utc::now()),
        )
        .await;

    assert!(long_bio.is_err(), "bio too long should fail");

    // ============ Update Validation ============
    // Validation also applies to updates
    let update_result = profiles
        .update(
            Profile::patch_builder()
                .entity_id(&profile_id)
                .username("x".to_string()), // Too short
        )
        .await;

    assert!(update_result.is_err(), "update with invalid data should fail");

    // ============ Valid Update with Macro ============
    snugom_update!(client, Profile(entity_id = &profile_id) {
        bio: "Updated bio that is valid".to_string(),
    }).await?;

    let updated = profiles.get_or_error(&profile_id).await?;
    assert_eq!(updated.bio, "Updated bio that is valid");

    Ok(())
}
