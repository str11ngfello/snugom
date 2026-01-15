//! Social Network Client
//!
//! Defines the SnugomClient that manages all social network entities.

use redis::aio::ConnectionManager;

use crate::SnugomClient;
use super::models::{Comment, Follow, Like, Notification, Post, User};

/// The main client for the social network application.
///
/// Provides typed accessors for all entity collections:
/// - `users()` - User profiles
/// - `posts()` - User posts
/// - `comments()` - Comments on posts
/// - `likes()` - Likes on posts and comments
/// - `follows()` - Follow relationships
/// - `notifications()` - User notifications
///
/// # Example
///
/// ```rust,ignore
/// let client = SocialNetworkClient::new(conn, "social_prod".to_string());
///
/// // Get typed collection handles
/// let mut users = client.users();
/// let mut posts = client.posts();
///
/// // All operations are type-safe
/// let user = users.create_and_get(User::validation_builder()...).await?;
/// ```
#[derive(SnugomClient)]
#[snugom_client(entities = [User, Post, Comment, Like, Follow, Notification])]
pub struct SocialNetworkClient {
    conn: ConnectionManager,
    prefix: String,
}
