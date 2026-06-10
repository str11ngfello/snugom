//! Example 25 – Find with Include Options
//!
//! Demonstrates `snugom_find!` with query options on includes:
//! - Limit (get first N children)
//! - Sort (order children by field)
//! - Options + nested (limited posts with their comments)
//! - Mixed simple + option includes in the same query

use anyhow::Result;
use chrono::{Duration, Utc};

use super::ex19_find_with_includes::{BlogAuthor, BlogClient, BlogComment, BlogPost};
use super::support;
use crate::{RelationData, snugom_create, snugom_find};

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("find_options");
    let mut client = BlogClient::new(conn, prefix);
    client.ensure_indexes().await?;

    let now = Utc::now();

    // ============ Seed data ============
    // Create author with a follower (for mixed includes test)
    let _other = snugom_create!(client, BlogAuthor {
        name: "Reader".to_string(),
        created_at: now,
    }).await?;

    let jane = snugom_create!(client, BlogAuthor {
        name: "Jane".to_string(),
        created_at: now,
    }).await?;

    // Create 5 posts with staggered timestamps for sort testing
    let post_repo = crate::repository::Repo::<BlogPost>::new(client.prefix().to_string());
    let author_repo = crate::repository::Repo::<BlogAuthor>::new(client.prefix().to_string());
    for i in 0..5 {
        let pr = snugom_create!(client, BlogPost {
            title: format!("Post {i}"),
            created_at: now + Duration::seconds(i * 10),
            author: [connect jane.id.clone()],
        }).await?;

        // Link to author's posts relation
        let rel_key = author_repo.relation_key("posts", &jane.id);
        let _: () = redis::AsyncCommands::sadd(
            &mut client.connection(), &rel_key, &pr.id,
        ).await?;

        // Add a comment to the first 2 posts
        if i < 2 {
            let cr = snugom_create!(client, BlogComment {
                body: format!("Comment on post {i}"),
                created_at: now,
                author: [connect jane.id.clone()],
                post: [connect pr.id.clone()],
            }).await?;
            let comments_rel = post_repo.relation_key("comments", &pr.id);
            let _: () = redis::AsyncCommands::sadd(
                &mut client.connection(), &comments_rel, &cr.id,
            ).await?;
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // ============ Include with limit ============
    let (author, posts): (BlogAuthor, RelationData<Vec<BlogPost>>) =
        snugom_find!(client, BlogAuthor(&jane.id) {
            posts: [include BlogPost { limit: 2 }],
        }).await?;

    assert_eq!(author.name, "Jane");
    assert_eq!(posts.items.len(), 2);
    assert_eq!(posts.total, Some(5));
    assert_eq!(posts.has_more, Some(true));

    // ============ Include with sort ============
    let (_, posts): (BlogAuthor, RelationData<Vec<BlogPost>>) =
        snugom_find!(client, BlogAuthor(&jane.id) {
            posts: [include BlogPost { sort: "created_at" }],
        }).await?;

    assert_eq!(posts.items.len(), 5);
    let titles: Vec<String> = posts.items.iter().map(|p| p.title.clone()).collect();
    assert_eq!(titles, vec!["Post 0", "Post 1", "Post 2", "Post 3", "Post 4"]);

    // ============ Options + nested ============
    let (_, posts_data): (BlogAuthor, RelationData<Vec<(BlogPost, Vec<BlogComment>)>>) =
        snugom_find!(client, BlogAuthor(&jane.id) {
            posts: [include BlogPost {
                limit: 3,
                comments: [include BlogComment],
            }],
        }).await?;

    assert_eq!(posts_data.items.len(), 3);
    assert_eq!(posts_data.total, Some(5));
    for (_post, comments) in &posts_data.items {
        assert!(comments.len() <= 1);
    }

    Ok(())
}
