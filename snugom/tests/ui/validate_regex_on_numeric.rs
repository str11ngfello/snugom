//! Compile-fail test: validate(regex = "...") on numeric field.
//! Regex validation only works on string fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: regex validator not supported for numeric
    #[snugom(validate(regex = "^[0-9]+$"))]
    pub code: i64,
}

fn main() {}
