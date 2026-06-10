//! Example 26 – Find Many with Includes
//!
//! Demonstrates `snugom_find_many!` for list queries with related data:
//! - Basic list (no includes)
//! - List with includes (all authors with posts)
//! - Filter + includes (specific authors with posts)
//! - Pagination with includes
//! - Nested includes in list queries

use anyhow::Result;
use chrono::Utc;

use super::ex19_find_with_includes::{BlogAuthor, BlogClient, BlogComment, BlogPost};
use super::support;
use crate::{snugom_create, snugom_find_many};

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("find_many_inc");
    let mut client = BlogClient::new(conn, prefix);
    client.ensure_indexes().await?;

    let now = Utc::now();

    // ============ Seed data ============
    let post_repo = crate::repository::Repo::<BlogPost>::new(client.prefix().to_string());
    let author_repo = crate::repository::Repo::<BlogAuthor>::new(client.prefix().to_string());

    for (name, post_count) in [("Alice", 3), ("Bob", 1), ("Carol", 2)] {
        let author = snugom_create!(client, BlogAuthor {
            name: name.to_string(),
            created_at: now,
        }).await?;

        for i in 0..post_count {
            let pr = snugom_create!(client, BlogPost {
                title: format!("{name}'s Post {i}"),
                created_at: now,
                author: [connect author.id.clone()],
            }).await?;

            let rel_key = author_repo.relation_key("posts", &author.id);
            let _: () = redis::AsyncCommands::sadd(
                &mut client.connection(), &rel_key, &pr.id,
            ).await?;

            // Add a comment to the first post of each author
            if i == 0 {
                let cr = snugom_create!(client, BlogComment {
                    body: format!("Comment on {name}'s post"),
                    created_at: now,
                    author: [connect author.id.clone()],
                    post: [connect pr.id.clone()],
                }).await?;
                let comments_rel = post_repo.relation_key("comments", &pr.id);
                let _: () = redis::AsyncCommands::sadd(
                    &mut client.connection(), &comments_rel, &cr.id,
                ).await?;
            }
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // ============ Basic list (no includes) ============
    let result = snugom_find_many!(client, BlogAuthor(
        page_size = 10,
    )).await?;

    assert_eq!(result.total, 3);
    assert_eq!(result.items.len(), 3);

    // ============ List with includes ============
    let result = snugom_find_many!(client, BlogAuthor(
        page_size = 10,
    ) {
        posts: [include BlogPost],
    }).await?;

    assert_eq!(result.items.len(), 3);
    let mut counts: Vec<(String, usize)> = result.items.iter()
        .map(|(a, p)| (a.name.clone(), p.len()))
        .collect();
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(counts, vec![
        ("Alice".to_string(), 3),
        ("Bob".to_string(), 1),
        ("Carol".to_string(), 2),
    ]);

    // ============ Filter + includes ============
    let result = snugom_find_many!(client, BlogAuthor(
        filter = "name:eq:Alice",
        page_size = 10,
    ) {
        posts: [include BlogPost],
    }).await?;

    assert_eq!(result.items.len(), 1);
    let (author, posts) = &result.items[0];
    assert_eq!(author.name, "Alice");
    assert_eq!(posts.len(), 3);

    // ============ Pagination with includes ============
    let page1 = snugom_find_many!(client, BlogAuthor(
        page_size = 2,
        page = 1,
    ) {
        posts: [include BlogPost],
    }).await?;

    assert_eq!(page1.items.len(), 2);
    assert_eq!(page1.total, 3);

    let page2 = snugom_find_many!(client, BlogAuthor(
        page_size = 2,
        page = 2,
    ) {
        posts: [include BlogPost],
    }).await?;

    assert_eq!(page2.items.len(), 1);

    // ============ Nested includes in list ============
    let result = snugom_find_many!(client, BlogAuthor(
        page_size = 10,
    ) {
        posts: [include BlogPost {
            comments: [include BlogComment],
        }],
    }).await?;

    assert_eq!(result.items.len(), 3);
    // Each author's first post has one comment
    for (author, posts_with_comments) in &result.items {
        let total_comments: usize = posts_with_comments.iter().map(|(_, c)| c.len()).sum();
        assert_eq!(total_comments, 1, "{} should have 1 comment total", author.name);
    }

    Ok(())
}
