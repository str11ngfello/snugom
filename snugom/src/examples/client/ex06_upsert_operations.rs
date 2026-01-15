//! Example 06 – Upsert Operations
//!
//! Demonstrates create-or-update semantics with `snugom_upsert!` macro:
//! - Atomic upsert with Prisma-style syntax
//! - Separate create and update blocks
//! - Idempotency key matching for existing entities

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_upsert};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "settings")]
struct UserSettings {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    updated_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    user_id: String,
    theme: String,
    notifications_enabled: bool,
    language: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [UserSettings])]
struct SettingsClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("upsert_ops");
    let mut client = SettingsClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut settings = client.user_settingses();

    let user_id = "user_123";
    // Use user_id as idempotency key to enable upsert behavior
    let idempotency_key = format!("settings:{user_id}");

    // ============ snugom_upsert! - Create case ============
    // First upsert creates the entity since it doesn't exist
    snugom_upsert!(client, UserSettings(id = &idempotency_key) {
        create: UserSettings {
            user_id: user_id.to_string(),
            theme: "light".to_string(),
            notifications_enabled: true,
            language: "en".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        update: UserSettings(entity_id = &idempotency_key) {
            theme: "dark".to_string(),
        },
    }).await?;

    // Verify it was created with create block values
    let created = settings
        .find_first(crate::SearchQuery {
            filter: vec![format!("user_id:eq:{user_id}")],
            ..Default::default()
        })
        .await?
        .expect("settings should exist");

    assert_eq!(created.theme, "light", "should use create block values");
    assert!(created.notifications_enabled);
    assert_eq!(created.language, "en");
    let settings_id = created.id.clone();

    // ============ snugom_upsert! - Update case ============
    // Second upsert with same id updates the existing entity
    snugom_upsert!(client, UserSettings(id = &idempotency_key) {
        create: UserSettings {
            user_id: user_id.to_string(),
            theme: "light".to_string(),
            notifications_enabled: true,
            language: "en".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        update: UserSettings(entity_id = &settings_id) {
            theme: "dark".to_string(),
            language: "fr".to_string(),
        },
    }).await?;

    // Verify it was updated
    let updated = settings.get_or_error(&settings_id).await?;
    assert_eq!(updated.theme, "dark", "should use update block values");
    assert_eq!(updated.language, "fr");
    assert!(updated.notifications_enabled, "unchanged fields preserved");

    // ============ Another User's Settings ============
    // Create settings for a different user
    let another_user = "user_456";
    let another_key = format!("settings:{another_user}");

    snugom_upsert!(client, UserSettings(id = &another_key) {
        create: UserSettings {
            user_id: another_user.to_string(),
            theme: "dark".to_string(),
            notifications_enabled: false,
            language: "de".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        update: UserSettings(entity_id = &another_key) {
            theme: "system".to_string(),
        },
    }).await?;

    // Verify creation (since it didn't exist, create block was used)
    let another = settings
        .find_first(crate::SearchQuery {
            filter: vec![format!("user_id:eq:{another_user}")],
            ..Default::default()
        })
        .await?
        .expect("settings should exist");

    assert_eq!(another.theme, "dark", "should use create block for new entity");
    assert_eq!(settings.count().await?, 2, "should have 2 settings");

    Ok(())
}
