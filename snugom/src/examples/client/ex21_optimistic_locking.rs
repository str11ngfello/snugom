//! Example 21 – Optimistic Locking
//!
//! Demonstrates version-based optimistic locking:
//! - Version field auto-increment
//! - Conditional updates with version checks
//! - Conditional deletes with version checks
//! - Handling version conflicts

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, errors::RepoError};

/// An inventory item with stock tracking.
/// Uses version field for optimistic locking to prevent race conditions.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "inventory_items")]
struct InventoryItem {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,

    /// Version field for optimistic locking.
    /// Automatically incremented on each update.
    /// Uses serde(default) so validation doesn't require it - create sets to 1.
    #[serde(default)]
    #[snugom(version)]
    version: i64,

    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable, sortable)]
    quantity: i64,
    #[snugom(filterable, sortable)]
    price: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [InventoryItem])]
struct InventoryClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("optimistic_locking");
    let client = InventoryClient::new(conn, prefix);
    let mut items = client.inventory_items();

    // ============ Version Auto-Increment ============
    {
        // Create item - version starts at 1
        let item_id = snugom_create!(client, InventoryItem {
            name: "Widget".to_string(),
            quantity: 100,
            price: 999,
            created_at: Utc::now(),
        }).await?.id;

        let item = items.get_or_error(&item_id).await?;
        assert_eq!(item.version, 1, "new entity should have version 1");

        // Update item - version increments to 2
        items
            .update(
                InventoryItem::patch_builder()
                    .entity_id(&item.id)
                    .quantity(95),
            )
            .await?;

        let updated = items.get_or_error(&item.id).await?;
        assert_eq!(updated.version, 2, "version should increment on update");
        assert_eq!(updated.quantity, 95);

        // Another update - version increments to 3
        items
            .update(
                InventoryItem::patch_builder()
                    .entity_id(&item.id)
                    .quantity(90),
            )
            .await?;

        let updated = items.get_or_error(&item.id).await?;
        assert_eq!(updated.version, 3);
    }

    // ============ Conditional Update with Version ============
    {
        let item_id = snugom_create!(client, InventoryItem {
            name: "Gadget".to_string(),
            quantity: 50,
            price: 1999,
            created_at: Utc::now(),
        }).await?.id;

        let item = items.get_or_error(&item_id).await?;

        // Update with correct version - succeeds
        items
            .update(
                InventoryItem::patch_builder()
                    .entity_id(&item.id)
                    .version(item.version) // Current version: 1
                    .quantity(45),
            )
            .await?;

        // Try update with stale version - fails
        let result = items
            .update(
                InventoryItem::patch_builder()
                    .entity_id(&item.id)
                    .version(item.version) // Still 1, but entity is now version 2
                    .quantity(40),
            )
            .await;

        match result {
            Err(RepoError::VersionConflict { expected, actual, .. }) => {
                assert_eq!(expected, Some(1));
                assert_eq!(actual, Some(2));
            }
            _ => panic!("expected VersionConflict error"),
        }

        // Verify quantity didn't change
        let current = items.get_or_error(&item.id).await?;
        assert_eq!(current.quantity, 45, "quantity should not have changed");
    }

    // ============ Read-Modify-Write Pattern ============
    {
        let item_id = snugom_create!(client, InventoryItem {
            name: "Thingamajig".to_string(),
            quantity: 200,
            price: 499,
            created_at: Utc::now(),
        }).await?.id;

        let item = items.get_or_error(&item_id).await?;

        // Simulate concurrent access: read-modify-write with retry
        let order_quantity = 5;
        let mut retries = 3;
        let mut current = item;

        loop {
            // Calculate new quantity
            let new_quantity = current.quantity - order_quantity;

            // Try to update with version check
            let result = items
                .update(
                    InventoryItem::patch_builder()
                        .entity_id(&current.id)
                        .version(current.version)
                        .quantity(new_quantity),
                )
                .await;

            match result {
                Ok(_) => {
                    // Success - update applied
                    break;
                }
                Err(RepoError::VersionConflict { .. }) => {
                    // Conflict - refetch and retry
                    retries -= 1;
                    if retries == 0 {
                        return Err(anyhow::anyhow!("failed after max retries"));
                    }
                    current = items.get_or_error(&current.id).await?;
                    // Continue loop with fresh data
                }
                Err(e) => return Err(e.into()),
            }
        }

        let final_item = items.get_or_error(&current.id).await?;
        assert_eq!(final_item.quantity, 195);
    }

    // ============ Conditional Delete ============
    {
        let item_id = snugom_create!(client, InventoryItem {
            name: "Temporary Item".to_string(),
            quantity: 10,
            price: 100,
            created_at: Utc::now(),
        }).await?.id;

        let item = items.get_or_error(&item_id).await?;

        // Update the item (version becomes 2)
        items
            .update(
                InventoryItem::patch_builder()
                    .entity_id(&item.id)
                    .quantity(0),
            )
            .await?;

        // Try to delete with old version - fails
        let result = items.delete_with_version(&item.id, item.version as u64).await;

        match result {
            Err(RepoError::VersionConflict { .. }) => {
                // Expected - version has changed
            }
            _ => panic!("expected VersionConflict error"),
        }

        // Item still exists
        assert!(items.exists(&item.id).await?);

        // Delete with correct version
        let current = items.get_or_error(&item.id).await?;
        items
            .delete_with_version(&item.id, current.version as u64)
            .await?;

        // Now it's gone
        assert!(!items.exists(&item.id).await?);
    }

    // ============ Update Without Version (Unconditional) ============
    {
        let item_id = snugom_create!(client, InventoryItem {
            name: "Unconditional Item".to_string(),
            quantity: 100,
            price: 500,
            created_at: Utc::now(),
        }).await?.id;

        let item = items.get_or_error(&item_id).await?;

        // Update without specifying version - always succeeds
        items
            .update(InventoryItem::patch_builder().entity_id(&item.id).quantity(80))
            .await?;

        items
            .update(InventoryItem::patch_builder().entity_id(&item.id).quantity(60))
            .await?;

        items
            .update(InventoryItem::patch_builder().entity_id(&item.id).quantity(40))
            .await?;

        let final_item = items.get_or_error(&item.id).await?;
        assert_eq!(final_item.quantity, 40);
        assert_eq!(final_item.version, 4); // Version still tracked
    }

    Ok(())
}
