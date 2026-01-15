//! Social Graph Workflow
//!
//! Demonstrates following/unfollowing users and managing the social graph.

use anyhow::Result;
use chrono::Utc;

use crate::{CollectionHandle, SortOrder};
use super::super::models::{Follow, Notification, User};

/// Follow a user.
///
/// Uses idempotency key to prevent duplicate follows.
pub async fn follow_user(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    notifications: &mut CollectionHandle<Notification>,
    follower_id: &str,
    following_id: &str,
) -> Result<Follow> {
    if follower_id == following_id {
        return Err(anyhow::anyhow!("Cannot follow yourself"));
    }

    // Idempotency key ensures one follow relationship
    let idempotency_key = format!("follow:{follower_id}:{following_id}");

    let follow = follows
        .create_and_get(
            Follow::validation_builder()
                .idempotency_key(idempotency_key)
                .follower_id(follower_id.to_string())
                .following_id(following_id.to_string())
                .created_at(Utc::now()),
        )
        .await?;

    // Update follower's following_count
    let follower = users.get_or_error(follower_id).await?;
    users
        .update(
            User::patch_builder()
                .entity_id(follower_id)
                .following_count(follower.following_count + 1)
                .updated_at(Utc::now()),
        )
        .await?;

    // Update followee's follower_count
    let followee = users.get_or_error(following_id).await?;
    users
        .update(
            User::patch_builder()
                .entity_id(following_id)
                .follower_count(followee.follower_count + 1)
                .updated_at(Utc::now()),
        )
        .await?;

    // Notify the followee
    notifications
        .create(
            Notification::validation_builder()
                .user_id(following_id.to_string())
                .notification_type("follow".to_string())
                .actor_id(follower_id.to_string())
                .target_id(None::<String>)
                .preview_text("started following you".to_string())
                .read(false)
                .created_at(Utc::now()),
        )
        .await?;

    Ok(follow)
}

/// Unfollow a user.
pub async fn unfollow_user(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    follower_id: &str,
    following_id: &str,
) -> Result<()> {
    // Find the follow relationship
    let follow = follows
        .find_first(crate::SearchQuery {
            filter: vec![
                format!("follower_id:eq:{follower_id}"),
                format!("following_id:eq:{following_id}"),
            ],
            ..Default::default()
        })
        .await?;

    if let Some(f) = follow {
        follows.delete(&f.id).await?;

        // Update follower's following_count
        let follower = users.get_or_error(follower_id).await?;
        users
            .update(
                User::patch_builder()
                    .entity_id(follower_id)
                    .following_count((follower.following_count - 1).max(0))
                    .updated_at(Utc::now()),
            )
            .await?;

        // Update followee's follower_count
        let followee = users.get_or_error(following_id).await?;
        users
            .update(
                User::patch_builder()
                    .entity_id(following_id)
                    .follower_count((followee.follower_count - 1).max(0))
                    .updated_at(Utc::now()),
            )
            .await?;
    }

    Ok(())
}

/// Check if user A is following user B.
pub async fn is_following(
    follows: &mut CollectionHandle<Follow>,
    follower_id: &str,
    following_id: &str,
) -> Result<bool> {
    follows
        .exists_where(crate::SearchQuery {
            filter: vec![
                format!("follower_id:eq:{follower_id}"),
                format!("following_id:eq:{following_id}"),
            ],
            ..Default::default()
        })
        .await
        .map_err(|e| e.into())
}

