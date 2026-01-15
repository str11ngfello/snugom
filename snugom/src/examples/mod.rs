//! Snugom Examples
//!
//! This module contains comprehensive examples demonstrating Snugom usage.
//!
//! ## Repository Structure
//!
//! ### Low-Level Repo API (`repo/`)
//! Examples demonstrating direct usage of the `Repo` struct for low-level operations.
//! Useful for understanding internals or when you need fine-grained control.
//!
//! ### High-Level SnugomClient API (`client/`)
//! Examples demonstrating the Prisma-style ergonomic API using `#[derive(SnugomClient)]`.
//! This is the recommended approach for most applications.
//!
//! ## Quick Start
//!
//! For most users, start with the client examples:
//!
//! ```rust,ignore
//! use snugom::examples::client;
//!
//! // Run all client examples
//! client::run_all().await?;
//!
//! // Or run the social network demo
//! client::social_network::tour::run().await?;
//! ```
//!
//! ## Example Categories
//!
//! ### Client Examples (23 focused examples + social network app)
//! - **CRUD (01-06)**: Create, Read, Update, Delete, Upsert operations
//! - **Search (07-10)**: Filtering, pagination, sorting
//! - **Schema (11-15)**: Field attributes, timestamps, validation, unique constraints
//! - **Relations (16-18)**: Defining and mutating relationships
//! - **Advanced (19-23)**: Multi-entity clients, error handling, optimistic locking
//! - **Social Network**: Complete application example with users, posts, follows, feeds
//!
//! ### Repo Examples (13 examples)
//! Low-level examples for those who need direct Repo access.

pub mod support;
pub mod repo;
pub mod client;

/// Run all examples (both repo and client).
pub async fn run_all() -> anyhow::Result<()> {
    println!("═══════════════════════════════════════════════════════════");
    println!("                    SNUGOM EXAMPLES");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("▶ Running Repo (low-level) examples...\n");
    repo::run_all().await?;

    println!("\n▶ Running Client (high-level) examples...\n");
    client::run_all().await?;

    println!("\n═══════════════════════════════════════════════════════════");
    println!("                 ALL EXAMPLES COMPLETED");
    println!("═══════════════════════════════════════════════════════════\n");

    Ok(())
}
