//! Compile-fail test: Relation target not registered in bundle.

use serde::{Deserialize, Serialize};
use snugom::{SnugomEntity, bundle};

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct ValidEntity {
    #[snugom(id)]
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct EntityWithBadTarget {
    #[snugom(id)]
    pub id: String,
    // Target "nonexistent_collection" is not in the bundle
    #[snugom(relation(target = "nonexistent_collection"))]
    pub parent_id: String,
}

bundle! {
    service: "test",
    entities: {
        ValidEntity => "valid_entities",
        EntityWithBadTarget => "bad_entities",
    }
}

fn main() {}
