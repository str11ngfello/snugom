use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomEntity, repository::{Repo, RelationPlan}};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "relation_boards")]
struct RelationBoard {
    #[snugom(id)]
    board_id: String,
    name: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[serde(default)]
    #[snugom(relation(target = "relation_members", cascade = "delete"))]
    board_members: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(schema = 1, service = "examples", collection = "relation_members")]
struct RelationMember {
    #[snugom(id)]
    member_id: String,
    user_id: String,
    #[snugom(datetime, filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
    #[snugom(relation(target = "relation_boards"))]
    board_id: String,
}

/// Example 11 – relation mutations using Repo API (connect, disconnect).
///
/// This demonstrates low-level relation management with `mutate_relations_with_conn`.
/// For a higher-level API using macros, see the client examples.
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("relation_mutations");

    let board_repo: Repo<RelationBoard> = Repo::new(prefix.clone());
    let member_repo: Repo<RelationMember> = Repo::new(prefix);

    // Create a board
    let board = board_repo
        .create_with_conn(
            &mut conn,
            RelationBoard::validation_builder()
                .name("Strategy Board".to_string())
                .created_at(Utc::now()),
        )
        .await?;
    let board_id = board.id.clone();

    // Create two members with relation to the board
    let member_one = member_repo
        .create_with_conn(
            &mut conn,
            RelationMember::validation_builder()
                .user_id("alpha".to_string())
                .joined_at(Utc::now())
                .relation("board", vec![board_id.clone()], Vec::new()),
        )
        .await?;
    let member_one_id = member_one.id.clone();

    let member_two = member_repo
        .create_with_conn(
            &mut conn,
            RelationMember::validation_builder()
                .user_id("beta".to_string())
                .joined_at(Utc::now())
                .relation("board", vec![board_id.clone()], Vec::new()),
        )
        .await?;
    let member_two_id = member_two.id.clone();

    // Connect member_one to board using mutate_relations_with_conn
    let relation_key = board_repo.relation_key("board_members", &board_id);
    board_repo
        .mutate_relations_with_conn(
            &mut conn,
            vec![RelationPlan::with_left(
                "board_members",
                board_id.clone(),
                vec![member_one_id.clone()], // connect
                Vec::new(),                   // disconnect
            )],
        )
        .await?;

    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(members, vec![member_one_id.clone()], "member one connected");

    // Swap members: connect member_two, disconnect member_one
    board_repo
        .mutate_relations_with_conn(
            &mut conn,
            vec![RelationPlan::with_left(
                "board_members",
                board_id.clone(),
                vec![member_two_id.clone()],  // connect
                vec![member_one_id.clone()],  // disconnect
            )],
        )
        .await?;

    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(members, vec![member_two_id.clone()], "member list updated");

    // Disconnect member_two
    board_repo
        .mutate_relations_with_conn(
            &mut conn,
            vec![RelationPlan::with_left(
                "board_members",
                board_id.clone(),
                Vec::new(),                   // connect
                vec![member_two_id.clone()],  // disconnect
            )],
        )
        .await?;

    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert!(members.is_empty(), "relation set cleared");

    Ok(())
}
