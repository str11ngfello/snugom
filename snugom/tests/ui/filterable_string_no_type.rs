//! Compile-fail test: #[snugom(filterable)] on String without explicit type.
//! String fields require filterable(tag) or filterable(text).

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: String filterable requires explicit type
    #[snugom(filterable)]
    pub category: String,
}

fn main() {}
