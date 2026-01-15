//! Engagement Workflow
//!
//! Demonstrates likes, comments, and other engagement features.

use anyhow::Result;
use chrono::Utc;

use crate::{CollectionHandle, SortOrder};
use super::super::models::{Comment, Like, Notification, Post, User};

/// Like a post.
///
/// Uses idempotency key to prevent duplicate likes.
pub async fn like_post(
    likes: &mut CollectionHandle<Like>,
    posts: &mut CollectionHandle<Post>,
    notifications: &mut CollectionHandle<Notification>,
    user_id: &str,
    post_id: &str,
) -> Result<Like> {
    // Idempotency key ensures one like per user per post
    let idempotency_key = format!("like:post:{user_id}:{post_id}");

    let like = likes
        .create_and_get(
            Like::validation_builder()
                .idempotency_key(idempotency_key)
                .user_id(user_id.to_string())
                .target_type("post".to_string())
                .target_id(post_id.to_string())
                .created_at(Utc::now()),
        )
        .await?;

    // Increment post's like count
    let post = posts.get_or_error(post_id).await?;
    posts
        .update(
            Post::patch_builder()
                .entity_id(post_id)
                .like_count(post.like_count + 1),
        )
        .await?;

    // Create notification for post author (if not self-like)
    if post.author_id != user_id {
        notifications
            .create(
                Notification::validation_builder()
                    .user_id(post.author_id)
                    .notification_type("like".to_string())
                    .actor_id(user_id.to_string())
                    .target_id(Some(post_id.to_string()))
                    .preview_text("liked your post".to_string())
                    .read(false)
                    .created_at(Utc::now()),
            )
            .await?;
    }

    Ok(like)
}

/// Unlike a post.
pub async fn unlike_post(
    likes: &mut CollectionHandle<Like>,
    posts: &mut CollectionHandle<Post>,
    user_id: &str,
    post_id: &str,
) -> Result<()> {
    // Find the like
    let like = likes
        .find_first(crate::SearchQuery {
            filter: vec![
                format!("user_id:eq:{user_id}"),
                format!("target_id:eq:{post_id}"),
                "target_type:eq:post".to_string(),
            ],
            ..Default::default()
        })
        .await?;

    if let Some(l) = like {
        likes.delete(&l.id).await?;

        // Decrement post's like count
        let post = posts.get_or_error(post_id).await?;
        posts
            .update(
                Post::patch_builder()
                    .entity_id(post_id)
                    .like_count((post.like_count - 1).max(0)),
            )
            .await?;
    }

    Ok(())
}

/// Check if user has liked a post.
pub async fn has_liked_post(
    likes: &mut CollectionHandle<Like>,
    user_id: &str,
    post_id: &str,
) -> Result<bool> {
    likes
        .exists_where(crate::SearchQuery {
            filter: vec![
                format!("user_id:eq:{user_id}"),
                format!("target_id:eq:{post_id}"),
                "target_type:eq:post".to_string(),
            ],
            ..Default::default()
        })
        .await
        .map_err(|e| e.into())
}

/// Add a comment to a post.
pub async fn add_comment(
    comments: &mut CollectionHandle<Comment>,
    posts: &mut CollectionHandle<Post>,
    notifications: &mut CollectionHandle<Notification>,
    author_id: &str,
    post_id: &str,
    content: String,
    parent_comment_id: Option<String>,
) -> Result<Comment> {
    let comment = comments
        .create_and_get(
            Comment::validation_builder()
                .post_id(post_id.to_string())
                .author_id(author_id.to_string())
                .content(content)
                .like_count(0)
                .parent_comment_id(parent_comment_id)
                .created_at(Utc::now())
                .updated_at(Utc::now()),
        )
        .await?;

    // Increment post's comment count
    let post = posts.get_or_error(post_id).await?;
    posts
        .update(
            Post::patch_builder()
                .entity_id(post_id)
                .comment_count(post.comment_count + 1),
        )
        .await?;

    // Create notification for post author
    if post.author_id != author_id {
        notifications
            .create(
                Notification::validation_builder()
                    .user_id(post.author_id)
                    .notification_type("comment".to_string())
                    .actor_id(author_id.to_string())
                    .target_id(Some(post_id.to_string()))
                    .preview_text("commented on your post".to_string())
                    .read(false)
                    .created_at(Utc::now()),
            )
            .await?;
    }

    Ok(comment)
}

