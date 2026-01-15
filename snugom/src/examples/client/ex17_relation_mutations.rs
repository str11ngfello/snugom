//! Example 17 – Relation Mutations
//!
//! Demonstrates modifying relationships using `snugom_update!` macro:
//! - `connect` - Add IDs to a relation
//! - `disconnect` - Remove IDs from a relation
//! - `delete` - Disconnect and delete the related entity

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "teams")]
struct Team {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,

    /// Has many members - a list of member IDs
    #[serde(default)]
    #[snugom(relation(target = "team_members", cascade = "delete"))]
    members: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "team_members")]
struct TeamMember {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(tag))]
    role: String,

    /// Belongs to a team
    #[snugom(relation(target = "teams"))]
    team_id: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Team, TeamMember])]
struct TeamClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("relation_mutations");
    let client = TeamClient::new(conn, prefix);

    let mut teams = client.teams();
    let mut members = client.team_members();

    // ============ Create Team ============
    let team_id = snugom_create!(client, Team {
        name: "Engineering".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let team = teams.get_or_error(&team_id).await?;

    // ============ Create Members ============
    let alice_id = snugom_create!(client, TeamMember {
        name: "Alice".to_string(),
        role: "developer".to_string(),
        team_id: team.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    let bob_id = snugom_create!(client, TeamMember {
        name: "Bob".to_string(),
        role: "developer".to_string(),
        team_id: team.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    let carol_id = snugom_create!(client, TeamMember {
        name: "Carol".to_string(),
        role: "lead".to_string(),
        team_id: team.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    // ============ Connect Members to Team ============
    // Use snugom_update! macro to connect members to the team
    snugom_update!(client, Team(entity_id = team.id.clone()) {
        members: [
            connect alice_id.clone(),
            connect bob_id.clone(),
        ],
    }).await?;

    // Verify connection (would need to check the relation set in Redis)
    // For now, we just verify the members exist
    assert!(members.exists(&alice_id).await?);
    assert!(members.exists(&bob_id).await?);

    // ============ Connect and Disconnect in One Operation ============
    // Add Carol, remove Alice
    snugom_update!(client, Team(entity_id = team.id.clone()) {
        members: [
            connect carol_id.clone(),
            disconnect alice_id.clone(),
        ],
    }).await?;

    // All members still exist (disconnect only removes from relation, not the entity)
    assert!(members.exists(&alice_id).await?, "alice should still exist");
    assert!(members.exists(&bob_id).await?);
    assert!(members.exists(&carol_id).await?);

    // ============ Delete via Relation ============
    // Delete removes from relation AND deletes the entity
    snugom_update!(client, Team(entity_id = team.id.clone()) {
        members: [
            delete bob_id.clone(),
        ],
    }).await?;

    // Bob should no longer exist
    assert!(!members.exists(&bob_id).await?, "bob should be deleted");

    // Others still exist
    assert!(members.exists(&alice_id).await?);
    assert!(members.exists(&carol_id).await?);

    // ============ Multiple Operations ============
    // You can mix all three in one update
    let dave_id = snugom_create!(client, TeamMember {
        name: "Dave".to_string(),
        role: "intern".to_string(),
        team_id: team.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    snugom_update!(client, Team(entity_id = team.id.clone()) {
        members: [
            connect dave_id.clone(),      // Add Dave
            disconnect carol_id.clone(),  // Remove Carol from team (but keep entity)
        ],
    }).await?;

    // Verify final state
    assert!(members.exists(&alice_id).await?);
    assert!(!members.exists(&bob_id).await?); // Deleted earlier
    assert!(members.exists(&carol_id).await?); // Disconnected but exists
    assert!(members.exists(&dave_id).await?);

    Ok(())
}
