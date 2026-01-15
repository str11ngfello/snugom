//! Example 16 – Relations
//!
//! Demonstrates defining relationships between entities:
//! - `belongs_to` - Many-to-one relation
//! - `has_many` - One-to-many relation (inverse of belongs_to)
//! - Relation fields and their storage

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create};

/// A blog post that belongs to an author.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "rel_posts")]
struct BlogPost {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    content: String,

    /// Belongs to an author - stores the author's ID.
    /// The target specifies which collection this relates to.
    #[snugom(filterable(tag), relation(target = "rel_authors"))]
    author_id: String,
}

/// An author who can have many posts.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "rel_authors")]
struct Author {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(text))]
    email: String,

    /// Has many posts - stores a list of post IDs.
    /// This is the inverse of the belongs_to relation.
    #[serde(default)]
    #[snugom(relation(target = "rel_posts"))]
    posts: Vec<String>,
}

/// A comment belongs to both a post and an author.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "rel_comments")]
struct Comment {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    content: String,

    /// Belongs to a post
    #[snugom(filterable(tag), relation(target = "rel_posts"))]
    post_id: String,

    /// Belongs to an author (commenter)
    #[snugom(filterable(tag), relation(target = "rel_authors"))]
    author_id: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Author, BlogPost, Comment])]
struct BlogClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("relations");
    let mut client = BlogClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries

    let mut authors = client.authors();
    let mut posts = client.blog_posts();
    let mut comments = client.comments();

    // ============ Create Author ============
    let author_id = snugom_create!(client, Author {
        name: "Jane Doe".to_string(),
        email: "jane@example.com".to_string(),
        created_at: Utc::now(),
    }).await?.id;

    let author = authors.get_or_error(&author_id).await?;

    // ============ Create Posts with Relation ============
    // Posts belong to the author via author_id
    let post1_id = snugom_create!(client, BlogPost {
        title: "Introduction to Rust".to_string(),
        content: "Rust is a systems programming language...".to_string(),
        author_id: author.id.clone(), // Set the relation
        created_at: Utc::now(),
    }).await?.id;

    let post2_id = snugom_create!(client, BlogPost {
        title: "Advanced Rust Patterns".to_string(),
        content: "Let's explore some advanced patterns...".to_string(),
        author_id: author.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    let post1 = posts.get_or_error(&post1_id).await?;
    let post2 = posts.get_or_error(&post2_id).await?;

    // Verify posts have the correct author_id
    assert_eq!(post1.author_id, author.id);
    assert_eq!(post2.author_id, author.id);

    // ============ Create Comments with Multiple Relations ============
    // Comments belong to both a post and an author
    let comment_id = snugom_create!(client, Comment {
        content: "Great article!".to_string(),
        post_id: post1.id.clone(),
        author_id: author.id.clone(),
        created_at: Utc::now(),
    }).await?.id;

    let comment = comments.get_or_error(&comment_id).await?;
    assert_eq!(comment.post_id, post1.id);
    assert_eq!(comment.author_id, author.id);

    // ============ Query by Relation ============
    // Find all posts by this author
    let query = crate::SearchQuery {
        filter: vec![format!("author_id:eq:{}", author.id)],
        ..Default::default()
    };
    let author_posts = posts.find_many(query).await?;
    assert_eq!(author_posts.items.len(), 2, "author should have 2 posts");

    // Find comments on a specific post
    let query = crate::SearchQuery {
        filter: vec![format!("post_id:eq:{}", post1.id)],
        ..Default::default()
    };
    let post_comments = comments.find_many(query).await?;
    assert_eq!(post_comments.items.len(), 1, "post1 should have 1 comment");

    // ============ Relation Consistency ============
    // When you fetch an entity, the relation field contains the related ID
    let fetched_post = posts.get_or_error(&post1.id).await?;
    assert_eq!(fetched_post.author_id, author.id);

    // You can then fetch the related entity if needed
    let post_author = authors.get_or_error(&fetched_post.author_id).await?;
    assert_eq!(post_author.name, "Jane Doe");

    Ok(())
}
