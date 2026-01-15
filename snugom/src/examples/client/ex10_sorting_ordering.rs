//! Example 10 – Sorting and Ordering
//!
//! Demonstrates how to sort search results:
//! - Sort by different fields
//! - Ascending vs descending order
//! - Sorting with filters

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, SearchQuery, search::SortOrder};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "players")]
struct Player {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(tag))]
    team: String,
    #[snugom(filterable, sortable)]
    score: i64,
    #[snugom(filterable, sortable)]
    games_played: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Player])]
struct PlayerClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("sorting");
    let mut client = PlayerClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut players = client.players();

    // Create test data
    let test_players = vec![
        Player::validation_builder()
            .name("Alice".to_string())
            .team("red".to_string())
            .score(1500)
            .games_played(20)
            .created_at(Utc::now()),
        Player::validation_builder()
            .name("Bob".to_string())
            .team("blue".to_string())
            .score(2200)
            .games_played(35)
            .created_at(Utc::now()),
        Player::validation_builder()
            .name("Carol".to_string())
            .team("red".to_string())
            .score(1800)
            .games_played(25)
            .created_at(Utc::now()),
        Player::validation_builder()
            .name("Dave".to_string())
            .team("blue".to_string())
            .score(900)
            .games_played(10)
            .created_at(Utc::now()),
        Player::validation_builder()
            .name("Eve".to_string())
            .team("red".to_string())
            .score(2500)
            .games_played(40)
            .created_at(Utc::now()),
    ];

    players.create_many(test_players).await?;

    // ============ Sort by Score (Descending) ============
    // Leaderboard: highest score first
    let query = SearchQuery {
        sort_by: Some("score".to_string()),
        sort_order: Some(SortOrder::Desc),
        ..Default::default()
    };
    let leaderboard = players.find_many(query).await?;

    assert_eq!(leaderboard.items.len(), 5);
    assert_eq!(leaderboard.items[0].name, "Eve", "highest score should be first");
    assert_eq!(leaderboard.items[0].score, 2500);
    assert_eq!(leaderboard.items[4].name, "Dave", "lowest score should be last");

    // ============ Sort by Score (Ascending) ============
    // Reverse leaderboard: lowest score first
    let query = SearchQuery {
        sort_by: Some("score".to_string()),
        sort_order: Some(SortOrder::Asc),
        ..Default::default()
    };
    let reverse = players.find_many(query).await?;

    assert_eq!(reverse.items[0].name, "Dave", "lowest score first");
    assert_eq!(reverse.items[4].name, "Eve", "highest score last");

    // ============ Sort by Games Played ============
    let query = SearchQuery {
        sort_by: Some("games_played".to_string()),
        sort_order: Some(SortOrder::Desc),
        ..Default::default()
    };
    let most_active = players.find_many(query).await?;

    assert_eq!(most_active.items[0].name, "Eve", "most games played first");
    assert_eq!(most_active.items[0].games_played, 40);

    // ============ Filter + Sort ============
    // Red team leaderboard
    let query = SearchQuery {
        filter: vec!["team:eq:red".to_string()],
        sort_by: Some("score".to_string()),
        sort_order: Some(SortOrder::Desc),
        ..Default::default()
    };
    let red_leaderboard = players.find_many(query).await?;

    assert_eq!(red_leaderboard.items.len(), 3, "should have 3 red team players");
    assert_eq!(red_leaderboard.items[0].name, "Eve", "Eve leads red team");
    assert_eq!(red_leaderboard.items[1].name, "Carol");
    assert_eq!(red_leaderboard.items[2].name, "Alice");

    // ============ Pagination + Sort ============
    // Top 2 players
    let query = SearchQuery {
        sort_by: Some("score".to_string()),
        sort_order: Some(SortOrder::Desc),
        page: Some(1),
        page_size: Some(2),
        ..Default::default()
    };
    let top2 = players.find_many(query).await?;

    assert_eq!(top2.items.len(), 2);
    assert_eq!(top2.items[0].name, "Eve");
    assert_eq!(top2.items[1].name, "Bob");
    assert!(top2.has_more(), "should have more pages");

    // Next 2 players
    let query = SearchQuery {
        sort_by: Some("score".to_string()),
        sort_order: Some(SortOrder::Desc),
        page: Some(2),
        page_size: Some(2),
        ..Default::default()
    };
    let next2 = players.find_many(query).await?;

    assert_eq!(next2.items.len(), 2);
    assert_eq!(next2.items[0].name, "Carol");
    assert_eq!(next2.items[1].name, "Alice");

    Ok(())
}
