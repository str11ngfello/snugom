//! Example 24 – Find with Includes
//!
//! Demonstrates `snugom_find!` for reading entities with related data:
//! - Simple find (no includes)
//! - One include (author → posts)
//! - Multiple includes in one query
//! - Empty relation (no children)
//! - Nested includes (author → posts → comments)

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_find};

// ── Blog domain entities (reused in ex25, ex26) ──

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "blog_authors")]
pub struct BlogAuthor {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    pub name: String,
    #[serde(default)]
    #[snugom(relation(target = "blog_posts", cascade = "delete"))]
    pub posts: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "blog_posts")]
pub struct BlogPost {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    pub title: String,
    #[snugom(filterable(tag), relation(target = "blog_authors"))]
    pub author_id: String,
    #[serde(default)]
    #[snugom(relation(target = "blog_comments", cascade = "delete"))]
    pub comments: Vec<String>,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, collection = "blog_comments")]
pub struct BlogComment {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    pub body: String,
    #[snugom(filterable(tag), relation(target = "blog_posts"))]
    pub post_id: String,
    #[snugom(filterable(tag), relation(target = "blog_authors"))]
    pub author_id: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [BlogAuthor, BlogPost, BlogComment])]
pub struct BlogClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("find_includes");
    let mut client = BlogClient::new(conn, prefix);
    client.ensure_indexes().await?;

    // ============ Seed data ============
    let jane = snugom_create!(client, BlogAuthor {
        name: "Jane".to_string(),
        created_at: Utc::now(),
        posts: [
            create BlogPost {
                title: "Intro to Rust".to_string(),
                created_at: Utc::now(),
            },
            create BlogPost {
                title: "Advanced Patterns".to_string(),
                created_at: Utc::now(),
            }
        ],
    }).await?;

    // Add a comment to the first post
    let author_repo = crate::repository::Repo::<BlogAuthor>::new(client.prefix().to_string());
    let rel_key = author_repo.relation_key("posts", &jane.id);
    let post_ids: Vec<String> = redis::AsyncCommands::smembers(
        &mut client.connection(), &rel_key,
    ).await?;
    let first_post_id = &post_ids[0];

    let comment = snugom_create!(client, BlogComment {
        body: "Great article!".to_string(),
        created_at: Utc::now(),
        author: [connect jane.id.clone()],
        post: [connect first_post_id.clone()],
    }).await?;

    // Link comment to post's comments relation
    let post_repo = crate::repository::Repo::<BlogPost>::new(client.prefix().to_string());
    let comments_rel = post_repo.relation_key("comments", first_post_id);
    let _: () = redis::AsyncCommands::sadd(
        &mut client.connection(), &comments_rel, &comment.id,
    ).await?;

    // ============ Simple find (no includes) ============
    let author: BlogAuthor = snugom_find!(client, BlogAuthor(&jane.id)).await?;
    assert_eq!(author.name, "Jane");

    // ============ One include ============
    let (author, posts): (BlogAuthor, Vec<BlogPost>) =
        snugom_find!(client, BlogAuthor(&jane.id) {
            posts: [include BlogPost],
        }).await?;

    assert_eq!(author.name, "Jane");
    assert_eq!(posts.len(), 2);

    // ============ Empty relation ============
    // Create an author with no posts
    let lonely = snugom_create!(client, BlogAuthor {
        name: "Lonely".to_string(),
        created_at: Utc::now(),
    }).await?;

    let (author, posts): (BlogAuthor, Vec<BlogPost>) =
        snugom_find!(client, BlogAuthor(&lonely.id) {
            posts: [include BlogPost],
        }).await?;

    assert_eq!(author.name, "Lonely");
    assert!(posts.is_empty());

    // ============ Nested includes (author → posts → comments) ============
    let (author, posts_with_comments): (BlogAuthor, Vec<(BlogPost, Vec<BlogComment>)>) =
        snugom_find!(client, BlogAuthor(&jane.id) {
            posts: [include BlogPost {
                comments: [include BlogComment],
            }],
        }).await?;

    assert_eq!(author.name, "Jane");
    assert_eq!(posts_with_comments.len(), 2);

    // One post has a comment, the other doesn't
    let total_comments: usize = posts_with_comments.iter().map(|(_, c)| c.len()).sum();
    assert_eq!(total_comments, 1);

    Ok(())
}