/// Get comments for a post.
pub async fn get_post_comments(
    comments: &mut CollectionHandle<Comment>,
    post_id: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<Comment>> {
    let result = comments
        .find_many(crate::SearchQuery {
            filter: vec![format!("post_id:eq:{post_id}")],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Asc),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Delete a comment.
pub async fn delete_comment(
    comments: &mut CollectionHandle<Comment>,
    posts: &mut CollectionHandle<Post>,
    comment_id: &str,
    author_id: &str,
) -> Result<()> {
    let comment = comments.get_or_error(comment_id).await?;
    if comment.author_id != author_id {
        return Err(anyhow::anyhow!("Not authorized to delete this comment"));
    }

    // Decrement post's comment count
    let post = posts.get_or_error(&comment.post_id).await?;
    posts
        .update(
            Post::patch_builder()
                .entity_id(&comment.post_id)
                .comment_count((post.comment_count - 1).max(0)),
        )
        .await?;

    comments.delete(comment_id).await.map_err(|e| e.into())
}

/// Get unread notifications for a user.
pub async fn get_unread_notifications(
    notifications: &mut CollectionHandle<Notification>,
    user_id: &str,
    limit: u64,
) -> Result<Vec<Notification>> {
    let result = notifications
        .find_many(crate::SearchQuery {
            filter: vec![
                format!("user_id:eq:{user_id}"),
                "read:eq:false".to_string(),
            ],
            page_size: Some(limit),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Mark notifications as read.
pub async fn mark_notifications_read(
    notifications: &mut CollectionHandle<Notification>,
    notification_ids: Vec<String>,
) -> Result<u64> {
    let id_refs: Vec<&str> = notification_ids.iter().map(|s| s.as_str()).collect();
    notifications
        .update_many_by_ids(&id_refs, |id| {
            Notification::patch_builder().entity_id(id).read(true)
        })
        .await
        .map_err(|e| e.into())
}

/// Run engagement workflow demonstration.
pub async fn run(
    posts: &mut CollectionHandle<Post>,
    comments: &mut CollectionHandle<Comment>,
    likes: &mut CollectionHandle<Like>,
    notifications: &mut CollectionHandle<Notification>,
    users: &mut CollectionHandle<User>,
) -> Result<()> {
    println!("  → Engagement Workflow");

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

    let carol = users
        .find_first(crate::SearchQuery {
            filter: vec!["username:eq:carol".to_string()],
            ..Default::default()
        })
        .await?
        .expect("Carol should exist");

    // Get Alice's first post
    let alice_post = posts
        .find_first(crate::SearchQuery {
            filter: vec![format!("author_id:eq:{}", alice.id)],
            ..Default::default()
        })
        .await?
        .expect("Alice should have a post");

    // Bob likes Alice's post
    like_post(likes, posts, notifications, &bob.id, &alice_post.id).await?;
    println!("    Bob liked Alice's post");

    // Carol also likes it
    like_post(likes, posts, notifications, &carol.id, &alice_post.id).await?;
    println!("    Carol liked Alice's post");

    // Check like count
    let updated_post = posts.get_or_error(&alice_post.id).await?;
    println!("    Post now has {} likes", updated_post.like_count);

    // Bob likes again (idempotent - should not double count)
    like_post(likes, posts, notifications, &bob.id, &alice_post.id).await?;
    let still_same = posts.get_or_error(&alice_post.id).await?;
    println!("    After Bob's second like: {} likes (idempotent)", still_same.like_count);

    // Bob comments on the post
    let comment = add_comment(
        comments,
        posts,
        notifications,
        &bob.id,
        &alice_post.id,
        "Great post! 👏".to_string(),
        None,
    )
    .await?;
    println!("    Bob commented: \"{}\"", comment.content);

    // Carol replies to Bob's comment
    let _reply = add_comment(
        comments,
        posts,
        notifications,
        &carol.id,
        &alice_post.id,
        "I agree!".to_string(),
        Some(comment.id.clone()),
    )
    .await?;
    println!("    Carol replied to Bob's comment");

    // Check comment count
    let post_with_comments = posts.get_or_error(&alice_post.id).await?;
    println!("    Post now has {} comments", post_with_comments.comment_count);

    // Get comments
    let post_comments = get_post_comments(comments, &alice_post.id, 1, 10).await?;
    println!("    Retrieved {} comments", post_comments.len());

    // Check Alice's notifications
    let alice_notifications = get_unread_notifications(notifications, &alice.id, 10).await?;
    println!("    Alice has {} unread notifications", alice_notifications.len());

    // Mark notifications as read
    let notif_ids: Vec<String> = alice_notifications.iter().map(|n| n.id.clone()).collect();
    if !notif_ids.is_empty() {
        let marked = mark_notifications_read(notifications, notif_ids).await?;
        println!("    Marked {} notifications as read", marked);
    }

    // Unlike (Bob changes mind)
    unlike_post(likes, posts, &bob.id, &alice_post.id).await?;
    let after_unlike = posts.get_or_error(&alice_post.id).await?;
    println!("    After Bob unliked: {} likes", after_unlike.like_count);

    Ok(())
}
