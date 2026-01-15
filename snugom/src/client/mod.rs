//! Client module for Prisma-style ergonomic API.
//!
//! This module provides:
//! - `Client` - Main entry point for database operations
//! - `CollectionHandle<T>` - Type-safe accessor for CRUD operations
//! - `EntityRegistration` - Auto-registration of entities via inventory
//! - `BulkCreateResult` - Result type for bulk create operations
//!
//! # Example
//! ```ignore
//! // Create a client
//! let client = Client::new(conn, "myapp".to_string());
//!
//! // Simple CRUD via generic collection accessor
//! let guild = client.collection::<Guild>().get(&id).await?;
//! let guild = client.collection::<Guild>().create(builder).await?;
//!
//! // Or use the derive macro for named accessors:
//! #[derive(SnugomClient)]
//! #[snugom_client(entities = [Guild, GuildMember])]
//! pub struct Snugom {
//!     #[snugom_client(connection)]
//!     conn: ConnectionManager,
//!     #[snugom_client(prefix)]
//!     prefix: String,
//! }
//!
//! let snugom = Snugom::new(conn, "myapp".to_string());
//! let guild = snugom.guilds().get(&id).await?;  // Named accessor!
//! ```

mod collection;
mod registration;

pub use collection::{BulkCreateResult, CollectionHandle};
pub use registration::{
    EntityRegistration, get_entity_by_collection, get_entity_by_name, is_entity_registered,
    registered_entities,
};

use redis::aio::ConnectionManager;

use crate::{repository::Repo, types::SnugomModel};

/// Main client for Prisma-style database operations.
///
/// This struct provides a generic `collection<T>()` method that works with any
/// entity type that implements `SnugomModel`. For named accessors like `guilds()`,
/// use the `#[derive(SnugomClient)]` macro.
///
/// # Example
/// ```ignore
/// let client = Client::new(conn, "myapp".to_string());
///
/// // Generic collection access
/// let mut guilds = client.collection::<Guild>();
/// let guild = guilds.get(&id).await?;
/// let guild = guilds.create(builder).await?;
/// ```
#[derive(Clone)]
pub struct Client {
    conn: ConnectionManager,
    prefix: String,
}

impl Client {
    /// Create a new client with the given connection and key prefix.
    pub fn new(conn: ConnectionManager, prefix: String) -> Self {
        Self { conn, prefix }
    }

    /// Create a client from an existing Redis connection URL.
    ///
    /// # Example
    /// ```ignore
    /// let client = Client::connect("redis://localhost:6379", "myapp").await?;
    /// ```
    pub async fn connect(url: &str, prefix: impl Into<String>) -> Result<Self, redis::RedisError> {
        let redis_client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(redis_client).await?;
        Ok(Self::new(conn, prefix.into()))
    }

    /// Get a type-safe handle for the specified entity collection.
    ///
    /// This is the generic way to access any registered entity type.
    /// For named accessors, use the `#[derive(SnugomClient)]` macro.
    ///
    /// # Example
    /// ```ignore
    /// let mut guilds = client.collection::<Guild>();
    /// let guild = guilds.get(&id).await?;
    /// ```
    pub fn collection<T: SnugomModel>(&self) -> CollectionHandle<T> {
        let repo = Repo::new(self.prefix.clone());
        CollectionHandle::new(repo, self.conn.clone())
    }

    /// Get the key prefix used by this client.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Get a clone of the connection manager for advanced operations.
    pub fn connection(&self) -> ConnectionManager {
        self.conn.clone()
    }

    /// Get a mutable reference to the connection manager.
    pub fn connection_mut(&mut self) -> &mut ConnectionManager {
        &mut self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_type_exists() {
        // Verify Client type exists and has expected structure
        let _ = std::mem::size_of::<Client>();
    }
}
