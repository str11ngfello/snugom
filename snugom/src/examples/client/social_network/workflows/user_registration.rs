//! User Registration Workflow
//!
//! Demonstrates user signup, profile management, and account operations.

use anyhow::Result;
use chrono::Utc;

use crate::CollectionHandle;
use super::super::models::User;

/// Register a new user.
///
/// Handles:
/// - Creating the user with validation
/// - Unique constraint checking for username/email
/// - Returning the created user
pub async fn register_user(
    users: &mut CollectionHandle<User>,
    username: String,
    email: String,
    display_name: String,
) -> Result<User> {
    let user = users
        .create_and_get(
            User::validation_builder()
                .username(username)
                .email(email)
                .display_name(display_name)
                .bio(None::<String>)
                .avatar_url(None::<String>)
                .verified(false)
                .follower_count(0)
                .following_count(0)
                .post_count(0)
                .created_at(Utc::now())
                .updated_at(Utc::now()),
        )
        .await?;

    Ok(user)
}

/// Check if a username is available.
pub async fn is_username_available(
    users: &mut CollectionHandle<User>,
    username: &str,
) -> Result<bool> {
    let exists = users
        .exists_where(crate::SearchQuery {
            filter: vec![format!("username:eq:{username}")],
            ..Default::default()
        })
        .await?;

    Ok(!exists)
}

/// Check if an email is available.
pub async fn is_email_available(
    users: &mut CollectionHandle<User>,
    email: &str,
) -> Result<bool> {
    let exists = users
        .exists_where(crate::SearchQuery {
            filter: vec![format!("email:eq:{email}")],
            ..Default::default()
        })
        .await?;

    Ok(!exists)
}

/// Update user profile.
pub async fn update_profile(
    users: &mut CollectionHandle<User>,
    user_id: &str,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_url: Option<String>,
) -> Result<User> {
    let mut builder = User::patch_builder().entity_id(user_id);

    if let Some(name) = display_name {
        builder = builder.display_name(name);
    }
    if let Some(b) = bio {
        builder = builder.bio(Some(b));
    }
    if let Some(url) = avatar_url {
        builder = builder.avatar_url(Some(url));
    }

    builder = builder.updated_at(Utc::now());

    users.update(builder).await?;
    users.get_or_error(user_id).await.map_err(|e| e.into())
}

/// Verify a user account.
pub async fn verify_user(
    users: &mut CollectionHandle<User>,
    user_id: &str,
) -> Result<User> {
    users
        .update(
            User::patch_builder()
                .entity_id(user_id)
                .verified(true)
                .updated_at(Utc::now()),
        )
        .await?;

    users.get_or_error(user_id).await.map_err(|e| e.into())
}

/// Find user by username.
pub async fn find_by_username(
    users: &mut CollectionHandle<User>,
    username: &str,
) -> Result<Option<User>> {
    users
        .find_first(crate::SearchQuery {
            filter: vec![format!("username:eq:{username}")],
            ..Default::default()
        })
        .await
        .map_err(|e| e.into())
}

/// Search users by display name (fuzzy search).
pub async fn search_users(
    users: &mut CollectionHandle<User>,
    query: &str,
    limit: u64,
) -> Result<Vec<User>> {
    let result = users
        .find_many(crate::SearchQuery {
            q: Some(query.to_string()),
            page_size: Some(limit),
            ..Default::default()
        })
        .await?;

    Ok(result.items)
}

/// Delete user account.
///
/// Note: In a real application, you'd want to cascade delete
/// or anonymize related data (posts, comments, etc.)
pub async fn delete_account(
    users: &mut CollectionHandle<User>,
    user_id: &str,
) -> Result<()> {
    users.delete(user_id).await.map_err(|e| e.into())
}

/// Run user registration workflow demonstration.
pub async fn run(users: &mut CollectionHandle<User>) -> Result<()> {
    println!("  → User Registration Workflow");

    // Check username availability
    let available = is_username_available(users, "alice").await?;
    println!("    Username 'alice' available: {available}");

    // Register a user
    let alice = register_user(
        users,
        "alice".to_string(),
        "alice@example.com".to_string(),
        "Alice Johnson".to_string(),
    )
    .await?;
    println!("    Registered user: {} (@{})", alice.display_name, alice.username);

    // Try to register duplicate - should fail
    let duplicate = register_user(
        users,
        "alice".to_string(),
        "alice2@example.com".to_string(),
        "Another Alice".to_string(),
    )
    .await;

    match duplicate {
        Err(e) if e.to_string().contains("Unique") => {
            println!("    Duplicate username rejected ✓");
        }
        _ => println!("    Warning: duplicate username not rejected"),
    }

    // Update profile
    let updated = update_profile(
        users,
        &alice.id,
        Some("Alice J.".to_string()),
        Some("Software engineer and cat lover".to_string()),
        None,
    )
    .await?;
    println!("    Updated display name to: {}", updated.display_name);

    // Verify user
    let verified = verify_user(users, &alice.id).await?;
    println!("    User verified: {}", verified.verified);

    // Search users
    let results = search_users(users, "Alice", 10).await?;
    println!("    Search for 'Alice' found {} users", results.len());

    // Register more users for later workflows
    register_user(
        users,
        "bob".to_string(),
        "bob@example.com".to_string(),
        "Bob Smith".to_string(),
    )
    .await?;

    register_user(
        users,
        "carol".to_string(),
        "carol@example.com".to_string(),
        "Carol White".to_string(),
    )
    .await?;

    println!("    Total users: {}", users.count().await?);

    Ok(())
}
