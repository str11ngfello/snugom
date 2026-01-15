//! Social Network Entity Models
//!
//! Defines all entity types for the social network application.

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::SnugomEntity;

/// A user in the social network.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_users")]
pub struct User {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    pub updated_at: chrono::DateTime<Utc>,
    #[serde(default)]
    #[snugom(version)]
    pub version: i64,

    /// Unique username (case-insensitive)
    #[snugom(unique(case_insensitive))]
    #[snugom(filterable(tag))]
    pub username: String,

    /// Unique email (case-insensitive)
    #[snugom(unique(case_insensitive))]
    #[snugom(filterable(tag))]
    pub email: String,

    /// Display name (searchable)
    #[snugom(filterable(text))]
    pub display_name: String,

    /// User bio
    pub bio: Option<String>,

    /// Profile picture URL
    pub avatar_url: Option<String>,

    /// Account verification status
    #[snugom(filterable(tag))]
    pub verified: bool,

    /// Follower count (denormalized for performance)
    #[snugom(filterable, sortable)]
    pub follower_count: i64,

    /// Following count (denormalized)
    #[snugom(filterable, sortable)]
    pub following_count: i64,

    /// Post count (denormalized)
    #[snugom(filterable, sortable)]
    pub post_count: i64,
}

/// A post created by a user.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_posts")]
pub struct Post {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    pub updated_at: chrono::DateTime<Utc>,
    #[serde(default)]
    #[snugom(version)]
    pub version: i64,

    /// The author of this post
    #[snugom(relation(target = "sn_users"))]
    pub author_id: String,

    /// Post content (searchable)
    #[snugom(filterable(text))]
    pub content: String,

    /// Post visibility
    #[snugom(filterable(tag))]
    pub visibility: String, // "public", "followers", "private"

    /// Like count (denormalized)
    #[snugom(filterable, sortable)]
    pub like_count: i64,

    /// Comment count (denormalized)
    #[snugom(filterable, sortable)]
    pub comment_count: i64,

    /// Share/repost count (denormalized)
    #[snugom(filterable, sortable)]
    pub share_count: i64,

    /// Optional media URLs
    #[serde(default)]
    pub media_urls: Vec<String>,
}

/// A comment on a post.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_comments")]
pub struct Comment {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,
    #[snugom(updated_at)]
    pub updated_at: chrono::DateTime<Utc>,

    /// The post this comment belongs to
    #[snugom(relation(target = "sn_posts"))]
    pub post_id: String,

    /// The author of this comment
    #[snugom(relation(target = "sn_users"))]
    pub author_id: String,

    /// Comment content
    #[snugom(filterable(text))]
    pub content: String,

    /// Like count on this comment
    #[snugom(filterable, sortable)]
    pub like_count: i64,

    /// Optional parent comment (for replies)
    pub parent_comment_id: Option<String>,
}

/// A like on a post or comment.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_likes")]
pub struct Like {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,

    /// The user who liked
    #[snugom(relation(target = "sn_users"))]
    pub user_id: String,

    /// Target type: "post" or "comment"
    #[snugom(filterable(tag))]
    pub target_type: String,

    /// The target ID (post or comment)
    #[snugom(filterable(tag))]
    pub target_id: String,
}

/// A follow relationship between users.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_follows")]
pub struct Follow {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,

    /// The follower (user who is following)
    #[snugom(relation(target = "sn_users"))]
    pub follower_id: String,

    /// The followee (user being followed)
    #[snugom(relation(target = "sn_users"))]
    pub following_id: String,
}

/// A notification for a user.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "social", collection = "sn_notifications")]
pub struct Notification {
    #[snugom(id)]
    pub id: String,
    #[snugom(created_at)]
    pub created_at: chrono::DateTime<Utc>,

    /// The user receiving this notification
    #[snugom(relation(target = "sn_users"))]
    pub user_id: String,

    /// Notification type
    #[snugom(filterable(tag))]
    pub notification_type: String, // "like", "comment", "follow", "mention"

    /// The actor who triggered this notification
    #[snugom(relation(target = "sn_users"))]
    pub actor_id: String,

    /// Optional target ID (post, comment, etc.)
    pub target_id: Option<String>,

    /// Preview text
    pub preview_text: String,

    /// Read status
    #[snugom(filterable(tag))]
    pub read: bool,
}
