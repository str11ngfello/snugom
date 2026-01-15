//! Example 12 – Timestamps
//!
//! Demonstrates automatic timestamp management:
//! - `#[snugom(created_at)]` - Set once on creation
//! - `#[snugom(updated_at)]` - Updated on every modification

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, sleep};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "notes")]
struct Note {
    #[snugom(id)]
    id: String,
    /// Automatically set to current time on creation.
    /// Cannot be modified after initial creation.
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    /// Automatically updated to current time on every update.
    #[snugom(updated_at)]
    updated_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    content: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Note])]
struct NoteClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("timestamps");
    let client = NoteClient::new(conn, prefix);
    let mut notes = client.notes();

    // ============ Created At ============
    // created_at is automatically set on creation
    let before_create = Utc::now();

    let note_id = snugom_create!(client, Note {
        title: "My First Note".to_string(),
        content: "This is the content".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }).await?.id;

    let after_create = Utc::now();

    let note = notes.get_or_error(&note_id).await?;

    // created_at should be between before and after
    assert!(
        note.created_at >= before_create && note.created_at <= after_create,
        "created_at should be set to current time"
    );

    // Initially, updated_at equals created_at
    let initial_created_at = note.created_at;
    let initial_updated_at = note.updated_at;

    // Small delay to ensure timestamps differ
    sleep(Duration::from_millis(100)).await;

    // ============ Updated At ============
    // updated_at is automatically updated on modification
    snugom_update!(client, Note(entity_id = &note_id) {
        content: "Updated content".to_string(),
    }).await?;

    let updated_note = notes.get_or_error(&note_id).await?;

    // created_at should remain unchanged
    assert_eq!(
        updated_note.created_at, initial_created_at,
        "created_at should never change"
    );

    // updated_at should be newer than initial
    assert!(
        updated_note.updated_at > initial_updated_at,
        "updated_at should be updated"
    );

    // ============ Multiple Updates ============
    // Each update should refresh updated_at
    let before_second_update = updated_note.updated_at;

    sleep(Duration::from_millis(100)).await;

    snugom_update!(client, Note(entity_id = &note_id) {
        title: "Updated Title".to_string(),
    }).await?;

    let twice_updated = notes.get_or_error(&note_id).await?;

    assert!(
        twice_updated.updated_at > before_second_update,
        "updated_at should change on each update"
    );
    assert_eq!(
        twice_updated.created_at, initial_created_at,
        "created_at still unchanged"
    );

    // ============ Verify Final State ============
    assert_eq!(twice_updated.title, "Updated Title");
    assert_eq!(twice_updated.content, "Updated content");

    Ok(())
}
