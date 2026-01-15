//! Social Network Tour
//!
//! A comprehensive walkthrough that demonstrates all social network workflows.
//! This serves as both documentation and an integration test.

use anyhow::Result;
use redis::Client;

use super::client::SocialNetworkClient;
use super::workflows::{engagement, feed, posting, social_graph, user_registration};
use crate::id::generate_entity_id;

/// Run the complete social network tour.
///
/// This demonstrates:
/// 1. User registration and profile management
/// 2. Creating and managing posts
/// 3. Engagement (likes, comments, notifications)
/// 4. Social graph (following/unfollowing)
/// 5. Feed generation
pub async fn run() -> Result<()> {
    println!("\n╔═══════════════════════════════════════════════════════════╗");
    println!("║           Social Network Example Application              ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // Connect to Redis
    let client = Client::open("redis://127.0.0.1/")?;
    let conn = client.get_connection_manager().await?;

    // Create unique namespace for this run
    let salt = generate_entity_id();
    let prefix = format!("social_demo_{}", &salt[..8]);
    println!("Using namespace: {prefix}\n");

    // Create the social network client
    let mut social_client = SocialNetworkClient::new(conn, prefix);
    social_client.ensure_indexes().await?; // Required for search queries

    // Get collection handles
    let mut users = social_client.users();
    let mut posts = social_client.posts();
    let mut comments = social_client.comments();
    let mut likes = social_client.likes();
    let mut follows = social_client.follows();
    let mut notifications = social_client.notifications();

    // Run all workflows
    println!("1. User Registration & Profile Management");
    println!("─────────────────────────────────────────");
    user_registration::run(&mut users).await?;
    println!();

    println!("2. Content Creation & Management");
    println!("─────────────────────────────────────────");
    posting::run(&mut posts, &mut users).await?;
    println!();

    println!("3. Social Engagement");
    println!("─────────────────────────────────────────");
    engagement::run(&mut posts, &mut comments, &mut likes, &mut notifications, &mut users).await?;
    println!();

    println!("4. Social Graph");
    println!("─────────────────────────────────────────");
    social_graph::run(&mut follows, &mut notifications, &mut users).await?;
    println!();

    println!("5. Feed Generation");
    println!("─────────────────────────────────────────");
    feed::run(&mut posts, &mut follows, &mut users).await?;
    println!();

    // Summary
    println!("═══════════════════════════════════════════════════════════");
    println!("                    Final Statistics");
    println!("═══════════════════════════════════════════════════════════");
    println!("  Users:         {}", users.count().await?);
    println!("  Posts:         {}", posts.count().await?);
    println!("  Comments:      {}", comments.count().await?);
    println!("  Likes:         {}", likes.count().await?);
    println!("  Follows:       {}", follows.count().await?);
    println!("  Notifications: {}", notifications.count().await?);
    println!("═══════════════════════════════════════════════════════════\n");

    println!("✓ Social Network tour completed successfully!\n");

    Ok(())
}
