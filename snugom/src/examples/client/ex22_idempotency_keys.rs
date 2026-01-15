//! Example 22 – Idempotency Keys
//!
//! Demonstrates using idempotency keys for safe retries:
//! - Preventing duplicate entity creation
//! - Safe retry behavior
//! - Idempotency key patterns

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity};

/// A payment transaction.
/// Uses idempotency keys to ensure payments are not duplicated.
/// Note: Idempotency key is passed to the builder during creation, not stored on the entity.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "payments")]
struct Payment {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    customer_id: String,
    #[snugom(filterable, sortable)]
    amount: i64,
    #[snugom(filterable(tag))]
    currency: String,
    #[snugom(filterable(tag))]
    status: String,
}

/// An order that can be retried.
/// Note: Idempotency key is passed to the builder during creation, not stored on the entity.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "idem_orders")]
struct Order {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(tag))]
    customer_id: String,
    #[snugom(filterable, sortable)]
    total: i64,
}

#[derive(SnugomClient)]
#[snugom_client(entities = [Payment, Order])]
struct PaymentClient {
    conn: ConnectionManager,
    prefix: String,
}

pub async fn run() -> Result<()> {
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("idempotency");
    let mut client = PaymentClient::new(conn, prefix);
    client.ensure_indexes().await?; // Required for search queries
    let mut payments = client.payments();
    let mut orders = client.orders();

    // ============ Basic Idempotency ============
    {
        let idempotency_key = "payment-abc123-attempt1".to_string();

        // First creation - succeeds
        let payment1_id = payments
            .create(
                Payment::validation_builder()
                    .idempotency_key(idempotency_key.clone())
                    .customer_id("cust_001".to_string())
                    .amount(9999)
                    .currency("USD".to_string())
                    .status("pending".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Second creation with same key - returns same ID
        let payment2_id = payments
            .create(
                Payment::validation_builder()
                    .idempotency_key(idempotency_key.clone())
                    .customer_id("cust_001".to_string())
                    .amount(9999)
                    .currency("USD".to_string())
                    .status("pending".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        assert_eq!(
            payment1_id.id, payment2_id.id,
            "same idempotency key should return same ID"
        );

        // Verify only one payment exists
        let all_payments = payments.find_many(crate::SearchQuery::default()).await?;
        assert_eq!(all_payments.items.len(), 1);
    }

    // ============ Different Keys Create Different Entities ============
    {
        let payment_a = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key("key-a".to_string())
                    .customer_id("cust_002".to_string())
                    .amount(1000)
                    .currency("EUR".to_string())
                    .status("pending".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        let payment_b = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key("key-b".to_string())
                    .customer_id("cust_002".to_string())
                    .amount(1000)
                    .currency("EUR".to_string())
                    .status("pending".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        assert_ne!(
            payment_a.id, payment_b.id,
            "different idempotency keys should create different entities"
        );
    }

    // ============ Retry Pattern ============
    {
        // Simulate a client that retries on failure
        let idem_key = "order-xyz789".to_string();

        async fn place_order(
            orders: &mut crate::CollectionHandle<Order>,
            idem_key: String,
        ) -> Result<Order> {
            orders
                .create_and_get(
                    Order::validation_builder()
                        .idempotency_key(idem_key)
                        .customer_id("cust_retry".to_string())
                        .total(5000)
                        .created_at(Utc::now()),
                )
                .await
                .map_err(|e| e.into())
        }

        // First attempt
        let order1 = place_order(&mut orders, idem_key.clone()).await?;

        // Simulate retry (maybe first response was lost)
        let order2 = place_order(&mut orders, idem_key.clone()).await?;

        // Third retry for good measure
        let order3 = place_order(&mut orders, idem_key.clone()).await?;

        // All return the same order
        assert_eq!(order1.id, order2.id);
        assert_eq!(order2.id, order3.id);

        // Only one order was created
        let customer_orders = orders
            .find_many(crate::SearchQuery {
                filter: vec!["customer_id:eq:cust_retry".to_string()],
                ..Default::default()
            })
            .await?;

        assert_eq!(customer_orders.items.len(), 1);
    }

    // ============ Real-World Pattern: Request ID ============
    {
        // Common pattern: use client request ID as idempotency key
        let client_request_id = format!("req-{}", uuid::Uuid::new_v4());

        let payment = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key(client_request_id.clone())
                    .customer_id("cust_api".to_string())
                    .amount(25000)
                    .currency("USD".to_string())
                    .status("completed".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Client retries with same request ID (network timeout, etc.)
        let retry = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key(client_request_id)
                    .customer_id("cust_api".to_string())
                    .amount(25000)
                    .currency("USD".to_string())
                    .status("completed".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        assert_eq!(payment.id, retry.id);
    }

    // ============ Idempotency with Composite Keys ============
    {
        // Pattern: Combine multiple identifiers for the idempotency key
        let customer = "cust_monthly";
        let month = "2024-01";
        let service = "subscription";

        // Composite key ensures one charge per customer per month per service
        let composite_key = format!("{customer}:{service}:{month}");

        payments
            .create(
                Payment::validation_builder()
                    .idempotency_key(composite_key.clone())
                    .customer_id(customer.to_string())
                    .amount(4999)
                    .currency("USD".to_string())
                    .status("completed".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Attempting same charge again (same composite key) returns existing
        let duplicate_attempt = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key(composite_key)
                    .customer_id(customer.to_string())
                    .amount(4999)
                    .currency("USD".to_string())
                    .status("completed".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        // Different month would be a new charge
        let next_month_key = format!("{customer}:{service}:2024-02");
        let next_month = payments
            .create_and_get(
                Payment::validation_builder()
                    .idempotency_key(next_month_key)
                    .customer_id(customer.to_string())
                    .amount(4999)
                    .currency("USD".to_string())
                    .status("completed".to_string())
                    .created_at(Utc::now()),
            )
            .await?;

        assert_ne!(duplicate_attempt.id, next_month.id);
    }

    Ok(())
}
