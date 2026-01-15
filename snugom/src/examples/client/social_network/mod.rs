//! Social Network Example Application
//!
//! A comprehensive multi-file example demonstrating how to structure a real
//! application using SnugomClient. This example implements a social network
//! with users, posts, comments, likes, and follows.
//!
//! ## Module Structure
//!
//! - `models` - Entity definitions (User, Post, Comment, Like, Follow)
//! - `client` - SnugomClient definition with all entity types
//! - `workflows/` - Domain workflows demonstrating common patterns:
//!   - `user_registration` - User signup and profile management
//!   - `posting` - Creating and managing posts
//!   - `engagement` - Likes, comments, and interactions
//!   - `social_graph` - Following/unfollowing users
//!   - `feed` - Generating user feeds
//! - `tour` - Comprehensive walkthrough running all workflows
//!
//! ## Usage
//!
//! ```rust,ignore
//! use snugom::examples::client::social_network;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     social_network::tour::run().await
//! }
//! ```

pub mod models;
pub mod client;
pub mod workflows;
pub mod tour;

pub use client::SocialNetworkClient;
pub use models::*;
