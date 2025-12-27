//! SnugOM core library.
//!
//! Phase 0 scaffolding: modules for errors, IDs, key management, shared types, and macro support.

extern crate self as snugom;

/// Compile-time validation that all relation targets exist in the bundle's collections.
///
/// This function is called from the generated `bundle!` macro code to ensure
/// that all relation targets reference valid collections at compile time.
///
/// # Panics
///
/// Panics at compile time if any relation target is not in the valid collections list.
/// The panic message includes the entity name and the invalid target.
pub const fn validate_relation_targets(entity_name: &str, relation_targets: &[&str], valid_collections: &[&str]) {
    let mut i = 0;
    while i < relation_targets.len() {
        let target = relation_targets[i];
        let mut found = false;
        let mut j = 0;
        while j < valid_collections.len() {
            if const_str_eq(target, valid_collections[j]) {
                found = true;
                break;
            }
            j += 1;
        }
        if !found {
            // Use const panic with a formatted message
            const_panic_invalid_target(entity_name, target, valid_collections);
        }
        i += 1;
    }
}

/// Const string equality comparison
const fn const_str_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut i = 0;
    while i < a_bytes.len() {
        if a_bytes[i] != b_bytes[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Const panic with helpful error message about invalid relation target
#[allow(unused_variables)]
const fn const_panic_invalid_target(entity_name: &str, target: &str, valid_collections: &[&str]) -> ! {
    // Build a helpful error message
    // Note: const panic formatting is limited, so we use a simple approach
    // The entity_name, target, and valid_collections are passed for future use
    // when Rust supports better const panic formatting
    panic!(
        "Invalid relation target in entity. The target collection is not registered in this bundle. \
            Check that the relation's target matches a collection name in the bundle! macro, \
            or add an explicit `target = \"collection_name\"` to the relation attribute."
    )
}

/// Compile-time validation that an entity has at least one indexed field.
///
/// Entities used in bundles must have at least one filterable or sortable field
/// to support search operations. This validation is called from the bundle! macro.
///
/// # Panics
///
/// Panics at compile time if the entity has no indexed fields.
#[allow(unused_variables)]
pub const fn validate_entity_has_indexed_fields(entity_name: &str, has_indexed_fields: bool) {
    if !has_indexed_fields {
        panic!(
            "Entity has no indexed fields. All entities in a bundle must have at least one \
             field marked with #[snugom(filterable)] or #[snugom(sortable)]. \
             Consider adding a 'created_at' field with #[snugom(datetime(epoch_millis), filterable, sortable)]."
        );
    }
}

pub mod errors;
pub mod examples;
pub mod filters;
pub mod id;
pub mod keys;
pub mod registry;
pub mod repository;
pub mod runtime;
pub mod search;
pub mod types;
pub mod validators;

pub mod macros;

pub use errors::*;
pub use registry::*;
pub use repository::*;
pub use snugom_macros::{SearchableFilters, SnugomEntity, bundle, run, snug};
pub use types::{
    BundleRegistered, DEFAULT_RELATION_LIMIT, MAX_RELATION_LIMIT, RelationData, RelationQueryOptions, RelationState,
    SortOrder,
};

use redis::aio::ConnectionManager;

/// Delete all keys matching a pattern (for test cleanup).
///
/// This performs a SCAN + DEL operation to safely delete keys without blocking Redis.
pub async fn cleanup_pattern(conn: &mut ConnectionManager, pattern: &str) -> Result<u64, RepoError> {
    const SCAN_COUNT: usize = 1000;
    let mut cursor: u64 = 0;
    let mut total_deleted: u64 = 0;

    loop {
        let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(SCAN_COUNT)
            .query_async(conn)
            .await?;

        if !keys.is_empty() {
            let deleted: u64 = redis::cmd("DEL").arg(&keys).query_async(conn).await?;
            total_deleted += deleted;
        }

        cursor = next_cursor;
        if cursor == 0 {
            break;
        }
    }

    Ok(total_deleted)
}
