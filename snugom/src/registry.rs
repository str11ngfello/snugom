use crate::types::EntityDescriptor;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct DescriptorKey {
    service: String,
    collection: String,
}

static REGISTRY: OnceLock<RwLock<HashMap<DescriptorKey, EntityDescriptor>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<DescriptorKey, EntityDescriptor>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_descriptor(descriptor: &EntityDescriptor) {
    let key = DescriptorKey {
        service: descriptor.service.clone(),
        collection: descriptor.collection.clone(),
    };
    registry().write().unwrap().insert(key, descriptor.clone());
}

pub fn get_descriptor(service: &str, collection: &str) -> Option<EntityDescriptor> {
    let key = DescriptorKey {
        service: service.to_string(),
        collection: collection.to_string(),
    };
    registry().read().unwrap().get(&key).cloned()
}

/// Information about a relation pointing TO an entity from another entity
#[derive(Debug, Clone)]
pub struct IncomingRelation {
    /// The source entity's service
    pub source_service: String,
    /// The source entity's collection
    pub source_collection: String,
    /// The alias of the relation on the source entity
    pub alias: String,
    /// The cascade policy from the source's perspective
    pub cascade: crate::types::CascadePolicy,
    /// The kind of relation
    pub kind: crate::types::RelationKind,
    /// Foreign key field name (for belongs_to relations)
    pub foreign_key: Option<String>,
}

/// Find all relations from other entities that point to the given entity.
/// This is used for cascade operations - when deleting an entity, we need to
/// find all children that have belongs_to relations pointing to it.
pub fn find_incoming_relations(target_service: &str, target_collection: &str) -> Vec<IncomingRelation> {
    let mut incoming = Vec::new();
    let reg = registry().read().unwrap();

    for (key, descriptor) in reg.iter() {
        for relation in &descriptor.relations {
            // Check if this relation points to our target
            let rel_service = relation.target_service.as_deref().unwrap_or(&descriptor.service);
            if rel_service == target_service && relation.target == target_collection {
                incoming.push(IncomingRelation {
                    source_service: key.service.clone(),
                    source_collection: key.collection.clone(),
                    alias: relation.alias.clone(),
                    cascade: relation.cascade,
                    kind: relation.kind,
                    foreign_key: relation.foreign_key.clone(),
                });
            }
        }
    }

    incoming
}
