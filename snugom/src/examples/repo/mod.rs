//! Low-level Repo API examples.
//!
//! These examples demonstrate direct usage of `Repo<T>` for fine-grained control
//! over Redis operations. For most applications, prefer the higher-level
//! `SnugomClient` API in the `client` module.

pub mod support;

pub mod ex01_hello_entity;
pub mod ex02_belongs_to;
pub mod ex03_has_many;
pub mod ex04_many_to_many;
pub mod ex05_timestamps;
pub mod ex06_validation_rules;
pub mod ex07_patch_updates;
pub mod ex08_search_filters;
pub mod ex09_cascade_strategies;
pub mod ex10_idempotency;
pub mod ex11_relation_mutations;
pub mod ex12_search_manager;
pub mod ex13_unique_constraints;

use anyhow::Result;

/// Run all repo examples in sequence.
pub async fn run_all() -> Result<()> {
    println!("=== Repo Examples ===\n");

    println!("01. Hello Entity (basic CRUD)...");
    ex01_hello_entity::run().await?;
    println!("    ✓ passed\n");

    println!("02. Belongs To (relations)...");
    ex02_belongs_to::run().await?;
    println!("    ✓ passed\n");

    println!("03. Has Many (one-to-many)...");
    ex03_has_many::run().await?;
    println!("    ✓ passed\n");

    println!("04. Many to Many...");
    ex04_many_to_many::run().await?;
    println!("    ✓ passed\n");

    println!("05. Timestamps (auto created_at/updated_at)...");
    ex05_timestamps::run().await?;
    println!("    ✓ passed\n");

    println!("06. Validation Rules...");
    ex06_validation_rules::run().await?;
    println!("    ✓ passed\n");

    println!("07. Patch Updates...");
    ex07_patch_updates::run().await?;
    println!("    ✓ passed\n");

    println!("08. Search Filters...");
    ex08_search_filters::run().await?;
    println!("    ✓ passed\n");

    println!("09. Cascade Strategies...");
    ex09_cascade_strategies::run().await?;
    println!("    ✓ passed\n");

    println!("10. Idempotency & Versions...");
    ex10_idempotency::run().await?;
    println!("    ✓ passed\n");

    println!("11. Relation Mutations...");
    ex11_relation_mutations::run().await?;
    println!("    ✓ passed\n");

    println!("12. Search Manager...");
    ex12_search_manager::run().await?;
    println!("    ✓ passed\n");

    println!("13. Unique Constraints...");
    ex13_unique_constraints::run().await?;
    println!("    ✓ passed\n");

    println!("=== All Repo Examples Passed ===");
    Ok(())
}
