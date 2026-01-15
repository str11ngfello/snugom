//! Example 14 – Unique Constraints
//!
//! Demonstrates unique field constraints:
//! - Case-sensitive unique
//! - Case-insensitive unique
//! - Compound unique constraints (via separate index)
//!
//! Note: Some error cases use collection API to demonstrate specific error handling.

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update, errors::RepoError};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "accounts")]
struct Account {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,

    /// Email must be unique (case-insensitive)
    /// "alice@example.com" and "Alice@Example.com" are considered duplicates
    #[snugom(unique(case_insensitive))]
    #[snugom(filterable(tag))]
    email: String,

    /// Username must be unique (case-sensitive)
    /// "alice" and "Alice" are different usernames
    #[snugom(unique)]
    #[snugom(filterable(tag))]
    username: String,

    #[snugom(filterable(text))]
    display_name: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Account])]
struct AccountClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("unique_constraints");
    let client = AccountClient::new(conn, prefix);
    let mut accounts = client.accounts();

    // ============ Create Initial Account ============
    let alice_id = snugom_create!(client, Account {
        email: "alice@example.com".to_string(),
        username: "alice".to_string(),
        display_name: "Alice Smith".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let alice = accounts.get_or_error(&alice_id).await?;
    assert_eq!(alice.email, "alice@example.com");
    assert_eq!(alice.username, "alice");

    // ============ Case-Insensitive Email Uniqueness ============
    // Same email with different case should fail
    let duplicate_email = accounts
        .create(
            Account::validation_builder()
                .email("ALICE@EXAMPLE.COM".to_string()) // Same email, different case
                .username("different_user".to_string())
                .display_name("Another Alice".to_string())
                .created_at(Utc::now()),
        )
        .await;

    assert!(duplicate_email.is_err(), "duplicate email should fail");
    match duplicate_email {
        Err(RepoError::UniqueConstraintViolation { fields, .. }) => {
            assert!(fields.contains(&"email".to_string()));
        }
        other => panic!("expected UniqueConstraintViolation, got {other:?}"),
    }

    // ============ Case-Sensitive Username Uniqueness ============
    // Same username with different case should succeed
    let alice2_id = snugom_create!(client, Account {
        email: "alice2@example.com".to_string(),
        username: "Alice".to_string(), // Capital A - different from "alice"
        display_name: "Alice Two".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    assert!(!alice2_id.is_empty(), "different case username should be allowed");

    // ============ Exact Username Duplicate ============
    // Exact same username should fail
    let exact_duplicate = accounts
        .create(
            Account::validation_builder()
                .email("bob@example.com".to_string())
                .username("alice".to_string()) // Exact duplicate
                .display_name("Bob".to_string())
                .created_at(Utc::now()),
        )
        .await;

    assert!(exact_duplicate.is_err(), "exact duplicate username should fail");
    match exact_duplicate {
        Err(RepoError::UniqueConstraintViolation { fields, .. }) => {
            assert!(fields.contains(&"username".to_string()));
        }
        other => panic!("expected UniqueConstraintViolation, got {other:?}"),
    }

    // ============ Valid New Account ============
    // Completely different email and username
    let bob_id = snugom_create!(client, Account {
        email: "bob@example.com".to_string(),
        username: "bob".to_string(),
        display_name: "Bob Jones".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let bob = accounts.get_or_error(&bob_id).await?;
    assert_eq!(bob.email, "bob@example.com");
    assert_eq!(bob.username, "bob");

    // ============ Update Uniqueness ============
    // Updating to a taken value should fail
    let update_to_taken = accounts
        .update(
            Account::patch_builder()
                .entity_id(&bob_id)
                .email("alice@example.com".to_string()), // Alice's email
        )
        .await;

    assert!(update_to_taken.is_err(), "updating to taken email should fail");

    // ============ Update to New Value ============
    // Updating to a new unique value should succeed
    snugom_update!(client, Account(entity_id = &bob_id) {
        email: "robert@example.com".to_string(),
    }).await?;

    // Verify update
    let updated_bob = accounts.get_or_error(&bob_id).await?;
    assert_eq!(updated_bob.email, "robert@example.com");

    Ok(())
}
