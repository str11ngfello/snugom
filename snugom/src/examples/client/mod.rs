//! SnugomClient API Examples
//!
//! These examples demonstrate the Prisma-style ergonomic API using `#[derive(SnugomClient)]`.
//! All examples use typed collection accessors with no direct Repo or low-level connection management.
//!
//! ## CRUD Operations (01-06)
//! - ex01: Hello Client - Basic client setup and usage
//! - ex02: Create Operations - create, create_and_get, create_many
//! - ex03: Read Operations - get, get_or_error, exists, count
//! - ex04: Update Operations - update, update_and_get, update_many
//! - ex05: Delete Operations - delete, delete_with_version, delete_many
//! - ex06: Upsert Operations - atomic upsert with snugom_upsert! macro
//!
//! ## Search & Filtering (07-10)
//! - ex07: Basic Search - find_many, find_first, count_where, exists_where
//! - ex08: Pagination - page-based pagination
//! - ex09: Advanced Search - filter types, operators, combining filters
//! - ex10: Sorting & Ordering - sort_by, sort_order, with pagination
//!
//! ## Schema & Fields (11-15)
//! - ex11: Field Attributes - all attribute types (filterable, sortable, etc.)
//! - ex12: Timestamps - created_at, updated_at behavior
//! - ex13: Validation - length constraints, error handling
//! - ex14: Unique Constraints - case-sensitive and case-insensitive unique
//! - ex15: Custom IDs - auto-generated vs user-provided IDs
//!
//! ## Relations (16-18)
//! - ex16: Relations - defining belongs_to/has_many relations
//! - ex17: Relation Mutations - connect/disconnect/delete with snugom_update!
//! - ex18: Cascade Strategies - cascade delete behavior
//!
//! ## Find with Includes (19-21)
//! - ex19: Find with Includes - snugom_find! for reading entities with related data
//! - ex20: Find with Options - include options (limit, sort) and nested + options
//! - ex21: Find Many with Includes - snugom_find_many! for list queries with related data
//!
//! ## Advanced Patterns (22-26)
//! - ex22: Multi-Entity Client - working with multiple entity types
//! - ex23: Error Handling - handling various error conditions
//! - ex24: Optimistic Locking - version-based conditional updates
//! - ex25: Idempotency Keys - safe retry patterns
//! - ex26: Batch Workflows - bulk operations for efficiency
//!
//! ## Social Network Application
//! - social_network: Complete multi-file example showing how to structure a real application

pub mod social_network;
pub mod support;

pub mod ex01_hello_client;
pub mod ex02_create_operations;
pub mod ex03_read_operations;
pub mod ex04_update_operations;
pub mod ex05_delete_operations;
pub mod ex06_upsert_operations;
pub mod ex07_search_basic;
pub mod ex08_search_pagination;
pub mod ex09_search_advanced;
pub mod ex10_sorting_ordering;
pub mod ex11_field_attributes;
pub mod ex12_timestamps;
pub mod ex13_validation;
pub mod ex14_unique_constraints;
pub mod ex15_custom_ids;
pub mod ex16_relations;
pub mod ex17_relation_mutations;
pub mod ex18_cascade_strategies;
pub mod ex19_find_with_includes;
pub mod ex20_find_with_options;
pub mod ex21_find_many_with_includes;
pub mod ex22_multi_entity_client;
pub mod ex23_error_handling;
pub mod ex24_optimistic_locking;
pub mod ex25_idempotency_keys;
pub mod ex26_batch_workflows;

use anyhow::Result;

/// Run all client examples in sequence.
pub async fn run_all() -> Result<()> {
    println!("Running SnugomClient examples...\n");

    println!("=== CRUD Operations ===");
    println!("Running ex01_hello_client...");
    ex01_hello_client::run().await?;
    println!("Running ex02_create_operations...");
    ex02_create_operations::run().await?;
    println!("Running ex03_read_operations...");
    ex03_read_operations::run().await?;
    println!("Running ex04_update_operations...");
    ex04_update_operations::run().await?;
    println!("Running ex05_delete_operations...");
    ex05_delete_operations::run().await?;
    println!("Running ex06_upsert_operations...");
    ex06_upsert_operations::run().await?;

    println!("\n=== Search & Filtering ===");
    println!("Running ex07_search_basic...");
    ex07_search_basic::run().await?;
    println!("Running ex08_search_pagination...");
    ex08_search_pagination::run().await?;
    println!("Running ex09_search_advanced...");
    ex09_search_advanced::run().await?;
    println!("Running ex10_sorting_ordering...");
    ex10_sorting_ordering::run().await?;

    println!("\n=== Schema & Fields ===");
    println!("Running ex11_field_attributes...");
    ex11_field_attributes::run().await?;
    println!("Running ex12_timestamps...");
    ex12_timestamps::run().await?;
    println!("Running ex13_validation...");
    ex13_validation::run().await?;
    println!("Running ex14_unique_constraints...");
    ex14_unique_constraints::run().await?;
    println!("Running ex15_custom_ids...");
    ex15_custom_ids::run().await?;

    println!("\n=== Relations ===");
    println!("Running ex16_relations...");
    ex16_relations::run().await?;
    println!("Running ex17_relation_mutations...");
    ex17_relation_mutations::run().await?;
    println!("Running ex18_cascade_strategies...");
    ex18_cascade_strategies::run().await?;

    println!("\n=== Find with Includes ===");
    println!("Running ex19_find_with_includes...");
    ex19_find_with_includes::run().await?;
    println!("Running ex20_find_with_options...");
    ex20_find_with_options::run().await?;
    println!("Running ex21_find_many_with_includes...");
    ex21_find_many_with_includes::run().await?;

    println!("\n=== Advanced Patterns ===");
    println!("Running ex22_multi_entity_client...");
    ex22_multi_entity_client::run().await?;
    println!("Running ex23_error_handling...");
    ex23_error_handling::run().await?;
    println!("Running ex24_optimistic_locking...");
    ex24_optimistic_locking::run().await?;
    println!("Running ex25_idempotency_keys...");
    ex25_idempotency_keys::run().await?;
    println!("Running ex26_batch_workflows...");
    ex26_batch_workflows::run().await?;

    println!("\n=== Social Network Application ===");
    println!("Running social_network tour...");
    social_network::tour::run().await?;

    println!("\nAll client examples completed successfully!");
    Ok(())
}
