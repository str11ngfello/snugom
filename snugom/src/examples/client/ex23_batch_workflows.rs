//! Example 23 – Batch Workflows
//!
//! Demonstrates batch operations for efficient bulk processing:
//! - create_many for bulk inserts
//! - update_many_by_ids for targeted updates
//! - update_many for query-based updates
//! - delete_many_by_ids for targeted deletes
//! - delete_many for query-based deletes

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity};

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "batch_products")]
struct Product {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    name: String,
    #[snugom(filterable(tag))]
    category: String,
    #[snugom(filterable, sortable)]
    price: i64,
    #[snugom(filterable(tag))]
    status: String,
}

#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "batch_events")]
struct Event {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    event_type: String,
    #[snugom(filterable(tag))]
    processed: String,
    payload: String,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Product, Event])]
struct BatchClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("batch_workflows");
    let mut client = BatchClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut products = client.products();
    let mut events = client.events();

    // ============ Create Many ============
    {
        // Prepare a batch of products
        let product_builders = vec![
            Product::validation_builder()
                .name("Widget A".to_string())
                .category("widgets".to_string())
                .price(999)
                .status("active".to_string())
                .created_at(Utc::now()),
            Product::validation_builder()
                .name("Widget B".to_string())
                .category("widgets".to_string())
                .price(1499)
                .status("active".to_string())
                .created_at(Utc::now()),
            Product::validation_builder()
                .name("Widget C".to_string())
                .category("widgets".to_string())
                .price(1999)
                .status("draft".to_string())
                .created_at(Utc::now()),
            Product::validation_builder()
                .name("Gadget X".to_string())
                .category("gadgets".to_string())
                .price(2999)
                .status("active".to_string())
                .created_at(Utc::now()),
            Product::validation_builder()
                .name("Gadget Y".to_string())
                .category("gadgets".to_string())
                .price(3999)
                .status("discontinued".to_string())
                .created_at(Utc::now()),
        ];

        // Bulk insert all products at once
        let created_ids = products.create_many(product_builders).await?;
        assert_eq!(created_ids.ids.len(), 5);

        // Verify all were created
        assert_eq!(products.count().await?, 5);
    }

    // ============ Update Many by IDs ============
    {
        // Get widget IDs for targeted update
        let widgets = products
            .find_many(crate::SearchQuery {
                filter: vec!["category:eq:widgets".to_string()],
                ..Default::default()
            })
            .await?;

        let widget_ids: Vec<String> = widgets.items.iter().map(|p| p.id.clone()).collect();
        assert_eq!(widget_ids.len(), 3);

        // Apply 10% discount to all widgets
        let widget_id_refs: Vec<&str> = widget_ids.iter().map(|s| s.as_str()).collect();
        let update_count = products
            .update_many_by_ids(&widget_id_refs, |id| {
                Product::patch_builder()
                    .entity_id(id)
                    .status("on_sale".to_string())
            })
            .await?;

        assert_eq!(update_count, 3);

        // Verify updates applied
        let on_sale = products
            .find_many(crate::SearchQuery {
                filter: vec!["status:eq:on_sale".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(on_sale.items.len(), 3);
    }

    // ============ Update Many by Query ============
    {
        // Update all discontinued products
        let update_count = products
            .update_many(
                crate::SearchQuery {
                    filter: vec!["status:eq:discontinued".to_string()],
                    ..Default::default()
                },
                |id| {
                    Product::patch_builder()
                        .entity_id(id)
                        .status("archived".to_string())
                },
            )
            .await?;

        assert_eq!(update_count, 1);

        // Verify update
        let archived = products
            .find_many(crate::SearchQuery {
                filter: vec!["status:eq:archived".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(archived.items.len(), 1);
        assert_eq!(archived.items[0].name, "Gadget Y");
    }

    // ============ Delete Many by IDs ============
    {
        // Get archived product IDs
        let archived = products
            .find_many(crate::SearchQuery {
                filter: vec!["status:eq:archived".to_string()],
                ..Default::default()
            })
            .await?;

        let archived_ids: Vec<String> = archived.items.iter().map(|p| p.id.clone()).collect();

        // Delete archived products
        let archived_id_refs: Vec<&str> = archived_ids.iter().map(|s| s.as_str()).collect();
        let delete_count = products.delete_many_by_ids(&archived_id_refs).await?;
        assert_eq!(delete_count, 1);

        // Verify deletion
        assert_eq!(products.count().await?, 4);
    }

    // ============ Delete Many by Query ============
    {
        // Mark all on_sale items for removal and delete them
        let delete_count = products
            .delete_many(crate::SearchQuery {
                filter: vec!["status:eq:on_sale".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(delete_count, 3);
        assert_eq!(products.count().await?, 1); // Only Gadget X remains
    }

    // ============ Event Processing Workflow ============
    {
        // Create a batch of events to process
        let event_builders: Vec<_> = (0..10)
            .map(|i| {
                Event::validation_builder()
                    .event_type("user.action".to_string())
                    .processed("false".to_string())
                    .payload(format!("{{\"action\": \"click\", \"index\": {i}}}"))
                    .created_at(Utc::now())
            })
            .collect();

        events.create_many(event_builders).await?;
        assert_eq!(events.count().await?, 10);

        // Simulate batch processing: find unprocessed events
        let unprocessed = events
            .find_many(crate::SearchQuery {
                filter: vec!["processed:eq:false".to_string()],
                page_size: Some(5), // Process in batches of 5
                ..Default::default()
            })
            .await?;

        assert_eq!(unprocessed.items.len(), 5);

        // Process this batch
        let batch_ids: Vec<String> = unprocessed.items.iter().map(|e| e.id.clone()).collect();

        // Mark as processed
        let batch_id_refs: Vec<&str> = batch_ids.iter().map(|s| s.as_str()).collect();
        let updated = events
            .update_many_by_ids(&batch_id_refs, |id| {
                Event::patch_builder()
                    .entity_id(id)
                    .processed("true".to_string())
            })
            .await?;

        assert_eq!(updated, 5);

        // Process next batch
        let remaining = events
            .find_many(crate::SearchQuery {
                filter: vec!["processed:eq:false".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(remaining.items.len(), 5);

        // Mark remaining as processed
        events
            .update_many(
                crate::SearchQuery {
                    filter: vec!["processed:eq:false".to_string()],
                    ..Default::default()
                },
                |id| {
                    Event::patch_builder()
                        .entity_id(id)
                        .processed("true".to_string())
                },
            )
            .await?;

        // Verify all processed
        let still_unprocessed = events
            .count_where(crate::SearchQuery {
                filter: vec!["processed:eq:false".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(still_unprocessed, 0);

        // Clean up old events
        let deleted = events
            .delete_many(crate::SearchQuery {
                filter: vec!["processed:eq:true".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(deleted, 10);
        assert_eq!(events.count().await?, 0);
    }

    Ok(())
}
