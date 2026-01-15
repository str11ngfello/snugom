//! Entity scanning module for parsing SnugomEntity structs from Rust source files.
//!
//! This module provides functionality to:
//! - Discover Rust files containing SnugomEntity derives
//! - Parse struct definitions and extract schema information
//! - Generate schema snapshots for migration diffing

mod discovery;
mod parser;
mod schema;

pub use discovery::discover_entities;
pub use parser::parse_entity_file;

// Re-export schema types for use by other modules
#[allow(unused_imports)]
pub use schema::{
    CascadeStrategy, EntitySchema, FieldInfo, FilterableType, IndexInfo, IndexType,
    RelationInfo, RelationKind, UniqueConstraint,
};
