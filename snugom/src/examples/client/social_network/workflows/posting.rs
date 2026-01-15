//! Posting Workflow
//!
//! Demonstrates creating, editing, and managing posts.

use anyhow::Result;
use chrono::Utc;

use crate::{CollectionHandle, SortOrder};
use super::super::models::{Post, User};

/// Create a new post.
pub async fn create_post(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
    author_id: &str,
    content: String,
    visibility: String,
    media_urls: Vec<String>,
) -> Result<Post> {
    // Create the post
    let post = posts
        .create_and_get(
            Post::validation_builder()
                .author_id(author_id.to_string())
                .content(content)
                .visibility(visibility)
                .like_count(0)
                .comment_count(0)
                .share_count(0)
                .media_urls(media_urls)
                .created_at(Utc::now())
                .updated_at(Utc::now()),
        )
        .await?;

    // Increment user's post count
    let user = users.get_or_error(author_id).await?;
    users
        .update(
            User::patch_builder()
                .entity_id(author_id)
                .post_count(user.post_count + 1)
                .updated_at(Utc::now()),
        )
        .await?;

    Ok(post)
}

/// Edit a post's content.
pub async fn edit_post(
    posts: &mut CollectionHandle<Post>,
    post_id: &str,
    author_id: &str,
    new_content: String,
) -> Result<Post> {
    // Verify ownership
    let post = posts.get_or_error(post_id).await?;
    if post.author_id != author_id {
        return Err(anyhow::anyhow!("Not authorized to edit this post"));
    }

    posts
        .update(
            Post::patch_builder()
                .entity_id(post_id)
                .content(new_content)
                .updated_at(Utc::now()),
        )
        .await?;

    posts.get_or_error(post_id).await.map_err(|e| e.into())
}

/// Change post visibility.
pub async fn change_visibility(
    posts: &mut CollectionHandle<Post>,
    post_id: &str,
    visibility: String,
) -> Result<()> {
    posts
        .update(
            Post::patch_builder()
                .entity_id(post_id)
                .visibility(visibility)
                .updated_at(Utc::now()),
        )
        .await?;
    Ok(())
}

/// Delete a post.
pub async fn delete_post(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
    post_id: &str,
    author_id: &str,
) -> Result<()> {
    // Verify ownership
    let post = posts.get_or_error(post_id).await?;
    if post.author_id != author_id {
        return Err(anyhow::anyhow!("Not authorized to delete this post"));
    }

    // Delete the post
    posts.delete(post_id).await?;

    // Decrement user's post count
    let user = users.get_or_error(author_id).await?;
    users
        .update(
            User::patch_builder()
                .entity_id(author_id)
                .post_count((user.post_count - 1).max(0))
                .updated_at(Utc::now()),
        )
        .await?;

    Ok(())
}

/// Get a user's posts.
pub async fn get_user_posts(
    posts: &mut CollectionHandle<Post>,
    author_id: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<Post>> {
    let result = posts
        .find_many(crate::SearchQuery {
            filter: vec![format!("author_id:eq:{author_id}")],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Search posts by content.
pub async fn search_posts(
    posts: &mut CollectionHandle<Post>,
    query: &str,
    limit: u64,
) -> Result<Vec<Post>> {
    let result = posts
        .find_many(crate::SearchQuery {
            q: Some(query.to_string()),
            filter: vec!["visibility:eq:public".to_string()],
            page_size: Some(limit),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Get trending posts (most liked).
pub async fn get_trending_posts(
    posts: &mut CollectionHandle<Post>,
    limit: u64,
) -> Result<Vec<Post>> {
    let result = posts
        .find_many(crate::SearchQuery {
            filter: vec!["visibility:eq:public".to_string()],
            page_size: Some(limit),
            sort_by: Some("like_count".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Run posting workflow demonstration.
pub async fn run(
    posts: &mut CollectionHandle<Post>,
    users: &mut CollectionHandle<User>,
) -> Result<()> {
    println!("  → Posting Workflow");

    // Get Alice (created in user registration)
    let alice = users
        .find_first(crate::SearchQuery {
            filter: vec!["username:eq:alice".to_string()],
            ..Default::default()
        })
        .await?
        .expect("Alice should exist");

    // Create a post
    let post1 = create_post(
        posts,
        users,
        &alice.id,
        "Hello world! This is my first post on the platform. #excited".to_string(),
        "public".to_string(),
        vec![],
    )
    .await?;
    println!("    Created post: {}", &post1.content[..40.min(post1.content.len())]);

    // Create another post with media
    let post2 = create_post(
        posts,
        users,
        &alice.id,
        "Check out this amazing sunset! 🌅".to_string(),
        "public".to_string(),
        vec!["https://example.com/sunset.jpg".to_string()],
    )
    .await?;
    println!("    Created post with media: {} attachments", post2.media_urls.len());

    // Create a private post
    let _private_post = create_post(
        posts,
        users,
        &alice.id,
        "This is a private thought...".to_string(),
        "private".to_string(),
        vec![],
    )
    .await?;
    println!("    Created private post");

    // Edit a post
    let edited = edit_post(posts, &post1.id, &alice.id, "Hello world! Updated content.".to_string())
        .await?;
    println!("    Edited post: {}", &edited.content[..30.min(edited.content.len())]);

    // Get user's posts
    let alice_posts = get_user_posts(posts, &alice.id, 1, 10).await?;
    println!("    Alice has {} posts", alice_posts.len());

    // Verify post count was updated
    let updated_alice = users.get_or_error(&alice.id).await?;
    println!("    Alice's post_count: {}", updated_alice.post_count);

    // Search posts
    let results = search_posts(posts, "Hello", 10).await?;
    println!("    Search for 'Hello' found {} posts", results.len());

    // Get Bob and Carol to create more posts for later
    let bob = users
        .find_first(crate::SearchQuery {
            filter: vec!["username:eq:bob".to_string()],
            ..Default::default()
        })
        .await?
        .expect("Bob should exist");

    create_post(
        posts,
        users,
        &bob.id,
        "Just learned about Rust today. It's amazing!".to_string(),
        "public".to_string(),
        vec![],
    )
    .await?;

    println!("    Total public posts: {}",
        posts.count_where(crate::SearchQuery {
            filter: vec!["visibility:eq:public".to_string()],
            ..Default::default()
        }).await?
    );

    Ok(())
}
