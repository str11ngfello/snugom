use anyhow::Result;
use chrono::Utc;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};

use crate::examples::support;
use crate::{SnugomEntity, bundle, repository::Repo};

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct RelationBoard {
    #[snugom(id)]
    board_id: String,
    name: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    created_at: chrono::DateTime<Utc>,
    #[serde(default)]
    #[snugom(relation(target = "relation_members", cascade = "delete"))]
    board_members: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize)]
#[snugom(version = 1)]
struct RelationMember {
    #[snugom(id)]
    member_id: String,
    user_id: String,
    #[snugom(datetime(epoch_millis), filterable, sortable)]
    joined_at: chrono::DateTime<Utc>,
    #[snugom(relation(target = "relation_boards"))]
    board_id: String,
}

bundle! {
    service: "examples",
    entities: {
        RelationBoard => "relation_boards",
        RelationMember => "relation_members",
    }
}

/// Example 13 – mixed relation mutations (connect, disconnect, delete).
pub async fn run() -> Result<()> {
    let mut conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("relation_mutations");
    let board_repo: Repo<RelationBoard> = Repo::new(prefix.clone());
    let member_repo: Repo<RelationMember> = Repo::new(prefix);

    let board = board_repo
        .create_with_conn(
            &mut conn,
            RelationBoard::validation_builder()
                .name("Strategy Board".to_string())
                .created_at(Utc::now()),
        )
        .await?;
    let board_id = board.id.clone();

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

    let relation_key = board_repo.relation_key("board_members", &board_id);
    board_repo
        .mutate_relations_with_conn(
            &mut conn,
            vec![crate::repository::RelationPlan::with_left(
                "board_members",
                board_id.clone(),
                vec![member_one_id.clone()],
                Vec::new(),
            )],
        )
        .await?;
    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(members, vec![member_one_id.clone()], "member one connected");

    // Mix connect and disconnect in a single patch.
    crate::run! {
        &board_repo,
        &mut conn,
        update => RelationBoard(entity_id = board_id.clone()) {
            board_members: [
                // Connect member two
                connect member_two_id.clone(),
                // Disconnect member one
                disconnect member_one_id.clone(),
            ],
        }
    }?;
    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert_eq!(members, vec![member_two_id.clone()], "member list updated");

    // Demonstrate delete directive: remove the relation edge and delete the member document.
    crate::run! {
        &board_repo,
        &mut conn,
        update => RelationBoard(entity_id = board_id.clone()) {
            board_members: [delete member_two_id.clone()],
        }
    }?;
    let members: Vec<String> = conn.smembers(&relation_key).await?;
    assert!(members.is_empty(), "relation set cleared after delete directive");
    let member_exists: bool = conn.exists(member_repo.entity_key(&member_two_id)).await?;
    assert!(!member_exists, "member document removed due to cascade delete on relation");

    Ok(())
}
