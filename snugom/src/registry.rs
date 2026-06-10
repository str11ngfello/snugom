use crate::types::EntityDescriptor;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

static REGISTRY: OnceLock<RwLock<HashMap<String, EntityDescriptor>>> = OnceLock::new();

fn registry() -> &'static RwLock<HashMap<String, EntityDescriptor>> {
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_descriptor(descriptor: &EntityDescriptor) {
    registry()
        .write()
        .unwrap()
        .insert(descriptor.collection.clone(), descriptor.clone());
}

pub fn get_descriptor(collection: &str) -> Option<EntityDescriptor> {
    registry().read().unwrap().get(collection).cloned()
}

/// Information about a relation pointing TO an entity from another entity
#[derive(Debug, Clone)]
pub struct IncomingRelation {
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
pub fn find_incoming_relations(target_collection: &str) -> Vec<IncomingRelation> {
    let mut incoming = Vec::new();
    let reg = registry().read().unwrap();

    for (collection, descriptor) in reg.iter() {
        for relation in &descriptor.relations {
            if relation.target == target_collection {
                incoming.push(IncomingRelation {
                    source_collection: collection.clone(),
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
