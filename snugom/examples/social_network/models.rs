use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct User {
    #[snugom(id)]
    pub id: String,
    #[snugom(datetime(epoch_millis))]
    pub created_at: DateTime<Utc>,
    pub display_name: String,
    pub bio: Option<String>,
    #[snugom(relation(many_to_many = "users"))]
    pub followers_ids: Vec<String>,
    #[snugom(relation(many_to_many = "users"))]
    pub following_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct Post {
    #[snugom(id)]
    pub id: String,
    #[snugom(datetime(epoch_millis))]
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub body: String,
    #[snugom(relation)]
    pub author_id: String,
    #[snugom(relation(many_to_many = "users"))]
    pub liked_by_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct Comment {
    #[snugom(id)]
    pub id: String,
    #[snugom(datetime(epoch_millis))]
    pub created_at: DateTime<Utc>,
    pub body: String,
    #[snugom(relation)]
    pub author_id: String,
    #[snugom(relation)]
    pub post_id: String,
}
