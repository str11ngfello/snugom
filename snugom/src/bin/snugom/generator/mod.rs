//! Migration code generator.
//!
//! This module provides functionality to:
//! - Generate Rust migration files from schema diffs
//! - Update source files with new schema versions
//! - Update migrations/mod.rs with new migration registrations

mod codegen;
mod source_updater;

#[allow(unused_imports)]
pub use codegen::{generate_migration_file, MigrationFile};
pub use source_updater::{update_migrations_mod, update_source_schema_version};
