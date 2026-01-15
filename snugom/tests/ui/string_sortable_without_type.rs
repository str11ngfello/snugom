//! Entry 66: `sortable` on String without explicit type should fail.
//! String fields need `searchable` (TEXT) or `filterable(tag)` (TAG) to determine index type.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: String with sortable needs explicit type
    #[snugom(sortable)]
    pub name: String,
}

fn main() {}
