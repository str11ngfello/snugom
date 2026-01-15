//! Entity auto-registration via inventory crate.
//!
//! This module provides compile-time entity discovery. When an entity is derived with
//! `#[derive(SnugomEntity)]`, it automatically registers itself with the inventory,
//! allowing the SnugomClient derive macro to discover all entities at compile time.

use std::any::TypeId;

/// Metadata for auto-discovered entities.
///
/// This struct is submitted to the inventory by the `SnugomEntity` derive macro,
/// enabling the `SnugomClient` derive to discover all registered entities.
pub struct EntityRegistration {
    /// The TypeId of the entity struct
    pub type_id: TypeId,
    /// The name of the entity type (e.g., "Guild")
    pub type_name: &'static str,
    /// The collection name (e.g., "guilds")
    pub collection_name: &'static str,
    /// The service name (e.g., "guild")
    pub service_name: &'static str,
    /// Function to get the entity descriptor
    pub descriptor_fn: fn() -> crate::types::EntityDescriptor,
}

// Collect all EntityRegistration instances via inventory
inventory::collect!(EntityRegistration);

/// Get all registered entities.
///
/// This iterates over all entities that have been registered via the
/// `#[derive(SnugomEntity)]` macro with the `#[snugom(collection = "...", service = "...")]`
/// attribute.
pub fn registered_entities() -> impl Iterator<Item = &'static EntityRegistration> {
    inventory::iter::<EntityRegistration>()
}

/// Get a registered entity by type name.
pub fn get_entity_by_name(type_name: &str) -> Option<&'static EntityRegistration> {
    registered_entities().find(|e| e.type_name == type_name)
}

/// Get a registered entity by collection name.
pub fn get_entity_by_collection(collection_name: &str) -> Option<&'static EntityRegistration> {
    registered_entities().find(|e| e.collection_name == collection_name)
}

/// Check if an entity type is registered.
pub fn is_entity_registered<T: 'static>() -> bool {
    let type_id = TypeId::of::<T>();
    registered_entities().any(|e| e.type_id == type_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registered_entities_iterator() {
        // This just tests that the iterator works, actual entities
        // would be registered by the derive macro
        let _count = registered_entities().count();
        // Count may be 0 or more depending on what's linked - iterator works if no panic
    }
}
