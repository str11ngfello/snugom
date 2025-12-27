//! Compile-fail test: validate(range(...)) on string field.
//! Range validation only works on numeric fields.

use serde::{Deserialize, Serialize};
use snugom::SnugomEntity;

#[derive(Debug, Clone, Serialize, Deserialize, SnugomEntity)]
#[snugom(version = 1)]
pub struct InvalidEntity {
    #[snugom(id)]
    pub id: String,

    // ERROR: range validator not supported for string
    #[snugom(validate(range(min = 0, max = 100)))]
    pub name: String,
}

fn main() {}
