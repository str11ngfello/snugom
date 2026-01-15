//! Compile-fail test: validate(enum(...)) on numeric field.
//! Enum validation only works on string fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(schema = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: enum validator not supported for numeric
    #[snugom(validate(enum(allowed = ["1", "2", "3"])))]
    pub status: u32,
}

fn main() {}