/// Get users that a user is following.
pub async fn get_following(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    user_id: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<User>> {
    let follow_records = follows
        .find_many(crate::SearchQuery {
            filter: vec![format!("follower_id:eq:{user_id}")],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    let mut following_users = Vec::new();
    for f in follow_records.items {
        if let Some(user) = users.get(&f.following_id).await? {
            following_users.push(user);
        }
    }

    Ok(following_users)
}

/// Get users that follow a user.
pub async fn get_followers(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    user_id: &str,
    page: u64,
    page_size: u64,
) -> Result<Vec<User>> {
    let follow_records = follows
        .find_many(crate::SearchQuery {
            filter: vec![format!("following_id:eq:{user_id}")],
            page: Some(page),
            page_size: Some(page_size),
            sort_by: Some("created_at".to_string()),
            sort_order: Some(SortOrder::Desc),
            ..Default::default()
        })
        .await?;

    let mut follower_users = Vec::new();
    for f in follow_records.items {
        if let Some(user) = users.get(&f.follower_id).await? {
            follower_users.push(user);
        }
    }

    Ok(follower_users)
}

/// Get mutual follows (users who follow each other).
pub async fn get_mutual_follows(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    user_id: &str,
) -> Result<Vec<User>> {
    // Get users that this user follows
    let following = follows
        .find_many(crate::SearchQuery {
            filter: vec![format!("follower_id:eq:{user_id}")],
            ..Default::default()
        })
        .await?;

    let mut mutuals = Vec::new();
    for f in following.items {
        // Check if they follow back
        let follows_back = is_following(follows, &f.following_id, user_id).await?;
        if follows_back && let Some(user) = users.get(&f.following_id).await? {
            mutuals.push(user);
        }
    }

    Ok(mutuals)
}

/// Get follow suggestions (popular users the user doesn't follow).
pub async fn get_follow_suggestions(
    follows: &mut CollectionHandle<Follow>,
    users: &mut CollectionHandle<User>,
    user_id: &str,
    limit: u64,
) -> Result<Vec<User>> {
    // Get users the current user is already following
    let following = follows
        .find_many(crate::SearchQuery {
            filter: vec![format!("follower_id:eq:{user_id}")],
            ..Default::default()
        })
        .await?;

    let following_ids: Vec<String> = following.items.iter().map(|f| f.following_id.clone()).collect();

    // Get popular users sorted by follower count
    let popular = users
        .find_many(crate::SearchQuery {
            sort_by: Some("follower_count".to_string()),
            sort_order: Some(SortOrder::Desc),
            page_size: Some(limit + following_ids.len() as u64 + 1),
            ..Default::default()
        })
        .await?;

    // Filter out users already being followed and self
    let suggestions: Vec<User> = popular
        .items
        .into_iter()
        .filter(|u| u.id != user_id && !following_ids.contains(&u.id))
        .take(limit as usize)
        .collect();

    Ok(suggestions)
}

/// Run social graph workflow demonstration.
pub async fn run(
    follows: &mut CollectionHandle<Follow>,
    notifications: &mut CollectionHandle<Notification>,
    users: &mut CollectionHandle<User>,
) -> Result<()> {
    println!("  → Social Graph Workflow");

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

    // Bob follows Alice
    follow_user(follows, users, notifications, &bob.id, &alice.id).await?;
    println!("    Bob followed Alice");

    // Carol follows Alice
    follow_user(follows, users, notifications, &carol.id, &alice.id).await?;
    println!("    Carol followed Alice");

    // Alice follows Bob back (mutual follow)
    follow_user(follows, users, notifications, &alice.id, &bob.id).await?;
    println!("    Alice followed Bob back");

    // Check counts
    let updated_alice = users.get_or_error(&alice.id).await?;
    println!("    Alice: {} followers, {} following",
        updated_alice.follower_count, updated_alice.following_count);

    let updated_bob = users.get_or_error(&bob.id).await?;
    println!("    Bob: {} followers, {} following",
        updated_bob.follower_count, updated_bob.following_count);

    // Check if Bob is following Alice
    let bob_follows_alice = is_following(follows, &bob.id, &alice.id).await?;
    println!("    Bob follows Alice: {bob_follows_alice}");

    // Get Alice's followers
    let alice_followers = get_followers(follows, users, &alice.id, 1, 10).await?;
    println!("    Alice's followers: {:?}",
        alice_followers.iter().map(|u| &u.username).collect::<Vec<_>>());

    // Get Bob's mutual follows
    let bob_mutuals = get_mutual_follows(follows, users, &bob.id).await?;
    println!("    Bob's mutual follows: {:?}",
        bob_mutuals.iter().map(|u| &u.username).collect::<Vec<_>>());

    // Get suggestions for Carol
    let suggestions = get_follow_suggestions(follows, users, &carol.id, 5).await?;
    println!("    Suggestions for Carol: {:?}",
        suggestions.iter().map(|u| &u.username).collect::<Vec<_>>());

    // Carol unfollows Alice
    unfollow_user(follows, users, &carol.id, &alice.id).await?;
    let alice_after = users.get_or_error(&alice.id).await?;
    println!("    After Carol unfollowed: Alice has {} followers", alice_after.follower_count);

    Ok(())
}
