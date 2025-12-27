//! Compile-fail test: validate(length(...)) on numeric field.
//! Length validation only works on strings and collections.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: length validator not supported for numeric
    #[snugom(validate(length(min = 1, max = 10)))]
    pub count: u32,
}

fn main() {}
