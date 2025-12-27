//! Compile-fail test: validate(uuid) on numeric field.
//! UUID validation only works on string fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: uuid validator not supported for numeric
    #[snugom(validate(uuid))]
    pub external_id: u128,
}

fn main() {}
