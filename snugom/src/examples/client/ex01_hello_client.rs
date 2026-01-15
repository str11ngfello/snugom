//! Example 01 – Hello Client
//!
//! This is the simplest introduction to the SnugomClient API.
//! It demonstrates basic CRUD operations using the Prisma-style macro DSL.

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};

use super::support;
use crate::{SnugomClient, SnugomEntity, snugom_create, snugom_delete, snugom_update};

/// A simple task entity.
#[derive(SnugomEntity, Serialize, Deserialize, Debug, Clone)]
#[snugom(schema = 1, service = "examples", collection = "tasks")]
struct Task {
    #[snugom(id)]
    id: String,
    #[snugom(created_at)]
    created_at: chrono::DateTime<Utc>,
    #[snugom(filterable(text))]
    title: String,
    done: bool,
}

/// A typed client with named accessors.
#[derive(SnugomClient)]
#[snugom_client(entities = [Task])]
struct TaskClient {
    conn: ConnectionManager,
    prefix: String,
}

/// Example 01 – basic CRUD with SnugomClient macro DSL.
pub async fn run() -> Result<()> {
    // Connect to Redis and create a client
    let conn = support::redis_connection().await?;
    let prefix = support::unique_namespace("hello_client");
    let mut client = TaskClient::new(conn, prefix);

    // Access the tasks collection with a typed accessor
    let mut tasks = client.tasks();

    // Verify namespace is empty
    let initial_count = tasks.count().await?;
    assert_eq!(initial_count, 0, "namespace should be empty");

    // ============ CREATE ============
    // Create a task using the snugom_create! macro
    let created = snugom_create!(
        client,
        Task {
            title: "Learn SnugomClient".to_string(),
            done: false,
            created_at: Utc::now(),
        }
    )
    .await?;

    let task_id = created.id.clone();

    // Verify it was created
    assert!(tasks.exists(&task_id).await?, "task should exist");
    assert_eq!(tasks.count().await?, 1, "should have 1 task");

    // ============ READ ============
    // Fetch the task
    let task = tasks.get(&task_id).await?;
    assert!(task.is_some(), "task should be fetchable");
    assert_eq!(task.unwrap().title, "Learn SnugomClient");

    // ============ UPDATE ============
    // Update the task (mark as done) using snugom_update! macro
    snugom_update!(client, Task(entity_id = &task_id) {
        done: true,
    })
    .await?;

    // Verify update
    let updated = tasks.get_or_error(&task_id).await?;
    assert!(updated.done, "task should be marked done");

    // ============ DELETE ============
    // Delete the task using snugom_delete! macro
    snugom_delete!(client, Task(&task_id)).await?;

    assert!(!tasks.exists(&task_id).await?, "task should be deleted");
    assert_eq!(tasks.count().await?, 0, "count should be 0");

    Ok(())
}
