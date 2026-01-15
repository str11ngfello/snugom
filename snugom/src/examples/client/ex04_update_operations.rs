//! Example 04 – Update Operations
//!
//! Demonstrates updating entities using the `snugom_update!` macro DSL:
//! - Partial updates with struct-literal syntax
//! - Bulk updates with `update_many_by_ids()` and `update_many()`
//!   (collection-level operations)

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_update, SearchQuery};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "articles")]
struct Article {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    updated_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    #[snugom(filterable(text))]
    content: String,
    #[snugom(filterable(tag))]
    status: String,
    #[snugom(filterable, sortable)]
    views: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Article])]
struct ArticleClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("update_ops");
    let mut client = ArticleClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search-based update_many
    let mut articles = client.articles();

    // Create test articles using snugom_create! macro
    let article1 = snugom_create!(client, Article {
        title: "Getting Started".to_string(),
        content: "Introduction to the system".to_string(),
        status: "draft".to_string(),
        views: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }).await?;

    let article2 = snugom_create!(client, Article {
        title: "Advanced Topics".to_string(),
        content: "Deep dive into features".to_string(),
        status: "draft".to_string(),
        views: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }).await?;

    let article3 = snugom_create!(client, Article {
        title: "Best Practices".to_string(),
        content: "Tips and tricks".to_string(),
        status: "draft".to_string(),
        views: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }).await?;

    // ============ snugom_update! macro ============
    // Partial update using Prisma-style struct literal syntax
    snugom_update!(client, Article(entity_id = &article1.id) {
        status: "published".to_string(),
        views: 100,
    }).await?;

    let updated = articles.get_or_error(&article1.id).await?;
    assert_eq!(updated.status, "published");
    assert_eq!(updated.views, 100);
    assert_eq!(updated.title, "Getting Started"); // Unchanged

    // ============ Another update ============
    snugom_update!(client, Article(entity_id = &article2.id) {
        status: "published".to_string(),
    }).await?;

    let updated_article = articles.get_or_error(&article2.id).await?;
    assert_eq!(updated_article.status, "published");
    assert_eq!(updated_article.title, "Advanced Topics");

    // ============ update_many_by_ids() ============
    // Update multiple specific entities by their IDs (collection-level API)
    let ids = [article2.id.as_str(), article3.id.as_str()];
    let updated_count = articles
        .update_many_by_ids(&ids, |id| {
            Article::patch_builder()
                .entity_id(id)
                .views(50)
        })
        .await?;

    assert_eq!(updated_count, 2, "should update 2 articles");

    // Verify updates
    let a2 = articles.get_or_error(&article2.id).await?;
    let a3 = articles.get_or_error(&article3.id).await?;
    assert_eq!(a2.views, 50);
    assert_eq!(a3.views, 50);

    // ============ update_many() ============
    // Update all entities matching a search query (collection-level API)
    let query = SearchQuery {
        filter: vec!["status:eq:draft".to_string()],
        ..Default::default()
    };
    let updated_count = articles
        .update_many(query, |id| {
            Article::patch_builder()
                .entity_id(id)
                .status("archived".to_string())
        })
        .await?;

    assert_eq!(updated_count, 1, "should update 1 draft article");

    // Verify article3 was updated (it was still draft)
    let a3 = articles.get_or_error(&article3.id).await?;
    assert_eq!(a3.status, "archived");

    Ok(())
}
