//! Compile-fail test: Entity in bundle has no indexed fields (filterable or sortable).

use serde::{Deserialize, Serialize};
use snugom::{SnugomEntity, bundle};

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct EntityWithoutIndexedFields {
    #[snugom(id)]
    pub id: String,
    // No filterable or sortable fields - this should fail
    pub name: String,
}

bundle! {
    service: "test",
    entities: {
        EntityWithoutIndexedFields => "no_index_entities",
    }
}

fn main() {}
