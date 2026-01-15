//! Migration execution engine.
//!
//! This module provides runtime components for executing migrations:
//! - `MigrationContext` - Redis connection and document access
//! - `MigrationRunner` - Executes pending migrations
//! - `MigrationState` - Tracks applied migrations in Redis

mod context;
mod runner;
pub mod state;

pub use context::MigrationContext;
#[allow(unused_imports)]
pub use runner::{MigrationRunner, MigrationStats};
#[allow(unused_imports)]
pub use state::{AppliedMigration, MigrationState};
