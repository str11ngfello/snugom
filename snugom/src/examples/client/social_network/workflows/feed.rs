//! Feed Workflow
//!
//! Demonstrates generating personalized feeds for users.

use anyhow::Result;

use crate::{CollectionHandle, SortOrder};
use super::super::models::{Follow, Post, User};

/// Feed item with additional context.
#[derive(Debug)]
pub struct FeedItem {
    pub post: Post,
    pub author: User,
    pub is_liked: bool,
}

/// Get a user's home feed (posts from people they follow).
///
/// This is a "pull" model where we fetch posts on demand.
/// For a production system, you might use a "push" model with
/// pre-computed feeds stored in Redis.
pub async fn get_home_feed(
    posts: &mut CollectionHandle<Post>,
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    user_id: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<FeedItem>> {
    // Get users this person follows
    let following = follows
        .find_many(crate::SearchQuery {
            filter: vec![format!("follower_id:eq:{user_id}")],
            ..Default::default()
        })
        .await?;

    if following.items.is_empty() {
        return Ok(Vec::new());
    }

    // Build a filter for posts from followed users
    // Note: In a real system, you'd use a more efficient query
    let following_ids: Vec<String> = following.items.iter()
        .map(|f| f.following_id.clone())
        .collect();

    let mut feed_items = Vec::new();

    for author_id in &following_ids {
        let author_posts = posts
            .find_many(crate::SearchQuery {
                filter: vec![
                    format!("author_id:eq:{author_id}"),
                    "visibility:eq:public".to_string(),
                ],
                page: Some(page),
                page_size: Some(page_size),
                sort_by: Some("created_at".to_string()),
                sort_order: Some(SortOrder::Desc),
                ..Default::default()
            })
            .await?;

        for post in author_posts.items {
            if let Some(author) = users.get(&post.author_id).await? {
                feed_items.push(FeedItem {
                    post,
                    author,
                    is_liked: false, // Would check likes in production
                });
            }
        }
    }

    // Sort by created_at (newest first)
    feed_items.sort_by(|a, b| b.post.created_at.cmp(&a.post.created_at));

    // Apply pagination
    let start = ((page - 1) * page_size) as usize;
    let end = (start + page_size as usize).min(feed_items.len());

    Ok(feed_items.into_iter().skip(start).take(end - start).collect())
}

/// Get the explore feed (trending/popular posts).
pub async fn get_explore_feed(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
    page: u64,
    page_size: u64,
) -> Result<Vec<FeedItem>> {
    let trending = posts
        .find_many(crate::SearchQuery {
            filter: vec!["visibility:eq:public".to_string()],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("like_count".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    let mut feed_items = Vec::new();
    for post in trending.items {
        if let Some(author) = users.get(&post.author_id).await? {
            feed_items.push(FeedItem {
                post,
                author,
                is_liked: false,
            });
        }
    }

    Ok(feed_items)
}

/// Get a user's profile feed (their posts).
pub async fn get_profile_feed(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
    profile_user_id: &str,
    viewer_id: Option<&str>,
    page: u64,
    page_size: u64,
) -> Result<Vec<FeedItem>> {
    // Determine which posts are visible
    let mut filters = vec![format!("author_id:eq:{profile_user_id}")];

    // If viewing own profile, show all. Otherwise, show based on follow status
    if viewer_id != Some(profile_user_id) {
        filters.push("visibility:eq:public".to_string());
    }

    let user_posts = posts
        .find_many(crate::SearchQuery {
            filter: filters,
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    let author = users.get_or_error(profile_user_id).await?;

    let feed_items: Vec<FeedItem> = user_posts
        .items
        .into_iter()
        .map(|post| FeedItem {
            post,
            author: author.clone(),
            is_liked: false,
        })
        .collect();

    Ok(feed_items)
}

/// Search feed (posts matching a query).
pub async fn search_feed(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
    query: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<FeedItem>> {
    let results = posts
        .find_many(crate::SearchQuery {
            q: Some(query.to_string()),
            filter: vec!["visibility:eq:public".to_string()],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("like_count".to_string()),
            sort_order: Some(SortOrder::Desc),
        })
        .await?;

    let mut feed_items = Vec::new();
    for post in results.items {
        if let Some(author) = users.get(&post.author_id).await? {
            feed_items.push(FeedItem {
                post,
                author,
                is_liked: false,
            });
        }
    }

    Ok(feed_items)
}

/// Get feed statistics.
#[derive(Debug)]
pub struct FeedStats {
    pub total_posts: u64,
    pub public_posts: u64,
    pub total_engagement: i64,
}

pub async fn get_feed_stats(posts: &mut CollectionHandle<Post>) -> Result<FeedStats> {
    let total_posts = posts.count().await?;

    let public_posts = posts
        .count_where(crate::SearchQuery {
            filter: vec!["visibility:eq:public".to_string()],
            ..Default::default()
        })
        .await?;

    // Get top posts to estimate engagement
    let top_posts = posts
        .find_many(crate::SearchQuery {
            sort_by: Some("like_count".to_string()),
            sort_order: Some(SortOrder::Desc),
            page_size: Some(100),
            ..Default::default()
        })
        .await?;

    let total_engagement: i64 = top_posts
        .items
        .iter()
        .map(|p| p.like_count + p.comment_count + p.share_count)
        .sum();

    Ok(FeedStats {
        total_posts,
        public_posts,
        total_engagement,
    })
}

/// Run feed workflow demonstration.
pub async fn run(
    posts: &mut CollectionHandle<Post>,
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
) -> Result<()> {
    println!("  → Feed Workflow");

    // Get users
    let alice = users
        .find_first(crate::SearchQuery {
            filter: vec!["username:eq:alice".to_string()],
            ..Default::default()
        })
        .await?
        .expect("Alice should exist");

    let bob = users
        .find_first(crate::SearchQuery {
            filter: vec!["username:eq:bob".to_string()],
            ..Default::default()
        })
        .await?
        .expect("Bob should exist");

    // Get Bob's home feed (posts from people Bob follows)
    let home_feed = get_home_feed(posts, follows, users, &bob.id, 1, 10).await?;
    println!("    Bob's home feed: {} posts", home_feed.len());

    for item in home_feed.iter().take(3) {
        println!("      - \"{}\" by @{}",
            &item.post.content[..30.min(item.post.content.len())],
            item.author.username);
    }

    // Get explore feed (trending posts)
    let explore = get_explore_feed(posts, users, 1, 10).await?;
    println!("    Explore feed: {} posts", explore.len());

    // Get Alice's profile feed
    let profile_feed = get_profile_feed(posts, users, &alice.id, Some(&bob.id), 1, 10).await?;
    println!("    Alice's profile: {} public posts visible to Bob", profile_feed.len());

    // Search posts
    let search_results = search_feed(posts, users, "Rust", 1, 10).await?;
    println!("    Search for 'Rust': {} results", search_results.len());

    // Get feed statistics
    let stats = get_feed_stats(posts).await?;
    println!("    Feed stats:");
    println!("      Total posts: {}", stats.total_posts);
    println!("      Public posts: {}", stats.public_posts);
    println!("      Total engagement: {}", stats.total_engagement);

    Ok(())
}
